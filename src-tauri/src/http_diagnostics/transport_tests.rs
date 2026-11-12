use super::tests::{client, fixture, messages};
use super::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

// A single blocking loopback connection; request and response I/O are bounded.
fn serve_once(
    respond: impl FnOnce(&mut TcpStream) + Send + 'static,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(false).unwrap();
    let url = format!(
        "http://{}/{}",
        listener.local_addr().unwrap(),
        uuid::Uuid::new_v4()
    );
    let worker = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut head = Vec::new();
        while !head.windows(4).any(|part| part == b"\r\n\r\n") {
            let mut bytes = [0; 1024];
            let size = socket.read(&mut bytes).unwrap();
            assert_ne!(size, 0, "request ended before its headers");
            head.extend_from_slice(&bytes[..size]);
            assert!(
                head.len() <= 32 * 1024,
                "fixture request headers are bounded"
            );
        }
        respond(&mut socket);
    });
    (url, worker)
}

#[tokio::test]
async fn redirect_to_400_retains_final_url_and_logs_only_safe_addresses() {
    let payload = b"redirected rejection: final-query-secret";
    let (final_base, final_worker) = fixture(
        400,
        "text/plain",
        payload.to_vec(),
        payload.len(),
        Duration::ZERO,
    );
    let final_url = format!("{final_base}?signature=final-query-secret#final-fragment-secret");
    let location = final_url.clone();
    let (initial_base, redirect_worker) = serve_once(move |socket| {
        write!(socket, "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        socket.flush().unwrap();
    });
    let initial_url =
        format!("{initial_base}?access_token=initial-query-secret#initial-fragment-secret");
    let response = client().get(&initial_url).send_logged().await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.url().path(),
        reqwest::Url::parse(&final_base).unwrap().path()
    );
    assert_eq!(response.url().query(), Some("signature=final-query-secret"));
    assert_eq!(response.version(), reqwest::Version::HTTP_11);
    assert_eq!(response.headers()["x-request-id"], "fixture-request");
    assert!(response.remote_addr().is_some());
    let preview = body_preview(&response).expect("text error has a safe preview");
    assert!(preview.contains("redirected rejection"));
    assert!(!preview.contains("final-query-secret"));
    assert_eq!(response.bytes().await.unwrap().as_ref(), payload);
    redirect_worker.join().unwrap();
    final_worker.join().unwrap();

    let logs = messages(&initial_base);
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].0, log::Level::Warn);
    assert!(logs[0].1.contains(&format!("地址={initial_base:?}")));
    assert!(logs[0].1.contains(&format!("最终地址={final_base:?}")));
    for secret in [
        "initial-query-secret",
        "initial-fragment-secret",
        "final-query-secret",
        "final-fragment-secret",
    ] {
        assert!(
            !logs[0].1.contains(secret),
            "URL credentials must not enter diagnostics"
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExtensionSentinel(&'static str);

#[tokio::test]
async fn inspect_preserves_extensions_inserted_before_response_rebuilding() {
    let payload = b"extension retention";
    let (url, worker) = fixture(
        400,
        "text/plain",
        payload.to_vec(),
        payload.len(),
        Duration::ZERO,
    );
    let http_client = client();
    let request = http_client.get(&url).build().unwrap();
    let redactor = Redactor::new(&request);
    let request_body = redactor.request_body(&request);
    let mut response = http_client.execute(request).await.unwrap();
    let remote = response.remote_addr();
    let headers = response.headers().clone();
    let version = response.version();
    response
        .extensions_mut()
        .insert(ExtensionSentinel("before-inspect"));

    let response = inspect(
        response,
        reqwest::Method::GET,
        url.clone(),
        request_body,
        redactor,
    )
    .await;
    assert_eq!(
        response.extensions().get::<ExtensionSentinel>(),
        Some(&ExtensionSentinel("before-inspect"))
    );
    assert!(body_preview(&response).is_some());
    assert_eq!(response.remote_addr(), remote);
    assert_eq!(response.headers(), &headers);
    assert_eq!(response.version(), version);
    assert_eq!(response.url().as_str(), url);
    assert_eq!(response.content_length(), Some(payload.len() as u64));
    assert_eq!(response.bytes().await.unwrap().as_ref(), payload);
    worker.join().unwrap();
}

#[tokio::test]
async fn chunked_response_keeps_unknown_length_and_data_before_trailers() {
    let (url, worker) = serve_once(|socket| {
        socket.write_all(
            b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\nTrailer: X-Checksum\r\nConnection: close\r\n\r\n5\r\nalpha\r\n4\r\nbeta\r\n0\r\nX-Checksum: alpha-beta\r\n\r\n",
        ).unwrap();
        socket.flush().unwrap();
    });
    let response = client()
        .get(&url)
        .header(reqwest::header::TE, "trailers")
        .send_logged()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.content_length(), None);
    assert_eq!(response.headers()["transfer-encoding"], "chunked");
    let raw: http::Response<reqwest::Body> = response.into();
    let mut body = raw.into_body();
    let mut data = Vec::new();
    let mut trailer_count = 0;
    assert_eq!(body.size_hint().exact(), None);
    while let Some(frame) = std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await {
        match frame.unwrap().into_data() {
            Ok(bytes) => {
                assert_eq!(trailer_count, 0, "data must precede the trailer frame");
                data.extend_from_slice(&bytes);
            }
            Err(frame) => {
                let trailers = frame.into_trailers().expect("unexpected frame kind");
                assert_eq!(data, b"alphabeta");
                assert_eq!(trailers["x-checksum"], "alpha-beta");
                trailer_count += 1;
            }
        }
        assert_eq!(
            body.size_hint().exact(),
            None,
            "preview must not invent Content-Length"
        );
    }
    assert_eq!(data, b"alphabeta");
    assert_eq!(
        trailer_count, 1,
        "the original trailer frame must survive preview and replay"
    );
    worker.join().unwrap();
    assert_eq!(messages(&url).len(), 1);
}

#[tokio::test]
async fn dropping_pending_preview_cancels_connection_and_emits_one_diagnostic() {
    let (closed_sender, closed_receiver) = tokio::sync::oneshot::channel();
    let (url, worker) = serve_once(move |socket| {
        socket.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n").unwrap();
        socket.flush().unwrap();
        let result = socket.read(&mut [0; 1]).map_err(|error| error.kind());
        let _ = closed_sender.send(result);
    });
    let http_client = client();
    let request = http_client.get(&url).build().unwrap();
    let redactor = Redactor::new(&request);
    let request_body = redactor.request_body(&request);
    let response = http_client.execute(request).await.unwrap();
    // Enter the real preview with headers received and the body still pending.
    let mut pending = Box::pin(inspect(
        response,
        reqwest::Method::GET,
        url.clone(),
        request_body,
        redactor,
    ));
    assert!(futures_util::poll!(pending.as_mut()).is_pending());
    assert!(messages(&url).is_empty());
    drop(pending);
    let closed = tokio::time::timeout(Duration::from_millis(750), closed_receiver)
        .await
        .expect("dropping preview must cancel without waiting for its one-second limit")
        .expect("fixture must observe peer closure");
    assert!(
        matches!(
            closed,
            Ok(0)
                | Err(std::io::ErrorKind::ConnectionReset)
                | Err(std::io::ErrorKind::ConnectionAborted)
        ),
        "unexpected peer state: {closed:?}"
    );
    worker.join().unwrap();
    let logs = messages(&url);
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].0, log::Level::Warn);
    assert!(logs[0].1.contains("请求已取消，预览未完成"));
}

