//! `oore tester group list|create|update|delete`, `oore build share` and
//! `oore runner list`: who gets builds to test, sending a build to them,
//! and which runners can take builds.

use clap::{Args, Subcommand};
use oore_contract::{BuildShare, ListRunnersResponse, ListTesterGroupsResponse, TesterGroup, User};
use serde_json::{Value, json};

use crate::admin_api::{self, api_url, call, confirm, decode, format_time, get, print_json};
use crate::build_commands::{Api, ApiConnectionArgs};

#[derive(Debug, Args)]
pub(crate) struct TesterArgs {
    #[command(subcommand)]
    pub(crate) command: TesterSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TesterSubcommand {
    /// Manage tester groups: named lists of people who get builds to test.
    Group(TesterGroupArgs),
}

#[derive(Debug, Args)]
pub(crate) struct TesterGroupArgs {
    #[command(subcommand)]
    command: TesterGroupSubcommand,
}

#[derive(Debug, Subcommand)]
enum TesterGroupSubcommand {
    /// List tester groups and their members.
    List(GroupListArgs),
    /// Create a tester group.
    Create(GroupCreateArgs),
    /// Rename a group, or add and remove members.
    Update(GroupUpdateArgs),
    /// Delete a tester group. Builds already sent stay installable.
    Delete(GroupDeleteArgs),
}

#[derive(Debug, Args)]
struct GroupListArgs {
    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
struct GroupCreateArgs {
    /// Group name, such as "QA" or "Leadership".
    #[arg(long)]
    name: String,

    /// A member, by email or user ID. Repeat for several.
    #[arg(long = "member", value_name = "EMAIL")]
    members: Vec<String>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the saved group as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
struct GroupUpdateArgs {
    /// The group, by name or ID.
    group: String,

    /// New name.
    #[arg(long)]
    name: Option<String>,

    /// Add a member, by email or user ID. Repeat for several.
    #[arg(long = "add", value_name = "EMAIL")]
    add: Vec<String>,

    /// Remove a member, by email or user ID. Repeat for several.
    #[arg(long = "remove", value_name = "EMAIL")]
    remove: Vec<String>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the saved group as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
struct GroupDeleteArgs {
    /// The group, by name or ID.
    group: String,

