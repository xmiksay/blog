//! Page export via a remote `mdcast-server` (#64–#68 grew the in-process
//! integration; mdcast 0.4 replaced it with a thin HTTP client). The site no
//! longer compiles typst or spawns pandoc: `render` bridges the site's
//! markdown directives to plain markdown + synthesized diagram SVGs, `bundle`
//! declares every referenced asset by sha256 digest (bytes upload only when
//! the server's content-addressed cache misses), and `mdcast-client` handles
//! the `409 → upload → retry` negotiation. Splitting/classification and the
//! actual PDF/reveal.js rendering happen server-side, driven by the
//! `BrandSpec` sent with each request.
//!
//! `MDCAST_URL` unset → `AppState.mdcast` is `None` and both export routes
//! answer 503; nothing else in the site degrades.

mod bundle;
mod render;

use std::fmt;
use std::time::Duration;

use anyhow::Context;

pub use crate::markdown::BridgedMarkdown;
pub use bundle::build_bundle;
pub use render::{ExportFormat, render_page};

/// Why an export render failed, split by the HTTP status the routes owe the
/// caller. Never constructed from a panic — every failure path (transport,
/// server error, bad bundle) funnels through here.
#[derive(Debug)]
pub enum ExportError {
    /// The render server is unreachable or answering as a bad gateway —
    /// routes answer 503, the caller should retry later.
    Unavailable(String),
    /// Anything else (render failure, bad request, auth, digest mismatch) —
    /// routes answer 500 and log the full chain.
    Failed(anyhow::Error),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(msg) => write!(f, "export render server unavailable: {msg}"),
            Self::Failed(err) => write!(f, "{err:#}"),
        }
    }
}

impl From<mdcast_client::Error> for ExportError {
    fn from(err: mdcast_client::Error) -> Self {
        use mdcast_client::Error as E;
        let unavailable = matches!(
            &err,
            E::Transport(_)
                | E::Server {
                    status: 502..=504,
                    ..
                }
        );
        if unavailable {
            Self::Unavailable(err.to_string())
        } else {
            Self::Failed(err.into())
        }
    }
}

impl From<anyhow::Error> for ExportError {
    fn from(err: anyhow::Error) -> Self {
        Self::Failed(err)
    }
}

/// Construct the process-wide mdcast client. `mdcast-client` refuses an
/// empty token while a tokenless server ignores the bearer value, so an
/// unset `MDCAST_TOKEN` becomes a literal placeholder. The injected reqwest
/// client is the only place timeouts can be set — the builder has no knob.
pub fn build_client(url: &str, token: Option<&str>) -> anyhow::Result<mdcast_client::Client> {
    let http = reqwest12::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()
        .context("building the HTTP client for mdcast-server")?;
    mdcast_client::Client::builder(url)
        .token(token.unwrap_or("unauthenticated"))
        .http_client(http)
        .build()
        .with_context(|| format!("configuring the mdcast client for `{url}`"))
}

/// Header-safe filename component shared by the public and admin export
/// routes — anything outside ASCII alnum/-/_ becomes `-`, so a page path can
/// never smuggle a CR/LF or quote into the `Content-Disposition` header
/// value.
pub(crate) fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_filename_keeps_safe_chars_and_replaces_the_rest() {
        assert_eq!(sanitize_filename("about/team"), "about-team");
        assert_eq!(sanitize_filename("a_b-c123"), "a_b-c123");
        assert_eq!(sanitize_filename("héllo\r\n\""), "h-llo---");
    }

    #[test]
    fn build_client_accepts_a_tokenless_config_via_the_placeholder() {
        build_client("http://127.0.0.1:9", None)
            .expect("a placeholder token must satisfy the client's non-empty check");
    }

    #[test]
    fn build_client_rejects_a_non_http_url() {
        assert!(build_client("ftp://example.com", None).is_err());
    }

    #[test]
    fn transport_and_gateway_errors_map_to_unavailable() {
        let gateway = mdcast_client::Error::Server {
            status: 503,
            code: mdcast_client::mdcast_api::wire::ErrorCode::Internal,
            message: "down".into(),
        };
        assert!(matches!(
            ExportError::from(gateway),
            ExportError::Unavailable(_)
        ));

        let render_failed = mdcast_client::Error::Server {
            status: 500,
            code: mdcast_client::mdcast_api::wire::ErrorCode::RenderFailed,
            message: "typst error".into(),
        };
        assert!(matches!(
            ExportError::from(render_failed),
            ExportError::Failed(_)
        ));
    }
}
