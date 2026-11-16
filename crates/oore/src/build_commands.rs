//! `oore build run`, `oore build watch`, `oore pipeline workflows` and
//! `oore pipeline readiness`: start a build of a pipeline, naming its
//! workflow, follow it to its result and the store's answer, list the
//! workflows it can name, and check what would stop its next store upload.

use anyhow::Context;
use clap::Args;
use oore_contract::{
    Artifact, ArtifactDownloadLinkResponse, ArtifactInstallLinkResponse, Build,
    BuildDetailResponse, BuildLogChunk, BuildPlatform, BuildReleaseOutcome, CancelBuildResponse,
    CreateBuildRequest, CreateBuildResponse, ListArtifactsResponse, ListBuildsResponse,
    PipelineFileState, PipelineWorkflowSummary, PipelineWorkflowsResponse, ReadinessLevel,
    ReleaseStore, RerunBuildResponse, RetryBuildStepRequest, StoreReadinessResponse,
};

use crate::build_logs::{LogStream, StepMarker};
use crate::cli_contract::{CliError, exit, print_json, print_json_line};
use crate::project_commands::{ago, render_table, resolve_pipeline, resolve_project};
use crate::{endpoint_http_client_builder, resolve_daemon_url, resolve_session_token};

/// Where to reach the daemon and how to sign in, shared by these commands.
#[derive(Debug, Default, Clone, Args)]
pub(crate) struct ApiConnectionArgs {
    /// Daemon URL. Falls back to OORE_URL, OORE_DAEMON_URL, the URL saved by
    /// `oore login`, then http://127.0.0.1:8787.
    #[arg(long = "url", visible_alias = "daemon-url", env = "OORE_URL")]
    pub(crate) daemon_url: Option<String>,

    /// API or session token. Falls back to OORE_TOKEN, OORE_SESSION_TOKEN,
    /// then the session saved by `oore login`.
    #[arg(long, env = "OORE_TOKEN")]
    pub(crate) token: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct BuildRunArgs {
    /// Pipeline to build. Takes a name or an ID.
    #[arg(long)]
    pipeline: String,

    /// App the pipeline belongs to, by name or ID, when its name is not
    /// unique.
    #[arg(long, value_name = "APP")]
    project: Option<String>,

    /// `.oore.yaml` version 2 workflow to build. Defaults to the pipeline's
    /// default workflow, then the file's only workflow.
    #[arg(long)]
    workflow: Option<String>,

    /// Branch to build. Defaults to the project's default branch.
    #[arg(long)]
    branch: Option<String>,

    /// Exact commit to build. Overrides the branch head.
    #[arg(long)]
    commit: Option<String>,

    /// Build only `android`, `ios` or `macos`. Repeat for several.
    #[arg(long = "platform", value_name = "PLATFORM")]
    platforms: Vec<String>,

    /// Release notes for the app version this build makes. They become the
    /// version's notes, shared by its other builds and sent to testers,
    /// Google Play and the App Store, unless the repository's CHANGELOG.md
    /// already has them. `oore notes set` changes them later.
    #[arg(long, value_name = "TEXT")]
    notes: Option<String>,

    /// Rehearse the release: build and sign as usual, then ask Google Play
    /// and App Store Connect to check it. Nothing is submitted or released.
    #[arg(long, default_value = "false")]
    rehearsal: bool,

    /// Follow the build to its result, like `oore build watch`, with the same
    /// exit codes.
    #[arg(long)]
    wait: bool,

    /// With --wait, also wait for the store's answer.
    #[arg(long, requires = "wait")]
    store: bool,

    /// With --wait, seconds between checks.
    #[arg(long, default_value = "10", requires = "wait", value_parser = clap::value_parser!(u64).range(2..=3600))]
    interval: u64,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the queued build as JSON. With --wait, print JSON lines as
    /// `oore build watch --json` does.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct BuildWatchArgs {
    /// Build to follow.
    build_id: String,

    /// Keep watching after the build succeeds until the store is done with
    /// its release: live, rejected, failed, or blocked until someone acts.
    #[arg(long, default_value = "false")]
    store: bool,

    /// Seconds between checks.
    #[arg(long, default_value = "10", value_parser = clap::value_parser!(u64).range(2..=3600))]
    interval: u64,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print one JSON line per change, then the settled build.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PipelineWorkflowsArgs {
    /// Pipeline whose file to read.
    pipeline_id: String,

    /// Branch, tag or commit to read the file at. Defaults to the project's
    /// default branch.
    #[arg(long = "ref", value_name = "REF")]
    reference: Option<String>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PipelineReadinessArgs {
    /// Pipeline whose next store upload to check.
    pipeline_id: String,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

/// An authenticated client for the daemon.
///
/// Every command family builds on this: `send` and friends turn a failed
/// answer into a [`CliError`] that carries the API's code and the shared
/// exit code (4 for 401 and 403, 5 for 404).
pub(crate) struct Api {
    pub(crate) client: reqwest::Client,
    pub(crate) daemon_url: String,
    token: String,
}

impl Api {
    pub(crate) fn connect(args: &ApiConnectionArgs) -> anyhow::Result<Self> {
        let daemon_url = resolve_daemon_url(args.daemon_url.as_deref())?;
        let token =
            resolve_session_token(args.token.as_deref(), &daemon_url)?.ok_or_else(|| {
                CliError::not_signed_in("not signed in")
                    .with_hint("Run `oore login`, or pass --token or set OORE_TOKEN.")
            })?;
        let client = endpoint_http_client_builder(&daemon_url)
            .build()
            .context("failed to build the API client")?;
        Ok(Self {
            client,
            daemon_url: daemon_url.trim_end_matches('/').to_string(),
            token,
        })
    }

    /// `path` on the daemon, such as `/v1/builds`.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.daemon_url)
    }

    /// A path built from segments, each one percent-encoded, so a name with
    /// a space or a slash stays one segment.
    pub(crate) fn url_with(&self, segments: &[&str]) -> anyhow::Result<String> {
        let mut url = url::Url::parse(&self.daemon_url)
            .with_context(|| format!("daemon URL {} is not valid", self.daemon_url))?;
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("daemon URL {} cannot have a path", self.daemon_url))?
            .pop_if_empty()
            .extend(segments);
        Ok(url.to_string())
    }

    /// GET `path` and parse the JSON answer.
    pub(crate) async fn get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
    ) -> anyhow::Result<T> {
        self.send(self.client.get(self.url(path)), what).await
    }

    /// POST `body` as JSON to a full URL, such as one from `url_with`, and
    /// parse the JSON answer.
    pub(crate) async fn post_url<B: serde::Serialize + ?Sized, T: serde::de::DeserializeOwned>(
        &self,
        url: String,
        body: &B,
        what: &str,
    ) -> anyhow::Result<T> {
        self.send(self.client.post(url).json(body), what).await
    }

    /// Sends a request signed in, returning the raw answer whatever its
    /// status.
    pub(crate) async fn execute(
        &self,
        request: reqwest::RequestBuilder,
    ) -> anyhow::Result<reqwest::Response> {
        request
            .bearer_auth(&self.token)
            .send()
            .await
            .with_context(|| format!("failed to reach daemon at {}", self.daemon_url))
    }

    pub(crate) async fn send<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        what: &str,
    ) -> anyhow::Result<T> {
        let response = self.execute(request).await?;
        let status = response.status();
        if status.is_success() {
            return response
                .json()
                .await
                .with_context(|| format!("failed to parse the {what} response"));
        }
        Err(api_failure(response, what).await.into())
    }

