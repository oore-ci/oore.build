//! `.oore.yaml` import, the inverse of the export, over generated settings
//! and the golden files.
//!
//! - Settings exported, imported and exported again give the same file.
//! - `tests/fixtures/pipeline-export/<name>.json` holds editor settings, and
//!   `<name>.yaml` the file they export to, so reviews show what users get;
//!   run with `UPDATE_GOLDEN=1` to rewrite it. Importing a golden file and
//!   exporting it again gives the same file.
//! - The `oore pipeline migrate` goldens, which the export did not write,
//!   either import or say why not, and a second round trip is stable.
//!
//! These live outside the library's unit tests because they plan hundreds
//! of files, which would slow the library's timing checks running beside
//! them.

use std::path::{Path, PathBuf};

use oore_contract::pipeline_export::{FormExport, FormSettings, export_form_settings};
use oore_contract::pipeline_import::{FormImport, ImportContext, import_form_settings};
use oore_contract::{
    AppStoreDestination, AppStorePublishConfig, BuildIsolation, BuildPlatform,
    ExportPipelineYamlRequest, GooglePlayPublishConfig, GooglePlayReleaseNotesSource,
    PipelineCommandStages, PipelineEnvVar, PipelineExecutionConfig, ScheduleTrigger, TriggerConfig,
    default_artifact_patterns_for_platforms,
};

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

fn json<T: serde::Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}

/// Everything the editor holds, owned, for generated cases.
#[derive(Debug, Clone)]
struct Case {
    execution: PipelineExecutionConfig,
    triggers: Option<TriggerConfig>,
    google_play: Option<GooglePlayPublishConfig>,
    app_store: Option<AppStorePublishConfig>,
}

impl Case {
    fn settings(&self) -> FormSettings<'_> {
        FormSettings {
            execution: &self.execution,
            triggers: self.triggers.as_ref(),
            google_play: self.google_play.as_ref(),
            app_store: self.app_store.as_ref(),
        }
    }

    fn context(&self, with_base: bool) -> ImportContext<'_> {
        ImportContext {
            execution: with_base.then_some(&self.execution),
            triggers: self.triggers.as_ref(),
            google_play: self.google_play.as_ref(),
            app_store: self.app_store.as_ref(),
        }
    }

    /// Export the import's settings with this case's Publish settings.
    fn reexport(&self, import: &FormImport) -> String {
        let execution = import
            .execution
            .as_ref()
            .unwrap_or_else(|| panic!("not imported: {:?}\n{self:?}", import.unsupported));
        export_form_settings(FormSettings {
            execution,
            triggers: import.triggers.as_ref().or(self.triggers.as_ref()),
            google_play: self.google_play.as_ref(),
            app_store: self.app_store.as_ref(),
        })
        .unwrap_or_else(|error| panic!("{error}\n{self:?}"))
        .yaml
    }
}

/// A small deterministic generator, so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() % items.len() as u64) as usize]
    }

    fn some(&mut self, items: &[&str], max: usize) -> Vec<String> {
        let count = (self.next() % (max as u64 + 1)) as usize;
        (0..count).map(|_| self.pick(items).to_string()).collect()
    }

    fn chance(&mut self) -> bool {
        self.next().is_multiple_of(2)
    }
}

