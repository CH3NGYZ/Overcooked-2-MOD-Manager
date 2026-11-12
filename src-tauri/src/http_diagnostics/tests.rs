use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Mutex, Once};

struct Logger(Mutex<Vec<(log::Level, String)>>);
static LOGGER: Logger = Logger(Mutex::new(Vec::new()));
impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        if record.target() == "app_lib::http_diagnostics" {
            self.0
                .lock()
                .unwrap()
                .push((record.level(), record.args().to_string()));
        }
    }
    fn flush(&self) {}
}

fn init_log() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        log::set_logger(&LOGGER).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
    });
}

pub(super) fn messages(url: &str) -> Vec<(log::Level, String)> {
    LOGGER
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, message)| message.contains(url))
        .cloned()
        .collect()
}

pub(super) fn client() -> reqwest::Client {
    init_log();
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap()
}

pub(super) fn fixture(
    status: u16,
    content_type: &str,
    body: Vec<u8>,
    declared_length: usize,
    delay: Duration,
) -> (String, std::thread::JoinHandle<()>) {
    serve_fixture(status, content_type, body, declared_length, delay, None)
}

fn serve_fixture(
    status: u16,
    content_type: &str,
    body: Vec<u8>,
    declared_length: usize,
    delay: Duration,
    release: Option<std::sync::mpsc::Receiver<()>>,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!(
        "http://{}/{}",
        listener.local_addr().unwrap(),
        uuid::Uuid::new_v4()
    );
    let content_type = content_type.to_owned();
    let worker = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut received = Vec::new();
        let end = loop {
            let mut bytes = [0; 1024];
            let size = socket.read(&mut bytes).unwrap();
            assert_ne!(size, 0);
            received.extend_from_slice(&bytes[..size]);
            assert!(received.len() < 128 * 1024);
            if let Some(end) = received.windows(4).position(|part| part == b"\r\n\r\n") {
                break end + 4;
            }
        };
        let length = String::from_utf8_lossy(&received[..end])
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|value| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
        while received.len() - end < length {
            let mut bytes = [0; 4096];
            let size = socket.read(&mut bytes).unwrap();
            assert_ne!(size, 0);
            received.extend_from_slice(&bytes[..size]);
        }
        write!(socket, "HTTP/1.1 {status} Fixture\r\nContent-Type: {content_type}\r\nContent-Length: {declared_length}\r\nX-Request-Id: fixture-request\r\nConnection: close\r\n\r\n").unwrap();
        socket.flush().unwrap();
        std::thread::sleep(delay);
        if let Some(release) = release {
            // Keep the body unavailable until the owner has checked the headers.
            let _ = release.recv_timeout(Duration::from_secs(5));
        }
        // Early drop by the owner is legal, especially for binary Range data.
        let _ = socket.write_all(&body);
    });
    (address, worker)
}

#[tokio::test]
async fn gitcode_400_logs_request_and_detailed_response_without_consuming_it() {
    let payload = serde_json::json!({"error_message":"Tag rejected", "errors":[{"field":"body","reason":"required"}],"code":400}).to_string();
    let (url, worker) = fixture(
        400,
        "application/json",
        payload.as_bytes().to_vec(),
        payload.len(),
        Duration::ZERO,
    );
    let mut response = client().post(&url).bearer_auth("fixture-credential-for-redaction")
        .json(&serde_json::json!({"tag_name":"v1", "body":"", "access_token":"fixture-credential-for-redaction"}))
        .send_logged().await.unwrap();
    assert_eq!(response.status().as_u16(), 400);
    assert_eq!(response.version(), reqwest::Version::HTTP_11);
    assert_eq!(response.url().as_str(), url);
    assert_eq!(response.headers()["x-request-id"], "fixture-request");
    assert_eq!(response.content_length(), Some(payload.len() as u64));
    assert!(response.remote_addr().is_some());
    response.extensions_mut().insert(123_u32);
    assert_eq!(response.extensions().get::<u32>(), Some(&123));
    assert_eq!(response.text().await.unwrap(), payload);
    worker.join().unwrap();
    let logs = messages(&url);
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].0, log::Level::Warn);
    let message = &logs[0].1;
    for expected in [
        "POST",
        "400",
        "tag_name",
        "v1",
        "Tag rejected",
        "errors",
        "required",
        "完整",
    ] {
        assert!(message.contains(expected), "missing {expected}");
    }
    assert!(!message.contains("fixture-credential-for-redaction"));
}

