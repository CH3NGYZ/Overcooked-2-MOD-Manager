use super::*;
use crate::http_diagnostics::HttpRequestExt;
use crate::models::DataProvider;
use std::io::Write;

fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (path, body) in files {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        if path.ends_with('/') {
            writer.add_directory(*path, options).unwrap();
        } else {
            writer.start_file(*path, options).unwrap();
            writer.write_all(body).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn wrapped_and_flat_archives_have_the_same_document_identity() {
    for prefix in ["", "repository-main/", "repository-main-a1b2c3d/"] {
        let root = format!("{prefix}mods.toml");
        let maps = format!("{prefix}maps.toml");
        let ignored = format!("{prefix}README.md");
        let bytes = archive(&[
            (&root, b"schema = 1"),
            (&maps, b"maps = []"),
            (&ignored, b"readme"),
        ]);
        let package = SourcePackage::from_zip(&bytes).unwrap();
        assert_eq!(package.read("mods.toml").unwrap(), "schema = 1");
        assert_eq!(package.read("MAPS.TOML").unwrap(), "maps = []");
        assert_eq!(package.documents.len(), 2);
        assert!(package.read("README.md").is_err());
    }
}

#[test]
fn explicit_root_directories_and_nested_documents_are_supported() {
    let bytes = archive(&[
        ("repo/", b""),
        ("repo/mods.toml", b"schema = 1"),
        ("repo/maps/", b""),
        ("repo/maps/custom.toml", b"maps = []"),
    ]);
    assert!(
        SourcePackage::from_zip(&bytes)
            .unwrap()
            .read("maps/custom.toml")
            .is_ok()
    );
}

#[test]
fn source_root_must_be_unique_and_contain_all_members() {
    for files in [
        vec![("maps.toml", b"x = 1".as_slice())],
        vec![
            ("a/mods.toml", b"x = 1".as_slice()),
            ("b/mods.toml", b"x = 1".as_slice()),
        ],
        vec![
            ("repo/mods.toml", b"x = 1".as_slice()),
            ("outside.md", b"outside".as_slice()),
        ],
    ] {
        assert!(SourcePackage::from_zip(&archive(&files)).is_err());
    }
}

#[test]
fn every_member_path_is_checked_even_for_ignored_files() {
    for path in [
        "../outside.md",
        "/outside.md",
        "C:/outside.md",
        "folder\\outside.md",
        "folder//outside.md",
        "folder/./outside.md",
    ] {
        let bytes = archive(&[("mods.toml", b"x = 1"), (path, b"ignored")]);
        assert!(SourcePackage::from_zip(&bytes).is_err(), "{path}");
    }
}

#[test]
fn case_aliases_and_file_directory_collisions_are_rejected() {
    for files in [
        vec![
            ("mods.toml", b"x = 1".as_slice()),
            ("MODS.TOML", b"x = 2".as_slice()),
        ],
        vec![
            ("mods.toml", b"x = 1".as_slice()),
            ("folder", b"file".as_slice()),
            ("folder/map.toml", b"x = 1".as_slice()),
        ],
        vec![
            ("mods.toml", b"x = 1".as_slice()),
            ("folder/", b"".as_slice()),
            ("folder", b"file".as_slice()),
        ],
    ] {
        assert!(SourcePackage::from_zip(&archive(&files)).is_err());
    }
}

#[test]
fn duplicate_central_directory_names_cannot_be_silently_collapsed() {
    let mut bytes = archive(&[
        ("mods.toml", b"x = 1"),
        ("a.toml", b"a = 1"),
        ("b.toml", b"b = 1"),
    ]);
    for index in 0..bytes.len() - 5 {
        if &bytes[index..index + 6] == b"b.toml" {
            bytes[index..index + 6].copy_from_slice(b"a.toml");
        }
    }
    assert!(SourcePackage::from_zip(&bytes).is_err());
}

#[test]
fn ignored_symlinks_are_rejected() {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("mods.toml", options).unwrap();
    writer.write_all(b"x = 1").unwrap();
    writer
        .add_symlink("ignored-link", "../outside", options)
        .unwrap();
    assert!(SourcePackage::from_zip(&writer.finish().unwrap().into_inner()).is_err());
}

#[test]
fn encrypted_ignored_members_are_rejected() {
    let mut bytes = archive(&[("mods.toml", b"x = 1"), ("ignored.bin", b"payload")]);
    // The encryption bit must fail before any attempt to read or ignore data.
    let central = bytes
        .windows(4)
        .rposition(|part| part == [80, 75, 1, 2])
        .unwrap();
    bytes[central + 8] |= 1;
    assert!(SourcePackage::from_zip(&bytes).is_err());
}

#[test]
fn ignored_member_checksums_and_document_utf8_are_validated() {
    let mut bytes = archive(&[("mods.toml", b"x = 1"), ("ignored.bin", b"payload")]);
    let position = bytes
        .windows(7)
        .position(|part| part == b"payload")
        .unwrap();
    bytes[position] ^= 1;
    assert!(SourcePackage::from_zip(&bytes).is_err());
    assert!(SourcePackage::from_zip(&archive(&[("mods.toml", &[255])])).is_err());
}

#[test]
fn html_empty_and_truncated_archives_are_not_valid_empty_packages() {
    for bytes in [
        b"".as_slice(),
        b"<html>not a ZIP</html>".as_slice(),
        b"PK-not-a-ZIP".as_slice(),
    ] {
        assert!(SourcePackage::from_zip(bytes).is_err());
    }
    let mut bytes = archive(&[("mods.toml", b"x = 1")]);
    bytes.pop();
    assert!(SourcePackage::from_zip(&bytes).is_err());
}

fn set_declared_size(bytes: &mut [u8], name: &[u8], size: u32) {
    let position = bytes
        .windows(4)
        .enumerate()
        .find(|(index, part)| {
            *part == [80, 75, 1, 2] && bytes.get(index + 46..index + 46 + name.len()) == Some(name)
        })
        .map(|(index, _)| index)
        .unwrap();
    bytes[position + 24..position + 28].copy_from_slice(&size.to_le_bytes());
}

#[test]
fn compressed_entry_count_document_and_expanded_limits_are_enforced() {
    assert!(
        SourcePackage::from_zip(&vec![0; MAX_ARCHIVE_BYTES + 1])
            .unwrap_err()
            .to_string()
            .contains("source.tooLarge")
    );
    let mut bytes = archive(&[("mods.toml", b"x = 1")]);
    let end = bytes.len() - 22;
    bytes[end + 10..end + 12].copy_from_slice(&((MAX_ENTRIES + 1) as u16).to_le_bytes());
    assert!(
        SourcePackage::from_zip(&bytes)
            .unwrap_err()
            .to_string()
            .contains("source.tooLarge")
    );
    let mut bytes = archive(&[("mods.toml", b"x = 1")]);
    set_declared_size(&mut bytes, b"mods.toml", (MAX_SOURCE_BYTES + 1) as u32);
    assert!(
        SourcePackage::from_zip(&bytes)
            .unwrap_err()
            .to_string()
            .contains("source.tooLarge")
    );
    let mut bytes = archive(&[("mods.toml", b"x = 1"), ("ignored.bin", b"payload")]);
    set_declared_size(&mut bytes, b"ignored.bin", MAX_EXPANDED_BYTES as u32);
    assert!(
        SourcePackage::from_zip(&bytes)
            .unwrap_err()
            .to_string()
            .contains("source.tooLarge")
    );
}

#[test]
fn percent_encoded_unicode_and_literal_plus_paths_share_document_identity() {
    let bytes = archive(&[("mods.toml", b"x = 1"), ("地图/a+b.toml", b"x = 2")]);
    let package = SourcePackage::from_zip(&bytes).unwrap();
    assert_eq!(
        package.read("%E5%9C%B0%E5%9B%BE/a+b.toml").unwrap(),
        "x = 2"
    );
    assert_eq!(package.read("地图/a%2Bb.toml").unwrap(), "x = 2");
    for path in [
        "%2e%2e/mods.toml",
        "%2fmods.toml",
        "a%5cb.toml",
        "a%3ab.toml",
        "bad%.toml",
        "%ff.toml",
    ] {
        assert!(package.read(path).is_err(), "{path}");
    }
}

#[test]
fn bundled_package_reuses_the_existing_embedded_documents() {
    let package = SourcePackage::bundled();
    assert!(Arc::ptr_eq(&package, &SourcePackage::bundled()));
    for (path, text) in super::super::embedded::bundled_files() {
        assert_eq!(package.read(path).unwrap(), *text);
    }
}

#[test]
fn deflated_document_at_the_exact_size_limit_is_readable() {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer.start_file("repo-main/mods.toml", options).unwrap();
    let text = format!("payload = \"{}\"", "x".repeat(MAX_SOURCE_BYTES - 12));
    assert_eq!(text.len(), MAX_SOURCE_BYTES);
    writer.write_all(text.as_bytes()).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    assert!(bytes.len() < MAX_SOURCE_BYTES);
    assert_eq!(
        SourcePackage::from_zip(&bytes)
            .unwrap()
            .read("mods.toml")
            .unwrap(),
        text
    );
}

#[test]
#[ignore = "requires local official ZIP paths in OC2_SOURCE_ARCHIVE_FIXTURES; no network"]
fn real_official_source_archives_have_readable_shared_root_documents() {
    let fixtures = std::env::var_os("OC2_SOURCE_ARCHIVE_FIXTURES")
        .expect("set OC2_SOURCE_ARCHIVE_FIXTURES to local downloaded ZIP paths");
    let paths = std::env::split_paths(&fixtures).collect::<Vec<_>>();
    assert!(!paths.is_empty());
    for path in paths {
        let started = std::time::Instant::now();
        assert!(std::fs::metadata(&path).unwrap().len() <= MAX_ARCHIVE_BYTES as u64);
        let bytes = std::fs::read(&path).unwrap();
        let package = SourcePackage::from_zip(&bytes).unwrap();
        for document in [
            "mods.toml",
            "maps.toml",
            "proxies.toml",
            "network-plans.toml",
        ] {
            assert!(
                !package.read(document).unwrap().trim().is_empty(),
                "{document}"
            );
        }
        eprintln!(
            "source archive fixture: compressed={} documents={} elapsed_ms={}",
            bytes.len(),
            package.documents.len(),
            started.elapsed().as_millis()
        );
    }
}

#[tokio::test]
async fn scoped_snapshot_supports_borrows_nesting_and_cancellation() {
    let outer = SourcePackage::bundled();
    let inner = Arc::new(SourcePackage::from_zip(&archive(&[("mods.toml", b"x = 9")])).unwrap());
    let mut borrowed = String::new();
    assert!(SourcePackage::active().is_none());
    SourcePackage::in_snapshot(Arc::clone(&outer), async {
        assert!(Arc::ptr_eq(&SourcePackage::active().unwrap(), &outer));
        SourcePackage::in_snapshot(Arc::clone(&inner), async {
            borrowed.push_str(SourcePackage::active().unwrap().read("mods.toml").unwrap());
            tokio::task::yield_now().await;
            assert!(Arc::ptr_eq(&SourcePackage::active().unwrap(), &inner));
        })
        .await;
        assert!(Arc::ptr_eq(&SourcePackage::active().unwrap(), &outer));
    })
    .await;
    assert_eq!(borrowed, "x = 9");
    assert!(SourcePackage::active().is_none());
    let mut pending = Box::pin(SourcePackage::in_snapshot(outer, async {
        assert!(SourcePackage::active().is_some());
        std::future::pending::<()>().await;
    }));
    assert!(futures_util::poll!(pending.as_mut()).is_pending());
    assert!(SourcePackage::active().is_none());
    drop(pending);
    assert!(SourcePackage::active().is_none());
}

#[test]
fn scoped_future_does_not_inline_large_work() {
    let payload = [7u8; 128 * 1024];
    let future = SourcePackage::in_snapshot(SourcePackage::bundled(), async move {
        std::hint::black_box(payload);
    });
    assert!(std::mem::size_of_val(&future) < 4096);
}

fn serve(responses: Vec<Vec<u8>>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    use std::io::{BufRead, BufReader};
    use std::time::{Duration, Instant};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for response in responses {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "request did not arrive");
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            // Windows accepted sockets can inherit the listener's mode.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
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
                assert!(request.len() < 16 * 1024, "request headers are too large");
            }
            stream.write_all(&response).unwrap();
            requests.push(request);
        }
        requests
    });
    (format!("http://{address}"), server)
}

