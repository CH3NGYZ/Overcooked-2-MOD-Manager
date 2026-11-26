use super::*;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread::{self, JoinHandle};
use std::time::Instant;

const ID: &str = "90ea1743-226b-4142-b6a0-000000000001";

fn snapshot() -> Value {
    json!({
        "revision": 3,
        "announcements": [{
            "id": ID,
            "published_at": "2026-11-26T12:00:00.123Z",
            "language": "zh-CN",
            "title": "公告",
            "body": "第一行\n\t第二行",
            "translations": { "en": { "title": "Notice", "body": "<strong>plain text</strong>" } }
        }, {
            "id": "90ea1743-226b-4142-b6a0-000000000002",
            "published_at": "2026-11-25T12:00:00.000Z",
            "language": "en",
            "title": "Previous announcement",
            "body": "Original text",
            "translations": {}
        }]
    })
}

fn parse(value: &Value) -> Result<AnnouncementSnapshot, ReadError> {
    parse_snapshot(&serde_json::to_vec(value).unwrap())
}

#[test]
fn preserves_order_original_language_translations_and_literal_text() {
    let value = snapshot();
    let parsed = parse(&value).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    assert!(
        parse(&json!({"revision": 0, "announcements": []}))
            .unwrap()
            .announcements
            .is_empty()
    );
}

#[test]
fn rejects_invalid_snapshot_keys_ids_timestamps_and_order() {
    let mut duplicate = snapshot();
    duplicate["announcements"][1]["id"] = json!(ID);
    let mut empty = snapshot();
    empty["announcements"] = json!(vec![Value::Null; PROTOCOL.max_items + 1]);
    let mut outdated_order = snapshot();
    outdated_order["announcements"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let mut unsafe_revision = snapshot();
    unsafe_revision["revision"] = json!(MAX_SAFE_REVISION + 1);
    for value in [duplicate, empty, outdated_order, unsafe_revision] {
        assert_eq!(parse(&value).unwrap_err(), ReadError::InvalidSnapshot);
    }

    for (field, value) in [
        ("id", json!(ID.to_ascii_uppercase())),
        ("id", json!("90ea1743-226b-3142-b6a0-000000000001")),
        ("id", json!("90ea1743-226b-4142-06a0-000000000001")),
        ("published_at", json!("2026-11-26T12:00:00+00:00")),
        ("published_at", json!("2026-11-26T12:00:60.000Z")),
        ("published_at", json!("2026-02-29T12:00:00.000Z")),
        ("unexpected", json!(true)),
    ] {
        let mut snapshot = snapshot();
        snapshot["announcements"][0][field] = value;
        assert_eq!(
            parse(&snapshot).unwrap_err(),
            ReadError::InvalidSnapshot,
            "{field}"
        );
    }
}

#[test]
fn accepts_exact_byte_limits_and_rejects_invalid_localized_content() {
    let mut at_limit = snapshot();
    at_limit["announcements"][0]["title"] = json!("A".repeat(PROTOCOL.max_title_bytes));
    at_limit["announcements"][0]["body"] = json!("B".repeat(PROTOCOL.max_body_bytes));
    assert!(parse(&at_limit).is_ok());

    for (field, value) in [
        ("language", json!("en-US")),
        (
            "title",
            json!("字".repeat(PROTOCOL.max_title_bytes / 3 + 1)),
        ),
        ("title", json!("Line 1\nLine 2")),
        ("body", json!("字".repeat(PROTOCOL.max_body_bytes / 3 + 1))),
        ("body", json!("\u{7f}")),
        ("body", json!(" \n\t ")),
        (
            "translations",
            json!({ "xx": { "title": "Title", "body": "Body" } }),
        ),
        (
            "translations",
            json!({ "zh-CN": { "title": "Title", "body": "Body" } }),
        ),
        (
            "translations",
            json!({ "en": { "title": "Title", "body": "" } }),
        ),
        ("translations", json!({ "en": { "title": "Title" } })),
        ("translations", Value::Null),
    ] {
        let mut snapshot = snapshot();
        snapshot["announcements"][0][field] = value;
        assert_eq!(
            parse(&snapshot).unwrap_err(),
            ReadError::InvalidSnapshot,
            "{field}"
        );
    }
}

fn fixture(response_headers: String, body: Vec<u8>) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}{}", listener.local_addr().unwrap(), PROTOCOL.path);
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(accepted) => break accepted,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "announcement HTTP fixture did not receive request"
                    );
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("announcement HTTP fixture accept failed: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut request = String::new();
        loop {
            let length = reader.read_line(&mut request).unwrap();
            assert_ne!(length, 0, "connection closed before request headers");
            assert!(request.len() < 16 * 1024);
            if length == 2 && request.ends_with("\r\n\r\n") {
                break;
            }
        }
        assert!(request.starts_with(&format!("GET {} HTTP/1.1\r\n", PROTOCOL.path)));
        let headers = request.to_ascii_lowercase();
        assert!(headers.contains(&format!("\r\naccept: {JSON_MEDIA_TYPE}\r\n")));
        assert!(headers.contains(&format!(
            "\r\nuser-agent: {}\r\n",
            crate::http_user_agent::app_user_agent().to_ascii_lowercase()
        )));
        assert!(!headers.contains("\r\nauthorization:"));
        assert!(!headers.contains("\r\ncookie:"));
        stream.write_all(response_headers.as_bytes()).unwrap();
        let _ = stream.write_all(&body);
    });
    (endpoint, worker)
}

