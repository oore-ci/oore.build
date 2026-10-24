//! Repository pipeline files (`.oore.yaml`).
//!
//! A file declares `version: 1` or `version: 2`. Both versions lower to one
//! [`PipelinePlan`]: the ordered, named steps a runner executes, plus the
//! build inputs around them. The runner, the CLI and the daemon only read
//! plans, so the two file versions never fork the execution path.

use std::collections::HashSet;
use std::fmt;
use std::marker::PhantomData;

use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

use crate::{
    AppStoreDestination, BuildIsolation, BuildPlatform, GooglePlayPublishConfig,
    PipelineCommandStages, PipelineEnvVar, PipelineExecutionConfig, PlatformBuildArgs,
    PlatformBuildCommands, RunnerAppStorePublishPlan, RunnerPublishPlan,
    default_artifact_patterns_for_platforms, is_valid_pipeline_env_key, normalize_runner_labels,
    parse_repository_pipeline_yaml, validate_artifact_pattern, validate_build_number_offset,
    validate_repository_config_path,
};

/// Longest step name a v2 file may use.
pub const MAX_STEP_NAME_LENGTH: usize = 100;

// ── Normalized plan ─────────────────────────────────────────────

/// Where a step sits relative to the platform builds. Android signing and the
/// iOS export seam apply to `build` steps only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepPhase {
    PreBuild,
    Build,
    PostBuild,
}

impl StepPhase {
    pub const ALL: [StepPhase; 3] = [StepPhase::PreBuild, StepPhase::Build, StepPhase::PostBuild];

    pub fn as_str(self) -> &'static str {
        match self {
            StepPhase::PreBuild => "pre_build",
            StepPhase::Build => "build",
            StepPhase::PostBuild => "post_build",
        }
    }
}

/// One command the runner executes, under the name logs and checks show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanStep {
    pub name: String,
    pub phase: StepPhase,
    pub run: String,
    /// Where the command came from in the configuration, for error messages.
    pub source: String,
    /// The target this step builds, for target build steps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<BuildPlatform>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flavor: Option<String>,
    /// Workspace-relative test report (`.json` or JUnit `.xml`) the step writes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_report: Option<String>,
}

/// What a build runs, resolved from a v1 or v2 configuration.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PipelinePlan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<String>,
    pub platforms: Vec<BuildPlatform>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flutter_version: Option<String>,
    pub steps: Vec<PlanStep>,
    pub env: Vec<PipelineEnvVar>,
    pub artifact_patterns: Vec<String>,
    pub build_number_offset: i64,
    pub runs_on: Vec<String>,
    pub isolation: BuildIsolation,
    /// Store uploads a v2 file asks for. Empty for v1 files and form settings,
    /// where the pipeline's Publish settings decide.
    #[serde(skip_serializing_if = "PlanPublish::is_empty")]
    pub publish: PlanPublish,
}

/// A store upload from a v2 file, with the field it came from. Values are
/// resolved: `${{ vars.NAME }}` references are already substituted.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlannedPublish<T> {
    #[serde(flatten)]
    pub spec: T,
    /// Such as `workflows.release.targets.android.publish.play`.
    pub source: String,
    /// Why this upload cannot run. The build still runs and keeps its
    /// artifacts; only its publish step fails, with this message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Where a v2 workflow publishes, after workflow overrides and the run's
/// platform selection. A store that is set here replaces the pipeline's
/// Publish settings for that store; credentials still come from them.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PlanPublish {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play: Option<PlannedPublish<PlayPublishSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub testflight: Option<PlannedPublish<TestflightPublishSpec>>,
}

impl PlanPublish {
    pub fn is_empty(&self) -> bool {
        self.play.is_none() && self.testflight.is_none()
    }

    /// One line per upload, for plan output.
    pub fn describe(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(play) = &self.play {
            lines.push(match &play.error {
                Some(error) => format!(
                    "android publishes to Google Play ({}), but its publish step will fail: {error}",
                    play.source
                ),
                None => {
                    let spec = &play.spec;
                    let draft = spec.draft == Some(true);
                    let mut release = match spec.rollout {
                        _ if draft => " as a draft".to_string(),
                        Some(rollout) if rollout < 1.0 => {
                            format!(" to {}% of users", play_rollout_percent(rollout))
                        }
                        Some(_) => " to all users".to_string(),
                        None => String::new(),
                    };
                    let inherited: Vec<&str> = [
                        (spec.rollout.is_none() && !draft, "rollout"),
                        (spec.draft.is_none(), "draft setting"),
                    ]
                    .into_iter()
                    .filter_map(|(inherited, name)| inherited.then_some(name))
                    .collect();
                    if !inherited.is_empty() {
                        release.push_str(&format!(
                            " with the {} from the pipeline's Google Play settings",
                            inherited.join(" and ")
                        ));
                    }
                    format!(
                        "android publishes to the Google Play {} track{release} ({})",
                        spec.track, play.source
                    )
                }
            });
        }
        if let Some(testflight) = &self.testflight {
            lines.push(match &testflight.error {
                Some(error) => format!(
                    "ios publishes to TestFlight ({}), but its publish step will fail: {error}",
                    testflight.source
                ),
                None => format!(
                    "ios publishes to the TestFlight group {} ({})",
                    testflight.spec.group, testflight.source
                ),
            });
        }
        lines
    }

    /// Settings the Publish settings would reject, which the upload adjusts
    /// instead of failing the build, as `oore pipeline validate` notes.
    fn adjustment_notes(&self) -> Vec<String> {
        let Some(play) = self.play.as_ref().filter(|play| play.error.is_none()) else {
            return Vec::new();
        };
        let staged = play.spec.rollout.is_some_and(|rollout| rollout < 1.0);
        if staged && play.spec.draft == Some(true) {
            vec![format!(
                "{}: a draft has no staged rollout, so `rollout` is ignored",
                play.source
            )]
        } else if staged && play.spec.track == "internal" {
            vec![format!(
                "{}: the internal track has no staged rollouts, so the release goes to every internal tester",
                play.source
            )]
        } else {
            Vec::new()
        }
    }
}

/// The store uploads a build runs once a v2 file's `publish` is applied to
/// the pipeline's Publish settings.
#[derive(Debug, Clone)]
pub struct EffectivePublish {
    pub changelog: Option<String>,
    /// `Err` says why the upload cannot run, such as the pipeline settings
    /// to turn on.
    pub google_play: Result<Option<GooglePlayPublishConfig>, String>,
    pub app_store: Result<Option<RunnerAppStorePublishPlan>, String>,
    /// Adjustments made to fit Play's rules, for the publish log.
    pub notes: Vec<String>,
}

impl PlanPublish {
    /// Apply the file's uploads to the pipeline's Publish settings, as the
    /// runner fetched them. A store the file names replaces where the
    /// settings upload to and turns the upload on; `rollout` and `draft` the
    /// file leaves out keep the settings' values. The settings still supply
    /// credentials, the expected package, release notes and rollout policy,
    /// and must be on. Stores the file does not name keep the settings as
    /// they are.
    pub fn apply(&self, settings: RunnerPublishPlan) -> EffectivePublish {
        let RunnerPublishPlan {
            changelog,
            google_play,
            app_store,
        } = settings;
        let mut notes = Vec::new();
        let google_play = match &self.play {
            None => Ok(google_play),
            Some(PlannedPublish {
                error: Some(error), ..
            }) => Err(error.clone()),
            Some(play) => match google_play {
                Some(mut config) => {
                    config.enabled = true;
                    config.track = play.spec.track.clone();
                    if let Some(rollout) = play.spec.rollout {
                        config.rollout_percent =
                            Some(play_rollout_percent(rollout)).filter(|percent| *percent < 100.0);
                    }
                    if let Some(draft) = play.spec.draft {
                        config.draft = draft;
                    }
                    // The rules the settings form enforces, applied to the
                    // combination instead of failing the upload.
                    if let Some(percent) = config.rollout_percent.filter(|p| *p < 100.0) {
                        if config.draft {
                            notes.push(format!(
                                "A draft has no staged rollout; ignoring the {percent}% rollout"
                            ));
                            config.rollout_percent = None;
                        } else if config.track == "internal" {
                            notes.push(format!(
                                "The internal track has no staged rollouts; releasing to every internal tester instead of {percent}%"
                            ));
                            config.rollout_percent = None;
                        }
                    }
                    Ok(Some(config))
                }
                None => Err(format!(
                    "{} publishes with the pipeline's Google Play settings, which are off. In the pipeline's Publish settings, turn on Google Play's \"Upload after each build\" and set its service account secret; the file's track, rollout and draft replace the settings' own",
                    play.source
                )),
            },
        };
        let app_store = match &self.testflight {
            None => Ok(app_store),
            Some(PlannedPublish {
                error: Some(error), ..
            }) => Err(error.clone()),
            Some(testflight) => match app_store {
                Some(mut plan) => {
                    plan.config.enabled = true;
                    plan.config.destination = AppStoreDestination::Testflight;
                    plan.config.testflight_groups = vec![testflight.spec.group.clone()];
                    // App Store review settings don't apply to TestFlight.
                    plan.config.submit_for_review = false;
                    plan.config.phased_release = false;
                    Ok(Some(plan))
                }
                None => Err(format!(
                    "{} publishes with the pipeline's App Store Connect settings, which are not set up. In the pipeline's Publish settings, turn on App Store Connect, and in its iOS signing settings export App Store IPAs with an App Store Connect API key; the file's group replaces the settings' destination and groups",
                    testflight.source
                )),
            },
        };
        EffectivePublish {
            changelog,
            google_play,
            app_store,
            notes,
        }
    }
}

/// A `publish.play.rollout` fraction as the percent Play settings use,
/// rounded so 0.07 is 7 and not 7.000000000000001.
pub fn play_rollout_percent(rollout: f64) -> f64 {
    (rollout * 100.0 * 1_000_000.0).round() / 1_000_000.0
}

/// Longest TestFlight group name, as the App Store publish settings allow.
pub const MAX_TESTFLIGHT_GROUP_LENGTH: usize = 128;

impl PipelinePlan {
    pub fn steps_in(&self, phase: StepPhase) -> impl Iterator<Item = &PlanStep> {
        self.steps.iter().filter(move |step| step.phase == phase)
    }

    pub fn commands(&self, phase: StepPhase) -> Vec<String> {
        self.steps_in(phase).map(|step| step.run.clone()).collect()
    }

    /// The plan in the v1 execution shape, for previews that predate named
    /// steps. Step names and test report paths are dropped; a target built for
    /// several flavors becomes one `&&`-joined platform command.
    pub fn to_execution_config(&self) -> PipelineExecutionConfig {
        let platform_command = |platform: BuildPlatform| {
            let runs: Vec<&str> = self
                .steps_in(StepPhase::Build)
                .filter(|step| step.platform == Some(platform.clone()))
                .map(|step| step.run.as_str())
                .collect();
            (!runs.is_empty()).then(|| runs.join(" && "))
        };
        PipelineExecutionConfig {
            platforms: self.platforms.clone(),
            flutter_version: self.flutter_version.clone(),
            commands: PipelineCommandStages {
                pre_build: self.commands(StepPhase::PreBuild),
                build: self
                    .steps_in(StepPhase::Build)
                    .filter(|step| step.platform.is_none())
                    .map(|step| step.run.clone())
                    .collect(),
                post_build: self.commands(StepPhase::PostBuild),
            },
            platform_build_args: PlatformBuildArgs::default(),
            platform_commands: PlatformBuildCommands {
                android: platform_command(BuildPlatform::Android),
                ios: platform_command(BuildPlatform::Ios),
                macos: platform_command(BuildPlatform::Macos),
            },
            env: self.env.clone(),
            artifact_patterns: self.artifact_patterns.clone(),
            build_number_offset: self.build_number_offset,
            runs_on: self.runs_on.clone(),
            isolation: self.isolation,
        }
    }
}

/// What a caller wants planned from a configuration.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlanRequest<'a> {
    /// The v2 workflow to plan. `None` picks the only workflow, or the
    /// top level when the file defines no workflows.
    pub workflow: Option<&'a str>,
    /// The run's platform selection. `None` builds every configured target.
    pub selected_platforms: Option<&'a [BuildPlatform]>,
    /// v1 only: a Play upload is configured, so the default Android build also
    /// produces an App Bundle.
    pub build_play_bundle: bool,
}

// ── Version detection ───────────────────────────────────────────

/// A parsed and validated repository pipeline file.
#[derive(Debug, Clone)]
pub enum RepositoryConfig {
    V1(Box<PipelineExecutionConfig>),
    V2(Box<ConfigV2>),
}

/// Parse `.oore.yaml` of either version. Errors are one per line.
pub fn parse_repository_config(raw: &str) -> Result<RepositoryConfig, String> {
    // Only `version` is read here, so errors elsewhere in the file (a
    // duplicate key, a wrong type) are left for the versioned parser.
    #[derive(Deserialize)]
    struct VersionProbe {
        #[serde(default)]
        version: Option<serde_yaml::Value>,
    }
    let declared = serde_yaml::from_str::<VersionProbe>(raw)
        .ok()
        .and_then(|probe| probe.version);
    match &declared {
        Some(serde_yaml::Value::String(text)) if matches!(text.trim(), "1" | "2") => {
            return Err(format!(
                "version must be a number; write `version: {}` without quotes",
                text.trim()
            ));
        }
        Some(serde_yaml::Value::Number(number))
            if number.as_f64().is_some_and(|v| v == 1.0 || v == 2.0)
                && number.as_u64().is_none() =>
        {
            return Err(format!(
                "version must be a whole number; write `version: {}`",
                number.as_f64().unwrap_or_default() as u64
            ));
        }
        _ => {}
    }
    let version = declared.as_ref().and_then(serde_yaml::Value::as_u64);
    match version {
        Some(2) => ConfigV2::parse(raw).map(|config| RepositoryConfig::V2(Box::new(config))),
        // A missing or malformed version gets v1's own error.
        Some(1) | None => {
            parse_repository_pipeline_yaml(raw).map(|config| RepositoryConfig::V1(Box::new(config)))
        }
        Some(other) => Err(format!(
            "unsupported config version {other}, expected 1 or 2"
        )),
    }
}

impl RepositoryConfig {
    pub fn version(&self) -> u32 {
        match self {
            RepositoryConfig::V1(_) => 1,
            RepositoryConfig::V2(_) => 2,
        }
    }

    /// Runner labels the file asks for, already normalized.
    pub fn runs_on(&self) -> &[String] {
        match self {
            RepositoryConfig::V1(config) => &config.runs_on,
            RepositoryConfig::V2(config) => &config.runs_on,
        }
    }

    /// Workflow names a plan can be asked for, in file order.
    pub fn workflow_names(&self) -> Vec<&str> {
        match self {
            RepositoryConfig::V1(_) => Vec::new(),
            RepositoryConfig::V2(config) => config.workflow_names(),
        }
    }

    pub fn plan(&self, request: PlanRequest<'_>) -> Result<PipelinePlan, String> {
        match self {
            RepositoryConfig::V1(config) => {
                if let Some(workflow) = request.workflow {
                    return Err(format!(
                        "workflow '{workflow}' was requested, but workflows need `version: 2`"
                    ));
                }
                let config = match request.selected_platforms {
                    Some(selected) => select_v1_platforms((**config).clone(), selected)?,
                    None => (**config).clone(),
                };
                let include_defaults = config.commands.build.is_empty();
                Ok(plan_execution_config(
                    &config,
                    include_defaults,
                    request.build_play_bundle,
                ))
            }
            RepositoryConfig::V2(config) => {
                config.plan(request.workflow, request.selected_platforms)
            }
        }
    }

