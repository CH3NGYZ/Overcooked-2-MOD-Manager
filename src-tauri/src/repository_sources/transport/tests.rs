use super::*;
use crate::releases;

fn serve_fresh_responses(
    path: &'static str,
    responses: Vec<(&'static str, String)>,
) -> (String, std::thread::JoinHandle<()>) {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        for (status, body) in responses {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "request did not arrive"
                        );
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            // Accepted sockets inherit the listener's mode on Windows.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                request.push_str(&line);
            }
            assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
            let headers = request.to_ascii_lowercase();
            assert!(headers.contains("cache-control: no-cache, no-store, max-age=0\r\n"));
            assert!(headers.contains("pragma: no-cache\r\n"));
            assert!(!headers.contains("if-none-match:"));
            assert!(!headers.contains("if-modified-since:"));
            write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nCache-Control: public, max-age=86400\r\nETag: old-etag\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (format!("http://{address}{path}"), server)
}

#[tokio::test]
async fn remote_source_reloads_changed_content_and_rejects_not_modified() {
    let document = |name| {
        format!(
            "[[repositories]]\nname = \"{name}\"\nname_i18n.zh-CN = \"{name}\"\ngroup_i18n.zh-CN = \"\"\nnote_i18n.zh-CN = \"\"\nprovider = \"github\"\nusername = \"example\"\nrepository = \"mod\"\ninstall_target = \"BepInEx/plugins\"\nenabled = true\nasset_rules = [{{ type = \"glob\", value = \"*.dll\" }}]\n"
        )
    };
    let (url, server) = serve_fresh_responses(
        "/source.toml",
        vec![
            ("200 OK", document("Old")),
            ("200 OK", document("New")),
            ("304 Not Modified", String::new()),
        ],
    );
    let location = SourceLocation::parse(&url).unwrap();
    assert_eq!(
        load_document(&location).await.unwrap().repositories[0].name,
        "Old"
    );
    assert_eq!(
        load_document(&location).await.unwrap().repositories[0].name,
        "New"
    );
    assert!(load_document(&location).await.is_err());
    server.join().unwrap();
}

/*
 * 源文档超时固定 30 秒，且**不受** `network.timeout_seconds` 影响。
 *
 * 用户把网络超时调到 5 秒是为了让下载更快失败，但索引和各级子源
 * 是串行递归的：一层被压到 5 秒，后面所有子源都没机会加载 ——
 * 表现为「源刷新随机失败、仓库列表时有时无」。
 *
 * 这里用「延迟 1.5 秒才响应」的本地服务验证读得完；如果超时仍走
 * 用户配置且被调到很小，这条会失败。
 */
#[tokio::test]
async fn source_document_timeout_is_independent_of_user_network_config() {
    assert_eq!(SOURCE_DOCUMENT_TIMEOUT, 30);

    let body = "[[repositories]]\nname_i18n.zh-CN = \"Slow\"\ngroup_i18n.zh-CN = \"\"\nnote_i18n.zh-CN = \"\"\nprovider = \"github\"\nusername = \"example\"\nrepository = \"mod\"\ninstall_target = \"BepInEx/plugins\"\nenabled = true\nasset_rules = [{ type = \"glob\", value = \"*.dll\" }]\n";
    let (url, server) = serve_delayed_response("/slow.toml", body, 1500);
    let location = SourceLocation::parse(&url).unwrap();
    let document = load_document(&location)
        .await
        .expect("1.5 秒延迟应在 30 秒超时内成功");
    assert_eq!(document.repositories[0].name, "Slow");
    server.join().unwrap();
}

/// 接受一次连接、睡眠 `delay_ms` 后再回 200。
fn serve_delayed_response(
    path: &'static str,
    body: &'static str,
    delay_ms: u64,
) -> (String, std::thread::JoinHandle<()>) {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "request did not arrive"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) => panic!("{error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            request.push_str(&line);
        }
        assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    (format!("http://{address}{path}"), server)
}

#[tokio::test]
async fn release_api_reloads_changed_content_and_rejects_not_modified() {
    let document = |tag| {
        serde_json::json!([{
            "tag_name": tag, "name": tag, "body": "", "prerelease": false,
            "draft": false, "assets": []
        }])
        .to_string()
    };
    let (url, server) = serve_fresh_responses(
        "/releases",
        vec![
            ("200 OK", document("v1.0.0")),
            ("200 OK", document("v2.0.0")),
            ("304 Not Modified", String::new()),
        ],
    );
    let client = releases::build_client(5).unwrap();
    for tag in ["v1.0.0", "v2.0.0"] {
        assert_eq!(
            releases::fetch_release_api(&client, &url, &crate::models::Provider::Github, None)
                .await
                .unwrap()[0]
                .tag_name,
            tag
        );
    }
    assert!(
        releases::fetch_release_api(&client, &url, &crate::models::Provider::Github, None)
            .await
            .is_err()
    );
    server.join().unwrap();
}

#[tokio::test]
async fn remote_source_resolves_children_from_its_redirect_destination() {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut count = 0;
        while count < 2 && std::time::Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            // Windows accepted sockets inherit the listener's nonblocking mode.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            let response = if count == 0 {
                assert!(request.starts_with("GET /start.toml "));
                "HTTP/1.1 302 Found\r\nLocation: /nested/index.toml\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
            } else {
                assert!(request.starts_with("GET /nested/index.toml "));
                let body = "[[repository_sources]]\nid = \"mods\"\nnickname = \"Mods\"\nurl = \"../mods.toml\"\n";
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            stream.write_all(response.as_bytes()).unwrap();
            count += 1;
        }
        assert_eq!(count, 2);
    });
    let location = SourceLocation::parse(&format!("http://{address}/start.toml")).unwrap();
    let result = load_document(&location).await;
    server.join().unwrap();
    assert_eq!(
        result.unwrap().repository_sources[0].url,
        format!("http://{address}/mods.toml")
    );
}
