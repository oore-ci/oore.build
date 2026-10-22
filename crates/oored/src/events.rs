//! Live build, runner and release events over SSE (`GET /v1/events`).
//!
//! A single change watcher scans SQLite for changed builds, runners and
//! releases while at least one client is connected, and fans the changes out
//! through a broadcast channel. Scanning the tables rather than relying on
//! publish calls catches every write path (runner claims, operator recovery,
//! retention, rollout monitor) with one cheap indexed query per tick. The
//! scheduler's broadcast events trigger an immediate scan so terminal states
//! arrive without waiting for the next tick.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use oore_contract::{
    ApiError, BuildChangedEvent, EventStreamReady, EventStreamResync, ReleaseChangedEvent,
    ReleaseState, RunnerChangedEvent,
};
use serde::Deserialize;
use sqlx::{Row, SqlitePool};
use tokio::sync::broadcast;
use tokio_stream::Stream;
use tracing::{info, warn};

use crate::AppState;
use crate::extractors::AuthUser;
use crate::logs::{StreamAuthorization, load_current_stream_session};
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::scheduler::Scheduler;
use crate::session::SessionInfo;
use crate::token::hash_token;
use crate::util::{api_err, now_unix};

/// How often the watcher scans for changes while clients are connected.
const WATCH_INTERVAL: Duration = Duration::from_millis(500);

/// Rows written up to this many seconds before the newest change seen are
/// rescanned, so a write that commits late with an older `updated_at` is
/// still picked up.
const WATCH_LOOKBACK_SECS: i64 = 2;

/// A scan returning this many rows means a burst; clients are told to resync.
const WATCH_BATCH_LIMIT: i64 = 500;

/// Credential and permission revalidation cadence for open streams.
const AUTHORIZATION_CHECK_INTERVAL: Duration = Duration::from_secs(5);

/// Comment frames keep idle connections alive through HAProxy and Warpgate.
/// Keep this well below the proxies' `timeout client` / `timeout server`.
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);

/// Reconnect delay suggested to EventSource-compatible clients.
const CLIENT_RETRY: Duration = Duration::from_secs(3);

const MAX_STREAMS_PER_USER: usize = 8;
const MAX_STREAMS_GLOBAL: usize = 256;

const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// A change fanned out to every connected stream before per-user filtering.
#[derive(Debug, Clone)]
pub enum LiveEvent {
    Build(BuildChangedEvent),
    Runner(RunnerChangedEvent),
    Release(ReleaseChangedEvent),
    /// The watcher could not describe every change; clients must refetch.
    Resync(&'static str),
}

/// Shared event fan-out and stream admission.
pub struct EventHub {
    tx: broadcast::Sender<LiveEvent>,
    active: Arc<Mutex<ActiveStreams>>,
}

#[derive(Default)]
struct ActiveStreams {
    by_user: HashMap<String, usize>,
    total: usize,
}

impl EventHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            tx,
            active: Arc::new(Mutex::new(ActiveStreams::default())),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LiveEvent> {
        self.tx.subscribe()
    }

    fn has_subscribers(&self) -> bool {
        self.tx.receiver_count() > 0
    }

    fn publish(&self, event: LiveEvent) {
        let _ = self.tx.send(event);
    }

    fn acquire(&self, user_id: &str) -> Option<StreamAdmission> {
        let mut active = lock(&self.active);
        if active.total >= MAX_STREAMS_GLOBAL
            || active.by_user.get(user_id).copied().unwrap_or(0) >= MAX_STREAMS_PER_USER
        {
            return None;
        }
        active.total += 1;
        *active.by_user.entry(user_id.to_string()).or_default() += 1;
        Some(StreamAdmission {
            active: self.active.clone(),
            user_id: user_id.to_string(),
        })
    }
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new()
    }
}

fn lock(active: &Mutex<ActiveStreams>) -> MutexGuard<'_, ActiveStreams> {
    active
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct StreamAdmission {
    active: Arc<Mutex<ActiveStreams>>,
    user_id: String,
}

impl Drop for StreamAdmission {
    fn drop(&mut self) {
        let mut active = lock(&self.active);
        active.total -= 1;
        if let Some(count) = active.by_user.get_mut(&self.user_id) {
            *count -= 1;
            if *count == 0 {
                active.by_user.remove(&self.user_id);
            }
        }
    }
}

// ── Change watcher ──────────────────────────────────────────────

