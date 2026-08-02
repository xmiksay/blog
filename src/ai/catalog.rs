//! `SiteCatalog` — the engine's model catalog, building
//! `entanglement_provider::LlmFactory`/`ModelResolver` closures. Hydrated from
//! the `llm_providers`/`llm_models` tables; `refresh()` re-reads them (call
//! after provider/model CRUD in the handlers).
//!
//! **`ModelResolver` keying convention** (documented here for the follow-up
//! phase that mints `InMsg::SetModel`): `provider` = `llm_providers.label`
//! (unique display name), `model` = `llm_models.id` as a decimal string. Our
//! own primary keys are the only unambiguous handle — two provider rows of
//! the same `kind` (e.g. two `anthropic` connections) or two models sharing a
//! wire id string are both legal, so keying by kind/wire-id would collide.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use entanglement_provider::{
    GenerationResolver, HttpClient, LlmFactory, ModelResolver, ResolvedModel, UserId,
};
use parking_lot::RwLock;
use sea_orm::{DatabaseConnection, EntityTrait, QueryOrder};

use crate::entity::{llm_model, llm_provider};

mod factory;
use factory::{build_factory, positive_u32, positive_usize};

mod throttle;
pub use throttle::ProviderThrottleStatus;
use throttle::{DEFAULT_CONCURRENCY_FALLBACK, ProviderHandle, provider_endpoint_label};

/// One usable (provider row, model row) pairing, with its ready-to-call LLM
/// factory closure baked in.
#[derive(Clone)]
pub struct CatalogModel {
    pub model_id: i32,
    pub provider_id: i32,
    pub provider_label: String,
    pub kind: String,
    pub wire_model: String,
    pub is_default: bool,
    /// The model's real context window in tokens (#40), from `llm_models`.
    pub context_window: Option<usize>,
    /// Effective per-endpoint in-flight request cap threaded into
    /// `llm_factory` (ADR-0111), from `llm_providers.concurrency`. `None` ⇒
    /// `entanglement_provider`'s own client default. Exposed for
    /// introspection/tests — the value is already baked into the closure.
    pub concurrency: Option<usize>,
    /// Effective per-endpoint requests-per-minute budget threaded into
    /// `llm_factory`, from `llm_providers.rpm`. `None` ⇒ the client default.
    pub rpm: Option<u32>,
    pub llm_factory: LlmFactory,
}

#[derive(Default)]
struct CatalogInner {
    by_model_id: HashMap<i32, CatalogModel>,
    default_model_id: Option<i32>,
    /// One dedicated `HttpClient` per provider row (see `refresh()`) — what
    /// makes `throttle_statuses()` (#89) addressable per provider instead of
    /// one shared client's unattributable global worst-offender reading.
    by_provider_id: HashMap<i32, ProviderHandle>,
}

pub struct SiteCatalog {
    db: DatabaseConnection,
    /// `parking_lot::RwLock`, not `std::sync`: a panic while this is locked
    /// must not poison it and fail-closed every later `model_by_id`/
    /// `default_llm_factory` lookup for every session (issue #28).
    inner: RwLock<CatalogInner>,
}

impl SiteCatalog {
    /// Load the catalog from the DB. Returns an `Arc` since `engine.rs` shares
    /// it between `EngineConfig.model_resolver` and any future admin surface.
    pub async fn load(db: DatabaseConnection) -> anyhow::Result<Arc<Self>> {
        let catalog = Arc::new(SiteCatalog {
            db,
            inner: RwLock::new(CatalogInner::default()),
        });
        catalog.refresh().await?;
        Ok(catalog)
    }

