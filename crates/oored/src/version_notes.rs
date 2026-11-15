//! Release notes per app version (#957).
//!
//! Notes belong to an app version such as 2.5.1, not to a pipeline or a
//! single build: every QA rebuild of the version shows the same notes, and
//! every store upload and promotion of it sends them. They come from one of
//! three places:
//!
//! - `commits`: drafted from commit titles since the last version that
//!   reached a store, and drafted again on each build until someone edits
//!   them.
//! - `manual`: written in Oore (or with `oore build --notes`).
//! - `repository`: read by the runner from the `.oore.yaml`
//!   `release_notes` file or directory at the build's commit. The repository
//!   is the source of truth, so Oore does not edit these.
//!
//! The same module also fills in a build's own Changes (its commit list)
//! for every trigger, not only for builds started from the web.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, AppStoreReleaseNote, ReleaseNotesDraftResponse, ReleaseNotesLocale,
    ReleaseNotesSource, RepositoryReleaseNotes, UpdateVersionReleaseNotesRequest,
    VersionReleaseNotes,
};
use sqlx::{Row, SqlitePool};
use tracing::{error, warn};

use crate::AppState;
use crate::extractors::AuthUser;
use crate::integrations::CommitSummary;
use crate::project_rbac::{ProjectPermission, require_project_permission};
use crate::releases::project_role;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiFailure = (StatusCode, Json<ApiError>);
type ApiResult<T> = Result<Json<T>, ApiFailure>;

/// The longest notes any store takes (App Store Connect's "What's New").
pub(crate) const MAX_NOTES_CHARS: usize = 4000;
const MAX_LOCALES: usize = 50;
const DEFAULT_LOCALE: &str = "en-US";

fn store_err(message: &'static str) -> impl FnOnce(sqlx::Error) -> ApiFailure {
    move |e| {
        error!(error = %e, "{message}");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", message)
    }
}

const SELECT: &str = "SELECT n.project_id, n.version_name, n.text, n.locales, n.source, \
     n.source_path, n.drafted_since, n.updated_at, u.email AS updated_by \
     FROM version_release_notes n LEFT JOIN users u ON u.id = n.updated_by";

fn source_str(source: ReleaseNotesSource) -> &'static str {
    match source {
        ReleaseNotesSource::Commits => "commits",
        ReleaseNotesSource::Manual => "manual",
        ReleaseNotesSource::Repository => "repository",
    }
}

fn parse_source(value: &str) -> ReleaseNotesSource {
    match value {
        "manual" => ReleaseNotesSource::Manual,
        "repository" => ReleaseNotesSource::Repository,
        _ => ReleaseNotesSource::Commits,
    }
}

fn notes_from_row(row: &sqlx::sqlite::SqliteRow) -> VersionReleaseNotes {
    let locales: String = row.get("locales");
    VersionReleaseNotes {
        project_id: row.get("project_id"),
        version_name: row.get("version_name"),
        text: row.get("text"),
        locales: serde_json::from_str(&locales).unwrap_or_default(),
        source: parse_source(row.get::<String, _>("source").as_str()),
        source_path: row.get("source_path"),
        drafted_since: row.get("drafted_since"),
        updated_at: row.get("updated_at"),
        updated_by: row.get("updated_by"),
    }
}

/// The notes of `version_name` in a project.
pub(crate) async fn load(
    pool: &SqlitePool,
    project_id: &str,
    version_name: &str,
) -> sqlx::Result<Option<VersionReleaseNotes>> {
    Ok(sqlx::query(sqlx::AssertSqlSafe(format!(
        "{SELECT} WHERE n.project_id = ?1 AND n.version_name = ?2"
    )))
    .bind(project_id)
    .bind(version_name)
    .fetch_optional(pool)
    .await?
    .as_ref()
    .map(notes_from_row))
}

