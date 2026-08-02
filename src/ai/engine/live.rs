//! [`LiveSessions`] — which engine sessions this process has confirmed have a
//! live in-memory `Holly` task. Split out of `engine.rs` to keep that file
//! under the project's 400-line cap.
//!
//! Why a cache is needed at all: `Holly::resume` refuses an already-live id
//! (see its doc), while sending any other `InMsg` to an id this process has
//! never touched lazily spawns a **blank** session instead of replaying
//! history. So a caller has to resume exactly once, before its first send —
//! this set is what makes it "once" rather than "every message". Deliberately
//! a flat per-process set, not a generalized cache (KISS).
//!
//! Three things put an id in:
//!
//! - [`SiteEngine::ensure_live`][super::SiteEngine::ensure_live], after a
//!   successful resume;
//! - [`SiteEngine::mark_live`][super::SiteEngine::mark_live], right after
//!   minting a brand-new id (whose first `InMsg` correctly spawns it blank,
//!   since it has no history yet);
//! - `engine.rs`'s session watcher, on every observed `SessionStarted` — the
//!   only way a sub-agent child ever gets in (#101), since nothing on this
//!   side mints a child's id.
//!
//! One thing takes an id out: the same watcher, on `SessionHibernated`/
//! `SessionEnded` (`session_tree::evict_on_hibernate_or_end`). That matters
//! because `Holly`'s own idle-TTL sweep evicts a settled session from *its*
//! bookkeeping without this site ever calling `hibernate`, so without the
//! listener this cache would keep vouching for a session `Holly` has already
//! forgotten — and the next `InMsg` would respawn it blank.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashSet;
use entanglement_core::SessionId;

/// How many times [`LiveSessions::await_live`] checks before concluding a
/// session really has no in-memory task, and how long it waits between checks.
/// Mirrors `session_tree.rs`'s `SESSION_PARENT_RETRY_*`: both wait on the same
/// watcher task folding the same `SessionStarted` event.
const LIVE_CHECK_ATTEMPTS: u32 = 5;
const LIVE_CHECK_DELAY: Duration = Duration::from_millis(5);

/// See the module doc. `Arc`-wrapped inside so the watcher task can hold its
/// own handle onto the same set.
#[derive(Clone, Default)]
pub(super) struct LiveSessions(Arc<DashSet<SessionId>>);

impl LiveSessions {
    /// The raw set, for the watcher task that folds `SessionStarted`/
    /// `SessionHibernated`/`SessionEnded` into it from its own tokio task.
    pub(super) fn handle(&self) -> Arc<DashSet<SessionId>> {
        self.0.clone()
    }

    pub(super) fn mark(&self, session: SessionId) {
        self.0.insert(session);
    }

    pub(super) fn forget(&self, session: &SessionId) {
        self.0.remove(session);
    }

    pub(super) fn contains(&self, session: &SessionId) -> bool {
        self.0.contains(session)
    }

    /// [`contains`][Self::contains], retried briefly before answering `false`
    /// (#101).
    ///
    /// The retry closes the same TOCTOU window `session_tree.rs`'s
    /// `user_id_from_session_awaiting` documents: a resume cascade (ADR-0112)
    /// re-materializes a root's descendants, but this set is written by the
    /// watcher — an *independent* broadcast subscriber, with no ordering
    /// guarantee against the handler that just called `ensure_live`. Answering
    /// `false` too eagerly would send a `Resume` for a session that is already
    /// back, which `Holly` refuses with a supervisor `Error` on that session's
    /// own stream. The watcher's work is a couple of map writes, so in the
    /// normal case this returns on the first check.
    pub(super) async fn await_live(&self, session: &SessionId) -> bool {
        for attempt in 0..LIVE_CHECK_ATTEMPTS {
            if self.contains(session) {
                return true;
            }
            if attempt + 1 < LIVE_CHECK_ATTEMPTS {
                tokio::time::sleep(LIVE_CHECK_DELAY).await;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn await_live_returns_true_once_a_late_mark_lands() {
        let live = LiveSessions::default();
        let session = SessionId::new_uuid();
        assert!(!live.contains(&session));

        let late = live.clone();
        let marked = session.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(6)).await;
            late.mark(marked);
        });

        assert!(live.await_live(&session).await);
    }

    #[tokio::test]
    async fn await_live_gives_up_on_a_session_that_never_starts() {
        let live = LiveSessions::default();
        assert!(!live.await_live(&SessionId::new_uuid()).await);
    }
}