#[tokio::test]
async fn created_and_empty_success_keep_success_semantics_and_debug_level() {
    for (status, body) in [(201, b"{\"id\":42}".to_vec()), (204, Vec::new())] {
        let (url, worker) = fixture(
            status,
            "application/json",
            body.clone(),
            body.len(),
            Duration::ZERO,
        );
        let response = client()
            .post(&url)
            .send_logged()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        assert_eq!(response.bytes().await.unwrap().as_ref(), &body);
        worker.join().unwrap();
        let logs = messages(&url);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].0, log::Level::Debug);
    }
}

#[tokio::test]
async fn partial_binary_and_200_return_headers_without_waiting_for_or_consuming_body() {
    for status in [200, 206] {
        let body = vec![0x9a; MAX_BODY_BYTES * 3];
        let (release, held) = std::sync::mpsc::channel();
        let (url, worker) = serve_fixture(
            status,
            "application/octet-stream",
            body.clone(),
            body.len(),
            Duration::ZERO,
            Some(held),
        );
        let response = tokio::time::timeout(
            Duration::from_secs(2),
            client()
                .get(&url)
                .header("Range", "bytes=0-196607")
                .send_logged(),
        )
        .await
        .expect("headers must return while the body is withheld")
        .unwrap();
        assert!(
            body_preview(&response).is_none(),
            "file streams must not be prefetched"
        );
        assert_eq!(response.content_length(), Some(body.len() as u64));
        release.send(()).unwrap();
        assert_eq!(response.bytes().await.unwrap().as_ref(), &body);
        worker.join().unwrap();
        let logs = messages(&url);
        if status == 200 {
            assert!(logs.is_empty());
        } else {
            assert_eq!(logs.len(), 1);
            assert!(logs[0].1.contains("未预读"));
        }
    }
}

#[tokio::test]
async fn head_keeps_advertised_file_length_without_waiting_for_a_body() {
    let (url, worker) = fixture(403, "text/plain", Vec::new(), 800_000, Duration::ZERO);
    let response = client().head(&url).send_logged().await.unwrap();
    assert_eq!(response.headers()["content-length"], "800000");
    assert_eq!(response.content_length(), Some(0));
    assert!(response.bytes().await.unwrap().is_empty());
    worker.join().unwrap();
    assert!(messages(&url)[0].1.contains("无响应正文"));
}

#[tokio::test]
async fn oversized_error_is_bounded_in_logs_and_replayed_in_full() {
    let payload = format!("reason: {}", "错".repeat(MAX_BODY_BYTES));
    let (url, worker) = fixture(
        500,
        "text/plain",
        payload.as_bytes().to_vec(),
        payload.len(),
        Duration::ZERO,
    );
    let response = client().get(&url).send_logged().await.unwrap();
    assert_eq!(response.content_length(), Some(payload.len() as u64));
    assert_eq!(response.text().await.unwrap(), payload);
    worker.join().unwrap();
    let logs = messages(&url);
    assert_eq!(logs.len(), 1);
    assert!(logs[0].1.contains("64KiB"));
    assert!(logs[0].1.len() < MAX_BODY_BYTES * 4);
}

#[tokio::test]
async fn preview_timeout_keeps_the_original_pending_body_readable() {
    let payload = "late error reason";
    let (release, held) = std::sync::mpsc::channel();
    let (url, worker) = serve_fixture(
        500,
        "text/plain",
        payload.as_bytes().to_vec(),
        payload.len(),
        Duration::ZERO,
        Some(held),
    );
    let response = tokio::time::timeout(Duration::from_secs(2), client().get(&url).send_logged())
        .await
        .expect("preview must finish while the body is withheld")
        .unwrap();
    assert!(messages(&url)[0].1.contains("预览读取超时"));
    release.send(()).unwrap();
    assert_eq!(response.text().await.unwrap(), payload);
    worker.join().unwrap();
    assert!(messages(&url)[0].1.contains("预览读取超时"));
}

#[tokio::test]
async fn original_read_error_survives_prefetch_and_replay() {
    let (url, worker) = fixture(502, "text/plain", b"partial".to_vec(), 1000, Duration::ZERO);
    let response = client().get(&url).send_logged().await.unwrap();
    assert!(response.text().await.is_err());
    worker.join().unwrap();
    let logs = messages(&url);
    assert_eq!(logs.len(), 1);
    assert!(logs[0].1.contains("读取失败"));
    assert!(logs[0].1.contains("partial"));
}

