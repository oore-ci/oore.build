//! Export a pipeline's form settings as a `version: 2` `.oore.yaml`.
//!
//! The export is pure: it takes the settings the pipeline editor holds and
//! returns the file's text. Before returning, it plans the settings the way
//! the runner does without a repository file, plans the generated file, and
//! checks that both run the same commands in the same order and phase with
//! the same inputs, as `oore pipeline migrate` does for version 1 files.
//!
//! Secret values never reach the file. Commands and env values are copied as
//! written, so a `${{ secrets.NAME }}` reference stays a reference, and the
//! Publish settings contribute only where to upload, never their credentials.

use std::collections::HashSet;

use crate::pipeline_config::{
    PipelinePlan, PlanRequest, PlayPublishSpec, RepositoryConfig, StepPhase, TestflightPublishSpec,
    TriggerSpec, default_platform_command, default_play_bundle_command, parse_repository_config,
    plan_execution_config, platform_key, platform_title,
};
use crate::pipeline_migrate::{
    Behaviour, Step, is_flow_safe, isolation_name, push_comment, push_list, push_step, scalar,
    step_name, unique_name,
};
use crate::{
    AppStoreDestination, AppStorePublishConfig, BuildIsolation, BuildPlatform,
    GooglePlayPublishConfig, PipelineExecutionConfig, TriggerConfig,
    default_artifact_patterns_for_platforms, normalize_runner_labels,
};

/// What the pipeline editor holds, as the daemon stores it.
#[derive(Debug, Clone, Copy)]
pub struct FormSettings<'a> {
    pub execution: &'a PipelineExecutionConfig,
    /// The pipeline's trigger settings. `None` leaves `on` out.
    pub triggers: Option<&'a TriggerConfig>,
    pub google_play: Option<&'a GooglePlayPublishConfig>,
    pub app_store: Option<&'a AppStorePublishConfig>,
}

/// A generated `version: 2` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormExport {
    /// The new file, ending in a newline. It parses as version 2.
    pub yaml: String,
    /// Behaviour the file cannot reproduce exactly. Each also appears as a
    /// `WARNING:` comment in [`FormExport::yaml`].
    pub warnings: Vec<String>,
    /// Settings that stay with the pipeline instead of moving to the file.
    pub notes: Vec<String>,
}

const ARTIFACTS_ANCHOR: &str = "artifacts";

/// The `on` a pipeline's trigger settings map to.
#[derive(Debug, Clone, Default, PartialEq)]
struct OnSpec {
    /// `Some(empty)` is every branch.
    push: Option<Vec<String>>,
    pull_request: bool,
    tag: Vec<String>,
    paths: Vec<String>,
}

struct Target {
    platform: BuildPlatform,
    build: String,
    comments: Vec<String>,
    play: Option<PlayPublishSpec>,
    testflight: Option<TestflightPublishSpec>,
}

