use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

struct DropSignal(Arc<AtomicBool>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn official_root_bounds_unpolled_task_size_for_builtin_and_remote_providers() {
    fn assert_send<T: Send>(_: &T) {}
    for provider in [DataProvider::Builtin, DataProvider::Github] {
        let runtime = ProxyRuntime::new(provider);
        // Construct only: no polling, configuration reads or HTTP requests.
        let network = Default::default();
        let task = runtime.official_root(false, &network);
        assert_send(&task);
        assert!(
            std::mem::size_of_val(&task) <= 1024,
            "the official root must bound its task before the first poll"
        );
    }
}

#[tokio::test]
async fn selection_bounds_unpolled_task_size_and_drops_cancelled_work() {
    let runtime = ProxyRuntime::new(DataProvider::Github);
    let dropped = Arc::new(AtomicBool::new(false));
    let signal = DropSignal(dropped.clone());
    let payload = [7_u8; 128 * 1024];
    let (started, ready) = tokio::sync::oneshot::channel();
    let work = async move {
        let _signal = signal;
        let _ = started.send(());
        std::future::pending::<()>().await;
        Ok::<_, String>(std::hint::black_box(&payload).len())
    };
    assert!(std::mem::size_of_val(&work) >= payload.len());
    let scoped = runtime.in_selection(runtime.selection(), work);
    assert!(
        std::mem::size_of_val(&scoped) <= 1024,
        "a selection scope must not inline its work into the IPC task"
    );
    let change = async {
        ready.await.unwrap();
        runtime.select_provider(DataProvider::Cloudflare, None);
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(scoped, change)
    })
    .await
    .expect("a provider switch must cancel the pending work");
    assert_eq!(result.err().as_deref(), Some("source.addressMismatch"));
    assert!(dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn selection_keeps_borrowed_results_and_rejects_a_late_provider_switch() {
    let runtime = ProxyRuntime::new(DataProvider::Github);
    let value = String::from("borrowed result");
    let work = async { Ok(value.as_str()) };
    assert_eq!(
        runtime
            .in_selection(runtime.selection(), work)
            .await
            .unwrap(),
        value
    );
    let old = runtime.selection();
    let result = runtime
        .in_selection(old, async {
            runtime.select_provider(DataProvider::Cloudflare, None);
            Ok(value.as_str())
        })
        .await;
    assert_eq!(result.err().as_deref(), Some("source.addressMismatch"));
}
