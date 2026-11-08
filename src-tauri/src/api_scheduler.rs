//! One sliding-window admission queue per provider, shared by all API callers.
//! Official, proxy, pagination and retries consume the same budget. Waiting is
//! outside request timeouts. Cancellation removes its ticket immediately.
use crate::models::Provider;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::Instant;

#[derive(Default)]
struct Queue {
    next: u64,
    waiting: Vec<(u64, String)>,
    started: VecDeque<Instant>,
}

struct Scheduler {
    queue: Mutex<Queue>,
    changed: Notify,
    window: Duration,
}

fn installed() -> &'static Mutex<HashMap<&'static str, HashSet<String>>> {
    static KEYS: OnceLock<Mutex<HashMap<&'static str, HashSet<String>>>> = OnceLock::new();
    KEYS.get_or_init(Default::default)
}

pub(crate) fn set_installed(domain: &'static str, keys: impl IntoIterator<Item = String>) {
    installed()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(domain, keys.into_iter().collect());
    for provider in [Provider::Github, Provider::Gitee, Provider::Gitcode] {
        scheduler(&provider).changed.notify_waiters();
    }
}

pub(crate) fn repository_key(provider: &Provider, owner: &str, repo: &str) -> String {
    format!(
        "{provider:?}/{}/{}",
        owner.trim().to_lowercase(),
        repo.trim().to_lowercase()
    )
}

pub(crate) fn is_installed(provider: &Provider, owner: &str, repo: &str) -> bool {
    let key = repository_key(provider, owner, repo);
    installed()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .any(|set| set.contains(&key))
}

fn request_key(provider: &Provider, url: &url::Url) -> String {
    let parts: Vec<_> = url.path().split('/').collect();
    parts
        .windows(3)
        .find(|p| p[0] == "repos")
        .map(|p| {
            // Decode path escapes without treating a literal '+' or '&' as form syntax.
            let decode = |segment: &str| {
                let encoded = format!("value={}", segment.replace('&', "%26").replace('+', "%2B"));
                url::form_urlencoded::parse(encoded.as_bytes())
                    .next()
                    .unwrap()
                    .1
                    .into_owned()
            };
            repository_key(provider, &decode(p[1]), &decode(p[2]))
        })
        .unwrap_or_default()
}

fn scheduler(provider: &Provider) -> &'static Arc<Scheduler> {
    static QUEUES: OnceLock<[Arc<Scheduler>; 3]> = OnceLock::new();
    &QUEUES.get_or_init(|| {
        std::array::from_fn(|_| {
            Arc::new(Scheduler {
                queue: Mutex::new(Queue::default()),
                changed: Notify::new(),
                window: Duration::from_secs(1),
            })
        })
    })[match provider {
        Provider::Github => 0,
        Provider::Gitee => 1,
        Provider::Gitcode => 2,
    }]
}

struct Ticket<'a> {
    scheduler: &'a Scheduler,
    id: u64,
}
impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        self.scheduler
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .waiting
            .retain(|(id, _)| *id != self.id);
        self.scheduler.changed.notify_waiters();
    }
}

impl Scheduler {
    async fn acquire(&self, key: String) {
        let ticket = {
            let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
            q.next += 1;
            let id = q.next;
            q.waiting.push((id, key));
            Ticket {
                scheduler: self,
                id,
            }
        };
        loop {
            // Register before inspecting the queue, avoiding lost wakeups.
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let wake = {
                let keys = installed().lock().unwrap_or_else(|e| e.into_inner());
                let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
                let now = Instant::now();
                while q
                    .started
                    .front()
                    .is_some_and(|at| now.duration_since(*at) >= self.window)
                {
                    q.started.pop_front();
                }
                let first = q
                    .waiting
                    .iter()
                    .min_by_key(|(id, key)| (!keys.values().any(|set| set.contains(key)), *id))
                    .map(|(id, _)| *id);
                if first == Some(ticket.id) && q.started.len() < 3 {
                    q.waiting.retain(|(id, _)| *id != ticket.id);
                    q.started.push_back(now);
                    return;
                }
                q.started
                    .front()
                    .map(|at| *at + self.window)
                    .unwrap_or(now + self.window)
            };
            tokio::select! { _ = notified => {}, _ = tokio::time::sleep_until(wake) => {} }
        }
    }
}

pub(crate) trait ApiRequestExt {
    async fn send_api(self, provider: Provider) -> Result<reqwest::Response, reqwest::Error>;
}

impl ApiRequestExt for reqwest::RequestBuilder {
    async fn send_api(self, provider: Provider) -> Result<reqwest::Response, reqwest::Error> {
        let (client, request) = self.build_split();
        let request = request?;
        scheduler(&provider)
            .acquire(request_key(&provider, request.url()))
            .await;
        client.execute(request).await
    }
}

#[cfg(test)]
mod tests;