/// The notes of a build's app version, once the runner reported it.
pub(crate) async fn for_build(
    pool: &SqlitePool,
    build_id: &str,
) -> sqlx::Result<Option<VersionReleaseNotes>> {
    Ok(sqlx::query(sqlx::AssertSqlSafe(format!(
        "{SELECT} JOIN builds b ON b.project_id = n.project_id AND b.version_name = n.version_name \
         WHERE b.id = ?1"
    )))
    .bind(build_id)
    .fetch_optional(pool)
    .await?
    .as_ref()
    .map(notes_from_row))
}

/// `for_build`, with empty notes as none.
pub(crate) async fn sendable_for_build(
    pool: &SqlitePool,
    build_id: &str,
) -> sqlx::Result<Option<VersionReleaseNotes>> {
    Ok(for_build(pool, build_id)
        .await?
        .filter(|notes| !notes.text.trim().is_empty() || !notes.locales.is_empty()))
}

/// The notes as App Store Connect takes them: the default text as en-US,
/// then every other language. Empty languages are left out.
pub(crate) fn app_store_notes(notes: &VersionReleaseNotes) -> Vec<AppStoreReleaseNote> {
    let mut out = Vec::new();
    if !notes.text.trim().is_empty() {
        out.push(AppStoreReleaseNote {
            locale: DEFAULT_LOCALE.to_string(),
            whats_new: limit(&notes.text),
        });
    }
    for locale in &notes.locales {
        if locale.text.trim().is_empty() || out.iter().any(|note| note.locale == locale.locale) {
            continue;
        }
        out.push(AppStoreReleaseNote {
            locale: locale.locale.clone(),
            whats_new: limit(&locale.text),
        });
    }
    out
}

fn limit(text: &str) -> String {
    text.trim().chars().take(MAX_NOTES_CHARS).collect()
}

struct Upsert<'a> {
    project_id: &'a str,
    version_name: &'a str,
    text: &'a str,
    locales: &'a [ReleaseNotesLocale],
    source: ReleaseNotesSource,
    source_path: Option<&'a str>,
    drafted_since: Option<&'a str>,
    updated_by: Option<&'a str>,
}

/// Saves notes. `replace` decides which saved sources the new notes may
/// overwrite; a saved record of another source is kept.
async fn upsert(
    pool: &SqlitePool,
    notes: Upsert<'_>,
    replace: &[ReleaseNotesSource],
) -> sqlx::Result<bool> {
    let replaceable = replace
        .iter()
        .map(|source| format!("'{}'", source_str(*source)))
        .collect::<Vec<_>>()
        .join(", ");
    let condition = if replaceable.is_empty() {
        "0".to_string()
    } else {
        format!("version_release_notes.source IN ({replaceable})")
    };
    let locales = serde_json::to_string(notes.locales).unwrap_or_else(|_| "[]".into());
    // `condition` is built from the fixed source names above, never input.
    let result = sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO version_release_notes \
             (project_id, version_name, text, locales, source, source_path, drafted_since, \
              updated_at, updated_by) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
         ON CONFLICT (project_id, version_name) DO UPDATE SET \
             text = excluded.text, locales = excluded.locales, source = excluded.source, \
             source_path = excluded.source_path, drafted_since = excluded.drafted_since, \
             updated_at = excluded.updated_at, updated_by = excluded.updated_by \
         WHERE {condition}"
    )))
    .bind(notes.project_id)
    .bind(notes.version_name)
    .bind(limit(notes.text))
    .bind(locales)
    .bind(source_str(notes.source))
    .bind(notes.source_path)
    .bind(notes.drafted_since)
    .bind(now_unix())
    .bind(notes.updated_by)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

// ── Drafts ──────────────────────────────────────────────────────

/// Commit titles as notes people read: one line each, merges and repeats
/// left out, no authors.
pub(crate) fn notes_from_commits(commits: &[CommitSummary]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for commit in commits {
        if commit.title.trim_start().starts_with("Merge ") {
            continue;
        }
        let title = readable_title(&commit.title);
        if title.is_empty() || lines.iter().any(|line| line[2..] == title) {
            continue;
        }
        lines.push(format!("- {title}"));
    }
    let mut text = String::new();
    for (index, line) in lines.iter().enumerate() {
        let more = lines.len() - index;
        if text.chars().count() + line.chars().count() + 40 > MAX_NOTES_CHARS {
            text.push_str(&format!("- …and {more} more changes\n"));
            break;
        }
        text.push_str(line);
        text.push('\n');
    }
    text.trim_end().to_string()
}

