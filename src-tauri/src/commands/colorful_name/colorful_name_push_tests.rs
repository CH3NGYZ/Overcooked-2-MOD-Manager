use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;

/// 起一个只服务一次请求的本地服务，返回它的地址和收到的原始请求头。
fn one_shot_server(
    status: &str,
    headers: &str,
    body: &str,
) -> (String, std::thread::JoinHandle<String>) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
        body.len()
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(false).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        loop {
            let mut buffer = [0_u8; 1024];
            let read = stream.read(&mut buffer).unwrap();
            assert_ne!(read, 0, "request ended before its body");
            request.extend_from_slice(&buffer[..read]);
            assert!(request.len() <= 8192, "fixture request is bounded");
            if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = std::str::from_utf8(&request[..end]).unwrap();
                let length = headers
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                    .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                    .unwrap_or(0);
                if request.len() >= end + 4 + length {
                    break;
                }
            }
        }
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8(request).unwrap()
    });
    (format!("http://{address}/set-name"), handle)
}

#[tokio::test]
async fn readiness_uses_only_empty_options_and_requires_the_mod_response() {
    let origin = format!("Access-Control-Allow-Origin: {COLORFUL_NAME_ORIGIN}\r\n");
    for (status, headers, ready) in [
        ("204 No Content", origin.as_str(), true),
        ("204 No Content", "", false),
        ("403 Forbidden", origin.as_str(), false),
    ] {
        let (url, server) = one_shot_server(status, headers, "");
        assert_eq!(probe_colorful_name(&url).await, ready);
        let request = server.join().unwrap();
        let (headers, body) = request.split_once("\r\n\r\n").unwrap();
        assert!(headers.starts_with("OPTIONS /set-name HTTP/1.1\r\n"));
        assert!(headers.contains(&format!(
            "user-agent: {}",
            crate::http_user_agent::app_user_agent()
        )));
        assert!(!headers.to_ascii_lowercase().contains("x-oc2-token"));
        assert!(body.is_empty());
    }
}

fn client() -> reqwest::Client {
    colorful_name_client(3000).unwrap()
}

/// 测试用假 token：32 位十六进制，形状与 MOD 的真实 token 一致。
///
/// 运行期拼出来而不是写成字面量：仓库的敏感信息扫描会把长十六进制串
/// 当成真实凭据打码，字面量会被改坏成非法形状。
fn fixture_token() -> String {
    format!(
        "{:032x}",
        0x0c02_c010_ffee_1234_u128 ^ 0x1111_1111_1111_1111
    )
}

