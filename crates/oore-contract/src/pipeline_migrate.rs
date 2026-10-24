//! `oore pipeline migrate`: rewrite a `version: 1` `.oore.yaml` as
//! `version: 2`.
//!
//! The conversion is pure: it takes the file's text and returns the new
//! file's text with the warnings and notes the CLI prints. Before returning,
//! it plans the original and the generated file and checks that they run the
//! same commands, in the same order and phase, with the same inputs, so a
//! file that would build differently is never produced.

use std::collections::HashSet;

use crate::pipeline_config::{
    PipelinePlan, PlanRequest, RepositoryConfig, StepPhase, default_platform_command,
    default_play_bundle_command, parse_repository_config, platform_key, platform_title,
};
use crate::{
    BuildIsolation, BuildPlatform, PipelineExecutionConfig, default_artifact_patterns_for_platforms,
};

/// What migrating a file produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrateOutcome {
    /// The file already declares `version: 2`; there is nothing to do.
    AlreadyV2,
    Migrated(Migration),
}

/// A generated `version: 2` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    /// The new file, ending in a newline. It parses as version 2.
    pub yaml: String,
    /// Behaviour version 2 cannot express exactly. Each also appears as a
    /// `WARNING:` comment in [`Migration::yaml`].
    pub warnings: Vec<String>,
    /// Behaviour that is preserved but worth knowing, such as settings that
    /// version 1 ignored and that the new file drops.
    pub notes: Vec<String>,
}

/// Migrate `.oore.yaml` text. Errors are the file's own validation errors,
/// or why it cannot be migrated.
pub fn migrate_pipeline_config(raw: &str) -> Result<MigrateOutcome, String> {
    let config = match parse_repository_config(raw)? {
        RepositoryConfig::V2(_) => return Ok(MigrateOutcome::AlreadyV2),
        RepositoryConfig::V1(config) => *config,
    };
    let converted = convert(&config)?;
    verify(&config, &converted)?;
    Ok(MigrateOutcome::Migrated(converted.migration))
}

// ── Conversion ──────────────────────────────────────────────────

/// Step names `oore pipeline migrate` gives to well-known commands.
const KNOWN_STEP_NAMES: [(&str, &str); 3] = [
    ("flutter pub get", "Get packages"),
    ("flutter analyze", "Analyze"),
    ("flutter test", "Run tests"),
];

/// Longest step name derived from a command, before `...`.
const DERIVED_NAME_LENGTH: usize = 60;

const ARTIFACTS_ANCHOR: &str = "artifacts";

struct Target {
    platform: BuildPlatform,
    build: String,
    comments: Vec<String>,
}

pub(crate) struct Step {
    pub(crate) name: String,
    pub(crate) run: String,
    pub(crate) comments: Vec<String>,
}

struct Converted {
    migration: Migration,
    /// The platform whose target runs the shared `commands.build`, when the
    /// v1 file has one.
    shared_build_platform: Option<BuildPlatform>,
}

