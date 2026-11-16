//! `oore project list` and `oore pipeline list`, and the resolver that lets
//! every `--project` and `--pipeline` take a name as well as an ID.

use clap::Args;
use oore_contract::{
    ListPipelinesResponse, ListProjectsResponse, Pipeline, PipelineDetailResponse, Project,
    ProjectDetailResponse,
};

use crate::build_commands::{Api, ApiConnectionArgs};
use crate::cli_contract::{CliError, exit, print_json};

/// Page size for the lists the resolver walks.
const PAGE: usize = 100;

#[derive(Debug, Args)]
pub(crate) struct ProjectListArgs {
    /// Only apps whose name or repository contains this text.
    #[arg(long)]
    search: Option<String>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the apps as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PipelineListArgs {
    /// Only this app's pipelines. Takes a name or an ID.
    #[arg(long, value_name = "APP")]
    project: Option<String>,

    /// Only pipelines whose name contains this text.
    #[arg(long)]
    search: Option<String>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the pipelines as JSON.
    #[arg(long)]
    json: bool,
}

/// A table with the ID in the first column, columns padded to fit.
pub(crate) fn render_table(header: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = header.iter().map(|cell| cell.chars().count()).collect();
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let line = |cells: Vec<&str>| {
        let mut text = String::new();
        for (index, cell) in cells.iter().enumerate() {
            if index + 1 == cells.len() {
                text.push_str(cell);
            } else {
                let pad = widths[index].saturating_sub(cell.chars().count());
                text.push_str(cell);
                text.push_str(&" ".repeat(pad + 2));
            }
        }
        text.trim_end().to_string() + "\n"
    };
    let mut out = line(header.to_vec());
    for row in rows {
        out.push_str(&line(row.iter().map(String::as_str).collect()));
    }
    out
}

/// "5m ago" for a Unix time in seconds.
pub(crate) fn ago(epoch: i64) -> String {
    let now = chrono::Utc::now().timestamp();
    let seconds = (now - epoch).max(0);
    match seconds {
        0..60 => format!("{seconds}s ago"),
        60..3600 => format!("{}m ago", seconds / 60),
        3600..86_400 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86_400),
    }
}

async fn list_projects(api: &Api, search: Option<&str>) -> anyhow::Result<Vec<Project>> {
    let mut projects = Vec::new();
    loop {
        let mut request = api.client.get(api.url("/v1/projects")).query(&[
            ("limit", PAGE.to_string()),
            ("offset", projects.len().to_string()),
        ]);
        if let Some(search) = search {
            request = request.query(&[("search", search)]);
        }
        let page: ListProjectsResponse = api.send(request, "app listing").await?;
        let done = page.projects.len() < PAGE
            || projects.len() + page.projects.len() >= usize::try_from(page.total).unwrap_or(0);
        projects.extend(page.projects);
        if done {
            return Ok(projects);
        }
    }
}

async fn list_project_pipelines(
    api: &Api,
    project_id: &str,
    search: Option<&str>,
) -> anyhow::Result<Vec<Pipeline>> {
    let path = api.url_with(&["v1", "projects", project_id, "pipelines"])?;
    let mut pipelines = Vec::new();
    loop {
        let mut request = api.client.get(&path).query(&[
            ("limit", PAGE.to_string()),
            ("offset", pipelines.len().to_string()),
        ]);
        if let Some(search) = search {
            request = request.query(&[("search", search)]);
        }
        let page: ListPipelinesResponse = api.send(request, "pipeline listing").await?;
        let done = page.pipelines.len() < PAGE
            || pipelines.len() + page.pipelines.len() >= usize::try_from(page.total).unwrap_or(0);
        pipelines.extend(page.pipelines);
        if done {
            return Ok(pipelines);
        }
    }
}

