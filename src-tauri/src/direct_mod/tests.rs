use super::*;

#[tokio::test]
async fn direct_download_uses_ranges_and_preserves_bytes() {
    use std::io::{BufRead, BufReader, Write};
    use std::sync::{Arc, Mutex};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/mod.dll", listener.local_addr().unwrap());
    let ranges = Arc::new(Mutex::new(Vec::new()));
    let recorded = ranges.clone();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server_done = done.clone();
    let truncated = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server_truncated = truncated.clone();
    listener.set_nonblocking(true).unwrap();
    const SIZE: usize = 9 * 1024 * 1024;
    // Serve the real HEAD, range probe, and three file segments, including
    // a truncated response that exercises the downloader's retry policy.
    let server = std::thread::spawn(move || {
        let mut workers = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while !server_done.load(std::sync::atomic::Ordering::Acquire)
            && std::time::Instant::now() < deadline
        {
            let (mut socket, _) = match listener.accept() {
                Ok(socket) => socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            let recorded = recorded.clone();
            let truncated = server_truncated.clone();
            workers.push(std::thread::spawn(move || {
                // Read the request in blocking mode even on Windows.
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut request = String::new();
                reader.read_line(&mut request).unwrap();
                let head = request.starts_with("HEAD ");
                let mut range = None;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        let (start, end) = value.trim().split_once('-').unwrap();
                        range = Some((
                            start.parse::<usize>().unwrap(),
                            end.parse::<usize>().unwrap(),
                        ));
                    }
                }
                let (start, end) = range.unwrap_or((0, SIZE - 1));
                let count = end - start + 1;
                let status = if range.is_some() {
                    "206 Partial Content"
                } else {
                    "200 OK"
                };
                let content_range = if range.is_some() {
                    format!("Content-Range: bytes {start}-{end}/{SIZE}\r\n")
                } else {
                    String::new()
                };
                write!(
                    socket,
                    "HTTP/1.1 {status}\r\nContent-Length: {count}\r\n{content_range}Connection: close\r\n\r\n"
                )
                .unwrap();
                if !head {
                    recorded.lock().unwrap().push((start, end));
                    let bytes: Vec<u8> =
                        (start..=end).map(|index| (index % 251) as u8).collect();
                    if start == 4 * 1024 * 1024
                        && !truncated.swap(true, std::sync::atomic::Ordering::AcqRel)
                    {
                        socket.write_all(&bytes[..bytes.len() / 2]).unwrap();
                    } else {
                        socket.write_all(&bytes).unwrap();
                    }
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    let cache = std::env::temp_dir().join(format!("direct-download-{}", uuid::Uuid::new_v4()));
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let reporter = archive::DownloadReporter::new("direct test");
    let path = archive::download_to_temp_with_threads_and_first_byte_timeout(
        &client,
        &url,
        &cache,
        "direct test",
        &reporter,
        32,
        Some(Duration::from_secs(5)),
    )
    .await
    .unwrap();
    done.store(true, std::sync::atomic::Ordering::Release);
    server.join().unwrap();
    assert!(truncated.load(std::sync::atomic::Ordering::Acquire));
    let bytes = fs::read(&path).unwrap();
    assert_eq!(bytes.len(), SIZE);
    assert!(
        bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| *byte == (index % 251) as u8)
    );
    let mut actual = ranges.lock().unwrap().clone();
    // A valid range can be retried up to three times after a truncated body.
    // Verify the unique partition and the retry without assuming one request
    // per segment under parallel test load.
    let retried = (4 * 1024 * 1024, 8 * 1024 * 1024 - 1);
    assert!(actual.iter().filter(|range| **range == retried).count() >= 2);
    for range in &actual {
        assert!(actual.iter().filter(|other| *other == range).count() <= 3);
    }
    actual.sort_unstable();
    actual.dedup();
    assert_eq!(
        actual,
        vec![
            (0, 0),
            (0, 4 * 1024 * 1024 - 1),
            (4 * 1024 * 1024, 8 * 1024 * 1024 - 1),
            (8 * 1024 * 1024, SIZE - 1)
        ]
    );
    fs::remove_dir_all(cache).unwrap();
}

#[test]
fn direct_filename_validation() {
    let url = Url::parse("https://example.com/My%20Mod.DLL?signature=hidden").unwrap();
    assert_eq!(download_filename(&url, None).unwrap(), "My Mod.DLL");
    assert_eq!(supported_filename("A&B.dll").unwrap(), "A&B.dll");
    assert_eq!(
        download_filename(&url, Some("attachment; filename*=UTF-8''Bundle%20Pack.zip")).unwrap(),
        "Bundle Pack.zip"
    );
    assert!(download_filename(&url, Some("attachment; filename=page.html")).is_err());
    for name in [
        "../mod.zip",
        "x%2Fmod.dll",
        "x.exe",
        "x.dll:stream",
        "x.zip.",
    ] {
        assert!(supported_filename(name).is_none(), "{name}");
    }
}

#[test]
fn downloaded_content_validation() {
    let path = std::env::temp_dir().join(format!("direct-mod-{}", uuid::Uuid::new_v4()));
    fs::write(&path, b"<html>not a mod</html>").unwrap();
    assert!(validate_mod_file(&path, "mod.dll").is_err());
    assert!(validate_mod_file(&path, "mod.zip").is_err());
    let mut pe = vec![0u8; 88];
    pe[..2].copy_from_slice(b"MZ");
    pe[60..64].copy_from_slice(&64u32.to_le_bytes());
    pe[64..68].copy_from_slice(b"PE\0\0");
    fs::write(&path, &pe).unwrap();
    assert!(validate_mod_file(&path, "mod.dll").is_err());
    pe[87] = 0x20;
    fs::write(&path, pe).unwrap();
    assert!(validate_mod_file(&path, "mod.dll").is_ok());
    fs::remove_file(path).unwrap();
}
