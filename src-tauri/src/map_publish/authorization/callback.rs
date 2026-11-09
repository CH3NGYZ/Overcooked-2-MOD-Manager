use super::AUTHORIZE_TIMEOUT;
use super::CallbackOutcome;
use super::release_pending_auth;
use crate::locale_tag::with_params;
use std::collections::HashMap;
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use super::super::credentials::{CALLBACK_URL, callback_endpoint};

/// 上一轮授权还没结束就再点一次时，端口还在旧监听手里。监听线程每 120 ms 看一次
/// 取消标志，所以重试两秒足够；直接 bind 失败会让用户看到「端口被占用」而浏览器
/// 根本没打开，只能干等到上一轮超时。
const BIND_RETRIES: usize = 25;

const BIND_RETRY_DELAY: Duration = Duration::from_millis(80);

#[cfg(test)]
#[test]
fn configured_callback_path_is_used_by_the_real_http_handler() {
    for (target, status, completed) in [
        ("/oauth/return?state=fixture&code=example", "200 OK", true),
        (
            "/callback?state=fixture&code=example",
            "404 Not Found",
            false,
        ),
        (
            "/oauth/return?state=other&code=example",
            "400 Bad Request",
            true,
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let outcome = Arc::new(Mutex::new(CallbackOutcome::Pending));
        let result = Arc::clone(&outcome);
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_nonblocking(false).unwrap();
            handle_callback(stream, "fixture", &result, "/oauth/return")
        });
        let mut client = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        client
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        client
            .write_all(format!("GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
            .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        assert!(
            response.starts_with(&format!("HTTP/1.1 {status}")),
            "{response}"
        );
        assert_eq!(server.join().unwrap(), completed);
        let result = outcome.lock().unwrap();
        if status == "200 OK" {
            assert!(matches!(&*result, CallbackOutcome::Code { code, .. } if code == "example"));
        } else if !completed {
            assert!(matches!(&*result, CallbackOutcome::Pending));
        } else {
            assert!(
                matches!(&*result, CallbackOutcome::Failed(tag) if tag == "mapPublish.stateMismatch")
            );
        }
    }
}

/// 占住回调端口：上一轮授权没结束时先取消，再等它松手。
pub(super) fn bind_callback_listener() -> Result<TcpListener, String> {
    let (address, _) = callback_endpoint(CALLBACK_URL)
        .ok_or_else(|| "mapPublish.callbackUnavailable".to_string())?;
    let mut last_error = None;
    for attempt in 0..BIND_RETRIES {
        if attempt > 0 {
            std::thread::sleep(BIND_RETRY_DELAY);
        }
        release_pending_auth();
        match TcpListener::bind(address) {
            Ok(listener) => return Ok(listener),
            Err(error) => last_error = Some(error),
        }
    }
    let error = last_error.expect("至少尝试过一次 bind");
    log::warn!("本机授权回调端口被占用：{error}");
    Err(with_params(
        "mapPublish.callbackUnavailable",
        &[("reason", &error.to_string())],
    ))
}

fn finish_callback(outcome: &Mutex<CallbackOutcome>, result: CallbackOutcome) {
    if let Ok(mut guard) = outcome.lock() {
        *guard = result;
    }
}

/// 后台等一次回调。用 std 线程而不是 tokio：这里只做「accept 一次 + 读一行请求」，
/// 不需要 runtime，也不需要把 `TcpListener` 塞进 async 状态。
pub(super) fn spawn_callback_listener(
    listener: TcpListener,
    expected_state: String,
    cancel: Arc<AtomicBool>,
    outcome: Arc<Mutex<CallbackOutcome>>,
) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + AUTHORIZE_TIMEOUT;
        let _ = listener.set_nonblocking(true);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            if Instant::now() >= deadline {
                finish_callback(
                    &outcome,
                    CallbackOutcome::Failed("mapPublish.authorizationExpired".to_string()),
                );
                return;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    let Some((_, path)) = callback_endpoint(CALLBACK_URL) else {
                        return;
                    };
                    if handle_callback(stream, &expected_state, &outcome, &path) {
                        return;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(120));
                }
                Err(error) => {
                    log::warn!("本机授权回调监听中断：{error}");
                    finish_callback(
                        &outcome,
                        CallbackOutcome::Failed(with_params(
                            "mapPublish.callbackUnavailable",
                            &[("reason", &error.to_string())],
                        )),
                    );
                    return;
                }
            }
        }
    });
}

