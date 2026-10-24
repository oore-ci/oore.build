//! Reports build progress to the source: a GitHub check run or a GitLab commit
//! status per platform, with links to the build logs and artifact installs.
//!
//! A build is due for a report while `scm_reported_status` differs from its
//! status, so every transition is picked up however it happened.

use std::time::Duration;

use chrono::{DateTime, SecondsFormat};
use oore_contract::{BuildSizeReport, BuildStatus, SizeBudget, SizeBudgetMode, StepResult};
use sqlx::{Row, SqlitePool};
use tracing::{debug, warn};
use url::Url;

use crate::artifact_sizes::{
    BudgetLimit, BudgetViolation, check_budget, format_delta, format_delta_with, format_size,
    load_size_report,
};
use crate::instance_settings::{
    DEFAULT_ALLOWED_ORIGINS, load_effective_external_access_network_settings,
};
use crate::integrations::{github, gitlab, scm_http_client};
use crate::util::now_unix;

const REPORT_INTERVAL: Duration = Duration::from_secs(2);
const REPORT_BATCH_SIZE: i64 = 20;
const MAX_REPORT_ATTEMPTS: i64 = 6;
const RETRY_BASE_SECS: i64 = 5;

/// A report the source refused for good, such as a read-only credential.
/// Retrying cannot help, so the build is marked reported at once.
#[derive(Debug)]
pub(crate) struct Refused(pub &'static str);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Refused {}

pub fn start_commit_check_reporter(pool: SqlitePool, encryption_key: Vec<u8>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(REPORT_INTERVAL).await;
            if let Err(error) =
                report_due_builds(&pool, &encryption_key, github::GITHUB_API_BASE_URL).await
            {
                warn!(%error, "commit check reporter failed");
            }
        }
    });
}