/// Start the background task that turns database changes into live events.
pub fn start_change_watcher(pool: SqlitePool, scheduler: Arc<Scheduler>, hub: Arc<EventHub>) {
    tokio::spawn(async move {
        let mut build_rx = scheduler.subscribe_events();
        let mut runner_rx = scheduler.subscribe_runner_events();
        let mut release_rx = scheduler.subscribe_release_events();
        let mut tick = tokio::time::interval(WATCH_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut cursor: Option<ChangeCursor> = None;

        loop {
            tokio::select! {
                _ = tick.tick() => {}
                _ = recv_or_pending(&mut build_rx) => {}
                _ = recv_or_pending(&mut runner_rx) => {}
                _ = recv_or_pending(&mut release_rx) => {}
            }

            if !hub.has_subscribers() {
                cursor = None;
                continue;
            }

            let result = if let Some(cursor) = cursor.as_mut() {
                cursor.scan(&pool).await
            } else {
                match ChangeCursor::baseline(&pool).await {
                    Ok(baseline) => {
                        cursor = Some(baseline);
                        Ok(Vec::new())
                    }
                    Err(error) => Err(error),
                }
            };
            match result {
                Ok(events) => {
                    for event in events {
                        if matches!(event, LiveEvent::Resync(_)) {
                            cursor = None;
                        }
                        hub.publish(event);
                    }
                }
                Err(error) => warn!(error = %error, "live event scan failed"),
            }
        }
    });
}

/// Wait for a scheduler event; a closed channel never wakes the watcher again.
async fn recv_or_pending<T: Clone>(rx: &mut broadcast::Receiver<T>) {
    match rx.recv().await {
        Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
        Err(broadcast::error::RecvError::Closed) => std::future::pending().await,
    }
}

/// Rows already reported inside the lookback window, keyed by id.
#[derive(Default)]
struct RecentRows {
    since: i64,
    seen: HashMap<String, (i64, String)>,
}

impl RecentRows {
    fn starting_at(since: i64) -> Self {
        Self {
            since,
            seen: HashMap::new(),
        }
    }

    fn scan_from(&self) -> i64 {
        self.since - WATCH_LOOKBACK_SECS
    }

    /// Record a row; returns whether it is new or changed since last reported.
    fn observe(&mut self, id: &str, updated_at: i64, version: String) -> bool {
        let entry = (updated_at, version);
        if self.seen.get(id) == Some(&entry) {
            return false;
        }
        self.since = self.since.max(updated_at);
        self.seen.insert(id.to_string(), entry);
        true
    }

    fn prune(&mut self) {
        let floor = self.scan_from();
        self.seen.retain(|_, (updated_at, _)| *updated_at >= floor);
    }
}

struct ChangeCursor {
    builds: RecentRows,
    releases: RecentRows,
    runners: HashMap<String, (String, String)>,
}

impl ChangeCursor {
    /// Start watching from now without reporting existing rows.
    async fn baseline(pool: &SqlitePool) -> Result<Self, sqlx::Error> {
        let now = now_unix();
        let mut cursor = Self {
            builds: RecentRows::starting_at(now),
            releases: RecentRows::starting_at(now),
            runners: load_runners(pool).await?,
        };
        // Mark rows inside the lookback window as already reported.
        cursor.scan_builds(pool).await?;
        cursor.scan_releases(pool).await?;
        Ok(cursor)
    }

    async fn scan(&mut self, pool: &SqlitePool) -> Result<Vec<LiveEvent>, sqlx::Error> {
        let mut events = Vec::new();
        let (builds, builds_overflowed) = self.scan_builds(pool).await?;
        let (releases, releases_overflowed) = self.scan_releases(pool).await?;
        if builds_overflowed || releases_overflowed {
            return Ok(vec![LiveEvent::Resync("burst")]);
        }
        events.extend(builds.into_iter().map(LiveEvent::Build));
        events.extend(releases.into_iter().map(LiveEvent::Release));
        events.extend(self.scan_runners(pool).await?);
        Ok(events)
    }

    async fn scan_builds(
        &mut self,
        pool: &SqlitePool,
    ) -> Result<(Vec<BuildChangedEvent>, bool), sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, project_id, build_number, status, updated_at FROM builds \
             WHERE updated_at >= ?1 ORDER BY updated_at ASC LIMIT ?2",
        )
        .bind(self.builds.scan_from())
        .bind(WATCH_BATCH_LIMIT)
        .fetch_all(pool)
        .await?;
        let overflowed = rows.len() as i64 >= WATCH_BATCH_LIMIT;
        let mut changed = Vec::new();
        for row in rows {
            let event = BuildChangedEvent {
                build_id: row.get("id"),
                project_id: row.get("project_id"),
                build_number: row.get("build_number"),
                status: row.get("status"),
                updated_at: row.get("updated_at"),
            };
            if self
                .builds
                .observe(&event.build_id, event.updated_at, event.status.clone())
            {
                changed.push(event);
            }
        }
        self.builds.prune();
        Ok((changed, overflowed))
    }

    async fn scan_releases(
        &mut self,
        pool: &SqlitePool,
    ) -> Result<(Vec<ReleaseChangedEvent>, bool), sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, project_id, build_id, state, rollout_fraction, updated_at FROM releases \
             WHERE updated_at >= ?1 ORDER BY updated_at ASC LIMIT ?2",
        )
        .bind(self.releases.scan_from())
        .bind(WATCH_BATCH_LIMIT)
        .fetch_all(pool)
        .await?;
        let overflowed = rows.len() as i64 >= WATCH_BATCH_LIMIT;
        let mut changed = Vec::new();
        for row in rows {
            let state: String = row.get("state");
            let Ok(parsed_state) = state.parse::<ReleaseState>() else {
                continue;
            };
            let event = ReleaseChangedEvent {
                release_id: row.get("id"),
                project_id: row.get("project_id"),
                build_id: row.get("build_id"),
                state: parsed_state,
                rollout_fraction: row.get("rollout_fraction"),
                updated_at: row.get("updated_at"),
            };
            let version = format!("{state}:{}", event.rollout_fraction);
            if self
                .releases
                .observe(&event.release_id, event.updated_at, version)
            {
                changed.push(event);
            }
        }
        self.releases.prune();
        Ok((changed, overflowed))
    }

    async fn scan_runners(&mut self, pool: &SqlitePool) -> Result<Vec<LiveEvent>, sqlx::Error> {
        let current = load_runners(pool).await?;
        let mut events = Vec::new();
        for (id, (name, status)) in &current {
            if self.runners.get(id) != Some(&(name.clone(), status.clone())) {
                events.push(LiveEvent::Runner(RunnerChangedEvent {
                    runner_id: id.clone(),
                    name: name.clone(),
                    status: status.clone(),
                    removed: false,
                }));
            }
        }
        for (id, (name, status)) in &self.runners {
            if !current.contains_key(id) {
                events.push(LiveEvent::Runner(RunnerChangedEvent {
                    runner_id: id.clone(),
                    name: name.clone(),
                    status: status.clone(),
                    removed: true,
                }));
            }
        }
        self.runners = current;
        Ok(events)
    }
}