/// A commit title as people read it: "feat(login): faster sign in" reads
/// as "Faster sign in".
fn readable_title(title: &str) -> String {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let conventional = |prefix: &str| {
        let kind = prefix.trim_end_matches('!');
        let kind = match kind.split_once('(') {
            Some((kind, scope)) if scope.ends_with(')') => kind,
            Some(_) => "",
            None => kind,
        };
        !kind.is_empty() && kind.chars().all(|c| c.is_ascii_lowercase())
    };
    let rest = match title.split_once(": ") {
        Some((prefix, rest)) if conventional(prefix) => rest,
        _ => title.as_str(),
    };
    let mut chars = rest.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The version a draft for `version_name` starts after, and its commit:
/// the newest build of another version that reached a store, else the
/// newest succeeded build of another version.
async fn draft_base(
    pool: &SqlitePool,
    project_id: &str,
    version_name: &str,
) -> sqlx::Result<Option<(String, String)>> {
    let released: Option<(String, String)> = sqlx::query_as(
        "SELECT b.version_name, b.commit_sha FROM builds b \
         WHERE b.project_id = ?1 AND b.version_name IS NOT NULL AND b.version_name != ?2 \
           AND b.commit_sha IS NOT NULL \
           AND EXISTS (SELECT 1 FROM releases r WHERE r.build_id = b.id) \
         ORDER BY b.created_at DESC, b.build_number DESC LIMIT 1",
    )
    .bind(project_id)
    .bind(version_name)
    .fetch_optional(pool)
    .await?;
    if released.is_some() {
        return Ok(released);
    }
    sqlx::query_as(
        "SELECT b.version_name, b.commit_sha FROM builds b \
         WHERE b.project_id = ?1 AND b.version_name IS NOT NULL AND b.version_name != ?2 \
           AND b.commit_sha IS NOT NULL AND b.status = 'succeeded' \
         ORDER BY b.created_at DESC, b.build_number DESC LIMIT 1",
    )
    .bind(project_id)
    .bind(version_name)
    .fetch_optional(pool)
    .await
}

/// Notes drafted from the commits between the last released version and
/// `target_commit`. Empty text when there is nothing to compare with.
async fn draft(
    pool: &SqlitePool,
    encryption_key: &[u8],
    project_id: &str,
    repository_id: Option<&str>,
    version_name: &str,
    target_commit: Option<&str>,
) -> Result<ReleaseNotesDraftResponse, ApiFailure> {
    let base = draft_base(pool, project_id, version_name)
        .await
        .map_err(store_err("Failed to find the last released version"))?;
    let (Some((since_version, base_commit)), Some(repository_id), Some(target)) =
        (base, repository_id, target_commit)
    else {
        return Ok(ReleaseNotesDraftResponse {
            text: String::new(),
            since_version: None,
        });
    };
    if base_commit == target {
        return Ok(ReleaseNotesDraftResponse {
            text: String::new(),
            since_version: Some(since_version),
        });
    }
    let commits = crate::integrations::compare_commits(
        pool,
        encryption_key,
        repository_id,
        &base_commit,
        target,
    )
    .await?;
    Ok(ReleaseNotesDraftResponse {
        text: notes_from_commits(&commits),
        since_version: Some(since_version),
    })
}

// ── After the runner reports a build's identity ─────────────────

/// What the runner reported that the notes need.
pub(crate) struct ReportedIdentity {
    pub build_id: String,
    pub release_notes: Option<RepositoryReleaseNotes>,
}

/// Runs after a runner reports a build's identity: records notes the
/// repository holds, makes or refreshes the version's notes, and fills in
/// the build's Changes. Best effort: a failure only leaves notes as they
/// were.
pub(crate) async fn after_identity(state: Arc<AppState>, reported: ReportedIdentity) {
    if let Err(error) = refresh(&state, &reported).await {
        warn!(build_id = %reported.build_id, error = %error.1.0.error, "failed to refresh release notes");
    }
    if let Err(error) = fill_build_changes(&state, &reported.build_id).await {
        warn!(build_id = %reported.build_id, error = %error.1.0.error, "failed to fill in the build's changes");
    }
}

struct BuildFacts {
    project_id: String,
    pipeline_id: String,
    build_number: i64,
    branch: Option<String>,
    commit_sha: Option<String>,
    version_name: Option<String>,
    changelog: Option<String>,
    store_notes: Option<String>,
    triggered_by: Option<String>,
    repository_id: Option<String>,
}

async fn build_facts(pool: &SqlitePool, build_id: &str) -> Result<Option<BuildFacts>, ApiFailure> {
    let row = sqlx::query(
        "SELECT b.project_id, b.pipeline_id, b.build_number, b.branch, b.commit_sha, \
                b.version_name, b.changelog, b.store_notes, b.triggered_by, p.repository_id \
         FROM builds b JOIN projects p ON p.id = b.project_id WHERE b.id = ?1",
    )
    .bind(build_id)
    .fetch_optional(pool)
    .await
    .map_err(store_err("Failed to load the build"))?;
    Ok(row.map(|row| BuildFacts {
        project_id: row.get("project_id"),
        pipeline_id: row.get("pipeline_id"),
        build_number: row.get("build_number"),
        branch: row.get("branch"),
        commit_sha: row.get("commit_sha"),
        version_name: row
            .get::<Option<String>, _>("version_name")
            .filter(|v| !v.trim().is_empty()),
        changelog: row.get("changelog"),
        store_notes: row
            .get::<Option<String>, _>("store_notes")
            .filter(|notes| !notes.trim().is_empty()),
        triggered_by: row.get("triggered_by"),
        repository_id: row.get("repository_id"),
    }))
}

async fn refresh(state: &AppState, reported: &ReportedIdentity) -> Result<(), ApiFailure> {
    let pool = &state.db;
    let Some(build) = build_facts(pool, &reported.build_id).await? else {
        return Ok(());
    };
    let Some(version) = build.version_name.as_deref() else {
        return Ok(());
    };
    let saved = |what| store_err(what);

    // The repository is the source of truth whenever it holds the notes.
    if let Some(notes) = &reported.release_notes {
        upsert(
            pool,
            Upsert {
                project_id: &build.project_id,
                version_name: version,
                text: &notes.text,
                locales: &notes.locales,
                source: ReleaseNotesSource::Repository,
                source_path: Some(&notes.path),
                drafted_since: None,
                updated_by: None,
            },
            &[
                ReleaseNotesSource::Commits,
                ReleaseNotesSource::Manual,
                ReleaseNotesSource::Repository,
            ],
        )
        .await
        .map_err(saved("Failed to save the release notes"))?;
        return Ok(());
    }

    // Notes written when the build was started become the version's notes.
    if let Some(text) = build.store_notes.as_deref() {
        upsert(
            pool,
            Upsert {
                project_id: &build.project_id,
                version_name: version,
                text,
                locales: &[],
                source: ReleaseNotesSource::Manual,
                source_path: None,
                drafted_since: None,
                updated_by: build.triggered_by.as_deref(),
            },
            &[ReleaseNotesSource::Commits, ReleaseNotesSource::Manual],
        )
        .await
        .map_err(saved("Failed to save the release notes"))?;
        return Ok(());
    }

    let existing = load(pool, &build.project_id, version)
        .await
        .map_err(saved("Failed to load the release notes"))?;
    if existing
        .as_ref()
        .is_some_and(|notes| notes.source != ReleaseNotesSource::Commits)
    {
        return Ok(());
    }

    // A project's first notes start from the notes its pipeline used to
    // send to TestFlight, which the Publishing tab no longer edits.
    if existing.is_none() && first_notes_of_project(pool, &build.project_id).await? {
        let saved_notes = crate::app_store_publish::load_config(pool, &build.pipeline_id)
            .await
            .map_err(saved("Failed to load the App Store settings"))?
            .map(|(config, _)| config.release_notes)
            .unwrap_or_default();
        if let Some((text, locales)) = split_saved_notes(&saved_notes) {
            upsert(
                pool,
                Upsert {
                    project_id: &build.project_id,
                    version_name: version,
                    text: &text,
                    locales: &locales,
                    source: ReleaseNotesSource::Manual,
                    source_path: None,
                    drafted_since: None,
                    updated_by: None,
                },
                &[ReleaseNotesSource::Commits],
            )
            .await
            .map_err(saved("Failed to save the release notes"))?;
            return Ok(());
        }
    }

    let drafted = draft(
        pool,
        &state.encryption_key,
        &build.project_id,
        build.repository_id.as_deref(),
        version,
        build.commit_sha.as_deref(),
    )
    .await?;
    upsert(
        pool,
        Upsert {
            project_id: &build.project_id,
            version_name: version,
            text: &drafted.text,
            locales: &[],
            source: ReleaseNotesSource::Commits,
            source_path: None,
            drafted_since: drafted.since_version.as_deref(),
            updated_by: None,
        },
        &[ReleaseNotesSource::Commits],
    )
    .await
    .map_err(saved("Failed to save the release notes"))?;
    Ok(())
}

async fn first_notes_of_project(pool: &SqlitePool, project_id: &str) -> Result<bool, ApiFailure> {
    let any: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM version_release_notes WHERE project_id = ?1)",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .map_err(store_err("Failed to load the release notes"))?;
    Ok(!any)
}

