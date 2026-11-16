//! `oore secret list|set|delete`: encrypted secrets for an instance, an app
//! or a pipeline. Values are read from standard input, a file or a variable,
//! never from an argument, and never printed.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use oore_contract::ListSecretsResponse;
use serde_json::json;

use crate::admin_api::{
    self, ApiFailure, SecretSource, api_url, call, confirm, decode, format_time, get, print_json,
};
use crate::build_commands::{Api, ApiConnectionArgs};

#[derive(Debug, Args)]
pub(crate) struct SecretArgs {
    #[command(subcommand)]
    pub(crate) command: SecretSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum SecretSubcommand {
    /// List secret names in a scope. Values are never shown.
    List(SecretListArgs),
    /// Create a secret, or replace its value if it exists. The value comes
    /// from standard input unless --file or --from-env says otherwise.
    Set(SecretSetArgs),
    /// Delete a secret. Builds that read it fail until it is set again.
    Delete(SecretDeleteArgs),
}

/// Which secrets: the instance's without a flag.
#[derive(Debug, Args)]
pub(crate) struct SecretScopeArgs {
    /// An app's secrets, shared by its pipelines.
    #[arg(long, value_name = "PROJECT_ID", conflicts_with = "pipeline")]
    project: Option<String>,

    /// One pipeline's secrets.
    #[arg(long, value_name = "PIPELINE_ID")]
    pipeline: Option<String>,
}

impl SecretScopeArgs {
    fn segments<'a>(&'a self, rest: &[&'a str]) -> Vec<&'a str> {
        let mut segments = match (&self.project, &self.pipeline) {
            (Some(project), _) => vec!["projects", project.as_str(), "secrets"],
            (None, Some(pipeline)) => vec!["pipelines", pipeline.as_str(), "secrets"],
            (None, None) => vec!["secrets"],
        };
        segments.extend_from_slice(rest);
        segments
    }

    fn label(&self) -> String {
        match (&self.project, &self.pipeline) {
            (Some(project), _) => format!("app {project}"),
            (None, Some(pipeline)) => format!("pipeline {pipeline}"),
            (None, None) => "this instance".to_string(),
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct SecretListArgs {
    #[command(flatten)]
    scope: SecretScopeArgs,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct SecretSetArgs {
    /// Secret name, as `${{ secrets.NAME }}` reads it.
    name: String,

    #[command(flatten)]
    scope: SecretScopeArgs,

    /// Read the value from a file.
    #[arg(long, value_name = "PATH", conflicts_with = "from_env")]
    file: Option<PathBuf>,

    /// Read the value from this environment variable.
    #[arg(long, value_name = "VAR")]
    from_env: Option<String>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the saved secret (without its value) as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct SecretDeleteArgs {
    /// Secret name.
    name: String,

    #[command(flatten)]
    scope: SecretScopeArgs,

    /// Delete without asking. Required when no terminal is attached.
    #[arg(long, short = 'y', default_value = "false")]
    yes: bool,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

pub(crate) async fn handle(args: SecretArgs) -> anyhow::Result<()> {
    match args.command {
        SecretSubcommand::List(args) => list(args).await,
        SecretSubcommand::Set(args) => set(args).await,
        SecretSubcommand::Delete(args) => delete(args).await,
    }
}

async fn list(args: SecretListArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let response = get(
        &api,
        api_url(&api, &args.scope.segments(&[]))?,
        "secret list",
    )
    .await?;
    if args.json {
        return print_json(&response);
    }
    let list: ListSecretsResponse = decode(&response, "secret list")?;
    if list.secrets.is_empty() {
        println!("No secrets for {}.", args.scope.label());
        return Ok(());
    }
    let rows = list
        .secrets
        .iter()
        .map(|secret| {
            vec![
                secret.name.clone(),
                serde_json::to_value(secret.scope)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_default(),
                format_time(Some(secret.updated_at)),
            ]
        })
        .collect::<Vec<_>>();
    admin_api::print_table(&["NAME", "SCOPE", "UPDATED"], &rows);
    Ok(())
}

async fn set(args: SecretSetArgs) -> anyhow::Result<()> {
    let name = args.name.trim();
    anyhow::ensure!(!name.is_empty(), "the secret name must not be empty");
    let source = match (&args.file, &args.from_env) {
        (Some(path), _) => SecretSource::File(path),
        (None, Some(var)) => SecretSource::Env(var),
        (None, None) => SecretSource::Stdin,
    };
    let value = admin_api::read_secret(source, &format!("Value for {name}"))?;
    let api = Api::connect(&args.connection)?;
    let created = call(
        &api,
        reqwest::Method::POST,
        api_url(&api, &args.scope.segments(&[]))?,
        Some(&json!({ "name": name, "value": value })),
        "secret create",
    )
    .await;
    let (saved, verb) = match created {
        Ok(saved) => (saved, "Created"),
        Err(error)
            if error
                .downcast_ref::<ApiFailure>()
                .is_some_and(|failure| failure.status == 409) =>
        {
            let rotated = call(
                &api,
                reqwest::Method::PUT,
                api_url(&api, &args.scope.segments(&[name]))?,
                Some(&json!({ "value": value })),
                "secret update",
            )
            .await?;
            (rotated, "Updated")
        }
        Err(error) => return Err(error),
    };
    if args.json {
        return print_json(&saved);
    }
    println!("{verb} secret {name} for {}.", args.scope.label());
    Ok(())
}

async fn delete(args: SecretDeleteArgs) -> anyhow::Result<()> {
    let name = args.name.trim();
    confirm(
        args.yes,
        &format!("Delete secret {name} from {}?", args.scope.label()),
    )?;
    let api = Api::connect(&args.connection)?;
    let response = call(
        &api,
        reqwest::Method::DELETE,
        api_url(&api, &args.scope.segments(&[name]))?,
        None,
        "secret delete",
    )
    .await?;
    if args.json {
        return print_json(&response);
    }
    println!("Deleted secret {name} from {}.", args.scope.label());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scope_picks_the_route() {
        let instance = SecretScopeArgs {
            project: None,
            pipeline: None,
        };
        assert_eq!(instance.segments(&["KEY"]), vec!["secrets", "KEY"]);
        let app = SecretScopeArgs {
            project: Some("p1".into()),
            pipeline: None,
        };
        assert_eq!(app.segments(&[]), vec!["projects", "p1", "secrets"]);
        let pipeline = SecretScopeArgs {
            project: None,
            pipeline: Some("pl".into()),
        };
        assert_eq!(pipeline.segments(&[]), vec!["pipelines", "pl", "secrets"]);
    }
}