async fn load_runners(pool: &SqlitePool) -> Result<HashMap<String, (String, String)>, sqlx::Error> {
    let rows = sqlx::query("SELECT id, name, status FROM runners")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get("id"), (row.get("name"), row.get("status"))))
        .collect())
}

// ── Handler ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct EventStreamQuery {
    pub project_id: Option<String>,
}

/// `GET /v1/events` — SSE stream of build, runner and release changes.
pub async fn stream_events(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<EventStreamQuery>,
    headers: axum::http::HeaderMap,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let session = auth.0;
    let credential_hash = hash_token(crate::util::extract_bearer(&headers).ok_or_else(|| {
        api_err(
            StatusCode::UNAUTHORIZED,
            "missing_auth",
            "Authentication required",
        )
    })?);
    let project_id = query.project_id.filter(|id| !id.trim().is_empty());

    if let Some(project_id) = &project_id {
        let effective = resolve_effective_project_role(
            &state.db,
            &session.user_id,
            &session.role,
            project_id,
            &session.auth_source,
        )
        .await?;
        require_project_permission(&effective, ProjectPermission::Read)?;
    }

    let admission = state.events.acquire(&session.user_id).ok_or_else(|| {
        api_err(
            StatusCode::TOO_MANY_REQUESTS,
            "stream_limit",
            "Too many active event streams",
        )
    })?;
    let runners_visible = can_read_runners(&state, &session).await;
    let authorization = StreamAuthorization {
        user_id: session.user_id.clone(),
        credential_hash,
        auth_source: session.auth_source.clone(),
        expires_at: session.expires_at,
    };
    let receiver = state.events.subscribe();

    info!(user = %session.email, project_id = ?project_id, "event stream opened");

    let stream = event_stream(
        state,
        session,
        authorization,
        project_id,
        runners_visible,
        receiver,
        admission,
    );
    let sse = Sse::new(stream).keep_alive(KeepAlive::new().interval(KEEPALIVE_INTERVAL));
    // `no-transform` stops intermediaries from compressing (and so buffering)
    // the stream; `X-Accel-Buffering` does the same for nginx-style proxies.
    Ok((
        [
            (
                axum::http::header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache, no-transform"),
            ),
            (
                HeaderName::from_static("x-accel-buffering"),
                HeaderValue::from_static("no"),
            ),
        ],
        sse,
    )
        .into_response())
}