    /// Like `send`, but `None` when the daemon answers 404 or 405, for an
    /// endpoint an older daemon does not have yet.
    pub(crate) async fn send_if_supported<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        what: &str,
    ) -> anyhow::Result<Option<T>> {
        let response = self.execute(request).await?;
        let status = response.status();
        if status.is_success() {
            return response
                .json()
                .await
                .map(Some)
                .with_context(|| format!("failed to parse the {what} response"));
        }
        if matches!(status.as_u16(), 404 | 405) {
            return Ok(None);
        }
        Err(api_failure(response, what).await.into())
    }

    /// The feature names the daemon lists at `GET /v1/meta`; empty on a
    /// daemon without it.
    pub(crate) async fn features(&self) -> Vec<String> {
        let Ok(response) = self.execute(self.client.get(self.url("/v1/meta"))).await else {
            return Vec::new();
        };
        if !response.status().is_success() {
            return Vec::new();
        }
        response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|meta| {
                meta.get("features")?.as_array().map(|features| {
                    features
                        .iter()
                        .filter_map(|feature| feature.as_str().map(str::to_string))
                        .collect()
                })
            })
            .unwrap_or_default()
    }

    /// Like `send`, but `None` when the session's role may not read it.
    pub(crate) async fn send_unless_forbidden<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        what: &str,
    ) -> anyhow::Result<Option<T>> {
        let response = request
            .bearer_auth(&self.token)
            .send()
            .await
            .with_context(|| format!("failed to reach daemon at {}", self.daemon_url))?;
        let status = response.status();
        if status == reqwest::StatusCode::FORBIDDEN {
            return Ok(None);
        }
        if status.is_success() {
            return response
                .json()
                .await
                .map(Some)
                .with_context(|| format!("failed to parse the {what} response"));
        }
        Err(api_failure(response, what).await.into())
    }
}

/// The [`CliError`] for an answer that was not a success.
pub(crate) async fn api_failure(response: reqwest::Response, what: &str) -> CliError {
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    CliError::from_api(status, what, &body)
}

fn parse_platforms(values: &[String]) -> anyhow::Result<Vec<BuildPlatform>> {
    values
        .iter()
        .map(|platform| {
            serde_json::from_value::<BuildPlatform>(serde_json::Value::String(
                platform.trim().to_ascii_lowercase(),
            ))
            .map_err(|_| {
                anyhow::anyhow!("unknown platform '{platform}'; use android, ios or macos")
            })
        })
        .collect()
}

fn trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn create_build_request(args: &BuildRunArgs) -> anyhow::Result<CreateBuildRequest> {
    let platforms = parse_platforms(&args.platforms)?;
    let branch = trimmed(args.branch.as_deref());
    Ok(CreateBuildRequest {
        pipeline_id: args.pipeline.trim().to_string(),
        platforms: (!platforms.is_empty()).then_some(platforms),
        trigger_ref: branch.clone(),
        branch,
        commit_sha: trimmed(args.commit.as_deref()),
        changelog: None,
        store_notes: trimmed(args.notes.as_deref()),
        workflow: trimmed(args.workflow.as_deref()),
        rehearsal: args.rehearsal.then_some(true),
    })
}

pub(crate) async fn handle_build_run(args: BuildRunArgs) -> anyhow::Result<()> {
    let mut request = create_build_request(&args)?;
    let api = Api::connect(&args.connection)?;
    // Builds are created under the pipeline's project.
    let pipeline = crate::project_commands::resolve_pipeline(
        &api,
        &request.pipeline_id,
        args.project.as_deref(),
    )
    .await?;
    request.pipeline_id = pipeline.id.clone();
    let path = api.url_with(&["v1", "projects", &pipeline.project_id, "builds"])?;
    let created: CreateBuildResponse = api
        .send(api.client.post(path).json(&request), "build request")
        .await?;
    let build = created.build;
    if args.json && !args.wait {
        return print_json(&build);
    }
    if args.json {
        print_json_line(&WatchEvent::Queued { build: &build })?;
    } else {
        let mut line = format!(
            "Queued {} #{} of {}",
            if build.rehearsal {
                "rehearsal"
            } else {
                "build"
            },
            build.build_number,
            pipeline.name
        );
        if let Some(workflow) = &build.workflow {
            line.push_str(&format!(", workflow {workflow}"));
        }
        if let Some(reference) = build.branch.as_deref().or(build.commit_sha.as_deref()) {
            line.push_str(&format!(", on {reference}"));
        }
        println!("{line}");
        println!("Build id: {}", build.id);
        if build.rehearsal {
            println!("Nothing is submitted. See what the stores said on the build page.");
        }
    }
    if !args.wait {
        return Ok(());
    }
    let settled = follow_build(&api, &build.id, args.store, args.interval, args.json).await?;
    finish_watch(&settled, args.store, args.json)
}

