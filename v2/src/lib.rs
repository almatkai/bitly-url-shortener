pub mod http;
mod store;
use http::{Api, Failure, Link};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use store::Recovery;
use tezbar_extension_sdk::{
    BoxFuture, CommandConcurrency, CoreResult, Extension, ExtensionContext,
};

pub const ID: &str = "com.tezbar.native.bitly-url-shortener";
const UNKNOWN: &str = "The previous shortening may have created a link. Retry may create a duplicate. Check List Links first; choose Retry Shortening (may create duplicate) only deliberately.";
#[derive(Clone)]
enum Action {
    RetryList,
    RetryShort {
        literal: String,
        key: String,
        resend: bool,
    },
    Copy(String),
    Open(String),
    Settings,
}
#[derive(Clone)]
enum Screen {
    List {
        query: String,
        rows: Vec<Link>,
        failure: Option<Failure>,
    },
    Short {
        failure: Failure,
        retry: Option<Action>,
    },
}
struct Session {
    epoch: u64,
    screen: Screen,
    actions: HashMap<String, Action>,
}
#[derive(Clone)]
struct Cached {
    at: Instant,
    rows: Vec<Link>,
}
#[derive(Default)]
struct Inner {
    sessions: HashMap<String, Session>,
    cache: HashMap<String, Cached>,
    inflight: HashSet<String>,
}
pub struct Adapter {
    api: Api,
    inner: Arc<Mutex<Inner>>,
    counter: AtomicU64,
}
struct Flight {
    inner: Arc<Mutex<Inner>>,
    key: String,
}
impl Drop for Flight {
    fn drop(&mut self) {
        if let Ok(mut i) = self.inner.lock() {
            i.inflight.remove(&self.key);
        }
    }
}
// Synchronous drop runs even when the SDK aborts an opening handler before a session exists.
struct Operation {
    inner: Arc<Mutex<Inner>>,
    sid: String,
    epoch: u64,
    opening: bool,
    complete: bool,
}
impl Drop for Operation {
    fn drop(&mut self) {
        if self.complete {
            return;
        }
        if let Ok(mut i) = self.inner.lock() {
            if i.sessions
                .get(&self.sid)
                .is_some_and(|s| s.epoch == self.epoch)
            {
                if self.opening {
                    i.sessions.remove(&self.sid);
                } else if let Some(s) = i.sessions.get_mut(&self.sid) {
                    s.actions.clear();
                }
            }
        }
    }
}
impl Adapter {
    pub fn production() -> Result<Self, String> {
        Ok(Self::with_api(Api::production()?))
    }
    pub fn with_api(api: Api) -> Self {
        Self {
            api,
            inner: Arc::default(),
            counter: AtomicU64::new(1),
        }
    }
    fn next(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::SeqCst)
    }
    fn begin(&self, sid: &str, opening: bool, screen: Option<Screen>) -> Result<Operation, String> {
        let mut inner = self.inner.lock().map_err(|_| "Session state unavailable")?;
        if opening && inner.sessions.len() >= 128 {
            return Err("Too many Bitly sessions".into());
        }
        let epoch = self.next();
        if let Some(screen) = screen {
            inner.sessions.insert(
                sid.into(),
                Session {
                    epoch,
                    screen,
                    actions: HashMap::new(),
                },
            );
        }
        let session = inner
            .sessions
            .get_mut(sid)
            .ok_or("Bitly session is closed")?;
        session.epoch = epoch;
        session.actions.clear();
        Ok(Operation {
            inner: self.inner.clone(),
            sid: sid.into(),
            epoch,
            opening,
            complete: false,
        })
    }
    fn finish(
        &self,
        op: &mut Operation,
        screen: Screen,
        message: Option<&str>,
        effects: Vec<Value>,
    ) -> CoreResult {
        let mut inner = self.inner.lock().map_err(|_| "Session state unavailable")?;
        let session = inner
            .sessions
            .get_mut(&op.sid)
            .filter(|s| s.epoch == op.epoch)
            .ok_or("Stale Bitly operation")?;
        session.screen = screen;
        let result = self.render(session, message, effects);
        op.complete = true;
        Ok(result)
    }
    fn render(&self, session: &mut Session, message: Option<&str>, effects: Vec<Value>) -> Value {
        session.actions.clear();
        let mut actions = Vec::new();
        let mut add = |title: &str, action: Action, kind: &str| {
            let id = format!("bitly:{}:{}", session.epoch, self.next());
            actions.push(json!({"id":id,"title":title,"kind":kind}));
            session.actions.insert(id.clone(), action);
            id
        };
        let (root, failure) = match &session.screen {
            Screen::List {
                query,
                rows,
                failure,
            } => {
                let mut children = Vec::new();
                if let Some(f) = failure {
                    add("Retry", Action::RetryList, "action");
                    children.push(json!({"type":"List.EmptyView","props":{"title":if f.auth {"Invalid Access Token"} else {"Error"},"description":f.message}}));
                } else if rows.is_empty() {
                    if query.is_empty() {
                        children.push(json!({"type":"List.EmptyView","props":{"icon":"resources/links-list-empty.png","title":"More clicks are just a link away","description":"Shorten long links and get attention by customizing what they say. No more bit.ly/3yqawYa, more bit.ly/brands-bitly."}}));
                        // Group is not embedded in a view or action ID; filled by the handler.
                        add("Create a Bitly Link", Action::Open("create".into()), "open");
                    } else {
                        children.push(json!({"type":"List.EmptyView","props":{"icon":"resources/links-filter-empty-state.png","title":"No results found","description":"Try adjusting your search, filters, or try searching hidden links instead."}}));
                    }
                } else {
                    for row in rows {
                        let copy = add("Copy Bitlink", Action::Copy(row.link.clone()), "copy");
                        let open = add("Open in Bitly", Action::Open(row.link.clone()), "open");
                        children.push(json!({"type":"List.Item","props":{"id":row.id,"title":row.title,"subtitle":row.link,"accessories":[{"date":row.created_at}],"actionIds":[copy,open]}}));
                    }
                }
                (
                    json!({"type":"List","props":{"searchBarPlaceholder":"Search links","searchText":query,"__hasServerSearch":true,"__hasMore":false,"isLoading":false},"children":children}),
                    failure.clone(),
                )
            }
            Screen::Short { failure, retry } => {
                if let Some(retry) = retry {
                    let title = match retry {
                        Action::RetryShort { resend: true, .. } => {
                            "Retry Shortening (may create duplicate)"
                        }
                        _ => "Retry",
                    };
                    add(title, retry.clone(), "action");
                }
                (
                    json!({"type":"List","props":{"searchBarPlaceholder":"","isLoading":false},"children":[{"type":"List.EmptyView","props":{"title":if failure.auth {"Invalid Access Token"} else {"Error"},"description":failure.message}}]}),
                    Some(failure.clone()),
                )
            }
        };
        if failure.as_ref().is_some_and(|f| f.auth) {
            add("Open Command Preferences", Action::Settings, "action");
        }
        let mut result = tezbar_extension_sdk::view("Bitly", root, actions);
        result["refreshMs"] = Value::Null;
        if let Some(message) = message {
            result["message"] = json!(message);
        }
        result["effects"] = json!(effects);
        result
    }
    async fn list(
        &self,
        ctx: &ExtensionContext,
        op: &mut Operation,
        prefs: &Prefs,
        query: String,
        fresh: bool,
    ) -> CoreResult {
        // Clear prior rows before awaiting so canceled/discarded results never authorize actions.
        {
            let mut i = self.inner.lock().map_err(|_| "Session state unavailable")?;
            let s = i.sessions.get_mut(&op.sid).ok_or("Session closed")?;
            s.screen = Screen::List {
                query: query.clone(),
                rows: Vec::new(),
                failure: Some(Failure::new("Search interrupted. Choose Retry.")),
            };
        }
        let key = store::key(&[
            &ctx.paths.data_dir.to_string_lossy(),
            &prefs.token,
            &prefs.group,
            &query,
        ]);
        let cached = {
            let mut i = self.inner.lock().map_err(|_| "Cache state unavailable")?;
            i.cache
                .retain(|_, v| v.at.elapsed() < Duration::from_secs(30));
            if fresh {
                None
            } else {
                i.cache.get(&key).cloned()
            }
        };
        let fetched = match cached {
            Some(c) => Ok(c.rows),
            None => self.api.list(&prefs.token, &prefs.group, &query).await,
        };
        match fetched {
            Ok(rows) => {
                {
                    let mut i = self.inner.lock().map_err(|_| "Cache state unavailable")?;
                    if i.cache.len() >= 8 {
                        if let Some(k) = i
                            .cache
                            .iter()
                            .min_by_key(|(_, v)| v.at)
                            .map(|(k, _)| k.clone())
                        {
                            i.cache.remove(&k);
                        }
                    }
                    i.cache.insert(
                        key,
                        Cached {
                            at: Instant::now(),
                            rows: rows.clone(),
                        },
                    );
                }
                self.finish(
                    op,
                    Screen::List {
                        query,
                        rows,
                        failure: None,
                    },
                    None,
                    vec![],
                )
            }
            Err(failure) => self.finish(
                op,
                Screen::List {
                    query,
                    rows: vec![],
                    failure: Some(failure),
                },
                None,
                vec![],
            ),
        }
    }
    fn failure(
        &self,
        op: &mut Operation,
        message: &str,
        auth: bool,
        retry: Option<Action>,
    ) -> CoreResult {
        self.finish(op, Screen::Short { failure: Failure { message: message.into(), auth }, retry }, None, vec![json!({"kind":"toast","style":"failure","title":if auth {"Invalid Access Token"} else {"Error"},"message":message})])
    }
    async fn short(
        &self,
        ctx: &ExtensionContext,
        op: &mut Operation,
        prefs: &Prefs,
        literal: String,
        key: String,
        resend: bool,
    ) -> CoreResult {
        let retry = |resend| {
            Some(Action::RetryShort {
                literal: literal.clone(),
                key: key.clone(),
                resend,
            })
        };
        let existing = match store::get(ctx, &key) {
            Ok(v) => v,
            Err(e) => return self.failure(op, &e, false, retry(false)),
        };
        let prepared = match existing {
            Some(Recovery::Prepared { link, copied }) => Some((link, copied)),
            Some(Recovery::Uncertain) if !resend => {
                return self.failure(op, UNKNOWN, false, retry(true))
            }
            _ => None,
        };
        let (link, copied) = if let Some(p) = prepared {
            p
        } else {
            {
                let mut i = self.inner.lock().map_err(|_| "Request state unavailable")?;
                if !i.inflight.insert(key.clone()) {
                    drop(i);
                    return self.failure(
                        op,
                        "Shortening is already in progress. Check List Links before retrying.",
                        false,
                        retry(false),
                    );
                }
            }
            let _flight = Flight {
                inner: self.inner.clone(),
                key: key.clone(),
            };
            // A write before POST fences refresh, reopen, cancellation and crash. No implicit POST retry.
            if let Err(e) = store::put(ctx, &key, Recovery::Uncertain) {
                return self.failure(op, &e, false, retry(false));
            }
            match self.api.shorten(&prefs.token, &literal).await {
                Ok(link) => {
                    if let Err(e) = store::put(
                        ctx,
                        &key,
                        Recovery::Prepared {
                            link: link.clone(),
                            copied: false,
                        },
                    ) {
                        return self.failure(op, &e, false, retry(false));
                    }
                    (link, false)
                }
                Err(f) => {
                    return self.failure(
                        op,
                        &format!("{} {UNKNOWN}", f.message),
                        f.auth,
                        retry(true),
                    )
                }
            }
        };
        let mut effects = vec![json!({"kind":"toast","style":"animated","title":"Shortening"})];
        if !copied || !prefs.paste {
            if strict_host(ctx, "host.clipboard.write", json!({"text":link}))
                .await
                .is_err()
            {
                return self.failure(op, "The link was shortened, but copying was not acknowledged. Retry copies the exact saved link without shortening again.", false, retry(false));
            }
            if let Err(e) = store::put(
                ctx,
                &key,
                Recovery::Prepared {
                    link: link.clone(),
                    copied: true,
                },
            ) {
                return self.failure(op, &e, false, retry(false));
            }
        }
        effects.push(json!({"kind":"toast","style":"success","title":"Success","message":"Copied shortened URL to clipboard"}));
        if prefs.paste
            && strict_host(ctx, "host.clipboard.pasteText", json!({"text":link}))
                .await
                .is_err()
        {
            return self.failure(op, "The shortened URL was copied, but paste was not acknowledged. Retry pastes the exact saved link without shortening again; a prior paste may already have happened.", false, retry(false));
        }
        op.complete = true;
        // Completed links remain in the bounded recovery store. Reopen safely reuses the same response.
        if let Ok(mut i) = self.inner.lock() {
            i.sessions.remove(&op.sid);
        }
        let mut result = tezbar_extension_sdk::no_view("Copied shortened URL to clipboard");
        result["effects"] = json!(effects);
        Ok(result)
    }
    async fn selected(&self, ctx: &ExtensionContext) -> Result<String, String> {
        let selected = ctx
            .host("host.selection.read", json!({}))
            .await
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned));
        let text = match selected.filter(|s| !http::js_trim(s).is_empty()) {
            Some(s) => s,
            None => ctx
                .host("host.clipboard.read", json!({}))
                .await
                .map_err(|_| "Could not read the clipboard")?
                .as_str()
                .ok_or("Clipboard returned an invalid response")?
                .into(),
        };
        http::input_url(&text)
    }
}
struct Prefs {
    token: String,
    group: String,
    paste: bool,
}
impl Prefs {
    fn parse(payload: &Value) -> Result<Self, String> {
        let p = &payload["preferences"];
        let token = p["accessToken"].as_str().unwrap_or("").to_owned();
        // Validate locally before storage/host actions; never echo credential values.
        if token.is_empty()
            || token.len() > 4096
            || !token.bytes().all(|b| (0x21..=0x7e).contains(&b))
        {
            return Err(
                "Check that your Bitly access token is correct and hasn't expired or been revoked."
                    .into(),
            );
        }
        let paste = p
            .get("pasteAfterShortening")
            .map(|v| {
                v.as_bool()
                    .ok_or("Paste preference must be a checkbox value")
            })
            .transpose()?
            .unwrap_or(false);
        Ok(Self {
            token,
            group: p["group_guid"].as_str().unwrap_or("").into(),
            paste,
        })
    }
}
pub fn strict_ack(value: &Value) -> bool {
    value.as_object().is_some_and(|o| {
        o.get("ok") == Some(&Value::Bool(true))
            && o.keys().all(|k| k == "ok" || k == "message")
            && o.get("message").is_none_or(Value::is_string)
    })
}
async fn strict_host(ctx: &ExtensionContext, method: &str, params: Value) -> Result<(), String> {
    let v = tokio::time::timeout(Duration::from_secs(10), ctx.host(method, params))
        .await
        .map_err(|_| "Host action timed out")?
        .map_err(|_| "Host action failed")?;
    if strict_ack(&v) {
        Ok(())
    } else {
        Err("Host action was not acknowledged".into())
    }
}
impl Extension for Adapter {
    fn id(&self) -> &'static str {
        ID
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }
    fn commands(&self) -> &'static [&'static str] {
        &["index", "list-links"]
    }
    fn concurrency(&self, _: &str) -> CommandConcurrency {
        CommandConcurrency::Serialize
    }
    fn handle<'a>(
        &'a self,
        ctx: &'a ExtensionContext,
        command: &'a str,
        operation: &'a str,
        payload: &'a Value,
        _: &'a mut Value,
    ) -> BoxFuture<'a, CoreResult> {
        Box::pin(async move {
            let sid = payload["sessionId"]
                .as_str()
                .filter(|s| s.len() <= 256)
                .ok_or("Session ID missing")?;
            if operation == "extension:dispose-session" {
                self.inner
                    .lock()
                    .map_err(|_| "Session state unavailable")?
                    .sessions
                    .remove(sid);
                return Ok(tezbar_extension_sdk::unchanged());
            }
            if !self.commands().contains(&command) {
                return Err("Unknown Bitly command".into());
            }
            let opening = operation == "extension:run-command";
            if operation == "extension:refresh-session" {
                let mut i = self.inner.lock().map_err(|_| "Session state unavailable")?;
                let s = i.sessions.get_mut(sid).ok_or("Session closed")?;
                return Ok(self.render(s, None, vec![]));
            }
            if operation == "extension:load-more" {
                return Ok(tezbar_extension_sdk::unchanged());
            }
            let screen = if opening {
                Some(if command == "list-links" {
                    Screen::List {
                        query: String::new(),
                        rows: vec![],
                        failure: None,
                    }
                } else {
                    Screen::Short {
                        failure: Failure::new("Shortening interrupted. Reopen to recover."),
                        retry: None,
                    }
                })
            } else {
                None
            };
            // Consume a one-use action BEFORE begin clears all IDs and BEFORE any await.
            let action = if operation == "extension:invoke-action" {
                Some(
                    self.inner
                        .lock()
                        .map_err(|_| "Session state unavailable")?
                        .sessions
                        .get_mut(sid)
                        .and_then(|s| s.actions.remove(payload["actionId"].as_str().unwrap_or("")))
                        .ok_or("Action is stale or already used")?,
                )
            } else {
                None
            };
            let mut op = self.begin(sid, opening, screen)?;
            if matches!(action, Some(Action::Settings)) {
                let result = strict_host(
                    ctx,
                    "host.settings.open",
                    json!({"surface":"settings-extensions"}),
                )
                .await;
                let screen = self
                    .inner
                    .lock()
                    .map_err(|_| "Session state unavailable")?
                    .sessions
                    .get(sid)
                    .ok_or("Session closed")?
                    .screen
                    .clone();
                return self.finish(&mut op, screen, Some(if result.is_ok() { "Done" } else { "Opening preferences was not acknowledged. Choose the action again to retry." }), vec![]);
            }
            let prefs = match Prefs::parse(payload) {
                Ok(v) => v,
                Err(e) => return self.failure(&mut op, &e, true, None),
            };
            if opening {
                if command == "list-links" {
                    return self.list(ctx, &mut op, &prefs, String::new(), false).await;
                }
                let literal = match self.selected(ctx).await {
                    Ok(v) => v,
                    Err(e) => return self.failure(&mut op, &e, false, None),
                };
                let key = store::key(&[
                    &ctx.paths.data_dir.to_string_lossy(),
                    &prefs.token,
                    &literal,
                ]);
                return self.short(ctx, &mut op, &prefs, literal, key, false).await;
            }
            if operation == "extension:search-text-changed" && command == "list-links" {
                let query = payload["searchText"]
                    .as_str()
                    .ok_or("Search text missing")?
                    .to_owned();
                return self.list(ctx, &mut op, &prefs, query, false).await;
            }
            if let Some(action) = action {
                match action {
                    Action::RetryShort {
                        literal,
                        key,
                        resend,
                    } => return self.short(ctx, &mut op, &prefs, literal, key, resend).await,
                    Action::RetryList => {
                        let query = match &self
                            .inner
                            .lock()
                            .map_err(|_| "Session state unavailable")?
                            .sessions
                            .get(sid)
                            .ok_or("Session closed")?
                            .screen
                        {
                            Screen::List { query, .. } => query.clone(),
                            _ => return Err("Wrong command action".into()),
                        };
                        return self.list(ctx, &mut op, &prefs, query, true).await;
                    }
                    action => {
                        let result = match action {
                            Action::Copy(text) => {
                                strict_host(ctx, "host.clipboard.write", json!({"text":text})).await
                            }
                            Action::Open(text) => {
                                self.api.list_url(&prefs.group, "").map_err(|f| f.message)?;
                                let mut url = url::Url::parse("https://app.bitly.com/").unwrap();
                                {
                                    let mut path =
                                        url.path_segments_mut().map_err(|_| "Invalid Bitly URL")?;
                                    path.clear().push(&prefs.group).push("links");
                                    if text == "create" {
                                        path.push("create");
                                    } else {
                                        http::safe_web(&text)?;
                                        for segment in text.split('/') {
                                            path.push(segment);
                                        }
                                        path.push("details");
                                    }
                                }
                                strict_host(ctx, "host.shell.open", json!({"target":url.as_str()}))
                                    .await
                            }
                            Action::Settings => {
                                strict_host(
                                    ctx,
                                    "host.settings.open",
                                    json!({"surface":"settings-extensions"}),
                                )
                                .await
                            }
                            _ => unreachable!(),
                        };
                        let screen = self
                            .inner
                            .lock()
                            .map_err(|_| "Session state unavailable")?
                            .sessions
                            .get(sid)
                            .ok_or("Session closed")?
                            .screen
                            .clone();
                        return self.finish(&mut op, screen, Some(if result.is_ok() {"Done"} else {"Host action was not acknowledged. Choose the action again to retry; it may already have happened."}), vec![]);
                    }
                }
            }
            Err("Unsupported Bitly operation".into())
        })
    }
}