/// A pipeline's saved App Store notes as a default text (en-US, else the
/// first language) and the other languages.
fn split_saved_notes(saved: &[AppStoreReleaseNote]) -> Option<(String, Vec<ReleaseNotesLocale>)> {
    let saved: Vec<&AppStoreReleaseNote> = saved
        .iter()
        .filter(|note| !note.whats_new.trim().is_empty())
        .collect();
    let default = saved
        .iter()
        .position(|note| note.locale == DEFAULT_LOCALE)
        .or((!saved.is_empty()).then_some(0))?;
    let text = saved[default].whats_new.trim().to_string();
    let locales = saved
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != default)
        .map(|(_, note)| ReleaseNotesLocale {
            locale: note.locale.clone(),
            text: note.whats_new.trim().to_string(),
        })
        .collect();
    Some((text, locales))
}

/// A build's Changes: the commits since the previous succeeded build of the
/// same pipeline and branch. Builds started from the web already have them.
async fn fill_build_changes(state: &AppState, build_id: &str) -> Result<(), ApiFailure> {
    let pool = &state.db;
    let Some(build) = build_facts(pool, build_id).await? else {
        return Ok(());
    };
    if build.changelog.is_some() {
        return Ok(());
    }
    let (Some(repository_id), Some(commit)) = (&build.repository_id, &build.commit_sha) else {
        return Ok(());
    };
    let base: Option<String> = sqlx::query_scalar(
        "SELECT commit_sha FROM builds \
         WHERE project_id = ?1 AND pipeline_id = ?2 AND build_number < ?3 \
           AND status = 'succeeded' AND commit_sha IS NOT NULL \
           AND (branch = ?4 OR ?4 IS NULL) \
         ORDER BY build_number DESC LIMIT 1",
    )
    .bind(&build.project_id)
    .bind(&build.pipeline_id)
    .bind(build.build_number)
    .bind(&build.branch)
    .fetch_optional(pool)
    .await
    .map_err(store_err("Failed to find the previous build"))?;
    let Some(base) = base.filter(|base| base != commit) else {
        return Ok(());
    };
    let commits = crate::integrations::compare_commits(
        pool,
        &state.encryption_key,
        repository_id,
        &base,
        commit,
    )
    .await?;
    let markdown = crate::builds::changelog_markdown(commits);
    if markdown.is_empty() {
        return Ok(());
    }
    sqlx::query("UPDATE builds SET changelog = ?1 WHERE id = ?2 AND changelog IS NULL")
        .bind(markdown)
        .bind(build_id)
        .execute(pool)
        .await
        .map_err(store_err("Failed to save the build's changes"))?;
    Ok(())
}