#[tokio::test]
async fn refused_loopback_connection_keeps_error_classification_and_safe_log() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let safe_url = format!(
        "http://{}/{}",
        listener.local_addr().unwrap(),
        uuid::Uuid::new_v4()
    );
    drop(listener);
    let url = format!("{safe_url}?access_token=connect-query-secret");
    let error = client().get(&url).send_logged().await.unwrap_err();
    assert!(error.is_connect());
    assert!(!error.is_timeout());
    assert_eq!(error.url().unwrap().as_str(), url);
    let logs = messages(&safe_url);
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].0, log::Level::Warn);
    for expected in ["未收到响应", "GET", "超时=false", "连接错误=true"] {
        assert!(logs[0].1.contains(expected), "missing {expected}");
    }
    assert!(!logs[0].1.contains("connect-query-secret"));
}

#[tokio::test]
async fn timeout_before_response_headers_keeps_timeout_error_and_one_log() {
    let (safe_url, worker) = serve_once(|socket| {
        // Do not send headers. The client's original request deadline closes it.
        let _ = socket.read(&mut [0; 1]);
    });
    let url = format!("{safe_url}?access_token=timeout-query-secret");
    let error = client()
        .get(&url)
        .timeout(Duration::from_millis(150))
        .send_logged()
        .await
        .unwrap_err();
    assert!(error.is_timeout());
    assert!(!error.is_connect());
    assert_eq!(error.url().unwrap().as_str(), url);
    worker.join().unwrap();
    let logs = messages(&safe_url);
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].0, log::Level::Warn);
    for expected in ["未收到响应", "GET", "超时=true", "连接错误=false"] {
        assert!(logs[0].1.contains(expected), "missing {expected}");
    }
    assert!(!logs[0].1.contains("timeout-query-secret"));
}
