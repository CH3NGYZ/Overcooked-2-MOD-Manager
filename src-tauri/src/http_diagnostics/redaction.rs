use super::MAX_BODY_BYTES;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

const HIDDEN: &str = "[REDACTED_SECRET]";

/// Request-local values also cover servers which echo credentials under an
/// unrelated key or in prose. No request body stream is polled here.
pub(super) struct Redactor {
    known: Vec<String>,
}

fn sensitive(key: &str) -> bool {
    let key = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "passwd",
        "authorization",
        "authentication",
        "cookie",
        "signature",
        "credential",
        "apikey",
        "accesskey",
        "privatekey",
        "codeverifier",
        "codechallenge",
    ]
    .iter()
    .any(|part| key.contains(part))
        || key.ends_with("auth")
        || matches!(
            key.as_str(),
            "auth"
                | "key"
                | "sig"
                | "verifier"
                | "authcode"
                | "oauthcode"
                | "oauthverifier"
                | "authorizationcode"
        )
}

fn content_type(request: &reqwest::Request) -> &str {
    request
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
}

fn media_type(value: &str) -> String {
    value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn binary(kind: &str) -> bool {
    !kind.is_empty()
        && !(kind.starts_with("text/")
            || kind.ends_with("json")
            || kind.ends_with("+json")
            || kind.ends_with("xml")
            || kind.ends_with("+xml")
            || kind == "application/x-www-form-urlencoded")
}

impl Redactor {
    pub(super) fn new(request: &reqwest::Request) -> Self {
        let mut result = Self { known: Vec::new() };
        for (key, value) in request.headers() {
            if sensitive(key.as_str()) || value.is_sensitive() {
                if let Ok(value) = value.to_str() {
                    result.remember(value);
                    if let Some((scheme, value)) = value.split_once(' ') {
                        if scheme.eq_ignore_ascii_case("bearer")
                            || scheme.eq_ignore_ascii_case("basic")
                        {
                            result.remember(value);
                        }
                        if scheme.eq_ignore_ascii_case("basic") && value.len() <= MAX_BODY_BYTES {
                            if let Ok(decoded) = STANDARD.decode(value.trim()) {
                                if let Ok(decoded) = std::str::from_utf8(&decoded) {
                                    result.remember(decoded);
                                    if let Some((username, password)) = decoded.split_once(':') {
                                        result.remember(username);
                                        result.remember(password);
                                    }
                                }
                            }
                        }
                    }
                    for part in value.split(';') {
                        if let Some((_, value)) = part.split_once('=') {
                            result.remember(value.trim());
                        }
                    }
                }
            }
        }
        if let Some(bytes) = request
            .body()
            .and_then(reqwest::Body::as_bytes)
            .filter(|b| b.len() <= MAX_BODY_BYTES)
        {
            let kind = media_type(content_type(request));
            if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
                result.collect_json(&value, false);
            } else if kind == "application/x-www-form-urlencoded" {
                for (key, value) in url::form_urlencoded::parse(bytes) {
                    if sensitive(&key) || key.eq_ignore_ascii_case("code") {
                        result.remember(&value);
                    }
                }
            } else if kind == "multipart/form-data" {
                // GitCode OAuth uses an already-buffered single secret part.
                // Gather named credentials without ever rendering multipart.
                if let Ok(text) = std::str::from_utf8(bytes) {
                    static PART: LazyLock<Regex> = LazyLock::new(|| {
                        Regex::new(r#"(?is)name="([^"]+)"[^\r\n]*\r?\n\r?\n(.*?)\r?\n--"#).unwrap()
                    });
                    for part in PART.captures_iter(text) {
                        if sensitive(&part[1]) || part[1].eq_ignore_ascii_case("code") {
                            result.remember(&part[2]);
                        }
                    }
                }
            }
        }
        result.include_url(request.url());
        result
    }

    /// Redirects can introduce signed credentials which were absent from the
    /// initial request. Hide their values if the destination echoes them too.
    pub(super) fn include_url(&mut self, url: &reqwest::Url) {
        self.remember(url.username());
        if let Some(password) = url.password() {
            self.remember(password);
        }
        for (key, value) in url.query_pairs() {
            if sensitive(&key) || key.eq_ignore_ascii_case("code") {
                self.remember(&value);
            }
        }
        self.known
            .sort_by_key(|value| std::cmp::Reverse(value.len()));
        self.known.dedup();
    }

    fn remember(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }
        self.known.push(value.to_string());
        if let Ok(escaped) = serde_json::to_string(value) {
            self.known.push(escaped[1..escaped.len() - 1].to_string());
        }
        let encoded = url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>();
        if encoded != value {
            // Form encoding uses '+', while an otherwise equivalent URL echo
            // can spell the same space as '%20'. Remember both representations.
            self.known.push(encoded.replace('+', "%20"));
            self.known.push(encoded);
        }
        let encoded = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("v", value)
            .finish();
        self.known.push(encoded[2..].to_string());
        let decoded = url::form_urlencoded::parse(format!("v={value}").as_bytes())
            .next()
            .map(|(_, value)| value.into_owned());
        if let Some(decoded) = decoded {
            if !decoded.is_empty() {
                self.known.push(decoded);
            }
        }
    }

    fn collect_json(&mut self, value: &Value, hidden: bool) {
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    self.collect_json(
                        value,
                        hidden || sensitive(key) || key.eq_ignore_ascii_case("code"),
                    );
                }
            }
            Value::Array(values) => {
                for value in values {
                    self.collect_json(value, hidden);
                }
            }
            Value::String(value) if hidden => self.remember(value),
            Value::Number(value) if hidden => self.remember(&value.to_string()),
            _ => {}
        }
    }

    pub(super) fn request_body(&self, request: &reqwest::Request) -> String {
        let Some(body) = request.body() else {
            return "[无请求正文]".into();
        };
        let kind = media_type(content_type(request));
        if kind.starts_with("multipart/") {
            return "[multipart请求正文已省略]".into();
        }
        let Some(bytes) = body.as_bytes() else {
            return "[流式请求正文未读取]".into();
        };
        self.body(bytes, &kind, false)
    }

    pub(super) fn body(&self, bytes: &[u8], content_type: &str, truncated: bool) -> String {
        if bytes.is_empty() {
            return "[无正文]".into();
        }
        if bytes.len() > MAX_BODY_BYTES {
            return "[正文超过64KiB，已省略]".into();
        }
        let kind = media_type(content_type);
        if binary(&kind) {
            return "[二进制或multipart正文已省略]".into();
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            return "[非UTF-8或不完整正文已省略]".into();
        };
        if text
            .chars()
            .any(|c| c == '\0' || (c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{1b}')))
        {
            return "[二进制正文已省略]".into();
        }
        if truncated && (kind.ends_with("json") || text.trim_start().starts_with(['{', '[', '"'])) {
            return "[JSON正文无效或不完整，已省略]".into();
        }
        if let Ok(mut value) = serde_json::from_str::<Value>(text) {
            self.clean_json(&mut value);
            let output =
                serde_json::to_string(&value).unwrap_or_else(|_| "[JSON正文已省略]".into());
            return if output.len() <= MAX_BODY_BYTES {
                output
            } else {
                "[脱敏正文超过64KiB，已省略]".into()
            };
        }
        // Never print a broken structured document: it may end halfway through
        // a quoted credential key/value or an escape sequence.
        if text.trim_start().starts_with(['{', '[', '"']) {
            return "[JSON正文无效或不完整，已省略]".into();
        }
        let mut text = text.to_string();
        if truncated {
            // A known secret cut at the preview boundary must not leak a prefix.
            for secret in &self.known {
                let overlap = text
                    .char_indices()
                    .map(|(i, _)| i)
                    .find(|&i| secret.starts_with(&text[i..]));
                if let Some(offset) = overlap {
                    text.truncate(offset);
                }
            }
        }
        let text = self.text(&text);
        let output = if truncated {
            format!("{text} [正文预览不完整]")
        } else {
            text
        };
        if output.len() <= MAX_BODY_BYTES {
            output
        } else {
            "[脱敏正文超过64KiB，已省略]".into()
        }
    }

    fn clean_json(&self, value: &mut Value) {
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    if sensitive(key) {
                        *value = Value::String(HIDDEN.into());
                    } else {
                        self.clean_json(value);
                    }
                }
            }
            Value::Array(values) => {
                for value in values {
                    self.clean_json(value);
                }
            }
            Value::String(value) => *value = self.text(value),
            Value::Number(number) if self.known.contains(&number.to_string()) => {
                *value = Value::String(HIDDEN.into());
            }
            _ => {}
        }
    }

    pub(super) fn text(&self, value: &str) -> String {
        let mut value = value.to_string();
        for secret in &self.known {
            value = value.replace(secret, HIDDEN);
        }
        redact_text(&value)
    }
}