// ── Handlers ────────────────────────────────────────────────────

async fn require(
    state: &AppState,
    auth: &AuthUser,
    project_id: &str,
    permission: ProjectPermission,
) -> Result<(), ApiFailure> {
    let role = project_role(state, auth, project_id).await?;
    require_project_permission(&role, ProjectPermission::Read)?;
    require_project_permission(&role, permission)
}

fn not_found(version_name: &str) -> ApiFailure {
    api_err(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("Version {version_name} has no release notes yet"),
    )
}

/// `GET /v1/projects/{project_id}/release-notes/{version_name}`
pub async fn get_release_notes(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((project_id, version_name)): Path<(String, String)>,
) -> ApiResult<VersionReleaseNotes> {
    require(&state, &auth, &project_id, ProjectPermission::Read).await?;
    load(&state.db, &project_id, &version_name)
        .await
        .map_err(store_err("Failed to load the release notes"))?
        .map(Json)
        .ok_or_else(|| not_found(&version_name))
}

fn validate(request: &UpdateVersionReleaseNotesRequest) -> Result<(), ApiFailure> {
    let invalid = |message: String| api_err(StatusCode::BAD_REQUEST, "invalid_input", message);
    if request.text.trim().chars().count() > MAX_NOTES_CHARS {
        return Err(invalid(format!(
            "Release notes must be {MAX_NOTES_CHARS} characters or fewer"
        )));
    }
    if request.locales.len() > MAX_LOCALES {
        return Err(invalid(format!("At most {MAX_LOCALES} languages")));
    }
    let mut seen = Vec::new();
    for locale in &request.locales {
        let code = locale.locale.trim();
        let valid = !code.is_empty()
            && code.len() <= 16
            && code
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid {
            return Err(invalid(format!(
                "{code:?} is not a language code such as de-DE"
            )));
        }
        if code == DEFAULT_LOCALE || seen.contains(&code) {
            return Err(invalid(format!("{code} is listed twice")));
        }
        if locale.text.trim().chars().count() > MAX_NOTES_CHARS {
            return Err(invalid(format!(
                "Release notes for {code} must be {MAX_NOTES_CHARS} characters or fewer"
            )));
        }
        seen.push(code);
    }
    Ok(())
}

