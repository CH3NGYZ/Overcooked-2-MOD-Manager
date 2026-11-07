use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;

/// 起一个只服务一次请求的本地服务，返回它的地址和收到的原始请求头。
fn one_shot_server(response: &'static str) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut buffer = [0_u8; 8192];
        let read = stream.read(&mut buffer).unwrap_or(0);
        let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
        let _ = stream.write_all(response.as_bytes());
        request
    });
    (format!("http://{address}/set-name"), handle)
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(3000))
        .build()
        .unwrap()
}

#[tokio::test]
async fn sends_the_official_origin_header_the_mod_accepts() {
    let (url, server) = one_shot_server(
        "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
    );
    let response = send_colorful_name(&client(), &url, "test-name", None)
        .await
        .unwrap();
    assert!(response.status().is_success());
    let request = server.join().unwrap();
    let lower = request.to_ascii_lowercase();
    // MOD 逐字节比较 Origin（忽略大小写），值必须与常量完全一致。
    assert!(
        request.contains(&format!("Origin: {COLORFUL_NAME_ORIGIN}"))
            || lower.contains(&format!(
                "origin: {}",
                COLORFUL_NAME_ORIGIN.to_ascii_lowercase()
            )),
        "请求必须带官方 Origin 头，实际请求：{request}"
    );
    assert!(
        lower.contains("content-type: application/json"),
        "请求必须声明 JSON，MOD 只有在 JSON 内容类型下才解析 name 字段：{request}"
    );
    assert!(
        request.contains("\"name\""),
        "正文必须含 name 字段：{request}"
    );
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
async fn a_handshake_token_replaces_the_origin_header() {
    let (url, server) = one_shot_server(
        "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
    );
    let token = fixture_token();
    let response = send_colorful_name(&client(), &url, "test-name", Some(&token))
        .await
        .unwrap();
    assert!(response.status().is_success());
    let request = server.join().unwrap();
    let lower = request.to_ascii_lowercase();
    assert!(
        lower.contains(&format!("x-oc2-token: {token}")),
        "有 token 时必须带 X-OC2-Token：{request}"
    );
    // token 是强凭据，不需要再冒充网页来源。
    assert!(
        !lower.contains("origin:"),
        "token 通道不应再带 Origin：{request}"
    );
}

#[tokio::test]
async fn a_rejected_port_is_not_reported_as_unreachable() {
    // MOD 在缺 token / Origin 不匹配时的真实响应。
    let (url, server) = one_shot_server(
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 13\r\nConnection: close\r\n\r\nToken 不匹配",
    );
    let response = send_colorful_name(&client(), &url, "test-name", None).await;
    match classify_colorful_name(28780, response).await {
        ColorfulNameOutcome::Rejected(port, detail) => {
            assert_eq!(port, 28780);
            // 状态码和 MOD 的正文都要留下，才能区分「鉴权失败」和「版本不对」。
            assert!(detail.contains("403"), "拒绝详情应含状态码：{detail}");
            assert!(
                detail.contains("Token"),
                "拒绝详情应保留 MOD 正文：{detail}"
            );
        }
        ColorfulNameOutcome::Delivered(_) => panic!("403 不能算投递成功"),
        ColorfulNameOutcome::Unreachable(_) => {
            panic!("连上并被拒绝不能被当成「端口无响应」")
        }
    }
    server.join().unwrap();

    // 未被占用的端口才应该落到 Unreachable。
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_port = closed.local_addr().unwrap().port();
    drop(closed);
    let unreachable = format!("http://127.0.0.1:{closed_port}/set-name");
    let response = send_colorful_name(&client(), &unreachable, "test-name", None).await;
    assert!(matches!(
        classify_colorful_name(closed_port, response).await,
        ColorfulNameOutcome::Unreachable(port) if port == closed_port
    ));
}

/// 握手文件的接受条件：版本、端口段、token 形状三项都必须成立。
///
/// 这个文件位于游戏目录，任何能写游戏目录的程序都能伪造；校验的目的
/// 是「不把畸形或过期内容当凭据」，随即退回官方 Origin，而不是让推送
/// 直接失败。
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

    // 版本未知（未来格式）：退回 Origin，而不是猜着用
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
/// 过），读握手文件的代码却只看配置字段，于是判定「没配目录」直接走
/// Origin 兜底；而当前街机MOD 只认 `X-OC2-Token`，用户拿到的是 403。
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
