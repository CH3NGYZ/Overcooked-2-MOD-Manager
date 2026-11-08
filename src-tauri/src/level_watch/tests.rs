use super::*;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

fn temp_root(label: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("oc2-level-watch-{label}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).expect("创建临时目录");
    root
}

#[test]
fn access_events_do_not_trigger_reload() {
    assert!(!affects_level_list(&EventKind::Access(
        notify::event::AccessKind::Any
    )));
    assert!(affects_level_list(&EventKind::Create(
        notify::event::CreateKind::Any
    )));
    assert!(affects_level_list(&EventKind::Remove(
        notify::event::RemoveKind::Any
    )));
    assert!(affects_level_list(&EventKind::Modify(
        notify::event::ModifyKind::Any
    )));
    // 平台差异兜底：Any/Other 也当作变化
    assert!(affects_level_list(&EventKind::Any));
    assert!(affects_level_list(&EventKind::Other));
}

#[test]
fn missing_root_falls_back_to_nearest_existing_ancestor() {
    let root = temp_root("fallback");
    let levels = root.join("levels");
    let disabled = root.join("cache/disabled");
    fs::create_dir_all(&levels).expect("创建 levels");
    fs::create_dir_all(root.join("cache")).expect("创建 cache");
    assert_eq!(
        watch_plan(&[levels.clone(), disabled]),
        vec![
            WatchTarget {
                path: root.join("cache"),
                recursive: false,
            },
            WatchTarget {
                path: levels,
                recursive: true,
            },
        ]
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn appearing_target_upgrades_the_plan() {
    let root = temp_root("upgrade");
    let levels = root.join("levels");
    // 还不存在：只能盯父目录的非递归变化
    assert_eq!(
        watch_plan(std::slice::from_ref(&levels)),
        vec![WatchTarget {
            path: root.clone(),
            recursive: false,
        }]
    );
    fs::create_dir_all(&levels).expect("创建 levels");
    // 出现了：换成对目标本身的递归监听
    assert_eq!(
        watch_plan(std::slice::from_ref(&levels)),
        vec![WatchTarget {
            path: levels,
            recursive: true,
        }]
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn overlapping_targets_merge_into_one_recursive_watch() {
    let root = temp_root("overlap");
    // nested 缺失时祖先就是 root；root 自己也是根：递归优先，只注册一次
    assert_eq!(
        watch_plan(&[root.clone(), root.join("levels")]),
        vec![WatchTarget {
            path: root.clone(),
            recursive: true,
        }]
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn no_roots_means_no_watch() {
    assert!(watch_plan(&[]).is_empty());
}

/// 没有事件时绝不能按窗口空转通知。
///
/// 与「一串事件只通知一次」合起来才排除了「固定周期通知」那种实现：只测
/// 并发发事件的版本在慢机器上会翻车 —— 发送线程被调度器抢占超过窗口时，
/// 防抖线程确实经历了一段安静窗口、完全有理由先通知一次，而用例按秒表
/// 断言「此刻必须还没通知」。所以事件先在同一个线程里全部塞进通道，再
/// 启动循环，发送与防抖不并发。
#[test]
fn an_idle_channel_never_notifies() {
    let (sender, receiver) = mpsc::channel::<()>();
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let thread = std::thread::spawn(move || {
        debounce_loop(receiver, Duration::from_millis(80), || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
    });
    std::thread::sleep(Duration::from_millis(260));
    assert_eq!(hits.load(Ordering::SeqCst), 0, "没有事件时不能通知");
    drop(sender);
    thread.join().expect("通道关闭后监听循环退出");
}

#[test]
fn burst_of_events_emits_once_after_the_quiet_window() {
    let (sender, receiver) = mpsc::channel::<()>();
    // 先把整串事件塞满通道再启动循环：发送与防抖不并发，窗口内的调度抖动
    // 不会凭空造出一个「安静的窗口」（原因见上一个用例）。
    for _ in 0..500 {
        sender.send(()).expect("发送事件");
    }
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let thread = std::thread::spawn(move || {
        debounce_loop(receiver, Duration::from_millis(80), || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while hits.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(hits.load(Ordering::SeqCst), 1, "一串事件只通知一次");
    // 再安静一会儿也不该补发：pending 已被清掉，窗口空转不通知
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    drop(sender);
    thread.join().expect("通道关闭后监听循环退出");
}

#[test]
fn real_watcher_reports_directory_creation() {
    let root = temp_root("notify");
    let plan = watch_plan(std::slice::from_ref(&root));
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let state = build_watch_state(plan, move || {
        counter.fetch_add(1, Ordering::SeqCst);
    })
    .expect("watcher 建起来了");
    fs::create_dir_all(root.join("新关卡集")).expect("创建关卡集目录");
    let deadline = Instant::now() + Duration::from_secs(5);
    while hits.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        hits.load(Ordering::SeqCst) >= 1,
        "目录创建没有触发通知（watcher 可能没注册成功）"
    );
    // stop 会关通道并 join 线程：能返回就说明线程退出了
    state.stop();
    fs::remove_dir_all(&root).ok();
}