    /// Re-read `llm_providers`/`llm_models` and rebuild every factory
    /// closure, including a **fresh** `HttpClient` per provider row to build
    /// them against. `entanglement_provider::HttpClient`'s per-endpoint
    /// rpm/concurrency state is created lazily on an endpoint's *first*
    /// request and then locked in for that `HttpClient`'s lifetime (see its
    /// own `endpoint()` doc: "Only the first caller for a key sets the bucket
    /// size"). Reusing one long-lived `HttpClient` across every `refresh()`
    /// would mean an admin's `concurrency`/`rpm` edit (#41/ADR-0111) never
    /// takes effect once *any* session had already hit that provider's
    /// endpoint — rebuilding here costs nothing until first use (no eager
    /// connections) and only affects *new* sessions/turns resolved after this
    /// refresh, matching this method's existing "next new session" contract.
    /// Each `LlmFactory` closure clones its provider's `http` into itself
    /// (`build_factory`), which is what keeps its `Arc`-shared pool alive —
    /// the per-provider handle stashed in `by_provider_id` is what actually
    /// keeps that `HttpClient` alive past this function returning, so it can
    /// still answer `throttle_status()` later. One dedicated client per
    /// provider row (rather than one shared client for every provider) is
    /// also what makes `throttle_statuses()` (#89) addressable per provider —
    /// a single shared client's `throttle_status()` can only ever report one
    /// global worst-offender reading, unattributable to a `provider_id`.
    ///
    /// Call after provider/model CRUD so live sessions pick up the change on
    /// their next `SetModel`/new-session resolve.
    pub async fn refresh(&self) -> anyhow::Result<()> {
        let providers = llm_provider::Entity::find()
            .all(&self.db)
            .await
            .context("loading llm_providers")?;
        // Ordered, not incidental: Postgres returns an unordered scan in
        // whatever physical order the heap happens to hold (an `UPDATE`
        // relocates a row to the end), and this loop's outcome — which model
        // ends up the engine-wide default, see `choose_default_model_id` —
        // must not depend on that.
        let models = llm_model::Entity::find()
            .order_by_asc(llm_model::Column::Id)
            .all(&self.db)
            .await
            .context("loading llm_models")?;

        let mut by_provider_id = HashMap::with_capacity(providers.len());
        for provider in &providers {
            by_provider_id.insert(
                provider.id,
                ProviderHandle {
                    endpoint: provider_endpoint_label(provider),
                    cap: positive_usize(provider.concurrency)
                        .unwrap_or(DEFAULT_CONCURRENCY_FALLBACK),
                    http: HttpClient::new()
                        .with_context(|| format!("HTTP client for '{}'", provider.label))?,
                },
            );
        }

        let mut by_model_id = HashMap::with_capacity(models.len());
        let mut flagged_default_ids = Vec::new();
        for model in &models {
            let Some(provider) = providers.iter().find(|p| p.id == model.provider_id) else {
                tracing::warn!(
                    model_id = model.id,
                    "llm_models row has no matching provider; skipping"
                );
                continue;
            };
            let Some(handle) = by_provider_id.get(&provider.id) else {
                // Unreachable in practice — `by_provider_id` was just built
                // from the same `providers` list `provider` came from above.
                continue;
            };
            let llm_factory = match build_factory(provider, &model.model, &handle.http) {
                Ok(f) => f,
                Err(e) => {
                    tracing::warn!(model_id = model.id, error = %e, "skipping unbuildable model");
                    continue;
                }
            };
            if model.is_default {
                flagged_default_ids.push(model.id);
            }
            by_model_id.insert(
                model.id,
                CatalogModel {
                    model_id: model.id,
                    provider_id: provider.id,
                    provider_label: provider.label.clone(),
                    kind: provider.kind.clone(),
                    wire_model: model.model.clone(),
                    is_default: model.is_default,
                    context_window: model.context_window.and_then(|w| usize::try_from(w).ok()),
                    concurrency: positive_usize(provider.concurrency),
                    rpm: positive_u32(provider.rpm),
                    llm_factory,
                },
            );
        }
        let default_model_id = choose_default_model_id(&flagged_default_ids, &models);
        *self.inner.write() = CatalogInner {
            by_model_id,
            default_model_id,
            by_provider_id,
        };
        Ok(())
    }

    pub fn model_by_id(&self, model_id: i32) -> Option<CatalogModel> {
        self.inner.read().by_model_id.get(&model_id).cloned()
    }

    pub fn default_model(&self) -> Option<CatalogModel> {
        let inner = self.inner.read();
        inner
            .default_model_id
            .and_then(|id| inner.by_model_id.get(&id).cloned())
    }

    /// The factory `EngineConfig.llm_factory` should use before any
    /// per-session `SetModel` — the default model's factory, or `EchoLlm` if
    /// nothing is configured yet (matching `EngineConfig::default()`'s own
    /// fallback, so an empty catalog degrades gracefully instead of panicking).
    pub fn default_llm_factory(&self) -> LlmFactory {
        match self.default_model() {
            Some(m) => m.llm_factory,
            None => Arc::new(|| Box::new(entanglement_provider::EchoLlm)),
        }
    }

