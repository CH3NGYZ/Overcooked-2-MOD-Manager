//! Shared diagnostics for application HTTP, after API admission where applicable.
//! Status handling, retries, redirects and cancellation remain with their owners.
mod redaction;
pub(crate) use redaction::redact_text;

use bytes::{Bytes, BytesMut};
use http_body::{Body, Frame, SizeHint};
use redaction::Redactor;
use reqwest::{ResponseBuilderExt, StatusCode};
use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

const MAX_BODY_BYTES: usize = 64 * 1024;
const BODY_PREVIEW_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone)]
struct SafeBodyPreview(String);

/// Reuse the same request-aware redaction for short owner-specific errors.
pub(crate) fn body_preview(response: &reqwest::Response) -> Option<&str> {
    response
        .extensions()
        .get::<SafeBodyPreview>()
        .map(|preview| preview.0.as_str())
}

pub(crate) trait HttpRequestExt {
    async fn send_logged(self) -> Result<reqwest::Response, reqwest::Error>;
}

impl HttpRequestExt for reqwest::RequestBuilder {
    async fn send_logged(self) -> Result<reqwest::Response, reqwest::Error> {
        let (client, request) = self.build_split();
        execute(client, request?).await
    }
}

pub(crate) async fn execute(
    client: reqwest::Client,
    request: reqwest::Request,
) -> Result<reqwest::Response, reqwest::Error> {
    let redactor = Redactor::new(&request);
    let method = request.method().clone();
    let address = crate::repository_sources::safe_log_address(request.url().as_str());
    // Request bodies can be one-shot upload streams; never poll or clone them.
    let request_body = redactor.request_body(&request);
    match client.execute(request).await {
        Ok(response) => Ok(inspect(response, method, address, request_body, redactor).await),
        Err(error) => {
            log::warn!(
                "HTTP 请求未收到响应：方法={}，地址={:?}，超时={}，连接错误={}",
                method,
                address,
                error.is_timeout(),
                error.is_connect()
            );
            Err(error)
        }
    }
}

async fn inspect(
    response: reqwest::Response,
    method: reqwest::Method,
    address: String,
    request_body: String,
    mut redactor: Redactor,
) -> reqwest::Response {
    if response.status() == StatusCode::OK {
        return response;
    }
    let status = response.status();
    let final_url = response.url().clone();
    redactor.include_url(&final_url);
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|header| header.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let context = Diagnostic {
        method: method.to_string(),
        address,
        final_address: crate::repository_sources::safe_log_address(final_url.as_str()),
        status,
        content_type,
        length: response.content_length(),
        request_body,
        redactor,
        sample: Vec::new(),
    };
    // Range responses are file streams. Inspecting them eagerly would change
    // first-byte timing and downloads. Their metadata is enough for diagnostics.
    if method == reqwest::Method::HEAD
        || matches!(status.as_u16(), 204 | 206 | 304)
        || (status.is_success() && context.binary_content())
    {
        context.emit(
            if method == reqwest::Method::HEAD || matches!(status.as_u16(), 204 | 304) {
                "无响应正文"
            } else {
                "二进制或分片正文，未预读"
            },
            false,
        );
        return response;
    }
    let raw: http::Response<reqwest::Body> = response.into();
    let (parts, inner) = raw.into_parts();
    let remaining = inner.size_hint();
    let mut body = PreviewBody {
        inner: Box::pin(inner),
        replay: VecDeque::new(),
        buffered: BytesMut::new(),
        ended: false,
        finished: false,
        remaining,
        safe_preview: None,
        diagnostic: Some(context),
    };
    body.prefetch(BODY_PREVIEW_TIMEOUT).await;
    let safe_preview = body.safe_preview.take();
    // Reqwest's conversion into http::Response does NOT retain its private URL
    // field. Restore it explicitly, and keep every original HTTP extension.
    let mut rebuilt = http::Response::builder()
        .url(final_url)
        .body(reqwest::Body::wrap(body))
        .expect("valid HTTP response");
    *rebuilt.status_mut() = parts.status;
    *rebuilt.version_mut() = parts.version;
    *rebuilt.headers_mut() = parts.headers;
    rebuilt.extensions_mut().extend(parts.extensions);
    if let Some(preview) = safe_preview {
        rebuilt.extensions_mut().insert(SafeBodyPreview(preview));
    }
    reqwest::Response::from(rebuilt)
}

struct Diagnostic {
    method: String,
    address: String,
    final_address: String,
    status: StatusCode,
    content_type: String,
    length: Option<u64>,
    request_body: String,
    redactor: Redactor,
    sample: Vec<u8>,
}

impl Diagnostic {
    fn binary_content(&self) -> bool {
        self.content_type.starts_with("image/")
            || self.content_type.starts_with("audio/")
            || self.content_type.starts_with("video/")
            || matches!(
                self.content_type.as_str(),
                "application/zip" | "application/octet-stream" | "application/pdf"
            )
    }

    fn observe(&mut self, frame: &Frame<Bytes>) {
        if let Some(data) = frame.data_ref() {
            let available = MAX_BODY_BYTES.saturating_sub(self.sample.len());
            self.sample
                .extend_from_slice(&data[..data.len().min(available)]);
        }
    }

