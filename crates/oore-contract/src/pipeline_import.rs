//! Read a `version: 2` `.oore.yaml` back into the pipeline editor's form
//! settings: the inverse of [`crate::pipeline_export`].
//!
//! The import is pure. It parses and validates the file with the v2
//! contract, maps what the form can hold, and lists what it cannot. Before
//! returning settings it exports them again and plans both files: they must
//! run the same commands in the same order and phase with the same inputs,
//! publish the same way and start on the same webhooks. Anything else is
//! reported as unsupported and no settings are returned, so applying an
//! import never changes what a build runs.
//!
//! Secrets are never invented. Commands and env values are copied as
//! written, so a `${{ secrets.NAME }}` reference stays a reference.

use crate::pipeline_config::{
    BranchTrigger, ConfigV2, PipelinePlan, PlayPublishSpec, RepositoryConfig, StepPhase,
    TestflightPublishSpec, default_platform_command, default_play_bundle_command,
    parse_repository_config, platform_key,
};
use crate::pipeline_export::{
    FormSettings, OnSpec, default_artifacts, export_form_settings, map_triggers, on_of,
    platform_inputs,
};
use crate::pipeline_migrate::Behaviour;
use crate::{
    AppStoreDestination, AppStorePublishConfig, BuildPlatform, GooglePlayPublishConfig,
    PipelineCommandStages, PipelineEnvVar, PipelineExecutionConfig, PlatformBuildArgs,
    PlatformBuildCommands, TriggerConfig, normalize_runner_labels,
};

/// The settings the editor holds now. The file decides what runs; these
/// only choose between equivalent ways of holding it (the pipeline's Build
/// commands, build arguments or an override) and supply what the file
/// leaves to the pipeline (schedules, store credentials).
#[derive(Debug, Clone, Copy, Default)]
pub struct ImportContext<'a> {
    pub execution: Option<&'a PipelineExecutionConfig>,
    /// The pipeline's trigger settings. `None` means the repository only
    /// builds manually, so the form has no trigger settings for `on`.
    pub triggers: Option<&'a TriggerConfig>,
    pub google_play: Option<&'a GooglePlayPublishConfig>,
    pub app_store: Option<&'a AppStorePublishConfig>,
}

/// A file read back into form settings.
#[derive(Debug, Clone, Default)]
pub struct FormImport {
    /// The form settings. Set only when the form can hold the whole file.
    pub execution: Option<PipelineExecutionConfig>,
    /// The trigger settings the file's `on` maps to, with the pipeline's
    /// schedules kept. `None` when the file has no `on` (the pipeline's
    /// trigger settings keep applying) or the form cannot hold the file.
    pub triggers: Option<TriggerConfig>,
    /// The file's `publish.play`, as written.
    pub play: Option<PlayPublishSpec>,
    /// The file's `publish.testflight`, as written.
    pub testflight: Option<TestflightPublishSpec>,
    /// What the form cannot hold. Non-empty means no settings.
    pub unsupported: Vec<String>,
    /// What the form drops without changing what runs, such as step names.
    pub notes: Vec<String>,
}

impl FormImport {
    fn unsupported(unsupported: Vec<String>) -> Self {
        FormImport {
            unsupported,
            ..FormImport::default()
        }
    }
}

const PUB_GET: &str = "flutter pub get";

