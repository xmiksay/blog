//! A minimal in-process OpenAI-compatible SSE endpoint, so a test can point a
//! real `llm_providers.base_url` at loopback and let the production path —
//! `SiteCatalog::refresh` → `build_factory` → `entanglement_provider`'s
//! `OpenAiLlm` → the real chat-completions wire format — run end to end
//! without a network or a pulled model.
//!
//! Pulled in via `#[path = "common/llm_mock.rs"] mod llm_mock;` by
//! `tests/ai_catalog.rs` (endpoint concurrency) and
//! `tests/assistant_session_compact.rs` (the `/compact` summarize oneshot).
//! Each `tests/*.rs` is its own crate, so which items count as "used" differs
//! per binary — hence the blanket `dead_code` allow rather than two
//! near-identical copies.
//!
//! Deliberately *not* shared with `src/ai/catalog/tests.rs`'s own probe: that
//! one is an in-crate unit test and cannot reach a `tests/` module.
#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A running mock endpoint. The accept loop is detached and lives for the rest
/// of the test process — there is nothing to shut down.
pub struct OpenAiMock {
    /// Ready to drop straight into `llm_providers.base_url` (for a `kind` of
    /// `ollama` or `openai`, both of which route through `openai_factory`).
    pub base_url: String,
    /// High-water mark of simultaneously open request handlers, for tests
    /// asserting on the endpoint's concurrency gate.
    pub max_in_flight: Arc<AtomicUsize>,
}

/// Serve `reply` as the assistant's message text on every request, holding each
/// connection open for `delay` first (the window during which `max_in_flight`
/// can observe overlap).
///
/// The response is a complete three-frame stream — one `delta.content` chunk,
/// one `finish_reason: "stop"` chunk, then `[DONE]` — which is the minimum
/// `entanglement_provider`'s parser needs to yield `LlmEvent::Text` followed by
/// a non-ambiguous `LlmEvent::Finish`. A summarize oneshot concatenates exactly
/// those `Text` events into its summary, so `reply` is what a caller gets back
/// as `OutEvent::Compacted { summary, .. }`.
pub async fn spawn_openai_sse_mock(reply: &str, delay: Duration) -> OpenAiMock {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    let in_flight = Arc::new(AtomicUsize::new(0));
    let max_in_flight = Arc::new(AtomicUsize::new(0));
    let (max_seen_task, reply) = (max_in_flight.clone(), reply.to_string());

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let in_flight = in_flight.clone();
            let max_seen = max_seen_task.clone();
            let reply = reply.clone();
            tokio::spawn(async move {
                drain_request(&mut socket).await;

                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                max_seen.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(delay).await;
                in_flight.fetch_sub(1, Ordering::SeqCst);

                let body = sse_body(&reply);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });

    OpenAiMock {
        base_url: format!("http://{addr}"),
        max_in_flight,
    }
}

fn sse_body(reply: &str) -> String {
    let text = serde_json::json!({
        "choices": [{ "delta": { "content": reply }, "finish_reason": null }]
    });
    let stop = serde_json::json!({
        "choices": [{ "delta": {}, "finish_reason": "stop" }]
    });
    format!("data: {text}\n\ndata: {stop}\n\ndata: [DONE]\n\n")
}

/// Consume the request headers *and* their declared body before replying.
/// Answering early would leave the client mid-write on a socket that is about
/// to be shut down, which surfaces as a connection reset instead of a
/// response — and a real summarize prompt is far larger than one read.
async fn drain_request(socket: &mut TcpStream) {
    let mut buf: Vec<u8> = Vec::with_capacity(8192);
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(headers_end) = find(&buf, b"\r\n\r\n") {
            let body_start = headers_end + 4;
            if buf.len() - body_start >= content_length(&buf[..headers_end]) {
                return;
            }
        }
        match tokio::time::timeout(Duration::from_millis(1000), socket.read(&mut chunk)).await {
            Ok(Ok(n)) if n > 0 => buf.extend_from_slice(&chunk[..n]),
            // EOF, a read error, or a client that sent nothing further: reply
            // with whatever arrived rather than hanging the handler.
            _ => return,
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// The request's `Content-Length`, or `0` when absent/unparsable.
fn content_length(headers: &[u8]) -> usize {
    String::from_utf8_lossy(headers)
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().ok())
                .flatten()
        })
        .unwrap_or(0)
}