/// Export form settings. Errors say why the settings cannot become a file.
pub fn export_form_settings(settings: FormSettings<'_>) -> Result<FormExport, String> {
    let config = settings.execution;
    if config.platforms.is_empty() {
        return Err("choose at least one platform to export".to_string());
    }
    for (index, platform) in config.platforms.iter().enumerate() {
        if config.platforms[..index].contains(platform) {
            return Err(format!(
                "platforms lists {} more than once",
                platform_key(platform)
            ));
        }
    }

    let mut warnings = Vec::new();
    let mut notes = Vec::new();
    let play_upload = settings.google_play.filter(|play| play.enabled);
    let app_store = settings.app_store.filter(|store| store.enabled);

    // Targets, with the platform builds the runner adds for form settings.
    let mut targets: Vec<Target> = Vec::new();
    for platform in &config.platforms {
        let (command, args) = platform_inputs(config, platform);
        let title = platform_title(platform);
        let mut build = default_platform_command(
            platform,
            &config.platform_commands,
            &config.platform_build_args,
        );
        let mut comments = vec![if command.is_some() {
            format!("The pipeline's {title} build command.")
        } else if args.is_empty() {
            format!("Oore's default {title} build command.")
        } else {
            format!(
                "Oore's default {title} build command with the pipeline's {title} build arguments."
            )
        }];
        let mut play = None;
        let mut testflight = None;
        if *platform == BuildPlatform::Android
            && let Some(upload) = play_upload
        {
            if command.is_none() {
                build = format!(
                    "{build} && {}",
                    default_play_bundle_command(&config.platform_build_args)
                );
                comments.push(
                    "The pipeline uploads to Google Play, so the App Bundle Play needs is built after the APK, as the pipeline's default Android build did."
                        .to_string(),
                );
            }
            play = Some(PlayPublishSpec {
                track: upload.track.trim().to_string(),
                // Always explicit, so the committed file keeps this rollout
                // whatever the settings hold later; unset is everyone. A
                // fraction, rounded so 33.3% is 0.333 and not 0.33299999999999996.
                rollout: Some(
                    upload
                        .rollout_percent
                        .filter(|percent| *percent < 100.0)
                        .map_or(1.0, |percent| (percent * 10_000.0).round() / 1_000_000.0),
                ),
                draft: Some(upload.draft),
            });
        }
        if *platform == BuildPlatform::Ios
            && let Some(store) = app_store
        {
            match (store.destination, store.testflight_groups.as_slice()) {
                (AppStoreDestination::Testflight, [group]) => {
                    testflight = Some(TestflightPublishSpec {
                        group: group.trim().to_string(),
                    });
                }
                (AppStoreDestination::Testflight, groups) => notes.push(format!(
                    "App Store Connect uploads stay in the pipeline's Publish settings: they name {} TestFlight groups, and publish.testflight names exactly one",
                    if groups.is_empty() { "no" } else { "several" }
                )),
                (AppStoreDestination::AppStore, _) => notes.push(
                    "App Store Connect uploads stay in the pipeline's Publish settings: they submit to the App Store, and publish.testflight only uploads to TestFlight".to_string(),
                ),
            }
        }
        targets.push(Target {
            platform: platform.clone(),
            build,
            comments,
            play,
            testflight,
        });
    }
    if play_upload.is_some() && !config.platforms.contains(&BuildPlatform::Android) {
        notes.push(
            "Google Play uploads are on, but the pipeline builds no Android target, so the file publishes nothing to Google Play".to_string(),
        );
    }
    if app_store.is_some() && !config.platforms.contains(&BuildPlatform::Ios) {
        notes.push(
            "App Store Connect uploads are on, but the pipeline builds no iOS target, so the file publishes nothing to TestFlight".to_string(),
        );
    }
    if targets
        .iter()
        .any(|t| t.play.is_some() || t.testflight.is_some())
    {
        notes.push(
            "Store credentials and release notes stay in the pipeline's Publish settings; the file's publish says where each build uploads. Keep the store's \"Upload after each build\" on: turned off later, the publish step fails".to_string(),
        );
    }

    // Artifacts: one list for the build, kept once per pattern.
    let mut artifacts: Vec<String> = Vec::new();
    for (index, pattern) in config.artifact_patterns.iter().enumerate() {
        if pattern.contains("${{") {
            return Err(format!(
                "artifact pattern {} contains `${{{{`, which version 2 reads as a reference; change the pattern before exporting",
                index + 1
            ));
        }
        let pattern = pattern.trim().to_string();
        if !pattern.is_empty() && !artifacts.contains(&pattern) {
            artifacts.push(pattern);
        }
    }
    let default_artifacts = default_artifacts(&config.platforms);
    if artifacts.is_empty() {
        let warning = "The pipeline collects no artifacts. Version 2 needs at least one pattern per target, so the file collects the platform defaults".to_string();
        warnings.push(warning);
        artifacts = default_artifacts.clone();
    }
    let explicit_artifacts = artifacts != default_artifacts;

    // Steps around the builds, in the order the runner runs them.
    let mut names: HashSet<String> = targets
        .iter()
        .map(|target| format!("Build {}", platform_title(&target.platform)))
        .collect();
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
    let mut pre_build = vec![Step {
        name: unique_name(&mut names, "Get packages"),
        run: "flutter pub get".to_string(),
        comments: vec![
            "Oore runs `flutter pub get` before the pipeline's own commands.".to_string(),
        ],
    }];
    pre_build.extend(user_steps(&mut names, &config.commands.pre_build));
    let mut after_build = user_steps(&mut names, &config.commands.build);
    if let Some(first) = after_build.first_mut() {
        first.comments.push(
            "From the pipeline's Build commands, which ran after the platform builds. Version 2 builds only through targets, so they run as steps right after them.".to_string(),
        );
        if config
            .commands
            .build
            .iter()
            .any(|command| command.contains("flutter build"))
        {
            let warning = "A Build command runs `flutter build`. As a step after the target builds it no longer gets Android signing, the iOS export settings or the dart-define file Oore adds to target builds; move that build into a target's `build` instead".to_string();
            first.comments.push(format!("WARNING: {warning}."));
            warnings.push(warning);
        }
    }
    after_build.extend(user_steps(&mut names, &config.commands.post_build));

    let on = match settings.triggers {
        Some(triggers) => match map_triggers(triggers) {
            Ok(on) => Some(on),
            Err(reason) => {
                notes.push(format!(
                    "The file has no `on`, so the pipeline's trigger settings keep deciding which webhooks start builds: {reason}"
                ));
                None
            }
        },
        None => None,
    };
    if settings.triggers.is_some_and(|t| !t.schedules.is_empty()) {
        notes.push("Schedules stay in the pipeline's trigger settings".to_string());
    }

    let runs_on = normalize_runner_labels(&config.runs_on)?;

    // Render.
    let mut out = String::new();
    for line in [
        "Generated by Oore from the pipeline's settings. It runs the same commands in the same order.",
        "Commit it as .oore.yaml at the repository root, or at the pipeline's config path. Builds then read it from the repository instead of the pipeline's Build settings.",
    ] {
        push_comment(&mut out, 0, line);
    }
    if !warnings.is_empty() {
        push_comment(
            &mut out,
            0,
            "Some settings cannot be expressed exactly; see the WARNING comments below.",
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
            push_comment(&mut out, 2, "A .fvmrc file still takes precedence.");
            out.push_str(&format!("  flutter:{}\n", scalar(version, 2)));
        }
        None => push_comment(
            &mut out,
            0,
            "No Flutter version is pinned: a .fvmrc file, else the runner's Flutter, decides. Set app.flutter to pin one.",
        ),
    }

    out.push_str("\ntargets:\n");
    let artifacts_warning = warnings.iter().any(|w| w.contains("collects no artifacts"));
    for (index, target) in targets.iter().enumerate() {
        out.push_str(&format!("  {}:\n", platform_key(&target.platform)));
        for comment in &target.comments {
            push_comment(&mut out, 4, comment);
        }
        out.push_str(&format!("    build:{}\n", scalar(&target.build, 4)));
        if artifacts_warning && index == 0 {
            push_comment(
                &mut out,
                4,
                "WARNING: The pipeline collects no artifacts; version 2 collects the platform defaults.",
            );
        }
        if explicit_artifacts {
            if targets.len() == 1 {
                out.push_str("    artifacts:\n");
                push_list(&mut out, 6, &artifacts);
            } else if index == 0 {
                push_comment(
                    &mut out,
                    4,
                    "The pipeline collects one artifact list for the whole build, so every target uses it.",
                );
                out.push_str(&format!("    artifacts: &{ARTIFACTS_ANCHOR}\n"));
                push_list(&mut out, 6, &artifacts);
            } else {
                out.push_str(&format!("    artifacts: *{ARTIFACTS_ANCHOR}\n"));
            }
        }
        if target.play.is_some() || target.testflight.is_some() {
            out.push_str("    publish:\n");
            if let Some(play) = &target.play {
                push_comment(
                    &mut out,
                    6,
                    "From the pipeline's Google Play settings, which still supply the service account and release notes.",
                );
                out.push_str("      play:\n");
                out.push_str(&format!("        track:{}\n", scalar(&play.track, 8)));
                if let Some(rollout) = play.rollout {
                    out.push_str(&format!("        rollout: {rollout}\n"));
                }
                if let Some(draft) = play.draft {
                    out.push_str(&format!("        draft: {draft}\n"));
                }
            }
            if let Some(testflight) = &target.testflight {
                push_comment(
                    &mut out,
                    6,
                    "From the pipeline's App Store Connect settings, which still supply the API key and release notes.",
                );
                out.push_str("      testflight:\n");
                out.push_str(&format!("        group:{}\n", scalar(&testflight.group, 8)));
            }
        }
    }

    out.push_str("\nsteps:\n");
    for step in &pre_build {
        push_step(&mut out, step);
    }
    push_comment(&mut out, 2, "The target builds run here.");
    out.push_str("  - build\n");
    for step in &after_build {
        push_step(&mut out, step);
    }

    if !config.env.is_empty() {
        out.push('\n');
        push_comment(
            &mut out,
            0,
            "Copied from the pipeline's environment variables. Anyone who can read the repository can read these values: keep sensitive ones in secrets and reference them as ${{ secrets.NAME }}.",
        );
        out.push_str("env:\n");
        for entry in &config.env {
            out.push_str(&format!(
                "  {}:{}\n",
                scalar(entry.key.trim(), 0).trim_start(),
                scalar(entry.value.trim(), 2)
            ));
        }
    }

    if let Some(on) = &on {
        out.push('\n');
        push_comment(
            &mut out,
            0,
            "From the pipeline's trigger settings. `on` replaces them for push, pull request and tag webhooks; schedules and manual builds still come from the pipeline.",
        );
        render_on(&mut out, on);
    }

    let mut extra = String::new();
    if config.build_number_offset != 0 {
        extra.push_str(&format!(
            "build_number_offset: {}\n",
            config.build_number_offset
        ));
    }
    if !runs_on.is_empty() {
        extra.push_str("runs_on:");
        if runs_on.iter().all(|label| is_flow_safe(label)) {
            extra.push_str(&format!(" [{}]\n", runs_on.join(", ")));
        } else {
            extra.push('\n');
            push_list(&mut extra, 2, &runs_on);
        }
    }
    if config.isolation != BuildIsolation::default() {
        extra.push_str(&format!(
            "isolation: {}\n",
            isolation_name(config.isolation)
        ));
    }
    if !extra.is_empty() {
        out.push('\n');
        out.push_str(&extra);
    }

    let export = FormExport {
        yaml: out,
        warnings,
        notes,
    };
    verify(&settings, &export, &targets, on.as_ref())?;
    Ok(export)
}

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

