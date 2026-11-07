use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

pub(in crate::community) fn item(owner: &str, name: &str) -> Value {
    serde_json::json!({"owner": {"login": owner}, "name": name, "private": false,
            "topics": ["overcooked-2", "bepinex", "mod"], "default_branch": "develop"})
}

pub(in crate::community) fn page(items: Vec<Value>, total: u64) -> String {
    serde_json::json!({"items": items, "total_count": total, "incomplete_results": false})
        .to_string()
}

pub(in crate::community) fn server(
    responses: Vec<(u16, String, String)>,
) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, headers, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut chunk = [0; 2048];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
            }
            requests.push(String::from_utf8(request).unwrap());
            write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len()).unwrap();
        }
        requests
    });
    (url, task)
}

pub(in crate::community) fn item_with_topics(owner: &str, name: &str, topics: &[&str]) -> Value {
    serde_json::json!({"owner": {"login": owner}, "name": name, "private": false,
            "topics": topics, "default_branch": "develop"})
}
