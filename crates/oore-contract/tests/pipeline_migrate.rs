//! `oore pipeline migrate` on the v1 golden fixtures.
//!
//! Every `tests/fixtures/pipeline/v1-*.yaml` is migrated, and the original
//! and the generated file are planned and compared: the same commands in the
//! same order, phase and platform, with the same env, artifacts, runner
//! labels, isolation, Flutter version and build number offset. The generated
//! text is kept in `tests/fixtures/pipeline-migrate/<name>.yaml` so reviews
//! show what users get; run with `UPDATE_GOLDEN=1` to rewrite it.

use std::path::{Path, PathBuf};

use oore_contract::pipeline_migrate::{MigrateOutcome, Migration, migrate_pipeline_config};
use oore_contract::{
    BuildPlatform, PipelinePlan, PlanRequest, RepositoryConfig, StepPhase, parse_repository_config,
};

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn migrate(raw: &str) -> Migration {
    match migrate_pipeline_config(raw).unwrap_or_else(|error| panic!("{error}\n{raw}")) {
        MigrateOutcome::Migrated(migration) => migration,
        MigrateOutcome::AlreadyV2 => panic!("expected a version 1 file:\n{raw}"),
    }
}

fn plan(raw: &str, version: u32) -> PipelinePlan {
    let config = parse_repository_config(raw).unwrap_or_else(|error| panic!("{error}\n{raw}"));
    assert_eq!(config.version(), version, "{raw}");
    config.plan(PlanRequest::default()).unwrap()
}

type Steps = Vec<(StepPhase, String, Option<BuildPlatform>)>;

fn steps(plan: &PipelinePlan) -> Steps {
    plan.steps
        .iter()
        .map(|step| (step.phase, step.run.clone(), step.platform.clone()))
        .collect()
}

/// Everything but the steps must match exactly.
fn assert_same_inputs(v1: &PipelinePlan, v2: &PipelinePlan, name: &str) {
    assert_eq!(v1.env, v2.env, "{name}: env");
    assert_eq!(
        v1.artifact_patterns, v2.artifact_patterns,
        "{name}: artifacts"
    );
    assert_eq!(v1.runs_on, v2.runs_on, "{name}: runs_on");
    assert_eq!(v1.isolation, v2.isolation, "{name}: isolation");
    assert_eq!(
        v1.flutter_version, v2.flutter_version,
        "{name}: flutter version"
    );
    assert_eq!(
        v1.build_number_offset, v2.build_number_offset,
        "{name}: build number offset"
    );
    assert!(v2.publish.is_empty(), "{name}: publish");
}

#[test]
fn v1_fixtures_migrate_to_files_that_plan_the_same() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(fixture_dir("pipeline"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "yaml")
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("v1-"))
        })
        .collect();
    fixtures.sort();
    assert_eq!(fixtures.len(), 4, "v1 fixtures changed: {fixtures:?}");

    for path in fixtures {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let raw = std::fs::read_to_string(&path).unwrap();
        let migration = migrate(&raw);

        let golden = fixture_dir("pipeline-migrate").join(format!("{name}.yaml"));
        if update {
            std::fs::create_dir_all(golden.parent().unwrap()).unwrap();
            std::fs::write(&golden, &migration.yaml).unwrap();
        } else {
            let expected = std::fs::read_to_string(&golden)
                .unwrap_or_else(|_| panic!("{} is missing", golden.display()));
            assert_eq!(migration.yaml, expected, "{} changed", golden.display());
        }

        let v1 = plan(&raw, 1);
        let v2 = plan(&migration.yaml, 2);
        assert_same_inputs(&v1, &v2, &name);

        if name == "v1-shared-build" {
            // The documented exception: a shared commands.build becomes the
            // first platform's target build, each command in its own subshell.
            assert_eq!(
                steps(&v2),
                vec![
                    (
                        StepPhase::PreBuild,
                        "dart run build_runner build".to_string(),
                        None
                    ),
                    (
                        StepPhase::Build,
                        "( flutter build appbundle --release ) &&\n( flutter build apk --release )"
                            .to_string(),
                        Some(BuildPlatform::Android)
                    ),
                    (StepPhase::PostBuild, "echo done".to_string(), None),
                ],
            );
            assert_eq!(v1.platforms, v2.platforms);
            assert_eq!(migration.warnings.len(), 1, "{:?}", migration.warnings);
            assert!(migration.warnings[0].contains("commands.build has 2 commands"));
            assert!(
                migration
                    .notes
                    .iter()
                    .any(|note| note.starts_with("platform_commands.android is dropped")),
                "{:?}",
                migration.notes
            );
            assert!(migration.yaml.contains("# WARNING: commands.build has 2"));
        } else {
            assert_eq!(steps(&v1), steps(&v2), "{name}: steps");
            assert_eq!(v1.platforms, v2.platforms, "{name}: platforms");
            assert!(
                migration.warnings.is_empty(),
                "{name}: {:?}",
                migration.warnings
            );
        }
    }
}

#[test]
fn version_2_files_are_left_alone() {
    let raw = std::fs::read_to_string(fixture_dir("pipeline").join("v2-minimal.yaml")).unwrap();
    assert_eq!(
        migrate_pipeline_config(&raw).unwrap(),
        MigrateOutcome::AlreadyV2
    );
}