fn convert(config: &PipelineExecutionConfig) -> Result<Converted, String> {
    let mut warnings = Vec::new();
    let mut notes = Vec::new();
    for (index, platform) in config.platforms.iter().enumerate() {
        if config.platforms[..index].contains(platform) {
            return Err(format!(
                "platforms lists {} more than once, which version 2 cannot express: each target builds once. Remove the duplicate before migrating",
                platform_key(platform)
            ));
        }
    }
    let shared_build = &config.commands.build;
    let uses_defaults = shared_build.is_empty();

    // Targets.
    let mut targets: Vec<Target> = Vec::new();
    if uses_defaults {
        for platform in &config.platforms {
            let key = platform_key(platform);
            let (command, args) = platform_inputs(config, platform);
            let build = default_platform_command(
                platform,
                &config.platform_commands,
                &config.platform_build_args,
            );
            let mut comments = Vec::new();
            if command.is_some() {
                comments.push(format!("From platform_commands.{key}."));
                if !args.is_empty() {
                    let note = format!(
                        "platform_build_args.{key} is dropped: version 1 ignored it because platform_commands.{key} replaces the default command"
                    );
                    comments.push(format!("{note}."));
                    notes.push(note);
                }
            } else if args.is_empty() {
                comments.push(format!(
                    "Version 1's default {} build command.",
                    platform_title(platform)
                ));
            } else {
                comments.push(format!(
                    "Version 1's default {} build command with platform_build_args.{key}.",
                    platform_title(platform)
                ));
            }
            if *platform == BuildPlatform::Android && command.is_none() {
                let bundle = default_play_bundle_command(&config.platform_build_args);
                comments.push(format!(
                    "When the pipeline's Google Play publishing was on, version 1 also built an App Bundle after this command. Version 2 runs only `build`, so if this pipeline uploads to Google Play, use:\n    build: {build} && {bundle}"
                ));
                notes.push(format!(
                    "targets.android.build: version 1 also built an App Bundle (`{bundle}`) when the pipeline's Google Play publishing was on. That comes from the pipeline's settings, not the file, so add it to `build` yourself if this pipeline uploads to Google Play"
                ));
            }
            targets.push(Target {
                platform: platform.clone(),
                build,
                comments,
            });
        }
    } else {
        // A shared commands.build replaced every platform command. Version 2
        // builds through targets, so the first platform's target runs it.
        let platform = config.platforms[0].clone();
        let key = platform_key(&platform);
        let mut comments = vec![
            "From commands.build, which version 1 ran once in place of the platform build commands."
                .to_string(),
        ];
        if shared_build.len() > 1 {
            let warning = format!(
                "commands.build has {} commands, which version 1 ran as separate steps. Version 2 runs them as one step, `Build {}`, each in its own subshell; a plain `flutter test` or `dart test` among them no longer reports test results on its own",
                shared_build.len(),
                platform_title(&platform)
            );
            comments.push(format!("WARNING: {warning}."));
            warnings.push(warning);
        }
        if config.platforms.len() > 1 {
            let others: Vec<&str> = config.platforms[1..].iter().map(platform_key).collect();
            let warning = format!(
                "platforms also lists {}, but version 1 built them with the shared commands.build. Version 2 needs a build command per target, so only {key} is a target; the build no longer counts {} as built platforms",
                others.join(", "),
                if others.len() == 1 { "it" } else { "them" }
            );
            comments.push(format!("WARNING: {warning}."));
            warnings.push(warning);
        }
        for platform in &config.platforms {
            let key = platform_key(platform);
            let (command, args) = platform_inputs(config, platform);
            if command.is_some() {
                notes.push(format!(
                    "platform_commands.{key} is dropped: version 1 ignored it because commands.build replaces the platform build commands"
                ));
            }
            if !args.is_empty() {
                notes.push(format!(
                    "platform_build_args.{key} is dropped: version 1 ignored it because commands.build replaces the platform build commands"
                ));
            }
        }
        targets.push(Target {
            platform,
            build: join_shared_build(shared_build),
            comments,
        });
    }

    // Artifacts: version 1 has one list for the build, version 2 one per
    // target, collected in target order and each pattern kept once.
    let mut artifacts: Vec<String> = Vec::new();
    for (index, pattern) in config.artifact_patterns.iter().enumerate() {
        if pattern.contains("${{") {
            return Err(format!(
                "artifacts.patterns[{index}] contains `${{{{`, which version 2 reads as a reference; change the pattern before migrating"
            ));
        }
        let pattern = pattern.trim().to_string();
        if artifacts.contains(&pattern) {
            notes.push(format!(
                "artifacts.patterns[{index}] repeats '{pattern}'; version 2 keeps each pattern once"
            ));
        } else {
            artifacts.push(pattern);
        }
    }
    let mut default_artifacts: Vec<String> = Vec::new();
    for target in &targets {
        for pattern in
            default_artifact_patterns_for_platforms(std::slice::from_ref(&target.platform))
        {
            if !default_artifacts.contains(&pattern) {
                default_artifacts.push(pattern);
            }
        }
    }
    let explicit_artifacts = artifacts != default_artifacts;

    // Steps around the builds.
    let mut names: HashSet<String> = targets
        .iter()
        .map(|target| format!("Build {}", platform_title(&target.platform)))
        .collect();
    let mut pre_build = Vec::new();
    if uses_defaults {
        pre_build.push(Step {
            name: unique_name(&mut names, "Get packages"),
            run: "flutter pub get".to_string(),
            comments: vec![
                "Version 1 ran `flutter pub get` before the pre_build commands whenever it used the default platform build commands.".to_string(),
            ],
        });
    }
    let user_steps = |names: &mut HashSet<String>, commands: &[String]| -> Vec<Step> {
        commands
            .iter()
            .map(|command| {
                let run = command.trim().to_string();
                Step {
                    name: unique_name(names, &step_name(&run)),
                    run,
                    comments: Vec::new(),
                }
            })
            .collect()
    };
    pre_build.extend(user_steps(&mut names, &config.commands.pre_build));
    let post_build = user_steps(&mut names, &config.commands.post_build);

    // Render.
    let mut out = String::new();
    for line in [
        "Generated by `oore pipeline migrate` from a version 1 .oore.yaml.",
        "It runs the same commands in the same order. Review it before committing: steps have new names, which logs and checks show, and comments from the old file are not carried over.",
    ] {
        push_comment(&mut out, 0, line);
    }
    if !warnings.is_empty() {
        push_comment(
            &mut out,
            0,
            "Some version 1 behaviour cannot be expressed exactly; see the WARNING comments below.",
        );
    }
    out.push_str("version: 2\n");

    out.push('\n');
    match config
        .flutter_version
        .as_deref()
        .map(str::trim)
        .filter(|version| !version.is_empty())
    {
        Some(version) => {
            out.push_str("app:\n");
            push_comment(
                &mut out,
                2,
                "A .fvmrc file still takes precedence, as it did over flutter_version.",
            );
            out.push_str(&format!("  flutter:{}\n", scalar(version, 2)));
        }
        None => push_comment(
            &mut out,
            0,
            "No Flutter version is pinned, as before: a .fvmrc file, else the runner's Flutter, decides. Set app.flutter to pin one.",
        ),
    }

    out.push_str("\ntargets:\n");
    for (index, target) in targets.iter().enumerate() {
        out.push_str(&format!("  {}:\n", platform_key(&target.platform)));
        for comment in &target.comments {
            push_comment(&mut out, 4, comment);
        }
        out.push_str(&format!("    build:{}\n", scalar(&target.build, 4)));
        if explicit_artifacts {
            if targets.len() == 1 {
                out.push_str("    artifacts:\n");
                push_list(&mut out, 6, &artifacts);
            } else if index == 0 {
                push_comment(
                    &mut out,
                    4,
                    "Version 1 collected one artifact list for the whole build. Every target uses it, and each pattern is kept once.",
                );
                out.push_str(&format!("    artifacts: &{ARTIFACTS_ANCHOR}\n"));
                push_list(&mut out, 6, &artifacts);
            } else {
                out.push_str(&format!("    artifacts: *{ARTIFACTS_ANCHOR}\n"));
            }
        }
    }

    if !pre_build.is_empty() || !post_build.is_empty() {
        out.push_str("\nsteps:\n");
        for step in &pre_build {
            push_step(&mut out, step);
        }
        push_comment(&mut out, 2, "The target builds run here.");
        out.push_str("  - build\n");
        for step in &post_build {
            push_step(&mut out, step);
        }
    }

    if !config.env.is_empty() {
        out.push_str("\nenv:\n");
        for entry in &config.env {
            out.push_str(&format!(
                "  {}:{}\n",
                scalar(entry.key.trim(), 0).trim_start(),
                scalar(entry.value.trim(), 2)
            ));
        }
    }

    let mut settings = String::new();
    if config.build_number_offset != 0 {
        settings.push_str(&format!(
            "build_number_offset: {}\n",
            config.build_number_offset
        ));
    }
    if !config.runs_on.is_empty() {
        settings.push_str("runs_on:");
        if config.runs_on.iter().all(|label| is_flow_safe(label)) {
            settings.push_str(&format!(" [{}]\n", config.runs_on.join(", ")));
        } else {
            settings.push('\n');
            push_list(&mut settings, 2, &config.runs_on);
        }
    }
    if config.isolation != BuildIsolation::default() {
        settings.push_str(&format!(
            "isolation: {}\n",
            isolation_name(config.isolation)
        ));
    }
    if !settings.is_empty() {
        out.push('\n');
        out.push_str(&settings);
    }

    Ok(Converted {
        migration: Migration {
            yaml: out,
            warnings,
            notes,
        },
        shared_build_platform: (!uses_defaults).then(|| config.platforms[0].clone()),
    })
}

