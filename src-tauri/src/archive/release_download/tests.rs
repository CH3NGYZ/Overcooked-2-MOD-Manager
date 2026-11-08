use super::*;
use crate::archive::DownloadRoute;
use crate::download::{DownloadCandidate, DownloadExecution};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

fn asset(name: &str) -> ReleaseAsset {
    ReleaseAsset {
        name: name.into(),
        size: 0,
        download_count: None,
        download_url: name.into(),
        digest: None,
    }
}

#[test]
fn volume_discovery_is_exact_ordered_and_rejects_incomplete_sets() {
    let main = asset("x_v1.zip");
    let parts = vec![
        asset("x_v1.z02"),
        main.clone(),
        asset("other_v1.z01"),
        asset("x_v1.z01"),
    ];
    let found = asset_set(&main, &parts).unwrap();
    assert_eq!(
        found.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["x_v1.z01", "x_v1.z02", "x_v1.zip"]
    );
    for names in [
        vec!["x_v1.zip", "x_v1.z02"],
        vec!["x_v1.zip", "x_v1.z01", "x_v1.z01"],
        vec!["x_v1.z01"],
        vec!["x_v1.zip", "x_v1.z00"],
        vec!["x_v1.zip", "x_v1.z100"],
    ] {
        assert_eq!(
            asset_set(&main, &names.iter().map(|n| asset(n)).collect::<Vec<_>>()).unwrap_err(),
            "download.invalidVolumeSet"
        );
    }
    assert_eq!(
        asset_set(&main, &[main.clone(), asset("other_v1.z01")])
            .unwrap()
            .len(),
        1
    );
}

/// Build classic disk-relative ZIP records from a real zip::ZipWriter archive.
/// The second entry and central directory are on later disks, and records can
/// cross boundaries. This is a protocol fixture, not an external-tool sample.
fn split_fixture() -> (Vec<Vec<u8>>, Vec<u8>) {
    let payload = (0..170000)
        .map(|i| ((i * 37 + i / 251) % 256) as u8)
        .collect::<Vec<_>>();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("firstentry", options).unwrap();
    writer.write_all(&payload).unwrap();
    writer.start_file("secondentry", options).unwrap();
    writer.write_all(b"second file").unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let mut data = b"PK\x07\x08".to_vec();
    data.extend(bytes);
    let end = data.len() - 22;
    let read32 = |bytes: &[u8], at: usize| {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
    };
    let cd = read32(&data, end + 16) + 4;
    let boundaries = [0, 65536, 131072];
    let disk_at = |offset: usize| boundaries.iter().rposition(|b| *b <= offset).unwrap();
    let mut cursor = cd;
    for _ in 0..2 {
        let local = read32(&data, cursor + 42) + 4;
        let disk = disk_at(local);
        data[cursor + 34..cursor + 36].copy_from_slice(&(disk as u16).to_le_bytes());
        data[cursor + 42..cursor + 46]
            .copy_from_slice(&((local - boundaries[disk]) as u32).to_le_bytes());
        let name = u16::from_le_bytes(data[cursor + 28..cursor + 30].try_into().unwrap()) as usize;
        let extra = u16::from_le_bytes(data[cursor + 30..cursor + 32].try_into().unwrap()) as usize;
        let comment =
            u16::from_le_bytes(data[cursor + 32..cursor + 34].try_into().unwrap()) as usize;
        cursor += 46 + name + extra + comment;
    }
    data[end + 4..end + 6].copy_from_slice(&2u16.to_le_bytes());
    data[end + 6..end + 8].copy_from_slice(&(disk_at(cd) as u16).to_le_bytes());
    data[end + 16..end + 20]
        .copy_from_slice(&((cd - boundaries[disk_at(cd)]) as u32).to_le_bytes());
    (
        vec![
            data[..65536].to_vec(),
            data[65536..131072].to_vec(),
            data[131072..].to_vec(),
        ],
        payload,
    )
}

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("oc2-volumes-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn reassembly_fixes_disk_relative_offsets_and_rejects_corruption_and_zip64() {
    let dir = TempDir::new();
    let (parts, payload) = split_fixture();
    let paths = parts
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let path = dir.0.join(format!("{i}"));
            std::fs::write(&path, b).unwrap();
            path
        })
        .collect::<Vec<_>>();
    let output = dir.0.join("merged.zip");
    super::super::split_zip::reassemble(&paths, &output).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();
    let mut actual = Vec::new();
    zip.by_name("firstentry")
        .unwrap()
        .read_to_end(&mut actual)
        .unwrap();
    assert_eq!(actual, payload);
    assert_eq!(zip.len(), 2);
    drop(zip);
    let mut last = parts[2].clone();
    let end = last.len() - 22;
    last[end + 4..end + 6].copy_from_slice(&u16::MAX.to_le_bytes());
    std::fs::write(&paths[2], &last).unwrap();
    assert_eq!(
        super::super::split_zip::reassemble(&paths, &output).unwrap_err(),
        "download.unsupportedSplitZip"
    );
    last[end + 4..end + 6].copy_from_slice(&1u16.to_le_bytes());
    std::fs::write(&paths[2], &last).unwrap();
    assert_eq!(
        super::super::split_zip::reassemble(&paths, &output).unwrap_err(),
        "download.invalidSplitZip"
    );
}