    /// The `on` that decides when a pipeline building `workflow` runs: the
    /// workflow's own `on`, else the top-level one. `None` for a v1 file or a
    /// file without `on`, where the pipeline's trigger settings apply.
    pub fn triggers(&self, workflow: Option<&str>) -> Option<&TriggerSpec> {
        match self {
            RepositoryConfig::V1(_) => None,
            RepositoryConfig::V2(config) => config.triggers(workflow),
        }
    }

    /// Declared settings this release parses and validates but does not act on.
    pub fn notices(&self) -> Vec<String> {
        match self {
            RepositoryConfig::V1(_) => Vec::new(),
            RepositoryConfig::V2(config) => config.notices(),
        }
    }
}

// ── v1 lowering ─────────────────────────────────────────────────

/// Narrow a v1 configuration to a run's platform selection.
pub fn select_v1_platforms(
    mut config: PipelineExecutionConfig,
    selected: &[BuildPlatform],
) -> Result<PipelineExecutionConfig, String> {
    if selected.is_empty() {
        return Err("selected_platforms must include at least one target".to_string());
    }
    let has_duplicate = selected
        .iter()
        .enumerate()
        .any(|(index, platform)| selected[..index].contains(platform));
    if has_duplicate
        || selected
            .iter()
            .any(|platform| !config.platforms.contains(platform))
    {
        return Err("selected_platforms must be unique and configured by the workflow".to_string());
    }
    if selected.len() < config.platforms.len() && !config.commands.build.is_empty() {
        return Err(
            "Per-run platform selection requires platform_commands or default platform commands; shared commands.build cannot be filtered safely"
                .to_string(),
        );
    }
    config.platforms = selected.to_vec();
    Ok(config)
}

fn build_default_command_with_args(base: &str, args: &[String]) -> String {
    if args.is_empty() {
        return base.to_string();
    }
    format!("{base} {}", args.join(" "))
}

fn default_platform_command(
    platform: &BuildPlatform,
    overrides: &PlatformBuildCommands,
    args: &PlatformBuildArgs,
) -> String {
    match platform {
        BuildPlatform::Android => overrides.android.clone().unwrap_or_else(|| {
            build_default_command_with_args("flutter build apk --release", &args.android)
        }),
        BuildPlatform::Ios => overrides.ios.clone().unwrap_or_else(|| {
            build_default_command_with_args("flutter build ios --release --no-codesign", &args.ios)
        }),
        BuildPlatform::Macos => overrides.macos.clone().unwrap_or_else(|| {
            build_default_command_with_args("flutter build macos --release", &args.macos)
        }),
    }
}

/// Google Play only accepts App Bundles. When the pipeline uploads to Play
/// and keeps the default Android command, build a bundle after the APK so
/// the upload has something to send and QA installs still get an APK.
fn default_play_bundle_command(args: &PlatformBuildArgs) -> String {
    // `--split-per-abi` is APK-only; `flutter build appbundle` rejects it.
    let args = args
        .android
        .iter()
        .filter(|arg| arg.as_str() != "--split-per-abi")
        .cloned()
        .collect::<Vec<_>>();
    build_default_command_with_args("flutter build appbundle --release", &args)
}

/// Lower a v1 (or UI) execution configuration. With
/// `include_default_platform_commands`, `flutter pub get` and one build
/// command per platform are added, as Oore always has. Steps are named
/// `<stage>-<n>`, the names v1 builds have always logged. A step's source
/// points at what the user wrote: `commands.<stage>[i]` for their own
/// commands, `platform_commands.<platform>` for an override, and `default`
/// for commands Oore adds.
pub fn plan_execution_config(
    config: &PipelineExecutionConfig,
    include_default_platform_commands: bool,
    build_play_bundle: bool,
) -> PipelinePlan {
    // (command, source, platform)
    type Lowered = Vec<(String, String, Option<BuildPlatform>)>;
    let user = |stage: &str, commands: &[String]| -> Lowered {
        commands
            .iter()
            .enumerate()
            .map(|(index, run)| (run.clone(), format!("commands.{stage}[{index}]"), None))
            .collect()
    };

    let mut pre_build: Lowered = Vec::new();
    if include_default_platform_commands && !config.platforms.is_empty() {
        pre_build.push(("flutter pub get".to_string(), "default".to_string(), None));
    }
    pre_build.extend(user("pre_build", &config.commands.pre_build));

    let mut build: Lowered = Vec::new();
    if include_default_platform_commands {
        for platform in &config.platforms {
            let overridden = match platform {
                BuildPlatform::Android => config.platform_commands.android.is_some(),
                BuildPlatform::Ios => config.platform_commands.ios.is_some(),
                BuildPlatform::Macos => config.platform_commands.macos.is_some(),
            };
            build.push((
                default_platform_command(
                    platform,
                    &config.platform_commands,
                    &config.platform_build_args,
                ),
                if overridden {
                    format!("platform_commands.{}", platform_key(platform))
                } else {
                    "default".to_string()
                },
                Some(platform.clone()),
            ));
            if build_play_bundle && *platform == BuildPlatform::Android && !overridden {
                build.push((
                    default_play_bundle_command(&config.platform_build_args),
                    "default".to_string(),
                    Some(BuildPlatform::Android),
                ));
            }
        }
    }
    build.extend(user("build", &config.commands.build));
    let post_build = user("post_build", &config.commands.post_build);

    let mut steps = Vec::new();
    for (phase, commands) in [
        (StepPhase::PreBuild, pre_build),
        (StepPhase::Build, build),
        (StepPhase::PostBuild, post_build),
    ] {
        let stage = phase.as_str();
        for (index, (run, source, platform)) in commands.into_iter().enumerate() {
            steps.push(PlanStep {
                name: format!("{stage}-{}", index + 1),
                phase,
                run,
                source,
                platform,
                flavor: None,
                test_report: None,
            });
        }
    }

    PipelinePlan {
        workflow: None,
        platforms: config.platforms.clone(),
        flutter_version: config.flutter_version.clone(),
        steps,
        env: config.env.clone(),
        artifact_patterns: config.artifact_patterns.clone(),
        build_number_offset: config.build_number_offset,
        runs_on: config.runs_on.clone(),
        isolation: config.isolation,
        publish: PlanPublish::default(),
    }
}

// ── v2 schema ───────────────────────────────────────────────────

/// A mapping that keeps file order and rejects duplicate keys.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderedMap<K, V>(pub Vec<(K, V)>);

impl<K, V> Default for OrderedMap<K, V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<K: PartialEq, V> OrderedMap<K, V> {
    pub fn get(&self, key: &K) -> Option<&V> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.0.iter().map(|(k, v)| (k, v))
    }
}

trait KeyName {
    fn key_name(&self) -> String;
}

impl KeyName for String {
    fn key_name(&self) -> String {
        self.clone()
    }
}

impl KeyName for BuildPlatform {
    fn key_name(&self) -> String {
        platform_key(self).to_string()
    }
}

impl<'de, K, V> Deserialize<'de> for OrderedMap<K, V>
where
    K: Deserialize<'de> + PartialEq + KeyName,
    V: Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MapVisitor<K, V>(PhantomData<(K, V)>);

        impl<'de, K, V> Visitor<'de> for MapVisitor<K, V>
        where
            K: Deserialize<'de> + PartialEq + KeyName,
            V: Deserialize<'de>,
        {
            type Value = OrderedMap<K, V>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a mapping")
            }

            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(OrderedMap::default())
            }

            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(OrderedMap::default())
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut entries: Vec<(K, V)> = Vec::new();
                let mut seen = HashSet::new();
                while let Some(key) = map.next_key::<K>()? {
                    if !seen.insert(key.key_name()) {
                        return Err(de::Error::custom(format!(
                            "duplicate key '{}'",
                            key.key_name()
                        )));
                    }
                    let value = map.next_value::<V>()?;
                    entries.push((key, value));
                }
                Ok(OrderedMap(entries))
            }
        }

        deserializer.deserialize_any(MapVisitor(PhantomData))
    }
}

/// A `version: 2` file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigV2 {
    pub version: u32,
    #[serde(default)]
    pub app: AppSpec,
    #[serde(default)]
    pub targets: OrderedMap<BuildPlatform, TargetSpec>,
    #[serde(default)]
    pub steps: Option<Vec<StepEntry>>,
    #[serde(default)]
    pub env: OrderedMap<String, String>,
    #[serde(default)]
    pub vars: OrderedMap<String, String>,
    #[serde(default)]
    pub on: Option<TriggerSpec>,
    #[serde(default)]
    pub workflows: OrderedMap<String, WorkflowSpec>,
    #[serde(default, deserialize_with = "crate::deserialize_runs_on")]
    pub runs_on: Vec<String>,
    #[serde(default)]
    pub isolation: BuildIsolation,
    #[serde(default)]
    pub build_number_offset: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSpec {
    /// Flutter version. `.fvmrc` still wins when present.
    #[serde(default)]
    pub flutter: Option<String>,
    /// Flavor matrix. Each target builds once per flavor.
    #[serde(default)]
    pub flavors: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetSpec {
    /// The full build command. Oore adds nothing to it.
    #[serde(default)]
    pub build: Option<String>,
    #[serde(default)]
    pub artifacts: Option<Vec<String>>,
    /// `false` or `null` drops a publish a workflow target would inherit.
    #[serde(default, deserialize_with = "deserialize_target_publish")]
    pub publish: Option<TargetPublish>,
}

/// A target's `publish`: where to upload, or `false` for nowhere.
#[derive(Debug, Clone)]
pub enum TargetPublish {
    Off,
    On(PublishSpec),
}

impl TargetPublish {
    pub fn spec(&self) -> Option<&PublishSpec> {
        match self {
            TargetPublish::Off => None,
            TargetPublish::On(spec) => Some(spec),
        }
    }
}

fn deserialize_target_publish<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<TargetPublish>, D::Error> {
    struct PublishVisitor;

    impl<'de> Visitor<'de> for PublishVisitor {
        type Value = TargetPublish;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a mapping with `play` or `testflight`, or false")
        }

        fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
            if value {
                Err(E::custom(
                    "publish: true is not supported; set `play` or `testflight`, or false to publish nowhere",
                ))
            } else {
                Ok(TargetPublish::Off)
            }
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(TargetPublish::Off)
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(TargetPublish::Off)
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            PublishSpec::deserialize(de::value::MapAccessDeserializer::new(map))
                .map(TargetPublish::On)
        }
    }

    deserializer.deserialize_any(PublishVisitor).map(Some)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishSpec {
    #[serde(default)]
    pub play: Option<PlayPublishSpec>,
    #[serde(default)]
    pub testflight: Option<TestflightPublishSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayPublishSpec {
    pub track: String,
    /// Initial staged rollout fraction, 0 < rollout <= 1. Left out, the
    /// pipeline's Google Play settings decide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout: Option<f64>,
    /// Left out, the pipeline's Google Play settings decide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestflightPublishSpec {
    pub group: String,
}

/// One entry in a `steps` list.
#[derive(Debug, Clone, PartialEq)]
pub enum StepEntry {
    /// `build`: the targets run here.
    Build,
    /// A workflow's reference to a top-level step, by name.
    Ref(String),
    Inline(InlineStep),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InlineStep {
    pub name: String,
    pub run: String,
    #[serde(default)]
    pub reports: Option<StepReports>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepReports {
    pub tests: String,
}

impl<'de> Deserialize<'de> for StepEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StepVisitor;

        impl<'de> Visitor<'de> for StepVisitor {
            type Value = StepEntry;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "`build`, the name of a top-level step, or a step with `name` and `run`",
                )
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(if value == "build" {
                    StepEntry::Build
                } else {
                    StepEntry::Ref(value.to_string())
                })
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                InlineStep::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(StepEntry::Inline)
            }
        }

        deserializer.deserialize_any(StepVisitor)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowSpec {
    #[serde(default)]
    pub on: Option<TriggerSpec>,
    /// Targets this workflow builds, as a list of platforms or a mapping of
    /// per-workflow overrides. `None` builds every top-level target.
    #[serde(default)]
    pub targets: Option<WorkflowTargets>,
    /// Narrows `app.flavors`.
    #[serde(default)]
    pub flavors: Option<Vec<String>>,
    /// Replaces the top-level steps.
    #[serde(default)]
    pub steps: Option<Vec<StepEntry>>,
    /// Added to, or replacing, the top-level env by key.
    #[serde(default)]
    pub env: OrderedMap<String, String>,
    /// Added to, or replacing, the top-level vars by name.
    #[serde(default)]
    pub vars: OrderedMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkflowTargets(pub OrderedMap<BuildPlatform, TargetSpec>);

impl<'de> Deserialize<'de> for WorkflowTargets {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TargetsVisitor;

        impl<'de> Visitor<'de> for TargetsVisitor {
            type Value = WorkflowTargets;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list of platforms or a mapping of platform to target overrides")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut entries: Vec<(BuildPlatform, TargetSpec)> = Vec::new();
                while let Some(platform) = seq.next_element::<BuildPlatform>()? {
                    if entries.iter().any(|(existing, _)| *existing == platform) {
                        return Err(de::Error::custom(format!(
                            "duplicate target '{}'",
                            platform_key(&platform)
                        )));
                    }
                    entries.push((platform, TargetSpec::default()));
                }
                Ok(WorkflowTargets(OrderedMap(entries)))
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                let entries = OrderedMap::<BuildPlatform, Option<TargetSpec>>::deserialize(
                    de::value::MapAccessDeserializer::new(map),
                )?;
                Ok(WorkflowTargets(OrderedMap(
                    entries
                        .0
                        .into_iter()
                        .map(|(platform, spec)| (platform, spec.unwrap_or_default()))
                        .collect(),
                )))
            }
        }

        deserializer.deserialize_any(TargetsVisitor)
    }
}

/// When a workflow runs.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerSpec {
    #[serde(default)]
    pub push: Option<BranchTrigger>,
    #[serde(default)]
    pub pull_request: Option<BranchTrigger>,
    #[serde(default, deserialize_with = "one_or_many")]
    pub tag: Vec<String>,
    #[serde(default)]
    pub paths: Vec<String>,
}

/// `true`, `false`, or `{ branches: [...] }`.
#[derive(Debug, Clone, PartialEq)]
pub enum BranchTrigger {
    Off,
    AllBranches,
    Branches(Vec<String>),
}

impl<'de> Deserialize<'de> for BranchTrigger {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct BranchFilter {
            branches: Vec<String>,
        }

        struct TriggerVisitor;

        impl<'de> Visitor<'de> for TriggerVisitor {
            type Value = BranchTrigger;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("true, false, or a mapping with `branches`")
            }

            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(if value {
                    BranchTrigger::AllBranches
                } else {
                    BranchTrigger::Off
                })
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                BranchFilter::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(|filter| BranchTrigger::Branches(filter.branches))
            }
        }

        deserializer.deserialize_any(TriggerVisitor)
    }
}

