//! In-process build event bus using tokio broadcast channel (ADR-0003).
//!
//! Job claims go directly through SQLite with optimistic locking (see `runners::claim_job`).
//! The scheduler provides:
//! - `tokio::sync::broadcast` for build state event fan-out (used by SSE in Phase 4)
//! - Startup recovery: reloads pending builds from DB (transitions stale 'scheduled' back to 'queued')

use std::sync::Arc;

use sqlx::Row;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Metadata for a queued build (used by background monitors for re-enqueue logging).
#[derive(Debug, Clone)]
pub struct QueuedJob {
    pub build_id: String,
    pub project_id: String,
    pub pipeline_id: String,
    pub build_number: i64,
    pub config_snapshot: serde_json::Value,
    pub commit_sha: Option<String>,
    pub branch: Option<String>,
    pub queued_at: i64,
}

/// A build state change event broadcast to all subscribers.
#[derive(Debug, Clone)]
pub struct BuildStateEvent {
    pub build_id: String,
    pub from_status: Option<String>,
    pub to_status: String,
    pub actor: Option<String>,
    pub reason: Option<String>,
    pub timestamp: i64,
}

/// A committed build-log append notification.
///
/// Log contents stay in SQLite; subscribers receive only the build and latest
/// sequence needed to decide whether to fetch.
#[derive(Debug, Clone)]
pub struct BuildLogEvent {
    pub build_id: String,
    pub latest_sequence: i64,
}

/// A runner state change event broadcast to all subscribers.
#[derive(Debug, Clone)]
pub struct RunnerStateEvent {
    pub runner_id: String,
    pub runner_name: String,
    pub from_status: String,
    pub to_status: String,
    pub timestamp: i64,
}

/// Central scheduler managing the build and runner event buses.
///
/// Job dispatch uses SQLite directly (via `runners::claim_job`) with optimistic
/// locking to prevent double-claims. The broadcast channels provide fan-out of
/// state change events for SSE subscribers and notification dispatch.
pub struct Scheduler {
    event_tx: broadcast::Sender<BuildStateEvent>,
    log_event_tx: broadcast::Sender<BuildLogEvent>,
    runner_event_tx: broadcast::Sender<RunnerStateEvent>,
}

impl Scheduler {
    /// Create a new scheduler with the given event bus capacity.
    pub fn new(capacity: usize) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(capacity);
        let (log_event_tx, _) = broadcast::channel(capacity);
        let (runner_event_tx, _) = broadcast::channel(16);
        Arc::new(Self {
            event_tx,
            log_event_tx,
            runner_event_tx,
        })
    }

    /// Subscribe to build state events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<BuildStateEvent> {
        self.event_tx.subscribe()
    }

    /// Publish a build state event to all subscribers.
    pub fn publish_event(&self, event: BuildStateEvent) {
        // Ignore send errors (no active subscribers is OK)
        let _ = self.event_tx.send(event);
    }

    /// Subscribe to committed build-log notifications.
    pub fn subscribe_log_events(&self) -> broadcast::Receiver<BuildLogEvent> {
        self.log_event_tx.subscribe()
    }

    /// Publish a committed build-log notification to all subscribers.
    pub fn publish_log_event(&self, event: BuildLogEvent) {
        let _ = self.log_event_tx.send(event);
    }

    /// Subscribe to runner state events.
    pub fn subscribe_runner_events(&self) -> broadcast::Receiver<RunnerStateEvent> {
        self.runner_event_tx.subscribe()
    }

    /// Publish a runner state event to all subscribers.
    pub fn publish_runner_event(&self, event: RunnerStateEvent) {
        let _ = self.runner_event_tx.send(event);
    }

    /// Recover stale builds on daemon startup.
    ///
    /// Finds builds stuck in 'scheduled' state (which means a claim was in progress
    /// when the daemon last shut down) and transitions them back to 'queued' so they
    /// can be re-claimed. Builds already in 'queued' state need no action since
    /// runners claim directly from SQLite.
    pub async fn reload_pending(&self, pool: &sqlx::SqlitePool) -> Result<usize, String> {
        let rows = sqlx::query("SELECT id FROM builds WHERE status = 'scheduled'")
            .fetch_all(pool)
            .await
            .map_err(|e| format!("failed to query stale scheduled builds: {}", e))?;

        let count = rows.len();
        for row in &rows {
            let build_id: String = row.get("id");
            match crate::builds::transition_build(
                pool,
                &build_id,
                oore_contract::BuildStatus::Queued,
                None,
                Some("daemon restart recovery"),
            )
            .await
            {
                Ok(_) => {
                    info!(build_id = %build_id, "recovered stale scheduled build back to queued");
                }
                Err(e) => {
                    warn!(build_id = %build_id, error = ?e, "failed to recover stale scheduled build");
                }
            }
        }

        if count > 0 {
            info!(count = count, "recovered stale builds on startup");
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::broadcast::error::RecvError;

    use super::*;

    #[tokio::test]
    async fn build_log_channel_is_bounded_and_retains_only_metadata() {
        let scheduler = Scheduler::new(1);
        let mut receiver = scheduler.subscribe_log_events();

        scheduler.publish_log_event(BuildLogEvent {
            build_id: "build-1".to_string(),
            latest_sequence: 1,
        });
        scheduler.publish_log_event(BuildLogEvent {
            build_id: "build-2".to_string(),
            latest_sequence: 2,
        });

        assert!(matches!(receiver.recv().await, Err(RecvError::Lagged(1))));
        let event = receiver.recv().await.unwrap();
        assert_eq!(event.build_id, "build-2");
        assert_eq!(event.latest_sequence, 2);
    }
}