/// A platform's `platform_commands` entry and `platform_build_args`.
fn platform_inputs<'a>(
    config: &'a PipelineExecutionConfig,
    platform: &BuildPlatform,
) -> (Option<&'a String>, &'a [String]) {
    match platform {
        BuildPlatform::Android => (
            config.platform_commands.android.as_ref(),
            &config.platform_build_args.android,
        ),
        BuildPlatform::Ios => (
            config.platform_commands.ios.as_ref(),
            &config.platform_build_args.ios,
        ),
        BuildPlatform::Macos => (
            config.platform_commands.macos.as_ref(),
            &config.platform_build_args.macos,
        ),
    }
}

/// A shared `commands.build` as one target build. Version 1 ran each command
/// as its own step, so each gets its own subshell: a `cd` or `export` in one
/// does not reach the next, and the first failure stops the rest.
pub fn join_shared_build(commands: &[String]) -> String {
    match commands {
        [only] => only.trim().to_string(),
        _ => commands
            .iter()
            .map(|command| {
                let command = command.trim();
                // A `#` comment or a line break would swallow an inline `)`.
                if command.contains(['#', '\n']) {
                    format!("(\n{command}\n)")
                } else {
                    format!("( {command} )")
                }
            })
            .collect::<Vec<_>>()
            .join(" &&\n"),
    }
}