#[test]
fn invalid_files_report_their_errors() {
    let error = migrate_pipeline_config("version: 1\nplatforms: []\n").unwrap_err();
    assert!(
        error.contains("platforms must include at least one target"),
        "{error}"
    );
}

#[test]
fn the_play_bundle_step_is_explained_not_guessed() {
    let raw = "version: 1\nplatforms: [android]\nplatform_build_args:\n  android: ['--split-per-abi', '--obfuscate']\n";
    let migration = migrate(raw);
    let v2 = plan(&migration.yaml, 2);
    assert_eq!(
        steps(&v2)[1].1,
        "flutter build apk --release --split-per-abi --obfuscate"
    );
    // The bundle command version 1 would have added, without the APK-only
    // flag, is named in a comment and a note.
    assert!(
        migration
            .yaml
            .contains("#    build: flutter build apk --release --split-per-abi --obfuscate && flutter build appbundle --release --obfuscate\n"),
        "{}",
        migration.yaml
    );
    assert!(
        migration
            .notes
            .iter()
            .any(|note| note.contains("Google Play")),
        "{:?}",
        migration.notes
    );
    // An overridden Android command never got the bundle, so no comment.
    let migration = migrate(
        "version: 1\nplatforms: [android]\nplatform_commands:\n  android: flutter build appbundle\n",
    );
    assert!(
        !migration.yaml.contains("Google Play"),
        "{}",
        migration.yaml
    );
}

#[test]
fn shared_build_for_several_platforms_warns_about_the_dropped_ones() {
    let raw = "version: 1\nplatforms: [ios, android]\ncommands:\n  build: ['make all']\n";
    let migration = migrate(raw);
    let v1 = plan(raw, 1);
    let v2 = plan(&migration.yaml, 2);
    assert_same_inputs(&v1, &v2, "shared");
    assert_eq!(v2.platforms, vec![BuildPlatform::Ios]);
    assert_eq!(
        steps(&v2),
        vec![(
            StepPhase::Build,
            "make all".to_string(),
            Some(BuildPlatform::Ios)
        )]
    );
    assert_eq!(migration.warnings.len(), 1, "{:?}", migration.warnings);
    assert!(migration.warnings[0].contains("platforms also lists android"));
}

#[test]
fn awkward_values_survive_the_round_trip() {
    let raw = r#"version: 1
platforms: [macos, android]
flutter_version: "3.27"
commands:
  pre_build:
    - flutter pub get
    - flutter pub get
    - |
      set -e
      echo "a: b" # trailing
    - "echo 'quoted'"
  post_build:
    - flutter test
env:
  - key: RETRIES
    value: "3"
  - key: FLAG
    value: "true"
  - key: EMPTY
    value: ""
  - key: TOKEN
    value: ${{ secrets.TOKEN }}
artifacts:
  patterns: ['*.app', '**/*.aab', '*.app']
runs_on: Linux
"#;
    let migration = migrate(raw);
    let v1 = plan(raw, 1);
    let v2 = plan(&migration.yaml, 2);
    let trimmed = |steps: Steps| -> Steps {
        steps
            .into_iter()
            .map(|(phase, run, platform)| (phase, run.trim().to_string(), platform))
            .collect()
    };
    assert_eq!(trimmed(steps(&v1)), trimmed(steps(&v2)));
    assert_eq!(v1.env, v2.env);
    assert_eq!(v2.artifact_patterns, vec!["*.app", "**/*.aab"]);
    assert_eq!(v1.runs_on, v2.runs_on);
    assert_eq!(v1.flutter_version, v2.flutter_version);
    let names: Vec<&str> = v2.steps.iter().map(|step| step.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Get packages",
            "Get packages (2)",
            "Get packages (3)",
            "set -e...",
            "echo 'quoted'",
            "Build macOS",
            "Build Android",
            "Run tests"
        ]
    );
    assert!(
        migration
            .notes
            .iter()
            .any(|note| note.contains("repeats '*.app'")),
        "{:?}",
        migration.notes
    );
}

#[test]
fn artifact_patterns_with_expressions_cannot_be_migrated() {
    let raw = "version: 1\nplatforms: [android]\nartifacts:\n  patterns: ['build/${{ secrets.X }}/*.apk']\n";
    let error = migrate_pipeline_config(raw).unwrap_err();
    assert!(error.contains("artifacts.patterns[0]"), "{error}");
}

#[test]
fn duplicate_platforms_cannot_be_migrated() {
    let raw = "version: 1\nplatforms: [android, android]\n";
    if parse_repository_config(raw).is_ok() {
        let error = migrate_pipeline_config(raw).unwrap_err();
        assert!(error.contains("android more than once"), "{error}");
    }
}

#[test]
fn generated_files_say_they_were_generated() {
    let migration = migrate("version: 1\nplatforms: [ios]\n");
    assert!(
        migration
            .yaml
            .starts_with("# Generated by `oore pipeline migrate`"),
        "{}",
        migration.yaml
    );
    assert!(matches!(
        parse_repository_config(&migration.yaml),
        Ok(RepositoryConfig::V2(_))
    ));
}