struct Server {
    url: String,
    stop: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new(parts: Vec<Vec<u8>>, fail: bool) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let (stop2, calls2, peak2) = (stop.clone(), calls.clone(), peak.clone());
        let parts = Arc::new(parts);
        let active = Arc::new(AtomicUsize::new(0));
        let thread = std::thread::spawn(move || {
            let mut handlers = Vec::new();
            while !stop2.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        let (parts, active, calls, peak) =
                            (parts.clone(), active.clone(), calls2.clone(), peak2.clone());
                        handlers.push(std::thread::spawn(move || {
                            socket.set_nonblocking(false).unwrap();
                            socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                            socket.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
                            let mut request=[0u8;4096]; let count=socket.read(&mut request).unwrap_or(0);
                            let request=String::from_utf8_lossy(&request[..count]);
                            let path=request.split_whitespace().nth(1).unwrap_or("/0");
                            let index=path.trim_start_matches('/').split('/').next().unwrap().parse::<usize>().unwrap_or(0);
                            calls.fetch_add(1,Ordering::Relaxed);
                            let count=active.fetch_add(1,Ordering::Relaxed)+1;
                            peak.fetch_max(count,Ordering::Relaxed);
                            std::thread::sleep(Duration::from_millis(40));
                            let data=&parts[index];
                            let status=if fail {"503 Unavailable"}else{"200 OK"};
                            let _=write!(socket,"HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",data.len());
                            let _=socket.write_all(data);
                            active.fetch_sub(1,Ordering::Relaxed);
                        }));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
            for handler in handlers {
                handler.join().unwrap();
            }
        });
        Self {
            url,
            stop,
            calls,
            peak,
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

#[tokio::test]
async fn real_http_downloads_two_volumes_and_cleans_all_parts_after_reassembly() {
    let (parts, payload) = split_fixture();
    let dir = TempDir::new();
    let server = Server::new(parts.clone(), false);
    let assets = parts
        .iter()
        .enumerate()
        .map(|(i, b)| ReleaseAsset {
            name: format!("part{i}"),
            size: b.len() as u64,
            download_count: None,
            download_url: format!("{}/{i}", server.url),
            digest: Some(format!("sha256:{:x}", Sha256::digest(b))),
        })
        .collect();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let path = download(&client, assets, &dir.0, "test", None, |url| {
        Ok(RepositoryDownloadPlan {
            candidates: vec![DownloadCandidate {
                route: DownloadRoute::Direct,
                url: url.into(),
            }],
            download_threads: 1,
            execution: DownloadExecution {
                max_parallelism: 1,
                race: false,
            },
        })
    })
    .await
    .unwrap();
    assert_eq!(server.calls.load(Ordering::Relaxed), 3);
    assert_eq!(server.peak.load(Ordering::Relaxed), 2);
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut actual = Vec::new();
    zip.by_name("firstentry")
        .unwrap()
        .read_to_end(&mut actual)
        .unwrap();
    assert_eq!(actual, payload);
}

#[tokio::test]
async fn exhausted_candidates_allow_only_three_outer_retries_and_leave_no_files() {
    let dir = TempDir::new();
    let server = Server::new(vec![vec![1]], true);
    let assets = vec![asset("a"), asset("b")];
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let result = download(&client, assets, &dir.0, "test", None, |_| {
        Ok(RepositoryDownloadPlan {
            candidates: (0..3)
                .map(|i| DownloadCandidate {
                    route: DownloadRoute::Proxy,
                    url: format!("{}/0/{i}", server.url),
                })
                .collect(),
            download_threads: 1,
            execution: DownloadExecution {
                max_parallelism: 3,
                race: true,
            },
        })
    })
    .await;
    assert!(result.is_err());
    let calls = server.calls.load(Ordering::Relaxed);
    assert!((21..=24).contains(&calls), "{calls}");
    assert!(server.peak.load(Ordering::Relaxed) <= 6);
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
}

#[test]
fn aggregate_counts_only_one_candidate_snapshot_per_volume() {
    let mut a = DownloadProgress::connecting("test");
    a.downloaded = 30;
    a.total = Some(100);
    a.parallel_sources = Some(3);
    let mut b = a.clone();
    b.downloaded = 20;
    let volumes = vec![
        VolumeProgress {
            name: "a".into(),
            state: "downloading".into(),
            retry: 2,
            progress: Box::new(a),
        },
        VolumeProgress {
            name: "b".into(),
            state: "downloading".into(),
            retry: 0,
            progress: Box::new(b),
        },
    ];
    let total = aggregate("test", &volumes, false);
    assert_eq!(total.downloaded, 50);
    assert_eq!(total.total, Some(200));
    assert!(!total.finished);
    assert_eq!(total.volumes[0].retry, 2);
}

#[tokio::test]
async fn bad_volume_digest_retries_and_cleans_successful_companions() {
    let (parts, _) = split_fixture();
    let dir = TempDir::new();
    let server = Server::new(parts.clone(), false);
    let assets = parts
        .iter()
        .enumerate()
        .map(|(i, bytes)| ReleaseAsset {
            name: format!("volume{i}"),
            size: bytes.len() as u64,
            download_count: None,
            download_url: format!("{}/{i}", server.url),
            digest: Some(if i == 1 {
                "0".repeat(64)
            } else {
                format!("{:x}", Sha256::digest(bytes))
            }),
        })
        .collect();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let result = download(&client, assets, &dir.0, "test", None, |url| {
        Ok(RepositoryDownloadPlan {
            candidates: vec![DownloadCandidate {
                route: DownloadRoute::Direct,
                url: url.into(),
            }],
            download_threads: 1,
            execution: DownloadExecution {
                max_parallelism: 1,
                race: false,
            },
        })
    })
    .await;
    assert!(result.unwrap_err().starts_with("download.checksumMismatch"));
    assert_eq!(server.calls.load(Ordering::Relaxed), 6);
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
}

#[tokio::test]
async fn dropping_download_cleans_completed_and_inflight_volumes() {
    let (parts, _) = split_fixture();
    let dir = TempDir::new();
    let server = Server::new(parts, false);
    let assets = (0..3)
        .map(|i| ReleaseAsset {
            download_url: format!("{}/{i}", server.url),
            ..asset(&format!("part{i}"))
        })
        .collect();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let result = tokio::time::timeout(
        Duration::from_millis(65),
        download(&client, assets, &dir.0, "test", None, |url| {
            Ok(RepositoryDownloadPlan {
                candidates: vec![DownloadCandidate {
                    route: DownloadRoute::Direct,
                    url: url.into(),
                }],
                download_threads: 1,
                execution: DownloadExecution {
                    max_parallelism: 1,
                    race: false,
                },
            })
        }),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
}

#[tokio::test]
async fn ordinary_single_file_keeps_exact_bytes_and_has_no_outer_retry() {
    let dir = TempDir::new();
    let payload = b"a plain DLL payload".to_vec();
    let server = Server::new(vec![payload.clone()], false);
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let path = download(
        &client,
        vec![asset("single.dll")],
        &dir.0,
        "test",
        None,
        |_| {
            Ok(RepositoryDownloadPlan {
                candidates: vec![DownloadCandidate {
                    route: DownloadRoute::Direct,
                    url: format!("{}/0", server.url),
                }],
                download_threads: 1,
                execution: DownloadExecution {
                    max_parallelism: 1,
                    race: false,
                },
            })
        },
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(path).unwrap(), payload);
    assert_eq!(server.calls.load(Ordering::Relaxed), 1);
}

#[test]
#[ignore = "external Info-ZIP sample; OC2_SPLIT_ZIP_SAMPLE points to its final .zip"]
fn external_infozip_sample_reassembles_and_validates_every_entry() {
    let main = PathBuf::from(std::env::var("OC2_SPLIT_ZIP_SAMPLE").expect("OC2_SPLIT_ZIP_SAMPLE"));
    let mut paths = Vec::new();
    for number in 1..=99 {
        let part = main.with_extension(format!("z{number:02}"));
        if !part.exists() {
            break;
        }
        paths.push(part);
    }
    paths.push(main);
    let dir = TempDir::new();
    let output = dir.0.join("merged.zip");
    super::super::split_zip::reassemble(&paths, &output).unwrap();
    let archive = zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();
    assert!(!archive.is_empty());
    if let Ok(target) = std::env::var("OC2_SPLIT_ZIP_OUTPUT") {
        std::fs::copy(output, target).unwrap();
    }
}
