use super::*;

#[test]
fn callback_config_only_binds_literal_loopback_with_the_configured_path() {
    let (address, path) = callback_endpoint("http://127.0.0.1:55881/oauth/return").unwrap();
    assert_eq!(address.to_string(), "127.0.0.1:55881");
    assert_eq!(path, "/oauth/return");
    assert_eq!(
        callback_endpoint("http://[::1]:55882/return")
            .unwrap()
            .0
            .to_string(),
        "[::1]:55882"
    );
    for invalid in [
        "",
        "https://127.0.0.1:55881/return",
        "http://0.0.0.0:55881/return",
        "http://192.168.1.2:55881/return",
        "http://example.com/return",
        "http://localhost:55881/return",
        "http://127.0.0.1:0/return",
        "http://127.0.0.1:55881/",
        "http://u:p@127.0.0.1:55881/return",
        "http://127.0.0.1:55881/return?state=fixed",
        "http://127.0.0.1:55881/return#fragment",
    ] {
        assert!(callback_endpoint(invalid).is_none(), "{invalid}");
    }
}

#[test]
fn github_install_config_only_accepts_official_app_installation_entries() {
    assert!(valid_install_url(
        "https://github.com/apps/fixture-app/installations/new"
    ));
    for invalid in [
        "",
        "http://github.com/apps/fixture-app/installations/new",
        "https://example.com/apps/fixture-app/installations/new",
        "https://github.com/settings/apps/fixture-app",
        "https://u:p@github.com/apps/fixture-app/installations/new",
        "https://github.com/apps/fixture-app/installations/new?state=fixed",
    ] {
        assert!(!valid_install_url(invalid), "{invalid}");
    }
}

fn token_body(json: &str) -> TokenResponse {
    serde_json::from_str(json).unwrap()
}

#[test]
fn expiring_token_responses_become_absolute_deadlines() {
    let body = token_body(
        r#"{"access_token":"ghu_a","expires_in":28800,"refresh_token":"ghr_b",
                "refresh_token_expires_in":15897600,"scope":"","token_type":"bearer"}"#,
    );
    let tokens = tokens_from_response(&body, 1_000).unwrap();
    assert_eq!(tokens.access_token, "ghu_a");
    assert_eq!(tokens.refresh_token.as_deref(), Some("ghr_b"));
    assert_eq!(tokens.expires_at, Some(1_000 + 28_800));
    assert_eq!(tokens.refresh_expires_at, Some(1_000 + 15_897_600));
}

#[test]
fn non_expiring_token_responses_have_no_refresh_side() {
    // App 关了令牌过期：只有 access_token，不过期，也不需要刷新
    let tokens = tokens_from_response(&token_body(r#"{"access_token":"ghu_a"}"#), 1_000).unwrap();
    assert_eq!(tokens.refresh_token, None);
    assert_eq!(tokens.expires_at, None);
    assert_eq!(tokens.refresh_expires_at, None);
    assert!(tokens_from_response(&token_body(r#"{"access_token":"  "}"#), 1).is_none());
    assert!(tokens_from_response(&token_body(r#"{"error":"bad_refresh_token"}"#), 1).is_none());
}

#[test]
fn gitcode_expiry_accepts_the_documented_string_or_integer() {
    for value in [r#""3600""#, "3600"] {
        let body = token_body(&format!(
            r#"{{"access_token":"test","expires_in":{value}}}"#
        ));
        assert_eq!(
            tokens_from_response(&body, 100).unwrap().expires_at,
            Some(3700)
        );
    }
    assert!(serde_json::from_str::<TokenResponse>(r#"{"expires_in":"invalid"}"#).is_err());
}

#[tokio::test]
async fn gitcode_token_exchange_sends_secret_only_in_multipart_and_refresh_only_as_documented() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let mut captured = Vec::new();
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            socket
                .set_write_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut data = Vec::new();
            loop {
                let mut buffer = [0; 4096];
                let count = socket.read(&mut buffer).unwrap();
                assert_ne!(count, 0);
                data.extend_from_slice(&buffer[..count]);
                if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&data[..at]);
                    let length = header
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|n| n.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if data.len() >= at + 4 + length {
                        break;
                    }
                }
            }
            captured.push(String::from_utf8(data).unwrap());
            let body = r#"{"access_token":"fixture-token","expires_in":"3600"}"#;
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
        captured
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap();
    for refresh in [None, Some("fixture-refresh")] {
        let mut request = token_request(
            &client,
            PublishProvider::Gitcode,
            "fixture-secret",
            Some("fixture-code"),
            None,
            refresh,
        )
        .unwrap()
        .build()
        .unwrap();
        let original = request.url().clone();
        assert_eq!(original.host_str(), Some("gitcode.com"));
        assert!(!original.as_str().contains("fixture-secret"));
        let mut local = reqwest::Url::parse(&endpoint).unwrap();
        local.set_path(original.path());
        local.set_query(original.query());
        *request.url_mut() = local;
        let body: TokenResponse = client.execute(request).await.unwrap().json().await.unwrap();
        assert_eq!(
            tokens_from_response(&body, 10).unwrap().expires_at,
            Some(3610)
        );
    }
    let captured = worker.join().unwrap();
    let (headers, body) = captured[0].split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with(
        "POST /oauth/token?grant_type=authorization_code&code=fixture-code&client_id="
    ));
    assert!(!headers.contains("fixture-secret"));
    assert!(headers.contains("multipart/form-data"));
    assert!(body.contains("name=\"client_secret\"\r\n\r\nfixture-secret"));
    assert!(!captured[0].to_ascii_lowercase().contains("authorization:"));
    assert!(
        captured[1].starts_with(
            "POST /oauth/token?grant_type=refresh_token&refresh_token=fixture-refresh "
        )
    );
    assert!(!captured[1].contains("fixture-secret"));
}