/// 读一条 HTTP 请求，认路径与 `state`，把结果写回 `outcome`。
///
/// 返回 true 表示这次授权该收尾了（成功、拒绝、state 不符都算）。
fn handle_callback(
    mut stream: TcpStream,
    expected_state: &str,
    outcome: &Mutex<CallbackOutcome>,
    callback_path: &str,
) -> bool {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = [0u8; 8192];
    let read = match stream.read(&mut buffer) {
        Ok(read) if read > 0 => read,
        _ => return false,
    };
    let request = String::from_utf8_lossy(&buffer[..read]);
    let Some(target) = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
    else {
        return false;
    };
    let Some((path, query)) = target.split_once('?') else {
        let _ = write_callback_page(&mut stream, "400 Bad Request", "这不是授权回调地址。");
        return false;
    };
    if path != callback_path {
        let _ = write_callback_page(&mut stream, "404 Not Found", "这不是授权回调地址。");
        return false;
    }
    let params = parse_query(query);
    if params.get("state").map(String::as_str) != Some(expected_state) {
        let _ = write_callback_page(
            &mut stream,
            "400 Bad Request",
            "state 校验失败，请回到管理器重新发起授权。",
        );
        finish_callback(
            outcome,
            CallbackOutcome::Failed("mapPublish.stateMismatch".to_string()),
        );
        return true;
    }
    if let Some(error) = params.get("error") {
        let reason = params
            .get("error_description")
            .map(String::as_str)
            .unwrap_or(error);
        let denied = error == "access_denied";
        let _ = write_callback_page(
            &mut stream,
            "200 OK",
            if denied {
                "已取消授权。"
            } else {
                "授权没有完成，请回到管理器重试。"
            },
        );
        log::warn!("GitHub 授权回调报错：{error}（{reason}）");
        let result = if denied {
            CallbackOutcome::Failed("mapPublish.authorizationDenied".to_string())
        } else {
            CallbackOutcome::Failed(with_params(
                "mapPublish.authorizationFailed",
                &[("reason", reason)],
            ))
        };
        finish_callback(outcome, result);
        return true;
    }
    // 安装页那条路会把刚装好的 installation id 一起带回来，它就是「用户勾了
    // 哪些仓库」的答案；纯 OAuth 授权没有这个参数。
    let installation = params
        .get("installation_id")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|id| *id > 0);
    match params.get("code").filter(|code| !code.is_empty()) {
        Some(code) => {
            let _ = write_callback_page(
                &mut stream,
                "200 OK",
                "授权完成，可以关闭此页面，回到 OC2 MOD Manager。",
            );
            finish_callback(
                outcome,
                CallbackOutcome::Code {
                    code: code.to_string(),
                    installation,
                },
            );
            true
        }
        None => {
            // 装上了却没有授权码：App 设置里没勾「Request user authorization
            // (OAuth) during installation」，那条路根本不会回传 code。
            let installed_only = installation.is_some();
            let _ = write_callback_page(
                &mut stream,
                "400 Bad Request",
                if installed_only {
                    "已安装 App，但没有拿到授权码：请回到管理器按提示检查 App 设置。"
                } else {
                    "回调里没有授权码，请回到管理器重试。"
                },
            );
            finish_callback(
                outcome,
                CallbackOutcome::Failed(if installed_only {
                    "mapPublish.installNotConfigured".to_string()
                } else {
                    with_params(
                        "mapPublish.authorizationFailed",
                        &[("reason", "missing code")],
                    )
                }),
            );
            true
        }
    }
}

fn write_callback_page(stream: &mut TcpStream, status: &str, message: &str) -> std::io::Result<()> {
    let body = format!(
        "<!doctype html>\r\n<html lang=\"zh-CN\"><head><meta charset=\"utf-8\">\
<title>OC2 地图上传</title></head>\r\n\
<body style=\"font-family:system-ui,-apple-system,'Segoe UI',sans-serif;padding:48px;\
line-height:1.7;color:#1f2328\">\r\n\
<h1 style=\"font-size:20px\">{message}</h1>\r\n\
<p style=\"color:#57606a\">OC2 MOD Manager · 地图上传</p>\r\n</body></html>\r\n"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((percent_decode(key), percent_decode(value)))
        })
        .collect()
}

/// 回调里的 `code` / `state` 都是 URL 编码过的；这里只需要解 `%XX` 与 `+`。
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let high = (bytes[index + 1] as char).to_digit(16);
                let low = (bytes[index + 2] as char).to_digit(16);
                match (high, low) {
                    (Some(high), Some(low)) => {
                        output.push((high * 16 + low) as u8);
                        index += 3;
                    }
                    _ => {
                        output.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}
