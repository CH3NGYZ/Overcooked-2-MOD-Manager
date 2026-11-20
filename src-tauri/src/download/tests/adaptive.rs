use super::*;

#[tokio::test]
async fn slow_http_race_validates_the_download_and_cleans_up_losers() {
    let payload = vec![42; 80 * 1024];
    // Socket handlers and disk writes are scheduled independently: their
    // nominally identical rates need not produce comparable speed samples.
    let server = TestServer::new(vec![ResponseKind::Slow(1024); 3], payload.clone());
    let cache = tempfile::tempdir().unwrap();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let digest = format!("sha256:{:x}", Sha256::digest(&payload));
    let winner = tokio::time::timeout(
        Duration::from_secs(8),
        race_candidates(
            server.candidates(3),
            || false,
            |candidate, reporter| {
                let client = &client;
                let cache = cache.path();
                let digest = &digest;
                let size = payload.len() as u64;
                async move {
                    let path = archive::download_to_temp_with_threads_and_first_byte_timeout(
                        client,
                        &candidate.url,
                        cache,
                        "test",
                        &reporter,
                        1,
                        Some(Duration::from_secs(2)),
                    )
                    .await?;
                    if let Err(error) =
                        archive::validate_download_file(&path, Some(size), Some(digest))
                    {
                        let _ = std::fs::remove_file(&path);
                        return Err(error);
                    }
                    Ok(path)
                }
            },
            None,
            RacePolicy::default(),
            |_| {},
        ),
    )
    .await
    .expect("a slow but healthy transfer must finish")
    .unwrap();
    assert_eq!(std::fs::read(&winner).unwrap(), payload);
    assert_eq!(std::fs::read_dir(cache.path()).unwrap().count(), 1);
}

#[tokio::test(start_paused = true)]
async fn comparable_slow_sources_keep_the_initial_connections() {
    let candidates: Vec<_> = (0..8).map(candidate).collect();
    let mut started = Vec::new();
    let size = 80 * 1024;
    let race = race_candidates(
        candidates.clone(),
        || false,
        |candidate, reporter| {
            // Different rates, all below the reference and above half of the
            // fastest. Even the first completion needs multiple speed windows.
            let chunk_size = [1024, 896, 768][started.len() % 3];
            started.push(candidate);
            async move {
                let mut progress = DownloadProgress::connecting("test");
                progress.total = Some(size);
                while progress.downloaded < size {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    let chunk = chunk_size.min(size - progress.downloaded);
                    progress.downloaded += chunk;
                    reporter.received(chunk);
                    reporter.update(progress.clone());
                }
                Ok(vec![42])
            }
        },
        None,
        RacePolicy {
            window: Duration::from_millis(300),
            reference_speed: 256 * 1024,
            tick: Duration::from_millis(20),
            ..policy()
        },
        |_| {},
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), race)
            .await
            .unwrap()
            .unwrap(),
        vec![42]
    );
    assert_eq!(started, candidates[..3], "keep the initial three sources");
}
