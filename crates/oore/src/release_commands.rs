//! `oore release ...` and `oore promote`: read store releases and drive
//! them from the terminal. Every write takes `--dry-run`, asks first in a
//! terminal and needs `--yes` without one, and names the token ability it
//! needs when the daemon refuses it.

use clap::{Args, Subcommand, ValueEnum};
use oore_contract::{
    EnvironmentPromotion, ListReleaseFreezesResponse, ListReleasesResponse, ProjectEnvironment,
    Release, ReleaseDetailResponse,
};
use serde_json::{Value, json};

use crate::admin_api::{
    self, ApiFailure, api_url, call, confirm, decode, format_time, get, print_json,
    with_forbidden_hint,
};
use crate::build_commands::{Api, ApiConnectionArgs};

const OPERATE_HINT: &str = "This needs the release_operate ability (pause and halt). Ask an admin for a token that has it.";
const MANAGE_HINT: &str = "This needs the release_manage ability (raise, resume, complete, approve, release now, retry and promote). Ask an admin for a token that has it.";

#[derive(Debug, Args)]
pub(crate) struct ReleaseArgs {
    #[command(subcommand)]
    pub(crate) command: ReleaseSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ReleaseSubcommand {
    /// List store releases, newest first.
    List(ReleaseListArgs),
    /// Show a release, its history and the actions it allows now.
    Show(ReleaseShowArgs),
    /// Pause a staged rollout. Needs release_operate.
    Pause(ReleaseActionArgs),
    /// Halt a release at the store. Needs release_operate.
    Halt(ReleaseActionArgs),
    /// Resume a paused or halted rollout. Needs release_manage.
    Resume(ReleaseActionArgs),
    /// Roll out to everyone and finish the release. Needs release_manage.
    Complete(ReleaseActionArgs),
    /// Approve a release that waits for approval. Needs release_manage.
    Approve(ReleaseActionArgs),
    /// Release an approved App Store version now. Needs release_manage.
    ReleaseNow(ReleaseActionArgs),
    /// Retry a release the store refused or that failed. Needs release_manage.
    Retry(ReleaseActionArgs),
    /// Raise a staged rollout to a percentage. Needs release_manage.
    Raise(ReleaseRaiseArgs),
    /// List, set and clear release freezes, which stop new store releases.
    Freeze(FreezeArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum StateFilter {
    Uploading,
    Processing,
    Blocked,
    InReview,
    Approved,
    Rejected,
    Rolling,
    Paused,
    Halted,
    Complete,
    Failed,
}

impl StateFilter {
    fn as_str(self) -> &'static str {
        match self {
            Self::Uploading => "uploading",
            Self::Processing => "processing",
            Self::Blocked => "blocked",
            Self::InReview => "in_review",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Rolling => "rolling",
            Self::Paused => "paused",
            Self::Halted => "halted",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct ReleaseListArgs {
    /// Only this app's releases.
    #[arg(long, value_name = "PROJECT_ID")]
    project: Option<String>,

    /// Only releases of this build.
    #[arg(long, value_name = "BUILD_ID")]
    build: Option<String>,

    /// Only releases in this state.
    #[arg(long, value_enum)]
    state: Option<StateFilter>,

    /// How many to list.
    #[arg(long, default_value = "20", value_parser = clap::value_parser!(u32).range(1..=200))]
    limit: u32,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ReleaseShowArgs {
    /// Release ID, from `oore release list`.
    release_id: String,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

/// What every release write takes.
#[derive(Debug, Args)]
pub(crate) struct WriteArgs {
    /// Print what would be sent and the actions the release allows now,
    /// and change nothing.
    #[arg(long, default_value = "false")]
    dry_run: bool,

    /// Go ahead without asking. Required when no terminal is attached.
    #[arg(long, short = 'y', default_value = "false")]
    yes: bool,

    /// Why, recorded in the release's history.
    #[arg(long)]
    reason: Option<String>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the result as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ReleaseActionArgs {
    /// Release ID, from `oore release list`.
    release_id: String,

    #[command(flatten)]
    write: WriteArgs,
}

#[derive(Debug, Args)]
pub(crate) struct ReleaseRaiseArgs {
    /// Release ID, from `oore release list`.
    release_id: String,

    /// Rollout percentage to raise to, above 0 and up to 100.
    #[arg(long, value_name = "PERCENT")]
    to: f64,

    #[command(flatten)]
    write: WriteArgs,
}

#[derive(Debug, Args)]
pub(crate) struct FreezeArgs {
    #[command(subcommand)]
    command: FreezeSubcommand,
}

#[derive(Debug, Subcommand)]
enum FreezeSubcommand {
    /// List the freezes in force.
    List(FreezeListArgs),
    /// Freeze store releases for one app, or for every app without
    /// --project. Needs release_manage.
    Set(FreezeSetArgs),
    /// Lift a freeze. Needs release_manage.
    Clear(FreezeClearArgs),
}

#[derive(Debug, Args)]
struct FreezeListArgs {
    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
struct FreezeSetArgs {
    /// Freeze only this app.
    #[arg(long, value_name = "PROJECT_ID")]
    project: Option<String>,

    #[command(flatten)]
    write: WriteArgs,
}

#[derive(Debug, Args)]
struct FreezeClearArgs {
    /// Freeze ID, from `oore release freeze list`.
    freeze_id: String,

    #[command(flatten)]
    write: WriteArgs,
}

#[derive(Debug, Args)]
pub(crate) struct PromoteArgs {
    /// Build whose release to promote.
    build_id: String,

    /// Track to promote into, by name, key or ID, such as production.
    #[arg(long, value_name = "TRACK")]
    to: String,

    #[command(flatten)]
    write: WriteArgs,
}

/// A release action: its route segment, its CLI name, and the hint for a
/// refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Action {
    route: &'static str,
    hint: &'static str,
}

const PAUSE: Action = Action {
    route: "pause",
    hint: OPERATE_HINT,
};
const HALT: Action = Action {
    route: "halt",
    hint: OPERATE_HINT,
};
const RESUME: Action = Action {
    route: "resume",
    hint: MANAGE_HINT,
};
const COMPLETE: Action = Action {
    route: "complete",
    hint: MANAGE_HINT,
};
const APPROVE: Action = Action {
    route: "approve",
    hint: MANAGE_HINT,
};
const RELEASE_NOW: Action = Action {
    route: "release_now",
    hint: MANAGE_HINT,
};
const RETRY: Action = Action {
    route: "retry",
    hint: MANAGE_HINT,
};
const RAISE: Action = Action {
    route: "raise",
    hint: MANAGE_HINT,
};

pub(crate) async fn handle(args: ReleaseArgs) -> anyhow::Result<()> {
    match args.command {
        ReleaseSubcommand::List(args) => list(args).await,
        ReleaseSubcommand::Show(args) => show(args).await,
        ReleaseSubcommand::Pause(args) => act(args.release_id, PAUSE, None, args.write).await,
        ReleaseSubcommand::Halt(args) => act(args.release_id, HALT, None, args.write).await,
        ReleaseSubcommand::Resume(args) => act(args.release_id, RESUME, None, args.write).await,
        ReleaseSubcommand::Complete(args) => act(args.release_id, COMPLETE, None, args.write).await,
        ReleaseSubcommand::Approve(args) => act(args.release_id, APPROVE, None, args.write).await,
        ReleaseSubcommand::ReleaseNow(args) => {
            act(args.release_id, RELEASE_NOW, None, args.write).await
        }
        ReleaseSubcommand::Retry(args) => act(args.release_id, RETRY, None, args.write).await,
        ReleaseSubcommand::Raise(args) => {
            let fraction = raise_fraction(args.to)?;
            act(args.release_id, RAISE, Some(fraction), args.write).await
        }
        ReleaseSubcommand::Freeze(args) => match args.command {
            FreezeSubcommand::List(args) => list_freezes(args).await,
            FreezeSubcommand::Set(args) => set_freeze(args).await,
            FreezeSubcommand::Clear(args) => clear_freeze(args).await,
        },
    }
}

fn raise_fraction(percent: f64) -> anyhow::Result<f64> {
    if !(percent > 0.0 && percent <= 100.0) {
        return Err(admin_api::UsageError(format!(
            "--to must be above 0 and at most 100, not {percent}"
        ))
        .into());
    }
    Ok(percent / 100.0)
}

fn snake(value: impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn percent(fraction: f64) -> String {
    let value = fraction * 100.0;
    if (value - value.round()).abs() < 0.05 {
        format!("{value:.0}%")
    } else {
        format!("{value:.1}%")
    }
}

fn release_line(release: &Release) -> Vec<String> {
    vec![
        release.id.clone(),
        release
            .project_name
            .clone()
            .unwrap_or_else(|| release.project_id.clone()),
        format!(
            "{} {}",
            snake(release.store),
            release
                .environment_name
                .as_deref()
                .unwrap_or(&release.track)
        ),
        release.version_name.clone().unwrap_or_else(|| "-".into()),
        snake(release.state),
        percent(release.rollout_fraction),
        format_time(Some(release.updated_at)),
    ]
}

async fn list(args: ReleaseListArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let mut url = api_url(&api, &["releases"])?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("limit", &args.limit.to_string());
        if let Some(project) = &args.project {
            query.append_pair("project_id", project.trim());
        }
        if let Some(build) = &args.build {
            query.append_pair("build_id", build.trim());
        }
        if let Some(state) = args.state {
            query.append_pair("state", state.as_str());
        }
    }
    let response = get(&api, url, "release list").await?;
    if args.json {
        return print_json(&response);
    }
    let list: ListReleasesResponse = decode(&response, "release list")?;
    if list.releases.is_empty() {
        println!("No releases.");
        return Ok(());
    }
    let rows = list.releases.iter().map(release_line).collect::<Vec<_>>();
    admin_api::print_table(
        &[
            "ID", "APP", "TRACK", "VERSION", "STATE", "ROLLOUT", "UPDATED",
        ],
        &rows,
    );
    if list.total > i64::from(args.limit) {
        println!(
            "Showing {} of {}. Use --limit for more.",
            rows.len(),
            list.total
        );
    }
    Ok(())
}

fn print_release(release: &Release) {
    println!(
        "Release {} of {} {}",
        release.id,
        release
            .project_name
            .as_deref()
            .unwrap_or(&release.project_id),
        release.version_name.as_deref().unwrap_or("")
    );
    println!(
        "  Store:   {} {} ({})",
        snake(release.store),
        release.track,
        release.platform
    );
    println!(
        "  State:   {}{}",
        snake(release.state),
        release
            .state_reason
            .as_deref()
            .map(|reason| format!(", {reason}"))
            .unwrap_or_default()
    );
    println!("  Rollout: {}", percent(release.rollout_fraction));
    if let Some(error) = &release.store_error {
        println!("  Store said: {error}");
    }
    if release.awaiting_approval {
        println!("  Waiting for approval.");
    }
    let actions = release
        .allowed_actions
        .iter()
        .map(|action| snake(action).replace('_', "-"))
        .collect::<Vec<_>>();
    println!(
        "  Allowed now: {}",
        if actions.is_empty() {
            "nothing".to_string()
        } else {
            actions.join(", ")
        }
    );
    if let Some(url) = &release.console_url {
        println!("  Console: {url}");
    }
}

async fn show(args: ReleaseShowArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let response = get(
        &api,
        api_url(&api, &["releases", &args.release_id])?,
        "release lookup",
    )
    .await?;
    if args.json {
        return print_json(&response);
    }
    let detail: ReleaseDetailResponse = decode(&response, "release lookup")?;
    print_release(&detail.release);
    if !detail.history.is_empty() {
        println!("\nHistory:");
        for entry in &detail.history {
            println!(
                "  {}  {}{}",
                format_time(Some(entry.at)),
                snake(entry.to_state),
                entry
                    .actor_email
                    .as_deref()
                    .map(|actor| format!(" by {actor}"))
                    .unwrap_or_default()
            );
        }
    }
    Ok(())
}

fn action_body(fraction: Option<f64>, reason: Option<&str>) -> Value {
    let mut body = json!({});
    if let Some(fraction) = fraction {
        body["fraction"] = json!(fraction);
    }
    if let Some(reason) = reason.map(str::trim).filter(|reason| !reason.is_empty()) {
        body["reason"] = json!(reason);
    }
    body
}

/// What `--dry-run` prints.
fn dry_run_plan(method: &str, path: &str, body: &Value, extra: Value) -> Value {
    let mut plan = json!({ "dry_run": true, "method": method, "path": path, "body": body });
    if let (Some(plan), Value::Object(extra)) = (plan.as_object_mut(), extra) {
        plan.extend(extra);
    }
    plan
}

fn print_plan(plan: &Value, json_output: bool) -> anyhow::Result<()> {
    if json_output {
        return print_json(plan);
    }
    println!(
        "Would send {} {} {}",
        plan["method"].as_str().unwrap_or(""),
        plan["path"].as_str().unwrap_or(""),
        plan["body"]
    );
    if let Some(actions) = plan["allowed_actions"].as_array() {
        let actions = actions.iter().filter_map(Value::as_str).collect::<Vec<_>>();
        println!(
            "The release allows now: {}",
            if actions.is_empty() {
                "nothing".to_string()
            } else {
                actions.join(", ")
            }
        );
    }
    if plan["allowed"] == json!(false) {
        println!("This action is not allowed in the release's current state.");
    }
    println!("Nothing was changed.");
    Ok(())
}

async fn act(
    release_id: String,
    action: Action,
    fraction: Option<f64>,
    write: WriteArgs,
) -> anyhow::Result<()> {
    let id = release_id.trim();
    let api = Api::connect(&write.connection)?;
    let body = action_body(fraction, write.reason.as_deref());
    let path = format!("/v1/releases/{id}/{}", action.route);
    if write.dry_run {
        let detail = get(&api, api_url(&api, &["releases", id])?, "release lookup").await?;
        let release: Release = decode(&detail["release"], "release lookup")?;
        let allowed_actions = release
            .allowed_actions
            .iter()
            .map(snake)
            .collect::<Vec<_>>();
        let allowed = allowed_actions.iter().any(|name| name == action.route);
        let plan = dry_run_plan(
            "POST",
            &path,
            &body,
            json!({
                "state": snake(release.state),
                "allowed": allowed,
                "allowed_actions": allowed_actions,
            }),
        );
        return print_plan(&plan, write.json);
    }
    let label = action.route.replace('_', " ");
    let question = match fraction {
        Some(fraction) => format!("Raise release {id} to {}?", percent(fraction)),
        None => format!("{} release {id}?", capitalize(&label)),
    };
    confirm(write.yes, &question)?;
    let result = call(
        &api,
        reqwest::Method::POST,
        api_url(&api, &["releases", id, action.route])?,
        Some(&body),
        &format!("release {label}"),
    )
    .await
    .map_err(|error| with_forbidden_hint(error, action.hint))?;
    if write.json {
        return print_json(&result);
    }
    let release: Release = decode(&result, "release action")?;
    println!(
        "Done. Release {} is {} at {}.",
        release.id,
        snake(release.state),
        percent(release.rollout_fraction)
    );
    Ok(())
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

async fn list_freezes(args: FreezeListArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let response = get(&api, api_url(&api, &["release-freezes"])?, "freeze list").await?;
    if args.json {
        return print_json(&response);
    }
    let list: ListReleaseFreezesResponse = decode(&response, "freeze list")?;
    if list.freezes.is_empty() {
        println!("No release freezes.");
        return Ok(());
    }
    let rows = list
        .freezes
        .iter()
        .map(|freeze| {
            vec![
                freeze.id.clone(),
                freeze
                    .project_name
                    .clone()
                    .or_else(|| freeze.project_id.clone())
                    .unwrap_or_else(|| "every app".into()),
                freeze.reason.clone(),
                format_time(Some(freeze.created_at)),
                freeze.created_by_email.clone().unwrap_or_default(),
            ]
        })
        .collect::<Vec<_>>();
    admin_api::print_table(&["ID", "APP", "REASON", "SINCE", "BY"], &rows);
    Ok(())
}

async fn set_freeze(args: FreezeSetArgs) -> anyhow::Result<()> {
    let reason = args
        .write
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .ok_or_else(|| admin_api::UsageError("--reason is required to set a freeze".into()))?;
    let mut body = json!({ "reason": reason });
    if let Some(project) = &args.project {
        body["project_id"] = json!(project.trim());
    }
    if args.write.dry_run {
        return print_plan(
            &dry_run_plan("PUT", "/v1/release-freezes", &body, json!({})),
            args.write.json,
        );
    }
    let scope = args
        .project
        .as_deref()
        .map(|project| format!("app {project}"))
        .unwrap_or_else(|| "every app".to_string());
    confirm(
        args.write.yes,
        &format!("Freeze store releases for {scope}?"),
    )?;
    let api = Api::connect(&args.write.connection)?;
    let saved = call(
        &api,
        reqwest::Method::PUT,
        api_url(&api, &["release-freezes"])?,
        Some(&body),
        "freeze set",
    )
    .await
    .map_err(|error| with_forbidden_hint(error, MANAGE_HINT))?;
    if args.write.json {
        return print_json(&saved);
    }
    println!(
        "Froze store releases for {scope} ({}).",
        saved["id"].as_str().unwrap_or("")
    );
    Ok(())
}

async fn clear_freeze(args: FreezeClearArgs) -> anyhow::Result<()> {
    let id = args.freeze_id.trim();
    if args.write.dry_run {
        return print_plan(
            &dry_run_plan(
                "DELETE",
                &format!("/v1/release-freezes/{id}"),
                &Value::Null,
                json!({}),
            ),
            args.write.json,
        );
    }
    confirm(args.write.yes, &format!("Lift release freeze {id}?"))?;
    let api = Api::connect(&args.write.connection)?;
    call(
        &api,
        reqwest::Method::DELETE,
        api_url(&api, &["release-freezes", id])?,
        None,
        "freeze clear",
    )
    .await
    .map_err(|error| with_forbidden_hint(error, MANAGE_HINT))?;
    if args.write.json {
        return print_json(&json!({ "cleared": true, "id": id }));
    }
    println!("Lifted release freeze {id}.");
    Ok(())
}

/// The environment named by `wanted`: its ID, key, name or store track.
fn find_environment<'a>(
    environments: &'a [ProjectEnvironment],
    wanted: &str,
) -> anyhow::Result<&'a ProjectEnvironment> {
    let wanted = wanted.trim();
    let found = environments
        .iter()
        .find(|environment| environment.id == wanted)
        .or_else(|| {
            environments.iter().find(|environment| {
                environment.key.eq_ignore_ascii_case(wanted)
                    || environment.name.eq_ignore_ascii_case(wanted)
            })
        })
        .or_else(|| {
            let by_track = environments
                .iter()
                .filter(|environment| environment.track.eq_ignore_ascii_case(wanted))
                .collect::<Vec<_>>();
            (by_track.len() == 1).then(|| by_track[0])
        });
    found.ok_or_else(|| {
        let names = environments
            .iter()
            .map(|environment| environment.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        ApiFailure {
            what: "track lookup".into(),
            status: 404,
            code: Some("not_found".into()),
            message: format!("the app has no track named {wanted}"),
            hint: (!names.is_empty()).then(|| format!("Tracks: {names}")),
        }
        .into()
    })
}

pub(crate) async fn promote(args: PromoteArgs) -> anyhow::Result<()> {
    let build_id = args.build_id.trim();
    let api = Api::connect(&args.write.connection)?;
    let build = get(&api, api_url(&api, &["builds", build_id])?, "build lookup").await?;
    let project_id = build["build"]["project_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("the build lookup response has no project"))?
        .to_string();
    let environments = get(
        &api,
        api_url(&api, &["projects", &project_id, "environments"])?,
        "track list",
    )
    .await?;
    let environments: Vec<ProjectEnvironment> =
        decode(&environments["environments"], "track list")?;
    let environment = find_environment(&environments, &args.to)?;
    let mut body = json!({ "build_id": build_id, "environment_id": environment.id });
    if let Some(reason) = args
        .write
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
    {
        body["reason"] = json!(reason);
    }
    if args.write.dry_run {
        return print_plan(
            &dry_run_plan(
                "POST",
                &format!("/v1/projects/{project_id}/promotions"),
                &body,
                json!({
                    "track": environment.name,
                    "promotable": environment.promotable,
                    "approval_required": environment.approval_required,
                }),
            ),
            args.write.json,
        );
    }
    confirm(
        args.write.yes,
        &format!(
            "Promote build {build_id} to {} ({} {})?",
            environment.name,
            snake(environment.store),
            environment.track
        ),
    )?;
    let promotion = call(
        &api,
        reqwest::Method::POST,
        api_url(&api, &["projects", &project_id, "promotions"])?,
        Some(&body),
        "promote",
    )
    .await
    .map_err(|error| with_forbidden_hint(error, MANAGE_HINT))?;
    let decoded: EnvironmentPromotion = decode(&promotion, "promote")?;
    let pending = snake(decoded.state) == "pending";
    if args.write.json {
        print_json(&promotion)?;
    } else if pending {
        println!(
            "Promotion {} to {} is waiting for approval.",
            decoded.id, decoded.environment_name
        );
    } else {
        println!(
            "Promotion {} to {} is {}.",
            decoded.id,
            decoded.environment_name,
            snake(decoded.state)
        );
    }
    if pending {
        // A person must approve it: exit 2, as for other blocked work.
        std::process::exit(2);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raise_takes_a_percentage_and_sends_a_fraction() {
        assert_eq!(raise_fraction(25.0).unwrap(), 0.25);
        assert_eq!(raise_fraction(100.0).unwrap(), 1.0);
        for bad in [0.0, -5.0, 100.5, f64::NAN] {
            assert_eq!(
                admin_api::exit_code_for(&raise_fraction(bad).unwrap_err()),
                64
            );
        }
    }

    #[test]
    fn an_action_body_carries_only_what_was_given() {
        assert_eq!(action_body(None, None), json!({}));
        assert_eq!(
            action_body(Some(0.5), Some(" crash spike ")),
            json!({ "fraction": 0.5, "reason": "crash spike" })
        );
    }

    #[test]
    fn refusals_name_the_ability_the_action_needs() {
        let refused = || -> anyhow::Error {
            ApiFailure {
                what: "release pause".into(),
                status: 403,
                code: Some("forbidden".into()),
                message: "no".into(),
                hint: None,
            }
            .into()
        };
        let operate = with_forbidden_hint(refused(), PAUSE.hint);
        assert!(operate.to_string().contains("release_operate"));
        assert_eq!(admin_api::exit_code_for(&operate), 4);
        assert!(
            with_forbidden_hint(refused(), RAISE.hint)
                .to_string()
                .contains("release_manage")
        );
    }

    #[test]
    fn rollout_percentages_read_cleanly() {
        assert_eq!(percent(0.2), "20%");
        assert_eq!(percent(0.025), "2.5%");
        assert_eq!(percent(1.0), "100%");
    }
}