fn generate(rng: &mut Rng, build_commands: bool) -> Case {
    let mut platforms = Vec::new();
    for platform in [
        BuildPlatform::Android,
        BuildPlatform::Ios,
        BuildPlatform::Macos,
    ] {
        if rng.chance() {
            platforms.push(platform);
        }
    }
    if platforms.is_empty() {
        platforms.push(
            rng.pick(&[BuildPlatform::Ios, BuildPlatform::Android])
                .clone(),
        );
    }
    // The form builds targets in a fixed order, so generated settings use it.
    let commands = [
        "dart run build_runner build",
        "echo ${{ secrets.SENTRY_DSN }} > dsn.txt",
        "echo 'a: b' # quoted",
        "flutter pub get",
        "flutter test",
        "./scripts/upload-symbols.sh",
        "yes",
    ];
    let args = [
        vec![],
        vec!["--split-per-abi".to_string()],
        vec![
            "--obfuscate".to_string(),
            "--split-debug-info=build/symbols".to_string(),
        ],
    ];
    let mut execution = PipelineExecutionConfig {
        platforms: platforms.clone(),
        flutter_version: rng
            .chance()
            .then(|| rng.pick(&["3.24.5", "stable"]).to_string()),
        commands: PipelineCommandStages {
            pre_build: rng.some(&commands, 2),
            build: if build_commands {
                rng.some(&["./scripts/sign.sh", "flutter build web"], 1)
            } else {
                Vec::new()
            },
            post_build: rng.some(&commands, 2),
        },
        env: Vec::new(),
        build_number_offset: *rng.pick(&[0, 100]),
        runs_on: if rng.chance() {
            vec!["arm64".to_string(), "xcode-16".to_string()]
        } else {
            Vec::new()
        },
        isolation: *rng.pick(&[BuildIsolation::Host, BuildIsolation::Vm]),
        ..PipelineExecutionConfig::default()
    };
    execution.platform_build_args.android = rng.pick(&args).clone();
    execution.platform_build_args.ios = rng.pick(&args[..2]).clone();
    if rng.chance() {
        execution.platform_commands.ios = Some("flutter build ipa --release".to_string());
        execution.platform_build_args.ios = Vec::new();
    }
    if rng.chance() {
        execution.platform_commands.android = Some("flutter build appbundle --release".to_string());
        execution.platform_build_args.android = Vec::new();
    }
    for (key, value) in [
        ("API_URL", "https://staging.example.com"),
        ("TOKEN", "${{ secrets.TOKEN }}"),
        ("FLAG", "on"),
    ] {
        if rng.chance() {
            execution.env.push(PipelineEnvVar {
                key: key.to_string(),
                value: value.to_string(),
            });
        }
    }
    execution.artifact_patterns = match rng.next() % 3 {
        0 => default_artifact_patterns_for_platforms(&platforms),
        1 => vec![
            "build/**/*.apk".to_string(),
            "build/ios/ipa/*.ipa".to_string(),
        ],
        _ => vec!["build/app/outputs/flutter-apk/*.apk".to_string()],
    };
    let trigger_cases = [
        None,
        Some(triggers(
            serde_json::json!({ "events": [], "branches": [] }),
        )),
        Some(triggers(
            serde_json::json!({ "events": ["push"], "branches": ["main", "release/*"] }),
        )),
        Some(triggers(serde_json::json!({
            "events": ["push", "tag_push"],
            "branches": ["main"],
            "tags": ["v*"],
            "paths": ["lib/"],
        }))),
        Some(triggers(
            serde_json::json!({ "events": ["pull_request"], "branches": [], "paths": ["lib/**"] }),
        )),
        Some(triggers(
            serde_json::json!({ "events": ["tag_push"], "branches": [], "tags": ["v*"] }),
        )),
        // These do not map, so the file has no `on` and they stay.
        Some(triggers(
            serde_json::json!({ "events": ["push"], "branches": [], "paths_ignore": ["docs/**"] }),
        )),
        Some(TriggerConfig {
            schedules: vec![ScheduleTrigger {
                cron: "@daily".to_string(),
                branch: None,
            }],
            ..triggers(serde_json::json!({ "events": ["pull_request"], "branches": ["main"] }))
        }),
    ];
    let triggers = rng.pick(&trigger_cases).clone();
    let google_play = match rng.next() % 3 {
        0 => Some(play("internal", None, false)),
        1 => Some(play("production", Some(33.3), true)),
        _ => None,
    };
    let app_store = match rng.next() % 3 {
        0 => Some(app_store(&["QA"])),
        1 => Some(app_store(&["QA", "Beta"])),
        _ => None,
    };
    Case {
        execution,
        triggers,
        google_play,
        app_store,
    }
}

#[test]
fn export_import_export_gives_the_same_file() {
    let mut rng = Rng(0x0005_eed0_0460);
    for _ in 0..150 {
        let case = generate(&mut rng, true);
        let yaml = export_form_settings(case.settings())
            .unwrap_or_else(|error| panic!("{error}\n{case:?}"))
            .yaml;
        // With the editor's settings as the base, nothing changes.
        let import = import_form_settings(&yaml, case.context(true))
            .unwrap_or_else(|error| panic!("{error}\n{yaml}"));
        assert!(
            import.unsupported.is_empty(),
            "{:?}\n{yaml}",
            import.unsupported
        );
        assert_eq!(case.reexport(&import), yaml, "{case:?}");
        assert_eq!(
            json(import.execution.as_ref().unwrap()),
            json(&case.execution),
            "{yaml}"
        );
        if let Some(imported) = &import.triggers {
            assert_eq!(Some(json(imported)), case.triggers.as_ref().map(json));
        }
        assert!(
            import.notes.iter().all(|note| !note.contains("Step names")),
            "{:?}",
            import.notes
        );
    }
}

