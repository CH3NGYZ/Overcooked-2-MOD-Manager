//! Startup diagnostics use the same bounded source reader, without loading children.
use super::{DEFAULT_INDEX_GITHUB_URL, SourceLocation, load_document};
use crate::config::ConfigError;
use std::sync::Mutex;
use std::time::Duration;

static ACTIVE: Mutex<Option<tokio::sync::watch::Sender<bool>>> = Mutex::new(None);

#[derive(serde::Serialize)]
pub(crate) struct StartupProbe {
    pub reachable: bool,
    pub failure_kind: Option<&'static str>,
}

impl StartupProbe {
    fn failed(kind: &'static str) -> Self {
        Self {
            reachable: false,
            failure_kind: Some(kind),
        }
    }
}

pub(crate) fn cancel_startup_probe() {
    if let Some(sender) = ACTIVE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = sender.send(true);
    }
}

pub(crate) async fn probe_startup_source() -> StartupProbe {
    let (sender, mut cancelled) = tokio::sync::watch::channel(false);
    {
        let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        if active.is_some() {
            return StartupProbe::failed("busy");
        }
        *active = Some(sender);
    }
    let location =
        SourceLocation::parse(DEFAULT_INDEX_GITHUB_URL).expect("official source URL must be valid");
    let _guard = ActiveGuard;
    let result = probe_location(&location, Duration::from_secs(10), &mut cancelled).await;
    // Never log raw errors: they can carry URL queries or response content.
    log::info!(
        "启动源预检：可达={}，分类={:?}",
        result.reachable,
        result.failure_kind
    );
    result
}

struct ActiveGuard;
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        ACTIVE.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
}

async fn probe_location(
    location: &SourceLocation,
    deadline: Duration,
    cancelled: &mut tokio::sync::watch::Receiver<bool>,
) -> StartupProbe {
    let inherited = Default::default();
    tokio::select! {
        biased;
        _ = cancelled.changed() => StartupProbe::failed("cancelled"),
        result = tokio::time::timeout(deadline, load_document(location, &inherited)) => {
            match result {
                Err(_) => StartupProbe::failed("timeout"),
                Ok(Ok(_)) => StartupProbe { reachable: true, failure_kind: None },
                Ok(Err(error)) => StartupProbe::failed(failure_kind(&error)),
            }
        }
    }
}

fn failure_kind(error: &ConfigError) -> &'static str {
    match error {
        ConfigError::Read(message) if message.starts_with("HTTP ") => "http",
        ConfigError::Read(_) => "network",
        ConfigError::Invalid(message) if message == "source.tooLarge" => "too_large",
        _ => "invalid_content",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serve(
        status: &str,
        body: Vec<u8>,
        delay: Duration,
    ) -> (SourceLocation, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let status = status.to_owned();
        let thread = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                if let Ok((socket, _)) = listener.accept() {
                    break socket;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                assert_eq!(socket.read(&mut byte).unwrap(), 1);
                request.push(byte[0]);
            }
            assert!(
                !String::from_utf8_lossy(&request)
                    .to_lowercase()
                    .contains("authorization:")
            );
            assert!(
                !String::from_utf8_lossy(&request)
                    .to_lowercase()
                    .contains("access_token")
            );
            std::thread::sleep(delay);
            let header = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            if socket.write_all(header.as_bytes()).is_ok() {
                let _ = socket.write_all(&body);
            }
        });
        (
            SourceLocation::parse(&format!("http://{address}/source.toml")).unwrap(),
            thread,
        )
    }

    #[tokio::test]
    async fn probe_uses_real_source_validation_and_classifies_failures() {
        let valid = b"[[repository_sources]]\nid = \"test\"\nnickname_i18n.zh-CN = \"Test\"\nurl = \"child.toml\"\n".to_vec();
        for (status, body, expected) in [
            ("200 OK", valid, None),
            ("403 Forbidden", Vec::new(), Some("http")),
            ("304 Not Modified", Vec::new(), Some("http")),
            ("200 OK", Vec::new(), Some("invalid_content")),
            (
                "200 OK",
                b"<html>Login required</html>".to_vec(),
                Some("invalid_content"),
            ),
            ("200 OK", vec![0xff], Some("invalid_content")),
            (
                "200 OK",
                vec![b'x'; super::super::transport::MAX_SOURCE_BYTES + 1],
                Some("too_large"),
            ),
        ] {
            let (location, server) = serve(status, body, Duration::ZERO);
            let (_sender, mut cancelled) = tokio::sync::watch::channel(false);
            let result = probe_location(&location, Duration::from_secs(3), &mut cancelled).await;
            assert_eq!(result.failure_kind, expected);
            assert_eq!(result.reachable, expected.is_none());
            server.join().unwrap();
        }
    }

    #[tokio::test]
    async fn deadline_stops_pending_response_and_cancellation_settles_the_request() {
        let (location, server) = serve("200 OK", Vec::new(), Duration::from_millis(250));
        let (_sender, mut cancelled) = tokio::sync::watch::channel(false);
        let started = std::time::Instant::now();
        let result = probe_location(&location, Duration::from_millis(100), &mut cancelled).await;
        assert_eq!(result.failure_kind, Some("timeout"));
        assert!(started.elapsed() < Duration::from_secs(2));
        server.join().unwrap();

        let (location, server) = serve("200 OK", Vec::new(), Duration::from_millis(250));
        let (sender, mut cancelled) = tokio::sync::watch::channel(false);
        let cancel = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            sender.send(true).unwrap();
        };
        let (result, ()) = tokio::join!(
            probe_location(&location, Duration::from_secs(3), &mut cancelled),
            cancel,
        );
        assert_eq!(result.failure_kind, Some("cancelled"));
        server.join().unwrap();
    }

    #[test]
    fn diagnostics_do_not_expose_error_details() {
        for (error, expected) in [
            (ConfigError::Read("HTTP 403".into()), "http"),
            (ConfigError::Read("secret-bearing URL".into()), "network"),
            (ConfigError::Invalid("source.tooLarge".into()), "too_large"),
            (
                ConfigError::Parse("invalid UTF-8".into()),
                "invalid_content",
            ),
            (
                ConfigError::Invalid("source.emptyDocument".into()),
                "invalid_content",
            ),
        ] {
            assert_eq!(failure_kind(&error), expected);
        }
    }
}
