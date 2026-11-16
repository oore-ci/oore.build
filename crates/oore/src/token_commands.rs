//! `oore token create|list|revoke`: API tokens for agents and scripts,
//! without the web UI. On a Local Only host, `oore login` then
//! `oore token create` sets up an agent with no browser.

use clap::{Args, Subcommand, ValueEnum};
use oore_contract::{CreateApiTokenResponse, ListApiTokensResponse};
use serde_json::json;

use crate::admin_api::{self, api_url, call, confirm, decode, format_time, get, print_json};
use crate::build_commands::{Api, ApiConnectionArgs};

/// How long a token lives when `--expires` is not given.
const DEFAULT_EXPIRY: &str = "90d";

#[derive(Debug, Args)]
pub(crate) struct TokenArgs {
    #[command(subcommand)]
    pub(crate) command: TokenSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TokenSubcommand {
    /// Create an API token. It is printed once and never again.
    Create(TokenCreateArgs),
    /// List the API tokens you can see.
    List(TokenListArgs),
    /// Revoke an API token. Anything using it stops working at once.
    Revoke(TokenRevokeArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum TokenRole {
    Owner,
    Admin,
    /// Can build, read and share. The least role that can start builds.
    Developer,
    /// Can read builds and install them.
    QaViewer,
}

impl TokenRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Developer => "developer",
            Self::QaViewer => "qa_viewer",
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct TokenCreateArgs {
    /// A name that says who or what uses the token, such as "release agent".
    #[arg(long)]
    name: String,

    /// The token's role. It can never do more than you can.
    #[arg(long, value_enum, default_value = "developer")]
    role: TokenRole,

    /// How long the token lives, such as 30d or 12h, or `never`.
    #[arg(long, default_value = DEFAULT_EXPIRY, value_name = "DURATION")]
    expires: String,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the new token and its details as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct TokenListArgs {
    /// Only tokens whose name contains this.
    #[arg(long)]
    search: Option<String>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct TokenRevokeArgs {
    /// ID of the token to revoke, from `oore token list`.
    token_id: String,

    /// Revoke without asking. Required when no terminal is attached.
    #[arg(long, short = 'y', default_value = "false")]
    yes: bool,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

/// The token's expiry as Unix seconds, or `None` for `never`.
fn expires_at(value: &str, now: i64) -> anyhow::Result<Option<i64>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("never") {
        return Ok(None);
    }
    let duration = humantime::parse_duration(value).map_err(|error| {
        admin_api::UsageError(format!(
            "--expires must be a duration such as 90d or 12h, or never: {error}"
        ))
    })?;
    anyhow::ensure!(duration.as_secs() > 0, "--expires must be longer than zero");
    let seconds = i64::try_from(duration.as_secs()).unwrap_or(i64::MAX);
    Ok(Some(now.saturating_add(seconds)))
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

pub(crate) async fn handle(args: TokenArgs) -> anyhow::Result<()> {
    match args.command {
        TokenSubcommand::Create(args) => create(args).await,
        TokenSubcommand::List(args) => list(args).await,
        TokenSubcommand::Revoke(args) => revoke(args).await,
    }
}

async fn create(args: TokenCreateArgs) -> anyhow::Result<()> {
    let name = args.name.trim();
    anyhow::ensure!(!name.is_empty(), "--name must not be empty");
    let mut body = json!({ "name": name, "role": args.role.as_str() });
    if let Some(at) = expires_at(&args.expires, now())? {
        body["expires_at"] = json!(at);
    }
    let api = Api::connect(&args.connection)?;
    let created = call(
        &api,
        reqwest::Method::POST,
        api_url(&api, &["api-tokens"])?,
        Some(&body),
        "token create",
    )
    .await?;
    if args.json {
        return print_json(&created);
    }
    let token: CreateApiTokenResponse = decode(&created, "token create")?;
    eprintln!(
        "Created token \"{}\" ({}), role {}, expires {}.",
        token.name,
        token.id,
        token.role,
        token
            .expires_at
            .map(|at| format_time(Some(at)))
            .unwrap_or_else(|| "never".to_string())
    );
    eprintln!("Copy it now. Oore shows it only once. Agents read it from OORE_TOKEN.");
    // Only the token goes to stdout, so `$(oore token create ...)` captures it.
    println!("{}", token.token);
    Ok(())
}

async fn list(args: TokenListArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let mut url = api_url(&api, &["api-tokens"])?;
    url.query_pairs_mut().append_pair("limit", "200");
    if let Some(search) = args
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        url.query_pairs_mut().append_pair("q", search);
    }
    let response = get(&api, url, "token list").await?;
    if args.json {
        return print_json(&response);
    }
    let list: ListApiTokensResponse = decode(&response, "token list")?;
    if list.tokens.is_empty() {
        println!("No API tokens. Create one with `oore token create --name <name>`.");
        return Ok(());
    }
    let rows = list
        .tokens
        .iter()
        .map(|token| {
            let state = if token.is_revoked {
                "revoked"
            } else if token.is_expired {
                "expired"
            } else {
                "active"
            };
            vec![
                token.id.clone(),
                token.name.clone(),
                token.role.clone(),
                state.to_string(),
                token
                    .expires_at
                    .map(|at| format_time(Some(at)))
                    .unwrap_or_else(|| "never".to_string()),
                format_time(token.last_used_at),
                token.created_by_email.clone(),
            ]
        })
        .collect::<Vec<_>>();
    admin_api::print_table(
        &[
            "ID",
            "NAME",
            "ROLE",
            "STATE",
            "EXPIRES",
            "LAST USED",
            "CREATED BY",
        ],
        &rows,
    );
    Ok(())
}

async fn revoke(args: TokenRevokeArgs) -> anyhow::Result<()> {
    let id = args.token_id.trim();
    confirm(
        args.yes,
        &format!("Revoke token {id}? Anything using it stops working."),
    )?;
    let api = Api::connect(&args.connection)?;
    let response = call(
        &api,
        reqwest::Method::DELETE,
        api_url(&api, &["api-tokens", id])?,
        None,
        "token revoke",
    )
    .await?;
    if args.json {
        return print_json(&response);
    }
    println!("Revoked token {id}.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_defaults_to_ninety_days_and_never_means_none() {
        assert_eq!(
            expires_at(DEFAULT_EXPIRY, 1_000).unwrap(),
            Some(1_000 + 90 * 86_400)
        );
        assert_eq!(expires_at("12h", 0).unwrap(), Some(12 * 3_600));
        assert_eq!(expires_at("Never", 0).unwrap(), None);
        assert!(expires_at("soon", 0).is_err());
    }
}