async fn local_fetch(headers: String, body: Vec<u8>) -> Result<AnnouncementSnapshot, ReadError> {
    let (endpoint, worker) = fixture(headers, body);
    let client = client_builder().no_proxy().build().unwrap();
    let result = fetch_snapshot(&client, &endpoint).await;
    worker.join().unwrap();
    result
}

fn with_length(status: &str, content_type: &str, length: usize) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
    )
}

#[tokio::test]
async fn requires_http_200_with_json_and_does_not_follow_redirects() {
    let body = serde_json::to_vec(&snapshot()).unwrap();
    let success = local_fetch(
        with_length(
            "200 OK",
            &format!("{JSON_MEDIA_TYPE}; charset=utf-8"),
            body.len(),
        ),
        body.clone(),
    )
    .await
    .unwrap();
    assert_eq!(success.revision, 3);
    assert_eq!(success.announcements.len(), 2);
    let html = local_fetch(with_length("200 OK", "text/html", body.len()), body).await;
    assert_eq!(html.unwrap_err(), ReadError::ContentType);

    let redirect = local_fetch(
        "HTTP/1.1 302 Found\r\nLocation: /not-the-announcement-route\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
        Vec::new(),
    ).await;
    assert_eq!(redirect.unwrap_err(), ReadError::Status(302));

    let unavailable = local_fetch(
        with_length("503 Service Unavailable", JSON_MEDIA_TYPE, 0),
        Vec::new(),
    )
    .await;
    assert_eq!(unavailable.unwrap_err(), ReadError::Status(503));
}

#[tokio::test]
async fn bounds_declared_and_chunked_bodies_and_rejects_truncation() {
    let maximum = PROTOCOL.max_response_bytes;
    let declared = local_fetch(
        with_length("200 OK", JSON_MEDIA_TYPE, maximum + 1),
        Vec::new(),
    )
    .await;
    assert_eq!(declared.unwrap_err(), ReadError::TooLarge);

    let prefix = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {JSON_MEDIA_TYPE}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
    );
    let mut chunked = format!("{:x}\r\n", maximum + 1).into_bytes();
    chunked.extend(std::iter::repeat_n(b'x', maximum + 1));
    chunked.extend_from_slice(b"\r\n0\r\n\r\n");
    assert_eq!(
        local_fetch(prefix, chunked).await.unwrap_err(),
        ReadError::TooLarge
    );

    let body = serde_json::to_vec(&snapshot()).unwrap();
    let truncated = local_fetch(
        with_length("200 OK", JSON_MEDIA_TYPE, body.len() + 1),
        body.clone(),
    )
    .await;
    assert_eq!(truncated.unwrap_err(), ReadError::Read);
    let mut malformed = body;
    malformed.pop();
    let invalid = local_fetch(
        with_length("200 OK", JSON_MEDIA_TYPE, malformed.len()),
        malformed,
    )
    .await;
    assert_eq!(invalid.unwrap_err(), ReadError::InvalidSnapshot);
}
