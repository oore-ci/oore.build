//! `oore build run` and `oore pipeline workflows`: start a build of a
//! pipeline, naming its workflow, and list the workflows it can name.

use anyhow::Context;
use clap::Args;
use oore_contract::{
    BuildPlatform, CreateBuildRequest, CreateBuildResponse, PipelineDetailResponse,
    PipelineFileState, PipelineWorkflowSummary, PipelineWorkflowsResponse,
};

use crate::{
    endpoint_http_client_builder, extract_error_message, resolve_daemon_url, resolve_session_token,
};

/// Where to reach the daemon and how to sign in, shared by these commands.
#[derive(Debug, Args)]
pub(crate) struct ApiConnectionArgs {
    #[arg(long, env = "OORE_DAEMON_URL")]
    daemon_url: Option<String>,

    /// Session token. If omitted, falls back to OORE_SESSION_TOKEN or the
    /// session saved by `oore login`.
    #[arg(long, env = "OORE_SESSION_TOKEN")]
    token: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct BuildRunArgs {
    /// Pipeline to build.
    #[arg(long)]
    pipeline: String,

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

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the queued build as JSON.
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

/// An authenticated client for the daemon.
struct Api {
    client: reqwest::Client,
    daemon_url: String,
    token: String,
}

impl Api {
    fn connect(args: &ApiConnectionArgs) -> anyhow::Result<Self> {
        let daemon_url = resolve_daemon_url(args.daemon_url.as_deref())?;
        let token = resolve_session_token(args.token.as_deref(), &daemon_url)?
            .context("not signed in: run `oore login`, or pass --token or OORE_SESSION_TOKEN")?;
        let client = endpoint_http_client_builder(&daemon_url)
            .build()
            .context("failed to build the API client")?;
        Ok(Self {
            client,
            daemon_url: daemon_url.trim_end_matches('/').to_string(),
            token,
        })
    }

    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        what: &str,
    ) -> anyhow::Result<T> {
        let response = request
            .bearer_auth(&self.token)
            .send()
            .await
            .with_context(|| format!("failed to reach daemon at {}", self.daemon_url))?;
        let status = response.status();
        if status.is_success() {
            return response
                .json()
                .await
                .with_context(|| format!("failed to parse the {what} response"));
        }
        let message = extract_error_message(response).await;
        anyhow::bail!("{what} failed (HTTP {}): {message}", status.as_u16())
    }
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
        workflow: trimmed(args.workflow.as_deref()),
    })
}

pub(crate) async fn handle_build_run(args: BuildRunArgs) -> anyhow::Result<()> {
    let request = create_build_request(&args)?;
    let api = Api::connect(&args.connection)?;
    // Builds are created under the pipeline's project.
    let detail: PipelineDetailResponse = api
        .send(
            api.client.get(format!(
                "{}/v1/pipelines/{}",
                api.daemon_url, request.pipeline_id
            )),
            "pipeline lookup",
        )
        .await?;
    let created: CreateBuildResponse = api
        .send(
            api.client
                .post(format!(
                    "{}/v1/projects/{}/builds",
                    api.daemon_url, detail.pipeline.project_id
                ))
                .json(&request),
            "build request",
        )
        .await?;
    let build = created.build;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&build)?);
        return Ok(());
    }
    let mut line = format!(
        "Queued build #{} of {}",
        build.build_number, detail.pipeline.name
    );
    if let Some(workflow) = &build.workflow {
        line.push_str(&format!(", workflow {workflow}"));
    }
    if let Some(reference) = build.branch.as_deref().or(build.commit_sha.as_deref()) {
        line.push_str(&format!(", on {reference}"));
    }
    println!("{line}");
    println!("Build id: {}", build.id);
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

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
