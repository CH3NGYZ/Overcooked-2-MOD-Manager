use super::*;

#[tokio::test]
async fn snapshot_bounds_unpolled_task_size_and_restores_the_previous_policy() {
    let outer = Arc::new(NetworkPolicy::bundled().unwrap());
    let inner = Arc::new(NetworkPolicy::bundled().unwrap());
    let expected = inner.clone();
    let payload = [7_u8; 128 * 1024];
    let work = async move {
        assert!(Arc::ptr_eq(&NetworkPolicy::active().unwrap(), &expected));
        tokio::task::yield_now().await;
        assert!(Arc::ptr_eq(&NetworkPolicy::active().unwrap(), &expected));
        std::hint::black_box(&payload)
            .iter()
            .map(|&v| v as usize)
            .sum::<usize>()
    };
    assert!(std::mem::size_of_val(&work) >= payload.len());
    let scoped = NetworkPolicy::in_snapshot(inner, work);
    assert!(
        std::mem::size_of_val(&scoped) <= 1024,
        "a network scope must not inline its work into the IPC task"
    );
    NetworkPolicy::in_snapshot(outer.clone(), async {
        assert!(Arc::ptr_eq(&NetworkPolicy::active().unwrap(), &outer));
        assert_eq!(scoped.await, 7 * 128 * 1024);
        assert!(Arc::ptr_eq(&NetworkPolicy::active().unwrap(), &outer));
    })
    .await;
}
