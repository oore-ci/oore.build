//! `oore commands` and `oore completions`: the command tree as JSON or as
//! the Markdown reference page, read from clap itself, and shell completion
//! scripts.

use clap::{Arg, ArgAction, Args, Command};

use crate::cli_contract::print_json;

#[derive(Debug, Args)]
pub(crate) struct CommandsArgs {
    /// Print the tree as JSON: every command with its flags, plus the exit
    /// codes.
    #[arg(long, conflicts_with = "markdown")]
    json: bool,

    /// Print the CLI reference page the docs publish.
    #[arg(long)]
    markdown: bool,
}

#[derive(Debug, Args)]
pub(crate) struct CompletionsArgs {
    /// Shell to write the completion script for.
    #[arg(value_enum)]
    shell: clap_complete::Shell,
}

fn arg_value(arg: &Arg) -> serde_json::Value {
    let takes_value = arg.get_action().takes_values();
    let possible: Vec<String> = arg
        .get_possible_values()
        .iter()
        .filter(|value| !value.is_hide_set())
        .map(|value| value.get_name().to_string())
        .collect();
    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect();
    let mut value = serde_json::json!({
        "name": arg.get_id().as_str(),
        "help": arg.get_help().map(ToString::to_string),
        "required": arg.is_required_set(),
        "takes_value": takes_value,
        "repeatable": matches!(arg.get_action(), ArgAction::Append | ArgAction::Count),
        "positional": arg.is_positional(),
        "global": arg.is_global_set(),
    });
    if let Some(long) = arg.get_long() {
        value["long"] = format!("--{long}").into();
    }
    if let Some(short) = arg.get_short() {
        value["short"] = format!("-{short}").into();
    }
    if let Some(aliases) = arg.get_visible_aliases() {
        value["aliases"] = aliases
            .iter()
            .map(|alias| format!("--{alias}"))
            .collect::<Vec<_>>()
            .into();
    }
    if takes_value && let Some(names) = arg.get_value_names() {
        value["value_name"] = names
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
            .into();
    }
    if !possible.is_empty() {
        value["possible_values"] = possible.into();
    }
    if !defaults.is_empty() && takes_value {
        value["default"] = defaults.join(",").into();
    }
    if let Some(env) = arg.get_env() {
        value["env"] = env.to_string_lossy().into_owned().into();
    }
    value
}

fn visible_args(command: &Command) -> impl Iterator<Item = &Arg> {
    command
        .get_arguments()
        .filter(|arg| !arg.is_hide_set() && arg.get_id() != "help" && arg.get_id() != "version")
}

fn command_value(command: &Command, path: &str) -> serde_json::Value {
    let subcommands: Vec<serde_json::Value> = command
        .get_subcommands()
        .filter(|sub| !sub.is_hide_set() && sub.get_name() != "help")
        .map(|sub| command_value(sub, &format!("{path} {}", sub.get_name())))
        .collect();
    serde_json::json!({
        "name": command.get_name(),
        "path": path,
        "about": command.get_about().map(ToString::to_string),
        "args": visible_args(command).map(arg_value).collect::<Vec<_>>(),
        "subcommands": subcommands,
    })
}

/// The whole tree, as `oore commands --json` prints it.
pub(crate) fn tree(mut command: Command) -> serde_json::Value {
    command.build();
    serde_json::json!({
        "name": command.get_name(),
        "version": env!("CARGO_PKG_VERSION"),
        "exit_codes": {
            "0": "ok",
            "1": "failed, or the build did not succeed",
            "2": "blocked until a person acts",
            "3": "the store blocked, rejected or failed the release",
            "4": "not signed in or not allowed",
            "5": "not found",
            "64": "usage error",
        },
        "commands": command_value(&command, command.get_name())["subcommands"].clone(),
    })
}