async fn report_due_builds(
    pool: &SqlitePool,
    encryption_key: &[u8],
    github_api_base_url: &str,
) -> anyhow::Result<()> {
    let now = now_unix();
    let rows = sqlx::query(
        "SELECT id, status, scm_reported_status, scm_report_attempts FROM builds \
         WHERE scm_reported_status IS NOT status \
           AND (scm_report_after IS NULL OR scm_report_after <= ?1) \
         ORDER BY created_at LIMIT ?2",
    )
    .bind(now)
    .bind(REPORT_BATCH_SIZE)
    .fetch_all(pool)
    .await?;

    for row in rows {
        let build_id: String = row.get("id");
        let status: String = row.get("status");
        let reported: Option<String> = row.get("scm_reported_status");
        let attempts: i64 = row.get("scm_report_attempts");

        let result = report_build(
            pool,
            encryption_key,
            github_api_base_url,
            &build_id,
            reported.as_deref(),
        )
        .await;
        let give_up = match &result {
            Ok(()) => true,
            Err(error) if error.downcast_ref::<Refused>().is_some() => {
                warn!(%error, build_id, "source refused the build status report");
                true
            }
            Err(error) => {
                warn!(%error, build_id, attempt = attempts + 1, "build status report failed");
                attempts + 1 >= MAX_REPORT_ATTEMPTS
            }
        };
        if give_up {
            sqlx::query(
                "UPDATE builds SET scm_reported_status = ?2, scm_report_attempts = 0, \
                 scm_report_after = NULL WHERE id = ?1",
            )
            .bind(&build_id)
            .bind(&status)
            .execute(pool)
            .await?;
        } else {
            sqlx::query(
                "UPDATE builds SET scm_report_attempts = ?2, scm_report_after = ?3 WHERE id = ?1",
            )
            .bind(&build_id)
            .bind(attempts + 1)
            .bind(now + RETRY_BASE_SECS * (1 << attempts.min(10)))
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

/// Where a check reports a build's state: queued, running or a final result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Queued,
    Running,
    Done(BuildStatus),
}

fn phase(status: BuildStatus) -> Phase {
    match status {
        BuildStatus::Queued | BuildStatus::Scheduled | BuildStatus::Assigned => Phase::Queued,
        BuildStatus::Running => Phase::Running,
        terminal => Phase::Done(terminal),
    }
}

fn status_title(build_number: i64, status: BuildStatus) -> String {
    let state = match status {
        BuildStatus::Queued | BuildStatus::Scheduled | BuildStatus::Assigned => "is queued",
        BuildStatus::Running => "is running",
        BuildStatus::Succeeded => "succeeded",
        BuildStatus::Failed => "failed",
        BuildStatus::TimedOut => "timed out",
        BuildStatus::Canceled => "was canceled",
        BuildStatus::Expired => "expired before a runner took it",
    };
    format!("Build #{build_number} {state}")
}

fn platform_label(platform: &str) -> &str {
    match platform {
        "android" => "Android",
        "ios" => "iOS",
        "macos" => "macOS",
        other => other,
    }
}

struct BuildContext {
    build_id: String,
    build_number: i64,
    status: BuildStatus,
    commit_sha: String,
    branch: Option<String>,
    pipeline_id: String,
    pipeline_name: String,
    created_at: i64,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    platforms: Vec<String>,
    /// The `.oore.yaml` v2 workflow the pipeline builds, from the snapshot.
    workflow: Option<String>,
    /// The steps the runner reported, in the order they ran.
    steps: Vec<StepResult>,
    /// The pipeline's artifact size limits.
    size_budget: SizeBudget,
    provider: String,
    integration_id: String,
    host_url: String,
    auth_mode: String,
    installation_external_id: String,
    repository_external_id: String,
    full_name: String,
}

struct ArtifactLink {
    name: String,
    platform: Option<&'static str>,
    install_url: Option<String>,
}

/// One check: a platform of the build, its links, and what it says.
struct PlatformReport {
    platform: String,
    name: String,
    title: String,
    /// The GitLab status description: the title, and any size budget
    /// overrun.
    description: String,
    summary: String,
    details_url: Option<String>,
    /// An artifact is over a size budget that fails the check.
    over_budget: bool,
}

/// Platforms the build was asked to produce, from its config snapshot.
fn snapshot_platforms(snapshot: &serde_json::Value) -> Vec<String> {
    let listed = snapshot
        .get("selected_platforms")
        .or_else(|| snapshot.pointer("/ui_execution_config/platforms"))
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if listed.is_empty() {
        vec!["android".to_string()]
    } else {
        listed
    }
}

fn artifact_platform(artifact_type: &str, name: &str) -> Option<&'static str> {
    match artifact_type {
        "apk" | "aab" => Some("android"),
        "ipa" => Some("ios"),
        _ => {
            let name = name.to_ascii_lowercase();
            if name.ends_with(".aab") || name.ends_with(".apk") {
                Some("android")
            } else if name.ends_with(".ipa") {
                Some("ios")
            } else {
                None
            }
        }
    }
}

async fn load_context(pool: &SqlitePool, build_id: &str) -> anyhow::Result<Option<BuildContext>> {
    let Some(row) = sqlx::query(
        "SELECT b.id, b.build_number, b.status, b.commit_sha, b.branch, b.pipeline_id, \
                b.config_snapshot, b.step_results, b.created_at, b.started_at, b.finished_at, pl.name AS pipeline_name, \
                pl.size_budget, \
                i.id AS integration_id, i.provider, i.host_url, i.auth_mode, \
                inst.external_id AS installation_external_id, \
                r.external_id AS repository_external_id, r.full_name \
         FROM builds b \
         JOIN pipelines pl ON pl.id = b.pipeline_id \
         JOIN projects p ON p.id = b.project_id \
         JOIN integration_repositories r ON r.id = p.repository_id \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         JOIN integrations i ON i.id = inst.integration_id \
         WHERE b.id = ?1 AND i.status = 'active' AND i.provider IN ('github', 'gitlab') \
           AND b.commit_sha IS NOT NULL AND b.commit_sha != ''",
    )
    .bind(build_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let status: String = row.get("status");
    let snapshot: String = row.get("config_snapshot");
    let snapshot = serde_json::from_str(&snapshot).unwrap_or(serde_json::Value::Null);
    let steps = row
        .get::<Option<String>, _>("step_results")
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    Ok(Some(BuildContext {
        build_id: row.get("id"),
        build_number: row.get("build_number"),
        status: status
            .parse()
            .map_err(|_| anyhow::anyhow!("build has invalid status {status}"))?,
        commit_sha: row.get("commit_sha"),
        branch: row.get("branch"),
        pipeline_id: row.get("pipeline_id"),
        pipeline_name: row.get("pipeline_name"),
        created_at: row.get("created_at"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        platforms: snapshot_platforms(&snapshot),
        workflow: snapshot
            .get("workflow")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        steps,
        size_budget: row
            .get::<Option<String>, _>("size_budget")
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default(),
        provider: row.get("provider"),
        integration_id: row.get("integration_id"),
        host_url: row.get("host_url"),
        auth_mode: row.get("auth_mode"),
        installation_external_id: row.get("installation_external_id"),
        repository_external_id: row.get("repository_external_id"),
        full_name: row.get("full_name"),
    }))
}

/// The web UI origin links point at. The local development defaults are
/// left out, since a link to them is useless to anyone reading the check.
async fn frontend_base_url(pool: &SqlitePool) -> Option<Url> {
    let settings = load_effective_external_access_network_settings(pool)
        .await
        .ok()?;
    let origin = github::preferred_frontend_origin(
        &settings.allowed_origins,
        settings.public_url.as_deref(),
    );
    if DEFAULT_ALLOWED_ORIGINS.contains(&origin.as_str()) {
        return None;
    }
    let url = Url::parse(&origin).ok()?;
    matches!(url.scheme(), "http" | "https").then_some(url)
}

fn web_url(base: &Url, path: &str, install: Option<&str>) -> String {
    let mut url = base.clone();
    url.set_path(path);
    url.set_query(None);
    url.set_fragment(None);
    if let Some(artifact_id) = install {
        url.query_pairs_mut().append_pair("install", artifact_id);
    }
    url.into()
}

async fn load_artifacts(
    pool: &SqlitePool,
    base: Option<&Url>,
    build_id: &str,
) -> anyhow::Result<Vec<ArtifactLink>> {
    let rows = sqlx::query(
        "SELECT id, name, artifact_type FROM artifacts \
         WHERE build_id = ?1 AND state = 'available' ORDER BY name",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| {
            let id: String = row.get("id");
            let name: String = row.get("name");
            let artifact_type: String = row.get("artifact_type");
            let installable = matches!(artifact_type.as_str(), "apk" | "ipa");
            ArtifactLink {
                platform: artifact_platform(&artifact_type, &name),
                install_url: base
                    .filter(|_| installable)
                    .map(|base| web_url(base, &format!("/builds/{build_id}"), Some(&id))),
                name,
            }
        })
        .collect())
}

fn markdown_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|' | '!'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// Whether an artifact belongs to a platform's check. Artifacts of no known
/// platform belong to a single-platform build's only check.
fn belongs_to(artifact_type: &str, name: &str, platform: &str, single_platform: bool) -> bool {
    match artifact_platform(artifact_type, name) {
        Some(owner) => owner == platform,
        None => single_platform,
    }
}

/// Whether a platform's check enforces the budget on an artifact. An
/// artifact of no known platform could hold back any of them, so every
/// check enforces it.
fn budget_belongs_to(artifact_type: &str, name: &str, platform: &str) -> bool {
    artifact_platform(artifact_type, name).is_none_or(|owner| owner == platform)
}

/// The size of each of a platform's artifacts and its change from the
/// baseline build, as a Markdown section.
fn size_section(
    report: &BuildSizeReport,
    base: Option<&Url>,
    platform: &str,
    single_platform: bool,
) -> Option<String> {
    let rows: Vec<_> = report
        .artifacts
        .iter()
        .filter(|artifact| {
            belongs_to(
                &artifact.artifact_type,
                &artifact.name,
                platform,
                single_platform,
            )
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    let mut section = String::from("\n**App size**");
    match (&report.baseline, &report.base_branch) {
        (Some(baseline), _) => {
            let label = format!("build #{}", baseline.build_number);
            let label = match base {
                Some(base) => format!(
                    "[{label}]({})",
                    web_url(base, &format!("/builds/{}", baseline.build_id), None)
                ),
                None => label,
            };
            section.push_str(&format!(
                " compared with {label} on `{}`\n",
                baseline.branch.replace('`', "")
            ));
        }
        (None, Some(branch)) => section.push_str(&format!(
            "\n\nNo earlier successful build on `{}` to compare with.\n",
            branch.replace('`', "")
        )),
        (None, None) => section.push('\n'),
    }
    section.push_str("\n| Artifact | Size | Change |\n| --- | ---: | ---: |\n");
    for artifact in rows {
        let change = match (artifact.delta_bytes, artifact.baseline_size_bytes) {
            (Some(delta), Some(baseline)) => format_delta(delta, baseline),
            _ if report.baseline.is_some() => "New".to_string(),
            _ => "—".to_string(),
        };
        section.push_str(&format!(
            "| {} | {} | {change} |\n",
            markdown_text(&artifact.name),
            format_size(artifact.size_bytes),
        ));
    }
    Some(section)
}

fn budget_limit_label(limit: BudgetLimit) -> String {
    match limit {
        BudgetLimit::Bytes(bytes) => format!("At most {}", format_size(bytes)),
        BudgetLimit::IncreaseBytes(bytes) => format!("Grows at most {}", format_size(bytes)),
        BudgetLimit::IncreasePercent(percent) => format!("Grows at most {percent}%"),
    }
}

/// What went over the limit: the size for a size limit, the change for an
/// increase limit.
fn budget_actual(violation: &BudgetViolation) -> String {
    match (violation.limit, violation.change) {
        (BudgetLimit::Bytes(_), _) | (_, None) => format_size(violation.size_bytes),
        // Two decimals, so a change just over a limit such as 5% does not
        // read as exactly 5.0%.
        (BudgetLimit::IncreasePercent(_), Some((delta, baseline))) => {
            format_delta_with(delta, baseline, 2)
        }
        (_, Some((delta, baseline))) => format_delta(delta, baseline),
    }
}

/// The size budget result for a platform's artifacts, as a Markdown
/// section. `None` when no rule covers them.
fn budget_section(
    report: &BuildSizeReport,
    budget: &SizeBudget,
    violations: &[&BudgetViolation],
    platform: &str,
) -> Option<String> {
    let covered: Vec<_> = report
        .artifacts
        .iter()
        .filter(|artifact| budget_belongs_to(&artifact.artifact_type, &artifact.name, platform))
        .filter_map(|artifact| {
            let kind =
                crate::artifact_sizes::artifact_kind(&artifact.name, &artifact.artifact_type);
            let rule = budget.rules.iter().find(|rule| rule.kind == kind)?;
            Some((artifact, rule))
        })
        .collect();
    if covered.is_empty() {
        return None;
    }
    // Artifacts whose increase limits could not apply: nothing in the
    // baseline build to compare them with, or no baseline build at all.
    let unmatched: Vec<String> = covered
        .iter()
        .filter(|(artifact, rule)| {
            artifact.delta_bytes.is_none()
                && (rule.max_increase_bytes.is_some() || rule.max_increase_percent.is_some())
        })
        .map(|(artifact, _)| markdown_text(&artifact.name))
        .collect();
    let skipped_note = (!unmatched.is_empty()).then(|| {
        format!(
            "\nNothing to compare {} with, so only the max size was checked.\n",
            unmatched.join(", ")
        )
    });
    if violations.is_empty() {
        let mut section = String::from("\n**Size budget**: every artifact is within the budget.\n");
        if let Some(note) = &skipped_note {
            section.push_str(note);
        }
        return Some(section);
    }
    let mut section = match budget.mode {
        SizeBudgetMode::Fail => String::from("\n**Size budget exceeded**\n"),
        SizeBudgetMode::Warn => String::from(
            "\n**Size budget warning**\n\nThe budget only warns, so the check keeps its result.\n",
        ),
    };
    if let Some(note) = &skipped_note {
        section.push_str(note);
    }
    section.push_str("\n| Artifact | Limit | Actual |\n| --- | --- | ---: |\n");
    for violation in violations {
        section.push_str(&format!(
            "| {} | {} | {} |\n",
            markdown_text(&violation.name),
            budget_limit_label(violation.limit),
            budget_actual(violation),
        ));
    }
    Some(section)
}

/// The longest description GitLab keeps for a commit status.
const MAX_GITLAB_DESCRIPTION_CHARS: usize = 255;

/// The GitLab status description: the title, and the first artifact over
/// the size budget.
fn status_description(
    title: &str,
    mode: SizeBudgetMode,
    violations: &[&BudgetViolation],
) -> String {
    let Some(first) = violations.first() else {
        return title.to_string();
    };
    let label = match mode {
        SizeBudgetMode::Fail => "over the size budget",
        SizeBudgetMode::Warn => "size budget warning",
    };
    let mut description = format!("{title} · {label}: {} {}", first.name, budget_actual(first));
    if violations.len() > 1 {
        description.push_str(&format!(" and {} more", violations.len() - 1));
    }
    if description.chars().count() > MAX_GITLAB_DESCRIPTION_CHARS {
        description = description
            .chars()
            .take(MAX_GITLAB_DESCRIPTION_CHARS - 1)
            .collect();
        description.push('…');
    }
    description
}

/// Steps the runner adds around any configuration's own steps.
const RUNNER_STEPS: &[&str] = &[
    "checkout",
    "vm-start",
    "flutter-version",
    "android-sign",
    "ios-sign",
    "artifacts",
    "publish-play",
    "publish-app-store",
];
/// At most this many steps are listed, to stay well inside the check
/// summary limit.
const MAX_LISTED_STEPS: usize = 50;
const MAX_TITLE_STEP_CHARS: usize = 60;

/// A name the v1 file gives its unnamed commands, such as `pre_build-1`.
fn is_v1_step_name(name: &str) -> bool {
    ["pre_build-", "build-", "post_build-"]
        .iter()
        .any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()))
        })
}