pub(crate) fn step_name(run: &str) -> String {
    if let Some((_, name)) = KNOWN_STEP_NAMES.iter().find(|(command, _)| *command == run) {
        return (*name).to_string();
    }
    let first_line = run.lines().next().unwrap_or_default();
    let collapsed = first_line.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut name: String = collapsed.chars().take(DERIVED_NAME_LENGTH).collect();
    if name.len() < collapsed.len() || run.lines().nth(1).is_some() {
        name.push_str("...");
    }
    if name.is_empty() {
        "Step".to_string()
    } else {
        name
    }
}

pub(crate) fn unique_name(names: &mut HashSet<String>, base: &str) -> String {
    let mut name = base.to_string();
    let mut counter = 2;
    while !names.insert(name.clone()) {
        name = format!("{base} ({counter})");
        counter += 1;
    }
    name
}

pub(crate) fn isolation_name(isolation: BuildIsolation) -> &'static str {
    match isolation {
        BuildIsolation::Host => "host",
        BuildIsolation::Vm => "vm",
    }
}

// ── YAML output ─────────────────────────────────────────────────

/// Width comments wrap at, including indentation.
const COMMENT_WIDTH: usize = 78;

/// Write `text` as `#` comment lines. Each `\n` starts a new line, and a
/// line that starts with spaces, such as a command to copy, is kept as is.
pub(crate) fn push_comment(out: &mut String, indent: usize, text: &str) {
    let pad = " ".repeat(indent);
    for paragraph in text.split('\n') {
        if paragraph.starts_with(' ') {
            out.push_str(&format!("{pad}#{}\n", paragraph.trim_end()));
        } else {
            push_wrapped_comment(out, &pad, paragraph);
        }
    }
}

fn push_wrapped_comment(out: &mut String, pad: &str, text: &str) {
    let width = COMMENT_WIDTH.saturating_sub(pad.len() + 2).max(20);
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            out.push_str(&format!("{pad}# {line}\n"));
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push_str(&format!("{pad}# {line}\n"));
    }
}

pub(crate) fn push_list(out: &mut String, indent: usize, items: &[String]) {
    let pad = " ".repeat(indent);
    for item in items {
        out.push_str(&format!("{pad}-{}\n", scalar(item, indent + 2)));
    }
}

