//! Per-provider-row `LlmFactory` construction, split out of `catalog.rs` to
//! keep that file under the 400-line cap. Everything here is pure: it maps one
//! `llm_providers` row onto the `entanglement_provider` factory that speaks
//! that provider's wire format, with no DB or network access of its own.

use entanglement_provider::{
    ANTHROPIC_BASE, GEMINI_BASE, HttpClient, LlmFactory, OLLAMA_BASE, anthropic_factory,
    fixed_model_concurrency, gemini_factory, openai_factory,
};

use crate::entity::llm_provider;

/// Build the `LlmFactory` for one provider row, dispatching on `kind`.
///
/// `provider.rpm`/`.concurrency` (ADR-0111) are threaded straight into the
/// factory so the client's per-endpoint pacing gate and in-flight permit are
/// sized from the DB row instead of the library's process-wide defaults —
/// this is what serializes many spawned sub-agents against one provider's
/// real limits instead of 429-storming it.
///
/// The per-*model* cap the factories gained in 0.6 (#521/ADR-0140) is always
/// `fixed_model_concurrency(None)` — `llm_models` has no such column, and
/// `None` admits solely through the endpoint gate above, as before 0.6.
pub(crate) fn build_factory(
    provider: &llm_provider::Model,
    default_model: &str,
    http: &HttpClient,
) -> anyhow::Result<LlmFactory> {
    let rpm = positive_u32(provider.rpm);
    let concurrency = positive_usize(provider.concurrency);
    match provider.kind.as_str() {
        "ollama" => Ok(openai_factory(
            ollama_base_url(provider),
            None,
            default_model,
            rpm,
            concurrency,
            fixed_model_concurrency(None),
            None,
            http.clone(),
        )),
        "anthropic" => {
            let api_key = provider.api_key.clone().ok_or_else(|| {
                anyhow::anyhow!("anthropic provider '{}' has no api_key", provider.label)
            })?;
            Ok(anthropic_factory(
                // 0.6 parameterized what it used to hard-code — same constant.
                ANTHROPIC_BASE,
                api_key,
                default_model,
                rpm,
                concurrency,
                fixed_model_concurrency(None),
                None,
                // `web_search_tool_version`: `None` keeps `web_search_20250305`.
                None,
                http.clone(),
            ))
        }
        "gemini" => {
            let api_key = provider.api_key.clone().ok_or_else(|| {
                anyhow::anyhow!("gemini provider '{}' has no api_key", provider.label)
            })?;
            Ok(gemini_factory(
                GEMINI_BASE,
                api_key,
                default_model,
                rpm,
                concurrency,
                fixed_model_concurrency(None),
                http.clone(),
            ))
        }
        "openai" => {
            let base_url = provider
                .base_url
                .clone()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("openai provider '{}' has no base_url", provider.label)
                })?;
            Ok(openai_factory(
                base_url,
                provider.api_key.clone().filter(|s| !s.is_empty()),
                default_model,
                rpm,
                concurrency,
                fixed_model_concurrency(None),
                None,
                http.clone(),
            ))
        }
        other => anyhow::bail!("provider kind not supported: {other}"),
    }
}

/// A DB-stored budget clamped to the factories' expected type. A non-positive
/// value is treated as "unset" (falls back to the client's own default)
/// rather than panicking on the cast or silently passing a zero-sized budget.
pub(crate) fn positive_u32(v: Option<i32>) -> Option<u32> {
    v.and_then(|n| u32::try_from(n).ok()).filter(|n| *n > 0)
}

/// See [`positive_u32`]; same clamp for the `usize` concurrency cap.
pub(crate) fn positive_usize(v: Option<i32>) -> Option<usize> {
    v.and_then(|n| usize::try_from(n).ok()).filter(|n| *n > 0)
}