fn reply(status: &str, content_type: &str, body: &[u8], extra: &str) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
        body.len(),
    ).into_bytes();
    response.extend_from_slice(body);
    response
}

fn assert_anonymous_archive_headers(request: &str) {
    let headers = request
        .lines()
        .filter_map(|line| line.split_once(':'))
        .collect::<Vec<_>>();
    let value = |name: &str| {
        headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .unwrap()
            .1
            .trim()
    };
    assert_eq!(
        value("User-Agent"),
        concat!("Overcooked 2 MOD Manager/", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(value("Cache-Control"), "no-cache, no-store, max-age=0");
    assert_eq!(value("Pragma"), "no-cache");
    assert!(
        !headers
            .iter()
            .any(|(key, _)| key.eq_ignore_ascii_case("Authorization")
                || key.eq_ignore_ascii_case("Cookie"))
    );
}

#[tokio::test]
async fn each_remote_provider_fetches_once_with_exact_ua_and_no_authentication() {
    for provider in [
        DataProvider::Github,
        DataProvider::Gitcode,
        DataProvider::Cloudflare,
    ] {
        let bytes = archive(&[("mods.toml", b"x = 1"), ("maps.toml", b"x = 2")]);
        let (address, server) = serve(vec![reply("200 OK", "application/zip", &bytes, "")]);
        let result = fetch(provider, &format!("{address}/archive.zip"), None).await;
        let requests = server.join().unwrap();
        let package = result.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /archive.zip HTTP/1.1\r\n"));
        assert_anonymous_archive_headers(&requests[0]);
        // Repeated document reads succeed after the HTTP fixture has closed.
        assert_eq!(package.read("mods.toml").unwrap(), "x = 1");
        assert_eq!(package.read("maps.toml").unwrap(), "x = 2");
        assert_eq!(package.read("mods.toml").unwrap(), "x = 1");
    }
}

#[tokio::test]
async fn archive_redirects_preserve_exact_anonymous_request_headers() {
    let bytes = archive(&[("repo-main/mods.toml", b"x = 1")]);
    let (address, server) = serve(vec![
        reply(
            "302 Found",
            "application/zip",
            b"",
            "Location: /download.zip\r\n",
        ),
        reply("200 OK", "application/zip", &bytes, ""),
    ]);
    let result = fetch(
        DataProvider::Github,
        &format!("{address}/archive.zip"),
        None,
    )
    .await;
    let requests = server.join().unwrap();
    assert_eq!(result.unwrap().read("mods.toml").unwrap(), "x = 1");
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("GET /archive.zip "));
    assert!(requests[1].starts_with("GET /download.zip "));
    for request in &requests {
        assert_anonymous_archive_headers(request);
    }
}

#[tokio::test]
async fn http_200_html_and_invalid_zip_bodies_are_rejected() {
    let valid = archive(&[("mods.toml", b"x = 1")]);
    for (content_type, body) in [
        ("text/html; charset=utf-8", valid.as_slice()),
        ("application/zip", b"<html>login required</html>".as_slice()),
        ("application/zip", b"not a ZIP".as_slice()),
    ] {
        let (address, server) = serve(vec![reply("200 OK", content_type, body, "")]);
        let result = fetch(
            DataProvider::Github,
            &format!("{address}/archive.zip"),
            None,
        )
        .await;
        assert_eq!(server.join().unwrap().len(), 1);
        assert!(result.is_err());
    }
}

#[tokio::test]
async fn http_errors_are_not_parsed_as_source_archives() {
    let bytes = archive(&[("mods.toml", b"x = 1")]);
    let (address, server) = serve(vec![reply(
        "503 Service Unavailable",
        "application/zip",
        &bytes,
        "",
    )]);
    let result = fetch(
        DataProvider::Github,
        &format!("{address}/archive.zip"),
        None,
    )
    .await;
    assert_eq!(server.join().unwrap().len(), 1);
    let error = result.unwrap_err();
    let (tag, query) = error.split_once('?').unwrap();
    assert_eq!(tag, "config.readFailedReason");
    let params = url::form_urlencoded::parse(query.as_bytes()).collect::<Vec<_>>();
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].0, "reason");
    assert_eq!(params[0].1, "HTTP 503");
}