pub(crate) fn push_step(out: &mut String, step: &Step) {
    for comment in &step.comments {
        push_comment(out, 2, comment);
    }
    out.push_str(&format!("  - name:{}\n", scalar(&step.name, 4)));
    out.push_str(&format!("    run:{}\n", scalar(&step.run, 4)));
}

/// `value` as a YAML scalar that follows a `key:` or `-`, with its leading
/// space. `indent` is the indentation of the key, for block scalars.
pub(crate) fn scalar(value: &str, indent: usize) -> String {
    if is_plain_safe(value) {
        format!(" {value}")
    } else if is_literal_safe(value) {
        let pad = " ".repeat(indent + 2);
        let mut out = String::from(" |-");
        for line in value.lines() {
            out.push('\n');
            if !line.is_empty() {
                out.push_str(&pad);
                out.push_str(line);
            }
        }
        out
    } else if !value.contains(|c: char| c.is_control()) {
        format!(" '{}'", value.replace('\'', "''"))
    } else {
        // JSON strings are valid YAML double-quoted scalars.
        format!(
            " {}",
            serde_json::to_string(value).unwrap_or_else(|_| format!("'{value}'"))
        )
    }
}

/// Words a YAML 1.1 reader (and many editors and linters) takes as a
/// boolean, null or special float. This crate reads YAML 1.2, where they are
/// strings, but they are quoted so the file means the same everywhere.
const YAML_11_KEYWORDS: [&str; 14] = [
    "true", "false", "yes", "no", "y", "n", "on", "off", "null", "~", ".inf", "-.inf", "+.inf",
    ".nan",
];

/// A plain (unquoted) scalar that reads back as the same string.
fn is_plain_safe(value: &str) -> bool {
    let Some(first) = value.chars().next() else {
        return false;
    };
    if YAML_11_KEYWORDS
        .iter()
        .any(|keyword| keyword.eq_ignore_ascii_case(value))
    {
        return false;
    }
    (first.is_ascii_alphabetic() || matches!(first, '_' | '.' | '/' | '$'))
        && value.trim() == value
        && !value.contains(|c: char| c.is_control())
        && !value.contains(": ")
        && !value.contains(" #")
        && !value.ends_with(':')
        && serde_yaml::from_str::<String>(value).is_ok_and(|parsed| parsed == value)
}

/// A multi-line value a `|-` block reproduces exactly.
fn is_literal_safe(value: &str) -> bool {
    value.contains('\n')
        && value.trim() == value
        && !value.contains(|c: char| c.is_control() && c != '\n' && c != '\t')
        && !value.lines().any(|line| line.trim_end() != line)
        && !value.starts_with([' ', '\t'])
}

/// A plain scalar that is also safe inside `[...]`.
pub(crate) fn is_flow_safe(value: &str) -> bool {
    is_plain_safe(value) && !value.contains([',', '[', ']', '{', '}'])
}

// ── Verification ────────────────────────────────────────────────

/// What a build depends on, from a plan: step names and sources differ
/// between the versions by design and are left out.
#[derive(Debug, PartialEq)]
pub(crate) struct Behaviour {
    pub(crate) platforms: Vec<BuildPlatform>,
    pub(crate) flutter_version: Option<String>,
    pub(crate) steps: Vec<(StepPhase, String, Option<BuildPlatform>)>,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) artifact_patterns: Vec<String>,
    pub(crate) build_number_offset: i64,
    pub(crate) runs_on: Vec<String>,
    pub(crate) isolation: BuildIsolation,
}

impl Behaviour {
    pub(crate) fn of(plan: &PipelinePlan) -> Self {
        let mut artifact_patterns: Vec<String> = Vec::new();
        for pattern in &plan.artifact_patterns {
            let pattern = pattern.trim().to_string();
            if !artifact_patterns.contains(&pattern) {
                artifact_patterns.push(pattern);
            }
        }
        Behaviour {
            platforms: plan.platforms.clone(),
            flutter_version: plan
                .flutter_version
                .as_deref()
                .map(str::trim)
                .filter(|version| !version.is_empty())
                .map(str::to_string),
            steps: plan
                .steps
                .iter()
                .map(|step| {
                    (
                        step.phase,
                        step.run.trim().to_string(),
                        step.platform.clone(),
                    )
                })
                .collect(),
            env: plan
                .env
                .iter()
                .map(|entry| (entry.key.trim().to_string(), entry.value.trim().to_string()))
                .collect(),
            artifact_patterns,
            build_number_offset: plan.build_number_offset,
            runs_on: plan.runs_on.clone(),
            isolation: plan.isolation,
        }
    }
}