const FINISHED_BUILD_STATUSES: [&str; 5] =
    ["succeeded", "failed", "canceled", "timed_out", "expired"];

/// One line for where a build stands, such as "Build #12 succeeded, in
/// store review".
fn watch_line(build: &Build) -> String {
    let mut line = format!(
        "Build #{} {}",
        build.build_number,
        build.status.replace('_', " ")
    );
    if let Some(outcome) = build.release_outcome {
        line.push_str(&format!(", {}", outcome.phrase()));
    }
    line
}

/// Whether there is nothing more to wait for: the build has finished and,
/// when the store's answer was asked for, the store is done with it too.
fn watch_done(build: &Build, store: bool) -> bool {
    if !FINISHED_BUILD_STATUSES.contains(&build.status.as_str()) {
        return false;
    }
    !store
        || build.status != "succeeded"
        || build
            .release_outcome
            .is_none_or(BuildReleaseOutcome::is_settled)
}

/// The exit code a finished watch reports: 0 for a good result, 1 for a
/// build that did not succeed, 3 for a release the store did not take.
fn watch_exit_code(build: &Build, store: bool) -> i32 {
    if build.status != "succeeded" {
        return 1;
    }
    match build.release_outcome {
        Some(
            BuildReleaseOutcome::Blocked
            | BuildReleaseOutcome::Rejected
            | BuildReleaseOutcome::Failed,
        ) if store => 3,
        _ => 0,
    }
}

pub(crate) async fn handle_build_watch(args: BuildWatchArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let build = follow_build(
        &api,
        args.build_id.trim(),
        args.store,
        args.interval,
        args.json,
    )
    .await?;
    finish_watch(&build, args.store, args.json)
}

/// Ends a watch: the settled build as JSON, a nudge about `--store`, and the
/// shared exit code for the result.
fn finish_watch(build: &Build, store: bool, json: bool) -> anyhow::Result<()> {
    if json {
        print_json_line(&WatchEvent::Settled { build })?;
    } else if !store
        && build.status == "succeeded"
        && build
            .release_outcome
            .is_some_and(|outcome| !outcome.is_settled())
    {
        println!("Add --store to wait for the store too.");
    }
    match watch_exit_code(build, store) {
        0 => Ok(()),
        code => Err(CliError::outcome(
            u8::try_from(code).unwrap_or(exit::FAILED),
            if code == i32::from(exit::STORE_REFUSED) {
                "store_refused"
            } else {
                "build_not_succeeded"
            },
            watch_line(build),
        )
        .into()),
    }
}

/// One change `oore build watch --json` prints as a JSON line.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub(crate) enum WatchEvent<'a> {
    /// `oore build run --wait` queued the build.
    Queued { build: &'a Build },
    /// The build's status or store answer changed.
    Status {
        build_id: &'a str,
        build_number: i64,
        status: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        release_outcome: Option<BuildReleaseOutcome>,
    },
    /// A step started or ended.
    Step {
        build_id: &'a str,
        name: &'a str,
        state: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i64>,
    },
    /// Nothing more to wait for.
    Settled { build: &'a Build },
}

/// Follows a build until `watch_done`, printing each status change and,
/// from the live log stream, each step as it starts and ends. Falls back to
/// checking every `interval` seconds when the stream is not available.
pub(crate) async fn follow_build(
    api: &Api,
    build_id: &str,
    store: bool,
    interval: u64,
    json: bool,
) -> anyhow::Result<Build> {
    let path = api.url_with(&["v1", "builds", build_id])?;
    let interval = std::time::Duration::from_secs(interval);
    let mut last_line = String::new();
    let mut stream: Option<LogStream> = None;
    let mut stream_finished = false;
    let mut last_sequence: Option<i64> = None;
    loop {
        let detail: BuildDetailResponse = api.send(api.client.get(&path), "build lookup").await?;
        let build = detail.build;
        let line = watch_line(&build);
        if line != last_line {
            if json {
                print_json_line(&WatchEvent::Status {
                    build_id: &build.id,
                    build_number: build.build_number,
                    status: &build.status,
                    release_outcome: build.release_outcome,
                })?;
            } else {
                println!("{line}");
            }
            last_line = line;
        }
        if watch_done(&build, store) {
            return Ok(build);
        }
        let running = !FINISHED_BUILD_STATUSES.contains(&build.status.as_str())
            && build.status != "queued"
            && build.status != "pending";
        if stream.is_none() && running && !stream_finished {
            stream = LogStream::open(api, build_id, last_sequence).await.ok();
        }
        let Some(live) = stream.as_mut() else {
            tokio::time::sleep(interval).await;
            continue;
        };
        // Read the stream until the next check is due.
        let deadline = tokio::time::Instant::now() + interval;
        loop {
            let event = match tokio::time::timeout_at(deadline, live.next()).await {
                Err(_elapsed) => break,
                Ok(Ok(Some(event))) => event,
                Ok(Ok(None) | Err(_)) => {
                    stream = None;
                    stream_finished = true;
                    break;
                }
            };
            if event.event == "done" {
                stream = None;
                stream_finished = true;
                break;
            }
            if event.event != "log" {
                continue;
            }
            let Ok(chunk) = serde_json::from_str::<BuildLogChunk>(&event.data) else {
                continue;
            };
            last_sequence = Some(chunk.sequence);
            for text in chunk.content.lines() {
                let Some(marker) = StepMarker::parse(text) else {
                    continue;
                };
                if json {
                    print_json_line(&WatchEvent::Step {
                        build_id,
                        name: &marker.name,
                        state: marker.state(),
                        exit_code: marker.exit_code,
                    })?;
                } else {
                    println!("  {}", marker.describe());
                }
            }
        }
    }
}

fn platform_label(platform: &BuildPlatform) -> &'static str {
    match platform {
        BuildPlatform::Android => "android",
        BuildPlatform::Ios => "ios",
        BuildPlatform::Macos => "macos",
    }
}