/// Import a repository file. `Err` holds the file's validation errors, one
/// per line, as `oore pipeline validate` reports them.
pub fn import_form_settings(yaml: &str, context: ImportContext<'_>) -> Result<FormImport, String> {
    let config = match parse_repository_config(yaml)? {
        RepositoryConfig::V2(config) => config,
        RepositoryConfig::V1(_) => {
            return Ok(FormImport::unsupported(vec![
                "This is a version 1 file. The pane edits version 2: run `oore pipeline migrate` on it, or start from the generated file".to_string(),
            ]));
        }
    };

    // Features the form has no place for at all.
    let mut unsupported = Vec::new();
    if !config.workflows.is_empty() {
        unsupported.push(format!(
            "workflows ({}): the form edits one set of build settings, so a file with workflows is edited as YAML only",
            config.workflow_names().join(", ")
        ));
    }
    if !config.vars.is_empty() {
        let names: Vec<&str> = config.vars.iter().map(|(name, _)| name.as_str()).collect();
        unsupported.push(format!(
            "vars ({}): the form has no variables; write the values where they are used",
            names.join(", ")
        ));
    }
    if !config.app.flavors.is_empty() {
        unsupported.push(format!(
            "app.flavors ({}): the form builds each target once, without a flavor matrix",
            config.app.flavors.join(", ")
        ));
    }
    if !unsupported.is_empty() {
        return Ok(FormImport::unsupported(unsupported));
    }

    let plan = config.plan(None, None)?;
    let mut notes = Vec::new();
    let play_upload = context.google_play.filter(|play| play.enabled);
    let app_store = context.app_store.filter(|store| store.enabled);

    // Steps around the builds.
    for step in &plan.steps {
        if step.test_report.is_some() {
            unsupported.push(format!(
                "steps: '{}' reports test results (reports.tests), which the form cannot hold",
                step.name
            ));
        }
        if step.platform.is_none() && step.run.contains('\n') {
            unsupported.push(format!(
                "steps: '{}' runs several lines; the form takes one command per line, so split it into steps or move it into a script",
                step.name
            ));
        }
    }
    let mut pre_build: Vec<String> = plan.commands(StepPhase::PreBuild);
    if pre_build.first().map(String::as_str) == Some(PUB_GET) {
        pre_build.remove(0);
    } else {
        unsupported.push(format!(
            "steps: the form always runs `{PUB_GET}` before its own commands; keep a `{PUB_GET}` step first"
        ));
    }
    // The form builds targets in a fixed order; another order would build
    // them in a different sequence.
    let canonical: Vec<BuildPlatform> = [
        BuildPlatform::Android,
        BuildPlatform::Ios,
        BuildPlatform::Macos,
    ]
    .into_iter()
    .filter(|platform| plan.platforms.contains(platform))
    .collect();
    if canonical != plan.platforms {
        let keys: Vec<&str> = canonical.iter().map(platform_key).collect();
        unsupported.push(format!(
            "targets: the form builds targets in the order {}; list them in that order",
            keys.join(", ")
        ));
    }
    let after_build = plan.commands(StepPhase::PostBuild);
    let base_build: Vec<String> = context
        .execution
        .map(|base| trimmed(&base.commands.build))
        .unwrap_or_default();
    let (build_commands, post_build) = if !base_build.is_empty()
        && after_build.starts_with(&base_build)
    {
        let rest = after_build[base_build.len()..].to_vec();
        (base_build, rest)
    } else {
        // Only when the file still runs them, now as plain steps.
        if base_build
            .iter()
            .any(|command| after_build.contains(command))
        {
            notes.push(
                    "The pipeline's Build commands now run after the target builds, as the file runs them".to_string(),
                );
        }
        (Vec::new(), after_build)
    };

    // Targets: one build step each, as there are no flavors. Platforms the
    // file does not build keep the inputs the form has for them.
    let mut platform_commands = context
        .execution
        .map(|base| base.platform_commands.clone())
        .unwrap_or_default();
    let mut platform_build_args = context
        .execution
        .map(|base| base.platform_build_args.clone())
        .unwrap_or_default();
    for step in plan.steps_in(StepPhase::Build) {
        let Some(platform) = &step.platform else {
            continue;
        };
        let (command, args) = target_inputs(
            platform,
            &step.run,
            play_upload.is_some() && *platform == BuildPlatform::Android,
            context.execution,
        );
        if command
            .as_deref()
            .is_some_and(|command| command.contains('\n'))
        {
            unsupported.push(format!(
                "targets.{}.build spans several lines; the form's build command override is one line",
                platform_key(platform)
            ));
        }
        match platform {
            BuildPlatform::Android => {
                platform_commands.android = command;
                platform_build_args.android = args;
            }
            BuildPlatform::Ios => {
                platform_commands.ios = command;
                platform_build_args.ios = args;
            }
            BuildPlatform::Macos => {
                platform_commands.macos = command;
                platform_build_args.macos = args;
            }
        }
    }

    // Env, one line per variable in the form.
    for entry in &plan.env {
        if entry.value.contains('\n') {
            unsupported.push(format!(
                "env.{} spans several lines; the form takes one line per variable",
                entry.key
            ));
        } else if entry.value.trim() != entry.value {
            unsupported.push(format!(
                "env.{} starts or ends with spaces, which the form trims",
                entry.key
            ));
        }
    }

    // Artifacts: the form collects one list for the whole build.
    let per_target: Vec<Option<&Vec<String>>> = config
        .targets
        .iter()
        .map(|(_, target)| target.artifacts.as_ref())
        .collect();
    if per_target.windows(2).any(|pair| pair[0] != pair[1]) {
        notes.push(
            "The targets' artifact lists become one list, which the form collects for every target"
                .to_string(),
        );
    }

    // Publish: the form does not hold store settings, so the file must
    // say what the pipeline's Publish settings already do.
    let mut play = None;
    if let Some(planned) = &plan.publish.play {
        if let Some(error) = &planned.error {
            unsupported.push(format!("publish.play cannot run: {error}"));
        }
        let spec = planned.spec.clone();
        match (play_upload, expected_play(play_upload, &plan.platforms)) {
            (None, _) => unsupported.push(
                "publish.play: turn on Google Play upload in the pipeline's Publish settings; the form cannot".to_string(),
            ),
            (Some(_), Some(expected)) if !same_play(&spec, &expected) => unsupported.push(format!(
                "publish.play: the pipeline's Google Play settings upload to {}; change the track, rollout or draft in the Publish settings",
                describe_play(&expected)
            )),
            _ => {}
        }
        play = Some(spec);
    }
    let mut testflight = None;
    if let Some(planned) = &plan.publish.testflight {
        if let Some(error) = &planned.error {
            unsupported.push(format!("publish.testflight cannot run: {error}"));
        }
        let spec = planned.spec.clone();
        match app_store {
            None => unsupported.push(
                "publish.testflight: turn on App Store Connect upload in the pipeline's Publish settings; the form cannot".to_string(),
            ),
            Some(store) => {
                let matches = store.destination == AppStoreDestination::Testflight
                    && matches!(store.testflight_groups.as_slice(), [group] if group.trim() == spec.group);
                if !matches {
                    unsupported.push(
                        "publish.testflight: the pipeline's App Store Connect settings upload elsewhere; change the TestFlight group in the Publish settings".to_string(),
                    );
                }
            }
        }
        testflight = Some(spec);
    }

    // Triggers.
    let file_on = match config.triggers(None) {
        Some(spec) => {
            if matches!(spec.pull_request, Some(BranchTrigger::Branches(_))) {
                unsupported.push(
                    "on.pull_request.branches matches the branch a pull request targets, while the trigger settings' branch filter matches its source branch".to_string(),
                );
                None
            } else {
                let mut on = normalized_on(on_of(spec));
                if on.push.is_none() && !on.pull_request && !on.paths.is_empty() {
                    notes.push("on.paths only narrows push and pull request events, so it has no effect here and is dropped".to_string());
                    on.paths.clear();
                }
                Some(on)
            }
        }
        None => {
            if context.triggers.is_some_and(|t| map_triggers(t).is_ok()) {
                notes.push(
                    "Without `on`, the pipeline's trigger settings decide which webhooks start builds, as they do now; the generated file writes them back as `on`".to_string(),
                );
            }
            None
        }
    };
    let mut triggers = None;
    if let Some(on) = &file_on {
        match context.triggers {
            None => unsupported.push(
                "on: this repository only builds manually, so the form has no trigger settings for it".to_string(),
            ),
            Some(base) => match unmap_triggers(on, base) {
                Ok(config) => {
                    if !trimmed(&base.paths_ignore).is_empty() && config.paths_ignore.is_empty() {
                        notes.push(
                            "The trigger settings' ignored paths (paths_ignore) are removed: `on` cannot ignore paths, so push and pull request builds no longer skip those changes".to_string(),
                        );
                    }
                    triggers = Some(config);
                }
                Err(reason) => unsupported.push(format!("on: {reason}")),
            },
        }
    }
    if let Some(config) = &triggers {
        for (field, patterns) in [("branches", &config.branches), ("tag", &config.tags)] {
            if patterns.iter().any(|pattern| pattern.contains(',')) {
                unsupported.push(format!(
                    "on: a {field} glob contains a comma, which the form uses to separate globs"
                ));
            }
        }
    }

    if !unsupported.is_empty() {
        return Ok(FormImport {
            play,
            testflight,
            unsupported,
            notes,
            ..FormImport::default()
        });
    }

    let execution = PipelineExecutionConfig {
        platforms: plan.platforms.clone(),
        flutter_version: keep_base(
            plan.flutter_version.clone(),
            context.execution.map(|base| base.flutter_version.clone()),
            |a, b| {
                a.as_deref().map(str::trim).filter(|v| !v.is_empty())
                    == b.as_deref().map(str::trim).filter(|v| !v.is_empty())
            },
        ),
        commands: PipelineCommandStages {
            pre_build,
            build: build_commands,
            post_build,
        },
        platform_build_args,
        platform_commands,
        env: keep_base(
            plan.env.clone(),
            context.execution.map(|base| base.env.clone()),
            |a, b| trimmed_env(a) == trimmed_env(b),
        ),
        artifact_patterns: import_artifacts(&plan, context.execution),
        build_number_offset: plan.build_number_offset,
        runs_on: keep_base(
            plan.runs_on.clone(),
            context.execution.map(|base| base.runs_on.clone()),
            |a, b| normalize_runner_labels(a).ok() == normalize_runner_labels(b).ok(),
        ),
        isolation: plan.isolation,
    };

    // Prove it: the settings, exported again, plan like the file.
    let check = |error: String| FormImport {
        play: play.clone(),
        testflight: testflight.clone(),
        unsupported: vec![error],
        notes: notes.clone(),
        ..FormImport::default()
    };
    let export = match export_form_settings(FormSettings {
        execution: &execution,
        triggers: triggers.as_ref().or(context.triggers),
        google_play: context.google_play,
        app_store: context.app_store,
    }) {
        Ok(export) => export,
        Err(error) => return Ok(check(format!("The form cannot hold this file: {error}"))),
    };
    let regenerated = match parse_repository_config(&export.yaml) {
        Ok(RepositoryConfig::V2(regenerated)) => regenerated,
        _ => {
            return Ok(check(
                "The form cannot hold this file: its settings do not export; please report this"
                    .to_string(),
            ));
        }
    };
    let replan = match regenerated.plan(None, None) {
        Ok(replan) => replan,
        Err(error) => {
            return Ok(check(format!(
                "The form cannot hold this file: its settings do not plan; please report this.\n{error}"
            )));
        }
    };
    if let Some(error) = differences(&plan, &replan, file_on.as_ref(), &regenerated) {
        return Ok(check(format!("The form cannot hold this file: {error}")));
    }
    let renamed: Vec<String> = plan
        .steps
        .iter()
        .zip(&replan.steps)
        .filter(|(file, form)| file.name != form.name)
        .map(|(file, form)| format!("'{}' becomes '{}'", file.name, form.name))
        .collect();
    if !renamed.is_empty() {
        notes.push(format!(
            "Step names are not kept: the form names steps after their commands ({})",
            renamed.join(", ")
        ));
    }

    Ok(FormImport {
        execution: Some(execution),
        triggers,
        play,
        testflight,
        unsupported: Vec::new(),
        notes,
    })
}