    fn emit(self, state: &str, truncated: bool) -> String {
        let body = self
            .redactor
            .body(&self.sample, &self.content_type, truncated);
        let message = format!(
            "HTTP 非200响应：方法={}，地址={:?}，最终地址={:?}，状态={}，类型={:?}，长度={:?}，请求体={}，响应体={}，正文状态={}",
            self.method,
            self.address,
            self.final_address,
            self.status.as_u16(),
            self.content_type,
            self.length,
            self.request_body,
            body,
            state
        );
        if self.status.is_success() || self.status.is_redirection() {
            log::debug!("{message}");
        } else {
            log::warn!("{message}");
        }
        body
    }
}

/// Replays prefetched data before the remaining original body. Retaining nondata
/// frames preserves trailers and read errors; size bounds track owner consumption.
struct PreviewBody {
    inner: Pin<Box<reqwest::Body>>,
    replay: VecDeque<Result<Frame<Bytes>, reqwest::Error>>,
    buffered: BytesMut,
    ended: bool,
    finished: bool,
    remaining: SizeHint,
    safe_preview: Option<String>,
    diagnostic: Option<Diagnostic>,
}

impl PreviewBody {
    fn flush_buffer(&mut self) {
        if !self.buffered.is_empty() {
            self.replay
                .push_back(Ok(Frame::data(self.buffered.split().freeze())));
        }
    }

    fn remember_frame(&mut self, frame: Result<Frame<Bytes>, reqwest::Error>) {
        match frame {
            Ok(frame) => match frame.into_data() {
                // Aggregate small preview frames. Otherwise polling the original
                // deadline during replay could replace many tiny cached frames
                // with as many large frames and grow the replay buffer.
                Ok(data) if self.buffered.len() + data.len() <= MAX_BODY_BYTES => {
                    self.buffered.extend_from_slice(&data);
                }
                Ok(data) => {
                    self.flush_buffer();
                    self.replay.push_back(Ok(Frame::data(data)));
                }
                Err(frame) => {
                    self.flush_buffer();
                    self.replay.push_back(Ok(frame));
                }
            },
            Err(error) => {
                self.flush_buffer();
                self.replay.push_back(Err(error));
            }
        }
    }

    async fn prefetch(&mut self, timeout: Duration) {
        let read = async {
            loop {
                let next = std::future::poll_fn(|cx| self.inner.as_mut().poll_frame(cx)).await;
                let state = match &next {
                    None => {
                        self.ended = true;
                        Some(("完整", false))
                    }
                    Some(Err(_)) => {
                        self.ended = true;
                        Some(("读取失败，保留原始错误", true))
                    }
                    Some(Ok(frame)) => {
                        let diagnostic = self.diagnostic.as_mut().expect("active diagnostic");
                        diagnostic.observe(frame);
                        (diagnostic.sample.len() >= MAX_BODY_BYTES)
                            .then_some(("达到64KiB预览上限", true))
                    }
                };
                if let Some(frame) = next {
                    self.remember_frame(frame);
                }
                if let Some(state) = state {
                    return state;
                }
            }
        };
        let (state, truncated) = tokio::time::timeout(timeout, read)
            .await
            .unwrap_or(("预览读取超时，保留原始流", true));
        self.flush_buffer();
        if let Some(diagnostic) = self.diagnostic.take() {
            self.safe_preview = Some(diagnostic.emit(state, truncated));
        }
    }
}

impl Body for PreviewBody {
    type Data = Bytes;
    type Error = reqwest::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        if self.finished {
            return Poll::Ready(None);
        }
        // Reqwest checks its original absolute deadline before polling network
        // frames, even after EOF. Buffered diagnostics must keep that check for
        // consumers which wait before reading the returned response.
        match self.inner.as_mut().poll_frame(cx) {
            Poll::Ready(Some(Err(error))) if error.is_timeout() => {
                self.replay.clear();
                self.finished = true;
                return Poll::Ready(Some(Err(error)));
            }
            Poll::Ready(Some(frame)) if !self.ended => {
                if frame.is_err() {
                    self.ended = true;
                }
                self.replay.push_back(frame);
            }
            Poll::Ready(None) => self.ended = true,
            _ => {}
        }
        if let Some(frame) = self.replay.pop_front() {
            if let Ok(frame) = &frame {
                if let Some(data) = frame.data_ref() {
                    let size = data.len() as u64;
                    let mut remaining = SizeHint::new();
                    remaining.set_lower(self.remaining.lower().saturating_sub(size));
                    if let Some(upper) = self.remaining.upper() {
                        remaining.set_upper(upper.saturating_sub(size));
                    }
                    self.remaining = remaining;
                }
            } else {
                self.finished = true;
            }
            return Poll::Ready(Some(frame));
        }
        if self.ended {
            self.finished = true;
            return Poll::Ready(None);
        }
        Poll::Pending
    }

    fn is_end_stream(&self) -> bool {
        self.finished || (self.replay.is_empty() && (self.ended || self.inner.is_end_stream()))
    }

    fn size_hint(&self) -> SizeHint {
        // Previews are not consumption by the owner. Preserve the original
        // bounds, including unknown and truncated advertised lengths, and only
        // subtract frames actually handed to the downstream reader.
        self.remaining.clone()
    }
}

impl Drop for PreviewBody {
    fn drop(&mut self) {
        if let Some(diagnostic) = self.diagnostic.take() {
            diagnostic.emit("请求已取消，预览未完成", true);
        }
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod transport_tests;