    /// Delete without asking. Required when no terminal is attached.
    #[arg(long, short = 'y', default_value = "false")]
    yes: bool,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print `{"deleted": true}` as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct BuildShareArgs {
    /// Build to send.
    build_id: String,

    /// Tester group to send it to, by name or ID.
    #[arg(long)]
    group: String,

    /// A note for the testers, added to the email.
    #[arg(long)]
    message: Option<String>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the share as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RunnerListArgs {
    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

pub(crate) async fn handle(args: TesterArgs) -> anyhow::Result<()> {
    let TesterSubcommand::Group(group) = args.command;
    match group.command {
        TesterGroupSubcommand::List(args) => list_groups(args).await,
        TesterGroupSubcommand::Create(args) => create_group(args).await,
        TesterGroupSubcommand::Update(args) => update_group(args).await,
        TesterGroupSubcommand::Delete(args) => delete_group(args).await,
    }
}

async fn fetch_groups(api: &Api) -> anyhow::Result<(Value, Vec<TesterGroup>)> {
    let response = get(api, api_url(api, &["tester-groups"])?, "tester group list").await?;
    let list: ListTesterGroupsResponse = decode(&response, "tester group list")?;
    Ok((response, list.groups))
}

/// The group with this ID, else the one with this name (ignoring case).
fn find_group<'a>(groups: &'a [TesterGroup], wanted: &str) -> anyhow::Result<&'a TesterGroup> {
    let wanted = wanted.trim();
    groups
        .iter()
        .find(|group| group.id == wanted)
        .or_else(|| {
            groups
                .iter()
                .find(|group| group.name.eq_ignore_ascii_case(wanted))
        })
        .ok_or_else(|| {
            let names = groups
                .iter()
                .map(|group| group.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::Error::from(admin_api::ApiFailure {
                what: "tester group lookup".to_string(),
                status: 404,
                code: Some("not_found".to_string()),
                message: format!("no tester group named {wanted}"),
                hint: (!names.is_empty()).then(|| format!("Groups: {names}")),
            })
        })
}

/// User IDs for members given by email or ID.
async fn resolve_users(api: &Api, members: &[String]) -> anyhow::Result<Vec<String>> {
    let mut ids = Vec::with_capacity(members.len());
    for member in members {
        let member = member.trim();
        if !member.contains('@') {
            ids.push(member.to_string());
            continue;
        }
        let mut url = api_url(api, &["users"])?;
        url.query_pairs_mut()
            .append_pair("q", member)
            .append_pair("limit", "100");
        let response = get(api, url, "user lookup").await?;
        let users: Vec<User> = decode(&response["users"], "user lookup")?;
        let user = users
            .into_iter()
            .find(|user| user.email.eq_ignore_ascii_case(member))
            .ok_or_else(|| {
                anyhow::anyhow!("no Oore user with email {member}; invite them first")
            })?;
        ids.push(user.id);
    }
    Ok(ids)
}

fn print_group(group: &TesterGroup) {
    let members = group
        .members
        .iter()
        .map(|member| member.email.as_str())
        .collect::<Vec<_>>();
    println!(
        "{} ({}): {}",
        group.name,
        group.id,
        if members.is_empty() {
            "no members".to_string()
        } else {
            members.join(", ")
        }
    );
}

async fn list_groups(args: GroupListArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let (response, groups) = fetch_groups(&api).await?;
    if args.json {
        return print_json(&response);
    }
    if groups.is_empty() {
        println!("No tester groups. Create one with `oore tester group create --name <name>`.");
    }
    groups.iter().for_each(print_group);
    Ok(())
}

async fn save_group(
    api: &Api,
    id: Option<&str>,
    name: &str,
    member_ids: &[String],
) -> anyhow::Result<Value> {
    let body = json!({ "name": name, "member_user_ids": member_ids });
    match id {
        None => {
            call(
                api,
                reqwest::Method::POST,
                api_url(api, &["tester-groups"])?,
                Some(&body),
                "tester group create",
            )
            .await
        }
        Some(id) => {
            call(
                api,
                reqwest::Method::PUT,
                api_url(api, &["tester-groups", id])?,
                Some(&body),
                "tester group update",
            )
            .await
        }
    }
}

async fn create_group(args: GroupCreateArgs) -> anyhow::Result<()> {
    let name = args.name.trim();
    anyhow::ensure!(!name.is_empty(), "--name must not be empty");
    let api = Api::connect(&args.connection)?;
    let member_ids = resolve_users(&api, &args.members).await?;
    let saved = save_group(&api, None, name, &member_ids).await?;
    if args.json {
        return print_json(&saved);
    }
    print_group(&decode(&saved, "tester group create")?);
    Ok(())
}

async fn update_group(args: GroupUpdateArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let (_, groups) = fetch_groups(&api).await?;
    let group = find_group(&groups, &args.group)?;
    let add = resolve_users(&api, &args.add).await?;
    let remove = resolve_users(&api, &args.remove).await?;
    let mut member_ids = group
        .members
        .iter()
        .map(|member| member.user_id.clone())
        .filter(|id| !remove.contains(id))
        .collect::<Vec<_>>();
    for id in add {
        if !member_ids.contains(&id) {
            member_ids.push(id);
        }
    }
    let name = args
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(&group.name);
    let saved = save_group(&api, Some(&group.id), name, &member_ids).await?;
    if args.json {
        return print_json(&saved);
    }
    print_group(&decode(&saved, "tester group update")?);
    Ok(())
}

async fn delete_group(args: GroupDeleteArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let (_, groups) = fetch_groups(&api).await?;
    let group = find_group(&groups, &args.group)?;
    confirm(args.yes, &format!("Delete tester group {}?", group.name))?;
    call(
        &api,
        reqwest::Method::DELETE,
        api_url(&api, &["tester-groups", &group.id])?,
        None,
        "tester group delete",
    )
    .await?;
    if args.json {
        return print_json(&json!({ "deleted": true, "id": group.id }));
    }
    println!("Deleted tester group {}.", group.name);
    Ok(())
}

pub(crate) async fn share_build(args: BuildShareArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let (_, groups) = fetch_groups(&api).await?;
    let group = find_group(&groups, &args.group)?;
    let mut body = json!({ "group_id": group.id });
    if let Some(message) = args
        .message
        .as_deref()
        .map(str::trim)
        .filter(|message| !message.is_empty())
    {
        body["message"] = json!(message);
    }
    let share = call(
        &api,
        reqwest::Method::POST,
        api_url(&api, &["builds", &args.build_id, "shares"])?,
        Some(&body),
        "build share",
    )
    .await?;
    if args.json {
        return print_json(&share);
    }
    let share: BuildShare = decode(&share, "build share")?;
    let without_access = share
        .recipients
        .iter()
        .filter(|recipient| recipient.no_access)
        .map(|recipient| recipient.email.as_str())
        .collect::<Vec<_>>();
    println!(
        "Sent build {} to {} ({} people).",
        share.build_id,
        share.group_name,
        share.recipients.len()
    );
    if !without_access.is_empty() {
        println!(
            "These people cannot open this app, so their email has no install link: {}",
            without_access.join(", ")
        );
    }
    Ok(())
}

pub(crate) async fn list_runners(args: RunnerListArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let mut url = api_url(&api, &["runners"])?;
    url.query_pairs_mut().append_pair("limit", "100");
    let response = get(&api, url, "runner list").await?;
    if args.json {
        return print_json(&response);
    }
    let list: ListRunnersResponse = decode(&response, "runner list")?;
    if list.runners.is_empty() {
        println!("No runners. Register one with `oore runner register`.");
        return Ok(());
    }
    let rows = list
        .runners
        .iter()
        .map(|runner| {
            vec![
                runner.id.clone(),
                runner.name.clone(),
                runner.status.clone(),
                runner.labels.join(","),
                format_time(runner.last_heartbeat_at),
            ]
        })
        .collect::<Vec<_>>();
    admin_api::print_table(&["ID", "NAME", "STATUS", "LABELS", "LAST SEEN"], &rows);
    println!("{} of {} online.", list.online_total, list.total);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: &str, name: &str) -> TesterGroup {
        TesterGroup {
            id: id.into(),
            name: name.into(),
            members: Vec::new(),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn a_group_is_found_by_id_or_name() {
        let groups = vec![group("g1", "QA"), group("g2", "Leadership")];
        assert_eq!(find_group(&groups, "g2").unwrap().name, "Leadership");
        assert_eq!(find_group(&groups, "qa").unwrap().id, "g1");
        let missing = find_group(&groups, "Design").unwrap_err();
        assert_eq!(admin_api::exit_code_for(&missing), 5);
        assert!(missing.to_string().contains("QA, Leadership"));
    }
}