fn default_artifacts(platforms: &[BuildPlatform]) -> Vec<String> {
    let mut patterns: Vec<String> = Vec::new();
    for platform in platforms {
        for pattern in default_artifact_patterns_for_platforms(std::slice::from_ref(platform)) {
            if !patterns.contains(&pattern) {
                patterns.push(pattern);
            }
        }
    }
    patterns
}

// ── Triggers ────────────────────────────────────────────────────

/// The `on` that starts builds for exactly the webhooks the trigger settings
/// do, or why there is none. Without `on` the trigger settings keep applying,
/// so leaving it out never changes which builds start.
fn map_triggers(triggers: &TriggerConfig) -> Result<OnSpec, String> {
    let listed = |event: &str| {
        triggers.events.is_empty()
            || triggers
                .events
                .iter()
                .any(|listed| TriggerConfig::normalize_event(listed) == event)
    };
    let trimmed = |patterns: &[String]| -> Vec<String> {
        patterns
            .iter()
            .map(|pattern| pattern.trim().to_string())
            .filter(|pattern| !pattern.is_empty())
            .collect()
    };
    let branches = trimmed(&triggers.branches);
    let tags = trimmed(&triggers.tags);
    let paths = trimmed(&triggers.paths);
    if !trimmed(&triggers.paths_ignore).is_empty() {
        return Err("they ignore changed paths, which `on` cannot express".to_string());
    }
    if listed("pull_request") && !branches.is_empty() {
        return Err("their branch filter matches a pull request's source branch, while on.pull_request.branches matches the branch it targets".to_string());
    }
    let every_ref = branches.is_empty() && tags.is_empty();
    let push = (listed("push") && (every_ref || !branches.is_empty())).then(|| branches.clone());
    let pull_request = listed("pull_request") && every_ref;
    let tag = if !listed("tag_push") {
        Vec::new()
    } else if every_ref {
        vec!["*".to_string()]
    } else {
        tags
    };
    let paths = if push.is_some() || pull_request {
        paths
    } else {
        Vec::new()
    };
    Ok(OnSpec {
        push,
        pull_request,
        tag,
        paths,
    })
}

