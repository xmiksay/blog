//! Unit tests for `src/ai/mcp.rs`, split out to keep that file under the
//! 400-line cap.

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::net::TcpListener;

/// A minimal streamable-HTTP MCP server that answers `initialize`,
/// `notifications/initialized` and `tools/list` instantly, as plain
/// `application/json`. Returns its endpoint URL.
async fn spawn_mock_mcp_server() -> String {
    use axum::{Json, Router, routing::post};
    use serde_json::json;

    let app = Router::new().route(
        "/mcp",
        post(|Json(frame): Json<Value>| async move {
            let result = match frame.get("method").and_then(Value::as_str) {
                Some("initialize") => json!({ "protocolVersion": "2025-03-26" }),
                Some("tools/list") => json!({ "tools": [{ "name": "ping" }] }),
                _ => json!({}),
            };
            Json(json!({
                "jsonrpc": "2.0",
                "id": frame.get("id").cloned().unwrap_or(Value::Null),
                "result": result,
            }))
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}/mcp")
}

/// The endpoint pool's request pacing must stay off for MCP. Entanglement 0.6
/// moved the transport onto the provider pool, whose default 50 rpm forces
/// 1.2s between two requests to one endpoint — so an instantly-answering
/// server would still take ~1.2s to hand back a connection and ~2.4s to reach
/// `tools/list`, per server, sequentially, on every cache refresh. Drives the
/// real handshake + listing against a local mock and fails if that spacing is
/// ever back.
#[tokio::test]
async fn connect_and_list_tools_are_not_paced() {
    let url = spawn_mock_mcp_server().await;

    let started = tokio::time::Instant::now();
    let client = connect_with_timeout("mock", &url, &HashMap::new(), CONNECT_TIMEOUT)
        .await
        .expect("the mock server completes the handshake");
    let tools = client.list_tools().await.expect("tools/list");
    let elapsed = started.elapsed();

    assert_eq!(tools.len(), 1, "the mock server advertises one tool");
    assert!(
        elapsed < Duration::from_secs(1),
        "handshake + tools/list took {elapsed:?}; the pool's default pacing \
         (~2.4s to reach tools/list) looks to be back on"
    );
}

/// A remote MCP server that accepts the TCP connection but never answers
/// — the boot-latency failure mode issue #28 calls out. Proves
/// `connect_with_timeout` returns in bounded time instead of hanging for
/// as long as the underlying HTTP client's own (much longer) per-request
/// timeout.
#[tokio::test]
async fn connect_with_timeout_bounds_a_server_that_never_answers() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        // Accept and hold every connection open without ever writing a
        // response — a black hole, not a refusal.
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            std::mem::forget(socket); // keep the fd open for the test's duration
        }
    });

    let bound = Duration::from_millis(200);
    let started = tokio::time::Instant::now();
    let result = connect_with_timeout(
        "black-hole",
        &format!("http://{addr}/mcp"),
        &HashMap::new(),
        bound,
    )
    .await;
    let elapsed = started.elapsed();

    assert!(
        result.is_err(),
        "a server that never answers must not connect"
    );
    assert!(
        elapsed < bound * 5,
        "connect_with_timeout took {elapsed:?}, expected roughly the {bound:?} bound"
    );
}

/// Expiring must *abort* the spawned request, not merely drop its
/// `JoinHandle` — dropping a handle detaches, which would leave a dead
/// server's request (and the endpoint lease it renews on a heartbeat)
/// running for the life of the process. Asserts the abandoned future is
/// really torn down, via a guard that flags its own `Drop`.
#[tokio::test]
async fn bounded_aborts_the_abandoned_request_instead_of_detaching_it() {
    struct DropFlag(Arc<AtomicBool>);
    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let dropped = Arc::new(AtomicBool::new(false));
    let flag = DropFlag(dropped.clone());

    let err = bounded(Duration::from_millis(50), "stuck".to_string(), async move {
        let _flag = flag;
        // Far longer than any plausible test runtime: only cancellation ends it.
        tokio::time::sleep(Duration::from_secs(3600)).await;
        Ok(())
    })
    .await
    .expect_err("a request that never completes must surface as an error");
    assert!(
        err.to_string().contains("timed out"),
        "unexpected error: {err}"
    );

    // The abort lands on a later scheduler pass, so yield until the guard's
    // `Drop` runs; the outer bound turns a detached (never-torn-down) task
    // into a failure instead of a hang.
    tokio::time::timeout(Duration::from_secs(5), async {
        while !dropped.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the abandoned request must be aborted, not left running detached");
}
