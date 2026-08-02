//! `DbSink` — the new engine's `RecordSink`, appending every `LogRecord` to
//! `assistant_events` (m_023), plus the lazy-resume and session-delete
//! helpers built on top of it (embedding.md §3 / §5).
//!
//! Lifecycle: this phase relies on `EngineConfig.idle_ttl` (set in
//! `engine.rs`) to auto-hibernate an idle session rather than an explicit
//! per-turn hibernate call — simpler, and sufficient since nothing is wired
//! to live traffic yet. The next phase's session-delete handler should call
//! [`delete_session_events`] (after telling the engine to close/forget the
//! session) to purge the log.

use anyhow::Context;
use entanglement_core::{Holly, SessionId};
use entanglement_runtime::persistence::RecordSink;
use entanglement_runtime::session_store::{LogPayload, LogRecord, integrity_gap, pair_records};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::entity::assistant_event;

/// Backlog cap on the writer task's channel. `RecordSink::append` must never
/// block (persistence.rs doc, embedding.md §3) — a full channel means the DB
/// writer has fallen far behind, so `append` sheds the record with an `Err`
/// rather than awaiting. Unlike a plain drop, the shed is not silent: it is
/// tallied in [`DbSink::dropped`] and turned into a [`LogPayload::Gap`]
/// tombstone by the writer task's periodic flush (see [`GAP_FLUSH_INTERVAL`]),
/// the same signal `entanglement_runtime`'s own broadcast-lag path writes —
/// so [`integrity_gap`] can actually see this kind of loss too, and
/// `resume_session` degrades gracefully instead of resuming over a silent
/// hole (issue #28).
const SINK_CHANNEL_CAPACITY: usize = 1024;

/// How often the writer task checks for accumulated backlog-drop counts and
/// persists them as `Gap` tombstones. A drop is tallied synchronously (in
/// `append`, off the critical never-block path) but the tombstone itself is
/// written on this cadence rather than inline, so `append` never has to wait
/// for a DB round-trip either.
const GAP_FLUSH_INTERVAL: Duration = Duration::from_secs(5);

/// A `RecordSink` that appends to `assistant_events` behind a bounded channel
/// and a dedicated writer task, so `append` (called from the persistence tap,
/// which reads `Holly`'s outbound broadcast) never awaits a DB round-trip.
pub struct DbSink {
    tx: mpsc::Sender<(SessionId, LogRecord)>,
    /// Per-root count of records shed since the last gap flush — see the
    /// module doc. A brief, uncontended lock, never held across an `.await`.
    dropped: std::sync::Arc<Mutex<HashMap<SessionId, u64>>>,
}

impl DbSink {
    pub fn new(db: DatabaseConnection) -> Self {
        let (tx, mut rx) = mpsc::channel::<(SessionId, LogRecord)>(SINK_CHANNEL_CAPACITY);
        let dropped = std::sync::Arc::new(Mutex::new(HashMap::new()));
        let writer_dropped = dropped.clone();
        tokio::spawn(async move {
            let mut gap_flush = tokio::time::interval(GAP_FLUSH_INTERVAL);
            gap_flush.tick().await; // first tick fires immediately; nothing to flush yet
            loop {
                tokio::select! {
                    biased;
                    maybe = rx.recv() => {
                        let Some((root, record)) = maybe else { break };
                        insert_record(&db, &root, &record).await;
                    }
                    _ = gap_flush.tick() => {
                        flush_gaps(&db, &writer_dropped).await;
                    }
                }
            }
            // Drain any drop tallied right before the channel closed.
            flush_gaps(&db, &writer_dropped).await;
        });
        DbSink { tx, dropped }
    }
}

impl RecordSink for DbSink {
    fn append(&self, root: &SessionId, record: &LogRecord) -> anyhow::Result<()> {
        self.tx
            .try_send((root.clone(), record.clone()))
            .map_err(|_| {
                *self
                    .dropped
                    .lock()
                    .unwrap()
                    .entry(root.clone())
                    .or_insert(0) += 1;
                anyhow::anyhow!("assistant_events sink backlog full, dropping record")
            })
    }
}

