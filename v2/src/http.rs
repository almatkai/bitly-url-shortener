use reqwest::{header, Client, Method};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use url::Url;

pub const API: &str = "https://api-ssl.bitly.com/v4/";
pub const MAX_BODY: usize = 2 * 1024 * 1024;
pub const MAX_ROWS: usize = 1000;
pub const MAX_TEXT: usize = 8192;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Link {
    pub id: String,
    pub link: String,
    pub title: String,
    pub long_url: String,
    pub created_at: String,
}

#[derive(Clone, Debug)]
pub struct Failure {
    pub auth: bool,
    pub message: String,
}
impl Failure {
    pub fn new(message: &str) -> Self {
        Self {
            auth: false,
            message: message.into(),
        }
    }
}
#[derive(Clone)]
pub struct Api {
    client: Client,
    base: Url,
}
impl Api {
    pub fn production() -> Result<Self, String> {
        let client = Self::builder()
            .https_only(true)
            .build()
            .map_err(|_| "HTTP initialization failed")?;
        Ok(Self {
            client,
            base: Url::parse(API).expect("constant API URL"),
        })
    }
    fn builder() -> reqwest::ClientBuilder {
        Client::builder()
            .http1_only()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .pool_max_idle_per_host(2)
    }
    // Only the explicit QA target can change origin/trust. Production has no environment seam.
    #[cfg(any(test, feature = "qa"))]
    pub fn fixture(base: &str, certificate: Option<&[u8]>) -> Self {
        let base = Url::parse(base).unwrap();
        assert!(matches!(
            base.host_str(),
            Some("127.0.0.1" | "localhost" | "[::1]")
        ));
        let mut builder = Self::builder();
        if let Some(pem) = certificate {
            builder = builder.add_root_certificate(reqwest::Certificate::from_pem(pem).unwrap());
        }
        Self {
            client: builder.build().unwrap(),
            base,
        }
    }
    pub fn list_url(&self, group: &str, query: &str) -> Result<Url, Failure> {
        if group.is_empty()
            || group.len() > 128
            || !group
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return Err(Failure::new(
                "Group GUID is invalid. Open Command Preferences.",
            ));
        }
        if query.len() > 1024 || query.contains('\0') {
            return Err(Failure::new("Search text is too long or invalid"));
        }
        let mut url = self.base.join(&format!("groups/{group}/bitlinks")).unwrap();
        url.query_pairs_mut().append_pair("query", query);
        Ok(url)
    }
    async fn request(
        &self,
        method: Method,
        url: Url,
        token: &str,
        body: Option<Value>,
    ) -> Result<Value, Failure> {
        if token.is_empty()
            || token.len() > 4096
            || !token.bytes().all(|b| (0x21..=0x7e).contains(&b))
        {
            return Err(Failure { auth: true, message: "Check that your Bitly access token is correct and hasn't expired or been revoked.".into() });
        }
        // Never follow API-supplied pagination/redirect URLs. Credentials stay on this one origin.
        if url.origin() != self.base.origin() || !url.path().starts_with(self.base.path()) {
            return Err(Failure::new("Rejected an unexpected Bitly API destination"));
        }
        let mut request = self
            .client
            .request(method, url)
            .bearer_auth(token)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| Failure::new("Bitly request failed or timed out"))?;
        let status = response.status();
        if status.is_redirection() {
            return Err(Failure::new("Bitly redirect refused"));
        }
        if response
            .content_length()
            .is_some_and(|v| v > MAX_BODY as u64)
        {
            return Err(Failure::new("Bitly response exceeds the size limit"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| Failure::new("Bitly response was incomplete"))?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_BODY {
                return Err(Failure::new("Bitly response exceeds the size limit"));
            }
            bytes.extend_from_slice(&chunk);
        }
        let result: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Failure::new("Bitly returned invalid JSON"))?;
        validate_tree(&result, 0, &mut 0)?;
        if !status.is_success() {
            // Original detail strings may echo tokens/URLs. Preserve categories, never remote text.
            return Err(if status.as_u16() == 401 {
                Failure { auth: true, message: "Check that your Bitly access token is correct and hasn't expired or been revoked.".into() }
            } else {
                Failure::new(match status.as_u16() {
                    403 => "Bitly API Error - Permission or plan access denied (403)",
                    429 => "Bitly API Error - Rate limit exceeded (429). Retry later.",
                    _ => "Bitly API Error - Request rejected",
                })
            });
        }
        if contains_secret(&result, token) {
            return Err(Failure::new("Bitly returned credential-bearing data"));
        }
        if result.get("error").is_some()
            || result.get("ok") == Some(&Value::Bool(false))
            || !result.is_object()
            || result
                .get("errors")
                .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
            || result.get("message").is_some()
                && result.get("link").is_none()
                && result.get("links").is_none()
        {
            return Err(Failure::new(
                "Bitly returned an incomplete or contradictory result",
            ));
        }
        Ok(result)
    }
    pub async fn shorten(&self, token: &str, literal: &str) -> Result<String, Failure> {
        let result = self
            .request(
                Method::POST,
                self.base.join("shorten").unwrap(),
                token,
                Some(json!({"long_url":literal})),
            )
            .await?;
        let link = result["link"]
            .as_str()
            .ok_or_else(|| Failure::new("Bitly returned no shortened link"))?;
        safe_web(link).map_err(|_| Failure::new("Bitly returned an unsafe shortened link"))?;
        Ok(link.into())
    }
    pub async fn list(&self, token: &str, group: &str, query: &str) -> Result<Vec<Link>, Failure> {
        let result = self
            .request(Method::GET, self.list_url(group, query)?, token, None)
            .await?;
        let rows = result["links"]
            .as_array()
            .ok_or_else(|| Failure::new("Bitly returned no link list"))?;
        if rows.len() > MAX_ROWS {
            return Err(Failure::new("Bitly link page exceeds the row limit"));
        }
        let mut seen = std::collections::HashSet::new();
        let mut links = Vec::with_capacity(rows.len());
        for row in rows {
            let link: Link = serde_json::from_value(row.clone())
                .map_err(|_| Failure::new("Bitly returned an incomplete link row"))?;
            if link.id.is_empty()
                || !seen.insert(link.id.clone())
                || [
                    &link.id,
                    &link.link,
                    &link.title,
                    &link.long_url,
                    &link.created_at,
                ]
                .iter()
                .any(|v| v.len() > MAX_TEXT || v.chars().any(|c| c.is_control()))
                || chrono::DateTime::parse_from_rfc3339(&link.created_at).is_err()
            {
                return Err(Failure::new("Bitly returned an invalid link row"));
            }
            safe_web(&link.link).map_err(|_| Failure::new("Bitly returned an unsafe link"))?;
            // No favicon fetch: long_url is data and never rendered as HTML/Markdown or requested.
            links.push(link);
        }
        // The original useFetch does not configure pagination. One complete page only.
        Ok(links)
    }
}
fn validate_tree(value: &Value, depth: usize, nodes: &mut usize) -> Result<(), Failure> {
    *nodes += 1;
    if depth > 32 || *nodes > 30000 {
        return Err(Failure::new("Bitly JSON exceeds structural limits"));
    }
    match value {
        Value::Array(a) => {
            for v in a {
                validate_tree(v, depth + 1, nodes)?;
            }
        }
        Value::Object(a) => {
            for (k, v) in a {
                if k.len() > MAX_TEXT {
                    return Err(Failure::new("Bitly JSON key exceeds the size limit"));
                }
                validate_tree(v, depth + 1, nodes)?;
            }
        }
        Value::String(s) if s.len() > 65536 => {
            return Err(Failure::new("Bitly JSON string exceeds the size limit"))
        }
        _ => {}
    }
    Ok(())
}
pub fn safe_web(text: &str) -> Result<Url, String> {
    if text.len() > MAX_TEXT || text.chars().any(|c| c.is_control() || c == '\\') {
        return Err("Unsafe web URL".into());
    }
    let url = Url::parse(text).map_err(|_| "Invalid URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Unsafe web URL".into());
    }
    Ok(url)
}
pub fn input_url(text: &str) -> Result<String, String> {
    let literal = js_trim(text);
    if literal.is_empty() {
        return Err("No text selected and clipboard is empty".into());
    }
    if literal.len() > MAX_TEXT || literal.contains('\0') {
        return Err("Invalid URL".into());
    }
    // WHATWG-compatible absolute parsing, but send the source literal, not serialization.
    Url::parse(literal).map_err(|_| "Invalid URL")?;
    Ok(literal.into())
}

pub fn js_trim(text: &str) -> &str {
    text.trim_matches(|c| matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'))
}
fn contains_secret(value: &Value, token: &str) -> bool {
    match value {
        Value::String(s) => s.contains(token),
        Value::Array(a) => a.iter().any(|v| contains_secret(v, token)),
        Value::Object(o) => o
            .iter()
            .any(|(k, v)| k.contains(token) || contains_secret(v, token)),
        _ => false,
    }
}