/// `PUT /v1/projects/{project_id}/release-notes/{version_name}`
pub async fn update_release_notes(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((project_id, version_name)): Path<(String, String)>,
    Json(request): Json<UpdateVersionReleaseNotesRequest>,
) -> ApiResult<VersionReleaseNotes> {
    require(
        &state,
        &auth,
        &project_id,
        ProjectPermission::WriteArtifacts,
    )
    .await?;
    validate(&request)?;
    let version_name = version_name.trim().to_string();
    let known: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM builds WHERE project_id = ?1 AND version_name = ?2) \
             OR EXISTS (SELECT 1 FROM version_release_notes WHERE project_id = ?1 AND version_name = ?2)",
    )
    .bind(&project_id)
    .bind(&version_name)
    .fetch_one(&state.db)
    .await
    .map_err(store_err("Failed to load the release notes"))?;
    if !known {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("No build of this app is version {version_name}"),
        ));
    }
    if let Some(saved) = load(&state.db, &project_id, &version_name)
        .await
        .map_err(store_err("Failed to load the release notes"))?
        && saved.source == ReleaseNotesSource::Repository
    {
        return Err(api_err(
            StatusCode::CONFLICT,
            "notes_in_repository",
            format!(
                "These notes come from {} in the repository. Change them there and build again.",
                saved.source_path.as_deref().unwrap_or("the repository")
            ),
        ));
    }
    let locales: Vec<ReleaseNotesLocale> = request
        .locales
        .iter()
        .filter(|locale| !locale.text.trim().is_empty())
        .map(|locale| ReleaseNotesLocale {
            locale: locale.locale.trim().to_string(),
            text: locale.text.trim().to_string(),
        })
        .collect();
    upsert(
        &state.db,
        Upsert {
            project_id: &project_id,
            version_name: &version_name,
            text: request.text.trim(),
            locales: &locales,
            source: ReleaseNotesSource::Manual,
            source_path: None,
            drafted_since: None,
            updated_by: Some(&auth.0.user_id),
        },
        &[ReleaseNotesSource::Commits, ReleaseNotesSource::Manual],
    )
    .await
    .map_err(store_err("Failed to save the release notes"))?;
    let _ = write_audit_log(
        &state.db,
        Some(&auth.0.user_id),
        "release_notes_updated",
        "project",
        Some(&project_id),
        Some(&serde_json::json!({ "version_name": version_name }).to_string()),
    )
    .await;
    load(&state.db, &project_id, &version_name)
        .await
        .map_err(store_err("Failed to load the release notes"))?
        .map(Json)
        .ok_or_else(|| not_found(&version_name))
}