#[tokio::test]
async fn oversized_content_length_is_rejected_before_reading_the_body() {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        MAX_ARCHIVE_BYTES + 1,
    ).into_bytes();
    let (address, server) = serve(vec![response]);
    let result = fetch(
        DataProvider::Github,
        &format!("{address}/archive.zip"),
        None,
    )
    .await;
    assert_eq!(server.join().unwrap().len(), 1);
    assert!(result.unwrap_err().contains("source.tooLarge"));
}

#[tokio::test]
async fn chunked_response_limit_checks_actual_bytes_and_allows_the_exact_boundary() {
    for limit in [32, 31] {
        let body = "x".repeat(32);
        let response = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n20\r\n{body}\r\n0\r\n\r\n",
        ).into_bytes();
        let (address, server) = serve(vec![response]);
        let response = crate::releases::build_client(30)
            .unwrap()
            .get(format!("{address}/stream"))
            .send_logged()
            .await
            .unwrap();
        assert!(response.content_length().is_none());
        let result = download::response_bytes(response, limit).await;
        assert_eq!(server.join().unwrap().len(), 1);
        if limit == 32 {
            assert_eq!(result.unwrap(), body.as_bytes());
        } else {
            assert!(result.unwrap_err().to_string().contains("source.tooLarge"));
        }
    }
}

#[tokio::test]
async fn builtin_fetch_uses_the_existing_embedded_package_without_a_request() {
    let package = fetch(DataProvider::Builtin, "not a network address", None)
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&package, &SourcePackage::bundled()));
}

#[tokio::test]
async fn gitee_requires_browser_instead_of_sending_an_unverified_http_request() {
    let error = fetch(DataProvider::Gitee, "not a network address", None)
        .await
        .unwrap_err();
    assert_eq!(error, "source.webDownloadFailed");
}

#[tokio::test]
async fn gitcode_verification_html_can_use_the_same_browser_download_layer() {
    for (status, mime) in [("200 OK", "text/html"), ("403 Forbidden", "text/html")] {
        let (address, server) = serve(vec![reply(status, mime, b"<html>verification</html>", "")]);
        let error = fetch(
            DataProvider::Gitcode,
            &format!("{address}/archive.zip"),
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(server.join().unwrap().len(), 1);
        assert_eq!(error, "source.webDownloadFailed");
    }
}