/// A webhook revision, as `on` sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerEvent<'a> {
    /// A push to `branch`.
    Push { branch: &'a str },
    /// A pull or merge request into `target_branch`.
    PullRequest { target_branch: Option<&'a str> },
    /// A pushed tag.
    Tag { tag: &'a str },
}

impl BranchTrigger {
    fn matches(&self, branch: Option<&str>) -> bool {
        match self {
            BranchTrigger::Off => false,
            BranchTrigger::AllBranches => true,
            BranchTrigger::Branches(patterns) => branch.is_some_and(|branch| {
                patterns
                    .iter()
                    .any(|pattern| crate::TriggerConfig::glob_match(pattern.trim(), branch))
            }),
        }
    }
}

impl TriggerSpec {
    /// Whether the event's kind and ref start this workflow. Branch and tag
    /// globs use the same `*` and `?` rules as the pipeline's trigger
    /// settings; `pull_request.branches` names the branch the request
    /// targets.
    pub fn matches_ref(&self, event: TriggerEvent<'_>) -> bool {
        match event {
            TriggerEvent::Push { branch } => self
                .push
                .as_ref()
                .is_some_and(|push| push.matches(Some(branch))),
            TriggerEvent::PullRequest { target_branch } => self
                .pull_request
                .as_ref()
                .is_some_and(|pull_request| pull_request.matches(target_branch)),
            TriggerEvent::Tag { tag } => self
                .tag
                .iter()
                .any(|pattern| crate::TriggerConfig::glob_match(pattern.trim(), tag)),
        }
    }

    /// Whether `paths` narrows push and pull request events. Tags ignore it.
    pub fn has_path_filters(&self) -> bool {
        !self.paths.is_empty()
    }

    /// Whether a changed file matches `paths`. `None` means the change set is
    /// unknown, which never suppresses a build.
    pub fn paths_match(&self, changed_files: Option<&[String]>) -> bool {
        let Some(files) = changed_files.filter(|_| self.has_path_filters()) else {
            return true;
        };
        files.iter().any(|file| {
            self.paths
                .iter()
                .any(|pattern| crate::changed_path_matches(pattern, file))
        })
    }
}

fn one_or_many<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    struct OneOrMany;

    impl<'de> Visitor<'de> for OneOrMany {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a string or a list of strings")
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
            Ok(vec![value.to_string()])
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element::<String>()? {
                values.push(value);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_any(OneOrMany)
}

pub fn platform_key(platform: &BuildPlatform) -> &'static str {
    match platform {
        BuildPlatform::Android => "android",
        BuildPlatform::Ios => "ios",
        BuildPlatform::Macos => "macos",
    }
}

fn platform_title(platform: &BuildPlatform) -> &'static str {
    match platform {
        BuildPlatform::Android => "Android",
        BuildPlatform::Ios => "iOS",
        BuildPlatform::Macos => "macOS",
    }
}

/// Whether `name` can name a v2 workflow, before any file is checked.
pub fn is_valid_workflow_name(name: &str) -> bool {
    is_valid_identifier(name)
}

/// Workflow and flavor names end up in step names and shell commands.
fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && name.len() <= 64
}

// ── v2 references ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expression {
    Secret,
    /// The var name's byte range in the parsed text.
    Var(std::ops::Range<usize>),
    Flavor,
}

/// Every `${{ ... }}` in `text`, as (start, end, expression).
fn parse_expressions(text: &str) -> Result<Vec<(usize, usize, Expression)>, String> {
    let mut found = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find("${{") {
        let start = cursor + offset;
        let body_start = start + 3;
        let Some(close) = text[body_start..].find("}}") else {
            return Err("has an unterminated ${{ expression".to_string());
        };
        let end = body_start + close + 2;
        let body = text[body_start..body_start + close].trim();
        let expression = if body == "flavor" {
            Expression::Flavor
        } else if body
            .strip_prefix("secrets.")
            .is_some_and(is_valid_pipeline_env_key)
        {
            Expression::Secret
        } else if let Some(name) = body
            .strip_prefix("vars.")
            .filter(|name| is_valid_pipeline_env_key(name))
        {
            let name_start = body_start + text[body_start..].find("vars.").unwrap_or(0) + 5;
            Expression::Var(name_start..name_start + name.len())
        } else {
            return Err(format!(
                "has unsupported expression '${{{{ {body} }}}}'; use ${{{{ vars.NAME }}}}, ${{{{ secrets.NAME }}}} or ${{{{ flavor }}}}"
            ));
        };
        found.push((start, end, expression));
        cursor = end;
    }
    Ok(found)
}

/// Whether `${{ flavor }}` can be used where a value is resolved.
#[derive(Debug, Clone, Copy)]
enum FlavorScope<'a> {
    /// In a target build or artifact pattern, for this flavor.
    Value(&'a str),
    /// In a target, but the workflow builds no flavors.
    NoFlavors,
    /// Outside a target.
    Outside,
}

/// Longest value a reference may expand to.
pub const MAX_EXPANDED_BYTES: usize = 64 * 1024;
/// Most var references one file may expand, across every workflow. Bounds
/// files whose vars refer to each other many times over, even when they
/// expand to little text.
pub const MAX_VAR_EXPANSIONS: usize = 250_000;
/// Most text vars may add across one file and every workflow.
pub const MAX_TOTAL_EXPANDED_BYTES: usize = 4 * 1024 * 1024;

// Size limits that keep validating a file cheap. The daemon validates files
// from requests and repositories, so the work must stay bounded.
pub const MAX_WORKFLOWS: usize = 32;
pub const MAX_FLAVORS: usize = 32;
pub const MAX_STEPS: usize = 100;
pub const MAX_ARTIFACT_PATTERNS: usize = 64;
pub const MAX_ENV_ENTRIES: usize = 128;
pub const MAX_VARS: usize = 128;
pub const MAX_TRIGGER_PATTERNS: usize = 64;
/// Longest `run` or `build` command as written.
pub const MAX_COMMAND_BYTES: usize = 32 * 1024;
/// Longest artifact pattern as written, before vars are expanded.
const MAX_RAW_PATTERN_BYTES: usize = 2048;

/// Expansion work shared by everything resolved for one file.
#[derive(Default)]
struct Budget {
    expansions: std::cell::Cell<usize>,
    bytes: std::cell::Cell<usize>,
}

/// Errors that end validation early: anything after them would spend the
/// same budget again.
fn is_limit_error(error: &str) -> bool {
    error.contains(" through vars") || error.contains(" in this file")
}

/// The flavor scope part of a cache entry. A flavor is told apart by where
/// its text lives, which is stable for one resolver; equal flavors at
/// different addresses only miss the cache.
type ScopeKey = (u8, usize, usize);

struct Resolver<'a> {
    vars: &'a [(String, String)],
    index: std::collections::HashMap<&'a str, usize>,
    budget: &'a Budget,
    /// Resolved vars by var and flavor scope, so each var is resolved once
    /// however often it is used.
    /// Per var, its resolved value for each flavor scope seen so far.
    cache: std::cell::RefCell<Vec<Vec<(ScopeKey, String)>>>,
}

impl FlavorScope<'_> {
    fn cache_key(&self) -> ScopeKey {
        match self {
            FlavorScope::Value(flavor) => (0, flavor.as_ptr() as usize, flavor.len()),
            FlavorScope::NoFlavors => (1, 0, 0),
            FlavorScope::Outside => (2, 0, 0),
        }
    }
}

impl<'a> Resolver<'a> {
    fn new(vars: &'a [(String, String)], budget: &'a Budget) -> Self {
        Self {
            vars,
            index: vars
                .iter()
                .enumerate()
                .map(|(index, (name, _))| (name.as_str(), index))
                .collect(),
            budget,
            cache: std::cell::RefCell::new(vec![Vec::new(); vars.len()]),
        }
    }

    /// Substitute vars and the flavor. Secret references stay as written; the
    /// runner resolves them without splicing values into shell source.
    fn resolve(&self, text: &str, flavor: FlavorScope<'_>, path: &str) -> Result<String, String> {
        self.resolve_from(text, flavor, path, Vec::new())
    }

    /// Resolve with `chain` naming the var `path` is itself, for validating vars.
    fn resolve_from(
        &self,
        text: &str,
        flavor: FlavorScope<'_>,
        path: &str,
        mut chain: Vec<String>,
    ) -> Result<String, String> {
        let base = chain.len();
        self.resolve_inner(text, flavor, path, &mut chain, base)
    }

    fn resolve_inner(
        &self,
        text: &str,
        flavor: FlavorScope<'_>,
        path: &str,
        chain: &mut Vec<String>,
        // Leading chain entries that name `path` itself, not a hop.
        base: usize,
    ) -> Result<String, String> {
        let through = |chain: &[String]| {
            let chain = &chain[base.min(chain.len())..];
            if chain.is_empty() {
                String::new()
            } else {
                let names: Vec<String> = chain.iter().map(|name| format!("vars.{name}")).collect();
                format!(" (through {})", names.join(" -> "))
            }
        };
        let expressions = parse_expressions(text).map_err(|error| {
            if chain.len() > base {
                format!("vars.{} {error}", chain[chain.len() - 1])
            } else {
                format!("{path} {error}")
            }
        })?;
        let mut out = String::with_capacity(text.len());
        let mut cursor = 0;
        for (start, end, expression) in expressions {
            out.push_str(&text[cursor..start]);
            match expression {
                Expression::Secret => out.push_str(&text[start..end]),
                Expression::Flavor => match flavor {
                    FlavorScope::Value(value) => out.push_str(value),
                    FlavorScope::NoFlavors => {
                        return Err(format!(
                            "{path} uses ${{{{ flavor }}}}{}, but no flavors are configured in app.flavors",
                            through(chain)
                        ));
                    }
                    FlavorScope::Outside => {
                        return Err(format!(
                            "{path} uses ${{{{ flavor }}}}{}, which is only available in a target's build and artifacts",
                            through(chain)
                        ));
                    }
                },
                Expression::Var(range) => {
                    let name = &text[range];
                    if let Some(position) = chain.iter().position(|seen| seen == name) {
                        // Start the cycle at its first name so each cycle reads
                        // the same wherever it was found.
                        let mut cycle = chain[position..].to_vec();
                        let first = (0..cycle.len()).min_by_key(|&i| &cycle[i]).unwrap_or(0);
                        cycle.rotate_left(first);
                        cycle.push(cycle[0].clone());
                        let cycle: Vec<String> =
                            cycle.iter().map(|name| format!("vars.{name}")).collect();
                        return Err(format!("vars form a cycle: {}", cycle.join(" -> ")));
                    }
                    let Some(&var) = self.index.get(name) else {
                        return Err(format!(
                            "{path} refers to vars.{name}{}, which is not defined",
                            through(chain)
                        ));
                    };
                    let spent = self.budget.expansions.get() + 1;
                    self.budget.expansions.set(spent);
                    if spent > MAX_VAR_EXPANSIONS {
                        return Err(format!(
                            "{path}: vars expand more than {MAX_VAR_EXPANSIONS} times in this file"
                        ));
                    }
                    let key = flavor.cache_key();
                    let added = match self.cache.borrow()[var]
                        .iter()
                        .find(|(scope, _)| *scope == key)
                        .map(|(_, resolved)| resolved)
                    {
                        Some(resolved) => {
                            out.push_str(resolved);
                            Some(resolved.len())
                        }
                        None => None,
                    };
                    let added = match added {
                        Some(added) => added,
                        None => {
                            chain.push(name.to_string());
                            let resolved =
                                self.resolve_inner(&self.vars[var].1, flavor, path, chain, base)?;
                            chain.pop();
                            out.push_str(&resolved);
                            let added = resolved.len();
                            self.cache.borrow_mut()[var].push((key, resolved));
                            added
                        }
                    };
                    let bytes = self.budget.bytes.get() + added;
                    self.budget.bytes.set(bytes);
                    if bytes > MAX_TOTAL_EXPANDED_BYTES {
                        return Err(format!(
                            "{path}: vars expand to more than {} MiB in this file",
                            MAX_TOTAL_EXPANDED_BYTES / (1024 * 1024)
                        ));
                    }
                }
            }
            cursor = end;
            // Text written out in the file is not capped, only what vars add.
            if out.len() > MAX_EXPANDED_BYTES && out.len() > text.len() {
                return Err(too_long(path));
            }
        }
        out.push_str(&text[cursor..]);
        if out.len() > MAX_EXPANDED_BYTES && out.len() > text.len() {
            return Err(too_long(path));
        }
        Ok(out)
    }
}

fn too_long(path: &str) -> String {
    format!(
        "{path} expands to more than {} KiB through vars",
        MAX_EXPANDED_BYTES / 1024
    )
}

// ── v2 validation and lowering ──────────────────────────────────

