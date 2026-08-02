//! DB-dependent tests for `site::ai::catalog::SiteCatalog` against real
//! `llm_providers`/`llm_models` rows. Gated on `DATABASE_URL`; follows
//! `tests/policy_db.rs`'s skip/cleanup convention. Neither table FKs to
//! `users` (`m_014_create_llm_providers.rs` / `m_015_split_llm_models.rs`), so
//! cleanup is a direct `llm_providers` delete — cascades to its `llm_models`
//! rows (`ON DELETE CASCADE` on `llm_models.provider_id`).
//!
//! Every provider row here uses `kind = "ollama"` with no `api_key` needed —
//! `build_factory`'s own per-`kind` dispatch (including the `api_key`-required
//! branches) is covered by the pure in-module tests at the bottom of
//! `src/ai/catalog.rs`; these tests are about the DB-hydrated catalog surface
//! (`default_model`, `model_by_id`, `model_resolver`), not that dispatch.
//!
//! Unlike `tests/policy_db.rs` (scoped by a unique `user_id` per test),
//! `SiteCatalog::load`/`refresh` reads the *entire* `llm_providers`/
//! `llm_models` tables with no per-test scoping — mirroring production, where
//! there's one site-wide catalog. `cargo test` runs test functions within a
//! binary concurrently by default, so two of these tests running at once
//! would race on which rows are visible when the other's `load` runs
//! (concretely: a sibling test's lower-id row would win the default). The
//! resolution is deterministic (`catalog::choose_default_model_id`) but still
//! table-wide, so serialize every test in this file with one process-wide
//! lock instead of relying on data isolation the code under test doesn't
//! provide.

#[path = "common/llm_mock.rs"]
mod llm_mock;

use entanglement_provider::LlmRequest;
use llm_mock::spawn_openai_sse_mock;
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, EntityTrait, Set};
use site::ai::catalog::SiteCatalog;
use site::entity::{llm_model, llm_provider};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::{Mutex, MutexGuard};

// `tokio::sync::Mutex` (not `std::sync::Mutex`): each test's body holds the
// guard across several `.await` points, which `std::sync::MutexGuard` isn't
// designed for (and clippy's `await_holding_lock` correctly flags).
static SERIALIZE: Mutex<()> = Mutex::const_new(());

/// Acquire the file-wide serialization lock (see the module doc for why every
/// test in this file needs one).
async fn exclusive() -> MutexGuard<'static, ()> {
    SERIALIZE.lock().await
}

async fn test_db() -> Option<DatabaseConnection> {
    let url = std::env::var("DATABASE_URL").ok()?;
    Some(
        Database::connect(&url)
            .await
            .expect("connect to DATABASE_URL"),
    )
}