/// Every pipeline the caller can see, with its app, optionally narrowed by
/// a search. Uses `GET /v1/pipelines` when the daemon has it.
async fn list_every_pipeline(
    api: &Api,
    search: Option<&str>,
) -> anyhow::Result<Vec<(Pipeline, Option<String>)>> {
    let mut request = api.client.get(api.url("/v1/pipelines"));
    if let Some(search) = search {
        request = request.query(&[("search", search)]);
    }
    if let Some(page) = api
        .send_if_supported::<ListPipelinesResponse>(request, "pipeline listing")
        .await?
    {
        let names = list_projects(api, None)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|project| (project.id, project.name))
            .collect::<std::collections::HashMap<_, _>>();
        return Ok(page
            .pipelines
            .into_iter()
            .map(|pipeline| {
                let name = names.get(&pipeline.project_id).cloned();
                (pipeline, name)
            })
            .collect());
    }
    let mut all = Vec::new();
    for project in list_projects(api, None).await? {
        for pipeline in list_project_pipelines(api, &project.id, search).await? {
            all.push((pipeline, Some(project.name.clone())));
        }
    }
    Ok(all)
}

fn same_name(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

/// The one match, or an error that lists the matches with their IDs.
fn pick_one<T>(
    kind: &str,
    value: &str,
    mut matches: Vec<T>,
    describe: impl Fn(&T) -> String,
    narrow: &str,
    ambiguous_hint: &str,
) -> anyhow::Result<T> {
    match matches.len() {
        0 => Err(
            CliError::not_found(format!("no {kind} has the ID or name '{value}'"))
                .with_hint(format!("List them with `{narrow}`."))
                .into(),
        ),
        1 => Ok(matches.remove(0)),
        _ => {
            let listed = matches
                .iter()
                .map(|item| format!("  {}", describe(item)))
                .collect::<Vec<_>>()
                .join("\n");
            Err(CliError::new(
                exit::FAILED,
                "ambiguous_name",
                format!("{} {kind}s are named '{value}':\n{listed}", matches.len()),
            )
            .with_hint(ambiguous_hint)
            .into())
        }
    }
}

/// The app with this ID, or else with this name or repository.
pub(crate) async fn resolve_project(api: &Api, value: &str) -> anyhow::Result<Project> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CliError::usage("the app name or ID is empty").into());
    }
    let by_id = api.url_with(&["v1", "projects", value])?;
    if let Some(detail) = api
        .send_if_supported::<ProjectDetailResponse>(api.client.get(by_id), "app lookup")
        .await?
    {
        return Ok(detail.project);
    }
    let candidates = list_projects(api, Some(value)).await?;
    let mut matches: Vec<Project> = candidates
        .iter()
        .filter(|project| same_name(&project.name, value))
        .cloned()
        .collect();
    if matches.is_empty() {
        matches = candidates
            .into_iter()
            .filter(|project| {
                project.repository_full_name.as_deref().is_some_and(|repo| {
                    same_name(repo, value)
                        || repo
                            .rsplit_once('/')
                            .is_some_and(|(_, name)| same_name(name, value))
                })
            })
            .collect();
    }
    pick_one(
        "app",
        value,
        matches,
        |project| format!("{}  {}", project.id, project.name),
        "oore project list",
        "Pass the app's ID instead.",
    )
}

/// The pipeline with this ID, or else with this name, within `project`
/// when one is named.
pub(crate) async fn resolve_pipeline(
    api: &Api,
    value: &str,
    project: Option<&str>,
) -> anyhow::Result<Pipeline> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CliError::usage("the pipeline name or ID is empty").into());
    }
    let by_id = api.url_with(&["v1", "pipelines", value])?;
    if let Some(detail) = api
        .send_if_supported::<PipelineDetailResponse>(api.client.get(by_id), "pipeline lookup")
        .await?
    {
        return Ok(detail.pipeline);
    }
    let matches: Vec<(Pipeline, Option<String>)> = match project {
        Some(project) => {
            let project = resolve_project(api, project).await?;
            list_project_pipelines(api, &project.id, Some(value))
                .await?
                .into_iter()
                .map(|pipeline| (pipeline, Some(project.name.clone())))
                .collect()
        }
        None => list_every_pipeline(api, Some(value)).await?,
    };
    let matches = matches
        .into_iter()
        .filter(|(pipeline, _)| same_name(&pipeline.name, value))
        .collect();
    pick_one(
        "pipeline",
        value,
        matches,
        |(pipeline, project)| {
            format!(
                "{}  {} (app {})",
                pipeline.id,
                pipeline.name,
                project.as_deref().unwrap_or(&pipeline.project_id)
            )
        },
        "oore pipeline list",
        if project.is_none() {
            "Name the app with --project, or pass the pipeline's ID."
        } else {
            "Pass the pipeline's ID instead."
        },
    )
    .map(|(pipeline, _)| pipeline)
}