/// Whether the build ran a `.oore.yaml` v2 plan, whose steps have names worth
/// showing. A pipeline that picks a workflow builds v2. Otherwise the runner's
/// reports tell: v1 commands are named by stage and number, v2 steps by the
/// file. Builds that stopped in the runner's own steps keep the v1 output.
fn has_named_steps(ctx: &BuildContext) -> bool {
    ctx.workflow.is_some()
        || (!ctx.steps.iter().any(|step| is_v1_step_name(&step.name))
            && ctx
                .steps
                .iter()
                .any(|step| !RUNNER_STEPS.contains(&step.name.as_str())))
}

/// The step a failed build stopped at.
fn failed_step(steps: &[StepResult]) -> Option<&StepResult> {
    steps.iter().find(|step| step.status == "failed")
}

fn title_step_name(name: &str) -> String {
    let name = name.trim();
    if name.chars().count() <= MAX_TITLE_STEP_CHARS {
        return name.to_string();
    }
    let mut short: String = name.chars().take(MAX_TITLE_STEP_CHARS - 1).collect();
    short.push('…');
    short
}

/// The check title: the build's state, and for a v2 build that failed, the
/// step it failed at.
fn check_title(ctx: &BuildContext, named_steps: bool) -> String {
    let title = status_title(ctx.build_number, ctx.status);
    if !named_steps || !matches!(ctx.status, BuildStatus::Failed | BuildStatus::TimedOut) {
        return title;
    }
    match failed_step(&ctx.steps) {
        Some(step) => format!("{title} at {}", title_step_name(&step.name)),
        None => title,
    }
}

fn format_duration(duration_ms: i64) -> String {
    let secs = duration_ms.max(0) / 1000;
    match secs {
        0 if duration_ms < 1000 => "<1s".to_string(),
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m {:02}s", secs / 60, secs % 60),
        _ => format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60),
    }
}

fn step_result_label(status: &str) -> String {
    match status {
        "succeeded" => "Passed".to_string(),
        "failed" => "**Failed**".to_string(),
        "skipped" => "Skipped".to_string(),
        "canceled" | "cancelled" => "Canceled".to_string(),
        other => markdown_text(other),
    }
}

/// The build's steps by name, with their results, as a Markdown section.
fn steps_section(steps: &[StepResult]) -> Option<String> {
    if steps.is_empty() {
        return None;
    }
    let failed = steps.iter().position(|step| step.status == "failed");
    let mut section =
        String::from("\n**Steps**\n\n| Step | Result | Duration |\n| --- | --- | ---: |\n");
    for (index, step) in steps.iter().enumerate().take(MAX_LISTED_STEPS) {
        let name = markdown_text(step.name.trim());
        let name = if failed == Some(index) {
            format!("**{name}**")
        } else {
            name
        };
        section.push_str(&format!(
            "| {name} | {} | {} |\n",
            step_result_label(&step.status),
            format_duration(step.duration_ms),
        ));
    }
    if steps.len() > MAX_LISTED_STEPS {
        section.push_str(&format!(
            "\n{} more steps are listed with the build.\n",
            steps.len() - MAX_LISTED_STEPS
        ));
    }
    Some(section)
}

fn platform_reports(
    ctx: &BuildContext,
    base: Option<&Url>,
    artifacts: &[ArtifactLink],
    sizes: Option<&BuildSizeReport>,
) -> Vec<PlatformReport> {
    let build_url = base.map(|base| web_url(base, &format!("/builds/{}", ctx.build_id), None));
    let named_steps = has_named_steps(ctx);
    let title = check_title(ctx, named_steps);
    let single_platform = ctx.platforms.len() == 1;
    let violations = sizes
        .map(|sizes| check_budget(sizes, &ctx.size_budget))
        .unwrap_or_default();
    ctx.platforms
        .iter()
        .map(|platform| {
            let platform_violations: Vec<&BudgetViolation> = violations
                .iter()
                .filter(|violation| {
                    budget_belongs_to(&violation.artifact_type, &violation.name, platform)
                })
                .collect();
            // The budget fails the check, never the build.
            let over_budget =
                ctx.size_budget.mode == SizeBudgetMode::Fail && !platform_violations.is_empty();
            let description =
                status_description(&title, ctx.size_budget.mode, &platform_violations);
            let title = if over_budget {
                format!("{title} but is over the size budget")
            } else {
                title.clone()
            };

            let platform_artifacts: Vec<&ArtifactLink> = artifacts
                .iter()
                .filter(|artifact| match artifact.platform {
                    Some(owner) => owner == platform,
                    None => single_platform,
                })
                .collect();

            let mut summary = format!(
                "**{}** for `{}`",
                markdown_text(&title),
                ctx.commit_sha.chars().take(12).collect::<String>()
            );
            if let Some(branch) = &ctx.branch {
                summary.push_str(&format!(" on `{}`", branch.replace('`', "")));
            }
            summary.push_str(".\n");
            if let Some(workflow) = &ctx.workflow {
                summary.push_str(&format!("\nWorkflow `{}`\n", workflow.replace('`', "")));
            }
            if let Some(build_url) = &build_url {
                summary.push_str(&format!("\n[View build and logs]({build_url})\n"));
            }
            if !platform_artifacts.is_empty() {
                summary.push_str("\n**Artifacts**\n\n");
                for artifact in &platform_artifacts {
                    match &artifact.install_url {
                        Some(url) => summary.push_str(&format!(
                            "- {} · [Install on a device]({url})\n",
                            markdown_text(&artifact.name)
                        )),
                        None => summary.push_str(&format!("- {}\n", markdown_text(&artifact.name))),
                    }
                }
            }

            if let Some(section) =
                sizes.and_then(|sizes| size_section(sizes, base, platform, single_platform))
            {
                summary.push_str(&section);
            }
            match sizes {
                Some(sizes) => {
                    if let Some(section) =
                        budget_section(sizes, &ctx.size_budget, &platform_violations, platform)
                    {
                        summary.push_str(&section);
                    }
                }
                // The sizes could not be loaded. The check keeps its result,
                // but says the budget went unchecked.
                None if ctx.status == BuildStatus::Succeeded
                    && !ctx.size_budget.rules.is_empty() =>
                {
                    summary.push_str("\n**The size budget could not be checked.**\n");
                }
                None => {}
            }

            if named_steps && let Some(section) = steps_section(&ctx.steps) {
                summary.push_str(&section);
            }

            // A successful build links straight to its one install, a failed
            // or running one to its logs.
            let installs: Vec<&String> = platform_artifacts
                .iter()
                .filter_map(|artifact| artifact.install_url.as_ref())
                .collect();
            let details_url = match (ctx.status, installs.as_slice()) {
                (BuildStatus::Succeeded, [only]) => Some((*only).clone()),
                _ => build_url.clone(),
            };

            PlatformReport {
                platform: platform.clone(),
                name: format!("{} ({})", ctx.pipeline_name, platform_label(platform)),
                description,
                title,
                summary,
                details_url,
                over_budget,
            }
        })
        .collect()
}