fn triggers_label(workflow: &PipelineWorkflowSummary) -> String {
    if workflow.uses_pipeline_triggers {
        "pipeline trigger settings".to_string()
    } else if workflow.triggers.is_empty() {
        "manual only".to_string()
    } else {
        workflow.triggers.join("; ")
    }
}

/// The table `oore pipeline workflows` prints.
fn render_workflows(response: &PipelineWorkflowsResponse) -> String {
    let reference = response
        .reference
        .as_deref()
        .unwrap_or("the default branch");
    let path = response.path.as_deref().unwrap_or("the pipeline file");
    let mut out = String::new();
    match response.state {
        PipelineFileState::Missing => {
            out.push_str(&format!(
                "No pipeline file on {reference}; builds use the pipeline's settings.\n"
            ));
        }
        PipelineFileState::Invalid => {
            out.push_str(&format!("{path} on {reference} has errors:\n"));
            for error in &response.errors {
                out.push_str(&format!("  {error}\n"));
            }
        }
        PipelineFileState::Unavailable => {
            out.push_str(&format!(
                "Could not read the pipeline file on {reference}:\n"
            ));
            for error in &response.errors {
                out.push_str(&format!("  {error}\n"));
            }
        }
        PipelineFileState::Valid if response.workflows.is_empty() => {
            out.push_str(&format!(
                "{path} on {reference} has no named workflows; builds run the whole file.\n"
            ));
        }
        PipelineFileState::Valid => {
            out.push_str(&format!("{path} on {reference}\n\n"));
            let rows: Vec<[String; 3]> = response
                .workflows
                .iter()
                .map(|workflow| {
                    let mut name = workflow.name.clone();
                    if response.default_workflow.as_deref() == Some(workflow.name.as_str()) {
                        name.push_str(" (default)");
                    }
                    let platforms = workflow
                        .platforms
                        .iter()
                        .map(platform_label)
                        .collect::<Vec<_>>()
                        .join(", ");
                    [name, platforms, triggers_label(workflow)]
                })
                .collect();
            let header = ["WORKFLOW", "PLATFORMS", "STARTS ON"];
            let mut widths = header.map(str::len);
            for row in &rows {
                for (width, cell) in widths.iter_mut().zip(row) {
                    *width = (*width).max(cell.len());
                }
            }
            let line = |cells: [&str; 3]| {
                let mut text = String::new();
                for (index, cell) in cells.iter().enumerate() {
                    if index == cells.len() - 1 {
                        text.push_str(cell);
                    } else {
                        text.push_str(&format!("{cell:<width$}  ", width = widths[index]));
                    }
                }
                text.trim_end().to_string() + "\n"
            };
            out.push_str(&line(header));
            for row in &rows {
                out.push_str(&line(row.each_ref().map(String::as_str)));
            }
            for workflow in &response.workflows {
                for publish in &workflow.publishes {
                    out.push_str(&format!("\n{}: {publish}", workflow.name));
                }
                for error in &workflow.errors {
                    out.push_str(&format!("\n{}: error: {error}", workflow.name));
                }
            }
            if response
                .workflows
                .iter()
                .any(|workflow| !workflow.publishes.is_empty() || !workflow.errors.is_empty())
            {
                out.push('\n');
            }
            if response.default_workflow.is_none() && response.workflows.len() > 1 {
                out.push_str(
                    "\nNo default workflow: name one with `oore build run --workflow`; \
                     triggers start every workflow that matches.\n",
                );
            }
        }
    }
    if let Some(default) = response.default_workflow.as_deref()
        && response.state == PipelineFileState::Valid
        && !response
            .workflows
            .iter()
            .any(|workflow| workflow.name == default)
    {
        out.push_str(&format!(
            "\nThe default workflow {default} is not in {path} on {reference}.\n"
        ));
    }
    out
}

pub(crate) async fn handle_pipeline_workflows(args: PipelineWorkflowsArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let mut request = api.client.get(format!(
        "{}/v1/pipelines/{}/workflows",
        api.daemon_url,
        args.pipeline_id.trim()
    ));
    if let Some(reference) = trimmed(args.reference.as_deref()) {
        request = request.query(&[("ref", reference)]);
    }
    let response: PipelineWorkflowsResponse = api.send(request, "workflow listing").await?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        print!("{}", render_workflows(&response));
    }
    Ok(())
}

pub(crate) async fn handle_pipeline_readiness(args: PipelineReadinessArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let response: StoreReadinessResponse = api
        .send(
            api.client.get(format!(
                "{}/v1/pipelines/{}/publish/readiness",
                api.daemon_url,
                args.pipeline_id.trim()
            )),
            "readiness check",
        )
        .await?;
    if args.json {
        print_json(&response)?;
    } else {
        print!("{}", render_readiness(&response));
    }
    // Scripts can stop before a long build when something blocks the upload.
    if response
        .items
        .iter()
        .any(|item| item.level == ReadinessLevel::Block)
    {
        return Err(CliError::outcome(
            exit::BLOCKED,
            "readiness_blocked",
            "something blocks the next store upload",
        )
        .into());
    }
    Ok(())
}