async fn make_provider(db: &DatabaseConnection, tag: &str) -> llm_provider::Model {
    llm_provider::ActiveModel {
        label: Set(format!("catalog-test-{tag}-{}", uuid::Uuid::new_v4())),
        kind: Set("ollama".to_string()),
        api_key: Set(None),
        base_url: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert throwaway llm_provider")
}

async fn make_provider_with_limits(
    db: &DatabaseConnection,
    tag: &str,
    concurrency: i32,
    rpm: i32,
) -> llm_provider::Model {
    llm_provider::ActiveModel {
        label: Set(format!("catalog-test-{tag}-{}", uuid::Uuid::new_v4())),
        kind: Set("ollama".to_string()),
        api_key: Set(None),
        base_url: Set(None),
        concurrency: Set(Some(concurrency)),
        rpm: Set(Some(rpm)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert throwaway llm_provider")
}

async fn make_model(
    db: &DatabaseConnection,
    provider_id: i32,
    wire: &str,
    is_default: bool,
) -> llm_model::Model {
    llm_model::ActiveModel {
        provider_id: Set(provider_id),
        label: Set(format!("label-{wire}")),
        model: Set(wire.to_string()),
        is_default: Set(is_default),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert throwaway llm_model")
}

async fn make_model_with_window(
    db: &DatabaseConnection,
    provider_id: i32,
    wire: &str,
    is_default: bool,
    context_window: i32,
) -> llm_model::Model {
    llm_model::ActiveModel {
        provider_id: Set(provider_id),
        label: Set(format!("label-{wire}")),
        model: Set(wire.to_string()),
        is_default: Set(is_default),
        context_window: Set(Some(context_window)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert throwaway llm_model")
}

async fn cleanup_provider(db: &DatabaseConnection, provider_id: i32) {
    llm_provider::Entity::delete_by_id(provider_id)
        .exec(db)
        .await
        .expect("delete throwaway llm_provider"); // cascades to llm_models rows
}

/// Wipe every `llm_providers`/`llm_models` row, called right after acquiring
/// `exclusive()` and before inserting this test's own fixture. Guards against
/// more than just concurrent tests: if an *earlier* run of this file panicked
/// after inserting rows but before its own `cleanup_provider` ran, those rows
/// would otherwise sit in `site_test` (which isn't reset between runs) and,
/// having lower ids than anything a later run inserts, silently win
/// `default_model()` for every run after. Safe to wipe unconditionally — this
/// file exclusively owns
/// these two tables (see the module doc); no other test file's fixtures live
/// here.
async fn wipe_catalog_tables(db: &DatabaseConnection) {
    llm_provider::Entity::delete_many()
        .exec(db)
        .await
        .expect("wipe llm_providers"); // cascades to llm_models
}

#[tokio::test]
async fn default_model_picks_the_row_flagged_default() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let _guard = exclusive().await;
    wipe_catalog_tables(&db).await;
    let provider = make_provider(&db, "flagged").await;
    make_model(&db, provider.id, "model-a", false).await;
    let flagged = make_model(&db, provider.id, "model-b", true).await;

    let catalog = SiteCatalog::load(db.clone()).await.expect("load catalog");
    let default = catalog
        .default_model()
        .expect("expected a default model to resolve");
    assert_eq!(default.model_id, flagged.id);

    cleanup_provider(&db, provider.id).await;
}

#[tokio::test]
async fn default_model_falls_back_to_first_row_when_none_flagged() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let _guard = exclusive().await;
    wipe_catalog_tables(&db).await;
    let provider = make_provider(&db, "unflagged").await;
    let first = make_model(&db, provider.id, "model-a", false).await;
    make_model(&db, provider.id, "model-b", false).await;

    let catalog = SiteCatalog::load(db.clone()).await.expect("load catalog");
    let default = catalog
        .default_model()
        .expect("expected a fallback default model to resolve");
    assert_eq!(default.model_id, first.id);

    cleanup_provider(&db, provider.id).await;
}

/// `llm_models` has no single-default constraint, so two flagged rows are a
/// legal (if misconfigured) catalog — and before the `ORDER BY`/`min` in
/// `refresh` the winner was whichever the unordered scan happened to hand over
/// last, i.e. re-decided on every restart. `EngineConfig.context_window` is
/// seeded from it (`ai/engine.rs`), so the ambiguous case has to be pinned
/// down: lowest id, always. The `UPDATE` below is the point — rewriting a row
/// relocates it in the Postgres heap, so the two loads see the rows in
/// opposite physical order and must still agree.
#[tokio::test]
async fn two_rows_flagged_default_always_resolve_to_the_lowest_id() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let _guard = exclusive().await;
    wipe_catalog_tables(&db).await;
    let provider = make_provider(&db, "two-defaults").await;
    let lower = make_model(&db, provider.id, "model-a", true).await;
    make_model(&db, provider.id, "model-b", true).await;

    let catalog = SiteCatalog::load(db.clone()).await.expect("load catalog");
    assert_eq!(
        catalog.default_model().map(|m| m.model_id),
        Some(lower.id),
        "the lowest-id flagged row must win"
    );

    let mut active: llm_model::ActiveModel = lower.clone().into();
    active.label = Set("relocated".to_string());
    active.update(&db).await.expect("rewrite the lower-id row");

    let catalog = SiteCatalog::load(db.clone()).await.expect("reload catalog");
    assert_eq!(
        catalog.default_model().map(|m| m.model_id),
        Some(lower.id),
        "the same row must win after its physical position moved"
    );

    cleanup_provider(&db, provider.id).await;
}

#[tokio::test]
async fn model_by_id_finds_and_misses() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let _guard = exclusive().await;
    wipe_catalog_tables(&db).await;
    let provider = make_provider(&db, "lookup").await;
    let model = make_model(&db, provider.id, "model-a", true).await;

    let catalog = SiteCatalog::load(db.clone()).await.expect("load catalog");
    assert!(catalog.model_by_id(model.id).is_some());
    assert!(catalog.model_by_id(-1).is_none());

    cleanup_provider(&db, provider.id).await;
}

#[tokio::test]
async fn model_resolver_resolves_a_correct_provider_model_pair() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let _guard = exclusive().await;
    wipe_catalog_tables(&db).await;
    let provider = make_provider(&db, "resolve-ok").await;
    let model = make_model(&db, provider.id, "model-a", true).await;

    let catalog = SiteCatalog::load(db.clone()).await.expect("load catalog");
    let resolver = catalog.model_resolver();
    let resolved = resolver(None, &provider.label, &model.id.to_string())
        .expect("expected the matching provider/model pair to resolve");
    assert_eq!(resolved.provider, provider.label);
    assert_eq!(resolved.model, "model-a");

    cleanup_provider(&db, provider.id).await;
}

/// #40: `ResolvedModel::context_window` must carry the `llm_models.
/// context_window` value through, not the hardcoded `None` the field used to
/// be pinned to (`src/ai/catalog.rs`'s `model_resolver`) — that's what lets
/// `entanglement_core`'s turn loop compact/refuse against the model's real
/// budget instead of a generic fallback.
#[tokio::test]
async fn model_resolver_populates_context_window_from_the_row() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let _guard = exclusive().await;
    wipe_catalog_tables(&db).await;
    let provider = make_provider(&db, "resolve-window").await;
    let model = make_model_with_window(&db, provider.id, "model-a", true, 128_000).await;

    let catalog = SiteCatalog::load(db.clone()).await.expect("load catalog");
    let resolver = catalog.model_resolver();
    let resolved = resolver(None, &provider.label, &model.id.to_string())
        .expect("expected the matching provider/model pair to resolve");
    assert_eq!(resolved.context_window, Some(128_000));

    cleanup_provider(&db, provider.id).await;
}

