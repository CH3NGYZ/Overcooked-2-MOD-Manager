#![cfg(target_os = "windows")]

#[path = "../src/gitee_webview.rs"]
mod gitee_webview;

use futures_util::FutureExt;
use gitee_webview::{BrowserOptions, TopicBrowser};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use url::Url;

static PRIVILEGED_CALLS: AtomicUsize = AtomicUsize::new(0);

#[tauri::command]
fn privileged_probe() {
    PRIVILEGED_CALLS.fetch_add(1, Ordering::Relaxed);
}

struct TopicServer {
    base: String,
    challenges: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl TopicServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let challenges = Arc::new(AtomicUsize::new(0));
        let thread_stop = stop.clone();
        let thread_challenges = challenges.clone();
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(_) => break,
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                while request.len() < 8192 && !request.ends_with(b"\r\n\r\n") {
                    let mut buffer = [0; 1024];
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(length) => request.extend_from_slice(&buffer[..length]),
                    }
                }
                let request = String::from_utf8_lossy(&request);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("");
                let (status, body) = if path.starts_with("/topic") {
                    if path.contains("page=2") {
                        (200, "<div class='explore-repo__list'></div>".to_string())
                    } else if path.contains("verified=1") {
                        (
                            200,
                            include_str!("../../tests/fixtures/gitee-topic.html").to_string(),
                        )
                    } else {
                        thread_challenges.fetch_add(1, Ordering::Relaxed);
                        (405, "<html><script>setTimeout(() => { const url = new URL(location.href); url.searchParams.set('verified', '1'); location.replace(url.href); }, 80)</script></html>".to_string())
                    }
                } else {
                    (
                        200,
                        "<html><p>Waiting for verification</p></html>".to_string(),
                    )
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self {
            base,
            challenges,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for TopicServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn discovery_window(app: &AppHandle) -> WebviewWindow {
    app.webview_windows()
        .into_iter()
        .find(|(label, _)| label.starts_with("gitee-discovery-"))
        .map(|(_, window)| window)
        .expect("the production browser created its auxiliary window")
}

async fn evaluate(window: &WebviewWindow, script: &str) -> Result<String, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = Mutex::new(Some(sender));
    window
        .eval_with_callback(script, move |result| {
            if let Some(sender) = sender.lock().unwrap().take() {
                let _ = sender.send(result);
            }
        })
        .map_err(|error| error.to_string())?;
    tokio::time::timeout(Duration::from_secs(3), receiver)
        .await
        .map_err(|_| "evaluation timeout".to_string())?
        .map_err(|_| "evaluation dropped".to_string())
}

async fn assert_closed(app: &AppHandle) {
    for _ in 0..100 {
        if app
            .webview_windows()
            .keys()
            .all(|label| !label.starts_with("gitee-discovery-"))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("auxiliary webview leaked after completion or cancellation");
}

async fn verify_windows(app: &AppHandle, directory: &Path, live: bool) -> Result<Value, String> {
    let server = TopicServer::start();
    let first = Url::parse(&format!("{}/topic?page=1", server.base)).unwrap();
    let browser = TopicBrowser::open(
        app,
        first.clone(),
        directory.join("topic"),
        BrowserOptions {
            interaction_after: Duration::from_secs(10),
            page_timeout: Duration::from_secs(10),
        },
    )?;
    let html = browser.read_page(first, false).await?;
    assert!(html.contains("Overcooked-2-SkipLoading"));
    assert_eq!(server.challenges.load(Ordering::Relaxed), 1);
    let window = discovery_window(app);
    let runtime = evaluate(
        &window,
        "({ ua: navigator.userAgent, ipc: typeof window.__TAURI_INTERNALS__?.invoke })",
    )
    .await?;
    let runtime: Value = serde_json::from_str(&runtime).unwrap();
    assert_eq!(runtime["ipc"], "function");
    assert!(runtime["ua"].as_str().unwrap().contains("Windows"));
    evaluate(&window, "(() => { window.__probeOutcome = 'pending'; window.__TAURI_INTERNALS__.invoke('privileged_probe').then(() => { window.__probeOutcome = 'allowed'; }, () => { window.__probeOutcome = 'denied'; }); return true; })()").await?;
    let mut denied = false;
    for _ in 0..100 {
        let outcome = evaluate(&window, "window.__probeOutcome").await?;
        if outcome == r#""denied""# {
            denied = true;
            break;
        }
        assert_ne!(outcome, r#""allowed""#);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        denied,
        "remote IPC must receive an explicit permission denial"
    );
    assert_eq!(
        PRIVILEGED_CALLS.load(Ordering::Relaxed),
        0,
        "remote pages must not reach application commands"
    );
    let second = Url::parse(&format!("{}/topic?page=2", server.base)).unwrap();
    assert!(
        !browser
            .read_page(second, true)
            .await?
            .contains("Overcooked-2-SkipLoading")
    );
    drop(browser);
    assert_closed(app).await;

    let waiting = Url::parse(&format!("{}/waiting", server.base)).unwrap();
    let browser = TopicBrowser::open(
        app,
        waiting.clone(),
        directory.join("timeout"),
        BrowserOptions {
            interaction_after: Duration::from_millis(40),
            page_timeout: Duration::from_millis(700),
        },
    )?;
    let window = discovery_window(app);
    let read = browser.read_page(waiting.clone(), false);
    let shown = async {
        for _ in 0..50 {
            if window.is_visible().unwrap() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("verification that needs interaction must become visible");
    };
    let (timeout, _) = tokio::join!(read, shown);
    assert!(timeout.is_err());
    drop(browser);
    assert_closed(app).await;

    let browser = TopicBrowser::open(
        app,
        waiting.clone(),
        directory.join("cancel"),
        BrowserOptions {
            interaction_after: Duration::from_secs(5),
            page_timeout: Duration::from_secs(5),
        },
    )?;
    let window = discovery_window(app);
    let close = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        window.close().unwrap();
    };
    let started = Instant::now();
    let (cancelled, _) = tokio::join!(browser.read_page(waiting.clone(), false), close);
    assert!(cancelled.is_err() && started.elapsed() < Duration::from_secs(3));
    drop(browser);
    assert_closed(app).await;

    let interrupted = tokio::time::timeout(Duration::from_millis(100), async {
        let browser = TopicBrowser::open(
            app,
            waiting.clone(),
            directory.join("abort"),
            BrowserOptions {
                interaction_after: Duration::from_secs(5),
                page_timeout: Duration::from_secs(5),
            },
        )?;
        browser.read_page(waiting, false).await
    })
    .await;
    assert!(interrupted.is_err());
    assert_closed(app).await;

    let mut report = json!({
        "runtime": "Tauri/Wry Windows WebView2", "challenge_reload": true,
        "pagination": true, "remote_command_denied": true, "privileged_calls": 0,
        "interaction_window": true, "timeout_cleanup": true,
        "close_cancellation": true, "abort_cleanup": true, "live_requested": live,
    });
    if live {
        let topic = Url::parse("https://gitee.com/explore/topic/overcooked-2?page=1").unwrap();
        let browser = TopicBrowser::open(
            app,
            topic.clone(),
            directory.join("live-gitee"),
            Default::default(),
        )?;
        let html = browser.read_page(topic, false).await?;
        let repositories = {
            let document = scraper::Html::parse_fragment(&html);
            let selector =
                scraper::Selector::parse(".explore-repo__list-item .project-namespace-path")
                    .unwrap();
            document
                .select(&selector)
                .filter_map(|item| item.value().attr("href").map(str::to_string))
                .collect::<Vec<_>>()
        };
        assert!(
            !repositories.is_empty(),
            "the live topic contains the user-provided repository list"
        );
        report["live_topic"] = json!({ "status": "ready", "html_bytes": html.len(), "repository_links": repositories });
        drop(browser);
        assert_closed(app).await;
    }
    let waiting = Url::parse(&format!("{}/waiting", server.base)).unwrap();
    let browser = TopicBrowser::open(
        app,
        waiting.clone(),
        directory.join("main-close"),
        BrowserOptions {
            interaction_after: Duration::from_secs(5),
            page_timeout: Duration::from_secs(5),
        },
    )?;
    let main = app.get_webview_window("main").unwrap();
    let close = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        main.close().unwrap();
    };
    let started = Instant::now();
    let (cancelled, _) = tokio::join!(browser.read_page(waiting, false), close);
    assert!(
        cancelled.is_err() && started.elapsed() < Duration::from_secs(3),
        "closing the manager must not leave a hidden verification window alive"
    );
    drop(browser);
    assert_closed(app).await;
    report["main_close_cleanup"] = json!(true);
    Ok(report)
}

#[test]
#[ignore = "Requires an interactive Windows WebView2 desktop; opt into real Gitee with OC2_GITEE_LIVE_VERIFY=1"]
fn windows_webview_challenge_isolation_and_lifecycle() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/verification/gitee-webview-runtime")
        .join(format!(
            "run-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        ));
    std::fs::create_dir_all(&directory).unwrap();
    let live = std::env::var("OC2_GITEE_LIVE_VERIFY").as_deref() == Ok("1");
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let output = directory.clone();
    let exit_code = tauri::Builder::default()
        .any_thread()
        .on_window_event(gitee_webview::close_with_main)
        .invoke_handler(tauri::generate_handler![privileged_probe])
        .setup(move |app| {
            WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(Url::parse("about:blank").unwrap()),
            )
            .visible(false)
            .focused(false)
            .data_directory(directory.join("anchor"))
            .build()?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Always stop the event loop, including assertion panics and stalled WebViews.
                let result = match tokio::time::timeout(
                    Duration::from_secs(if live { 180 } else { 40 }),
                    AssertUnwindSafe(verify_windows(&handle, &directory, live)).catch_unwind(),
                )
                .await
                {
                    Ok(Ok(result)) => result,
                    Ok(Err(_)) => Err("native WebView assertion failed".into()),
                    Err(_) => Err("native WebView verification deadline exceeded".into()),
                };
                let code = i32::from(result.is_err());
                let _ = sender.send(result);
                handle.exit(code);
            });
            Ok(())
        })
        .build(context)
        .expect("build the isolated native webview test, not the manager")
        .run_return(|_, event| {
            // Keep the test process alive until assertions send an explicit exit code.
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
            }
        });
    let report = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    let evidence = match &report {
        Ok(value) => value.clone(),
        Err(error) => json!({"error":error, "live_requested":live}),
    };
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    println!(
        "Native WebView2 evidence: {}",
        output.join("report.json").display()
    );
    assert_eq!(exit_code, i32::from(report.is_err()));
    report.expect("native webview validation must finish successfully");
}