fn trimmed(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn trimmed_env(env: &[PipelineEnvVar]) -> Vec<(String, String)> {
    env.iter()
        .map(|entry| (entry.key.trim().to_string(), entry.value.trim().to_string()))
        .collect()
}

/// The base's value when it is equivalent to the file's, so an import does
/// not rewrite settings the file did not change.
fn keep_base<T>(file: T, base: Option<T>, same: impl Fn(&T, &T) -> bool) -> T {
    match base {
        Some(base) if same(&file, &base) => base,
        _ => file,
    }
}

/// The form's artifact list: the base's when it collects the same patterns
/// (an empty list collects the defaults), else the file's.
fn import_artifacts(plan: &PipelinePlan, base: Option<&PipelineExecutionConfig>) -> Vec<String> {
    let mut patterns: Vec<String> = Vec::new();
    for pattern in &plan.artifact_patterns {
        let pattern = pattern.trim().to_string();
        if !patterns.contains(&pattern) {
            patterns.push(pattern);
        }
    }
    if let Some(base) = base {
        let mut listed: Vec<String> = Vec::new();
        for pattern in trimmed(&base.artifact_patterns) {
            if !listed.contains(&pattern) {
                listed.push(pattern);
            }
        }
        if listed == patterns
            || (listed.is_empty() && patterns == default_artifacts(&plan.platforms))
        {
            return base.artifact_patterns.clone();
        }
    }
    patterns
}

/// The form inputs for a target's `build`: the base's when they make the
/// same command, else the default command with build arguments, else an
/// override. With `play`, the default Android build also makes the App
/// Bundle, as the runner does for form settings.
fn target_inputs(
    platform: &BuildPlatform,
    build: &str,
    play: bool,
    base: Option<&PipelineExecutionConfig>,
) -> (Option<String>, Vec<String>) {
    let render = |command: Option<&String>, args: &[String]| -> String {
        let mut commands = PlatformBuildCommands::default();
        let mut build_args = PlatformBuildArgs::default();
        match platform {
            BuildPlatform::Android => {
                commands.android = command.cloned();
                build_args.android = args.to_vec();
            }
            BuildPlatform::Ios => {
                commands.ios = command.cloned();
                build_args.ios = args.to_vec();
            }
            BuildPlatform::Macos => {
                commands.macos = command.cloned();
                build_args.macos = args.to_vec();
            }
        }
        let rendered = default_platform_command(platform, &commands, &build_args);
        if play && command.is_none() {
            format!("{rendered} && {}", default_play_bundle_command(&build_args))
        } else {
            rendered
        }
    };
    if let Some(base) = base {
        let (command, args) = platform_inputs(base, platform);
        if render(command, args).trim() == build {
            return (command.cloned(), args.to_vec());
        }
    }
    let default = render(None, &[]);
    let head = if play {
        default.split(" && ").next().unwrap_or_default().to_string()
    } else {
        default
    };
    if let Some(rest) = build.strip_prefix(&head) {
        let rest = if play {
            rest.split(" && ").next().unwrap_or_default()
        } else {
            rest
        };
        let quoted = rest.contains(['"', '\'', '\\']);
        if (rest.is_empty() || rest.starts_with(' ')) && !quoted {
            let args: Vec<String> = rest.split_whitespace().map(str::to_string).collect();
            if render(None, &args) == build {
                return (None, args);
            }
        }
    }
    (Some(build.to_string()), Vec::new())
}

/// The `publish.play` the Publish settings export to, as in the export.
fn expected_play(
    play: Option<&GooglePlayPublishConfig>,
    platforms: &[BuildPlatform],
) -> Option<PlayPublishSpec> {
    let play = play?;
    platforms
        .contains(&BuildPlatform::Android)
        .then(|| PlayPublishSpec {
            track: play.track.trim().to_string(),
            rollout: Some(
                play.rollout_percent
                    .filter(|percent| *percent < 100.0)
                    .map_or(1.0, |percent| (percent * 10_000.0).round() / 1_000_000.0),
            ),
            draft: Some(play.draft),
        })
}

/// A field the file leaves out is left to the Publish settings, so it
/// matches them.
fn same_play(file: &PlayPublishSpec, settings: &PlayPublishSpec) -> bool {
    file.track == settings.track
        && file
            .rollout
            .is_none_or(|rollout| Some(rollout) == settings.rollout)
        && file.draft.is_none_or(|draft| Some(draft) == settings.draft)
}

fn describe_play(spec: &PlayPublishSpec) -> String {
    let rollout = spec.rollout.unwrap_or(1.0);
    let mut text = format!("the {} track", spec.track);
    if rollout < 1.0 {
        text.push_str(&format!(
            " at {}%",
            crate::pipeline_config::play_rollout_percent(rollout)
        ));
    }
    if spec.draft == Some(true) {
        text.push_str(" as a draft");
    }
    text
}

fn normalized_on(on: OnSpec) -> OnSpec {
    let trim = |patterns: Vec<String>| -> Vec<String> {
        patterns
            .into_iter()
            .map(|pattern| pattern.trim().to_string())
            .collect()
    };
    OnSpec {
        push: on.push.map(trim),
        pull_request: on.pull_request,
        tag: trim(on.tag),
        paths: trim(on.paths),
    }
}

/// Trigger settings that start builds for exactly the webhooks `on` does,
/// keeping the pipeline's schedules: the base itself when it already maps to
/// `on`, else the plainest settings that do.
pub(crate) fn unmap_triggers(on: &OnSpec, base: &TriggerConfig) -> Result<TriggerConfig, String> {
    if map_triggers(base).as_ref() == Ok(on) {
        return Ok(base.clone());
    }
    const TAG_FILTER: &str = "the trigger settings' tag filter turns off branch events unless branches are listed, so list the branches under `push.branches`";
    let every_tag = on.tag == ["*"];
    let mut events: Vec<&str> = Vec::new();
    let mut branches = Vec::new();
    let mut tags = Vec::new();
    if on.pull_request {
        if on
            .push
            .as_ref()
            .is_some_and(|branches| !branches.is_empty())
        {
            return Err("with pull requests on, the trigger settings' branch filter would also filter pull requests by their source branch; use `push: true` or turn pull requests off".to_string());
        }
        if !on.tag.is_empty() && !every_tag {
            return Err(format!(
                "with pull requests on, {TAG_FILTER}, which pull requests cannot use"
            ));
        }
        if on.push.is_some() {
            events.push("push");
        }
        events.push("pull_request");
        if every_tag {
            events.push("tag_push");
        }
    } else {
        match &on.push {
            Some(listed) if !listed.is_empty() => {
                events.push("push");
                branches = listed.clone();
                if !on.tag.is_empty() {
                    events.push("tag_push");
                    tags = on.tag.clone();
                }
            }
            Some(_) => {
                if !on.tag.is_empty() && !every_tag {
                    return Err(format!("with `push: true`, {TAG_FILTER}"));
                }
                events.push("push");
                if every_tag {
                    events.push("tag_push");
                }
            }
            None if on.tag.is_empty() => {
                return Err("it starts no webhook builds, which the trigger settings cannot express; remove `on` to keep the pipeline's trigger settings".to_string());
            }
            None => {
                events.push("tag_push");
                if !every_tag {
                    tags = on.tag.clone();
                }
            }
        }
    }
    let config = TriggerConfig {
        events: events.into_iter().map(str::to_string).collect(),
        branches,
        tags,
        paths: on.paths.clone(),
        paths_ignore: Vec::new(),
        schedules: base.schedules.clone(),
    };
    if map_triggers(&config).as_ref() != Ok(on) {
        return Err(
            "the trigger settings cannot express it exactly; please report this".to_string(),
        );
    }
    Ok(config)
}

/// How the regenerated file would differ from the original, if at all.
fn differences(
    file: &PipelinePlan,
    form: &PipelinePlan,
    file_on: Option<&OnSpec>,
    regenerated: &ConfigV2,
) -> Option<String> {
    let (expected, actual) = (Behaviour::of(file), Behaviour::of(form));
    if expected != actual {
        return Some(format!(
            "it would build differently; please report this.\nfile: {expected:?}\nform: {actual:?}"
        ));
    }
    if let Some(on) = file_on {
        let actual_on = regenerated.triggers(None).map(on_of).map(normalized_on);
        if actual_on.as_ref() != Some(on) {
            return Some(format!(
                "it would start on different webhooks; please report this.\nfile: {on:?}\nform: {actual_on:?}"
            ));
        }
    }
    if let Some(play) = &file.publish.play {
        let same = form
            .publish
            .play
            .as_ref()
            .is_some_and(|form| same_play(&play.spec, &form.spec));
        if !same {
            return Some(
                "it would publish to Google Play differently; please report this".to_string(),
            );
        }
    }
    if let Some(testflight) = &file.publish.testflight {
        let same = form
            .publish
            .testflight
            .as_ref()
            .is_some_and(|form| form.spec == testflight.spec);
        if !same {
            return Some(
                "it would publish to TestFlight differently; please report this".to_string(),
            );
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GooglePlayReleaseNotesSource, ScheduleTrigger};

    fn execution(json: serde_json::Value) -> PipelineExecutionConfig {
        serde_json::from_value(json).unwrap()
    }

    fn triggers(json: serde_json::Value) -> TriggerConfig {
        serde_json::from_value(json).unwrap()
    }

    fn play(track: &str, rollout_percent: Option<f64>, draft: bool) -> GooglePlayPublishConfig {
        GooglePlayPublishConfig {
            enabled: true,
            service_account_secret: "PLAY_SERVICE_ACCOUNT_JSON".to_string(),
            package_name: Some("com.example.app".to_string()),
            track: track.to_string(),
            rollout_percent,
            draft,
            release_notes: GooglePlayReleaseNotesSource::default(),
            release_notes_locale: "en-US".to_string(),
            release_notes_dir: None,
            upload_mapping: true,
            in_app_update_priority: None,
            rollout_policy: None,
        }
    }

    fn app_store(groups: &[&str]) -> AppStorePublishConfig {
        AppStorePublishConfig {
            enabled: true,
            destination: AppStoreDestination::Testflight,
            testflight_groups: groups.iter().map(|g| g.to_string()).collect(),
            release_notes: Vec::new(),
            uses_non_exempt_encryption: false,
            submit_for_review: false,
            phased_release: false,
        }
    }

    fn import(yaml: &str) -> FormImport {
        let base = triggers(serde_json::json!({ "events": [], "branches": [] }));
        import_form_settings(
            yaml,
            ImportContext {
                triggers: Some(&base),
                ..ImportContext::default()
            },
        )
        .unwrap_or_else(|error| panic!("{error}\n{yaml}"))
    }

    fn unsupported(yaml: &str, expected: &str) {
        let import = import(yaml);
        assert!(import.execution.is_none(), "{yaml}");
        assert!(
            import
                .unsupported
                .iter()
                .any(|item| item.contains(expected)),
            "expected '{expected}' in {:?}",
            import.unsupported
        );
    }

    const TARGET: &str = "version: 2\ntargets:\n  android:\n    build: flutter build apk --release\nsteps:\n  - name: Get packages\n    run: flutter pub get\n  - build\n";

    #[test]
    fn a_hand_written_file_maps_to_the_form() {
        let yaml = "version: 2\napp:\n  flutter: '3.24.5'\ntargets:\n  android:\n    build: flutter build apk --release --split-per-abi\n  ios:\n    build: flutter build ipa --export-options-plist=ios/export.plist\n    artifacts: [build/ios/ipa/*.ipa]\nsteps:\n  - name: Get packages\n    run: flutter pub get\n  - name: Codegen\n    run: dart run build_runner build\n  - build\n  - name: Tests\n    run: flutter test\nenv:\n  SENTRY_DSN: ${{ secrets.SENTRY_DSN }}\non:\n  push:\n    branches: [main]\n  tag: 'v*'\nruns_on: [Xcode-16]\nbuild_number_offset: 7\n";
        let import = import(yaml);
        assert!(import.unsupported.is_empty(), "{:?}", import.unsupported);
        let config = import.execution.unwrap();
        assert_eq!(
            config.platforms,
            [BuildPlatform::Android, BuildPlatform::Ios]
        );
        assert_eq!(config.platform_build_args.android, ["--split-per-abi"]);
        assert_eq!(config.platform_commands.android, None);
        assert_eq!(
            config.platform_commands.ios.as_deref(),
            Some("flutter build ipa --export-options-plist=ios/export.plist")
        );
        assert_eq!(config.commands.pre_build, ["dart run build_runner build"]);
        assert_eq!(config.commands.post_build, ["flutter test"]);
        assert_eq!(config.env[0].value, "${{ secrets.SENTRY_DSN }}");
        assert_eq!(config.runs_on, ["xcode-16"]);
        assert_eq!(config.build_number_offset, 7);
        assert_eq!(config.flutter_version.as_deref(), Some("3.24.5"));
        let triggers = import.triggers.unwrap();
        assert_eq!(triggers.events, ["push", "tag_push"]);
        assert_eq!(triggers.branches, ["main"]);
        assert_eq!(triggers.tags, ["v*"]);
        // Hand-written names are not kept, and the note says so.
        assert!(
            import
                .notes
                .iter()
                .any(|note| note.contains("'Codegen' becomes")),
            "{:?}",
            import.notes
        );
        // The targets' different lists become one.
        assert!(import.notes.iter().any(|note| note.contains("one list")));
    }

    #[test]
    fn what_the_form_cannot_hold_is_listed() {
        for (yaml, expected) in [
            (
                format!("{TARGET}workflows:\n  release: {{}}\n  adhoc: {{}}\n"),
                "workflows (release, adhoc)",
            ),
            (
                "version: 2\nvars:\n  MODE: release\ntargets:\n  android:\n    build: flutter build apk --${{ vars.MODE }}\n".to_string(),
                "vars (MODE)",
            ),
            (
                "version: 2\napp:\n  flavors: [free, pro]\ntargets:\n  android:\n    build: flutter build apk --flavor ${{ flavor }}\n".to_string(),
                "app.flavors",
            ),
            (
                format!("{TARGET}  - name: Tests\n    run: flutter test --machine > report.json\n    reports:\n      tests: report.json\n"),
                "reports test results",
            ),
            (
                format!("{TARGET}  - name: Script\n    run: |\n      echo one\n      echo two\n"),
                "runs several lines",
            ),
            (
                "version: 2\ntargets:\n  android:\n    build: flutter build apk --release\n".to_string(),
                "always runs `flutter pub get`",
            ),
            (
                format!("{TARGET}env:\n  NOTE: \"line one\\nline two\"\n"),
                "env.NOTE spans several lines",
            ),
            (
                format!("{TARGET}on:\n  pull_request:\n    branches: [main]\n"),
                "matches the branch a pull request targets",
            ),
            (
                format!("{TARGET}on: {{}}\n"),
                "starts no webhook builds",
            ),
            (
                format!("{TARGET}on:\n  push: true\n  tag: 'v*'\n"),
                "tag filter turns off branch events",
            ),
            (
                format!("{TARGET}on:\n  push:\n    branches: [main]\n  pull_request: true\n"),
                "source branch",
            ),
            (
                format!("{TARGET}on:\n  push:\n    branches: ['release/{{a,b}}']\n"),
                "contains a comma",
            ),
            (
                "version: 2\ntargets:\n  android:\n    build: flutter build appbundle --release\n    publish:\n      play: { track: internal }\nsteps:\n  - name: Get packages\n    run: flutter pub get\n  - build\n".to_string(),
                "turn on Google Play upload",
            ),
            (
                "version: 2\ntargets:\n  ios:\n    build: flutter build ipa\n    publish:\n      testflight: { group: QA }\nsteps:\n  - name: Get packages\n    run: flutter pub get\n  - build\n".to_string(),
                "turn on App Store Connect upload",
            ),
            (
                "version: 1\nplatforms: [android]\n".to_string(),
                "version 1 file",
            ),
        ] {
            unsupported(&yaml, expected);
        }
    }

    #[test]
    fn targets_out_of_the_form_order_are_unsupported() {
        let yaml = "version: 2\ntargets:\n  ios:\n    build: flutter build ios --release --no-codesign\n  android:\n    build: flutter build apk --release\nsteps:\n  - name: Get packages\n    run: flutter pub get\n  - build\n";
        unsupported(yaml, "in the order android, ios");
        let ordered = yaml.replace("  ios:\n    build: flutter build ios --release --no-codesign\n  android:\n    build: flutter build apk --release\n", "  android:\n    build: flutter build apk --release\n  ios:\n    build: flutter build ios --release --no-codesign\n");
        assert!(import(&ordered).execution.is_some());
    }

    #[test]
    fn deleted_build_commands_need_no_note() {
        let config = execution(serde_json::json!({
            "platforms": ["android"],
            "commands": { "pre_build": [], "build": ["./scripts/sign.sh"], "post_build": [] },
            "artifact_patterns": ["build/app/outputs/flutter-apk/*.apk"],
        }));
        let context = ImportContext {
            execution: Some(&config),
            ..ImportContext::default()
        };
        let import = import_form_settings(TARGET, context).unwrap();
        let imported = import.execution.unwrap();
        assert!(imported.commands.build.is_empty());
        assert!(
            import
                .notes
                .iter()
                .all(|note| !note.contains("Build commands")),
            "{:?}",
            import.notes
        );
    }

    #[test]
    fn a_new_on_says_ignored_paths_are_removed() {
        let base = triggers(
            serde_json::json!({ "events": ["push"], "branches": [], "paths_ignore": ["docs/**"] }),
        );
        let import = import_form_settings(
            &format!("{TARGET}on:\n  push: true\n"),
            ImportContext {
                triggers: Some(&base),
                ..ImportContext::default()
            },
        )
        .unwrap();
        assert!(import.triggers.unwrap().paths_ignore.is_empty());
        assert!(
            import
                .notes
                .iter()
                .any(|note| note.contains("paths_ignore")),
            "{:?}",
            import.notes
        );
    }

    #[test]
    fn invalid_files_return_the_validator_errors() {
        let error = import_form_settings(
            "version: 2\ntargets:\n  android:\n    bild: x\n",
            ImportContext::default(),
        )
        .unwrap_err();
        assert!(error.contains("bild"), "{error}");
        assert!(import_form_settings("version: [", ImportContext::default()).is_err());
    }

    #[test]
    fn on_without_trigger_settings_is_unsupported() {
        let yaml = format!("{TARGET}on:\n  push: true\n");
        let import = import_form_settings(&yaml, ImportContext::default()).unwrap();
        assert!(
            import
                .unsupported
                .iter()
                .any(|item| item.contains("only builds manually")),
            "{:?}",
            import.unsupported
        );
        // Without `on`, the trigger settings stay as they are.
        let import = import_form_settings(TARGET, ImportContext::default()).unwrap();
        assert!(import.unsupported.is_empty(), "{:?}", import.unsupported);
        assert!(import.triggers.is_none());
    }

    #[test]
    fn publish_must_match_the_publish_settings() {
        let config = execution(serde_json::json!({
            "platforms": ["android"],
            "artifact_patterns": ["build/app/outputs/flutter-apk/*.apk"],
        }));
        let upload = play("production", Some(10.0), false);
        let context = ImportContext {
            execution: Some(&config),
            triggers: None,
            google_play: Some(&upload),
            app_store: None,
        };
        let yaml = export_form_settings(FormSettings {
            execution: &config,
            triggers: None,
            google_play: Some(&upload),
            app_store: None,
        })
        .unwrap()
        .yaml;
        let import = import_form_settings(&yaml, context).unwrap();
        assert!(import.unsupported.is_empty(), "{:?}", import.unsupported);
        let spec = import.play.unwrap();
        assert_eq!(
            (spec.track.as_str(), spec.rollout),
            ("production", Some(0.1))
        );
        // The default build with the bundle Play needs reads back as no override.
        assert_eq!(import.execution.unwrap().platform_commands.android, None);

        // A different track is the Publish settings' to change.
        let changed = yaml.replace("track: production", "track: beta");
        let import = import_form_settings(&changed, context).unwrap();
        assert!(import.execution.is_none());
        assert!(
            import.unsupported[0].contains("production track at 10%"),
            "{:?}",
            import.unsupported
        );
        assert_eq!(import.play.unwrap().track, "beta");

        // Leaving rollout out leaves it to the settings, which matches.
        let unset = yaml.replace("        rollout: 0.1\n", "");
        assert!(
            import_form_settings(&unset, context)
                .unwrap()
                .execution
                .is_some()
        );

        // Credentials never come from the file.
        assert!(!yaml.contains("PLAY_SERVICE_ACCOUNT_JSON"));
    }

    #[test]
    fn testflight_groups_must_match() {
        let config = execution(serde_json::json!({
            "platforms": ["ios"],
            "artifact_patterns": ["build/ios/ipa/*.ipa"],
        }));
        let yaml = "version: 2\ntargets:\n  ios:\n    build: flutter build ios --release --no-codesign\n    artifacts: [build/ios/ipa/*.ipa]\n    publish:\n      testflight: { group: QA }\nsteps:\n  - name: Get packages\n    run: flutter pub get\n  - build\n";
        for (groups, ok) in [
            (&["QA"][..], true),
            (&["Beta"][..], false),
            (&["QA", "Beta"][..], false),
        ] {
            let store = app_store(groups);
            let import = import_form_settings(
                yaml,
                ImportContext {
                    execution: Some(&config),
                    app_store: Some(&store),
                    ..ImportContext::default()
                },
            )
            .unwrap();
            assert_eq!(
                import.execution.is_some(),
                ok,
                "{groups:?}: {:?}",
                import.unsupported
            );
            assert_eq!(import.testflight.unwrap().group, "QA");
        }
    }

    #[test]
    fn build_commands_stay_build_commands_while_the_file_keeps_them() {
        let config = execution(serde_json::json!({
            "platforms": ["android"],
            "commands": { "pre_build": [], "build": ["./scripts/sign.sh"], "post_build": ["flutter test"] },
            "artifact_patterns": ["build/**/*.apk"],
        }));
        let yaml = export_form_settings(FormSettings {
            execution: &config,
            triggers: None,
            google_play: None,
            app_store: None,
        })
        .unwrap()
        .yaml;
        let context = ImportContext {
            execution: Some(&config),
            ..ImportContext::default()
        };
        let import = import_form_settings(&yaml, context).unwrap();
        let imported = import.execution.unwrap();
        assert_eq!(imported.commands.build, ["./scripts/sign.sh"]);
        assert_eq!(imported.commands.post_build, ["flutter test"]);

        // With a step put before them, they become what the file runs:
        // post-build steps, and the note says so.
        let edited = yaml.replace(
            "  - name: ./scripts/sign.sh\n",
            "  - name: Analyze\n    run: flutter analyze\n  - name: ./scripts/sign.sh\n",
        );
        let import = import_form_settings(&edited, context).unwrap();
        let imported = import.execution.unwrap();
        assert!(imported.commands.build.is_empty());
        assert_eq!(
            imported.commands.post_build,
            ["flutter analyze", "./scripts/sign.sh", "flutter test"]
        );
        assert!(
            import
                .notes
                .iter()
                .any(|note| note.contains("Build commands now run after"))
        );
    }

    #[test]
    fn edits_in_the_file_reach_the_form() {
        let config = execution(serde_json::json!({
            "platforms": ["android"],
            "commands": { "pre_build": ["dart run build_runner build"], "build": [], "post_build": [] },
            "artifact_patterns": ["build/app/outputs/flutter-apk/*.apk"],
        }));
        let yaml = export_form_settings(FormSettings {
            execution: &config,
            triggers: None,
            google_play: None,
            app_store: None,
        })
        .unwrap()
        .yaml;
        let edited =
            yaml.replace(
                "build: flutter build apk --release",
                "build: flutter build apk --release --obfuscate --split-debug-info=build/symbols",
            )
            .replace(
                "  - build\n",
                "  - name: Analyze\n    run: flutter analyze\n  - build\n",
            ) + "env:\n  API_URL: https://example.com\n";
        let import = import_form_settings(
            &edited,
            ImportContext {
                execution: Some(&config),
                ..ImportContext::default()
            },
        )
        .unwrap();
        let imported = import
            .execution
            .unwrap_or_else(|| panic!("{:?}", import.unsupported));
        assert_eq!(
            imported.platform_build_args.android,
            ["--obfuscate", "--split-debug-info=build/symbols"]
        );
        assert_eq!(
            imported.commands.pre_build,
            ["dart run build_runner build", "flutter analyze"]
        );
        assert_eq!(imported.env[0].key, "API_URL");
        // The step named after its command keeps its name.
        assert!(
            import.notes.iter().all(|note| !note.contains("Step names")),
            "{:?}",
            import.notes
        );
    }

    #[test]
    fn trigger_settings_round_trip_through_on() {
        let base = TriggerConfig {
            schedules: vec![ScheduleTrigger {
                cron: "@daily".to_string(),
                branch: None,
            }],
            ..triggers(
                serde_json::json!({ "events": ["push"], "branches": [], "paths_ignore": ["docs/**"] }),
            )
        };
        for on in [
            "on:\n  push: true\n",
            "on:\n  push: true\n  pull_request: true\n  tag: '*'\n  paths: [lib/**]\n",
            "on:\n  pull_request: true\n",
            "on:\n  tag: [v*, release-*]\n",
            "on:\n  push:\n    branches: [main, 'release/*']\n  tag: '*'\n",
        ] {
            let yaml = format!("{TARGET}{on}");
            let import = import_form_settings(
                &yaml,
                ImportContext {
                    triggers: Some(&base),
                    ..ImportContext::default()
                },
            )
            .unwrap();
            assert!(
                import.unsupported.is_empty(),
                "{on}: {:?}",
                import.unsupported
            );
            let settings = import.triggers.unwrap();
            // Schedules stay with the pipeline; `on` replaces the rest.
            assert_eq!(settings.schedules, base.schedules);
            assert!(settings.paths_ignore.is_empty());
            let spec = parse_repository_config(&yaml).unwrap();
            assert_eq!(
                map_triggers(&settings).unwrap(),
                normalized_on(on_of(spec.triggers(None).unwrap())),
                "{on}"
            );
        }
    }
}
