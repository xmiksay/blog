//! Unit tests for `src/ai/catalog.rs`, split out to keep that file under the
//! 400-line cap.

use super::test_fixtures::{model_row, provider, provider_with_limits};
use super::*;
use entanglement_provider::{GEMINI_BASE, OLLAMA_BASE};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Two rows flagged `is_default` is a legal catalog — `llm_models` has no
/// single-default constraint — and an unordered Postgres scan can hand
/// `refresh()` those rows in either order (an `UPDATE` alone relocates a row
/// in the heap). Whichever order they arrive in, the same id must win, or a
/// plain server restart silently re-seeds `EngineConfig.context_window` from
/// a different model.
#[test]
fn two_flagged_rows_resolve_to_the_lowest_id_whatever_the_scan_order() {
    let ascending = [model_row(3, true), model_row(7, true)];
    let descending = [model_row(7, true), model_row(3, true)];
    assert_eq!(choose_default_model_id(&[3, 7], &ascending), Some(3));
    assert_eq!(choose_default_model_id(&[7, 3], &descending), Some(3));
}

/// The `first()`-fallback path (no row flagged at all) needs the same
/// guarantee — it used to hand back whichever row the scan happened to
/// return first.
#[test]
fn the_unflagged_fallback_is_the_lowest_id_whatever_the_scan_order() {
    let ascending = [model_row(4, false), model_row(9, false)];
    let descending = [model_row(9, false), model_row(4, false)];
    assert_eq!(choose_default_model_id(&[], &ascending), Some(4));
    assert_eq!(choose_default_model_id(&[], &descending), Some(4));
}

/// A single flagged row — what the admin UI actually produces — must still
/// resolve to exactly that row, even when it is not the lowest id present.
#[test]
fn one_flagged_row_wins_over_lower_unflagged_ids() {
    let models = [model_row(1, false), model_row(5, true)];
    assert_eq!(choose_default_model_id(&[5], &models), Some(5));
}

#[test]
fn an_empty_catalog_has_no_default() {
    assert_eq!(choose_default_model_id(&[], &[]), None);
}

/// The exact lock type `SiteCatalog.inner` uses. A `std::sync::RwLock`
/// would poison here — this proves `parking_lot::RwLock` doesn't, so one
/// panicking `refresh()` call can't fail-closed every later
/// `model_by_id`/`default_model` lookup for every session (issue #28).
#[test]
fn panicking_while_holding_the_write_lock_does_not_poison_it() {
    let lock = Arc::new(RwLock::new(CatalogInner::default()));
    let panicking = lock.clone();

    let result = std::thread::spawn(move || {
        let _guard = panicking.write();
        panic!("simulated panic mid-refresh");
    })
    .join();
    assert!(result.is_err(), "the spawned thread should have panicked");

    // A `std::sync::RwLock` would return `Err(Poisoned)` here instead.
    let inner = lock.read();
    assert!(inner.by_model_id.is_empty());
    assert!(inner.default_model_id.is_none());
}