    /// A default factory that defers to the *current* default at call time,
    /// so `refresh()` (provider/model CRUD) takes effect for un-pinned/resumed
    /// sessions without a server restart. Unlike [`default_llm_factory`][Self::
    /// default_llm_factory] — whose result the engine would otherwise freeze
    /// into `EngineConfig.llm_factory` at spawn — this closure re-reads the
    /// catalog every time the engine builds an LLM for a session that has no
    /// `SetModel` pin yet (a fresh session's first turn, a `/compact` fork's
    /// seed, a resumed session before replay re-pins it).
    pub fn dynamic_default_factory(self: &Arc<Self>) -> LlmFactory {
        let catalog = self.clone();
        Arc::new(move || (catalog.default_llm_factory())())
    }

    /// Build the `ModelResolver` closure for `EngineConfig.model_resolver`
    /// (live `InMsg::SetModel` support). See the module doc for the
    /// `provider`/`model` keying convention.
    pub fn model_resolver(self: &Arc<Self>) -> ModelResolver {
        let catalog = self.clone();
        Arc::new(
            // `_user` (0.6/ADR-0147) is ignored: the site never hands `Holly`
            // a `UserId` — it keys sessions itself — so every resolve is
            // against the one global `llm_models` table.
            move |_user: Option<&UserId>,
                  provider: &str,
                  model: &str|
                  -> Result<ResolvedModel, String> {
                let model_id: i32 = model
                    .parse()
                    .map_err(|_| format!("model `{model}` is not a valid model id"))?;
                let found = catalog
                    .model_by_id(model_id)
                    .ok_or_else(|| format!("model id {model_id} not found"))?;
                if found.provider_label != provider {
                    return Err(format!(
                        "model id {model_id} belongs to provider `{}`, not `{provider}`",
                        found.provider_label
                    ));
                }
                Ok(ResolvedModel {
                    provider: found.provider_label,
                    model: found.wire_model,
                    llm_factory: found.llm_factory,
                    generation: None,
                    context_window: found.context_window,
                })
            },
        )
    }

    /// Build the `GenerationResolver` closure for
    /// `EngineConfig.generation_resolver` (per-*profile* persisted generation
    /// overrides, ADR-0094 — the generation-parameter analogue of
    /// `AgentProfile::model_pin`). This site has no per-profile generation
    /// config store: unlike the model pin, which `engine/profiles.rs` bakes
    /// straight into each `AgentProfile`, generation knobs (temperature,
    /// reasoning effort) are set live per-*session* via `InMsg::SetGeneration`
    /// (`handlers/sessions`, #42), not pinned per-profile. Always returns
    /// `None`, wiring the seam for parity with [`model_resolver`][Self::
    /// model_resolver] without inventing an admin surface nothing populates.
    pub fn generation_resolver(self: &Arc<Self>) -> GenerationResolver {
        Arc::new(|_profile: &str| None)
    }
}

/// Which model id `refresh()` installs as the engine-wide default: `flagged`
/// (the ids of the *buildable* rows carrying `is_default`) if any, else the
/// lowest id in `all`, mirroring `ProviderRegistry::resolve_default`'s
/// "first row wins" fallback now that the query is ordered by id.
///
/// **Lowest id wins on both paths.** `llm_models` has no single-default
/// constraint, so two flagged rows are a legal (if misconfigured) catalog, and
/// nothing about Postgres' physical row order is stable — an `UPDATE` alone
/// relocates a row. Ids, by contrast, are assigned once and never change, so
/// `min` is the only tie-break that survives a restart, a `refresh()` after
/// unrelated CRUD, or a table rewrite. That stability is load-bearing rather
/// than cosmetic: `EngineConfig.context_window` is seeded from this choice
/// (`engine.rs`), so an ambiguous catalog resolved by scan order would
/// silently re-budget every fresh session on each restart. A catalog with
/// exactly one flagged row — the configuration the admin UI produces —
/// resolves to that row either way.
fn choose_default_model_id(flagged: &[i32], all: &[llm_model::Model]) -> Option<i32> {
    flagged
        .iter()
        .copied()
        .min()
        .or_else(|| all.iter().map(|m| m.id).min())
}

#[cfg(test)]
mod test_fixtures;

#[cfg(test)]
impl SiteCatalog {
    /// Build a catalog with a pre-seeded `inner` and a disconnected DB — for
    /// unit tests that exercise the in-memory lookup/factory paths (which never
    /// touch `db`) without a live Postgres.
    fn new_for_test(inner: CatalogInner) -> Self {
        SiteCatalog {
            db: DatabaseConnection::default(),
            inner: RwLock::new(inner),
        }
    }

    /// Rewrite the default model id the way `refresh()` would on an admin edit.
    fn set_default_for_test(&self, id: Option<i32>) {
        self.inner.write().default_model_id = id;
    }
}

#[cfg(test)]
mod tests;
