use super::transport::API_REQUEST_TIMEOUT;
use crate::releases::RepositoryRequest;
use reqwest::Client;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct ApiTestServer {
    pub(super) url: String,
    requests: Arc<Mutex<Vec<String>>>,
    stopped: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl ApiTestServer {
    pub(super) fn new(responses: Vec<(&'static str, &'static str)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stopped = Arc::new(AtomicBool::new(false));
        let server_requests = requests.clone();
        let server_stopped = stopped.clone();
        let worker = std::thread::spawn(move || {
            let mut index = 0;
            while !server_stopped.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("test server accept failed: {error}"),
                };
                // Windows accepted sockets inherit the listener's mode.
                stream.set_nonblocking(false).unwrap();
                stream.set_read_timeout(Some(API_REQUEST_TIMEOUT)).unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                    let count = stream.read(&mut buffer).unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..count]);
                }
                server_requests
                    .lock()
                    .unwrap()
                    .push(String::from_utf8(request).unwrap());
                // Unexpected extra requests also get a response, so a failed
                // assertion reports the actual count instead of hanging.
                let (status, body) = responses
                    .get(index)
                    .copied()
                    .unwrap_or(("500 Internal Server Error", ""));
                index += 1;
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        Self {
            url: format!("http://{address}/releases"),
            requests,
            stopped,
            worker: Some(worker),
        }
    }

    pub(super) fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for ApiTestServer {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}

pub(super) fn api_test_client() -> Client {
    Client::builder().no_proxy().build().unwrap()
}

pub(super) fn api_test_repository() -> RepositoryRequest {
    serde_json::from_value(serde_json::json!({
        "id": "retry-test", "name": "Retry test", "provider": "gitee",
        "username": "owner", "repository": "repo", "install_target": "BepInEx/plugins",
        "enabled": true, "builtin": false
    }))
    .unwrap()
}