fn render_on(out: &mut String, on: &OnSpec) {
    if on.push.is_none() && !on.pull_request && on.tag.is_empty() {
        push_comment(out, 0, "No webhook starts a build.");
        out.push_str("on: {}\n");
        return;
    }
    out.push_str("on:\n");
    match &on.push {
        Some(branches) if branches.is_empty() => out.push_str("  push: true\n"),
        Some(branches) => {
            out.push_str("  push:\n    branches:\n");
            push_list(out, 6, branches);
        }
        None => {}
    }
    if on.pull_request {
        out.push_str("  pull_request: true\n");
    }
    if !on.tag.is_empty() {
        out.push_str("  tag:\n");
        push_list(out, 4, &on.tag);
    }
    if !on.paths.is_empty() {
        out.push_str("  paths:\n");
        push_list(out, 4, &on.paths);
    }
}

// ── Verification ────────────────────────────────────────────────

/// Plan the form settings as the runner does without a repository file and
/// plan the generated file, and require the same behaviour. The differences
/// allowed are the documented ones: the Play bundle joins the Android build,
/// the pipeline's Build commands run right after the target builds as their
/// own steps, and a pipeline without artifacts collects the defaults.
fn verify(
    settings: &FormSettings<'_>,
    export: &FormExport,
    targets: &[Target],
    on: Option<&OnSpec>,
) -> Result<(), String> {
    let generated = match parse_repository_config(&export.yaml) {
        Ok(RepositoryConfig::V2(config)) => RepositoryConfig::V2(config),
        Ok(RepositoryConfig::V1(_)) => {
            return Err("the generated file is not version 2; please report this".to_string());
        }
        Err(error) => {
            return Err(format!("the generated file does not validate:\n{error}"));
        }
    };
    let config = settings.execution;
    let play_upload = settings.google_play.is_some_and(|play| play.enabled);
    let form_plan = plan_execution_config(config, true, play_upload);
    let expected = expected_behaviour(&form_plan, config)?;
    let plan: PipelinePlan = generated.plan(PlanRequest::default())?;
    let actual = Behaviour::of(&plan);
    if expected != actual {
        return Err(format!(
            "the generated file would build differently; please report this.\nsettings: {expected:?}\nfile: {actual:?}"
        ));
    }

    let expected_play = targets.iter().find_map(|t| t.play.clone());
    let actual_play = match &plan.publish.play {
        Some(play) if play.error.is_some() => {
            return Err(format!(
                "the generated Google Play publish cannot run: {}",
                play.error.clone().unwrap_or_default()
            ));
        }
        Some(play) => Some(play.spec.clone()),
        None => None,
    };
    let expected_testflight = targets.iter().find_map(|t| t.testflight.clone());
    let actual_testflight = match &plan.publish.testflight {
        Some(testflight) if testflight.error.is_some() => {
            return Err(format!(
                "the generated TestFlight publish cannot run: {}",
                testflight.error.clone().unwrap_or_default()
            ));
        }
        Some(testflight) => Some(testflight.spec.clone()),
        None => None,
    };
    if expected_play != actual_play || expected_testflight != actual_testflight {
        return Err(format!(
            "the generated file would publish differently; please report this.\nsettings: {expected_play:?} {expected_testflight:?}\nfile: {actual_play:?} {actual_testflight:?}"
        ));
    }

    let actual_on = generated.triggers(None).map(on_of);
    if actual_on.as_ref() != on {
        return Err(format!(
            "the generated file would trigger differently; please report this.\nsettings: {on:?}\nfile: {actual_on:?}"
        ));
    }
    Ok(())
}