/// A minimal OpenAI-compat SSE mock: accepts a connection, tracks how many
/// are open at once (updating `max_seen`), holds the connection for
/// `delay` before responding, then closes it. Good enough for
/// `openai_factory`'s stream parser — it doesn't validate the request, only
/// that a response arrived.
async fn spawn_concurrency_probe(delay: std::time::Duration) -> (String, Arc<AtomicUsize>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    let in_flight = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));
    let (in_flight, max_seen_task) = (in_flight, max_seen.clone());

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let in_flight = in_flight.clone();
            let max_seen = max_seen_task.clone();
            tokio::spawn(async move {
                // Drain whatever the client already wrote; the mock never
                // inspects the request, so a best-effort read is enough.
                let mut buf = [0u8; 4096];
                let _ = tokio::time::timeout(
                    std::time::Duration::from_millis(500),
                    socket.read(&mut buf),
                )
                .await;

                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                max_seen.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(delay).await;
                in_flight.fetch_sub(1, Ordering::SeqCst);

                let body = "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
                             data: [DONE]\n\n";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });

    (format!("http://{addr}"), max_seen)
}

/// ADR-0111: `llm_providers.concurrency` must cap **simultaneously
/// in-flight requests to that endpoint**, not just be plumbed through and
/// ignored. Three turns fired at once against a `concurrency: Some(1)`
/// ollama provider must serialize — without the fix (both `None`, the
/// library's default concurrency of 3) all three would race through
/// together and `max_seen` would hit 3. `rpm` is pinned to a very high
/// budget so the (separate) adaptive pacing gate can't itself space the
/// three dispatches out and produce `max_seen == 1` for the wrong reason —
/// the concurrency semaphore must be what's actually gating them.
#[tokio::test]
async fn concurrency_capped_provider_serializes_concurrent_turns() {
    let (base_url, max_seen) = spawn_concurrency_probe(std::time::Duration::from_millis(150)).await;
    let http = HttpClient::new().expect("test HTTP client");
    let p = provider_with_limits("ollama", None, Some(&base_url), Some(1), Some(1_000_000));
    let factory = build_factory(&p, "model", &http).expect("build factory");

    let mut handles = Vec::new();
    for _ in 0..3 {
        let factory = factory.clone();
        handles.push(tokio::spawn(async move {
            let mut llm = factory();
            let mut stream = llm
                .stream(entanglement_provider::LlmRequest {
                    system: "",
                    model: None,
                    messages: &[],
                    tools: &[],
                    generation: None,
                })
                .await
                .expect("stream should start");
            while futures_util::StreamExt::next(&mut stream).await.is_some() {}
        }));
    }
    for h in handles {
        h.await.expect("turn task should not panic");
    }

    assert_eq!(
        max_seen.load(Ordering::SeqCst),
        1,
        "concurrency: Some(1) must serialize every in-flight request to this endpoint"
    );
}

/// One catalog entry pointing at `base_url`, so which endpoint the produced
/// `Llm` connects to reveals which model a factory resolved.
fn ollama_catalog_model(id: i32, base_url: &str, http: &HttpClient) -> CatalogModel {
    let p = provider("ollama", None, Some(base_url));
    CatalogModel {
        model_id: id,
        provider_id: p.id,
        provider_label: p.label.clone(),
        kind: p.kind.clone(),
        wire_model: "test-model".to_string(),
        is_default: false,
        context_window: None,
        concurrency: None,
        rpm: None,
        llm_factory: build_factory(&p, "test-model", http).expect("build ollama factory"),
    }
}

async fn drive_one_stream(factory: &LlmFactory) {
    let factory = factory.clone();
    let mut llm = factory();
    let mut stream = llm
        .stream(entanglement_provider::LlmRequest {
            system: "",
            model: None,
            messages: &[],
            tools: &[],
            generation: None,
        })
        .await
        .expect("stream should start");
    while futures_util::StreamExt::next(&mut stream).await.is_some() {}
}

/// The engine freezes `EngineConfig.llm_factory` at spawn, so an un-pinned
/// session (a resumed one, a `/compact` fork's seed) built from a *snapshot*
/// factory would keep calling whatever model was default at boot even after an
/// admin default change (the `qwen3.5:9b`-after-switch-to-`ornith` bug).
/// `dynamic_default_factory` must instead re-resolve the current default on
/// every invocation, so the same factory handle follows a mid-life refresh.
#[tokio::test]
async fn dynamic_default_factory_tracks_the_current_default_across_refresh() {
    let (url_a, hits_a) = spawn_concurrency_probe(std::time::Duration::from_millis(10)).await;
    let (url_b, hits_b) = spawn_concurrency_probe(std::time::Duration::from_millis(10)).await;
    let http = HttpClient::new().expect("test HTTP client");

    let catalog = Arc::new(SiteCatalog::new_for_test(CatalogInner {
        by_model_id: HashMap::from([
            (1, ollama_catalog_model(1, &url_a, &http)),
            (2, ollama_catalog_model(2, &url_b, &http)),
        ]),
        default_model_id: Some(1),
        ..Default::default()
    }));

    // Resolved once — the exact handle the engine would freeze into its config.
    let factory = catalog.dynamic_default_factory();

    drive_one_stream(&factory).await;
    assert_eq!(hits_a.load(Ordering::SeqCst), 1, "default 1 → endpoint A");
    assert_eq!(hits_b.load(Ordering::SeqCst), 0);

    // Simulate the admin flipping the default (what `refresh()` rewrites).
    catalog.set_default_for_test(Some(2));

    drive_one_stream(&factory).await;
    assert_eq!(
        hits_b.load(Ordering::SeqCst),
        1,
        "the same factory handle must follow the new default to endpoint B"
    );
    assert_eq!(hits_a.load(Ordering::SeqCst), 1, "endpoint A not hit again");
}

#[test]
fn provider_endpoint_label_ollama_without_base_url_uses_default() {
    let p = provider("ollama", None, None);
    assert_eq!(provider_endpoint_label(&p), OLLAMA_BASE);
}

#[test]
fn provider_endpoint_label_ollama_with_base_url_uses_it() {
    let p = provider("ollama", None, Some("http://example.internal:1234/v1"));
    assert_eq!(
        provider_endpoint_label(&p),
        "http://example.internal:1234/v1"
    );
}

#[test]
fn provider_endpoint_label_gemini_is_the_gemini_base() {
    let p = provider("gemini", Some("key"), None);
    assert_eq!(provider_endpoint_label(&p), GEMINI_BASE);
}

#[test]
fn provider_endpoint_label_anthropic_is_the_anthropic_base() {
    let p = provider("anthropic", Some("key"), None);
    assert_eq!(provider_endpoint_label(&p), "https://api.anthropic.com");
}

#[test]
fn provider_endpoint_label_openai_uses_its_base_url() {
    let p = provider("openai", None, Some("http://example.internal:1234/v1"));
    assert_eq!(
        provider_endpoint_label(&p),
        "http://example.internal:1234/v1"
    );
}

/// Idle synthesis path: a provider whose `HttpClient` has never made a
/// request yet still gets a `ProviderThrottleStatus` entry (from
/// `ProviderHandle`'s own label/cap, not the live pool), so the admin view
/// always has a row per provider instead of only ones that have misbehaved.
#[test]
fn throttle_statuses_reports_idle_providers_sorted_by_id() {
    let catalog = SiteCatalog::new_for_test(CatalogInner {
        by_provider_id: HashMap::from([
            (
                2,
                ProviderHandle {
                    endpoint: "http://b.internal".to_string(),
                    cap: 5,
                    http: HttpClient::new().expect("test HTTP client"),
                },
            ),
            (
                1,
                ProviderHandle {
                    endpoint: "http://a.internal".to_string(),
                    cap: DEFAULT_CONCURRENCY_FALLBACK,
                    http: HttpClient::new().expect("test HTTP client"),
                },
            ),
        ]),
        ..Default::default()
    });

    let statuses = catalog.throttle_statuses();
    assert_eq!(statuses.len(), 2);

    assert_eq!(statuses[0].provider_id, 1);
    assert_eq!(statuses[0].endpoint, "http://a.internal");
    assert_eq!(statuses[0].cap, DEFAULT_CONCURRENCY_FALLBACK);
    assert_eq!(statuses[0].in_flight, 0);
    assert!(!statuses[0].penalized);
    assert_eq!(statuses[0].backoff_remaining_ms, None);

    assert_eq!(statuses[1].provider_id, 2);
    assert_eq!(statuses[1].endpoint, "http://b.internal");
    assert_eq!(statuses[1].cap, 5);
    assert_eq!(statuses[1].in_flight, 0);
}

/// Proves `throttle_statuses()` reflects genuinely live pool state (an
/// in-flight permit held by a real request), not just the idle synthesis path
/// above. `HttpClient::endpoint()` (and the per-endpoint semaphore it hands
/// out) is private to `entanglement_provider`, so there is no way to hold or
/// inspect a permit directly from here — driving one real (mocked) request
/// through the factory is the only way to populate the pool at all. Polls
/// `throttle_statuses()` rather than sleeping a guessed delay, so this can't
/// flake by racing the probe's own connection accept.
#[tokio::test]
async fn throttle_statuses_reflects_a_live_in_flight_request() {
    let (base_url, _max_seen) =
        spawn_concurrency_probe(std::time::Duration::from_millis(300)).await;
    let busy_http = HttpClient::new().expect("test HTTP client");
    let p = provider_with_limits("ollama", None, Some(&base_url), Some(1), Some(1_000_000));
    let factory = build_factory(&p, "model", &busy_http).expect("build factory");

    let catalog = Arc::new(SiteCatalog::new_for_test(CatalogInner {
        by_provider_id: HashMap::from([
            (
                1,
                ProviderHandle {
                    endpoint: "unused".to_string(),
                    cap: 1,
                    http: busy_http,
                },
            ),
            (
                2,
                ProviderHandle {
                    endpoint: "http://sibling.internal".to_string(),
                    cap: DEFAULT_CONCURRENCY_FALLBACK,
                    http: HttpClient::new().expect("test HTTP client"),
                },
            ),
        ]),
        ..Default::default()
    }));

    let driven = tokio::spawn(async move { drive_one_stream(&factory).await });

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut observed = None;
    while std::time::Instant::now() < deadline {
        let statuses = catalog.throttle_statuses();
        if statuses
            .iter()
            .any(|s| s.provider_id == 1 && s.in_flight == 1)
        {
            observed = Some(statuses);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let statuses = observed.expect("never observed the in-flight request via throttle_statuses()");

    let provider_1 = statuses
        .iter()
        .find(|s| s.provider_id == 1)
        .expect("provider 1 present");
    assert_eq!(provider_1.in_flight, 1);
    assert_eq!(provider_1.cap, 1);

    let provider_2 = statuses
        .iter()
        .find(|s| s.provider_id == 2)
        .expect("sibling provider present");
    assert_eq!(
        provider_2.in_flight, 0,
        "sibling provider's own client must be untouched by provider 1's request"
    );

    driven.await.expect("driven stream task should not panic");
}