/// The counterpart to the above: an unset `context_window` row must resolve
/// to `None`, not silently default to some magic number — the engine's own
/// generic fallback (`entanglement_core::context::CONTEXT_LIMIT_TOKENS`)
/// takes over from there.
#[tokio::test]
async fn model_resolver_leaves_context_window_none_when_unset() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let _guard = exclusive().await;
    wipe_catalog_tables(&db).await;
    let provider = make_provider(&db, "resolve-window-unset").await;
    let model = make_model(&db, provider.id, "model-a", true).await;

    let catalog = SiteCatalog::load(db.clone()).await.expect("load catalog");
    let resolver = catalog.model_resolver();
    let resolved = resolver(None, &provider.label, &model.id.to_string())
        .expect("expected the matching provider/model pair to resolve");
    assert_eq!(resolved.context_window, None);

    cleanup_provider(&db, provider.id).await;
}

#[tokio::test]
async fn model_resolver_rejects_a_model_id_under_the_wrong_provider_label() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let _guard = exclusive().await;
    wipe_catalog_tables(&db).await;
    let provider = make_provider(&db, "resolve-wrong-provider").await;
    let model = make_model(&db, provider.id, "model-a", true).await;

    let catalog = SiteCatalog::load(db.clone()).await.expect("load catalog");
    let resolver = catalog.model_resolver();
    let err = resolver(None, "not-the-real-label", &model.id.to_string())
        .err()
        .expect("a real model id under the wrong provider label must be rejected");
    assert!(err.contains("not `not-the-real-label`") || err.contains("belongs to provider"));

    cleanup_provider(&db, provider.id).await;
}

#[tokio::test]
async fn model_resolver_rejects_a_non_numeric_model_string() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let _guard = exclusive().await;
    wipe_catalog_tables(&db).await;
    let provider = make_provider(&db, "resolve-non-numeric").await;
    make_model(&db, provider.id, "model-a", true).await;

    let catalog = SiteCatalog::load(db.clone()).await.expect("load catalog");
    let resolver = catalog.model_resolver();
    let err = resolver(None, &provider.label, "not-a-number")
        .err()
        .expect("a non-numeric model string must be rejected");
    assert!(err.contains("not a valid model id"));

    cleanup_provider(&db, provider.id).await;
}

/// ADR-0111: `llm_providers.concurrency`/`.rpm` must reach `CatalogModel` —
/// the values `build_factory` (`src/ai/catalog.rs`) bakes into the
/// `LlmFactory` closure, so the per-endpoint permit + pacing gate are sized
/// per provider row instead of the library's process-wide defaults.
#[tokio::test]
async fn catalog_carries_per_provider_concurrency_and_rpm_from_the_db() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let _guard = exclusive().await;
    wipe_catalog_tables(&db).await;
    let provider = make_provider_with_limits(&db, "limits", 2, 30).await;
    let model = make_model(&db, provider.id, "model-a", true).await;

    let catalog = SiteCatalog::load(db.clone()).await.expect("load catalog");
    let resolved = catalog
        .model_by_id(model.id)
        .expect("expected the model to resolve");
    assert_eq!(resolved.concurrency, Some(2));
    assert_eq!(resolved.rpm, Some(30));

    cleanup_provider(&db, provider.id).await;
}