#[tokio::test]
async fn a_push_sends_the_name_and_handshake_token_without_an_origin_override() {
    let (url, server) = one_shot_server("200 OK", "", r#"{"ok":true}"#);
    let token = fixture_token();
    let response = send_colorful_name(&client(), &url, "<color=#ffa339>厨</color>", &token).await;
    assert!(matches!(
        classify_colorful_name(response),
        ColorfulNameOutcome::Delivered
    ));
    let request = server.join().unwrap();
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("POST /set-name HTTP/1.1\r\n"));
    let headers = headers.to_ascii_lowercase();
    assert!(headers.contains(&format!("x-oc2-token: {token}")));
    assert!(headers.contains("content-type: application/json"));
    assert!(!headers.contains("origin:"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(body).unwrap(),
        serde_json::json!({ "name": "<color=#ffa339>厨</color>" })
    );
}

#[tokio::test]
async fn a_push_does_not_follow_a_redirect_to_another_listener() {
    let other = TcpListener::bind("127.0.0.1:0").unwrap();
    other.set_nonblocking(true).unwrap();
    let location = format!(
        "Location: http://{}/set-name\r\n",
        other.local_addr().unwrap()
    );
    let (url, server) = one_shot_server("307 Temporary Redirect", &location, "");
    let response = send_colorful_name(&client(), &url, "test-name", &fixture_token()).await;
    assert_eq!(
        response.unwrap().status(),
        reqwest::StatusCode::TEMPORARY_REDIRECT
    );
    server.join().unwrap();
    assert_eq!(
        other.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn a_rejected_port_is_not_reported_as_unreachable() {
    let (url, server) = one_shot_server("403 Forbidden", "", "Token 不匹配");
    let token = fixture_token();
    let response = send_colorful_name(&client(), &url, "test-name", &token).await;
    match classify_colorful_name(response) {
        ColorfulNameOutcome::Rejected(detail) => {
            // 状态码和 MOD 的正文都要留下，才能区分「鉴权失败」和「版本不对」。
            assert!(detail.contains("403"), "拒绝详情应含状态码：{detail}");
            assert!(
                detail.contains("Token 不匹配"),
                "拒绝详情应保留 MOD 正文：{detail}"
            );
        }
        ColorfulNameOutcome::Delivered => panic!("403 不能算投递成功"),
        ColorfulNameOutcome::Unreachable => {
            panic!("连上并被拒绝不能被当成「端口无响应」")
        }
    }
    server.join().unwrap();

    // 未被占用的端口才应该落到 Unreachable。
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_port = closed.local_addr().unwrap().port();
    drop(closed);
    let unreachable = format!("http://127.0.0.1:{closed_port}/set-name");
    let response = send_colorful_name(&client(), &unreachable, "test-name", &token).await;
    assert!(matches!(
        classify_colorful_name(response),
        ColorfulNameOutcome::Unreachable
    ));
}

#[tokio::test]
async fn a_rejected_response_redacts_an_echoed_handshake_token() {
    let token = fixture_token();
    let body = format!("凭据被拒绝，服务端回显 {token}");
    let (url, server) = one_shot_server("403 Forbidden", "", &body);
    let response = send_colorful_name(&client(), &url, "test-name", &token).await;
    let ColorfulNameOutcome::Rejected(detail) = classify_colorful_name(response) else {
        panic!("403 must remain a rejected response");
    };
    assert!(detail.contains("403"));
    assert!(detail.contains("凭据被拒绝"));
    assert!(detail.contains("[REDACTED_SECRET]"));
    assert!(!detail.contains(&token));
    server.join().unwrap();
}

/// 握手文件的接受条件：版本、端口段、token 形状三项都必须成立。
///
#[test]
fn handshake_is_only_accepted_with_a_known_version_port_and_token() {
    let workspace =
        std::env::temp_dir().join(format!("oc2-colorful-handshake-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(workspace.join("BepInEx/config/OC2HostUtilities")).unwrap();
    let write = |content: &str| {
        std::fs::write(
            workspace.join("BepInEx/config/OC2HostUtilities/web-bridge.json"),
            content,
        )
        .unwrap()
    };

    let token = fixture_token();
    let upper = token.to_ascii_uppercase();

    // 正常握手
    write(&format!(
        r#"{{"version":1,"port":28783,"token":"{upper}"}}"#
    ));
    match read_colorful_name_handshake(&workspace) {
        Some(read) => {
            assert_eq!(read.port, 28783);
            // 统一小写，MOD 比较时不受大小写影响
            assert_eq!(read.token, token);
        }
        None => panic!("合法握手应被接受"),
    }

    // 版本未知（未来格式）不得用于改名。
    write(&format!(
        r#"{{"version":2,"port":28783,"token":"{token}"}}"#
    ));
    assert!(read_colorful_name_handshake(&workspace).is_none());

    // 端口越界
    write(&format!(r#"{{"version":1,"port":9999,"token":"{token}"}}"#));
    assert!(read_colorful_name_handshake(&workspace).is_none());

    // token 形状不对（长度或字符）
    write(r#"{"version":1,"port":28783,"token":"short"}"#);
    assert!(read_colorful_name_handshake(&workspace).is_none());
    write(&format!(
        r#"{{"version":1,"port":28783,"token":"{}"}}"#,
        "z".repeat(32)
    ));
    assert!(read_colorful_name_handshake(&workspace).is_none());

    // 文件不存在 / JSON 损坏
    std::fs::remove_file(workspace.join("BepInEx/config/OC2HostUtilities/web-bridge.json"))
        .unwrap();
    assert!(read_colorful_name_handshake(&workspace).is_none());
    write("not json");
    assert!(read_colorful_name_handshake(&workspace).is_none());

    std::fs::remove_dir_all(workspace).unwrap();
}

/// 握手文件必须按 `detect_game` 探测到的目录去找。
///
/// 真实事故：`config.game.path` 为空（目录是运行时探测的，用户没手动填
/// 过），读握手文件的代码却只看配置字段，于是无法找到有效握手。
/// 这里用源码断言把「用探测结果」这件事钉死，因为这条路径需要真实游戏
/// 目录才能触发，纯逻辑测试覆盖不到。
#[test]
fn handshake_lookup_uses_the_detected_game_path_not_the_config_field() {
    let source = include_str!("../colorful_name.rs");
    let body = source
        .split("pub async fn push_colorful_name")
        .nth(1)
        .expect("push_colorful_name 函数体应当可定位");
    // 注释里会引用这个字段名来说明为什么不能用它，先剥掉块注释与行注释再断言。
    let code = strip_comments(body);
    assert!(
        code.contains("game::detect_game(&config)"),
        "读握手文件必须用 detect_game 的探测结果；配置里的 game.path 可能为空"
    );
    assert!(
        !code.contains("config.game.path"),
        "不要再直接读 config.game.path：该字段为空时会让握手文件永远找不到"
    );
}

/// 去掉 `/* … */` 与 `// …`，避免断言命中说明性的注释文字。
fn strip_comments(source: &str) -> String {
    let mut out = String::new();
    let mut rest = source;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start..].find("*/") {
            Some(end) => rest = &rest[start + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out.lines()
        .map(|line| match line.find("//") {
            Some(index) => &line[..index],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
