use super::*;

#[tokio::test]
async fn comparable_slow_http_sources_keep_the_initial_connections() {
    let payload = vec![42; 80 * 1024];
    let server = TestServer::new(vec![ResponseKind::Slow(1024); 8], payload.clone());
    let cache = tempfile::tempdir().unwrap();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let digest = format!("sha256:{:x}", Sha256::digest(&payload));
    let mut started = Vec::new();
    let began = Instant::now();
    let winner = tokio::time::timeout(
        Duration::from_secs(8),
        race_candidates(
            server.candidates(8),
            || false,
            |candidate, reporter| {
                started.push(candidate.url.clone());
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
            RacePolicy {
                window: Duration::from_millis(300),
                reference_speed: 256 * 1024,
                tick: Duration::from_millis(20),
                ..policy()
            },
            |_| {},
        ),
    )
    .await
    .expect("a slow but healthy transfer must finish")
    .unwrap();
    eprintln!(
        "similar slow HTTP routes: {} of 8 started in {:?}",
        started.len(),
        began.elapsed()
    );
    assert_eq!(std::fs::read(&winner).unwrap(), payload);
    assert_eq!(std::fs::read_dir(cache.path()).unwrap().count(), 1);
    assert_eq!(
        started.len(),
        3,
        "slow, similarly paced sources are worth keeping"
    );
}