/// `GET /v1/projects/{project_id}/release-notes/{version_name}/draft` —
/// notes drafted from the commits since the last released version, up to
/// the version's newest build.
pub async fn draft_release_notes(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((project_id, version_name)): Path<(String, String)>,
) -> ApiResult<ReleaseNotesDraftResponse> {
    require(&state, &auth, &project_id, ProjectPermission::Read).await?;
    let newest: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT b.commit_sha, p.repository_id FROM builds b JOIN projects p ON p.id = b.project_id \
         WHERE b.project_id = ?1 AND b.version_name = ?2 AND b.commit_sha IS NOT NULL \
         ORDER BY b.build_number DESC LIMIT 1",
    )
    .bind(&project_id)
    .bind(&version_name)
    .fetch_optional(&state.db)
    .await
    .map_err(store_err("Failed to load the version's builds"))?;
    let Some((commit, repository_id)) = newest else {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("No build of this app is version {version_name}"),
        ));
    };
    draft(
        &state.db,
        &state.encryption_key,
        &project_id,
        repository_id.as_deref(),
        &version_name,
        commit.as_deref(),
    )
    .await
    .map(Json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(title: &str) -> CommitSummary {
        CommitSummary {
            title: title.to_string(),
            author: "Mina".to_string(),
        }
    }

    #[test]
    fn drafts_titles_without_authors_or_merges() {
        let text = notes_from_commits(&[
            commit("Fix login on slow networks"),
            commit("Merge branch 'main' into release"),
            commit("Fix  login on slow networks"),
            commit("feat(settings): add dark mode"),
            commit("Fix: Login on tablets"),
        ]);
        assert_eq!(
            text,
            "- Fix login on slow networks\n- Add dark mode\n- Fix: Login on tablets"
        );
        assert!(!text.contains("Mina"));
    }

    #[test]
    fn long_drafts_stop_within_the_store_limit() {
        let commits: Vec<CommitSummary> = (0..400)
            .map(|index| commit(&format!("Change number {index} with a fairly long title")))
            .collect();
        let text = notes_from_commits(&commits);
        assert!(text.chars().count() <= MAX_NOTES_CHARS, "{}", text.len());
        assert!(text.ends_with("more changes"), "{text}");
    }

    #[test]
    fn app_store_notes_put_the_default_text_first_as_en_us() {
        let notes = VersionReleaseNotes {
            project_id: "p".into(),
            version_name: "2.5.1".into(),
            text: "Faster sync".into(),
            locales: vec![
                ReleaseNotesLocale {
                    locale: "de-DE".into(),
                    text: "Schneller".into(),
                },
                ReleaseNotesLocale {
                    locale: "fr-FR".into(),
                    text: " ".into(),
                },
            ],
            source: ReleaseNotesSource::Manual,
            source_path: None,
            drafted_since: None,
            updated_at: 0,
            updated_by: None,
        };
        let out = app_store_notes(&notes);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].locale, "en-US");
        assert_eq!(out[0].whats_new, "Faster sync");
        assert_eq!(out[1].locale, "de-DE");
    }

    #[test]
    fn saved_pipeline_notes_split_into_default_and_languages() {
        let saved = vec![
            AppStoreReleaseNote {
                locale: "de-DE".into(),
                whats_new: "Neu".into(),
            },
            AppStoreReleaseNote {
                locale: "en-US".into(),
                whats_new: "New".into(),
            },
        ];
        let (text, locales) = split_saved_notes(&saved).unwrap();
        assert_eq!(text, "New");
        assert_eq!(locales.len(), 1);
        assert_eq!(locales[0].locale, "de-DE");
        assert!(split_saved_notes(&[]).is_none());
    }

    #[test]
    fn updates_validate_languages() {
        let mut request = UpdateVersionReleaseNotesRequest {
            text: "Fixes".into(),
            locales: vec![ReleaseNotesLocale {
                locale: "de-DE".into(),
                text: "Korrekturen".into(),
            }],
        };
        assert!(validate(&request).is_ok());
        request.locales.push(ReleaseNotesLocale {
            locale: "en-US".into(),
            text: "dup".into(),
        });
        assert!(validate(&request).is_err());
        request.locales.pop();
        request.text = "x".repeat(MAX_NOTES_CHARS + 1);
        assert!(validate(&request).is_err());
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
             VALUES ('u1', 'owner@example.com', 'sub', 'owner', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p1', 'App', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'main', 0, 0)",
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             version_name, queued_at, created_at, updated_at) \
             VALUES ('b1', 'p1', 'pl1', 1, 'succeeded', 'manual', '2.5.1', 0, 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    fn notes(text: &str, source: ReleaseNotesSource) -> Upsert<'_> {
        Upsert {
            project_id: "p1",
            version_name: "2.5.1",
            text,
            locales: &[],
            source,
            source_path: None,
            drafted_since: None,
            updated_by: None,
        }
    }

    #[tokio::test]
    async fn a_draft_never_replaces_written_notes() {
        let pool = pool().await;
        let any = [
            ReleaseNotesSource::Commits,
            ReleaseNotesSource::Manual,
            ReleaseNotesSource::Repository,
        ];
        assert!(
            upsert(
                &pool,
                notes("- Fix login", ReleaseNotesSource::Commits),
                &[ReleaseNotesSource::Commits]
            )
            .await
            .unwrap()
        );
        assert!(
            upsert(
                &pool,
                notes("Faster login", ReleaseNotesSource::Manual),
                &[ReleaseNotesSource::Commits, ReleaseNotesSource::Manual]
            )
            .await
            .unwrap()
        );
        assert!(
            !upsert(
                &pool,
                notes("- Fix login\n- More", ReleaseNotesSource::Commits),
                &[ReleaseNotesSource::Commits]
            )
            .await
            .unwrap()
        );

        let saved = for_build(&pool, "b1").await.unwrap().unwrap();
        assert_eq!(saved.text, "Faster login");
        assert_eq!(saved.source, ReleaseNotesSource::Manual);

        // The repository always wins.
        assert!(
            upsert(
                &pool,
                notes("From the changelog", ReleaseNotesSource::Repository),
                &any
            )
            .await
            .unwrap()
        );
        let saved = load(&pool, "p1", "2.5.1").await.unwrap().unwrap();
        assert_eq!(saved.source, ReleaseNotesSource::Repository);
        assert_eq!(saved.text, "From the changelog");
        assert!(for_build(&pool, "missing").await.unwrap().is_none());
    }
}