struct EffectiveTarget<'a> {
    platform: BuildPlatform,
    build: &'a str,
    build_source: String,
    artifacts: Option<(&'a [String], String)>,
    /// With the path of the `publish` it came from.
    publish: Option<(&'a PublishSpec, String)>,
}

/// Most errors reported for one file; the rest repeat the same mistakes.
const MAX_ERRORS: usize = 50;

fn push_unique(errors: &mut Vec<String>, error: String) {
    if errors.len() < MAX_ERRORS && !errors.contains(&error) {
        errors.push(error);
    }
}

impl ConfigV2 {
    /// Parse and fully validate a `version: 2` file: its shape, then every
    /// workflow's plan, so reference errors surface before a build runs.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let mut config: ConfigV2 =
            serde_yaml::from_str(raw).map_err(|error| format!("YAML parse error: {error}"))?;
        if config.version != 2 {
            return Err(format!(
                "unsupported config version {}, expected 2",
                config.version
            ));
        }
        // Sizes first: the other checks are cheap only within these limits.
        let size_errors = config.size_errors();
        if !size_errors.is_empty() {
            return Err(size_errors.join("\n"));
        }
        let mut errors = config.shape_errors();
        if errors.is_empty() {
            let workflows: Vec<Option<String>> = if config.workflows.is_empty() {
                vec![None]
            } else {
                config
                    .workflows
                    .0
                    .iter()
                    .map(|(name, _)| Some(name.clone()))
                    .collect()
            };
            let budget = Budget::default();
            for (index, workflow) in workflows.iter().enumerate() {
                // Vars only need checking where a workflow changes them.
                let validate_vars = index == 0
                    || workflow
                        .as_deref()
                        .and_then(|name| config.workflows.get(&name.to_string()))
                        .is_some_and(|spec| !spec.vars.is_empty());
                if let Err(plan_errors) =
                    config.lower(workflow.as_deref(), None, &budget, validate_vars)
                {
                    let over_limit = plan_errors.iter().any(|error| is_limit_error(error));
                    for error in plan_errors {
                        push_unique(&mut errors, error);
                    }
                    if over_limit {
                        break;
                    }
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
        config.runs_on = normalize_runner_labels(&config.runs_on)?;
        Ok(config)
    }

    pub fn workflow_names(&self) -> Vec<&str> {
        self.workflows
            .0
            .iter()
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// See [`RepositoryConfig::triggers`]. Without a named workflow, the only
    /// workflow's `on` applies; with several, only the top-level `on` can.
    pub fn triggers(&self, workflow: Option<&str>) -> Option<&TriggerSpec> {
        let selected = match workflow {
            Some(name) => self.workflows.get(&name.to_string()),
            None => match self.workflows.0.as_slice() {
                [(_, only)] => Some(only),
                _ => None,
            },
        };
        selected
            .and_then(|workflow| workflow.on.as_ref())
            .or(self.on.as_ref())
    }

    /// Plan one workflow. Errors are one per line.
    pub fn plan(
        &self,
        workflow: Option<&str>,
        selected_platforms: Option<&[BuildPlatform]>,
    ) -> Result<PipelinePlan, String> {
        // Parsing checked every var already.
        self.lower(workflow, selected_platforms, &Budget::default(), false)
            .map_err(|errors| errors.join("\n"))
    }

    pub fn notices(&self) -> Vec<String> {
        let mut notices = Vec::new();
        if self.workflows.0.len() > 1 {
            notices.push(format!(
                "this file defines workflows [{}]; set the Workflow in each pipeline's settings, or its builds fail with `choose one for the pipeline`",
                self.workflow_names().join(", ")
            ));
        }
        let declares_on = self.on.is_some()
            || self
                .workflows
                .iter()
                .any(|(_, workflow)| workflow.on.is_some());
        if declares_on {
            notices.push(
                "`on` replaces the pipeline's trigger settings for push, pull request and tag webhooks; schedules and manual builds still come from the pipeline"
                    .to_string(),
            );
        }
        let declares_publish = self
            .targets
            .iter()
            .map(|(_, target)| target)
            .chain(self.workflows.iter().flat_map(|(_, workflow)| {
                workflow
                    .targets
                    .iter()
                    .flat_map(|targets| targets.0.iter().map(|(_, target)| target))
            }))
            .any(|target| {
                target
                    .publish
                    .as_ref()
                    .and_then(TargetPublish::spec)
                    .is_some()
            });
        if declares_publish {
            notices.push(
                "`publish` replaces the pipeline's Publish settings for the stores it names: the file sets the Google Play track, rollout and draft, and the TestFlight group; turn the store on in the pipeline's Publish settings for its credentials and release notes, or the publish step fails"
                    .to_string(),
            );
            notices.extend(self.publish_notices());
        }
        notices
    }

    /// Per workflow: uploads whose publish step will fail, values the upload
    /// adjusts to fit Play's rules, and Play uploads without an Android build
    /// that looks like it makes an App Bundle. v2 runs `build` as written;
    /// the bundle check is a notice, not an error, since the command may run
    /// a script that builds one.
    fn publish_notices(&self) -> Vec<String> {
        let workflows: Vec<Option<&str>> = if self.workflows.is_empty() {
            vec![None]
        } else {
            self.workflow_names().into_iter().map(Some).collect()
        };
        let mut notices = Vec::new();
        for workflow in workflows {
            let Ok(plan) = self.plan(workflow, None) else {
                continue;
            };
            let scope = workflow
                .map(|name| format!("workflow {name}: "))
                .unwrap_or_default();
            for error in [
                &plan.publish.play.as_ref().and_then(|p| p.error.clone()),
                &plan
                    .publish
                    .testflight
                    .as_ref()
                    .and_then(|p| p.error.clone()),
            ]
            .into_iter()
            .flatten()
            {
                push_unique(
                    &mut notices,
                    format!("{scope}{error}; the build runs, and its publish step fails"),
                );
            }
            for note in plan.publish.adjustment_notes() {
                push_unique(&mut notices, format!("{scope}{note}"));
            }
            let Some(play) = plan.publish.play.as_ref().filter(|p| p.error.is_none()) else {
                continue;
            };
            let builds_bundle = plan
                .steps_in(StepPhase::Build)
                .filter(|step| step.platform == Some(BuildPlatform::Android))
                .any(|step| step.run.to_ascii_lowercase().contains("bundle"));
            if !builds_bundle {
                push_unique(
                    &mut notices,
                    format!(
                        "{scope}{} uploads an App Bundle, but the android build does not run `flutter build appbundle`; Oore runs `build` as written, so the upload fails with `No release App Bundle found` unless the command builds one under build/app/outputs/bundle",
                        play.source
                    ),
                );
            }
        }
        notices
    }

    /// Counts and lengths over the limits that keep validation cheap.
    fn size_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut count = |path: &str, len: usize, max: usize, what: &str| {
            if len > max {
                errors.push(format!("{path} has {len} {what}; the limit is {max}"));
            }
        };
        count(
            "workflows",
            self.workflows.0.len(),
            MAX_WORKFLOWS,
            "workflows",
        );
        count(
            "app.flavors",
            self.app.flavors.len(),
            MAX_FLAVORS,
            "flavors",
        );
        count("env", self.env.0.len(), MAX_ENV_ENTRIES, "entries");
        count("vars", self.vars.0.len(), MAX_VARS, "vars");
        if let Some(steps) = &self.steps {
            count("steps", steps.len(), MAX_STEPS, "steps");
        }
        let mut targets: Vec<(String, &TargetSpec)> = self
            .targets
            .iter()
            .map(|(platform, target)| (format!("targets.{}", platform_key(platform)), target))
            .collect();
        let mut steps: Vec<(String, &[StepEntry])> = Vec::new();
        let mut triggers: Vec<(String, &TriggerSpec)> = Vec::new();
        if let Some(on) = &self.on {
            triggers.push(("on".to_string(), on));
        }
        if let Some(entries) = &self.steps {
            steps.push(("steps".to_string(), entries));
        }
        for (name, workflow) in self.workflows.iter().take(MAX_WORKFLOWS) {
            let path = format!("workflows.{name}");
            if let Some(flavors) = &workflow.flavors {
                count(
                    &format!("{path}.flavors"),
                    flavors.len(),
                    MAX_FLAVORS,
                    "flavors",
                );
            }
            count(
                &format!("{path}.env"),
                workflow.env.0.len(),
                MAX_ENV_ENTRIES,
                "entries",
            );
            count(
                &format!("{path}.vars"),
                workflow.vars.0.len(),
                MAX_VARS,
                "vars",
            );
            if let Some(entries) = &workflow.steps {
                count(&format!("{path}.steps"), entries.len(), MAX_STEPS, "steps");
                steps.push((format!("{path}.steps"), entries));
            }
            if let Some(on) = &workflow.on {
                triggers.push((format!("{path}.on"), on));
            }
            if let Some(overrides) = &workflow.targets {
                targets.extend(overrides.0.iter().map(|(platform, target)| {
                    (format!("{path}.targets.{}", platform_key(platform)), target)
                }));
            }
        }
        for (path, target) in targets {
            if let Some(build) = &target.build
                && build.len() > MAX_COMMAND_BYTES
            {
                errors.push(format!(
                    "{path}.build is longer than {} KiB",
                    MAX_COMMAND_BYTES / 1024
                ));
            }
            if let Some(patterns) = &target.artifacts {
                if patterns.len() > MAX_ARTIFACT_PATTERNS {
                    errors.push(format!(
                        "{path}.artifacts has {} patterns; the limit is {MAX_ARTIFACT_PATTERNS}",
                        patterns.len()
                    ));
                } else if let Some(index) = patterns
                    .iter()
                    .position(|pattern| pattern.len() > MAX_RAW_PATTERN_BYTES)
                {
                    errors.push(format!(
                        "{path}.artifacts[{index}] is longer than {MAX_RAW_PATTERN_BYTES} characters"
                    ));
                }
            }
        }
        for (path, entries) in steps {
            for (index, entry) in entries.iter().enumerate().take(MAX_STEPS) {
                if let StepEntry::Inline(step) = entry
                    && step.run.len() > MAX_COMMAND_BYTES
                {
                    errors.push(format!(
                        "{path}[{index}].run is longer than {} KiB",
                        MAX_COMMAND_BYTES / 1024
                    ));
                }
            }
        }
        for (path, on) in triggers {
            for (field, len) in [
                ("push.branches", branch_count(&on.push)),
                ("pull_request.branches", branch_count(&on.pull_request)),
                ("tag", on.tag.len()),
                ("paths", on.paths.len()),
            ] {
                if len > MAX_TRIGGER_PATTERNS {
                    errors.push(format!(
                        "{path}.{field} has {len} patterns; the limit is {MAX_TRIGGER_PATTERNS}"
                    ));
                }
            }
        }
        errors.truncate(MAX_ERRORS);
        errors
    }

    /// Errors that need no reference resolution.
    fn shape_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if let Some(flutter) = &self.app.flutter
            && flutter.trim().is_empty()
        {
            errors.push("app.flutter must not be empty".to_string());
        }
        check_flavor_list(&mut errors, "app.flavors", &self.app.flavors, None);

        let any_workflow_targets = self
            .workflows
            .iter()
            .any(|(_, workflow)| workflow.targets.as_ref().is_some_and(|t| !t.0.is_empty()));
        if self.targets.is_empty() && !any_workflow_targets {
            errors.push("targets must define at least one of android, ios or macos".to_string());
        }
        for (platform, target) in self.targets.iter() {
            let path = format!("targets.{}", platform_key(platform));
            if target.build.is_none() {
                errors.push(format!("{path}.build is required"));
            }
            check_target(&mut errors, &path, platform, target);
        }

        if let Some(steps) = &self.steps {
            check_steps(&mut errors, "steps", steps, None);
        }
        check_keys(&mut errors, "env", &self.env);
        check_keys(&mut errors, "vars", &self.vars);
        if let Some(on) = &self.on {
            check_triggers(&mut errors, "on", on);
        }

        for (name, workflow) in self.workflows.iter() {
            let path = format!("workflows.{name}");
            if !is_valid_identifier(name) {
                errors.push(format!(
                    "{path}: workflow names must start with a letter or digit and use only letters, digits, '-' and '_' (max 64 chars)"
                ));
            }
            if let Some(on) = &workflow.on {
                check_triggers(&mut errors, &format!("{path}.on"), on);
            }
            if let Some(targets) = &workflow.targets {
                if targets.0.is_empty() {
                    errors.push(format!("{path}.targets must not be empty"));
                }
                for (platform, target) in targets.0.iter() {
                    let target_path = format!("{path}.targets.{}", platform_key(platform));
                    if target.build.is_none() && self.targets.get(platform).is_none() {
                        errors.push(format!(
                            "{target_path} needs a build command, because there is no top-level targets.{}",
                            platform_key(platform)
                        ));
                    }
                    check_target(&mut errors, &target_path, platform, target);
                }
            }
            if let Some(flavors) = &workflow.flavors {
                check_flavor_list(
                    &mut errors,
                    &format!("{path}.flavors"),
                    flavors,
                    Some(&self.app.flavors),
                );
            }
            if let Some(steps) = &workflow.steps {
                let shared = self.steps.as_deref().unwrap_or_default();
                check_steps(&mut errors, &format!("{path}.steps"), steps, Some(shared));
            }
            check_keys(&mut errors, &format!("{path}.env"), &workflow.env);
            check_keys(&mut errors, &format!("{path}.vars"), &workflow.vars);
        }

        if let Err(error) = validate_build_number_offset(self.build_number_offset) {
            errors.push(format!("build_number_offset {error}"));
        }
        if let Err(error) = normalize_runner_labels(&self.runs_on) {
            errors.push(format!("runs_on {error}"));
        }
        errors
    }

    fn select_workflow<'a>(
        &'a self,
        name: Option<&str>,
    ) -> Result<Option<(&'a str, &'a WorkflowSpec)>, String> {
        let names = || self.workflow_names().join(", ");
        match name {
            Some(name) => self
                .workflows
                .0
                .iter()
                .find(|(key, _)| key == name)
                .map(|(key, workflow)| Some((key.as_str(), workflow)))
                .ok_or_else(|| {
                    if self.workflows.is_empty() {
                        format!("workflow '{name}' is not defined; this file defines no workflows")
                    } else {
                        format!(
                            "workflow '{name}' is not defined; this file defines [{}]",
                            names()
                        )
                    }
                }),
            None => match self.workflows.0.as_slice() {
                [] => Ok(None),
                [(key, workflow)] => Ok(Some((key.as_str(), workflow))),
                _ => Err(format!(
                    "this file defines workflows [{}]; choose one for the pipeline",
                    names()
                )),
            },
        }
    }

    fn lower(
        &self,
        workflow: Option<&str>,
        selected_platforms: Option<&[BuildPlatform]>,
        budget: &Budget,
        validate_vars: bool,
    ) -> Result<PipelinePlan, Vec<String>> {
        let selected_workflow = self
            .select_workflow(workflow)
            .map_err(|error| vec![error])?;
        let workflow_spec = selected_workflow.map(|(_, spec)| spec);
        let prefix = selected_workflow
            .map(|(name, _)| format!("workflows.{name}."))
            .unwrap_or_default();
        let mut errors = Vec::new();

        // Vars: the workflow's replace the top-level ones by name.
        let mut vars: Vec<(String, String)> = self.vars.0.clone();
        let mut var_paths: Vec<String> = self
            .vars
            .0
            .iter()
            .map(|(name, _)| format!("vars.{name}"))
            .collect();
        if let Some(spec) = workflow_spec {
            for (name, value) in spec.vars.iter() {
                let path = format!("{prefix}vars.{name}");
                match vars.iter().position(|(key, _)| key == name) {
                    Some(index) => {
                        vars[index].1 = value.clone();
                        var_paths[index] = path;
                    }
                    None => {
                        vars.push((name.clone(), value.clone()));
                        var_paths.push(path);
                    }
                }
            }
        }
        let resolver = Resolver::new(&vars, budget);
        for ((name, value), path) in vars.iter().zip(&var_paths).filter(|_| validate_vars) {
            if let Err(error) = resolver.resolve_from(
                value,
                FlavorScope::Value("flavor"),
                path,
                vec![name.clone()],
            ) {
                // Vars built on an oversized var hit the limit again; one
                // report is enough and stops the work early.
                let over_limit = is_limit_error(&error);
                push_unique(&mut errors, error);
                if over_limit {
                    break;
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }

        // Targets.
        let mut targets: Vec<EffectiveTarget<'_>> = Vec::new();
        match workflow_spec.and_then(|spec| spec.targets.as_ref()) {
            Some(overrides) => {
                for (platform, spec) in overrides.0.iter() {
                    let key = platform_key(platform);
                    let base = self.targets.get(platform);
                    let (build, build_source) = match (
                        &spec.build,
                        base.and_then(|b| b.build.as_ref()),
                    ) {
                        (Some(build), _) => {
                            (build.as_str(), format!("{prefix}targets.{key}.build"))
                        }
                        (None, Some(build)) => (build.as_str(), format!("targets.{key}.build")),
                        (None, None) => {
                            errors.push(format!(
                                "{prefix}targets.{key} needs a build command, because there is no top-level targets.{key}"
                            ));
                            continue;
                        }
                    };
                    let artifacts = match (&spec.artifacts, base.and_then(|b| b.artifacts.as_ref()))
                    {
                        (Some(patterns), _) => Some((
                            patterns.as_slice(),
                            format!("{prefix}targets.{key}.artifacts"),
                        )),
                        (None, Some(patterns)) => {
                            Some((patterns.as_slice(), format!("targets.{key}.artifacts")))
                        }
                        (None, None) => None,
                    };
                    // The top-level publish uploads what the top-level build
                    // makes, so it applies only while the workflow keeps it.
                    // `false` drops it.
                    let publish = match &spec.publish {
                        Some(publish) => publish
                            .spec()
                            .map(|publish| (publish, format!("{prefix}targets.{key}.publish"))),
                        None if spec.build.is_none() => base
                            .and_then(|b| b.publish.as_ref())
                            .and_then(TargetPublish::spec)
                            .map(|publish| (publish, format!("targets.{key}.publish"))),
                        None => None,
                    };
                    targets.push(EffectiveTarget {
                        platform: platform.clone(),
                        build,
                        build_source,
                        artifacts,
                        publish,
                    });
                }
            }
            None => {
                for (platform, spec) in self.targets.iter() {
                    let key = platform_key(platform);
                    let Some(build) = spec.build.as_deref() else {
                        continue;
                    };
                    targets.push(EffectiveTarget {
                        platform: platform.clone(),
                        build,
                        build_source: format!("targets.{key}.build"),
                        artifacts: spec.artifacts.as_ref().map(|patterns| {
                            (patterns.as_slice(), format!("targets.{key}.artifacts"))
                        }),
                        publish: spec
                            .publish
                            .as_ref()
                            .and_then(TargetPublish::spec)
                            .map(|publish| (publish, format!("targets.{key}.publish"))),
                    });
                }
            }
        }
        if let Some(selected) = selected_platforms {
            if selected.is_empty() {
                errors.push("selected_platforms must include at least one target".to_string());
            } else if selected
                .iter()
                .enumerate()
                .any(|(index, platform)| selected[..index].contains(platform))
                || selected
                    .iter()
                    .any(|platform| !targets.iter().any(|target| target.platform == *platform))
            {
                errors.push(
                    "selected_platforms must be unique and configured by the workflow".to_string(),
                );
            } else {
                targets.retain(|target| selected.contains(&target.platform));
            }
        }

        let flavors: &[String] = workflow_spec
            .and_then(|spec| spec.flavors.as_deref())
            .unwrap_or(&self.app.flavors);
        let flavor_scopes: Vec<(Option<&str>, FlavorScope<'_>)> = if flavors.is_empty() {
            vec![(None, FlavorScope::NoFlavors)]
        } else {
            flavors
                .iter()
                .map(|flavor| (Some(flavor.as_str()), FlavorScope::Value(flavor)))
                .collect()
        };

        // Store uploads. Problems with them fail only the publish step, so
        // the build still runs and keeps its artifacts.
        let mut publish = PlanPublish::default();
        for target in &targets {
            let Some((spec, path)) = &target.publish else {
                continue;
            };
            // Each store takes one build.
            let flavor_error = (flavors.len() > 1).then(|| {
                let flavors_path = if workflow_spec.is_some_and(|spec| spec.flavors.is_some()) {
                    format!("{prefix}flavors")
                } else {
                    "app.flavors".to_string()
                };
                format!(
                    "{path} uploads one build, but {flavors_path} builds {} flavors ({}); narrow it with `flavors: [<one flavor>]` on the workflow",
                    flavors.len(),
                    flavors.join(", ")
                )
            });
            let resolve = |value: &str, field: &str| -> Result<String, String> {
                let field_path = format!("{path}.{field}");
                let resolved = resolver
                    .resolve(value, FlavorScope::Outside, &field_path)?
                    .trim()
                    .to_string();
                if resolved.contains("${{") {
                    Err(format!(
                        "{field_path} cannot use ${{{{ secrets.NAME }}}}; only vars are resolved here"
                    ))
                } else if resolved.is_empty() {
                    Err(format!("{field_path} is empty once its vars are resolved"))
                } else {
                    Ok(resolved)
                }
            };
            if let Some(play) = &spec.play {
                let track = resolve(&play.track, "play.track");
                let error = match &track {
                    Err(error) => Some(error.clone()),
                    Ok(track) if !crate::is_valid_play_track(track) => Some(format!(
                        "{path}.play.track '{track}' must be internal, closed, open, production, or a custom track name"
                    )),
                    Ok(_) => flavor_error.clone(),
                };
                if error.as_deref().is_some_and(is_limit_error) {
                    return Err(vec![error.unwrap_or_default()]);
                }
                publish.play = Some(PlannedPublish {
                    spec: PlayPublishSpec {
                        track: track.unwrap_or_else(|_| play.track.trim().to_string()),
                        ..play.clone()
                    },
                    source: format!("{path}.play"),
                    error,
                });
            }
            if let Some(testflight) = &spec.testflight {
                let group = resolve(&testflight.group, "testflight.group");
                let error = match &group {
                    Err(error) => Some(error.clone()),
                    // Uploads report several groups joined by ", ".
                    Ok(group) if group.contains(',') => Some(format!(
                        "{path}.testflight.group '{group}' must be one group name, without commas"
                    )),
                    // `app_store` is the release track of App Store uploads.
                    Ok(group) if group.eq_ignore_ascii_case("app_store") => {
                        Some(format!("{path}.testflight.group cannot be named app_store"))
                    }
                    Ok(group) if group.chars().count() > MAX_TESTFLIGHT_GROUP_LENGTH => {
                        Some(format!(
                            "{path}.testflight.group is longer than {MAX_TESTFLIGHT_GROUP_LENGTH} characters"
                        ))
                    }
                    Ok(_) => flavor_error.clone(),
                };
                if error.as_deref().is_some_and(is_limit_error) {
                    return Err(vec![error.unwrap_or_default()]);
                }
                publish.testflight = Some(PlannedPublish {
                    spec: TestflightPublishSpec {
                        group: group.unwrap_or_else(|_| testflight.group.trim().to_string()),
                    },
                    source: format!("{path}.testflight"),
                    error,
                });
            }
        }

        // Target builds, one per platform and flavor.
        let mut build_steps = Vec::new();
        for target in &targets {
            for (flavor, scope) in &flavor_scopes {
                let name = match flavor {
                    Some(flavor) => {
                        format!("Build {} ({flavor})", platform_title(&target.platform))
                    }
                    None => format!("Build {}", platform_title(&target.platform)),
                };
                match resolver.resolve(target.build, *scope, &target.build_source) {
                    Ok(run) => build_steps.push(PlanStep {
                        name,
                        phase: StepPhase::Build,
                        run: run.trim().to_string(),
                        source: target.build_source.clone(),
                        platform: Some(target.platform.clone()),
                        flavor: flavor.map(str::to_string),
                        test_report: None,
                    }),
                    Err(error) => {
                        if is_limit_error(&error) {
                            return Err(vec![error]);
                        }
                        push_unique(&mut errors, error)
                    }
                }
            }
        }

        // Steps around them.
        let shared: &[StepEntry] = self.steps.as_deref().unwrap_or_default();
        let (entries, entries_path): (&[StepEntry], String) =
            match workflow_spec.and_then(|spec| spec.steps.as_deref()) {
                Some(steps) => (steps, format!("{prefix}steps")),
                None => (shared, "steps".to_string()),
            };
        let mut steps = Vec::new();
        let build_names: HashSet<String> =
            build_steps.iter().map(|step| step.name.clone()).collect();
        let mut names = build_names.clone();
        let mut build_placed = false;
        for (index, entry) in entries.iter().enumerate() {
            let (step, source) = match entry {
                StepEntry::Build => {
                    build_placed = true;
                    steps.append(&mut build_steps);
                    continue;
                }
                StepEntry::Inline(step) => (step, format!("{entries_path}[{index}]")),
                StepEntry::Ref(name) => {
                    // Shape validation reported unknown references already.
                    let Some((shared_index, step)) =
                        shared
                            .iter()
                            .enumerate()
                            .find_map(|(i, entry)| match entry {
                                StepEntry::Inline(step) if step.name.trim() == name.trim() => {
                                    Some((i, step))
                                }
                                _ => None,
                            })
                    else {
                        continue;
                    };
                    (step, format!("steps[{shared_index}]"))
                }
            };
            let name = step.name.trim().to_string();
            if !names.insert(name.clone()) {
                let reason = if build_names.contains(&name) {
                    "is the name of a target build step"
                } else {
                    "is used by another step"
                };
                push_unique(
                    &mut errors,
                    format!("{entries_path}[{index}] name '{name}' {reason}"),
                );
                continue;
            }
            let run_path = format!("{source}.run");
            match resolver.resolve(&step.run, FlavorScope::Outside, &run_path) {
                Ok(run) => steps.push(PlanStep {
                    name,
                    phase: if build_placed {
                        StepPhase::PostBuild
                    } else {
                        StepPhase::PreBuild
                    },
                    run: run.trim().to_string(),
                    source: run_path,
                    platform: None,
                    flavor: None,
                    test_report: step
                        .reports
                        .as_ref()
                        .map(|reports| reports.tests.trim().to_string()),
                }),
                Err(error) => {
                    if is_limit_error(&error) {
                        return Err(vec![error]);
                    }
                    push_unique(&mut errors, error)
                }
            }
        }
        if !build_placed {
            steps.append(&mut build_steps);
        }

        // Env: the workflow's replace the top-level values by key.
        let mut env: Vec<(String, String, String)> = self
            .env
            .iter()
            .map(|(key, value)| (key.clone(), value.clone(), format!("env.{key}")))
            .collect();
        if let Some(spec) = workflow_spec {
            for (key, value) in spec.env.iter() {
                let path = format!("{prefix}env.{key}");
                match env.iter().position(|(existing, _, _)| existing == key) {
                    Some(index) => env[index] = (key.clone(), value.clone(), path),
                    None => env.push((key.clone(), value.clone(), path)),
                }
            }
        }
        let mut resolved_env = Vec::new();
        for (key, value, path) in env {
            match resolver.resolve(&value, FlavorScope::Outside, &path) {
                Ok(value) => resolved_env.push(PipelineEnvVar {
                    key: key.trim().to_string(),
                    value: value.trim().to_string(),
                }),
                Err(error) => {
                    if is_limit_error(&error) {
                        return Err(vec![error]);
                    }
                    push_unique(&mut errors, error)
                }
            }
        }

        // Artifacts, per target and flavor.
        let mut artifact_patterns: Vec<String> = Vec::new();
        let mut seen_patterns: HashSet<String> = HashSet::new();
        for target in &targets {
            let defaults;
            let (patterns, path) = match &target.artifacts {
                Some((patterns, path)) => (*patterns, path.clone()),
                None => {
                    defaults = default_artifact_patterns_for_platforms(std::slice::from_ref(
                        &target.platform,
                    ));
                    (defaults.as_slice(), String::new())
                }
            };
            for (index, pattern) in patterns.iter().enumerate() {
                let pattern_path = format!("{path}[{index}]");
                for (_, scope) in &flavor_scopes {
                    match resolver.resolve(pattern, *scope, &pattern_path) {
                        Ok(resolved) => {
                            let resolved = resolved.trim().to_string();
                            // Only secret references are left after resolving.
                            if resolved.contains("${{") {
                                push_unique(
                                    &mut errors,
                                    format!(
                                        "{pattern_path} cannot use ${{{{ secrets.NAME }}}}; artifact patterns are matched as written"
                                    ),
                                );
                            } else if let Err(error) = validate_artifact_pattern(&resolved) {
                                push_unique(&mut errors, format!("{pattern_path} {error}"));
                            } else if seen_patterns.insert(resolved.clone()) {
                                artifact_patterns.push(resolved);
                            }
                        }
                        Err(error) => {
                            if is_limit_error(&error) {
                                return Err(vec![error]);
                            }
                            push_unique(&mut errors, error)
                        }
                    }
                }
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(PipelinePlan {
            workflow: selected_workflow.map(|(name, _)| name.to_string()),
            platforms: targets
                .iter()
                .map(|target| target.platform.clone())
                .collect(),
            flutter_version: self.app.flutter.as_ref().map(|v| v.trim().to_string()),
            steps,
            env: resolved_env,
            artifact_patterns,
            build_number_offset: self.build_number_offset,
            runs_on: normalize_runner_labels(&self.runs_on).unwrap_or_default(),
            isolation: self.isolation,
            publish,
        })
    }
}

fn check_flavor_list(
    errors: &mut Vec<String>,
    path: &str,
    flavors: &[String],
    allowed: Option<&[String]>,
) {
    let mut seen = HashSet::new();
    for (index, flavor) in flavors.iter().enumerate() {
        if !is_valid_identifier(flavor) {
            errors.push(format!(
                "{path}[{index}] must start with a letter or digit and use only letters, digits, '-' and '_' (max 64 chars)"
            ));
        } else if !seen.insert(flavor.as_str()) {
            errors.push(format!("{path} contains duplicate flavor '{flavor}'"));
        } else if let Some(allowed) = allowed
            && !allowed.contains(flavor)
        {
            errors.push(format!(
                "{path}[{index}] '{flavor}' is not one of app.flavors"
            ));
        }
    }
    if allowed.is_some() && flavors.is_empty() {
        errors.push(format!(
            "{path} must not be empty; omit it to build every flavor"
        ));
    }
}

fn check_target(
    errors: &mut Vec<String>,
    path: &str,
    platform: &BuildPlatform,
    target: &TargetSpec,
) {
    if let Some(build) = &target.build
        && build.trim().is_empty()
    {
        errors.push(format!("{path}.build must not be empty"));
    }
    if let Some(artifacts) = &target.artifacts
        && artifacts.is_empty()
    {
        errors.push(format!(
            "{path}.artifacts must not be empty; omit it to use the platform defaults"
        ));
    }
    let Some(publish) = target.publish.as_ref().and_then(TargetPublish::spec) else {
        return;
    };
    if let Some(play) = &publish.play {
        if *platform != BuildPlatform::Android {
            errors.push(format!("{path}.publish.play is only available for android"));
        }
        if play.track.trim().is_empty() {
            errors.push(format!("{path}.publish.play.track must not be empty"));
        }
        if let Some(rollout) = play.rollout
            && !(rollout > 0.0 && rollout <= 1.0)
        {
            errors.push(format!(
                "{path}.publish.play.rollout must be greater than 0 and at most 1 (0.05 is 5%)"
            ));
        }
    }
    if let Some(testflight) = &publish.testflight {
        if *platform != BuildPlatform::Ios {
            errors.push(format!(
                "{path}.publish.testflight is only available for ios"
            ));
        }
        if testflight.group.trim().is_empty() {
            errors.push(format!("{path}.publish.testflight.group must not be empty"));
        }
    }
    if publish.play.is_none() && publish.testflight.is_none() {
        errors.push(format!("{path}.publish must set play or testflight"));
    }
}

/// `shared` is the top-level step list when checking a workflow's steps.
fn check_steps(
    errors: &mut Vec<String>,
    path: &str,
    steps: &[StepEntry],
    shared: Option<&[StepEntry]>,
) {
    let mut builds = 0;
    let mut names = HashSet::new();
    for (index, entry) in steps.iter().enumerate() {
        let entry_path = format!("{path}[{index}]");
        match entry {
            StepEntry::Build => {
                builds += 1;
                if builds == 2 {
                    errors.push(format!("{entry_path}: build may appear only once"));
                }
            }
            StepEntry::Ref(name) => match shared {
                None => errors.push(format!(
                    "{entry_path}: '{name}' is not a step; give a step as `name` and `run`, or use `build` to place the targets"
                )),
                Some(shared) => {
                    let defined = shared
                        .iter()
                        .any(|entry| matches!(entry, StepEntry::Inline(step) if step.name.trim() == name.trim()));
                    if !defined {
                        errors.push(format!(
                            "{entry_path} refers to step '{name}', which is not defined in the top-level steps"
                        ));
                    } else if !names.insert(name.trim().to_string()) {
                        errors.push(format!("{entry_path}: step '{name}' appears more than once"));
                    }
                }
            },
            StepEntry::Inline(step) => {
                let name = step.name.trim();
                if name.is_empty() {
                    errors.push(format!("{entry_path}.name must not be empty"));
                } else if name.len() > MAX_STEP_NAME_LENGTH {
                    errors.push(format!(
                        "{entry_path}.name is too long (max {MAX_STEP_NAME_LENGTH} chars)"
                    ));
                } else if name.chars().any(char::is_control) {
                    errors.push(format!("{entry_path}.name must be a single line"));
                } else if !names.insert(name.to_string()) {
                    errors.push(format!("{entry_path} name '{name}' is used by another step"));
                }
                if step.run.trim().is_empty() {
                    errors.push(format!("{entry_path}.run must not be empty"));
                }
                if let Some(reports) = &step.reports {
                    let tests = reports.tests.trim();
                    if let Err(error) = validate_repository_config_path(tests) {
                        errors.push(format!("{entry_path}.reports.tests {error}"));
                    } else if !(tests.ends_with(".json") || tests.ends_with(".xml")) {
                        errors.push(format!(
                            "{entry_path}.reports.tests must be a .json (dart test) or .xml (JUnit) file"
                        ));
                    }
                }
            }
        }
    }
}

fn branch_count(trigger: &Option<BranchTrigger>) -> usize {
    match trigger {
        Some(BranchTrigger::Branches(branches)) => branches.len(),
        _ => 0,
    }
}

fn check_keys(errors: &mut Vec<String>, path: &str, map: &OrderedMap<String, String>) {
    for (key, _) in map.iter() {
        if !is_valid_pipeline_env_key(key) {
            errors.push(format!("{path}.{key} must match [A-Za-z_][A-Za-z0-9_]*"));
        }
    }
}

fn check_triggers(errors: &mut Vec<String>, path: &str, on: &TriggerSpec) {
    for (field, trigger) in [("push", &on.push), ("pull_request", &on.pull_request)] {
        if let Some(BranchTrigger::Branches(branches)) = trigger {
            if branches.is_empty() {
                errors.push(format!(
                    "{path}.{field}.branches must not be empty; use `{field}: true` for every branch"
                ));
            }
            for (index, branch) in branches.iter().enumerate() {
                if branch.trim().is_empty() {
                    errors.push(format!(
                        "{path}.{field}.branches[{index}] must not be empty"
                    ));
                }
            }
        }
    }
    for (index, tag) in on.tag.iter().enumerate() {
        if tag.trim().is_empty() {
            errors.push(format!("{path}.tag[{index}] must not be empty"));
        }
    }
    for (index, pattern) in on.paths.iter().enumerate() {
        if pattern.trim().is_empty() {
            errors.push(format!("{path}.paths[{index}] must not be empty"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "version: 2\ntargets:\n  android:\n    build: flutter build apk --release\n";

    fn errors(yaml: &str) -> String {
        match parse_repository_config(yaml) {
            Ok(_) => panic!("expected errors for:\n{yaml}"),
            Err(error) => error,
        }
    }

    fn plan(yaml: &str, workflow: Option<&str>) -> PipelinePlan {
        parse_repository_config(yaml)
            .unwrap()
            .plan(PlanRequest {
                workflow,
                ..PlanRequest::default()
            })
            .unwrap()
    }

    #[test]
    fn version_picks_the_parser() {
        assert_eq!(parse_repository_config(BASE).unwrap().version(), 2);
        let v1 = parse_repository_config("version: 1\nplatforms: [android]\n").unwrap();
        assert_eq!(v1.version(), 1);
        assert_eq!(
            errors("version: 3\nplatforms: [android]\n"),
            "unsupported config version 3, expected 1 or 2"
        );
        // v1 keeps its own errors for v1-shaped files.
        assert!(errors("platforms: [android]\n").contains("missing field `version`"));
        // v1 fields are not v2 fields.
        let error = errors("version: 2\nplatforms: [android]\n");
        assert!(error.contains("unknown field `platforms`"), "{error}");
        assert!(error.contains("line 2 column 1"), "{error}");
    }

    #[test]
    fn shape_errors_carry_line_and_column() {
        let error = errors(&format!("{BASE}    artifact: ['*.apk']\n"));
        assert!(error.contains("unknown field `artifact`"), "{error}");
        assert!(error.contains("line 5"), "{error}");

        let error = errors("version: 2\ntargets:\n  web:\n    build: x\n");
        assert!(error.contains("unknown variant `web`"), "{error}");
        assert!(error.contains("line 3"), "{error}");

        let error = errors(&format!("{BASE}steps:\n  - 42\n"));
        assert!(
            error.contains("expected `build`, the name of a top-level step"),
            "{error}"
        );

        let error = errors(&format!("{BASE}env:\n  A: x\n  A: y\n"));
        assert!(error.contains("duplicate"), "{error}");
    }

    #[test]
    fn targets_need_a_full_build_command() {
        assert!(errors("version: 2\n").contains("targets must define at least one"));
        assert!(
            errors("version: 2\ntargets:\n  android:\n    artifacts: ['*.apk']\n")
                .contains("targets.android.build is required")
        );
        assert!(
            errors("version: 2\ntargets:\n  ios:\n    build: '  '\n")
                .contains("targets.ios.build must not be empty")
        );
        let error = errors(&format!(
            "{BASE}workflows:\n  release:\n    targets: [ios]\n"
        ));
        assert!(
            error.contains("workflows.release.targets.ios needs a build command"),
            "{error}"
        );
    }

    #[test]
    fn steps_are_named_and_build_runs_where_placed() {
        let placed = plan(
            &format!(
                "{BASE}steps:\n  - name: Get packages\n    run: flutter pub get\n  - build\n  - name: Upload\n    run: ./upload.sh\n"
            ),
            None,
        );
        let summary: Vec<(&str, StepPhase)> = placed
            .steps
            .iter()
            .map(|s| (s.name.as_str(), s.phase))
            .collect();
        assert_eq!(
            summary,
            [
                ("Get packages", StepPhase::PreBuild),
                ("Build Android", StepPhase::Build),
                ("Upload", StepPhase::PostBuild),
            ]
        );

        // Without `build`, the targets run after the listed steps.
        let appended = plan(
            &format!("{BASE}steps:\n  - name: Get packages\n    run: flutter pub get\n"),
            None,
        );
        assert_eq!(appended.steps.last().unwrap().name, "Build Android");

        let error = errors(&format!("{BASE}steps:\n  - build\n  - build\n"));
        assert!(
            error.contains("steps[1]: build may appear only once"),
            "{error}"
        );
        let error = errors(&format!(
            "{BASE}steps:\n  - name: A\n    run: x\n  - name: A\n    run: y\n"
        ));
        assert!(
            error.contains("steps[1] name 'A' is used by another step"),
            "{error}"
        );
        let error = errors(&format!(
            "{BASE}steps:\n  - name: Build Android\n    run: x\n"
        ));
        assert!(
            error.contains("is the name of a target build step"),
            "{error}"
        );
        let error = errors(&format!("{BASE}steps:\n  - name: ''\n    run: ''\n"));
        assert!(error.contains("steps[0].name must not be empty"), "{error}");
        assert!(error.contains("steps[0].run must not be empty"), "{error}");
        let error = errors(&format!("{BASE}steps:\n  - Get packages\n"));
        assert!(
            error.contains("steps[0]: 'Get packages' is not a step"),
            "{error}"
        );
        let error = errors(&format!(
            "{BASE}steps:\n  - name: T\n    run: x\n    reports: {{ tests: ../out.json }}\n"
        ));
        assert!(
            error.contains("steps[0].reports.tests must not contain"),
            "{error}"
        );
        let error = errors(&format!(
            "{BASE}steps:\n  - name: T\n    run: x\n    reports: {{ tests: out.txt }}\n"
        ));
        assert!(
            error.contains(".json (dart test) or .xml (JUnit)"),
            "{error}"
        );
    }

    #[test]
    fn workflows_reference_shared_steps_by_name() {
        let yaml = format!(
            "{BASE}steps:\n  - name: Get packages\n    run: flutter pub get\n  - name: Tests\n    run: flutter test\n  - build\nworkflows:\n  quick:\n    steps: [Get packages, build]\n  full: {{}}\n"
        );
        let names = |plan: PipelinePlan| -> Vec<String> {
            plan.steps.into_iter().map(|step| step.name).collect()
        };
        assert_eq!(
            names(plan(&yaml, Some("quick"))),
            ["Get packages", "Build Android"]
        );
        assert_eq!(
            names(plan(&yaml, Some("full"))),
            ["Get packages", "Tests", "Build Android"]
        );
        let quick = plan(&yaml, Some("quick"));
        assert_eq!(quick.steps[0].source, "steps[0].run");
        assert_eq!(quick.workflow.as_deref(), Some("quick"));

        let error = errors(&format!(
            "{BASE}steps:\n  - name: Get packages\n    run: x\nworkflows:\n  quick:\n    steps: [Lint, build]\n"
        ));
        assert!(
            error.contains(
                "workflows.quick.steps[0] refers to step 'Lint', which is not defined in the top-level steps"
            ),
            "{error}"
        );
    }

    #[test]
    fn workflow_selection_is_explicit_when_ambiguous() {
        let config = parse_repository_config(&format!(
            "{BASE}workflows:\n  adhoc: {{}}\n  release: {{}}\n"
        ))
        .unwrap();
        assert_eq!(config.workflow_names(), ["adhoc", "release"]);
        let error = config.plan(PlanRequest::default()).unwrap_err();
        assert_eq!(
            error,
            "this file defines workflows [adhoc, release]; choose one for the pipeline"
        );
        let error = config
            .plan(PlanRequest {
                workflow: Some("nightly"),
                ..PlanRequest::default()
            })
            .unwrap_err();
        assert_eq!(
            error,
            "workflow 'nightly' is not defined; this file defines [adhoc, release]"
        );

        // A single workflow is picked without asking.
        let only = plan(&format!("{BASE}workflows:\n  adhoc: {{}}\n"), None);
        assert_eq!(only.workflow.as_deref(), Some("adhoc"));

        let v1 = parse_repository_config("version: 1\nplatforms: [android]\n").unwrap();
        assert!(
            v1.plan(PlanRequest {
                workflow: Some("adhoc"),
                ..PlanRequest::default()
            })
            .unwrap_err()
            .contains("workflows need `version: 2`")
        );

        assert!(
            errors(&format!("{BASE}workflows:\n  'bad name': {{}}\n"))
                .contains("workflows.bad name: workflow names must")
        );
    }

    #[test]
    fn vars_resolve_recursively_and_workflows_override_them() {
        let yaml = "version: 2\nvars:\n  MODE: release\n  CMD: flutter build apk --${{ vars.MODE }}\ntargets:\n  android:\n    build: ${{ vars.CMD }}\nenv:\n  TOKEN: ${{ secrets.TOKEN }}\n  LABEL: ${{vars.MODE}}-build\nworkflows:\n  debug:\n    vars:\n      MODE: debug\n  release: {}\n";
        let debug = plan(yaml, Some("debug"));
        assert_eq!(debug.steps[0].run, "flutter build apk --debug");
        assert_eq!(debug.env[1].value, "debug-build");
        // Secret references are left for the runner, which never splices them
        // into shell source.
        assert_eq!(debug.env[0].value, "${{ secrets.TOKEN }}");
        let release = plan(yaml, Some("release"));
        assert_eq!(release.steps[0].run, "flutter build apk --release");
    }

    #[test]
    fn invalid_references_are_reported_with_their_path() {
        let error = errors(&format!("{BASE}env:\n  A: ${{{{ vars.MISSING }}}}\n"));
        assert_eq!(error, "env.A refers to vars.MISSING, which is not defined");

        let error = errors(&format!("{BASE}env:\n  A: ${{{{ env.HOME }}}}\n"));
        assert!(
            error.starts_with("env.A has unsupported expression '${{ env.HOME }}'"),
            "{error}"
        );

        let error = errors(&format!("{BASE}env:\n  A: ${{{{ secrets.X\n"));
        assert_eq!(error, "env.A has an unterminated ${{ expression");

        let error = errors(&format!(
            "{BASE}vars:\n  A: ${{{{ vars.B }}}}\n  B: ok\nsteps:\n  - name: X\n    run: echo ${{{{ vars.C }}}}\n"
        ));
        assert_eq!(error, "steps[0].run refers to vars.C, which is not defined");

        let error = errors(&format!("{BASE}vars:\n  A: ${{{{ vars.NOPE }}}}\n"));
        assert_eq!(error, "vars.A refers to vars.NOPE, which is not defined");

        assert!(
            errors(&format!("{BASE}vars:\n  bad-key: x\n")).contains("vars.bad-key must match")
        );
    }

    #[test]
    fn var_cycles_are_rejected() {
        let error = errors(&format!(
            "{BASE}vars:\n  A: ${{{{ vars.B }}}}\n  B: x ${{{{ vars.C }}}}\n  C: ${{{{ vars.A }}}}\n"
        ));
        // Reported once, not once per member.
        assert_eq!(
            error,
            "vars form a cycle: vars.A -> vars.B -> vars.C -> vars.A"
        );
        let error = errors(&format!("{BASE}vars:\n  SELF: ${{{{ vars.SELF }}}}\n"));
        assert_eq!(error, "vars form a cycle: vars.SELF -> vars.SELF");
        // A cycle created only by a workflow's override is caught for that workflow.
        let error = errors(&format!(
            "{BASE}vars:\n  A: ${{{{ vars.B }}}}\n  B: x\nworkflows:\n  loop:\n    vars:\n      B: ${{{{ vars.A }}}}\n"
        ));
        assert_eq!(error, "vars form a cycle: vars.A -> vars.B -> vars.A");
    }

    fn doubling_vars(seed: &str, count: usize) -> String {
        let mut yaml = format!("version: 2\nvars:\n  V0: '{seed}'\n");
        for index in 1..count {
            let previous = index - 1;
            yaml.push_str(&format!(
                "  V{index}: '${{{{ vars.V{previous} }}}}${{{{ vars.V{previous} }}}}'\n"
            ));
        }
        yaml.push_str(&format!(
            "targets:\n  android:\n    build: echo ${{{{ vars.V{} }}}}\n",
            count - 1
        ));
        yaml
    }

    #[test]
    fn var_expansion_is_bounded() {
        let started = std::time::Instant::now();
        // 23 doubling vars would expand to a 4 MiB command.
        let error = errors(&doubling_vars("xxxxxxxx", 23));
        assert_eq!(error, "vars.V14 expands to more than 64 KiB through vars");
        // Each var is resolved once, so doubling vars with empty values cost
        // a few expansions per var instead of 2^30.
        assert!(parse_repository_config(&doubling_vars("", 30)).is_ok());
        assert!(started.elapsed() < std::time::Duration::from_millis(500));
        // Moderate reuse stays within the limits.
        assert!(parse_repository_config(&doubling_vars("ab", 10)).is_ok());
    }

    #[test]
    fn many_flavors_patterns_and_workflows_are_rejected_quickly() {
        let started = std::time::Instant::now();
        let flavors: Vec<String> = (0..200).map(|i| format!("f{i}")).collect();
        let patterns: Vec<String> = (0..200)
            .map(|i| format!("'out/{i}/${{{{ flavor }}}}/*.apk'"))
            .collect();
        let mut yaml = format!(
            "version: 2\napp:\n  flavors: [{}]\ntargets:\n  android:\n    build: flutter build apk --flavor ${{{{ flavor }}}}\n    artifacts: [{}]\nworkflows:\n",
            flavors.join(", "),
            patterns.join(", ")
        );
        for i in 0..5 {
            yaml.push_str(&format!("  w{i}: {{}}\n"));
        }
        let error = errors(&yaml);
        assert!(started.elapsed() < std::time::Duration::from_millis(500));
        assert!(
            error.contains("app.flavors has 200 flavors; the limit is 32"),
            "{error}"
        );
        assert!(
            error.contains("targets.android.artifacts has 200 patterns; the limit is 64"),
            "{error}"
        );

        let mut many = BASE.to_string() + "workflows:\n";
        for i in 0..40 {
            many.push_str(&format!("  w{i}: {{}}\n"));
        }
        assert_eq!(errors(&many), "workflows has 40 workflows; the limit is 32");
        let long = "x".repeat(MAX_COMMAND_BYTES + 1);
        assert_eq!(
            errors(&format!(
                "version: 2\ntargets:\n  android:\n    build: echo {long}\n"
            )),
            "targets.android.build is longer than 32 KiB"
        );
    }

    #[test]
    fn the_largest_file_within_limits_validates_quickly() {
        let flavors: Vec<String> = (0..MAX_FLAVORS).map(|i| format!("f{i}")).collect();
        let patterns: Vec<String> = (0..MAX_ARTIFACT_PATTERNS)
            .map(|i| format!("'out/{i}/${{{{ flavor }}}}/${{{{ vars.DIR }}}}/*'"))
            .collect();
        let mut yaml = format!(
            "version: 2\napp:\n  flavors: [{}]\nvars:\n  DIR: release\ntargets:\n",
            flavors.join(", ")
        );
        for platform in ["android", "ios", "macos"] {
            yaml.push_str(&format!(
                "  {platform}:\n    build: flutter build --flavor ${{{{ flavor }}}}\n    artifacts: [{}]\n",
                patterns.join(", ")
            ));
        }
        yaml.push_str("workflows:\n");
        for i in 0..MAX_WORKFLOWS {
            yaml.push_str(&format!("  w{i}:\n    vars:\n      DIR: d{i}\n"));
        }
        let started = std::time::Instant::now();
        parse_repository_config(&yaml).unwrap();
        let elapsed = started.elapsed();
        assert!(elapsed < std::time::Duration::from_secs(2), "{elapsed:?}");
    }

    #[test]
    fn expansion_budget_is_shared_across_the_whole_file() {
        // A chain of vars referenced many times from every pattern, flavor
        // and workflow. Each use is cheap, but the uses add up past the
        // budget, which spans every workflow instead of resetting per value.
        let mut yaml = String::from("version: 2\napp:\n  flavors: [");
        yaml.push_str(
            &(0..20)
                .map(|i| format!("f{i}"))
                .collect::<Vec<_>>()
                .join(", "),
        );
        yaml.push_str("]\nvars:\n  V0: ''\n");
        for index in 1..14 {
            let previous = index - 1;
            yaml.push_str(&format!(
                "  V{index}: '${{{{ vars.V{previous} }}}}${{{{ vars.V{previous} }}}}'\n"
            ));
        }
        let uses = "${{ vars.V13 }}".repeat(120);
        yaml.push_str("targets:\n  android:\n    build: flutter build apk\n    artifacts:\n");
        for i in 0..20 {
            yaml.push_str(&format!(
                "      - 'out/{i}/${{{{ flavor }}}}{uses}/*.apk'\n"
            ));
        }
        yaml.push_str("workflows:\n");
        for i in 0..20 {
            yaml.push_str(&format!("  w{i}:\n    vars:\n      V0: ''\n"));
        }
        let started = std::time::Instant::now();
        let error = errors(&yaml);
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "{elapsed:?}"
        );
        assert!(
            error.starts_with("targets.android.artifacts[")
                && error.ends_with(": vars expand more than 250000 times in this file"),
            "{error}"
        );
        assert!(!error.contains('\n'), "{error}");
    }

    #[test]
    fn artifact_patterns_cannot_use_secrets() {
        let error = errors(&format!(
            "{BASE}    artifacts: ['build/${{{{ secrets.DIR }}}}/*.apk']\n"
        ));
        assert_eq!(
            error,
            "targets.android.artifacts[0] cannot use ${{ secrets.NAME }}; artifact patterns are matched as written"
        );
    }

    #[test]
    fn several_workflows_get_a_notice_to_pick_one() {
        let config = parse_repository_config(&format!(
            "{BASE}workflows:\n  adhoc: {{}}\n  release: {{}}\n"
        ))
        .unwrap();
        let notices = config.notices();
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(notices[0].starts_with("this file defines workflows [adhoc, release]"));
        let one = parse_repository_config(&format!("{BASE}workflows:\n  adhoc: {{}}\n")).unwrap();
        assert!(one.notices().is_empty());
    }

    #[test]
    fn step_references_ignore_surrounding_spaces() {
        let yaml = format!(
            "{BASE}steps:\n  - name: 'Get packages '\n    run: flutter pub get\nworkflows:\n  quick:\n    steps: [' Get packages', build]\n"
        );
        let quick = plan(&yaml, Some("quick"));
        assert_eq!(quick.steps[0].name, "Get packages");
    }

    #[test]
    fn quoted_or_decimal_versions_get_a_hint() {
        assert_eq!(
            errors("version: '2'\ntargets: {}\n"),
            "version must be a number; write `version: 2` without quotes"
        );
        assert_eq!(
            errors("version: 2.0\n"),
            "version must be a whole number; write `version: 2`"
        );
    }

    #[test]
    fn flavors_expand_target_builds_only() {
        let yaml = "version: 2\napp:\n  flavors: [free, pro]\ntargets:\n  android:\n    build: flutter build apk --flavor ${{ flavor }}\nworkflows:\n  pro:\n    flavors: [pro]\n  all: {}\n";
        let all = plan(yaml, Some("all"));
        let runs: Vec<&str> = all.steps.iter().map(|s| s.run.as_str()).collect();
        assert_eq!(
            runs,
            [
                "flutter build apk --flavor free",
                "flutter build apk --flavor pro"
            ]
        );
        assert_eq!(all.steps[1].name, "Build Android (pro)");
        assert_eq!(plan(yaml, Some("pro")).steps.len(), 1);

        let error = errors(&format!(
            "{BASE}steps:\n  - name: X\n    run: echo ${{{{ flavor }}}}\n"
        ));
        assert_eq!(
            error,
            "steps[0].run uses ${{ flavor }}, which is only available in a target's build and artifacts"
        );
        let error = errors(
            "version: 2\nvars:\n  F: ${{ flavor }}\ntargets:\n  android:\n    build: x\nenv:\n  A: ${{ vars.F }}\n",
        );
        assert_eq!(
            error,
            "env.A uses ${{ flavor }} (through vars.F), which is only available in a target's build and artifacts"
        );
        let error =
            errors("version: 2\ntargets:\n  android:\n    build: x --flavor ${{ flavor }}\n");
        assert_eq!(
            error,
            "targets.android.build uses ${{ flavor }}, but no flavors are configured in app.flavors"
        );
        let error = errors(&format!(
            "version: 2\napp:\n  flavors: [free]\n{}workflows:\n  x:\n    flavors: [pro]\n",
            &BASE["version: 2\n".len()..]
        ));
        assert!(
            error.contains("workflows.x.flavors[0] 'pro' is not one of app.flavors"),
            "{error}"
        );
        assert!(
            errors("version: 2\napp:\n  flavors: ['a b']\ntargets:\n  android:\n    build: x\n")
                .contains("app.flavors[0] must start with")
        );
    }

    #[test]
    fn artifacts_default_per_target_and_are_validated() {
        let both = plan(
            "version: 2\ntargets:\n  ios:\n    build: flutter build ipa\n  android:\n    build: flutter build apk\n",
            None,
        );
        assert_eq!(both.platforms, [BuildPlatform::Ios, BuildPlatform::Android]);
        assert_eq!(
            both.artifact_patterns,
            [
                "build/ios/ipa/*.ipa",
                "build/app/outputs/flutter-apk/*.apk",
                "build/app/outputs/bundle/**/*.aab"
            ]
        );
        let error = errors(&format!("{BASE}    artifacts: [build/app.apk]\n"));
        assert_eq!(
            error,
            "targets.android.artifacts[0] must contain '*' or '?'"
        );
    }

    #[test]
    fn platform_selection_narrows_targets() {
        let config = parse_repository_config(
            "version: 2\ntargets:\n  android:\n    build: a\n  ios:\n    build: i\n",
        )
        .unwrap();
        let ios = config
            .plan(PlanRequest {
                selected_platforms: Some(&[BuildPlatform::Ios]),
                ..PlanRequest::default()
            })
            .unwrap();
        assert_eq!(ios.platforms, [BuildPlatform::Ios]);
        assert_eq!(ios.steps.len(), 1);
        assert_eq!(ios.artifact_patterns, ["build/ios/ipa/*.ipa"]);
        let error = config
            .plan(PlanRequest {
                selected_platforms: Some(&[BuildPlatform::Macos]),
                ..PlanRequest::default()
            })
            .unwrap_err();
        assert_eq!(
            error,
            "selected_platforms must be unique and configured by the workflow"
        );
    }

    #[test]
    fn on_matches_pushes_pull_requests_and_tags() {
        let config = parse_repository_config(&format!(
            "{BASE}on:\n  push: {{ branches: [main, release/*] }}\n  pull_request: true\n  tag: v*\n  paths: [lib/, pubspec.*]\n"
        ))
        .unwrap();
        let on = config.triggers(None).unwrap();
        let push = |branch| TriggerEvent::Push { branch };
        assert!(on.matches_ref(push("main")));
        assert!(on.matches_ref(push("release/1.2")));
        assert!(!on.matches_ref(push("feature/x")));
        assert!(on.matches_ref(TriggerEvent::PullRequest {
            target_branch: Some("anything")
        }));
        assert!(on.matches_ref(TriggerEvent::Tag { tag: "v1.0.0" }));
        assert!(!on.matches_ref(TriggerEvent::Tag { tag: "nightly" }));

        let files = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert!(on.paths_match(Some(&files(&["lib/src/app.dart"]))));
        assert!(on.paths_match(Some(&files(&["README.md", "pubspec.yaml"]))));
        assert!(!on.paths_match(Some(&files(&["README.md", "docs/a.md"]))));
        // An unknown change set never suppresses a build.
        assert!(on.paths_match(None));

        // Fields left out, or `false`, never start a build.
        let tags_only =
            parse_repository_config(&format!("{BASE}on:\n  push: false\n  tag: [v*, rc-?]\n"))
                .unwrap();
        let on = tags_only.triggers(None).unwrap();
        assert!(!on.matches_ref(push("main")));
        assert!(!on.matches_ref(TriggerEvent::PullRequest {
            target_branch: Some("main")
        }));
        assert!(on.matches_ref(TriggerEvent::Tag { tag: "rc-1" }));
        assert!(!on.has_path_filters());
        assert!(on.paths_match(Some(&files(&["anything"]))));
    }

    #[test]
    fn pull_request_branches_name_the_target_branch() {
        let config = parse_repository_config(&format!(
            "{BASE}on:\n  pull_request: {{ branches: [main] }}\n"
        ))
        .unwrap();
        let on = config.triggers(None).unwrap();
        assert!(on.matches_ref(TriggerEvent::PullRequest {
            target_branch: Some("main")
        }));
        assert!(!on.matches_ref(TriggerEvent::PullRequest {
            target_branch: Some("develop")
        }));
        assert!(!on.matches_ref(TriggerEvent::PullRequest {
            target_branch: None
        }));
        assert!(!on.matches_ref(TriggerEvent::Push { branch: "main" }));
    }

    #[test]
    fn a_workflow_on_replaces_the_top_level_on() {
        let yaml = format!(
            "{BASE}on:\n  push: true\nworkflows:\n  adhoc:\n    on:\n      pull_request: true\n  release:\n    on:\n      tag: v*\n  nightly: {{}}\n"
        );
        let config = parse_repository_config(&yaml).unwrap();
        let push = TriggerEvent::Push { branch: "main" };
        let pull_request = TriggerEvent::PullRequest {
            target_branch: Some("main"),
        };
        let tag = TriggerEvent::Tag { tag: "v2.0.0" };
        let matches = |workflow, event| {
            config
                .triggers(workflow)
                .is_some_and(|on| on.matches_ref(event))
        };
        assert!(matches(Some("adhoc"), pull_request));
        assert!(!matches(Some("adhoc"), push));
        assert!(matches(Some("release"), tag));
        assert!(!matches(Some("release"), push));
        // No `on` of its own: the top level applies.
        assert!(matches(Some("nightly"), push));
        // Several workflows and none named: only the top level can apply.
        assert!(matches(None, push));
        assert!(!matches(None, tag));

        let single = parse_repository_config(&format!(
            "{BASE}workflows:\n  release:\n    on:\n      tag: v*\n"
        ))
        .unwrap();
        assert!(single.triggers(None).unwrap().matches_ref(tag));
        // No `on` anywhere, and v1 files: the pipeline's settings apply.
        assert!(
            parse_repository_config(BASE)
                .unwrap()
                .triggers(None)
                .is_none()
        );
        assert!(
            parse_repository_config("version: 1\nplatforms: [android]\n")
                .unwrap()
                .triggers(None)
                .is_none()
        );
    }

    #[test]
    fn triggers_and_publish_are_validated_and_reported() {
        let config = parse_repository_config(&format!(
            "{BASE}    publish:\n      play: {{ track: closed, rollout: 0.05 }}\non:\n  push: {{ branches: [main] }}\n  pull_request: true\n  tag: v*\n"
        ))
        .unwrap();
        let notices = config.notices();
        assert_eq!(notices.len(), 3, "{notices:?}");
        assert!(notices[0].starts_with("`on` replaces the pipeline's trigger settings"));
        assert!(notices[1].starts_with("`publish` replaces the pipeline's Publish settings"));
        assert!(
            notices[2].starts_with("targets.android.publish.play uploads an App Bundle"),
            "{notices:?}"
        );

        assert!(
            errors(&format!(
                "{BASE}    publish:\n      testflight: {{ group: QA }}\n"
            ))
            .contains("targets.android.publish.testflight is only available for ios")
        );
        assert!(
            errors(&format!(
                "{BASE}    publish:\n      play: {{ track: internal, rollout: 5 }}\n"
            ))
            .contains("rollout must be greater than 0 and at most 1")
        );
        assert!(
            errors(&format!("{BASE}on:\n  push: {{ branches: [] }}\n"))
                .contains("on.push.branches must not be empty")
        );
        assert!(
            errors(&format!("{BASE}on:\n  push: main\n"))
                .contains("expected true, false, or a mapping with `branches`")
        );
        assert!(parse_repository_config(BASE).unwrap().notices().is_empty());
    }

    #[test]
    fn v2_plans_project_into_the_v1_preview_shape() {
        let config = plan(
            "version: 2\napp:\n  flavors: [a, b]\ntargets:\n  android:\n    build: flutter build apk --flavor ${{ flavor }}\nsteps:\n  - name: Get\n    run: flutter pub get\n  - build\n  - name: Done\n    run: echo done\n",
            None,
        )
        .to_execution_config();
        assert_eq!(config.commands.pre_build, ["flutter pub get"]);
        assert!(config.commands.build.is_empty());
        assert_eq!(config.commands.post_build, ["echo done"]);
        assert_eq!(
            config.platform_commands.android.as_deref(),
            Some("flutter build apk --flavor a && flutter build apk --flavor b")
        );
    }

    #[test]
    fn v1_selection_keeps_its_rules() {
        let config = parse_repository_config(
            "version: 1\nplatforms: [android, ios]\ncommands:\n  build: [make all]\n",
        )
        .unwrap();
        let error = config
            .plan(PlanRequest {
                selected_platforms: Some(&[BuildPlatform::Android]),
                ..PlanRequest::default()
            })
            .unwrap_err();
        assert!(error.contains("shared commands.build cannot be filtered safely"));
    }

    const PUBLISHING: &str = "version: 2\ntargets:\n  android:\n    build: flutter build appbundle --release\n    publish:\n      play: { track: internal }\n  ios:\n    build: flutter build ipa --release\n    publish:\n      testflight: { group: QA }\n";

    fn publish_of(yaml: &str, workflow: Option<&str>) -> PlanPublish {
        plan(yaml, workflow).publish
    }

    fn tracks(publish: &PlanPublish) -> (Option<String>, Option<String>) {
        (
            publish.play.as_ref().map(|play| play.source.clone()),
            publish
                .testflight
                .as_ref()
                .map(|testflight| testflight.source.clone()),
        )
    }

    #[test]
    fn publish_resolves_per_workflow() {
        let top = publish_of(PUBLISHING, None);
        assert_eq!(
            top.play.as_ref().unwrap().spec,
            PlayPublishSpec {
                track: "internal".to_string(),
                rollout: None,
                draft: None,
            }
        );
        assert_eq!(
            tracks(&top),
            (
                Some("targets.android.publish.play".to_string()),
                Some("targets.ios.publish.testflight".to_string())
            )
        );

        let yaml = format!(
            "{PUBLISHING}workflows:\n  all: {{}}\n  listed:\n    targets: [ios]\n  artifacts_only:\n    targets:\n      android:\n        artifacts: [build/app/outputs/bundle/**/*.aab]\n  apk:\n    targets:\n      android:\n        build: flutter build apk --release\n  own:\n    targets:\n      android:\n        build: flutter build appbundle --release\n        publish:\n          play: {{ track: production, rollout: 0.1 }}\n  off:\n    targets:\n      android:\n        publish: false\n      ios:\n        publish: null\n"
        );
        // No targets, or a list: the top-level targets as they are.
        assert_eq!(tracks(&publish_of(&yaml, Some("all"))), tracks(&top));
        assert_eq!(
            tracks(&publish_of(&yaml, Some("listed"))),
            (None, Some("targets.ios.publish.testflight".to_string()))
        );
        // Keeping the top-level build keeps its publish.
        assert_eq!(
            tracks(&publish_of(&yaml, Some("artifacts_only"))),
            (Some("targets.android.publish.play".to_string()), None)
        );
        // A new build command does not inherit where the old one published.
        assert!(publish_of(&yaml, Some("apk")).is_empty());
        let own = publish_of(&yaml, Some("own"));
        assert_eq!(
            own.play.as_ref().unwrap().source,
            "workflows.own.targets.android.publish.play"
        );
        assert_eq!(own.play.unwrap().spec.rollout, Some(0.1));
        // `false` or `null` drops an inherited publish and keeps the build.
        let off = plan(&yaml, Some("off"));
        assert!(off.publish.is_empty());
        assert_eq!(off.platforms.len(), 2);

        assert!(
            errors(&format!("{BASE}    publish: true\n"))
                .contains("publish: true is not supported")
        );
    }

    #[test]
    fn publish_follows_the_platform_selection() {
        let config = parse_repository_config(PUBLISHING).unwrap();
        let android_only = config
            .plan(PlanRequest {
                selected_platforms: Some(&[BuildPlatform::Android]),
                ..PlanRequest::default()
            })
            .unwrap()
            .publish;
        assert!(android_only.play.is_some());
        assert!(android_only.testflight.is_none());
        // Platforms without publish keep the pipeline's Publish settings.
        assert!(publish_of(BASE, None).is_empty());
    }

    fn settings(play: serde_json::Value) -> RunnerPublishPlan {
        let mut config = serde_json::json!({
            "enabled": true,
            "service_account_secret": "PLAY_KEY",
            "package_name": "com.example.app",
            "track": "production",
            "release_notes": "changelog",
        });
        config
            .as_object_mut()
            .unwrap()
            .extend(play.as_object().unwrap().clone());
        RunnerPublishPlan {
            changelog: None,
            google_play: Some(serde_json::from_value(config).unwrap()),
            app_store: Some(app_store_settings()),
        }
    }

    fn play_error(yaml: &str, workflow: Option<&str>) -> String {
        let publish = publish_of(yaml, workflow);
        let error = publish.play.as_ref().unwrap().error.clone().unwrap();
        // The build runs; only the publish step fails, with this message.
        assert_eq!(
            publish
                .apply(settings(serde_json::json!({})))
                .google_play
                .unwrap_err(),
            error
        );
        error
    }

    #[test]
    fn publishing_several_flavors_fails_only_the_publish_step() {
        let flavored = "version: 2\napp:\n  flavors: [free, pro]\ntargets:\n  android:\n    build: flutter build appbundle --flavor ${{ flavor }}\n    publish:\n      play: { track: internal }\n";
        let plan = plan(flavored, None);
        assert_eq!(plan.steps.len(), 2, "both flavors still build");
        assert_eq!(
            play_error(flavored, None),
            "targets.android.publish uploads one build, but app.flavors builds 2 flavors (free, pro); narrow it with `flavors: [<one flavor>]` on the workflow"
        );
        let notices = parse_repository_config(flavored).unwrap().notices();
        assert!(
            notices.iter().any(|notice| notice
                .ends_with("; the build runs, and its publish step fails")
                && notice.contains("builds 2 flavors")),
            "{notices:?}"
        );
        let narrowed = format!("{flavored}workflows:\n  release:\n    flavors: [pro]\n");
        let publish = publish_of(&narrowed, Some("release"));
        assert_eq!(publish.play.as_ref().unwrap().error, None);
        let widened = format!("{flavored}workflows:\n  release:\n    flavors: [free, pro]\n");
        assert!(
            play_error(&widened, Some("release"))
                .contains("but workflows.release.flavors builds 2 flavors")
        );
    }

    #[test]
    fn invalid_publish_values_fail_only_the_publish_step() {
        let play = |publish: &str| {
            format!(
                "version: 2\ntargets:\n  android:\n    build: flutter build appbundle\n    publish:\n      {publish}\n"
            )
        };
        assert!(
            play_error(&play("play: { track: 'my track' }"), None)
                .contains("track 'my track' must be internal, closed, open, production")
        );
        assert!(
            play_error(&play("play: { track: '${{ vars.MISSING }}' }"), None)
                .contains("vars.MISSING")
        );
        assert!(
            play_error(&play("play: { track: '${{ secrets.TRACK }}' }"), None)
                .contains("cannot use ${{ secrets.NAME }}")
        );

        let group = |name: &str| {
            let yaml = format!(
                "version: 2\ntargets:\n  ios:\n    build: flutter build ipa\n    publish:\n      testflight: {{ group: '{name}' }}\n"
            );
            publish_of(&yaml, None).testflight.unwrap().error
        };
        assert_eq!(group("QA"), None);
        assert!(group("QA, Beta").unwrap().contains("without commas"));
        assert!(
            group("App_Store")
                .unwrap()
                .contains("cannot be named app_store")
        );
        assert!(group(&"g".repeat(129)).unwrap().contains("longer than 128"));
        assert_eq!(group(&"g".repeat(128)), None);
    }

    #[test]
    fn publish_values_resolve_vars_per_workflow() {
        let yaml = "version: 2\nvars:\n  TRACK: internal\n  GROUP: QA\ntargets:\n  android:\n    build: flutter build appbundle\n    publish:\n      play: { track: '${{ vars.TRACK }}' }\n  ios:\n    build: flutter build ipa\n    publish:\n      testflight: { group: '${{ vars.GROUP }} testers' }\nworkflows:\n  beta: {}\n  release:\n    vars:\n      TRACK: production\n      GROUP: External\n";
        let beta = publish_of(yaml, Some("beta"));
        assert_eq!(beta.play.unwrap().spec.track, "internal");
        assert_eq!(beta.testflight.unwrap().spec.group, "QA testers");
        let release = publish_of(yaml, Some("release"));
        assert_eq!(release.play.as_ref().unwrap().spec.track, "production");
        assert_eq!(
            release.testflight.as_ref().unwrap().spec.group,
            "External testers"
        );
        let effective = release.apply(settings(serde_json::json!({})));
        assert_eq!(effective.google_play.unwrap().unwrap().track, "production");
        assert_eq!(
            effective
                .app_store
                .unwrap()
                .unwrap()
                .config
                .testflight_groups,
            vec!["External testers"]
        );
    }

    #[test]
    fn rollouts_play_does_not_allow_are_noted_and_adjusted() {
        let yaml = "version: 2\ntargets:\n  android:\n    build: flutter build appbundle\n    publish:\n      play: { track: internal, rollout: 0.5 }\nworkflows:\n  internal: {}\n  draft:\n    targets:\n      android:\n        publish:\n          play: { track: production, rollout: 0.5, draft: true }\n";
        let notices = parse_repository_config(yaml).unwrap().notices();
        assert!(
            notices.iter().any(|notice| notice
                == "workflow internal: targets.android.publish.play: the internal track has no staged rollouts, so the release goes to every internal tester"),
            "{notices:?}"
        );
        assert!(
            notices.iter().any(|notice| notice
                == "workflow draft: workflows.draft.targets.android.publish.play: a draft has no staged rollout, so `rollout` is ignored"),
            "{notices:?}"
        );

        let internal = publish_of(yaml, Some("internal")).apply(settings(serde_json::json!({})));
        let config = internal.google_play.unwrap().unwrap();
        assert_eq!(
            (config.track.as_str(), config.rollout_percent),
            ("internal", None)
        );
        assert_eq!(internal.notes.len(), 1);
        let draft = publish_of(yaml, Some("draft")).apply(settings(serde_json::json!({})));
        let config = draft.google_play.unwrap().unwrap();
        assert!(config.draft);
        assert_eq!(config.rollout_percent, None);
        assert!(draft.notes[0].starts_with("A draft has no staged rollout"));
    }

    #[test]
    fn omitted_rollout_and_draft_keep_the_play_settings() {
        let yaml = |play: &str| {
            format!(
                "version: 2\ntargets:\n  android:\n    build: flutter build appbundle\n    publish:\n      play: {play}\n"
            )
        };
        // A 20% staged rollout in the settings stays with only a track.
        let staged = publish_of(&yaml("{ track: closed }"), None)
            .apply(settings(serde_json::json!({ "rollout_percent": 20.0 })));
        let config = staged.google_play.unwrap().unwrap();
        assert_eq!(config.track, "closed");
        assert_eq!(config.rollout_percent, Some(20.0));
        assert!(!config.draft);
        // A draft in the settings stays a draft unless the file says so.
        let draft = settings(serde_json::json!({ "draft": true }));
        let kept = publish_of(&yaml("{ track: closed }"), None).apply(draft.clone());
        assert!(kept.google_play.unwrap().unwrap().draft);
        let live =
            publish_of(&yaml("{ track: closed, draft: false, rollout: 0.1 }"), None).apply(draft);
        let config = live.google_play.unwrap().unwrap();
        assert!(!config.draft);
        assert_eq!(config.rollout_percent, Some(10.0));
        // The combination follows Play's rules: the settings' 20% does not
        // apply on the internal track, or to a draft from the file.
        let internal = publish_of(&yaml("{ track: internal }"), None)
            .apply(settings(serde_json::json!({ "rollout_percent": 20.0 })));
        assert_eq!(internal.google_play.unwrap().unwrap().rollout_percent, None);
        assert_eq!(internal.notes.len(), 1);
        let file_draft = publish_of(&yaml("{ track: closed, draft: true }"), None)
            .apply(settings(serde_json::json!({ "rollout_percent": 20.0 })));
        let config = file_draft.google_play.unwrap().unwrap();
        assert!(config.draft);
        assert_eq!(config.rollout_percent, None);
    }

    #[test]
    fn publish_to_play_without_a_bundle_build_is_noticed() {
        let notices = |yaml: &str| parse_repository_config(yaml).unwrap().notices();
        assert_eq!(notices(PUBLISHING).len(), 1);
        let yaml = format!(
            "{PUBLISHING}workflows:\n  apk:\n    targets:\n      android:\n        build: flutter build apk\n        publish:\n          play: {{ track: internal }}\n  release: {{}}\n"
        );
        let notices = notices(&yaml);
        assert_eq!(notices.len(), 3, "{notices:?}");
        assert!(notices[2].starts_with(
            "workflow apk: workflows.apk.targets.android.publish.play uploads an App Bundle, but the android build does not run `flutter build appbundle`"
        ));
    }

    fn play_settings() -> GooglePlayPublishConfig {
        serde_json::from_value(serde_json::json!({
            "enabled": true,
            "service_account_secret": "PLAY_KEY",
            "package_name": "com.example.app",
            "track": "production",
            "rollout_percent": 20.0,
            "release_notes": "changelog",
        }))
        .unwrap()
    }

    fn app_store_settings() -> RunnerAppStorePublishPlan {
        RunnerAppStorePublishPlan {
            config: crate::AppStorePublishConfig {
                enabled: true,
                destination: AppStoreDestination::AppStore,
                submit_for_review: true,
                uses_non_exempt_encryption: true,
                ..Default::default()
            },
            credentials: crate::RunnerAppStoreUploadCredentials {
                key_id: "KEY".to_string(),
                issuer_id: "ISSUER".to_string(),
                private_key_pem: "PEM".to_string(),
            },
        }
    }

    #[test]
    fn file_publish_replaces_where_the_settings_upload() {
        let settings = RunnerPublishPlan {
            changelog: Some("Fixes".to_string()),
            google_play: Some(play_settings()),
            app_store: Some(app_store_settings()),
        };
        let yaml = "version: 2\ntargets:\n  android:\n    build: flutter build appbundle\n    publish:\n      play: { track: closed, rollout: 0.07 }\n  ios:\n    build: flutter build ipa\n    publish:\n      testflight: { group: Beta }\n";
        let effective = publish_of(yaml, None).apply(settings.clone());
        assert_eq!(effective.changelog.as_deref(), Some("Fixes"));
        assert!(effective.notes.is_empty());
        let play = effective.google_play.unwrap().unwrap();
        assert_eq!(
            play,
            GooglePlayPublishConfig {
                track: "closed".to_string(),
                rollout_percent: Some(7.0),
                ..play_settings()
            }
        );
        let app_store = effective.app_store.unwrap().unwrap();
        assert_eq!(
            app_store.config.destination,
            AppStoreDestination::Testflight
        );
        assert_eq!(app_store.config.testflight_groups, vec!["Beta"]);
        assert!(!app_store.config.submit_for_review);
        assert!(app_store.config.uses_non_exempt_encryption);
        assert_eq!(app_store.credentials.key_id, "KEY");

        // A full rollout reads like the settings form saves it.
        let full = "version: 2\ntargets:\n  android:\n    build: flutter build appbundle\n    publish:\n      play: { track: production, rollout: 1, draft: false }\n";
        let play = publish_of(full, None)
            .apply(settings.clone())
            .google_play
            .unwrap()
            .unwrap();
        assert_eq!(play.rollout_percent, None);
        assert!(!play.draft);

        // Stores the file does not name keep the settings as they are.
        let android = publish_of(full, None).apply(settings.clone());
        assert_eq!(
            android.app_store.unwrap().unwrap().config,
            settings.app_store.as_ref().unwrap().config
        );
        let none = PlanPublish::default().apply(settings);
        assert_eq!(none.google_play.unwrap(), Some(play_settings()));
    }

    #[test]
    fn file_publish_without_store_settings_fails_with_the_settings_to_set() {
        let settings = RunnerPublishPlan {
            changelog: None,
            google_play: None,
            app_store: None,
        };
        let effective = publish_of(PUBLISHING, None).apply(settings);
        let play = effective.google_play.unwrap_err();
        assert!(
            play.starts_with(
                "targets.android.publish.play publishes with the pipeline's Google Play settings, which are off"
            ),
            "{play}"
        );
        assert!(play.contains("\"Upload after each build\""), "{play}");
        let app_store = effective.app_store.unwrap_err();
        assert!(
            app_store.contains("turn on App Store Connect")
                && app_store.contains("App Store Connect API key"),
            "{app_store}"
        );
        // Without a file publish, missing settings just mean no upload.
        let none = PlanPublish::default().apply(RunnerPublishPlan {
            changelog: None,
            google_play: None,
            app_store: None,
        });
        assert_eq!(none.google_play.unwrap(), None);
        assert!(none.app_store.unwrap().is_none());
    }

    #[test]
    fn plan_output_describes_each_upload() {
        let describe = |play: &str| {
            let yaml = format!(
                "version: 2\ntargets:\n  android:\n    build: flutter build appbundle\n    publish:\n      play: {play}\n  ios:\n    build: flutter build ipa\n    publish:\n      testflight: {{ group: QA }}\n"
            );
            publish_of(&yaml, None).describe()
        };
        assert_eq!(
            describe("{ track: production, rollout: 0.05, draft: false }"),
            vec![
                "android publishes to the Google Play production track to 5% of users (targets.android.publish.play)",
                "ios publishes to the TestFlight group QA (targets.ios.publish.testflight)",
            ]
        );
        assert_eq!(
            describe("{ track: closed }")[0],
            "android publishes to the Google Play closed track with the rollout and draft setting from the pipeline's Google Play settings (targets.android.publish.play)"
        );
        assert_eq!(
            describe("{ track: closed, draft: true }")[0],
            "android publishes to the Google Play closed track as a draft (targets.android.publish.play)"
        );
        assert_eq!(
            describe("{ track: closed, rollout: 1 }")[0],
            "android publishes to the Google Play closed track to all users with the draft setting from the pipeline's Google Play settings (targets.android.publish.play)"
        );
        assert!(
            describe("{ track: 'a b' }")[0]
                .starts_with("android publishes to Google Play (targets.android.publish.play), but its publish step will fail: ")
        );
    }
}