/// Effective Ollama base URL for `provider`'s row: its own `base_url` unless
/// unset/blank, else the default local endpoint. Split out from
/// `build_factory` so the fallback is unit-testable without a network call
/// (the resulting `LlmFactory` closure is opaque — there's no other way to
/// observe which URL it captured).
pub(crate) fn ollama_base_url(provider: &llm_provider::Model) -> String {
    provider
        .base_url
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| OLLAMA_BASE.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::catalog::test_fixtures::provider;

    #[test]
    fn ollama_without_base_url_falls_back_to_default() {
        let p = provider("ollama", None, None);
        assert_eq!(ollama_base_url(&p), OLLAMA_BASE);
        assert!(build_factory(&p, "model", &HttpClient::new().expect("test HTTP client")).is_ok());
    }

    #[test]
    fn ollama_with_blank_base_url_falls_back_to_default() {
        let p = provider("ollama", None, Some(""));
        assert_eq!(ollama_base_url(&p), OLLAMA_BASE);
    }

    #[test]
    fn ollama_with_base_url_uses_it() {
        let p = provider("ollama", None, Some("http://example.internal:1234/v1"));
        assert_eq!(ollama_base_url(&p), "http://example.internal:1234/v1");
    }

    #[test]
    fn anthropic_without_api_key_errs() {
        let p = provider("anthropic", None, None);
        let err = build_factory(&p, "model", &HttpClient::new().expect("test HTTP client"))
            .err()
            .expect("expected build_factory to fail");
        assert!(err.to_string().contains("no api_key"));
    }

    #[test]
    fn anthropic_with_api_key_builds_ok() {
        let p = provider("anthropic", Some("key"), None);
        assert!(build_factory(&p, "model", &HttpClient::new().expect("test HTTP client")).is_ok());
    }

    #[test]
    fn gemini_without_api_key_errs() {
        let p = provider("gemini", None, None);
        let err = build_factory(&p, "model", &HttpClient::new().expect("test HTTP client"))
            .err()
            .expect("expected build_factory to fail");
        assert!(err.to_string().contains("no api_key"));
    }

    #[test]
    fn gemini_with_api_key_builds_ok() {
        let p = provider("gemini", Some("key"), None);
        assert!(build_factory(&p, "model", &HttpClient::new().expect("test HTTP client")).is_ok());
    }

    #[test]
    fn openai_without_base_url_errs() {
        let p = provider("openai", Some("key"), None);
        let err = build_factory(&p, "model", &HttpClient::new().expect("test HTTP client"))
            .err()
            .expect("expected build_factory to fail");
        assert!(err.to_string().contains("no base_url"));
    }

    #[test]
    fn openai_with_base_url_and_no_key_builds_ok() {
        let p = provider("openai", None, Some("http://example.internal:1234/v1"));
        assert!(build_factory(&p, "model", &HttpClient::new().expect("test HTTP client")).is_ok());
    }

    #[test]
    fn openai_with_base_url_and_key_builds_ok() {
        let p = provider(
            "openai",
            Some("key"),
            Some("http://example.internal:1234/v1"),
        );
        assert!(build_factory(&p, "model", &HttpClient::new().expect("test HTTP client")).is_ok());
    }

    #[test]
    fn unsupported_kind_errs_naming_it() {
        let p = provider("mystery", None, None);
        let err = build_factory(&p, "model", &HttpClient::new().expect("test HTTP client"))
            .err()
            .expect("expected build_factory to fail");
        assert!(err.to_string().contains("mystery"));
    }

    #[test]
    fn positive_u32_passes_through_a_positive_value() {
        assert_eq!(positive_u32(Some(30)), Some(30));
    }

    #[test]
    fn positive_u32_treats_zero_or_negative_as_unset() {
        assert_eq!(positive_u32(Some(0)), None);
        assert_eq!(positive_u32(Some(-1)), None);
        assert_eq!(positive_u32(None), None);
    }

    #[test]
    fn positive_usize_passes_through_a_positive_value() {
        assert_eq!(positive_usize(Some(2)), Some(2));
    }

    #[test]
    fn positive_usize_treats_zero_or_negative_as_unset() {
        assert_eq!(positive_usize(Some(0)), None);
        assert_eq!(positive_usize(Some(-5)), None);
        assert_eq!(positive_usize(None), None);
    }
}