/// Plan the original and the generated file and require the same behaviour.
/// The one difference allowed is the documented one for a shared
/// `commands.build`, which becomes the first platform's target build.
fn verify(config: &PipelineExecutionConfig, converted: &Converted) -> Result<(), String> {
    let generated = match parse_repository_config(&converted.migration.yaml) {
        Ok(RepositoryConfig::V2(config)) => RepositoryConfig::V2(config),
        Ok(RepositoryConfig::V1(_)) => {
            return Err("the generated file is not version 2; please report this".to_string());
        }
        Err(error) => {
            return Err(format!(
                "the generated file does not validate; please report this:\n{error}"
            ));
        }
    };
    let original = RepositoryConfig::V1(Box::new(config.clone()));
    let plan = |config: &RepositoryConfig| config.plan(PlanRequest::default());
    let mut expected = Behaviour::of(&plan(&original)?);
    let actual = Behaviour::of(&plan(&generated)?);

    if let Some(platform) = &converted.shared_build_platform {
        let shared: Vec<String> = expected
            .steps
            .iter()
            .filter(|(phase, _, _)| *phase == StepPhase::Build)
            .map(|(_, run, _)| run.clone())
            .collect();
        let first = expected
            .steps
            .iter()
            .position(|(phase, _, _)| *phase == StepPhase::Build)
            .unwrap_or(expected.steps.len());
        expected
            .steps
            .retain(|(phase, _, _)| *phase != StepPhase::Build);
        expected.steps.insert(
            first,
            (
                StepPhase::Build,
                join_shared_build(&shared),
                Some(platform.clone()),
            ),
        );
        expected.platforms = vec![platform.clone()];
    }

    if expected != actual {
        return Err(format!(
            "the generated file would build differently; please report this.\nversion 1: {expected:?}\nversion 2: {actual:?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_read_back_as_written() {
        for value in [
            "flutter pub get",
            "echo \"Build complete\"",
            "${{ secrets.SENTRY_DSN }}",
            "3.24.5",
            "true",
            "3",
            "a: b",
            "x # not a comment",
            "- dash",
            "it's",
            "line one\nline two",
            "  leading",
            "tab\there",
            "ends with colon:",
            "*star",
            "",
            "yes",
            "No",
            "ON",
            "off",
            "y",
            "Null",
            "~",
            ".inf",
            "-.Inf",
            ".NaN",
        ] {
            let yaml = format!("value:{}\n", scalar(value, 0));
            let parsed: std::collections::BTreeMap<String, String> =
                serde_yaml::from_str(&yaml).unwrap_or_else(|error| panic!("{yaml}: {error}"));
            assert_eq!(parsed["value"], value, "{yaml}");
        }
        // YAML 1.1 booleans, nulls and special floats are quoted.
        for value in [
            "yes", "No", "ON", "off", "y", "Null", "~", ".inf", "-.Inf", ".NaN",
        ] {
            assert!(scalar(value, 0).starts_with(" '"), "{value}");
        }
    }

    #[test]
    fn derived_step_names_are_short_single_lines() {
        assert_eq!(step_name("flutter pub get"), "Get packages");
        assert_eq!(step_name("echo   done"), "echo done");
        assert_eq!(step_name("set -e\nmake"), "set -e...");
        let long = format!("echo {}", "x".repeat(200));
        assert_eq!(step_name(&long).chars().count(), DERIVED_NAME_LENGTH + 3);
    }

    #[test]
    fn shared_build_commands_keep_their_own_shells() {
        assert_eq!(join_shared_build(&["make".to_string()]), "make");
        assert_eq!(
            join_shared_build(&["cd a && make".to_string(), "make # all".to_string()]),
            "( cd a && make ) &&\n(\nmake # all\n)"
        );
    }
}
