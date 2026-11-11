use super::*;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

fn key(index: usize) -> BlockKey {
    (PathBuf::from("bundle"), index, 1, 1, 0)
}

#[test]
fn adopting_a_decoded_block_keeps_its_allocation_without_copying() {
    let cache = BundleBlockCache::default();
    let decoded = vec![7; 1024];
    let allocation = decoded.as_ptr();
    let bytes = cache.block(key(0), || Ok(decoded)).unwrap();
    assert_eq!(bytes.as_ptr(), allocation);
    assert_eq!(bytes.as_slice(), &[7; 1024]);
    let reused = cache.block(key(0), || panic!("must reuse")).unwrap();
    assert!(Arc::ptr_eq(&bytes, &reused));
    assert_eq!(cache.stats().0, 1024);
}

#[test]
fn cloned_contexts_share_bytes_and_one_aggregate_eviction_budget() {
    let cache = BundleBlockCache {
        shared: Arc::new(Shared {
            budget: 8,
            ..Shared::default()
        }),
    };
    let worker = cache.clone();
    let first = cache.block(key(0), || Ok(vec![1; 4])).unwrap();
    let shared = worker.block(key(0), || panic!("must reuse")).unwrap();
    assert!(Arc::ptr_eq(&first, &shared));
    worker.block(key(1), || Ok(vec![2; 4])).unwrap();
    cache.block(key(2), || Ok(vec![3; 4])).unwrap();
    let state = cache.shared.state.lock().unwrap();
    assert_eq!(state.bytes, 8);
    assert_eq!(state.entries.len(), 2);
    assert!(state.entries.iter().all(|(key, _)| key.1 != 0));
    assert_eq!(&*first, &[1; 4], "an in-flight reader owns its Arc");
}

#[test]
fn simultaneous_requests_decode_the_same_block_once() {
    let cache = BundleBlockCache::default();
    let loaded = AtomicUsize::new(0);
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (started, received) = mpsc::channel();
    std::thread::scope(|scope| {
        let mut tasks = vec![];
        for _ in 0..4 {
            let (cache, loaded, gate, started) =
                (cache.clone(), &loaded, gate.clone(), started.clone());
            tasks.push(scope.spawn(move || {
                cache.block(key(0), || {
                    loaded.fetch_add(1, Ordering::SeqCst);
                    started.send(()).unwrap();
                    let (open, _) = gate
                        .1
                        .wait_timeout_while(
                            gate.0.lock().unwrap(),
                            Duration::from_secs(5),
                            |open| !*open,
                        )
                        .unwrap();
                    assert!(*open);
                    Ok(vec![7; 16])
                })
            }));
        }
        received.recv_timeout(Duration::from_secs(3)).unwrap();
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        let values: Vec<_> = tasks
            .into_iter()
            .map(|t| t.join().unwrap().unwrap())
            .collect();
        assert!(values.iter().all(|bytes| Arc::ptr_eq(&values[0], bytes)));
    });
    assert_eq!(loaded.load(Ordering::SeqCst), 1);
}

#[test]
fn different_blocks_decode_in_parallel_without_the_cache_mutex() {
    let cache = BundleBlockCache::default();
    let gate = (Mutex::new(false), Condvar::new());
    let (started, received) = mpsc::channel();
    std::thread::scope(|scope| {
        let mut tasks = vec![];
        for index in 0..2 {
            let (cache, gate, started) = (cache.clone(), &gate, started.clone());
            tasks.push(scope.spawn(move || {
                cache.block(key(index), || {
                    started.send(()).unwrap();
                    let (open, _) = gate
                        .1
                        .wait_timeout_while(
                            gate.0.lock().unwrap(),
                            Duration::from_secs(5),
                            |open| !*open,
                        )
                        .unwrap();
                    assert!(*open);
                    Ok(vec![index as u8])
                })
            }));
        }
        let parallel = received.recv_timeout(Duration::from_secs(2)).is_ok()
            && received.recv_timeout(Duration::from_secs(2)).is_ok();
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        for task in tasks {
            task.join().unwrap().unwrap();
        }
        assert!(parallel, "unrelated blocks must overlap");
    });
}

#[test]
fn errors_and_panics_release_same_block_waiters_and_remain_retryable() {
    let cache = BundleBlockCache::default();
    assert!(cache.block(key(0), || Err(ParseError::NotABundle)).is_err());
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cache.block(key(0), || panic!("decoder"))
    }));
    assert!(panic.is_err());
    assert!(cache.shared.state.lock().unwrap().loading.is_empty());
    assert_eq!(&*cache.block(key(0), || Ok(vec![9])).unwrap(), &[9]);
}

#[test]
#[ignore = "requires the installed commonW3 LZMA bundle; reads only"]
fn real_large_lzma_ranges_match_uncached_reads_and_reuse_one_block() {
    use crate::asset_bundle::{bundle_file_names, bundle_needs_conversion, read_bundle_file_range};

    let path = std::env::var_os("OC2_LZMA_BUNDLE_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            "E:/SteamLibrary/steamapps/common/Overcooked! 2/BepInEx/plugins/OC2DIYLevelRuntimeWLoader/commonW3"
                .into()
        });
    assert!(bundle_needs_conversion(&path).unwrap(), "LZMA fixture");
    let name = bundle_file_names(&path).unwrap().remove(0);
    let started = std::time::Instant::now();
    let expected = read_bundle_file_range(&path, &name, 0, Some(8192), None).unwrap();
    let mut cache = BundleBlockCache::default();
    let first = read_bundle_file_range(&path, &name, 0, Some(4096), Some(&mut cache)).unwrap();
    let allocation = {
        let state = cache.shared.state.lock().unwrap();
        assert_eq!(state.entries.len(), 1);
        assert!(state.bytes > 100 * 1024 * 1024);
        assert_eq!(state.entries[0].0.4 & 63, 1);
        state.entries[0].1.clone()
    };
    let mut worker = cache.clone();
    let second = read_bundle_file_range(&path, &name, 4096, Some(4096), Some(&mut worker)).unwrap();
    assert_eq!(first, expected[..4096]);
    assert_eq!(second, expected[4096..]);
    let state = cache.shared.state.lock().unwrap();
    assert_eq!(state.entries.len(), 1);
    assert!(Arc::ptr_eq(&allocation, &state.entries[0].1));
    eprintln!(
        "real LZMA range cache: bytes={}, blocks=1, ranges=2, shared_allocation=true, elapsed={:?}",
        state.bytes,
        started.elapsed()
    );
}