/// Escapes text for MDX outside code spans: `<` and `{` start JSX there.
fn mdx_text(text: &str) -> String {
    let mut out = String::new();
    for (index, part) in text.split('`').enumerate() {
        if index > 0 {
            out.push('`');
        }
        if index % 2 == 1 {
            out.push_str(part);
        } else {
            out.push_str(
                &part
                    .replace('<', "&lt;")
                    .replace('{', "\\{")
                    .replace('}', "\\}")
                    .replace('|', "\\|"),
            );
        }
    }
    out
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Clap drops a short help's last full stop; put it back so sentences can
/// follow.
fn sentence(text: &str) -> String {
    let text = one_line(text);
    if text.is_empty() || text.ends_with(['.', '!', '?', ':']) {
        text
    } else {
        format!("{text}.")
    }
}

fn flag_name(arg: &serde_json::Value) -> String {
    let mut name = if arg["positional"].as_bool() == Some(true) {
        format!(
            "<{}>",
            arg["value_name"]
                .as_str()
                .unwrap_or_else(|| arg["name"].as_str().unwrap_or_default())
        )
    } else {
        let mut name = arg["long"].as_str().unwrap_or_default().to_string();
        if let Some(short) = arg["short"].as_str() {
            name = format!("{short}, {name}");
        }
        if arg["takes_value"].as_bool() == Some(true) {
            let value = arg["value_name"]
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_else(|| arg["name"].as_str().unwrap_or_default().to_uppercase());
            name.push_str(&format!(" <{value}>"));
        }
        name
    };
    if let Some(aliases) = arg["aliases"].as_array() {
        for alias in aliases {
            name.push_str(&format!(", {}", alias.as_str().unwrap_or_default()));
        }
    }
    name
}

fn markdown_command(command: &serde_json::Value, out: &mut String) {
    let subcommands = command["subcommands"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let args = command["args"].as_array().cloned().unwrap_or_default();
    let own_args: Vec<_> = args
        .iter()
        .filter(|arg| arg["global"].as_bool() != Some(true))
        .collect();
    if subcommands.is_empty() || !own_args.is_empty() {
        out.push_str(&format!(
            "\n## `{}`\n\n",
            command["path"].as_str().unwrap_or_default()
        ));
        if let Some(about) = command["about"].as_str() {
            out.push_str(&format!("{}\n", mdx_text(&sentence(about))));
        }
        if !own_args.is_empty() {
            out.push_str("\n| Flag | What it does |\n| --- | --- |\n");
            for arg in own_args {
                let mut help = arg["help"].as_str().map(sentence).unwrap_or_default();
                if arg["required"].as_bool() == Some(true) {
                    help.push_str(" Required.");
                }
                if let Some(default) = arg["default"].as_str() {
                    help.push_str(&format!(" Default `{default}`."));
                }
                if let Some(values) = arg["possible_values"].as_array() {
                    let values: Vec<String> = values
                        .iter()
                        .map(|value| format!("`{}`", value.as_str().unwrap_or_default()))
                        .collect();
                    help.push_str(&format!(" One of {}.", values.join(", ")));
                }
                if let Some(env) = arg["env"].as_str() {
                    help.push_str(&format!(" Env `{env}`."));
                }
                out.push_str(&format!(
                    "| `{}` | {} |\n",
                    flag_name(arg).replace('|', "\\|"),
                    mdx_text(help.trim())
                ));
            }
        }
    }
    for sub in &subcommands {
        markdown_command(sub, out);
    }
}

/// The reference page `apps/docs/content/docs/reference/cli/commands.mdx`.
pub(crate) fn markdown(command: Command) -> String {
    let tree = tree(command);
    let mut out = String::from(
        "---\ntitle: All commands\ndescription: Every oore command and flag, generated from the CLI itself.\n---\n\n\
         {/* Generated by `oore commands --markdown`. Do not edit by hand; run\n   \
         `cargo run -p oore -- commands --markdown > apps/docs/content/docs/reference/cli/commands.mdx`. */}\n\n\
         This page is generated from the CLI. `oore commands --json` prints the same\n\
         tree for scripts and agents, and `oore <command> --help` prints one command.\n\
         Every command takes `--color`, `--header` and `--help`, and exits with the codes in\n\
         [Exit codes](/reference/cli/exit-codes).\n",
    );
    for command in tree["commands"].as_array().cloned().unwrap_or_default() {
        markdown_command(&command, &mut out);
    }
    out
}

pub(crate) fn handle_commands(args: CommandsArgs, command: Command) -> anyhow::Result<()> {
    if args.markdown {
        print!("{}", markdown(command));
        return Ok(());
    }
    let tree = tree(command);
    if args.json {
        return print_json(&tree);
    }
    fn walk(command: &serde_json::Value, out: &mut String) {
        let subs = command["subcommands"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if subs.is_empty() {
            out.push_str(&format!(
                "{:<36} {}\n",
                command["path"].as_str().unwrap_or_default(),
                command["about"].as_str().map(one_line).unwrap_or_default()
            ));
        }
        for sub in &subs {
            walk(sub, out);
        }
    }
    let mut out = String::new();
    for command in tree["commands"].as_array().cloned().unwrap_or_default() {
        walk(&command, &mut out);
    }
    print!("{out}");
    Ok(())
}

pub(crate) fn handle_completions(args: CompletionsArgs, mut command: Command) {
    let name = command.get_name().to_string();
    clap_complete::generate(args.shell, &mut command, name, &mut std::io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mdx_text_escapes_only_outside_code() {
        assert_eq!(
            mdx_text("Print `{\"a\": 1}` for <id> {x}"),
            "Print `{\"a\": 1}` for &lt;id> \\{x\\}"
        );
    }
}
