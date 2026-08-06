//! A minimal in-process `mdcast-server` stand-in, so export tests can point
//! `Config::mdcast_url` at loopback and let the production path —
//! `export::render_page` → `build_bundle` → `mdcast-client`'s
//! `409 → upload → retry` negotiation — run end to end without a real
//! render server (which would drag typst/pandoc back into the test bill).
//!
//! Speaks just enough of the wire protocol (`mdcast_api::wire`):
//! `GET /v1/capabilities`, `POST /v1/render` (canned `%PDF`/`<html>` bytes
//! keyed on the request's `target`), `POST /v1/blobs` (accepted, counted).
//! In negotiation mode the first render answers `409` naming one manifest
//! entry, which is exactly what forces a digest-only bundle entry to fetch
//! its bytes from `file_blobs`.
//!
//! Pulled in via `#[path = "common/mdcast_mock.rs"] mod mdcast_mock;` —
//! same convention as `common/llm_mock.rs`, and like it shared across test
//! binaries that each use a different subset, hence the `dead_code` allow.
#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};

pub const PDF_MAGIC: &[u8] = b"%PDF-1.7\n% mdcast-mock canned artifact\n";
pub const HTML_CANNED: &str = "<html><body>mdcast-mock reveal deck</body></html>";

/// A running mock server. The serve task is detached and lives for the rest
/// of the test process — there is nothing to shut down.
pub struct MdcastMock {
    /// Ready to drop straight into `Config::mdcast_url`.
    pub base_url: String,
    /// Number of `POST /v1/render` requests seen (negotiation mode answers
    /// the first with a `409`, so a cold-cache export counts 2).
    pub render_calls: Arc<AtomicUsize>,
    /// Number of `POST /v1/blobs` uploads seen.
    pub blob_uploads: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct MockState {
    negotiate: bool,
    render_calls: Arc<AtomicUsize>,
    blob_uploads: Arc<AtomicUsize>,
}

/// With `negotiate: false` every render succeeds immediately; with `true`
/// the first render `409`s with one blob picked from the request's own
/// manifest and the retry succeeds.
pub async fn spawn_mdcast_mock(negotiate: bool) -> MdcastMock {
    let state = MockState {
        negotiate,
        render_calls: Arc::new(AtomicUsize::new(0)),
        blob_uploads: Arc::new(AtomicUsize::new(0)),
    };
    let (render_calls, blob_uploads) = (state.render_calls.clone(), state.blob_uploads.clone());

    let app = axum::Router::new()
        .route("/v1/capabilities", get(capabilities))
        .route("/v1/render", post(render))
        .route("/v1/blobs", post(upload))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve mdcast mock");
    });

    MdcastMock {
        base_url: format!("http://{addr}"),
        render_calls,
        blob_uploads,
    }
}

async fn capabilities() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "targets": ["pdf", "pdf-presentation", "html-reveal"],
        "template_formats": ["pdf"],
        "max_upload_bytes": 33554432u64,
        "version": "0.4.0-test",
    }))
}

/// `mdcast-client` always sends a bearer token (a placeholder when
/// `MDCAST_TOKEN` is unset) — answering 401 on a missing header locks that
/// behavior in as a test failure rather than a silent pass.
fn bearer_present(headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().starts_with("bearer "))
}

async fn render(
    State(state): State<MockState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if !bearer_present(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"code": "unauthorized", "message": "no bearer token"})),
        )
            .into_response();
    }
    let calls_before = state.render_calls.fetch_add(1, Ordering::SeqCst);

    if state.negotiate && calls_before == 0 {
        let Some((key, digest)) = body["assets"]
            .as_object()
            .and_then(|manifest| manifest.iter().next())
        else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "code": "bad_request",
                    "message": "negotiation mode needs a non-empty manifest",
                })),
            )
                .into_response();
        };
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "message": "missing blobs",
                "missing": [{"key": key, "digest": digest}],
            })),
        )
            .into_response();
    }

    let (content_type, bytes): (&str, Vec<u8>) = match body["target"].as_str() {
        Some("html-reveal") => ("text/html; charset=utf-8", HTML_CANNED.as_bytes().to_vec()),
        _ => ("application/pdf", PDF_MAGIC.to_vec()),
    };
    let extension = if body["target"].as_str() == Some("html-reveal") {
        "html"
    } else {
        "pdf"
    };
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"output.{extension}\""),
            ),
        ],
        bytes,
    )
        .into_response()
}

async fn upload(
    State(state): State<MockState>,
    headers: HeaderMap,
    _body: axum::body::Bytes,
) -> Response {
    if !bearer_present(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"code": "unauthorized", "message": "no bearer token"})),
        )
            .into_response();
    }
    state.blob_uploads.fetch_add(1, Ordering::SeqCst);
    StatusCode::OK.into_response()
}
