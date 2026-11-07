use super::*;

#[test]
fn graceful_close_prefers_visible_windows_then_falls_back_to_all() {
    // 没有任何顶层窗口：没有可投递的目标
    assert!(close_window_indexes(&[]).is_empty());
    // 有可见窗口：只投递可见的（等价于用户点 ×）
    assert_eq!(close_window_indexes(&[true, false]), vec![0]);
    assert_eq!(close_window_indexes(&[false, true]), vec![1]);
    assert_eq!(close_window_indexes(&[true, false, true]), vec![0, 2]);
    // 一个可见窗口都没有（最小化到托盘 / 主窗口被插件隐藏）：退回全部顶层窗口
    assert_eq!(close_window_indexes(&[false]), vec![0]);
    assert_eq!(close_window_indexes(&[false, false]), vec![0, 1]);
}

#[test]
fn graceful_wait_timeout_follows_successfully_posted_windows() {
    // 完整超时要留给 Unity 退出 + BepInEx/Harmony 卸载 + 存档写入
    assert_eq!(GRACEFUL_EXIT_TIMEOUT, std::time::Duration::from_secs(20));
    // 一个都没投递成功时只等短宽限，而且必须明显短于完整超时
    assert_eq!(
        GRACEFUL_GRACE_TIMEOUT,
        std::time::Duration::from_millis(1500)
    );
    assert!(GRACEFUL_GRACE_TIMEOUT < GRACEFUL_EXIT_TIMEOUT);

    // 一个都没投递成功时怎么等：Windows 上有 WM_CLOSE 可能还在队列里，等短宽限；
    // 非 Windows（原生 macOS / Wine）无窗口消息投递，保持旧行为进入强制结束。
    let not_posted_plan = if cfg!(windows) {
        GracefulWait::ShortGrace
    } else {
        GracefulWait::Immediate
    };
    assert_eq!(GracefulWait::Immediate.timeout(), std::time::Duration::ZERO);

    // 目标进程一个窗口都没有（例如 Wine 宿主进程）
    let no_windows = GracefulCloseReport {
        targets: 2,
        ..GracefulCloseReport::default()
    };
    assert_eq!(graceful_wait_plan(&no_windows), not_posted_plan);
    assert_eq!(GracefulWait::ShortGrace.timeout(), GRACEFUL_GRACE_TIMEOUT);

    // 有窗口但一个都没投递出去（例如全部被 UIPI 拒绝）：同样走「没投递成功」分支
    let all_failed = GracefulCloseReport {
        targets: 1,
        windows: 2,
        visible_windows: 2,
        posted: 0,
        failed: 2,
        last_error: Some(ERROR_ACCESS_DENIED),
    };
    assert_eq!(graceful_wait_plan(&all_failed), not_posted_plan);

    // PostMessageW 失败、SendMessageTimeoutW 兜底成功：算投递成功，等完整超时
    let retried = GracefulCloseReport {
        targets: 1,
        windows: 1,
        visible_windows: 1,
        posted: 1,
        failed: 0,
        last_error: None,
    };
    assert_eq!(graceful_wait_plan(&retried), GracefulWait::Full);
    assert_eq!(GracefulWait::Full.timeout(), GRACEFUL_EXIT_TIMEOUT);
}