/// The form plan as the generated file should run it.
fn expected_behaviour(
    form_plan: &PipelinePlan,
    config: &PipelineExecutionConfig,
) -> Result<Behaviour, String> {
    let mut expected = Behaviour::of(form_plan);
    let mut steps: Vec<(StepPhase, String, Option<BuildPlatform>)> = Vec::new();
    for (phase, run, platform) in expected.steps {
        match (phase, &platform) {
            // The pipeline's Build commands run as steps after the targets.
            (StepPhase::Build, None) => steps.push((StepPhase::PostBuild, run, None)),
            // The Play bundle joins its target's build.
            (StepPhase::Build, Some(_))
                if steps
                    .last()
                    .is_some_and(|(p, _, last)| *p == StepPhase::Build && *last == platform) =>
            {
                if let Some(last) = steps.last_mut() {
                    last.1 = format!("{} && {run}", last.1);
                }
            }
            _ => steps.push((phase, run, platform)),
        }
    }
    expected.steps = steps;
    expected.runs_on = normalize_runner_labels(&config.runs_on)?;
    if expected.artifact_patterns.is_empty() {
        expected.artifact_patterns = default_artifacts(&config.platforms);
    }
    Ok(expected)
}

fn on_of(spec: &TriggerSpec) -> OnSpec {
    use crate::pipeline_config::BranchTrigger;
    let branches = |trigger: &Option<BranchTrigger>| match trigger {
        Some(BranchTrigger::AllBranches) => Some(Vec::new()),
        Some(BranchTrigger::Branches(branches)) => Some(branches.clone()),
        Some(BranchTrigger::Off) | None => None,
    };
    OnSpec {
        push: branches(&spec.push),
        pull_request: branches(&spec.pull_request).is_some(),
        tag: spec.tag.clone(),
        paths: spec.paths.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_config::TriggerEvent;
    use crate::{GooglePlayReleaseNotesSource, PipelineEnvVar, ScheduleTrigger};

    fn execution(json: serde_json::Value) -> PipelineExecutionConfig {
        serde_json::from_value(json).unwrap()
    }

    fn export(config: &PipelineExecutionConfig) -> FormExport {
        export_form_settings(FormSettings {
            execution: config,
            triggers: None,
            google_play: None,
            app_store: None,
        })
        .unwrap_or_else(|error| panic!("{error}"))
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

    fn app_store(destination: AppStoreDestination, groups: &[&str]) -> AppStorePublishConfig {
        AppStorePublishConfig {
            enabled: true,
            destination,
            testflight_groups: groups.iter().map(|g| g.to_string()).collect(),
            release_notes: Vec::new(),
            uses_non_exempt_encryption: false,
            submit_for_review: false,
            phased_release: false,
        }
    }

    fn triggers(json: serde_json::Value) -> TriggerConfig {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn plain_settings_round_trip() {
        let config = execution(serde_json::json!({
            "platforms": ["android", "ios"],
            "flutter_version": "3.24.5",
            "commands": {
                "pre_build": ["dart run build_runner build"],
                "build": [],
                "post_build": ["echo \"done\""],
            },
            "platform_build_args": { "android": ["--split-per-abi"], "ios": [], "macos": [] },
            "platform_commands": { "ios": "flutter build ipa --release" },
            "env": [{ "key": "API_URL", "value": "https://staging.example.com" }],
            "artifact_patterns": ["build/app/outputs/flutter-apk/*.apk", "build/ios/ipa/*.ipa"],
            "build_number_offset": 100,
            "runs_on": ["Xcode-16", "arm64"],
            "isolation": "vm",
        }));
        let export = export(&config);
        assert!(export.warnings.is_empty(), "{:?}", export.warnings);
        let yaml = &export.yaml;
        assert!(yaml.contains("version: 2\n"), "{yaml}");
        assert!(
            yaml.contains("build: flutter build apk --release --split-per-abi"),
            "{yaml}"
        );
        assert!(
            yaml.contains("build: flutter build ipa --release"),
            "{yaml}"
        );
        assert!(yaml.contains("runs_on: [arm64, xcode-16]"), "{yaml}");
        assert!(yaml.contains("isolation: vm"), "{yaml}");
        // The form's list differs from the v2 defaults (which add the
        // Android App Bundle), so every target names it.
        assert!(yaml.contains("artifacts: &artifacts"), "{yaml}");
        assert!(yaml.contains("artifacts: *artifacts"), "{yaml}");

        // Planned side by side, the two run the same steps.
        let form = plan_execution_config(&config, true, false);
        let file = parse_repository_config(yaml)
            .unwrap()
            .plan(PlanRequest::default())
            .unwrap();
        let runs = |plan: &PipelinePlan| -> Vec<(StepPhase, String)> {
            plan.steps
                .iter()
                .map(|step| (step.phase, step.run.clone()))
                .collect()
        };
        assert_eq!(runs(&form), runs(&file));
        assert_eq!(form.env, file.env);
        assert_eq!(file.flutter_version.as_deref(), Some("3.24.5"));
        assert_eq!(file.build_number_offset, 100);
    }

    #[test]
    fn build_commands_follow_the_targets() {
        let config = execution(serde_json::json!({
            "platforms": ["android"],
            "commands": {
                "pre_build": [],
                "build": ["./scripts/upload-symbols.sh"],
                "post_build": ["flutter test"],
            },
            "artifact_patterns": ["build/**/*.apk"],
        }));
        let export = export(&config);
        assert!(export.warnings.is_empty(), "{:?}", export.warnings);
        let file = parse_repository_config(&export.yaml)
            .unwrap()
            .plan(PlanRequest::default())
            .unwrap();
        let post: Vec<&str> = file
            .steps_in(StepPhase::PostBuild)
            .map(|step| step.run.as_str())
            .collect();
        assert_eq!(post, ["./scripts/upload-symbols.sh", "flutter test"]);
        assert_eq!(file.artifact_patterns, ["build/**/*.apk"]);

        // A `flutter build` among them is flagged.
        let mut config = config;
        config.commands.build = vec!["flutter build web".to_string()];
        let export = export_form_settings(FormSettings {
            execution: &config,
            triggers: None,
            google_play: None,
            app_store: None,
        })
        .unwrap();
        assert_eq!(export.warnings.len(), 1);
        assert!(export.yaml.contains("# WARNING:"), "{}", export.yaml);
    }

    #[test]
    fn secrets_stay_references_and_publish_credentials_stay_out() {
        let config = execution(serde_json::json!({
            "platforms": ["android", "ios"],
            "commands": {
                "pre_build": ["echo ${{ secrets.SENTRY_DSN }} > dsn.txt"],
                "build": [],
                "post_build": [],
            },
            "env": [{ "key": "SIGNING_TOKEN", "value": "${{ secrets.SIGNING_TOKEN }}" }],
            "artifact_patterns": ["build/app/outputs/flutter-apk/*.apk", "build/ios/ipa/*.ipa"],
        }));
        let play = play("production", Some(10.0), false);
        let store = app_store(AppStoreDestination::Testflight, &["QA"]);
        let export = export_form_settings(FormSettings {
            execution: &config,
            triggers: None,
            google_play: Some(&play),
            app_store: Some(&store),
        })
        .unwrap();
        let yaml = &export.yaml;
        assert!(yaml.contains("${{ secrets.SENTRY_DSN }}"), "{yaml}");
        assert!(
            yaml.contains("SIGNING_TOKEN: ${{ secrets.SIGNING_TOKEN }}"),
            "{yaml}"
        );
        assert!(!yaml.contains("PLAY_SERVICE_ACCOUNT_JSON"), "{yaml}");
        assert!(!yaml.contains("com.example.app"), "{yaml}");

        let file = parse_repository_config(yaml)
            .unwrap()
            .plan(PlanRequest::default())
            .unwrap();
        assert_eq!(file.env[0].value, "${{ secrets.SIGNING_TOKEN }}");
        let play = file.publish.play.clone().unwrap();
        assert_eq!(play.spec.track, "production");
        assert_eq!(play.spec.rollout, Some(0.1));
        assert_eq!(play.spec.draft, Some(false));
        assert_eq!(play.source, "targets.android.publish.play");
        assert_eq!(file.publish.testflight.clone().unwrap().spec.group, "QA");
        // Play needs a bundle, built after the APK as the form did.
        let android = file
            .steps_in(StepPhase::Build)
            .find(|step| step.platform == Some(BuildPlatform::Android))
            .unwrap();
        assert_eq!(
            android.run,
            "flutter build apk --release && flutter build appbundle --release"
        );
    }

    #[test]
    fn publish_that_does_not_fit_stays_in_settings() {
        let config = execution(serde_json::json!({
            "platforms": ["ios"],
            "artifact_patterns": ["build/ios/ipa/*.ipa"],
        }));
        for store in [
            app_store(AppStoreDestination::Testflight, &["QA", "Beta"]),
            app_store(AppStoreDestination::AppStore, &[]),
        ] {
            let export = export_form_settings(FormSettings {
                execution: &config,
                triggers: None,
                google_play: None,
                app_store: Some(&store),
            })
            .unwrap();
            assert!(!export.yaml.contains("publish:"), "{}", export.yaml);
            assert!(
                export
                    .notes
                    .iter()
                    .any(|note| note.contains("stay in the pipeline's Publish settings")),
                "{:?}",
                export.notes
            );
        }
        // Off settings publish nothing.
        let mut off = play("internal", None, true);
        off.enabled = false;
        let android = execution(serde_json::json!({
            "platforms": ["android"],
            "artifact_patterns": ["build/app/outputs/flutter-apk/*.apk"],
        }));
        let export = export_form_settings(FormSettings {
            execution: &android,
            triggers: None,
            google_play: Some(&off),
            app_store: None,
        })
        .unwrap();
        assert!(!export.yaml.contains("publish:"));
        assert!(!export.yaml.contains("appbundle"));
    }

    /// Every webhook a sample of refs produces starts a build under the
    /// generated `on` exactly when it does under the trigger settings.
    fn assert_same_triggers(settings: &TriggerConfig, yaml: &str) {
        let config = parse_repository_config(yaml).unwrap();
        let on = config.triggers(None).expect("the file has `on`");
        for branch in ["main", "develop", "release/1.0", "feature/x"] {
            assert_eq!(
                on.matches_ref(TriggerEvent::Push { branch }),
                settings.should_trigger("push", Some(branch), None),
                "push to {branch}"
            );
        }
        for target in ["main", "develop"] {
            // Without a branch filter the source branch does not matter.
            assert_eq!(
                on.matches_ref(TriggerEvent::PullRequest {
                    target_branch: Some(target)
                }),
                settings.should_trigger("pull_request", Some("feature/x"), None),
                "pull request into {target}"
            );
        }
        for tag in ["v1.0.0", "nightly"] {
            assert_eq!(
                on.matches_ref(TriggerEvent::Tag { tag }),
                settings.should_trigger("tag_push", None, Some(tag)),
                "tag {tag}"
            );
        }
        assert_eq!(on.has_path_filters(), settings.has_path_filters());
        for files in [
            vec!["lib/main.dart".to_string()],
            vec!["README.md".to_string()],
        ] {
            assert_eq!(
                on.paths_match(Some(&files)),
                settings.paths_match(Some(&files))
            );
        }
    }

    #[test]
    fn trigger_settings_become_on_when_they_map_exactly() {
        let config = execution(serde_json::json!({
            "platforms": ["android"],
            "artifact_patterns": ["build/app/outputs/flutter-apk/*.apk"],
        }));
        for settings in [
            triggers(serde_json::json!({ "events": [], "branches": [] })),
            triggers(serde_json::json!({ "events": ["push"], "branches": ["main", "release/*"] })),
            triggers(serde_json::json!({
                "events": ["push", "tag_push"],
                "branches": ["main"],
                "tags": ["v*"],
                "paths": ["lib/"],
            })),
            triggers(
                serde_json::json!({ "events": ["pull_request"], "branches": [], "paths": ["lib/**"] }),
            ),
            triggers(serde_json::json!({ "events": ["tag_push"], "branches": [], "tags": ["v*"] })),
            triggers(serde_json::json!({ "events": ["push"], "branches": [], "tags": ["v*"] })),
        ] {
            let export = export_form_settings(FormSettings {
                execution: &config,
                triggers: Some(&settings),
                google_play: None,
                app_store: None,
            })
            .unwrap_or_else(|error| panic!("{settings:?}: {error}"));
            assert_same_triggers(&settings, &export.yaml);
        }
    }

    #[test]
    fn trigger_settings_that_do_not_map_keep_applying() {
        let config = execution(serde_json::json!({
            "platforms": ["android"],
            "artifact_patterns": ["build/app/outputs/flutter-apk/*.apk"],
        }));
        let mut with_schedule =
            triggers(serde_json::json!({ "events": ["pull_request"], "branches": ["main"] }));
        with_schedule.schedules = vec![ScheduleTrigger {
            cron: "@daily".to_string(),
            branch: None,
        }];
        for settings in [
            with_schedule,
            triggers(
                serde_json::json!({ "events": ["push"], "branches": [], "paths_ignore": ["docs/**"] }),
            ),
        ] {
            let export = export_form_settings(FormSettings {
                execution: &config,
                triggers: Some(&settings),
                google_play: None,
                app_store: None,
            })
            .unwrap();
            assert!(
                parse_repository_config(&export.yaml)
                    .unwrap()
                    .triggers(None)
                    .is_none(),
                "{}",
                export.yaml
            );
            assert!(export.notes.iter().any(|note| note.contains("no `on`")));
        }
    }

    #[test]
    fn awkward_values_round_trip() {
        let config = PipelineExecutionConfig {
            platforms: vec![BuildPlatform::Macos],
            commands: crate::PipelineCommandStages {
                pre_build: vec![
                    "echo 'a: b' # quoted".to_string(),
                    "flutter pub get".to_string(),
                ],
                build: Vec::new(),
                post_build: Vec::new(),
            },
            env: ["yes", "Off", "~", "null", ".inf", "on"]
                .iter()
                .enumerate()
                .map(|(index, value)| PipelineEnvVar {
                    key: format!("VALUE_{index}"),
                    value: value.to_string(),
                })
                .collect(),
            artifact_patterns: Vec::new(),
            ..PipelineExecutionConfig::default()
        };
        let export = export(&config);
        // No artifacts: the file collects the defaults and says so.
        assert_eq!(export.warnings.len(), 1);
        let file = parse_repository_config(&export.yaml)
            .unwrap()
            .plan(PlanRequest::default())
            .unwrap();
        // YAML 1.1 keywords are quoted and read back as the same strings.
        assert!(export.yaml.contains("VALUE_0: 'yes'"), "{}", export.yaml);
        let values: Vec<&str> = file.env.iter().map(|entry| entry.value.as_str()).collect();
        assert_eq!(values, ["yes", "Off", "~", "null", ".inf", "on"]);
        // A duplicate step name gets a suffix.
        let names: Vec<&str> = file.steps.iter().map(|step| step.name.as_str()).collect();
        assert!(names.contains(&"Get packages (2)"), "{names:?}");
    }

    fn android_with_play(
        config: &PipelineExecutionConfig,
        upload: &GooglePlayPublishConfig,
    ) -> (FormExport, PipelinePlan) {
        let export = export_form_settings(FormSettings {
            execution: config,
            triggers: None,
            google_play: Some(upload),
            app_store: None,
        })
        .unwrap_or_else(|error| panic!("{error}"));
        let plan = parse_repository_config(&export.yaml)
            .unwrap()
            .plan(PlanRequest::default())
            .unwrap();
        (export, plan)
    }

    #[test]
    fn an_android_override_gets_no_bundle() {
        let config = execution(serde_json::json!({
            "platforms": ["android"],
            "platform_commands": { "android": "flutter build appbundle --release" },
            "artifact_patterns": ["build/app/outputs/bundle/**/*.aab"],
        }));
        let (export, plan) = android_with_play(&config, &play("internal", None, false));
        assert!(
            !export.yaml.contains("&& flutter build appbundle"),
            "{}",
            export.yaml
        );
        let builds: Vec<&str> = plan
            .steps_in(StepPhase::Build)
            .map(|step| step.run.as_str())
            .collect();
        assert_eq!(builds, ["flutter build appbundle --release"]);
        assert_eq!(plan.publish.play.unwrap().spec.track, "internal");
    }

    #[test]
    fn a_full_or_unset_rollout_is_written_as_everyone() {
        let config = execution(serde_json::json!({
            "platforms": ["android"],
            "artifact_patterns": ["build/app/outputs/flutter-apk/*.apk"],
        }));
        for rollout in [None, Some(100.0)] {
            let (export, plan) = android_with_play(&config, &play("production", rollout, false));
            assert!(
                export.yaml.contains("        rollout: 1\n"),
                "{}",
                export.yaml
            );
            assert_eq!(plan.publish.play.unwrap().spec.rollout, Some(1.0));
        }
        let (export, _) = android_with_play(&config, &play("production", Some(33.3), false));
        assert!(
            export.yaml.contains("        rollout: 0.333\n"),
            "{}",
            export.yaml
        );
    }

    #[test]
    fn targets_keep_the_form_order_and_share_one_artifact_list() {
        let config = execution(serde_json::json!({
            "platforms": ["ios", "android"],
            "artifact_patterns": ["build/ios/ipa/*.ipa", "build/app/outputs/flutter-apk/*.apk"],
        }));
        let export = export(&config);
        let ios = export.yaml.find("  ios:\n").unwrap();
        let android = export.yaml.find("  android:\n").unwrap();
        assert!(ios < android, "{}", export.yaml);
        let anchor = export.yaml.find("artifacts: &artifacts").unwrap();
        let alias = export.yaml.find("artifacts: *artifacts").unwrap();
        assert!(
            ios < anchor && anchor < android && android < alias,
            "{}",
            export.yaml
        );
        let plan = parse_repository_config(&export.yaml)
            .unwrap()
            .plan(PlanRequest::default())
            .unwrap();
        assert_eq!(plan.platforms, [BuildPlatform::Ios, BuildPlatform::Android]);
        assert_eq!(
            plan.artifact_patterns,
            ["build/ios/ipa/*.ipa", "build/app/outputs/flutter-apk/*.apk"]
        );
    }

    #[test]
    fn every_event_with_a_branch_filter_keeps_the_trigger_settings() {
        let config = execution(serde_json::json!({
            "platforms": ["android"],
            "artifact_patterns": ["build/app/outputs/flutter-apk/*.apk"],
        }));
        // An empty event list includes pull requests, whose branch filter
        // matches the source branch.
        let settings = triggers(serde_json::json!({ "events": [], "branches": ["main"] }));
        let export = export_form_settings(FormSettings {
            execution: &config,
            triggers: Some(&settings),
            google_play: None,
            app_store: None,
        })
        .unwrap();
        assert!(!export.yaml.contains("\non:"), "{}", export.yaml);
        assert!(
            export
                .notes
                .iter()
                .any(|note| note.contains("source branch"))
        );
    }

    #[test]
    fn tag_pushes_without_tag_patterns_start_nothing() {
        let config = execution(serde_json::json!({
            "platforms": ["android"],
            "artifact_patterns": ["build/app/outputs/flutter-apk/*.apk"],
        }));
        let settings =
            triggers(serde_json::json!({ "events": ["tag_push"], "branches": ["main"] }));
        let export = export_form_settings(FormSettings {
            execution: &config,
            triggers: Some(&settings),
            google_play: None,
            app_store: None,
        })
        .unwrap();
        assert!(export.yaml.contains("on: {}"), "{}", export.yaml);
        assert_same_triggers(&settings, &export.yaml);
    }

    #[test]
    fn testflight_without_groups_stays_in_settings() {
        let config = execution(serde_json::json!({
            "platforms": ["ios"],
            "artifact_patterns": ["build/ios/ipa/*.ipa"],
        }));
        let store = app_store(AppStoreDestination::Testflight, &[]);
        let export = export_form_settings(FormSettings {
            execution: &config,
            triggers: None,
            google_play: None,
            app_store: Some(&store),
        })
        .unwrap();
        assert!(!export.yaml.contains("publish:"));
        assert!(
            export
                .notes
                .iter()
                .any(|note| note.contains("no TestFlight groups")),
            "{:?}",
            export.notes
        );
    }
}
