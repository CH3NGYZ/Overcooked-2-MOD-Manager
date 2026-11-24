#![cfg(target_os = "windows")]

#[path = "gitee-webview/archive.rs"]
mod archive;
#[path = "../src/gitee_webview.rs"]
mod gitee_webview;
#[path = "../src/main_webview.rs"]
mod main_webview;
#[path = "../src/webview_download.rs"]
mod webview_download;

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
use tauri::{AppHandle, Manager, WebviewWindow, utils::config::FrontendDist};
use url::Url;

static PRIVILEGED_CALLS: AtomicUsize = AtomicUsize::new(0);

#[tauri::command]
fn privileged_probe() {
    PRIVILEGED_CALLS.fetch_add(1, Ordering::Relaxed);
}

struct TopicServer {
    base: String,
    requests: Arc<Mutex<Vec<(String, Vec<String>)>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl TopicServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = stop.clone();
        let thread_requests = requests.clone();
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
                let user_agents = request
                    .lines()
                    .skip(1)
                    .take_while(|line| !line.is_empty())
                    .filter_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("User-Agent")
                            .then(|| value.trim().to_owned())
                    })
                    .collect();
                thread_requests
                    .lock()
                    .unwrap()
                    .push((path.to_owned(), user_agents));
                let (status, content_type, body) = if path == "/" {
                    (
                        200,
                        "text/html; charset=utf-8",
                        "<!doctype html><img src='/ua-probe.svg' alt=''>",
                    )
                } else if path == "/ua-probe.svg" {
                    (
                        200,
                        "image/svg+xml",
                        "<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/>",
                    )
                } else if path.starts_with("/topic") {
                    let content_type = "text/html; charset=utf-8";
                    if path.contains("page=2") {
                        (200, content_type, "<div class='explore-repo__list'></div>")
                    } else if path.contains("verified=1") {
                        (
                            200,
                            content_type,
                            include_str!("../../tests/fixtures/gitee-topic.html"),
                        )
                    } else {
                        (
                            405,
                            content_type,
                            "<html><script>setTimeout(() => { const url = new URL(location.href); url.searchParams.set('verified', '1'); location.replace(url.href); }, 80)</script></html>",
                        )
                    }
                } else {
                    (
                        200,
                        "text/html; charset=utf-8",
                        "<html><p>Waiting for verification</p></html>",
                    )
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self {
            base,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    fn user_agents(&self, path: &str) -> Vec<Vec<String>> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(request_path, _)| request_path == path)
            .map(|(_, user_agents)| user_agents.clone())
            .collect()
    }

    fn assert_browser_user_agent(&self, path: &str) -> Value {
        let recorded = self.user_agents(path);
        assert!(!recorded.is_empty(), "no request received for {path}");
        for headers in &recorded {
            assert_eq!(headers.len(), 1, "expected one User-Agent for {path}");
            let user_agent = &headers[0];
            assert!(
                user_agent.starts_with("Mozilla/5.0 ")
                    && user_agent.contains("Windows")
                    && !user_agent.contains("Overcooked 2 MOD Manager/"),
                "expected a native Windows browser User-Agent for {path}, got {user_agent:?}"
            );
        }
        json!({ "requests": recorded.len(), "user_agent": recorded[0][0] })
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
    let challenge = server.assert_browser_user_agent("/topic?page=1");
    assert_eq!(challenge["requests"], 1);
    let reload = server.assert_browser_user_agent("/topic?page=1&verified=1");
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
    let next_page = server.assert_browser_user_agent("/topic?page=2");
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
        "server_user_agent": { "challenge": challenge, "reload": reload, "next_page": next_page },
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
    run_native_validation(false);
}

#[test]
#[ignore = "One interactive Gitee source-ZIP download; requires OC2_GITEE_LIVE_ARCHIVE=1 and user-ready verification"]
fn windows_live_gitee_archive_download_and_autoclose() {
    assert_eq!(
        std::env::var("OC2_GITEE_LIVE_ARCHIVE").as_deref(),
        Ok("1"),
        "the live source-ZIP case requires explicit opt-in"
    );
    archive::install_logger();
    run_native_validation(true);
}

fn run_native_validation(archive_download: bool) {
    let started = Instant::now();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/verification/gitee-webview-runtime")
        .join(format!(
            "{}-{}-{}",
            if archive_download { "archive" } else { "topic" },
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            std::process::id()
        ));
    std::fs::create_dir_all(&directory).unwrap();
    let live = std::env::var("OC2_GITEE_LIVE_VERIFY").as_deref() == Ok("1");
    let main_server = TopicServer::start();
    let main_url = Url::parse(&main_server.base).unwrap();
    let mut context = tauri::generate_context!();
    context.config_mut().build.dev_url = Some(main_url.clone());
    context.config_mut().build.frontend_dist = Some(FrontendDist::Url(main_url));
    let main = context.config_mut().app.windows.first_mut().unwrap();
    main.visible = false;
    main.focus = false;
    main.data_directory = Some(directory.join(if archive_download {
        "profile"
    } else {
        "anchor"
    }));
    context.config_mut().identifier =
        format!("com.ch3ngyz.oc2.validation.run-{}", uuid::Uuid::new_v4());
    let identifier = context.config().identifier.clone();
    // This also isolates production webviews that do not set data_directory.
    context.config_mut().app.app_directories_override =
        Some(serde_json::from_value(json!(directory.join("profile"))).unwrap());
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let output = directory.clone();
    let exit_code = tauri::Builder::default()
        .any_thread()
        .on_window_event(|window, event| {
            gitee_webview::close_with_main(window, event);
            webview_download::close_with_main(window, event);
        })
        .invoke_handler(tauri::generate_handler![privileged_probe])
        .setup(move |app| {
            main_webview::create(app)?;
            assert_eq!(app.path().app_local_data_dir()?, directory.join("profile"));
            assert!(!app.get_webview_window("main").unwrap().is_visible()?);
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Always stop the event loop, including assertion panics and stalled WebViews.
                let result = match tokio::time::timeout(
                    Duration::from_secs(if archive_download {
                        120
                    } else if live {
                        180
                    } else {
                        40
                    }),
                    AssertUnwindSafe(async {
                        if archive_download {
                            archive::verify(&handle, &directory).await
                        } else {
                            for _ in 0..500 {
                                if !main_server.user_agents("/ua-probe.svg").is_empty() {
                                    break;
                                }
                                tokio::time::sleep(Duration::from_millis(20)).await;
                            }
                            let main_page = main_server.assert_browser_user_agent("/");
                            let main_image = main_server.assert_browser_user_agent("/ua-probe.svg");
                            let mut report = verify_windows(&handle, &directory, live).await?;
                            report["server_user_agent"]["main_page"] = main_page;
                            report["server_user_agent"]["main_image"] = main_image;
                            Ok(report)
                        }
                    })
                    .catch_unwind(),
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
    let mut evidence = match &report {
        Ok(value) => value.clone(),
        Err(error) => json!({"error":error}),
    };
    evidence["elapsed_ms"] = json!(started.elapsed().as_millis());
    evidence["live_requested"] = json!(archive_download || live);
    evidence["scenario"] = json!(if archive_download {
        "live-gitee-source-zip"
    } else {
        "community-topic"
    });
    evidence["isolated_profile"] = json!(output.join("profile"));
    evidence["identifier"] = json!(identifier);
    evidence["profile_isolation_checked"] = json!(true);
    evidence["anchor_hidden"] = json!(true);
    if archive_download {
        evidence["download_events"] = json!(archive::events());
        evidence["acceptance_inputs"] = archive::inputs();
    }
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