async fn insert_record(db: &DatabaseConnection, root: &SessionId, record: &LogRecord) {
    let payload = match serde_json::to_value(record) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, root = %root.0, "failed to serialize LogRecord");
            return;
        }
    };
    let row = assistant_event::ActiveModel {
        root_session_id: Set(root.0.clone()),
        payload: Set(payload),
        ..Default::default()
    };
    if let Err(e) = row.insert(db).await {
        tracing::error!(error = %e, root = %root.0, "failed to persist assistant event");
    }
}

/// Turn every root's tallied backlog-drop count into one `Gap` tombstone,
/// mirroring `entanglement_runtime::persistence::record_gap`'s shape for the
/// broadcast-lag case — see the module doc.
async fn flush_gaps(db: &DatabaseConnection, dropped: &Mutex<HashMap<SessionId, u64>>) {
    let pending: Vec<(SessionId, u64)> = dropped.lock().unwrap().drain().collect();
    for (root, count) in pending {
        tracing::warn!(root = %root.0, count, "sink backlog overflowed; recording a gap tombstone");
        let record = LogRecord::new(root.clone(), LogPayload::Gap { dropped: count });
        insert_record(db, &root, &record).await;
    }
}

/// Load `root`'s log from `assistant_events` and resume it into `holly`.
/// Nothing is loaded until this is actually called (lazy resume,
/// embedding.md §3) — e.g. the next phase's "open session" handler, when the
/// session isn't already live.
///
/// A detected [`LogPayload::Gap`] tombstone (a broadcast lag or a sink
/// backlog overflow) no longer hard-refuses the whole resume: replaying
/// *through* the gap would silently fold an incomplete history, but the
/// prefix strictly before it is intact, so that prefix is what gets resumed
/// — the tail after the gap is permanently lost, but the session itself
/// stays resumable forever after, instead of every future `ensure_live`
/// failing (issue #28). See [`truncate_at_gap`].
pub async fn resume_session(
    db: &DatabaseConnection,
    holly: &Holly,
    root: SessionId,
) -> anyhow::Result<SessionId> {
    let rows = assistant_event::Entity::find()
        .filter(assistant_event::Column::RootSessionId.eq(root.0.clone()))
        .order_by_asc(assistant_event::Column::Id)
        .all(db)
        .await
        .context("loading assistant_events for resume")?;

    let mut records: Vec<LogRecord> = rows
        .into_iter()
        .map(|r| serde_json::from_value(r.payload).context("deserializing LogRecord"))
        .collect::<anyhow::Result<_>>()?;

    if let Some((dropped, discarded)) = truncate_at_gap(&mut records) {
        tracing::warn!(
            root = %root.0,
            dropped,
            discarded,
            "resuming `{}` from the last good prefix before a persistence gap; \
             {discarded} record(s) after it are permanently lost",
            root.0
        );
    }

    holly
        .resume(root.clone(), pair_records(&records))
        .await
        .map_err(|_| anyhow::anyhow!("engine inbox closed"))
}

/// Truncate `records` to the prefix strictly before its first [`LogPayload::Gap`]
/// tombstone, if any. Returns `(total dropped per the tombstone(s), how many
/// trailing records were discarded)` — `None` when the log carries no gap, in
/// which case `records` is untouched.
fn truncate_at_gap(records: &mut Vec<LogRecord>) -> Option<(u64, usize)> {
    let gap_at = records
        .iter()
        .position(|r| matches!(r.payload, LogPayload::Gap { .. }))?;
    let dropped = integrity_gap(records).unwrap_or(0);
    let discarded = records.len() - gap_at;
    records.truncate(gap_at);
    Some((dropped, discarded))
}

/// Delete every persisted event for `root` — call after telling the engine to
/// close the session (`InMsg::CloseSession`), when the next phase's
/// session-delete handler removes the `assistant_sessions` row.
pub async fn delete_session_events(
    db: &DatabaseConnection,
    root: &SessionId,
) -> anyhow::Result<()> {
    assistant_event::Entity::delete_many()
        .filter(assistant_event::Column::RootSessionId.eq(root.0.clone()))
        .exec(db)
        .await
        .context("deleting assistant_events")?;
    Ok(())
}

#[cfg(test)]
mod tests;