fn rfc3339(timestamp: i64) -> Option<String> {
    DateTime::from_timestamp(timestamp, 0)
        .map(|time| time.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn github_check_run(ctx: &BuildContext, report: &PlatformReport) -> serde_json::Value {
    let mut run = serde_json::json!({
        "name": report.name,
        "head_sha": ctx.commit_sha,
        "external_id": ctx.build_id,
        "output": {
            "title": report.title,
            "summary": report.summary,
        },
    });
    if let Some(details_url) = &report.details_url {
        run["details_url"] = details_url.as_str().into();
    }
    match phase(ctx.status) {
        Phase::Queued => run["status"] = "queued".into(),
        Phase::Running => run["status"] = "in_progress".into(),
        Phase::Done(status) => {
            run["status"] = "completed".into();
            run["conclusion"] = match status {
                BuildStatus::Succeeded if report.over_budget => "failure",
                BuildStatus::Succeeded => "success",
                BuildStatus::TimedOut => "timed_out",
                BuildStatus::Failed => "failure",
                _ => "cancelled",
            }
            .into();
            if let Some(completed_at) = rfc3339(ctx.finished_at.unwrap_or_else(now_unix)) {
                run["completed_at"] = completed_at.into();
            }
        }
    }
    if let Some(started_at) = ctx.started_at.and_then(rfc3339) {
        run["started_at"] = started_at.into();
    }
    run
}

fn gitlab_state(status: BuildStatus, over_budget: bool) -> &'static str {
    match phase(status) {
        Phase::Queued => "pending",
        Phase::Running => "running",
        Phase::Done(BuildStatus::Succeeded) if over_budget => "failed",
        Phase::Done(BuildStatus::Succeeded) => "success",
        Phase::Done(BuildStatus::Failed | BuildStatus::TimedOut) => "failed",
        Phase::Done(_) => "canceled",
    }
}

async fn report_build(
    pool: &SqlitePool,
    encryption_key: &[u8],
    github_api_base_url: &str,
    build_id: &str,
    reported: Option<&str>,
) -> anyhow::Result<()> {
    let Some(ctx) = load_context(pool, build_id).await? else {
        return Ok(());
    };
    // Queued, scheduled and assigned all read as queued on the source.
    let reported_phase = reported
        .and_then(|status| status.parse::<BuildStatus>().ok())
        .map(phase);
    if reported_phase == Some(phase(ctx.status)) {
        return Ok(());
    }
    // Retention expires finished builds. The source keeps their result.
    if ctx.status == BuildStatus::Expired && matches!(reported_phase, Some(Phase::Done(_))) {
        return Ok(());
    }

    let base = frontend_base_url(pool).await;
    let artifacts = if ctx.status.is_terminal() {
        load_artifacts(pool, base.as_ref(), &ctx.build_id).await?
    } else {
        Vec::new()
    };
    // Sizes only mean something for a build that finished its work.
    // A failed size lookup must not hold back the build's result.
    let sizes = if ctx.status == BuildStatus::Succeeded {
        load_size_report(pool, &ctx.build_id)
            .await
            .unwrap_or_else(|error| {
                warn!(%error, build_id = %ctx.build_id, "failed to load artifact sizes for the check");
                None
            })
    } else {
        None
    };
    let reports = platform_reports(&ctx, base.as_ref(), &artifacts, sizes.as_ref());

    match ctx.provider.as_str() {
        "github" => {
            let client = scm_http_client().map_err(|e| anyhow::anyhow!(e))?;
            let token = github::load_installation_access_token(
                client,
                pool,
                encryption_key,
                &ctx.integration_id,
                &ctx.installation_external_id,
            )
            .await
            .map_err(|(_, axum::Json(e))| anyhow::anyhow!(e.error))?;
            report_github(pool, client, github_api_base_url, &token, &ctx, &reports).await
        }
        "gitlab" => {
            for report in &reports {
                let name = format!("Oore: {}", report.name);
                if let CheckOwner::Newer = check_owner(pool, &ctx, &report.platform).await? {
                    continue;
                }
                gitlab::set_commit_status(
                    pool,
                    encryption_key,
                    &ctx.integration_id,
                    &ctx.host_url,
                    &ctx.auth_mode,
                    &ctx.repository_external_id,
                    &gitlab::CommitStatus {
                        sha: &ctx.commit_sha,
                        state: gitlab_state(ctx.status, report.over_budget),
                        name: &name,
                        git_ref: ctx.branch.as_deref(),
                        target_url: report.details_url.as_deref(),
                        description: &report.description,
                    },
                )
                .await?;
                record_check_owner(pool, &ctx, &report.platform, &name).await?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Who reports a platform's check for a pipeline and commit. The newest
/// build owns it, so an older build finishing late does not overwrite a rerun.
enum CheckOwner {
    /// A newer build reported this check; leave it alone.
    Newer,
    /// This build may report, updating the existing check if there is one.
    This(Option<String>),
}

async fn check_owner(
    pool: &SqlitePool,
    ctx: &BuildContext,
    platform: &str,
) -> anyhow::Result<CheckOwner> {
    let Some(row) = sqlx::query(
        "SELECT c.external_id, c.build_id, owner.created_at AS owner_created_at \
         FROM scm_check_runs c LEFT JOIN builds owner ON owner.id = c.build_id \
         WHERE c.pipeline_id = ?1 AND c.commit_sha = ?2 AND c.platform = ?3",
    )
    .bind(&ctx.pipeline_id)
    .bind(&ctx.commit_sha)
    .bind(platform)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(CheckOwner::This(None));
    };
    let owner: Option<String> = row.get("build_id");
    let owner_created_at: Option<i64> = row.get("owner_created_at");
    if owner.as_deref() != Some(ctx.build_id.as_str())
        && owner_created_at.is_some_and(|created_at| created_at > ctx.created_at)
    {
        debug!(build_id = %ctx.build_id, "a newer build owns this check");
        return Ok(CheckOwner::Newer);
    }
    Ok(CheckOwner::This(Some(row.get("external_id"))))
}

async fn record_check_owner(
    pool: &SqlitePool,
    ctx: &BuildContext,
    platform: &str,
    external_id: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO scm_check_runs (pipeline_id, commit_sha, platform, external_id, build_id, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT (pipeline_id, commit_sha, platform) DO UPDATE SET \
             external_id = excluded.external_id, build_id = excluded.build_id, \
             updated_at = excluded.updated_at",
    )
    .bind(&ctx.pipeline_id)
    .bind(&ctx.commit_sha)
    .bind(platform)
    .bind(external_id)
    .bind(&ctx.build_id)
    .bind(now_unix())
    .execute(pool)
    .await?;
    Ok(())
}

/// Create or update one check run per platform the build owns.
async fn report_github(
    pool: &SqlitePool,
    client: &reqwest::Client,
    api_base_url: &str,
    token: &str,
    ctx: &BuildContext,
    reports: &[PlatformReport],
) -> anyhow::Result<()> {
    for report in reports {
        let CheckOwner::This(existing_id) = check_owner(pool, ctx, &report.platform).await? else {
            continue;
        };
        let check_run_id = github::upsert_check_run(
            client,
            api_base_url,
            token,
            &ctx.full_name,
            existing_id.as_deref(),
            &github_check_run(ctx, report),
        )
        .await?;
        record_check_owner(pool, ctx, &report.platform, &check_run_id).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    type Received = Arc<Mutex<Vec<(String, String, serde_json::Value)>>>;

    /// Answers every request with `{"id": 42}` and records method, path and body.
    async fn capture_server() -> (String, Received) {
        let received: Received = Arc::new(Mutex::new(Vec::new()));
        let app = axum::Router::new()
            .fallback(
                |axum::extract::State(received): axum::extract::State<Received>,
                 method: axum::http::Method,
                 uri: axum::http::Uri,
                 axum::Json(body): axum::Json<serde_json::Value>| async move {
                    received.lock().unwrap().push((
                        method.to_string(),
                        uri.path().to_string(),
                        body,
                    ));
                    axum::Json(serde_json::json!({ "id": 42 }))
                },
            )
            .with_state(received.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://127.0.0.1:{port}"), received)
    }

    async fn seeded_pool(provider: &str, host_url: &str) -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let auth_mode = if provider == "github" {
            "github_app"
        } else {
            "personal_token"
        };
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, created_at, updated_at) \
             VALUES ('u1', 'dev@example.com', 'sub', 0, 0)"
                .to_string(),
            format!(
                "INSERT INTO integrations (id, provider, host_url, auth_mode, created_by, created_at, updated_at) \
                 VALUES ('i1', '{provider}', '{host_url}', '{auth_mode}', 'u1', 0, 0)"
            ),
            "INSERT INTO integration_installations (id, integration_id, external_id, account_name, created_at, updated_at) \
             VALUES ('inst1', 'i1', '99', 'acme', 0, 0)"
                .to_string(),
            "INSERT INTO integration_repositories (id, installation_id, external_id, full_name, created_at, updated_at) \
             VALUES ('r1', 'inst1', '7', 'acme/app', 0, 0)"
                .to_string(),
            "INSERT INTO projects (id, name, repository_id, created_by, created_at, updated_at) \
             VALUES ('p1', 'App', 'r1', 'u1', 0, 0)"
                .to_string(),
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'Release', 0, 0)"
                .to_string(),
            "INSERT INTO external_access_network_settings (id, public_url, allowed_origins_json, created_at, updated_at) \
             VALUES (1, 'https://api.oore.example.com', '[\"http://localhost:3000\",\"https://oore.example.com\"]', 0, 0) \
             ON CONFLICT (id) DO UPDATE SET public_url = excluded.public_url, \
                 allowed_origins_json = excluded.allowed_origins_json"
                .to_string(),
        ] {
            sqlx::query(sqlx::AssertSqlSafe(statement))
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    async fn insert_build(pool: &SqlitePool, id: &str, number: i64, created_at: i64) {
        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
                 commit_sha, branch, config_snapshot, queued_at, created_at, updated_at) \
             VALUES (?1, 'p1', 'pl1', ?2, 'queued', 'webhook', 'abc123', 'main', \
                 '{\"ui_execution_config\":{\"platforms\":[\"android\",\"ios\"]}}', ?3, ?3, ?3)",
        )
        .bind(id)
        .bind(number)
        .bind(created_at)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn set_commit(pool: &SqlitePool, id: &str, sha: &str) {
        sqlx::query("UPDATE builds SET commit_sha = ?2 WHERE id = ?1")
            .bind(id)
            .bind(sha)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn set_status(pool: &SqlitePool, id: &str, status: &str) {
        sqlx::query(
            "UPDATE builds SET status = ?2, started_at = 10, finished_at = 20 WHERE id = ?1",
        )
        .bind(id)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn report_github_build(pool: &SqlitePool, base: &str, build_id: &str) {
        let ctx = load_context(pool, build_id).await.unwrap().unwrap();
        let url = frontend_base_url(pool).await;
        let artifacts = load_artifacts(pool, url.as_ref(), build_id).await.unwrap();
        let sizes = load_size_report(pool, build_id).await.unwrap();
        let reports = platform_reports(&ctx, url.as_ref(), &artifacts, sizes.as_ref());
        let client = scm_http_client().unwrap();
        report_github(pool, client, base, "token", &ctx, &reports)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn github_reports_one_check_per_platform_and_rerun_reuses_it() {
        let (base, received) = capture_server().await;
        let pool = seeded_pool("github", "https://github.com").await;
        insert_build(&pool, "b1", 1, 100).await;

        report_github_build(&pool, &base, "b1").await;
        {
            let calls = received.lock().unwrap();
            assert_eq!(calls.len(), 2);
            for (method, path, body) in calls.iter() {
                assert_eq!(method, "POST");
                assert_eq!(path, "/repos/acme/app/check-runs");
                assert_eq!(body["status"], "queued");
                assert_eq!(body["head_sha"], "abc123");
            }
            assert_eq!(calls[0].2["name"], "Release (Android)");
            assert_eq!(calls[1].2["name"], "Release (iOS)");
        }

        sqlx::query(
            "INSERT INTO artifacts (id, build_id, name, artifact_type, file_path, created_at) \
             VALUES ('a1', 'b1', 'app-release.apk', 'apk', 'k', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        set_status(&pool, "b1", "succeeded").await;
        report_github_build(&pool, &base, "b1").await;
        {
            let calls = received.lock().unwrap();
            let (method, path, android) = &calls[2];
            assert_eq!(method, "PATCH");
            assert_eq!(path, "/repos/acme/app/check-runs/42");
            assert!(android.get("head_sha").is_none());
            assert_eq!(android["conclusion"], "success");
            assert_eq!(
                android["details_url"],
                "https://oore.example.com/builds/b1?install=a1"
            );
            let summary = android["output"]["summary"].as_str().unwrap();
            assert!(summary.contains("app-release.apk"), "{summary}");
            let ios = &calls[3].2;
            assert_eq!(ios["details_url"], "https://oore.example.com/builds/b1");
            assert!(!ios["output"]["summary"].as_str().unwrap().contains("apk"));
        }

        // A rerun of the same commit takes over the existing checks.
        insert_build(&pool, "b2", 2, 200).await;
        report_github_build(&pool, &base, "b2").await;
        // The older build finishing late must not overwrite the rerun.
        report_github_build(&pool, &base, "b1").await;
        let calls = received.lock().unwrap();
        assert_eq!(calls.len(), 6);
        assert_eq!(calls[4].0, "PATCH");
        assert_eq!(calls[4].1, "/repos/acme/app/check-runs/42");
        assert_eq!(calls[4].2["status"], "queued");
        assert_eq!(calls[4].2["external_id"], "b2");
    }

    fn step(name: &str, status: &str, duration_secs: i64) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "status": status,
            "exit_code": if status == "failed" { 1 } else { 0 },
            "started_at": 10,
            "finished_at": 10 + duration_secs,
            "duration_ms": duration_secs * 1000,
        })
    }

    /// Finishes a build with the runner's step results and, for a pipeline
    /// that picks one, its v2 workflow in the snapshot.
    async fn finish_with_steps(
        pool: &SqlitePool,
        id: &str,
        status: &str,
        workflow: Option<&str>,
        steps: &[serde_json::Value],
    ) {
        let mut snapshot =
            serde_json::json!({ "ui_execution_config": { "platforms": ["android", "ios"] } });
        if let Some(workflow) = workflow {
            snapshot["workflow"] = workflow.into();
        }
        sqlx::query("UPDATE builds SET config_snapshot = ?2, step_results = ?3 WHERE id = ?1")
            .bind(id)
            .bind(snapshot.to_string())
            .bind(serde_json::Value::from(steps.to_vec()).to_string())
            .execute(pool)
            .await
            .unwrap();
        set_status(pool, id, status).await;
    }

    #[tokio::test]
    async fn github_v2_check_lists_named_steps_and_the_failed_one() {
        let (base, received) = capture_server().await;
        let pool = seeded_pool("github", "https://github.com").await;
        insert_build(&pool, "b1", 1, 100).await;
        finish_with_steps(
            &pool,
            "b1",
            "failed",
            Some("release"),
            &[
                step("checkout", "succeeded", 3),
                step("Get packages", "succeeded", 0),
                step("Run tests", "failed", 65),
            ],
        )
        .await;

        report_github_build(&pool, &base, "b1").await;
        let calls = received.lock().unwrap();
        assert_eq!(calls.len(), 2);
        let android = &calls[0].2;
        // Branch protection requires checks by name, so it never changes.
        assert_eq!(android["name"], "Release (Android)");
        assert_eq!(calls[1].2["name"], "Release (iOS)");
        assert_eq!(android["conclusion"], "failure");
        assert_eq!(android["output"]["title"], "Build #1 failed at Run tests");
        let summary = android["output"]["summary"].as_str().unwrap();
        assert!(
            summary.starts_with("**Build \\#1 failed at Run tests** for `abc123` on `main`.\n"),
            "{summary}"
        );
        assert!(summary.contains("\nWorkflow `release`\n"), "{summary}");
        assert!(
            summary.contains(
                "**Steps**\n\n| Step | Result | Duration |\n| --- | --- | ---: |\n\
                 | checkout | Passed | 3s |\n\
                 | Get packages | Passed | <1s |\n\
                 | **Run tests** | **Failed** | 1m 05s |\n"
            ),
            "{summary}"
        );
        // Every platform's check shows the step the build stopped at.
        let ios = calls[1].2["output"]["summary"].as_str().unwrap();
        assert!(ios.contains("| **Run tests** | **Failed** |"), "{ios}");
    }

    #[tokio::test]
    async fn github_v2_check_without_a_workflow_lists_steps_by_name() {
        let (base, received) = capture_server().await;
        let pool = seeded_pool("github", "https://github.com").await;
        insert_build(&pool, "b1", 1, 100).await;
        finish_with_steps(
            &pool,
            "b1",
            "succeeded",
            None,
            &[
                step("checkout", "succeeded", 2),
                step("Build Android", "succeeded", 312),
                step("Build iOS", "succeeded", 4000),
                step("Upload [symbols]", "succeeded", 5),
            ],
        )
        .await;

        report_github_build(&pool, &base, "b1").await;
        let calls = received.lock().unwrap();
        let android = &calls[0].2;
        assert_eq!(android["name"], "Release (Android)");
        assert_eq!(android["output"]["title"], "Build #1 succeeded");
        let summary = android["output"]["summary"].as_str().unwrap();
        assert!(!summary.contains("Workflow"), "{summary}");
        assert!(
            summary.contains("| Build Android | Passed | 5m 12s |\n"),
            "{summary}"
        );
        assert!(
            summary.contains("| Build iOS | Passed | 1h 06m |\n"),
            "{summary}"
        );
        assert!(
            summary.contains("| Upload \\[symbols\\] | Passed | 5s |\n"),
            "{summary}"
        );
    }

    #[tokio::test]
    async fn github_v1_check_output_is_unchanged() {
        let (base, received) = capture_server().await;
        let pool = seeded_pool("github", "https://github.com").await;
        insert_build(&pool, "b1", 1, 100).await;
        finish_with_steps(
            &pool,
            "b1",
            "failed",
            None,
            &[
                step("checkout", "succeeded", 3),
                step("pre_build-1", "succeeded", 4),
                step("build-1", "failed", 30),
            ],
        )
        .await;

        report_github_build(&pool, &base, "b1").await;
        let calls = received.lock().unwrap();
        let android = &calls[0].2;
        assert_eq!(android["name"], "Release (Android)");
        assert_eq!(android["output"]["title"], "Build #1 failed");
        assert_eq!(
            android["output"]["summary"],
            "**Build \\#1 failed** for `abc123` on `main`.\n\
             \n[View build and logs](https://oore.example.com/builds/b1)\n"
        );
    }

    #[tokio::test]
    async fn gitlab_v2_status_names_the_failed_step_and_keeps_its_name() {
        let (base, received) = capture_server().await;
        let key = vec![7u8; 32];
        let pool = gitlab_pool(&base, &key).await;
        insert_build(&pool, "b1", 1, 100).await;
        finish_with_steps(
            &pool,
            "b1",
            "failed",
            Some("adhoc"),
            &[
                step("checkout", "succeeded", 1),
                step("Analyze", "failed", 9),
            ],
        )
        .await;

        report_due_builds(&pool, &key, &base).await.unwrap();
        let calls = received.lock().unwrap();
        assert_eq!(calls[0].2["name"], "Oore: Release (Android)");
        assert_eq!(calls[0].2["description"], "Build #1 failed at Analyze");
        assert_eq!(calls[0].2["state"], "failed");
    }

    #[test]
    fn v1_step_names_are_told_from_v2_names() {
        for name in ["pre_build-1", "build-2", "post_build-10"] {
            assert!(is_v1_step_name(name), "{name}");
        }
        for name in [
            "build",
            "build-",
            "build-x",
            "Build Android",
            "Run tests",
            "checkout",
        ] {
            assert!(!is_v1_step_name(name), "{name}");
        }
    }

    #[test]
    fn long_step_names_are_shortened_in_the_title() {
        let name = "x".repeat(100);
        let short = title_step_name(&name);
        assert_eq!(short.chars().count(), MAX_TITLE_STEP_CHARS);
        assert!(short.ends_with('…'));
    }

    #[tokio::test]
    async fn app_bundles_link_to_android_without_an_install_link() {
        let pool = seeded_pool("github", "https://github.com").await;
        insert_build(&pool, "b1", 1, 100).await;
        sqlx::query(
            "INSERT INTO artifacts (id, build_id, name, artifact_type, file_path, created_at) \
             VALUES ('a1', 'b1', 'app-release.aab', 'aab', 'k1', 0), \
                    ('a2', 'b1', 'app-release.apk', 'apk', 'k2', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let url = frontend_base_url(&pool).await;
        let artifacts = load_artifacts(&pool, url.as_ref(), "b1").await.unwrap();
        let aab = artifacts
            .iter()
            .find(|a| a.name == "app-release.aab")
            .unwrap();
        assert_eq!(aab.platform, Some("android"));
        assert_eq!(aab.install_url, None);
        let apk = artifacts
            .iter()
            .find(|a| a.name == "app-release.apk")
            .unwrap();
        assert!(apk.install_url.is_some());
    }

    #[tokio::test]
    async fn github_summary_shows_app_size_change_from_the_base_branch() {
        let (base, received) = capture_server().await;
        let pool = seeded_pool("github", "https://github.com").await;
        sqlx::query("UPDATE projects SET default_branch = 'main'")
            .execute(&pool)
            .await
            .unwrap();
        for (build, number, bytes) in [("b1", 1, 20_000_000_i64), ("b2", 2, 21_258_291)] {
            insert_build(&pool, build, number, number * 100).await;
            set_commit(&pool, build, &format!("sha-{build}")).await;
            set_status(&pool, build, "succeeded").await;
            let artifact_id = format!("{build}-aab");
            sqlx::query(
                "INSERT INTO artifacts (id, build_id, name, artifact_type, file_path, file_size, created_at) \
                 VALUES (?1, ?2, 'app-release.aab', 'generic', 'k', ?3, 0)",
            )
            .bind(&artifact_id)
            .bind(build)
            .bind(bytes)
            .execute(&pool)
            .await
            .unwrap();
            crate::artifact_sizes::record_artifact_size(&pool, &artifact_id).await;
        }

        report_github_build(&pool, &base, "b2").await;
        let calls = received.lock().unwrap();
        let android = calls[0].2["output"]["summary"].as_str().unwrap();
        assert!(
            android.contains(
                "**App size** compared with [build #1](https://oore.example.com/builds/b1) on `main`"
            ),
            "{android}"
        );
        assert!(
            android.contains("| app-release.aab | 20.3 MB | +1.2 MB (+6.3%) |"),
            "{android}"
        );
        // An app bundle belongs to Android only.
        let ios = calls[1].2["output"]["summary"].as_str().unwrap();
        assert!(!ios.contains("App size"), "{ios}");
    }

    async fn gitlab_pool(base: &str, key: &[u8]) -> SqlitePool {
        let pool = seeded_pool("gitlab", base).await;
        sqlx::query(
            "INSERT INTO integration_credentials (id, integration_id, credential_type, encrypted_value, created_at, updated_at) \
             VALUES ('c1', 'i1', 'access_token', ?1, 0, 0)",
        )
        .bind(crate::crypto::encrypt("glpat-test", key).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn gitlab_states(received: &Received) -> Vec<(String, String)> {
        received
            .lock()
            .unwrap()
            .iter()
            .map(|(_, _, body)| {
                (
                    body["description"].as_str().unwrap().to_string(),
                    body["state"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn gitlab_statuses_follow_build_transitions() {
        let (base, received) = capture_server().await;
        let key = vec![7u8; 32];
        let pool = gitlab_pool(&base, &key).await;
        insert_build(&pool, "b1", 1, 100).await;

        report_due_builds(&pool, &key, &base).await.unwrap();
        // Scheduled still reads as pending, so nothing new is posted.
        set_status(&pool, "b1", "scheduled").await;
        report_due_builds(&pool, &key, &base).await.unwrap();
        set_status(&pool, "b1", "failed").await;
        report_due_builds(&pool, &key, &base).await.unwrap();

        let calls = received.lock().unwrap().clone();
        let states: Vec<(&str, &str)> = calls
            .iter()
            .map(|(_, _, body)| {
                (
                    body["name"].as_str().unwrap(),
                    body["state"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            states,
            [
                ("Oore: Release (Android)", "pending"),
                ("Oore: Release (iOS)", "pending"),
                ("Oore: Release (Android)", "failed"),
                ("Oore: Release (iOS)", "failed"),
            ]
        );
        assert_eq!(calls[0].1, "/api/v4/projects/7/statuses/abc123");
        assert_eq!(calls[0].2["ref"], "main");
        assert_eq!(
            calls[2].2["target_url"],
            "https://oore.example.com/builds/b1"
        );

        let reported: Option<String> =
            sqlx::query_scalar("SELECT scm_reported_status FROM builds WHERE id = 'b1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(reported.as_deref(), Some("failed"));
    }

    #[tokio::test]
    async fn a_failed_size_lookup_still_reports_the_result() {
        let (base, received) = capture_server().await;
        let key = vec![7u8; 32];
        let pool = gitlab_pool(&base, &key).await;
        insert_build(&pool, "b1", 1, 100).await;
        set_status(&pool, "b1", "succeeded").await;
        sqlx::query("DROP TABLE build_artifact_sizes")
            .execute(&pool)
            .await
            .unwrap();

        report_due_builds(&pool, &key, &base).await.unwrap();

        let states = gitlab_states(&received);
        let succeeded = ("Build #1 succeeded".to_string(), "success".to_string());
        assert_eq!(states, [succeeded.clone(), succeeded]);
    }

    #[tokio::test]
    async fn retention_expiring_a_reported_build_leaves_its_result_alone() {
        let (base, received) = capture_server().await;
        let key = vec![7u8; 32];
        let pool = gitlab_pool(&base, &key).await;
        insert_build(&pool, "b1", 1, 100).await;
        set_status(&pool, "b1", "succeeded").await;
        report_due_builds(&pool, &key, &base).await.unwrap();
        assert_eq!(received.lock().unwrap().len(), 2);

        set_status(&pool, "b1", "expired").await;
        report_due_builds(&pool, &key, &base).await.unwrap();

        assert_eq!(received.lock().unwrap().len(), 2);
        let reported: Option<String> =
            sqlx::query_scalar("SELECT scm_reported_status FROM builds WHERE id = 'b1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(reported.as_deref(), Some("expired"));
    }

    #[tokio::test]
    async fn gitlab_older_build_finishing_late_does_not_overwrite_a_rerun() {
        let (base, received) = capture_server().await;
        let key = vec![7u8; 32];
        let pool = gitlab_pool(&base, &key).await;
        insert_build(&pool, "b1", 1, 100).await;
        report_due_builds(&pool, &key, &base).await.unwrap();
        insert_build(&pool, "b2", 2, 200).await;
        set_status(&pool, "b2", "succeeded").await;
        report_due_builds(&pool, &key, &base).await.unwrap();
        set_status(&pool, "b1", "failed").await;
        report_due_builds(&pool, &key, &base).await.unwrap();

        let succeeded = ("Build #2 succeeded".to_string(), "success".to_string());
        let states = gitlab_states(&received);
        assert_eq!(states.len(), 4, "{states:?}");
        assert_eq!(states[2..], [succeeded.clone(), succeeded]);
        let reported: Option<String> =
            sqlx::query_scalar("SELECT scm_reported_status FROM builds WHERE id = 'b1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(reported.as_deref(), Some("failed"));
    }

    const MB: i64 = 1024 * 1024;

    /// An artifact's name, type and size in bytes.
    type SizedArtifact<'a> = (&'a str, &'a str, i64);

    /// Finished, successful builds on `main` with their artifacts' sizes, as
    /// `(build, number, artifacts)`.
    async fn sized_builds(pool: &SqlitePool, builds: &[(&str, i64, &[SizedArtifact<'_>])]) {
        sqlx::query("UPDATE projects SET default_branch = 'main'")
            .execute(pool)
            .await
            .unwrap();
        for (build, number, artifacts) in builds {
            insert_build(pool, build, *number, number * 100).await;
            set_commit(pool, build, &format!("sha-{build}")).await;
            set_status(pool, build, "succeeded").await;
            for (name, artifact_type, bytes) in *artifacts {
                let artifact_id = format!("{build}-{name}");
                sqlx::query(
                    "INSERT INTO artifacts (id, build_id, name, artifact_type, file_path, file_size, created_at) \
                     VALUES (?1, ?2, ?3, ?4, 'k', ?5, 0)",
                )
                .bind(&artifact_id)
                .bind(build)
                .bind(name)
                .bind(artifact_type)
                .bind(bytes)
                .execute(pool)
                .await
                .unwrap();
                crate::artifact_sizes::record_artifact_size(pool, &artifact_id).await;
            }
        }
    }

    async fn set_budget(pool: &SqlitePool, budget: serde_json::Value) {
        sqlx::query("UPDATE pipelines SET size_budget = ?1 WHERE id = 'pl1'")
            .bind(budget.to_string())
            .execute(pool)
            .await
            .unwrap();
    }

    /// An app bundle that grew by 1.2 MB (+6.3%) from build #1 to #2.
    async fn grown_bundle(pool: &SqlitePool) {
        sized_builds(
            pool,
            &[
                ("b1", 1, &[("app-release.aab", "aab", 20_000_000)]),
                ("b2", 2, &[("app-release.aab", "aab", 21_258_291)]),
            ],
        )
        .await;
    }

    /// The Android and iOS check runs reported for build #2.
    async fn report_checks(pool: &SqlitePool) -> (serde_json::Value, serde_json::Value) {
        let (base, received) = capture_server().await;
        report_github_build(pool, &base, "b2").await;
        let calls = received.lock().unwrap();
        assert_eq!(calls.len(), 2);
        (calls[0].2.clone(), calls[1].2.clone())
    }

    fn summary(check: &serde_json::Value) -> &str {
        check["output"]["summary"].as_str().unwrap()
    }

    #[tokio::test]
    async fn a_build_within_its_size_budget_passes_the_check() {
        let pool = seeded_pool("github", "https://github.com").await;
        grown_bundle(&pool).await;
        set_budget(
            &pool,
            serde_json::json!({ "mode": "fail", "rules": [
                { "kind": "aab", "max_bytes": 30 * MB, "max_increase_bytes": 2 * MB, "max_increase_percent": 10 }
            ] }),
        )
        .await;

        let (android, ios) = report_checks(&pool).await;
        assert_eq!(android["conclusion"], "success");
        assert_eq!(android["output"]["title"], "Build #2 succeeded");
        assert!(
            summary(&android).contains("**Size budget**: every artifact is within the budget."),
            "{}",
            summary(&android)
        );
        // No rule covers the iOS check's artifacts.
        assert!(!summary(&ios).contains("Size budget"), "{}", summary(&ios));
    }

    #[tokio::test]
    async fn a_failing_size_budget_fails_the_check_with_the_delta() {
        let pool = seeded_pool("github", "https://github.com").await;
        grown_bundle(&pool).await;
        set_budget(
            &pool,
            serde_json::json!({ "mode": "fail", "rules": [
                { "kind": "aab", "max_bytes": 30 * MB, "max_increase_bytes": MB }
            ] }),
        )
        .await;

        let (android, ios) = report_checks(&pool).await;
        // Branch protection requires checks by name, so it never changes.
        assert_eq!(android["name"], "Release (Android)");
        assert_eq!(android["status"], "completed");
        assert_eq!(android["conclusion"], "failure");
        assert_eq!(
            android["output"]["title"],
            "Build #2 succeeded but is over the size budget"
        );
        let text = summary(&android);
        assert!(text.contains("**Size budget exceeded**"), "{text}");
        assert!(
            text.contains("| app-release.aab | Grows at most 1.0 MB | +1.2 MB (+6.3%) |"),
            "{text}"
        );
        // The size limit holds, so only the increase is listed.
        assert!(!text.contains("At most"), "{text}");
        // The app bundle is Android's, so the iOS check still passes.
        assert_eq!(ios["name"], "Release (iOS)");
        assert_eq!(ios["conclusion"], "success");
        // The build itself still succeeded.
        let status: String = sqlx::query_scalar("SELECT status FROM builds WHERE id = 'b2'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "succeeded");
    }

    #[tokio::test]
    async fn a_warning_size_budget_keeps_the_check_result() {
        let pool = seeded_pool("github", "https://github.com").await;
        grown_bundle(&pool).await;
        set_budget(
            &pool,
            serde_json::json!({ "mode": "warn", "rules": [
                { "kind": "aab", "max_increase_percent": 5 }
            ] }),
        )
        .await;

        let (android, _) = report_checks(&pool).await;
        assert_eq!(android["conclusion"], "success");
        assert_eq!(android["output"]["title"], "Build #2 succeeded");
        let text = summary(&android);
        assert!(text.contains("**Size budget warning**"), "{text}");
        assert!(
            text.contains("| app-release.aab | Grows at most 5% | +1.2 MB (+6.29%) |"),
            "{text}"
        );
    }

    #[tokio::test]
    async fn without_a_baseline_only_maximum_sizes_apply() {
        let pool = seeded_pool("github", "https://github.com").await;
        sized_builds(&pool, &[("b2", 2, &[("app-release.aab", "aab", 25 * MB)])]).await;

        set_budget(
            &pool,
            serde_json::json!({ "mode": "fail", "rules": [
                { "kind": "aab", "max_increase_bytes": 0, "max_increase_percent": 0 }
            ] }),
        )
        .await;
        let (android, _) = report_checks(&pool).await;
        assert_eq!(android["conclusion"], "success");
        assert!(
            summary(&android).contains(
                "Nothing to compare app-release.aab with, so only the max size was checked."
            ),
            "{}",
            summary(&android)
        );

        set_budget(
            &pool,
            serde_json::json!({ "mode": "fail", "rules": [
                { "kind": "aab", "max_bytes": 20 * MB, "max_increase_bytes": 0 }
            ] }),
        )
        .await;
        let (android, _) = report_checks(&pool).await;
        assert_eq!(android["conclusion"], "failure");
        let text = summary(&android);
        assert!(
            text.contains("| app-release.aab | At most 20.0 MB | 25.0 MB |"),
            "{text}"
        );
        assert!(!text.contains("Grows at most"), "{text}");
    }

    #[tokio::test]
    async fn each_artifact_is_held_to_the_rule_for_its_kind() {
        let pool = seeded_pool("github", "https://github.com").await;
        sized_builds(
            &pool,
            &[
                (
                    "b1",
                    1,
                    &[
                        ("app-release.aab", "aab", 20 * MB),
                        ("app-release.apk", "apk", 30 * MB),
                        ("Runner.ipa", "ipa", 40 * MB),
                    ],
                ),
                (
                    "b2",
                    2,
                    &[
                        ("app-release.aab", "aab", 21 * MB),
                        ("app-release.apk", "apk", 33 * MB),
                        ("Runner.ipa", "ipa", 45 * MB),
                    ],
                ),
            ],
        )
        .await;
        set_budget(
            &pool,
            serde_json::json!({ "mode": "fail", "rules": [
                { "kind": "aab", "max_increase_percent": 10 },
                { "kind": "apk", "max_increase_percent": 5, "max_bytes": 32 * MB },
                { "kind": "ipa", "max_bytes": 44 * MB }
            ] }),
        )
        .await;

        let (android, ios) = report_checks(&pool).await;
        assert_eq!(android["conclusion"], "failure");
        let text = summary(&android);
        assert!(
            text.contains("| app-release.apk | At most 32.0 MB | 33.0 MB |"),
            "{text}"
        );
        assert!(
            text.contains("| app-release.apk | Grows at most 5% | +3.0 MB (+10.00%) |"),
            "{text}"
        );
        assert!(!text.contains("| app-release.aab | Grows"), "{text}");
        assert_eq!(ios["conclusion"], "failure");
        assert!(
            summary(&ios).contains("| Runner.ipa | At most 44.0 MB | 45.0 MB |"),
            "{}",
            summary(&ios)
        );
    }

    #[tokio::test]
    async fn gitlab_statuses_describe_the_size_budget() {
        let (base, received) = capture_server().await;
        let key = vec![7u8; 32];
        let pool = gitlab_pool(&base, &key).await;
        grown_bundle(&pool).await;
        set_budget(
            &pool,
            serde_json::json!({ "mode": "fail", "rules": [{ "kind": "aab", "max_increase_bytes": MB }] }),
        )
        .await;
        sqlx::query("UPDATE builds SET scm_reported_status = status WHERE id = 'b1'")
            .execute(&pool)
            .await
            .unwrap();

        report_due_builds(&pool, &key, &base).await.unwrap();
        assert_eq!(
            gitlab_states(&received),
            [
                (
                    "Build #2 succeeded · over the size budget: app-release.aab +1.2 MB (+6.3%)"
                        .to_string(),
                    "failed".to_string()
                ),
                ("Build #2 succeeded".to_string(), "success".to_string()),
            ]
        );

        // A warning keeps the status and still says so.
        received.lock().unwrap().clear();
        set_budget(
            &pool,
            serde_json::json!({ "mode": "warn", "rules": [{ "kind": "aab", "max_increase_bytes": MB }] }),
        )
        .await;
        sqlx::query("UPDATE builds SET scm_reported_status = NULL WHERE id = 'b2'")
            .execute(&pool)
            .await
            .unwrap();
        report_due_builds(&pool, &key, &base).await.unwrap();
        assert_eq!(
            gitlab_states(&received)[0],
            (
                "Build #2 succeeded · size budget warning: app-release.aab +1.2 MB (+6.3%)"
                    .to_string(),
                "success".to_string()
            )
        );
    }

    #[tokio::test]
    async fn artifacts_of_no_platform_count_against_every_check() {
        let pool = seeded_pool("github", "https://github.com").await;
        sized_builds(
            &pool,
            &[(
                "b2",
                2,
                &[
                    ("symbols.zip", "generic", 50 * MB),
                    // The name tells an untyped APK's platform.
                    ("app-release.apk", "generic", 40 * MB),
                ],
            )],
        )
        .await;
        set_budget(
            &pool,
            serde_json::json!({ "mode": "fail", "rules": [
                { "kind": "zip", "max_bytes": 10 * MB },
                { "kind": "apk", "max_bytes": 10 * MB }
            ] }),
        )
        .await;

        let (android, ios) = report_checks(&pool).await;
        assert_eq!(android["conclusion"], "failure");
        assert_eq!(ios["conclusion"], "failure");
        let android = summary(&android);
        assert!(
            android.contains("| symbols.zip | At most 10.0 MB |"),
            "{android}"
        );
        assert!(
            android.contains("| app-release.apk | At most 10.0 MB |"),
            "{android}"
        );
        let ios = summary(&ios);
        assert!(ios.contains("| symbols.zip | At most 10.0 MB |"), "{ios}");
        assert!(!ios.contains("app-release.apk"), "{ios}");
    }

    #[tokio::test]
    async fn a_rerun_of_the_same_commit_is_not_its_own_baseline() {
        let pool = seeded_pool("github", "https://github.com").await;
        grown_bundle(&pool).await;
        // b3 reruns b2's commit on main.
        sized_builds(
            &pool,
            &[("b3", 3, &[("app-release.aab", "aab", 21_258_291)])],
        )
        .await;
        set_commit(&pool, "b3", "sha-b2").await;

        let report = load_size_report(&pool, "b3").await.unwrap().unwrap();
        let baseline = report.baseline.unwrap();
        assert_eq!(baseline.build_id, "b1");
        assert_eq!(report.artifacts[0].delta_bytes, Some(1_258_291));
    }

    #[tokio::test]
    async fn an_unavailable_size_report_leaves_the_check_result_and_says_so() {
        let pool = seeded_pool("github", "https://github.com").await;
        grown_bundle(&pool).await;
        set_budget(
            &pool,
            serde_json::json!({ "mode": "fail", "rules": [{ "kind": "aab", "max_increase_bytes": 0 }] }),
        )
        .await;
        sqlx::query("DROP TABLE build_artifact_sizes")
            .execute(&pool)
            .await
            .unwrap();

        let ctx = load_context(&pool, "b2").await.unwrap().unwrap();
        // The reporter passes no sizes when loading them fails.
        let reports = platform_reports(&ctx, None, &[], None);
        let android = github_check_run(&ctx, &reports[0]);
        assert_eq!(android["conclusion"], "success");
        assert_eq!(android["output"]["title"], "Build #2 succeeded");
        assert!(
            summary(&android).contains("**The size budget could not be checked.**"),
            "{}",
            summary(&android)
        );
    }

    #[tokio::test]
    async fn any_growth_from_an_empty_baseline_exceeds_a_percent_limit() {
        let pool = seeded_pool("github", "https://github.com").await;
        sized_builds(
            &pool,
            &[
                ("b1", 1, &[("app-release.aab", "aab", 0)]),
                ("b2", 2, &[("app-release.aab", "aab", 2_048)]),
            ],
        )
        .await;
        set_budget(
            &pool,
            serde_json::json!({ "mode": "fail", "rules": [{ "kind": "aab", "max_increase_percent": 1000 }] }),
        )
        .await;

        let (android, _) = report_checks(&pool).await;
        assert_eq!(android["conclusion"], "failure");
        assert!(
            summary(&android).contains("| app-release.aab | Grows at most 1000% | +2.0 KB |"),
            "{}",
            summary(&android)
        );
    }

    #[tokio::test]
    async fn only_the_split_apk_over_its_budget_is_listed() {
        let pool = seeded_pool("github", "https://github.com").await;
        sized_builds(
            &pool,
            &[
                (
                    "b1",
                    1,
                    &[
                        ("app-arm64-v8a-release.apk", "apk", 20 * MB),
                        ("app-x86_64-release.apk", "apk", 20 * MB),
                    ],
                ),
                (
                    "b2",
                    2,
                    &[
                        ("app-arm64-v8a-release.apk", "apk", 22 * MB),
                        ("app-x86_64-release.apk", "apk", 20 * MB + 512 * 1024),
                    ],
                ),
            ],
        )
        .await;
        set_budget(
            &pool,
            serde_json::json!({ "mode": "fail", "rules": [{ "kind": "apk", "max_increase_bytes": MB }] }),
        )
        .await;

        let (android, _) = report_checks(&pool).await;
        assert_eq!(android["conclusion"], "failure");
        let text = summary(&android);
        assert!(
            text.contains(
                "| app-arm64-v8a-release.apk | Grows at most 1.0 MB | +2.0 MB (+10.0%) |"
            ),
            "{text}"
        );
        assert!(!text.contains("x86\\_64-release.apk | Grows"), "{text}");
    }
}