pub(crate) async fn handle_project_list(args: ProjectListArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let search = args
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let projects = list_projects(&api, search).await?;
    if args.json {
        return print_json(&serde_json::json!({
            "projects": projects,
            "total": projects.len(),
        }));
    }
    if projects.is_empty() {
        println!("No apps.");
        return Ok(());
    }
    let rows: Vec<Vec<String>> = projects
        .iter()
        .map(|project| {
            vec![
                project.id.clone(),
                project.name.clone(),
                project.repository_full_name.clone().unwrap_or_default(),
                project.last_build_at.map(ago).unwrap_or_default(),
            ]
        })
        .collect();
    print!(
        "{}",
        render_table(&["ID", "APP", "REPOSITORY", "LAST BUILD"], &rows)
    );
    Ok(())
}

pub(crate) async fn handle_pipeline_list(args: PipelineListArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let search = args
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let pipelines = match args.project.as_deref() {
        Some(project) => {
            let project = resolve_project(&api, project).await?;
            list_project_pipelines(&api, &project.id, search)
                .await?
                .into_iter()
                .map(|pipeline| (pipeline, Some(project.name.clone())))
                .collect()
        }
        None => list_every_pipeline(&api, search).await?,
    };
    if args.json {
        let pipelines: Vec<&Pipeline> = pipelines.iter().map(|(pipeline, _)| pipeline).collect();
        return print_json(&serde_json::json!({
            "pipelines": pipelines,
            "total": pipelines.len(),
        }));
    }
    if pipelines.is_empty() {
        println!("No pipelines.");
        return Ok(());
    }
    let rows: Vec<Vec<String>> = pipelines
        .iter()
        .map(|(pipeline, project)| {
            vec![
                pipeline.id.clone(),
                pipeline.name.clone(),
                project
                    .clone()
                    .unwrap_or_else(|| pipeline.project_id.clone()),
                pipeline.workflow.clone().unwrap_or_default(),
                if pipeline.enabled { "" } else { "disabled" }.to_string(),
            ]
        })
        .collect();
    print!(
        "{}",
        render_table(&["ID", "PIPELINE", "APP", "WORKFLOW", ""], &rows)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_pad_every_column_but_the_last() {
        let table = render_table(
            &["ID", "APP", "REPOSITORY"],
            &[
                vec!["p1".into(), "Coin".into(), "zerodha/coin".into()],
                vec!["p22".into(), "Kite".into(), String::new()],
            ],
        );
        assert_eq!(
            table,
            "ID   APP   REPOSITORY\np1   Coin  zerodha/coin\np22  Kite\n"
        );
    }

    #[test]
    fn a_name_must_match_exactly_one() {
        let one = pick_one(
            "app",
            "coin",
            vec!["a"],
            |s| s.to_string(),
            "oore project list",
            "",
        );
        assert_eq!(one.unwrap(), "a");

        let none = pick_one::<&str>(
            "app",
            "coin",
            vec![],
            |s| s.to_string(),
            "oore project list",
            "",
        )
        .unwrap_err();
        let failure = crate::cli_contract::describe_failure(&none);
        assert_eq!(failure.exit, exit::NOT_FOUND);
        assert!(failure.hint.unwrap().contains("oore project list"));

        let two = pick_one(
            "pipeline",
            "Release",
            vec!["pl1  Release (app Coin)", "pl2  Release (app Kite)"],
            |s| s.to_string(),
            "oore pipeline list",
            "Name the app with --project.",
        )
        .unwrap_err();
        let failure = crate::cli_contract::describe_failure(&two);
        assert_eq!(failure.code, "ambiguous_name");
        assert!(failure.message.contains("pl1  Release (app Coin)"));
        assert!(failure.message.contains("pl2  Release (app Kite)"));
        assert_eq!(
            failure.hint.as_deref(),
            Some("Name the app with --project.")
        );
    }

    #[test]
    fn ages_read_in_the_largest_unit() {
        let now = chrono::Utc::now().timestamp();
        assert_eq!(ago(now - 5), "5s ago");
        assert_eq!(ago(now - 125), "2m ago");
        assert_eq!(ago(now - 7300), "2h ago");
        assert_eq!(ago(now - 3 * 86_400), "3d ago");
    }
}