/// The checklist `oore pipeline readiness` prints, problems first.
fn render_readiness(response: &StoreReadinessResponse) -> String {
    if response.items.is_empty() {
        return "Nothing to check: this pipeline has no store uploads turned on.\n".to_string();
    }
    let mut items: Vec<_> = response.items.iter().collect();
    let rank = |level: ReadinessLevel| match level {
        ReadinessLevel::Block => 0,
        ReadinessLevel::Warn => 1,
        ReadinessLevel::Ok => 2,
    };
    items.sort_by_key(|item| rank(item.level));
    let problems = items
        .iter()
        .filter(|item| item.level != ReadinessLevel::Ok)
        .count();
    let mut out = if problems == 0 {
        "Ready to submit.\n\n".to_string()
    } else {
        format!(
            "{problems} {} before the next store upload.\n\n",
            if problems == 1 {
                "thing to fix"
            } else {
                "things to fix"
            }
        )
    };
    for item in items {
        let mark = match item.level {
            ReadinessLevel::Block => "✗",
            ReadinessLevel::Warn => "!",
            ReadinessLevel::Ok => "✓",
        };
        let store = match item.store {
            ReleaseStore::GooglePlay => "Google Play",
            ReleaseStore::AppStore => "App Store",
        };
        let remembered = if item.remembered {
            " (from the store's last refusal)"
        } else {
            ""
        };
        out.push_str(&format!("{mark} {store}: {}{remembered}\n", item.title));
        if let Some(detail) = item
            .detail
            .as_deref()
            .filter(|_| item.level != ReadinessLevel::Ok)
        {
            out.push_str(&format!("    {detail}\n"));
        }
        if let Some(url) = item
            .console_url
            .as_deref()
            .filter(|_| item.level != ReadinessLevel::Ok)
        {
            out.push_str(&format!("    Fix it: {url}\n"));
        }
    }
    out
}

#[derive(Debug, Args)]
pub(crate) struct BuildListArgs {
    /// Only this app's builds. Takes a name or an ID.
    #[arg(long, value_name = "APP")]
    project: Option<String>,

    /// Only this pipeline's builds. Takes a name or an ID.
    #[arg(long)]
    pipeline: Option<String>,

    /// Only builds of this branch.
    #[arg(long)]
    branch: Option<String>,

    /// Only builds with this status, such as failed or running.
    #[arg(long)]
    status: Option<String>,

    /// Only builds of this `.oore.yaml` workflow.
    #[arg(long)]
    workflow: Option<String>,

    /// Only builds of this commit, by full SHA or prefix.
    #[arg(long, value_name = "SHA")]
    commit: Option<String>,

    /// Only builds you started.
    #[arg(long)]
    mine: bool,