async fn can_read_runners(state: &AppState, session: &SessionInfo) -> bool {
    state.enforcer.check(&session.role, "runners", "read").await
}

/// Project read checks, cached between authorization checks.
struct ProjectAccess {
    readable: HashMap<String, bool>,
}

impl ProjectAccess {
    /// `None` when the role lookup failed. Failures are not cached, so the
    /// next event for the project checks again.
    async fn can_read(
        &mut self,
        pool: &SqlitePool,
        session: &SessionInfo,
        project_id: &str,
    ) -> Option<bool> {
        if let Some(readable) = self.readable.get(project_id) {
            return Some(*readable);
        }
        let effective = resolve_effective_project_role(
            pool,
            &session.user_id,
            &session.role,
            project_id,
            &session.auth_source,
        )
        .await
        .ok()?;
        let readable = require_project_permission(&effective, ProjectPermission::Read).is_ok();
        self.readable.insert(project_id.to_string(), readable);
        Some(readable)
    }

    /// The frame to send for a project-scoped change: the event itself, or a
    /// resync when access could not be checked, so the client refetches
    /// instead of silently missing the change.
    async fn project_event(
        &mut self,
        pool: &SqlitePool,
        session: &SessionInfo,
        project_id: &str,
        event: impl FnOnce() -> Event,
    ) -> Option<Event> {
        match self.can_read(pool, session, project_id).await {
            Some(true) => Some(event()),
            Some(false) => None,
            None => {
                warn!(
                    project_id,
                    "project access check failed; asking client to resync"
                );
                Some(json_event(
                    "resync",
                    &EventStreamResync {
                        reason: "access_check_failed".to_string(),
                    },
                ))
            }
        }
    }
}

fn json_event(name: &str, data: &impl serde::Serialize) -> Event {
    Event::default()
        .event(name)
        .data(serde_json::to_string(data).unwrap_or_default())
}