/// The counterpart to the above: a provider row with no `concurrency`/`rpm`
/// set must resolve to `None` on both — the library's own client default
/// takes over from there, not some silently-invented value.
#[tokio::test]
async fn catalog_leaves_concurrency_and_rpm_none_when_unset() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let _guard = exclusive().await;
    wipe_catalog_tables(&db).await;
    let provider = make_provider(&db, "limits-unset").await;
    let model = make_model(&db, provider.id, "model-a", true).await;

    let catalog = SiteCatalog::load(db.clone()).await.expect("load catalog");
    let resolved = catalog
        .model_by_id(model.id)
        .expect("expected the model to resolve");
    assert_eq!(resolved.concurrency, None);
    assert_eq!(resolved.rpm, None);

    cleanup_provider(&db, provider.id).await;
}

/// Drive one turn through `model_id`'s current `llm_factory` to completion.
async fn fire_one_turn(catalog: &SiteCatalog, model_id: i32) {
    let mut llm = (catalog
        .model_by_id(model_id)
        .expect("model should resolve")
        .llm_factory)();
    let mut stream = llm
        .stream(LlmRequest {
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

/// Regression test for #41/ADR-0111: `SiteCatalog::refresh()` must rebuild
/// its `HttpClient` fresh, not reuse one long-lived instance across every
/// refresh. `entanglement_provider::HttpClient` locks in an endpoint's
/// rpm/concurrency on that endpoint's *first* request and ignores later
/// values passed for the same `(base_url, api_key)` key — see its own
/// `endpoint()` doc ("Only the first caller for a key sets the bucket
/// size"). Without rebuilding the `HttpClient` per `refresh()`, an admin's
/// `concurrency` edit would silently have no effect once any turn had
/// already gone through that provider.
#[tokio::test]
async fn refresh_applies_an_updated_concurrency_cap_even_after_the_endpoint_already_served_a_request()
 {
    let Some(db) = test_db().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let _guard = exclusive().await;
    wipe_catalog_tables(&db).await;

    let mock = spawn_openai_sse_mock("", Duration::from_millis(150)).await;
    // `rpm` is pinned high from the start so the (separate) adaptive pacing
    // gate can't itself space the 3 dispatches out — only the `concurrency`
    // semaphore we set *after* the endpoint has already served a request
    // should determine whether they overlap.
    let provider = llm_provider::ActiveModel {
        label: Set(format!(
            "catalog-test-refresh-live-{}",
            uuid::Uuid::new_v4()
        )),
        kind: Set("ollama".to_string()),
        api_key: Set(None),
        base_url: Set(Some(mock.base_url.clone())),
        rpm: Set(Some(1_000_000)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert throwaway llm_provider");
    let model = make_model(&db, provider.id, "model-a", true).await;

    let catalog = SiteCatalog::load(db.clone()).await.expect("load catalog");

    // Warm the endpoint with no concurrency cap set (library default: 3) —
    // this is what locks in the endpoint's bucket size under the bug.
    fire_one_turn(&catalog, model.id).await;

    // Now cap concurrency to 1 and refresh, exactly as the admin handler does.
    let mut active: llm_provider::ActiveModel = provider.clone().into();
    active.concurrency = Set(Some(1));
    active
        .update(&db)
        .await
        .expect("update provider concurrency");
    catalog.refresh().await.expect("refresh catalog");

    // Fire 3 concurrent turns; if the cap took effect, at most 1 runs at once.
    let mut handles = Vec::new();
    for _ in 0..3 {
        let catalog = catalog.clone();
        let model_id = model.id;
        handles.push(tokio::spawn(async move {
            fire_one_turn(&catalog, model_id).await
        }));
    }
    for h in handles {
        h.await.expect("turn task should not panic");
    }

    assert_eq!(
        mock.max_in_flight.load(Ordering::SeqCst),
        1,
        "the concurrency cap set after refresh() must actually bound in-flight requests"
    );

    cleanup_provider(&db, provider.id).await;
}