    /// How many builds to list, newest first.
    #[arg(long, default_value = "20", value_parser = clap::value_parser!(u64).range(1..=200))]
    limit: u64,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the builds as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct BuildIdArgs {
    /// The build.
    build_id: String,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's answer as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct BuildRetryStepArgs {
    /// The build whose step to run again.
    build_id: String,

    /// Step to run again, such as publish-play or publish-app-store.
    #[arg(long)]
    step: String,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the new build as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum InstallPlatform {
    Android,
    Ios,
}

#[derive(Debug, Args)]
pub(crate) struct BuildInstallLinkArgs {
    /// The build to install.
    build_id: String,

    /// Which app to link to when the build made both.
    #[arg(long, value_enum)]
    platform: Option<InstallPlatform>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the links as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ArtifactArgs {
    #[command(subcommand)]
    pub(crate) command: ArtifactSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum ArtifactSubcommand {
    /// Download one build file, by the ID `oore build artifacts` prints.
    Download(ArtifactDownloadArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ArtifactDownloadArgs {
    /// The artifact to download.
    artifact_id: String,

    /// Where to write the file. `-` writes it to standard output.
    #[arg(long, short = 'o', value_name = "PATH")]
    output: std::path::PathBuf,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print where the file went as JSON.
    #[arg(long)]
    json: bool,
}

fn short_sha(sha: Option<&str>) -> String {
    sha.map(|sha| sha.chars().take(8).collect())
        .unwrap_or_default()
}

pub(crate) async fn handle_build_list(args: BuildListArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let mut query: Vec<(&str, String)> = vec![("limit", args.limit.to_string())];
    let project = match args.project.as_deref() {
        Some(project) => Some(resolve_project(&api, project).await?),
        None => None,
    };
    if let Some(project) = &project {
        query.push(("project_id", project.id.clone()));
    }
    if let Some(pipeline) = args.pipeline.as_deref() {
        let pipeline =
            resolve_pipeline(&api, pipeline, project.as_ref().map(|p| p.id.as_str())).await?;
        query.push(("pipeline_id", pipeline.id));
    }
    for (key, value) in [
        ("branch", &args.branch),
        ("status", &args.status),
        ("workflow", &args.workflow),
        ("commit_sha", &args.commit),
    ] {
        if let Some(value) = value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            query.push((key, value.to_string()));
        }
    }
    if args.mine {
        query.push(("mine", "true".to_string()));
    }
    let mut response: ListBuildsResponse = api
        .send(
            api.client.get(api.url("/v1/builds")).query(&query),
            "build listing",
        )
        .await?;
    // A daemon without `commit_sha` ignores it; filter here too.
    if let Some(commit) = args
        .commit
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        let commit = commit.to_ascii_lowercase();
        response.builds.retain(|build| {
            build
                .commit_sha
                .as_deref()
                .is_some_and(|sha| sha.to_ascii_lowercase().starts_with(&commit))
        });
    }
    if args.json {
        return print_json(&response);
    }
    if response.builds.is_empty() {
        println!("No builds.");
        return Ok(());
    }
    let rows: Vec<Vec<String>> = response
        .builds
        .iter()
        .map(|build| {
            vec![
                build.id.clone(),
                format!("#{}", build.build_number),
                build.status.replace('_', " "),
                build
                    .context
                    .as_ref()
                    .and_then(|context| context.pipeline_name.clone())
                    .unwrap_or_default(),
                build.branch.clone().unwrap_or_default(),
                short_sha(build.commit_sha.as_deref()),
                ago(build.created_at),
            ]
        })
        .collect();
    print!(
        "{}",
        render_table(
            &[
                "ID", "BUILD", "STATUS", "PIPELINE", "BRANCH", "COMMIT", "CREATED"
            ],
            &rows
        )
    );
    Ok(())
}

pub(crate) async fn handle_build_show(args: BuildIdArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let path = api.url_with(&["v1", "builds", args.build_id.trim()])?;
    let detail: BuildDetailResponse = api.send(api.client.get(path), "build lookup").await?;
    if args.json {
        return print_json(&detail);
    }
    print!("{}", render_build(&detail));
    Ok(())
}

fn render_build(detail: &BuildDetailResponse) -> String {
    let build = &detail.build;
    let mut out = format!("{}\n", watch_line(build));
    let mut field = |label: &str, value: Option<String>| {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            out.push_str(&format!("  {label:<9} {value}\n"));
        }
    };
    let context = build.context.as_ref();
    field("ID", Some(build.id.clone()));
    field("App", context.and_then(|c| c.project_name.clone()));
    field("Pipeline", context.and_then(|c| c.pipeline_name.clone()));
    field("Workflow", build.workflow.clone());
    field("Branch", build.branch.clone());
    field(
        "Commit",
        build
            .commit_sha
            .as_deref()
            .map(|sha| match &build.commit_title {
                Some(title) => format!("{} {title}", short_sha(Some(sha))),
                None => short_sha(Some(sha)),
            }),
    );
    field(
        "Version",
        build
            .version_name
            .as_deref()
            .map(|name| match build.version_code {
                Some(code) => format!("{name} ({code})"),
                None => name.to_string(),
            }),
    );
    field("Started by", build.trigger_actor.clone());
    field("Runner", context.and_then(|c| c.runner_name.clone()));
    field("Created", Some(ago(build.created_at)));
    if let Some(steps) = build
        .step_results
        .as_ref()
        .filter(|steps| !steps.is_empty())
    {
        out.push_str("\nSteps:\n");
        for step in steps {
            let seconds = step.duration_ms / 1000;
            let code = step
                .exit_code
                .filter(|code| *code != 0)
                .map(|code| format!(" (exit {code})"))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {:<9} {}  {seconds}s{code}\n",
                step.status, step.name
            ));
        }
    }
    if build.status == "failed" {
        out.push_str(&format!(
            "\nRun `oore build why {}` to see why.\n",
            build.id
        ));
    }
    out
}

pub(crate) async fn handle_build_cancel(args: BuildIdArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let path = api.url_with(&["v1", "builds", args.build_id.trim(), "cancel"])?;
    let response: CancelBuildResponse = api.send(api.client.post(path), "cancel").await?;
    if args.json {
        return print_json(&response.build);
    }
    println!("{}", watch_line(&response.build));
    Ok(())
}

pub(crate) async fn handle_build_rerun(args: BuildIdArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let path = api.url_with(&["v1", "builds", args.build_id.trim(), "rerun"])?;
    let response: RerunBuildResponse = api.send(api.client.post(path), "re-run").await?;
    if args.json {
        return print_json(&response.build);
    }
    println!(
        "Queued build #{} as a re-run\nBuild id: {}",
        response.build.build_number, response.build.id
    );
    Ok(())
}

pub(crate) async fn handle_build_retry_step(args: BuildRetryStepArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let path = api.url_with(&["v1", "builds", args.build_id.trim(), "retry-step"])?;
    let request = RetryBuildStepRequest {
        step: args.step.trim().to_string(),
    };
    let response: RerunBuildResponse = api.post_url(path, &request, "step retry").await?;
    if args.json {
        return print_json(&response.build);
    }
    println!(
        "Queued build #{} to run {} again\nBuild id: {}",
        response.build.build_number, request.step, response.build.id
    );
    Ok(())
}

async fn build_artifacts(api: &Api, build_id: &str) -> anyhow::Result<Vec<Artifact>> {
    let path = api.url_with(&["v1", "builds", build_id, "artifacts"])?;
    let response: ListArtifactsResponse =
        api.send(api.client.get(path), "artifact listing").await?;
    Ok(response.artifacts)
}

fn human_size(bytes: Option<i64>) -> String {
    let Some(bytes) = bytes else {
        return String::new();
    };
    let bytes = bytes.max(0) as f64;
    match bytes {
        b if b >= 1024.0 * 1024.0 * 1024.0 => format!("{:.1} GB", b / 1024.0 / 1024.0 / 1024.0),
        b if b >= 1024.0 * 1024.0 => format!("{:.1} MB", b / 1024.0 / 1024.0),
        b if b >= 1024.0 => format!("{:.0} KB", b / 1024.0),
        b => format!("{b} B"),
    }
}

pub(crate) async fn handle_build_artifacts(args: BuildIdArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let artifacts = build_artifacts(&api, args.build_id.trim()).await?;
    if args.json {
        return print_json(&serde_json::json!({ "artifacts": artifacts }));
    }
    if artifacts.is_empty() {
        println!("This build has no files.");
        return Ok(());
    }
    let rows: Vec<Vec<String>> = artifacts
        .iter()
        .map(|artifact| {
            vec![
                artifact.id.clone(),
                artifact.name.clone(),
                artifact.artifact_type.clone(),
                human_size(artifact.file_size),
                artifact.state.clone(),
            ]
        })
        .collect();
    print!(
        "{}",
        render_table(&["ID", "NAME", "TYPE", "SIZE", "STATE"], &rows)
    );
    Ok(())
}

/// Whether a phone of `platform` (any when `None`) installs this file.
fn installs_on(artifact: &Artifact, platform: Option<InstallPlatform>) -> bool {
    matches!(
        (platform, artifact.artifact_type.as_str()),
        (None | Some(InstallPlatform::Android), "apk") | (None | Some(InstallPlatform::Ios), "ipa")
    )
}

/// The artifacts an install link can point at, for the platform asked.
fn installable(artifacts: &[Artifact], platform: Option<InstallPlatform>) -> Vec<&Artifact> {
    artifacts
        .iter()
        .filter(|artifact| artifact.state == "available")
        .filter(|artifact| installs_on(artifact, platform))
        .collect()
}

pub(crate) async fn handle_build_install_link(args: BuildInstallLinkArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let artifacts = build_artifacts(&api, args.build_id.trim()).await?;
    let mut candidates = installable(&artifacts, args.platform);
    if candidates.is_empty() {
        // None is marked available yet; let the daemon say why.
        candidates = artifacts
            .iter()
            .filter(|artifact| installs_on(artifact, args.platform))
            .collect();
    }
    if candidates.is_empty() {
        return Err(
            CliError::not_found("this build has no app to install (an APK or IPA)")
                .with_hint("See its files with `oore build artifacts`.")
                .into(),
        );
    }
    let mut links = Vec::new();
    for artifact in candidates {
        let path = api.url_with(&["v1", "artifacts", &artifact.id, "install-link"])?;
        let link: ArtifactInstallLinkResponse =
            api.send(api.client.post(path), "install link").await?;
        links.push(serde_json::json!({
            "artifact_id": artifact.id,
            "name": artifact.name,
            "platform": link.platform,
            "install_url": link.install_url,
            "download_url": link.download_url,
            "manifest_url": link.manifest_url,
            "expires_at": link.expires_at,
        }));
    }
    if args.json {
        return print_json(&serde_json::json!({ "links": links }));
    }
    for link in &links {
        println!(
            "{} ({}): {}",
            link["name"].as_str().unwrap_or_default(),
            link["platform"].as_str().unwrap_or_default(),
            link["install_url"].as_str().unwrap_or_default()
        );
    }
    Ok(())
}

pub(crate) async fn handle_artifact_download(args: ArtifactDownloadArgs) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;
    let api = Api::connect(&args.connection)?;
    let path = api.url_with(&["v1", "artifacts", args.artifact_id.trim(), "download-link"])?;
    let link: ArtifactDownloadLinkResponse =
        api.send(api.client.post(path), "download link").await?;
    let base = url::Url::parse(&format!("{}/", api.daemon_url))?;
    let url = base
        .join(&link.download_url)
        .context("the daemon gave a download link that is not a URL")?;
    // The link carries its own short-lived token. Send nothing else to a
    // host that is not the daemon.
    let same_host = url.origin() == base.origin();
    let client = if same_host {
        api.client.clone()
    } else {
        reqwest::Client::new()
    };
    let mut response = client
        .get(url.clone())
        .send()
        .await
        .with_context(|| format!("failed to download {url}"))?;
    if !response.status().is_success() {
        return Err(crate::build_commands::api_failure(response, "download")
            .await
            .into());
    }
    let to_stdout = args.output.as_os_str() == "-";
    let mut written: u64 = 0;
    if to_stdout {
        let mut stdout = tokio::io::stdout();
        while let Some(bytes) = response.chunk().await.context("the download was cut")? {
            stdout.write_all(&bytes).await?;
        }
        stdout.flush().await?;
        return Ok(());
    }
    let staged = args.output.with_extension("oore-download");
    let mut file = tokio::fs::File::create(&staged)
        .await
        .with_context(|| format!("failed to create {}", staged.display()))?;
    while let Some(bytes) = response.chunk().await.context("the download was cut")? {
        file.write_all(&bytes).await?;
        written += bytes.len() as u64;
    }
    file.flush().await?;
    drop(file);
    tokio::fs::rename(&staged, &args.output)
        .await
        .with_context(|| format!("failed to write {}", args.output.display()))?;
    if args.json {
        return print_json(&serde_json::json!({
            "artifact_id": args.artifact_id.trim(),
            "path": args.output,
            "bytes": written,
        }));
    }
    println!(
        "Saved {} ({})",
        args.output.display(),
        human_size(i64::try_from(written).ok())
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_lists_problems_first() {
        use oore_contract::{PublishPreflight, StoreReadinessItem};
        let item = |code: &str, level, title: &str| StoreReadinessItem {
            store: ReleaseStore::AppStore,
            code: code.into(),
            level,
            title: title.into(),
            detail: Some("Answer them in App Store Connect.".into()),
            console_url: Some("https://appstoreconnect.apple.com/apps/1".into()),
            remembered: false,
        };
        let response = StoreReadinessResponse {
            pipeline_id: "pl1".into(),
            items: vec![
                item("api_key", ReadinessLevel::Ok, "API key works"),
                item(
                    "age_rating",
                    ReadinessLevel::Block,
                    "Age rating questions are unanswered",
                ),
            ],
            preflight: PublishPreflight::Auto,
            checked_at: 0,
        };
        let text = render_readiness(&response);
        assert!(text.starts_with("1 thing to fix"));
        let blocking = text.find("✗ App Store: Age rating").unwrap();
        let ok = text.find("✓ App Store: API key works").unwrap();
        assert!(blocking < ok);
        assert!(text.contains("Fix it: https://appstoreconnect.apple.com/apps/1"));
    }

    fn watched(status: &str, outcome: Option<BuildReleaseOutcome>) -> Build {
        let mut build: Build = serde_json::from_value(serde_json::json!({
            "id": "b1",
            "project_id": "p1",
            "pipeline_id": "pl1",
            "build_number": 12,
            "status": status,
            "trigger_type": "manual",
            "queued_at": 0,
            "created_at": 0,
            "updated_at": 0,
            "config_snapshot": {},
        }))
        .unwrap();
        build.release_outcome = outcome;
        build
    }

    #[test]
    fn watch_follows_the_build_then_the_store_when_asked() {
        use BuildReleaseOutcome::*;
        let running = watched("running", None);
        assert_eq!(watch_line(&running), "Build #12 running");
        assert!(!watch_done(&running, false));

        let in_review = watched("succeeded", Some(InReview));
        assert_eq!(
            watch_line(&in_review),
            "Build #12 succeeded, in store review"
        );
        assert!(watch_done(&in_review, false));
        assert!(!watch_done(&in_review, true));
        assert_eq!(watch_exit_code(&in_review, false), 0);

        let live = watched("succeeded", Some(Live));
        assert!(watch_done(&live, true));
        assert_eq!(watch_exit_code(&live, true), 0);

        let rejected = watched("succeeded", Some(Rejected));
        assert!(watch_done(&rejected, true));
        assert_eq!(watch_exit_code(&rejected, true), 3);
        assert_eq!(watch_exit_code(&rejected, false), 0);

        let timed_out = watched("timed_out", None);
        assert_eq!(watch_line(&timed_out), "Build #12 timed out");
        assert!(watch_done(&timed_out, true));
        assert_eq!(watch_exit_code(&timed_out, true), 1);

        // A build with no store release has nothing to wait for.
        assert!(watch_done(&watched("succeeded", None), true));
    }

    fn run_args(argv: &[&str]) -> BuildRunArgs {
        #[derive(clap::Parser)]
        struct Wrapper {
            #[command(flatten)]
            args: BuildRunArgs,
        }
        use clap::Parser;
        Wrapper::parse_from(std::iter::once("oore").chain(argv.iter().copied())).args
    }

    #[test]
    fn build_run_sends_the_named_workflow_and_platforms() {
        let args = run_args(&[
            "--pipeline",
            " pipe-1 ",
            "--workflow",
            "release",
            "--branch",
            "main",
            "--platform",
            "iOS",
            "--platform",
            "android",
        ]);
        let request = create_build_request(&args).unwrap();
        assert_eq!(request.pipeline_id, "pipe-1");
        assert_eq!(request.workflow.as_deref(), Some("release"));
        assert_eq!(request.branch.as_deref(), Some("main"));
        assert_eq!(request.trigger_ref.as_deref(), Some("main"));
        assert_eq!(
            request.platforms,
            Some(vec![BuildPlatform::Ios, BuildPlatform::Android])
        );

        // Nothing named: the daemon picks the default workflow and branch.
        let request = create_build_request(&run_args(&["--pipeline", "pipe-1"])).unwrap();
        assert_eq!(request.workflow, None);
        assert_eq!(request.branch, None);
        assert_eq!(request.platforms, None);
        let body = serde_json::to_value(&request).unwrap();
        assert!(body.get("workflow").is_none());
        assert!(body.get("rehearsal").is_none());

        let request =
            create_build_request(&run_args(&["--pipeline", "pipe-1", "--rehearsal"])).unwrap();
        assert_eq!(request.rehearsal, Some(true));

        let error = create_build_request(&run_args(&[
            "--pipeline",
            "pipe-1",
            "--platform",
            "windows",
        ]))
        .unwrap_err();
        assert!(error.to_string().contains("unknown platform 'windows'"));
    }

    fn summary(name: &str, platforms: Vec<BuildPlatform>) -> PipelineWorkflowSummary {
        PipelineWorkflowSummary {
            name: name.to_string(),
            platforms,
            triggers: Vec::new(),
            uses_pipeline_triggers: true,
            publishes: Vec::new(),
            signing: Default::default(),
            errors: Vec::new(),
        }
    }

    fn listing(state: PipelineFileState) -> PipelineWorkflowsResponse {
        PipelineWorkflowsResponse {
            pipeline_id: "pipe-1".to_string(),
            reference: Some("main".to_string()),
            state,
            path: Some(".oore.yaml".to_string()),
            errors: Vec::new(),
            default_workflow: None,
            workflows: Vec::new(),
        }
    }

    #[test]
    fn workflows_print_as_a_table_with_the_default_marked() {
        let mut response = listing(PipelineFileState::Valid);
        let mut release = summary("release", vec![BuildPlatform::Android, BuildPlatform::Ios]);
        release.uses_pipeline_triggers = false;
        release.triggers = vec!["push to main".to_string(), "tag v*".to_string()];
        release.publishes =
            vec!["ios publishes to the TestFlight group QA (.oore.yaml)".to_string()];
        response.workflows = vec![summary("adhoc", vec![BuildPlatform::Android]), release];
        response.default_workflow = Some("release".to_string());

        let text = render_workflows(&response);
        assert_eq!(
            text,
            ".oore.yaml on main\n\n\
             WORKFLOW           PLATFORMS     STARTS ON\n\
             adhoc              android       pipeline trigger settings\n\
             release (default)  android, ios  push to main; tag v*\n\
             \nrelease: ios publishes to the TestFlight group QA (.oore.yaml)\n"
        );

        response.default_workflow = None;
        assert!(render_workflows(&response).contains("No default workflow"));
    }

    #[test]
    fn workflows_say_why_there_is_nothing_to_pick() {
        assert_eq!(
            render_workflows(&listing(PipelineFileState::Missing)),
            "No pipeline file on main; builds use the pipeline's settings.\n"
        );
        let mut invalid = listing(PipelineFileState::Invalid);
        invalid.errors = vec!["workflows.release: unknown key".to_string()];
        assert_eq!(
            render_workflows(&invalid),
            ".oore.yaml on main has errors:\n  workflows.release: unknown key\n"
        );
        let mut stale = listing(PipelineFileState::Valid);
        stale.workflows = vec![summary("adhoc", vec![BuildPlatform::Android])];
        stale.default_workflow = Some("release".to_string());
        assert!(
            render_workflows(&stale)
                .ends_with("The default workflow release is not in .oore.yaml on main.\n")
        );
    }
}