/// Shared plain-text rule for detailed logs and the short publish UI reason.
/// Redact before any display truncation; do not return control/format characters.
pub(crate) fn redact_text(value: &str) -> String {
    static SECRETS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(concat!(
        r#"(?is)\b(?:[a-z0-9_-]*(?:token|secret|password|passwd|authorization|authentication|cookie|signature|credential|api[_-]?key|access[_-]?key|private[_-]?key)[a-z0-9_-]*|auth|code|verifier|code[_-]?verifier|code[_-]?challenge|(?:oauth|auth)[_-]?code|oauth[_-]?verifier)\b\s*["']?\s*[:=].*"#,
        r#"|\b(?:bearer|basic)\s+[^\s,;<>"']+"#,
        r#"|https?://[^\s<>"']+"#,
        r#"|\b(?:gh[pousr]_|github_pat_)[A-Za-z0-9_]+"#,
        r#"|\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+"#,
        r#"|-----BEGIN [^-]*PRIVATE KEY-----.*"#,
    )).expect("valid HTTP credential redaction pattern")
    });
    let redacted = SECRETS.replace_all(value, HIDDEN);
    redacted.chars().map(|ch| if ch.is_control() || matches!(ch, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') { ' ' } else { ch }).collect::<String>()
        .split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests;