#[tokio::test]
async fn html_error_is_logged_as_text_and_replayed() {
    let payload = "<html><body>upstream unavailable</body></html>";
    let (url, worker) = fixture(
        502,
        "text/html",
        payload.as_bytes().to_vec(),
        payload.len(),
        Duration::ZERO,
    );
    let response = client().get(&url).send_logged().await.unwrap();
    assert_eq!(response.text().await.unwrap(), payload);
    worker.join().unwrap();
    assert!(messages(&url)[0].1.contains("upstream unavailable"));
}

#[tokio::test]
async fn replay_preserves_the_original_total_timeout_for_late_consumers() {
    for (size, declared) in [
        (32, 32),
        (MAX_BODY_BYTES * 3, MAX_BODY_BYTES * 3),
        (7, 1000),
    ] {
        for logged in [false, true] {
            let payload = vec![b'x'; size];
            let (url, worker) = fixture(400, "text/plain", payload, declared, Duration::ZERO);
            let request = client().get(&url).timeout(Duration::from_millis(200));
            let response = if logged {
                request.send_logged().await.unwrap()
            } else {
                request.send().await.unwrap()
            };
            tokio::time::sleep(Duration::from_millis(250)).await;
            let result = response.bytes().await;
            assert!(
                result.is_err(),
                "late body read must time out: logged={logged}, size={size}"
            );
            assert!(
                result.unwrap_err().is_timeout(),
                "original timeout classification must survive replay"
            );
            worker.join().unwrap();
        }
    }
}

#[tokio::test]
async fn truncated_body_preserves_advertised_length_and_original_read_error() {
    for logged in [false, true] {
        let (url, worker) = fixture(400, "text/plain", b"partial".to_vec(), 1000, Duration::ZERO);
        let request = client().get(&url);
        let mut response = if logged {
            request.send_logged().await.unwrap()
        } else {
            request.send().await.unwrap()
        };
        assert_eq!(response.content_length(), Some(1000));
        assert_eq!(response.headers()["content-length"], "1000");
        assert_eq!(
            response.chunk().await.unwrap().unwrap(),
            b"partial".as_slice()
        );
        assert_eq!(response.content_length(), Some(993));
        let error = response.chunk().await.unwrap_err();
        // Response::chunk maps body failures to reqwest's decode category, in
        // both the raw and inspected paths. Preserve that public classification.
        assert!(error.is_decode());
        assert!(!error.is_timeout());
        worker.join().unwrap();
    }
}

#[tokio::test]
async fn small_preview_frames_are_coalesced_before_replaying_large_frames() {
    let request = reqwest::Request::new(
        reqwest::Method::GET,
        reqwest::Url::parse("http://localhost/preview-buffer").unwrap(),
    );
    let big_size = MAX_BODY_BYTES * 4;
    let data = (0..32)
        .map(|_| Bytes::from(vec![b'x'; MAX_BODY_BYTES / 32]))
        .chain((0..6).map(|_| Bytes::from(vec![b'y'; big_size])))
        .collect::<Vec<_>>();
    let inner = reqwest::Body::wrap_stream(futures_util::stream::iter(
        data.into_iter().map(Ok::<_, std::io::Error>),
    ));
    let mut body = PreviewBody {
        remaining: inner.size_hint(),
        inner: Box::pin(inner),
        replay: VecDeque::new(),
        buffered: BytesMut::new(),
        ended: false,
        finished: false,
        safe_preview: None,
        diagnostic: Some(Diagnostic {
            method: "GET".into(),
            address: "http://localhost/preview-buffer".into(),
            final_address: "http://localhost/preview-buffer".into(),
            status: StatusCode::BAD_REQUEST,
            content_type: "text/plain".into(),
            length: None,
            request_body: "".into(),
            redactor: Redactor::new(&request),
            sample: Vec::new(),
        }),
    };
    body.prefetch(BODY_PREVIEW_TIMEOUT).await;
    assert_eq!(
        body.replay.len(),
        1,
        "tiny frames must be coalesced before replay"
    );
    let mut received = 0;
    while let Some(frame) = std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await {
        let frame = frame.unwrap();
        if let Some(data) = frame.data_ref() {
            received += data.len();
        }
        let replay_bytes = body
            .replay
            .iter()
            .filter_map(|frame| frame.as_ref().ok()?.data_ref())
            .map(Bytes::len)
            .sum::<usize>();
        assert!(
            body.replay.len() <= 1,
            "replay must not accumulate later large frames"
        );
        assert!(
            replay_bytes + body.buffered.len() <= big_size,
            "retained replay must remain bounded by one large frame after each read"
        );
    }
    assert_eq!(received, MAX_BODY_BYTES + 6 * big_size);
    assert!(body.replay.is_empty());
    assert!(body.buffered.is_empty());
}