#[test]
fn a_file_imported_without_the_editor_settings_exports_the_same() {
    let mut rng = Rng(0x0bad_cafe_0460);
    for _ in 0..150 {
        // Build commands export as steps after the targets, which only
        // the editor's settings tell apart from post-build commands.
        let case = generate(&mut rng, false);
        let yaml = export_form_settings(case.settings()).unwrap().yaml;
        let import = import_form_settings(&yaml, case.context(false)).unwrap();
        assert_eq!(case.reexport(&import), yaml, "{case:?}");
    }
    // With Build commands, a second round trip changes nothing more.
    let mut rng = Rng(0x0dd_ba11_0460);
    for _ in 0..80 {
        let case = generate(&mut rng, true);
        let yaml = export_form_settings(case.settings()).unwrap().yaml;
        let once = case.reexport(&import_form_settings(&yaml, case.context(false)).unwrap());
        let twice = case.reexport(&import_form_settings(&once, case.context(false)).unwrap());
        assert_eq!(once, twice, "{case:?}");
    }
}

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn fixtures(dir: &str, extension: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(fixture_dir(dir))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect();
    paths.sort();
    paths
}

impl From<ExportPipelineYamlRequest> for Case {
    fn from(request: ExportPipelineYamlRequest) -> Self {
        Case {
            execution: request.execution_config,
            triggers: request.trigger_config,
            google_play: request.google_play,
            app_store: request.app_store,
        }
    }
}

#[test]
fn export_goldens_import_and_export_again_unchanged() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let inputs = fixtures("pipeline-export", "json");
    assert_eq!(inputs.len(), 4, "export fixtures changed: {inputs:?}");
    for input in inputs {
        let name = input.file_stem().unwrap().to_string_lossy().to_string();
        let request: ExportPipelineYamlRequest =
            serde_json::from_str(&std::fs::read_to_string(&input).unwrap()).unwrap();
        let case = Case::from(request);
        let export: FormExport =
            export_form_settings(case.settings()).unwrap_or_else(|error| panic!("{name}: {error}"));

        let golden = input.with_extension("yaml");
        if update {
            std::fs::write(&golden, &export.yaml).unwrap();
        } else {
            let expected = std::fs::read_to_string(&golden)
                .unwrap_or_else(|_| panic!("{} is missing", golden.display()));
            assert_eq!(export.yaml, expected, "{} changed", golden.display());
        }

        // The golden file, read back with the editor's settings, is them.
        let import = import_form_settings(&export.yaml, case.context(true))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(
            import.unsupported.is_empty(),
            "{name}: {:?}",
            import.unsupported
        );
        assert_eq!(case.reexport(&import), export.yaml, "{name}");
        assert_eq!(
            json(import.execution.as_ref().unwrap()),
            json(&case.execution),
            "{name}"
        );

        // Without them it still exports to a file that runs the same, and
        // a second round trip changes nothing.
        let import = import_form_settings(&export.yaml, case.context(false)).unwrap();
        let once = case.reexport(&import);
        let twice = case.reexport(&import_form_settings(&once, case.context(false)).unwrap());
        assert_eq!(once, twice, "{name}");
        assert!(
            !once.contains("PLAY_SERVICE_ACCOUNT_JSON") && !once.contains("com.example"),
            "{name}: credentials leaked"
        );
    }
}

#[test]
fn migrate_goldens_import_or_say_why_not() {
    let goldens = fixtures("pipeline-migrate", "yaml");
    assert_eq!(goldens.len(), 4, "migrate goldens changed: {goldens:?}");
    let base = triggers(serde_json::json!({ "events": [], "branches": [] }));
    let case = Case {
        execution: PipelineExecutionConfig::default(),
        triggers: Some(base),
        google_play: None,
        app_store: None,
    };
    for path in goldens {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let raw = std::fs::read_to_string(&path).unwrap();
        let import: FormImport = import_form_settings(&raw, case.context(false))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        if name == "v1-shared-build" {
            // Its Android build runs two commands on two lines, and v1 ran
            // no `flutter pub get` when it had a shared build.
            assert!(
                import
                    .unsupported
                    .iter()
                    .any(|item| item.contains("targets.android.build spans several lines")),
                "{:?}",
                import.unsupported
            );
            continue;
        }
        if name == "v1-platform-commands" {
            // It builds iOS before Android, an order the form cannot keep.
            assert_eq!(
                import.unsupported,
                [
                    "targets: the form builds targets in the order android, ios, macos; list them in that order"
                ]
            );
            continue;
        }
        assert!(
            import.unsupported.is_empty(),
            "{name}: {:?}",
            import.unsupported
        );
        let once = case.reexport(&import);
        let twice = case.reexport(&import_form_settings(&once, case.context(false)).unwrap());
        assert_eq!(once, twice, "{name}");
    }
}