fn event_stream(
    state: Arc<AppState>,
    mut session: SessionInfo,
    authorization: StreamAuthorization,
    project_filter: Option<String>,
    mut runners_visible: bool,
    mut receiver: broadcast::Receiver<LiveEvent>,
    admission: StreamAdmission,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        let _admission = admission;
        let pool = state.db.clone();
        let mut access = ProjectAccess { readable: HashMap::new() };
        let mut authorization_interval = tokio::time::interval_at(
            tokio::time::Instant::now() + AUTHORIZATION_CHECK_INTERVAL,
            AUTHORIZATION_CHECK_INTERVAL,
        );
        authorization_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        yield Ok(json_event("ready", &EventStreamReady {
            project_id: project_filter.clone(),
            runners: runners_visible,
        })
        .retry(CLIENT_RETRY));

        loop {
            tokio::select! {
                biased;
                _ = authorization_interval.tick() => {
                    let current = if authorization.expires_at > now_unix() {
                        load_current_stream_session(&pool, &authorization).await.ok().flatten()
                    } else {
                        None
                    };
                    let Some(current) = current.filter(|s| s.user_id == authorization.user_id) else {
                        yield Ok(Event::default().event("done").data("authorization_ended"));
                        break;
                    };
                    session = current;
                    access.readable.clear();
                    // A failed lookup keeps the stream open; events stay
                    // gated by the per-event check until it succeeds.
                    if let Some(project_id) = &project_filter
                        && access.can_read(&pool, &session, project_id).await == Some(false)
                    {
                        yield Ok(Event::default().event("done").data("authorization_ended"));
                        break;
                    }
                    runners_visible = can_read_runners(&state, &session).await;
                }
                received = receiver.recv() => match received {
                    Ok(LiveEvent::Build(event)) => {
                        if project_filter.as_ref().is_none_or(|id| *id == event.project_id)
                            && let Some(frame) = access
                                .project_event(&pool, &session, &event.project_id, || json_event("build", &event))
                                .await
                        {
                            yield Ok(frame);
                        }
                    }
                    Ok(LiveEvent::Release(event)) => {
                        if project_filter.as_ref().is_none_or(|id| *id == event.project_id)
                            && let Some(frame) = access
                                .project_event(&pool, &session, &event.project_id, || json_event("release", &event))
                                .await
                        {
                            yield Ok(frame);
                        }
                    }
                    Ok(LiveEvent::Runner(event)) => {
                        if runners_visible {
                            yield Ok(json_event("runner", &event));
                        }
                    }
                    Ok(LiveEvent::Resync(reason)) => {
                        yield Ok(json_event("resync", &EventStreamResync { reason: reason.to_string() }));
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "event stream receiver lagged; asking client to resync");
                        yield Ok(json_event("resync", &EventStreamResync { reason: "lagged".to_string() }));
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_rows_reports_each_change_once() {
        let mut rows = RecentRows::starting_at(100);
        assert!(rows.observe("a", 100, "queued".into()));
        assert!(!rows.observe("a", 100, "queued".into()));
        assert!(rows.observe("a", 100, "running".into()));
        assert!(rows.observe("a", 101, "running".into()));
        assert_eq!(rows.since, 101);
    }

    #[test]
    fn recent_rows_keeps_lookback_window() {
        let mut rows = RecentRows::starting_at(100);
        rows.observe("old", 97, "queued".into());
        rows.observe("new", 105, "queued".into());
        rows.prune();
        assert_eq!(rows.scan_from(), 103);
        assert!(!rows.seen.contains_key("old"));
        assert!(rows.seen.contains_key("new"));
    }

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u1', 'dev@example.com', 'sub', 'developer', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) VALUES ('p1', 'App', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) VALUES ('pl1', 'p1', 'main', 0, 0)",
            "INSERT INTO runners (id, name, token_hash, status, capabilities, created_at, updated_at) \
             VALUES ('r1', 'mac', 'hash', 'online', '{}', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    async fn next_event(rx: &mut broadcast::Receiver<LiveEvent>) -> LiveEvent {
        tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("event within timeout")
            .expect("open channel")
    }

    #[tokio::test]
    async fn watcher_reports_build_and_runner_changes() {
        let pool = pool().await;
        let hub = Arc::new(EventHub::new());
        let mut rx = hub.subscribe();
        start_change_watcher(pool.clone(), Scheduler::new(16), hub.clone());
        // Let the first tick take its baseline.
        tokio::time::sleep(WATCH_INTERVAL * 2).await;

        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, queued_at, created_at, updated_at) \
             VALUES ('b1', 'p1', 'pl1', 7, 'queued', 'manual', ?1, ?1, ?1)",
        )
        .bind(now_unix())
        .execute(&pool)
        .await
        .unwrap();
        match next_event(&mut rx).await {
            LiveEvent::Build(event) => {
                assert_eq!(event.build_id, "b1");
                assert_eq!(event.project_id, "p1");
                assert_eq!(event.status, "queued");
            }
            other => panic!("unexpected event {other:?}"),
        }

        sqlx::query("UPDATE builds SET status = 'running', updated_at = ?1 WHERE id = 'b1'")
            .bind(now_unix())
            .execute(&pool)
            .await
            .unwrap();
        match next_event(&mut rx).await {
            LiveEvent::Build(event) => assert_eq!(event.status, "running"),
            other => panic!("unexpected event {other:?}"),
        }

        sqlx::query("UPDATE runners SET status = 'busy' WHERE id = 'r1'")
            .execute(&pool)
            .await
            .unwrap();
        match next_event(&mut rx).await {
            LiveEvent::Runner(event) => {
                assert_eq!(event.runner_id, "r1");
                assert_eq!(event.status, "busy");
                assert!(!event.removed);
            }
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[tokio::test]
    async fn failed_access_check_asks_for_resync_and_is_not_cached() {
        let pool = pool().await;
        let session = SessionInfo {
            user_id: "u1".to_string(),
            email: "dev@example.com".to_string(),
            oidc_subject: "sub".to_string(),
            role: "developer".to_string(),
            expires_at: i64::MAX,
            auth_source: crate::session::AuthSource::Session,
        };
        let mut access = ProjectAccess {
            readable: HashMap::new(),
        };
        let build = || Event::default().event("build").data("{}");
        sqlx::query("DROP TABLE project_members")
            .execute(&pool)
            .await
            .unwrap();

        let frame = access
            .project_event(&pool, &session, "p1", build)
            .await
            .expect("a frame");

        assert!(format!("{frame:?}").contains("event: resync"), "{frame:?}");
        assert!(access.readable.is_empty());
    }

    #[test]
    fn admission_limits_streams_per_user() {
        let hub = EventHub::new();
        let held: Vec<_> = (0..MAX_STREAMS_PER_USER)
            .map(|_| hub.acquire("u").expect("admitted"))
            .collect();
        assert!(hub.acquire("u").is_none());
        assert!(hub.acquire("v").is_some());
        drop(held);
        assert!(hub.acquire("u").is_some());
    }
}
