use super::*;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

fn delayed_server(
    headers_after: Duration,
    body_after: Duration,
    split_body: bool,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/asset", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "fixture accept timed out");
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) => panic!("{error}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let mut buffer = [0; 1024];
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0 && request.len() + count <= 8192);
            request.extend_from_slice(&buffer[..count]);
        }
        std::thread::sleep(headers_after);
        if socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n")
            .is_err()
        {
            return;
        }
        if split_body && socket.write_all(b"o").is_err() {
            return;
        }
        std::thread::sleep(body_after);
        let _ = socket.write_all(if split_body { b"k" } else { b"ok" });
    });
    (url, worker)
}

#[tokio::test]
async fn headers_and_first_payload_share_one_deadline_and_cleanup() {
    let (url, worker) = delayed_server(
        Duration::from_millis(250),
        Duration::from_millis(250),
        false,
    );
    let root = std::env::temp_dir().join(format!("first-byte-{}", Uuid::new_v4()));
    let result = download_to_temp_with_threads_and_first_byte_timeout(
        &crate::releases::build_download_client(Duration::from_millis(400)).unwrap(),
        &url,
        &root,
        "fixture",
        &DownloadReporter::new("fixture"),
        1,
        Some(Duration::from_millis(400)),
    )
    .await;
    worker.join().unwrap();
    assert_eq!(result.unwrap_err(), FIRST_BYTE_TIMEOUT_ERROR);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    fs::remove_dir(root).unwrap();
}

#[tokio::test]
async fn configured_start_deadline_does_not_limit_later_body_transfer() {
    let (url, worker) = delayed_server(Duration::ZERO, Duration::from_millis(500), true);
    let root = std::env::temp_dir().join(format!("first-byte-{}", Uuid::new_v4()));
    let path = download_to_temp_with_threads_and_first_byte_timeout(
        &crate::releases::build_download_client(Duration::from_millis(200)).unwrap(),
        &url,
        &root,
        "fixture",
        &DownloadReporter::new("fixture"),
        1,
        Some(Duration::from_millis(200)),
    )
    .await
    .unwrap();
    worker.join().unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"ok");
    fs::remove_file(path).unwrap();
    fs::remove_dir(root).unwrap();
}

#[tokio::test]
async fn empty_chunks_do_not_complete_first_payload_wait() {
    let mut body = futures_util::stream::iter([
        Ok(bytes::Bytes::new()),
        Ok(bytes::Bytes::new()),
        Ok(bytes::Bytes::from_static(b"payload")),
    ]);
    let bytes = first_nonempty_chunk(
        &mut body,
        Some(tokio::time::Instant::now() + Duration::from_secs(1)),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(bytes.as_ref(), b"payload");
}
