use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
use tezbar_bitly_url_shortener::{
    http::{input_url, safe_web, Api},
    strict_ack, Adapter,
};
use tezbar_extension_sdk::{Extension, ExtensionPaths, TestContext};

fn server(
    status: u16,
    body: String,
    headers: &str,
    delay: u64,
) -> (Api, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/v4/", listener.local_addr().unwrap());
    let headers = headers.to_owned();
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut raw = Vec::new();
        let mut buf = [0; 4096];
        loop {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            raw.extend_from_slice(&buf[..n]);
            if let Some(h) = raw.windows(4).position(|v| v == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&raw[..h]).to_lowercase();
                let length = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                if raw.len() >= h + 4 + length {
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(delay));
        let response = format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len());
        let _ = stream.write_all(response.as_bytes());
        String::from_utf8(raw).unwrap()
    });
    (Api::fixture(&base, None), task)
}
fn row() -> Value {
    json!({"id":"bit.ly/test","link":"https://bit.ly/test","long_url":"https://example.test/a","title":"<img src=x> ![x](https://evil.test)","created_at":"2026-01-02T03:04:05Z"})
}
fn context() -> (tempfile::TempDir, TestContext) {
    let qa_tmp = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qa/tmp");
    std::fs::create_dir_all(&qa_tmp).unwrap();
    let temp = tempfile::tempdir_in(qa_tmp).unwrap();
    let data = temp.path().canonicalize().unwrap();
    let ctx = TestContext::new(
        ExtensionPaths {
            data_dir: data.clone(),
            temp_dir: data.clone(),
            package_dir: data,
        },
        &[
            "clipboard.read",
            "selection.read",
            "clipboard.write",
            "clipboard.pasteText",
            "shell.open",
            "settings.open",
        ],
    );
    (temp, ctx)
}
fn payload(sid: &str) -> Value {
    json!({"sessionId":sid,"preferences":{"accessToken":"synthetic-test-credential","group_guid":"group_test"}})
}
async fn call(a: &Adapter, t: &TestContext, c: &str, o: &str, p: &Value) -> Value {
    t.run(a.handle(&t.context, c, o, p, &mut json!({})))
        .await
        .unwrap()
}
#[test]
fn literal_url_and_closed_ack() {
    for text in [
        "HTTPS://EXAMPLE.test/a/../b?x=1",
        "mailto:hello@example.test",
        "javascript:alert(1)",
        "file:///tmp/test",
        "https://münich.test/π",
        "https:example.test",
    ] {
        assert_eq!(input_url(&format!("\u{feff} {text} \n")).unwrap(), text);
    }
    for text in ["", "example.test", "/relative", "http://", "https://[oops"] {
        assert!(input_url(text).is_err());
    }
    for text in [
        "javascript:alert(1)",
        "file:///tmp/a",
        "https://user:pass@example.test",
        "https://example.test\\evil",
    ] {
        assert!(safe_web(text).is_err());
    }
    assert!(strict_ack(&json!({"ok":true})));
    assert!(strict_ack(&json!({"ok":true,"message":"fine"})));
    for v in [
        Value::Null,
        json!(true),
        json!({"ok":false}),
        json!({"ok":true,"error":"bad"}),
        json!({"ok":true,"message":null}),
        json!({"ok":true,"extra":1}),
    ] {
        assert!(!strict_ack(&v));
    }
}
#[tokio::test]
async fn http_literal_and_encoded_group_query_no_pagination() {
    let (a, t) = server(201, json!({"link":"https://bit.ly/a"}).to_string(), "", 0);
    assert_eq!(
        a.shorten("synthetic-test-credential", "HTTPS://EXAMPLE.test/a/../b")
            .await
            .unwrap(),
        "https://bit.ly/a"
    );
    let raw = t.join().unwrap();
    assert!(raw.contains("Bearer synthetic-test-credential"));
    assert!(raw.contains("HTTPS://EXAMPLE.test/a/../b"));
    let (a, t) = server(
        200,
        json!({"links":[row()],"pagination":{"next":"https://evil.test/?steal"}}).to_string(),
        "",
        0,
    );
    assert_eq!(
        a.list("synthetic-test-credential", "group_test", "a&b #😀")
            .await
            .unwrap()
            .len(),
        1
    );
    let raw = t.join().unwrap();
    assert!(raw.starts_with("GET /v4/groups/group_test/bitlinks?query=a%26b+%23%F0%9F%98%80 "));
    assert!(a.list_url("../escape", "").is_err());
    assert!(a.list_url("group_test", &"a".repeat(1025)).is_err());
}
#[tokio::test]
async fn status_redirect_partial_json_and_credential_redaction() {
    for (status, body, headers, auth) in [
        (
            401,
            json!({"message":"synthetic-test-credential https://secret.test"}).to_string(),
            "",
            true,
        ),
        (403, json!({"message":"secret"}).to_string(), "", false),
        (429, json!({"message":"secret"}).to_string(), "", false),
        (302, "{}".into(), "Location: https://evil.test/\r\n", false),
        (200, "{".into(), "", false),
        (
            200,
            json!({"links":[row(),{"title":"partial"}]}).to_string(),
            "",
            false,
        ),
        (
            200,
            json!({"links":[],"errors":[{"message":"secret"}]}).to_string(),
            "",
            false,
        ),
        (
            200,
            json!({"links":[],"error":"contradiction"}).to_string(),
            "",
            false,
        ),
        (
            200,
            json!({"links":[],"unexpected":"synthetic-test-credential"}).to_string(),
            "",
            false,
        ),
        (200, " ".repeat(2 * 1024 * 1024 + 1), "", false),
    ] {
        let (a, t) = server(status, body, headers, 0);
        let e = a
            .list("synthetic-test-credential", "group_test", "")
            .await
            .unwrap_err();
        assert_eq!(e.auth, auth);
        assert!(!e.message.contains("synthetic-test-credential"));
        assert!(!e.message.contains("https://"));
        t.join().unwrap();
    }
}
#[tokio::test]
async fn shorten_copy_retry_exact_no_refetch_and_reopen() {
    let (api, t) = server(
        200,
        json!({"link":"https://bit.ly/saved"}).to_string(),
        "",
        0,
    );
    let a = Adapter::with_api(api);
    let (_d, c) = context();
    c.reply(Ok(json!("   ")));
    c.reply(Ok(json!(" https://example.test/a ")));
    c.reply(Ok(json!(null)));
    let p = payload("one");
    let failed = call(&a, &c, "index", "extension:run-command", &p).await;
    t.join().unwrap();
    assert_eq!(failed["mode"], "view");
    let id = failed["actions"][0]["id"].clone();
    c.reply(Ok(json!({"ok":true})));
    let mut action = p.clone();
    action["actionId"] = id.clone();
    assert_eq!(
        call(&a, &c, "index", "extension:invoke-action", &action).await["mode"],
        "no-view"
    );
    let mut state = json!({});
    assert!(c
        .run(a.handle(
            &c.context,
            "index",
            "extension:invoke-action",
            &action,
            &mut state
        ))
        .await
        .is_err());
    c.reply(Ok(json!("https://example.test/a")));
    c.reply(Ok(json!({"ok":true})));
    let mut p2 = payload("two");
    p2["preferences"]["pasteAfterShortening"] = json!(false);
    assert_eq!(
        call(&a, &c, "index", "extension:run-command", &p2).await["mode"],
        "no-view"
    );
    let copies: Vec<_> = c
        .calls()
        .into_iter()
        .filter(|v| v["method"] == "host.clipboard.write")
        .collect();
    assert_eq!(copies.len(), 3);
    assert!(copies
        .iter()
        .all(|v| v["params"] == json!({"text":"https://bit.ly/saved"})));
}
#[tokio::test]
async fn selection_error_clipboard_fallback_and_copy_paste_order() {
    let (api, t) = server(
        200,
        json!({"link":"https://bit.ly/paste"}).to_string(),
        "",
        0,
    );
    let a = Adapter::with_api(api);
    let (_d, c) = context();
    c.reply(Err("no selection".into()));
    c.reply(Ok(json!("mailto:hello@example.test")));
    c.reply(Ok(json!({"ok":true})));
    c.reply(Ok(json!({"ok":true})));
    let mut p = payload("paste");
    p["preferences"]["pasteAfterShortening"] = json!(true);
    let result = call(&a, &c, "index", "extension:run-command", &p).await;
    t.join().unwrap();
    assert_eq!(result["effects"][1]["style"], "success");
    let methods: Vec<_> = c
        .calls()
        .iter()
        .map(|v| v["method"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        methods,
        [
            "host.selection.read",
            "host.clipboard.read",
            "host.clipboard.write",
            "host.clipboard.pasteText"
        ]
    );
}
#[tokio::test]
async fn canceled_post_persists_uncertainty_no_automatic_resend() {
    let (api, t) = server(
        200,
        json!({"link":"https://bit.ly/late"}).to_string(),
        "",
        100,
    );
    let a = Adapter::with_api(api);
    let (_d, c) = context();
    c.reply(Ok(json!("https://example.test/cancel")));
    let p = payload("cancel");
    let mut s = json!({});
    assert!(tokio::time::timeout(
        Duration::from_millis(30),
        c.run(a.handle(&c.context, "index", "extension:run-command", &p, &mut s))
    )
    .await
    .is_err());
    t.join().unwrap();
    c.reply(Ok(json!("https://example.test/cancel")));
    let p = payload("reopen");
    let r = call(&a, &c, "index", "extension:run-command", &p).await;
    assert_eq!(
        r["actions"][0]["title"],
        "Retry Shortening (may create duplicate)"
    );
    assert!(r.to_string().contains("may have created"));
    assert_eq!(
        call(&a, &c, "index", "extension:refresh-session", &p).await["mode"],
        "view"
    );
}
#[tokio::test]
async fn ordinary_list_failure_retry_stale_actions_and_profile_cache_isolation() {
    let (api, t) = server(401, json!({"message":"bad"}).to_string(), "", 0);
    let a = Adapter::with_api(api);
    let (_d, c) = context();
    let p = payload("list");
    let r = call(&a, &c, "list-links", "extension:run-command", &p).await;
    t.join().unwrap();
    assert_eq!(r["actions"][0]["title"], "Retry");
    assert_eq!(r["actions"][1]["title"], "Open Command Preferences");
    let refresh = call(&a, &c, "list-links", "extension:refresh-session", &p).await;
    assert_ne!(r["actions"][0]["id"], refresh["actions"][0]["id"]);
    let mut p = p.clone();
    p["actionId"] = r["actions"][1]["id"].clone();
    let mut state = json!({});
    assert!(c
        .run(a.handle(
            &c.context,
            "list-links",
            "extension:invoke-action",
            &p,
            &mut state
        ))
        .await
        .is_err());
}
#[tokio::test]
async fn store_corruption_and_symlink_refused_before_http() {
    let a = Adapter::with_api(Api::fixture("http://127.0.0.1:1/v4/", None));
    let (d, c) = context();
    std::fs::create_dir(d.path().join("bitly-v2")).unwrap();
    std::fs::write(d.path().join("bitly-v2/recovery.json"), "{").unwrap();
    c.reply(Ok(json!("https://example.test/a")));
    let r = call(&a, &c, "index", "extension:run-command", &payload("bad")).await;
    assert!(r.to_string().contains("corrupt"));
    assert_eq!(c.calls().len(), 1);
    #[cfg(unix)]
    {
        let (d, c) = context();
        std::os::unix::fs::symlink(d.path(), d.path().join("bitly-v2")).unwrap();
        c.reply(Ok(json!("https://example.test/a")));
        let r = call(
            &a,
            &c,
            "index",
            "extension:run-command",
            &payload("symlink"),
        )
        .await;
        assert!(r.to_string().contains("unsafe"));
    }
}

#[test]
fn executed_source_url_oracle_matches_rust_literal_validation() {
    let cases: Value = serde_json::from_str(include_str!("../evidence/url-oracle.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let result = input_url(case["input"].as_str().unwrap());
        assert_eq!(
            result.is_ok(),
            case["accepted"].as_bool().unwrap(),
            "input={:?}",
            case["input"]
        );
        if let Ok(literal) = result {
            assert_eq!(literal, case["literal"].as_str().unwrap());
        }
    }
}
#[tokio::test]
async fn missing_token_preferences_action_remains_reachable() {
    let a = Adapter::with_api(Api::fixture("http://127.0.0.1:1/v4/", None));
    let (_d, c) = context();
    let mut p = payload("settings");
    p["preferences"]["accessToken"] = json!("");
    let r = call(&a, &c, "index", "extension:run-command", &p).await;
    p["actionId"] = r["actions"][0]["id"].clone();
    c.reply(Ok(json!({"ok":true})));
    assert_eq!(
        call(&a, &c, "index", "extension:invoke-action", &p).await["message"],
        "Done"
    );
    assert_eq!(c.calls()[0]["method"], "host.settings.open");
}

#[tokio::test]
async fn list_open_preserves_original_bitly_route_and_safe_ack() {
    let (api, t) = server(200, json!({"links":[row()]}).to_string(), "", 0);
    let a = Adapter::with_api(api);
    let (_d, c) = context();
    let mut p = payload("browser");
    let r = call(&a, &c, "list-links", "extension:run-command", &p).await;
    t.join().unwrap();
    p["actionId"] = r["actions"][1]["id"].clone();
    c.reply(Ok(json!({"ok":true})));
    assert_eq!(
        call(&a, &c, "list-links", "extension:invoke-action", &p).await["message"],
        "Done"
    );
    assert_eq!(
        c.calls()[0]["params"],
        json!({"target":"https://app.bitly.com/group_test/links/https://bit.ly/test/details"})
    );
}
