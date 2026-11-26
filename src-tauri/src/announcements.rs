use crate::http_diagnostics::HttpRequestExt;
use chrono::{DateTime, SecondsFormat, Timelike};
use reqwest::{Client, ClientBuilder, StatusCode, header};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;
use std::time::Duration;
use uuid::{Uuid, Variant, Version};

const MAX_SAFE_REVISION: u64 = (1 << 53) - 1;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const JSON_MEDIA_TYPE: &str = "application/json";

static PROTOCOL: LazyLock<Protocol> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../src/announcements/protocol.json"))
        .expect("valid shared announcement protocol")
});
static LOCALES: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../src/locale/labels.json"))
        .expect("valid shared locale labels")
});

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Protocol {
    origin: String,
    path: String,
    max_items: usize,
    max_response_bytes: usize,
    max_title_bytes: usize,
    max_body_bytes: usize,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnnouncementSnapshot {
    revision: u64,
    announcements: Vec<Announcement>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Announcement {
    id: String,
    published_at: String,
    language: String,
    title: String,
    body: String,
}

#[derive(Debug, PartialEq, Eq)]
enum ReadError {
    Language,
    Client,
    Request,
    Status(u16),
    ContentType,
    TooLarge,
    Read,
    InvalidSnapshot,
}

fn valid_content(title: &str, body: &str) -> bool {
    let protocol = &*PROTOCOL;
    title.len() <= protocol.max_title_bytes
        && body.len() <= protocol.max_body_bytes
        && !title.trim().is_empty()
        && !body.trim().is_empty()
        && !title.chars().any(char::is_control)
        && !body
            .chars()
            .any(|character| character.is_control() && character != '\n' && character != '\t')
}

fn parse_snapshot(bytes: &[u8]) -> Result<AnnouncementSnapshot, ReadError> {
    let snapshot: AnnouncementSnapshot =
        serde_json::from_slice(bytes).map_err(|_| ReadError::InvalidSnapshot)?;
    if snapshot.revision > MAX_SAFE_REVISION || snapshot.announcements.len() > PROTOCOL.max_items {
        return Err(ReadError::InvalidSnapshot);
    }

    let mut ids = HashSet::new();
    let mut previous = None;
    for announcement in &snapshot.announcements {
        let id = Uuid::parse_str(&announcement.id).map_err(|_| ReadError::InvalidSnapshot)?;
        let time = DateTime::parse_from_rfc3339(&announcement.published_at)
            .map_err(|_| ReadError::InvalidSnapshot)?;
        if id.get_version() != Some(Version::Random)
            || id.get_variant() != Variant::RFC4122
            || id.to_string() != announcement.id
            || !ids.insert(id)
            || announcement.published_at.len() != 24
            || time.nanosecond() >= 1_000_000_000
            || time.to_rfc3339_opts(SecondsFormat::Millis, true) != announcement.published_at
            || previous.is_some_and(|previous| time > previous)
            || !LOCALES.contains_key(&announcement.language)
            || !valid_content(&announcement.title, &announcement.body)
        {
            return Err(ReadError::InvalidSnapshot);
        }
        previous = Some(time);
    }
    Ok(snapshot)
}

fn client_builder() -> ClientBuilder {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .referer(false)
}

async fn fetch_snapshot(
    client: &Client,
    endpoint: &str,
    language: &str,
    fresh: bool,
) -> Result<AnnouncementSnapshot, ReadError> {
    if !LOCALES.contains_key(language) {
        return Err(ReadError::Language);
    }
    let mut request = client
        .get(endpoint)
        .query(&[("lang", language)])
        .header(header::ACCEPT, JSON_MEDIA_TYPE);
    if fresh {
        request = request.query(&[("fresh", "1")]);
    }
    let mut response = request
        .send_logged_metadata()
        .await
        .map_err(|_| ReadError::Request)?;
    if response.status() != StatusCode::OK {
        return Err(ReadError::Status(response.status().as_u16()));
    }
    let json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(JSON_MEDIA_TYPE));
    if !json {
        return Err(ReadError::ContentType);
    }

    let maximum = PROTOCOL.max_response_bytes;
    if response
        .content_length()
        .is_some_and(|size| size > maximum as u64)
    {
        return Err(ReadError::TooLarge);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ReadError::Read)? {
        if chunk.len() > maximum - bytes.len() {
            return Err(ReadError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    parse_snapshot(&bytes)
}

async fn read_snapshot(language: &str, fresh: bool) -> Result<AnnouncementSnapshot, ReadError> {
    let client = client_builder()
        .https_only(true)
        .build()
        .map_err(|_| ReadError::Client)?;
    let endpoint = format!("{}{}", PROTOCOL.origin, PROTOCOL.path);
    fetch_snapshot(&client, &endpoint, language, fresh).await
}

pub(crate) async fn load(language: &str, fresh: bool) -> Option<AnnouncementSnapshot> {
    match read_snapshot(language, fresh).await {
        Ok(snapshot) => Some(snapshot),
        Err(error) => {
            log::warn!("读取公告失败：类别={error:?}");
            None
        }
    }
}

#[cfg(test)]
mod tests;
