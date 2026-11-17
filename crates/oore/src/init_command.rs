//! `oore init`: write a starter `.oore.yaml` for the Flutter app in a folder.
//!
//! It reads `pubspec.yaml` and the platform folders the way the web UI's
//! "Add your app" flow does, writes a version 2 file with one target per
//! platform, and validates it with the runner's schema before saying so.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::Args;
use oore_contract::{BuildPlatform, parse_repository_config};

#[derive(Debug, Args)]
pub struct InitArgs {
    /// The app's repository folder.
    #[arg(default_value = ".")]
    pub dir: PathBuf,
    /// Replace an existing `.oore.yaml`.
    #[arg(long)]
    pub force: bool,
    /// Print the file instead of writing it.
    #[arg(long)]
    pub print: bool,
    /// Write instructions for coding agents instead: a marked section in
    /// AGENTS.md and a Claude Code skill in .claude/skills/oore/SKILL.md.
    /// Running it again updates them in place.
    #[arg(long)]
    pub agents: bool,
}

/// The schema line editors read to check and complete the file.
pub const SCHEMA_LINE: &str =
    "# yaml-language-server: $schema=https://docs.oore.build/schema/oore-yaml-v2.json\n";

const AGENTS_BEGIN: &str =
    "<!-- oore:begin (written by oore init --agents; edits here are replaced) -->";
const AGENTS_END: &str = "<!-- oore:end -->";

/// What coding agents need to build this app on Oore. The same text is
/// published at https://docs.oore.build/build/automate/agent-instructions.
pub const AGENT_INSTRUCTIONS: &str = include_str!("agent_instructions.md");

const SKILL_HEADER: &str = "---\nname: oore\ndescription: Build, test and ship this Flutter app on Oore CI with the oore CLI. Use when asked to build the app, check a failed build, change .oore.yaml, or share a build with testers.\n---\n\n";

/// `existing` with the Oore section replaced, or added at the end.
pub fn merge_agents_section(existing: &str) -> String {
    let section = format!("{AGENTS_BEGIN}\n{AGENT_INSTRUCTIONS}{AGENTS_END}\n");
    if let (Some(start), Some(end)) = (existing.find(AGENTS_BEGIN), existing.find(AGENTS_END))
        && start < end
    {
        let after = &existing[end + AGENTS_END.len()..];
        let after = after.strip_prefix('\n').unwrap_or(after);
        return format!("{}{section}{after}", &existing[..start]);
    }
    if existing.trim().is_empty() {
        return section;
    }
    let separator = if existing.ends_with("\n\n") {
        ""
    } else if existing.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!("{existing}{separator}{section}")
}

fn handle_agents(args: &InitArgs) -> anyhow::Result<()> {
    let skill = format!("{SKILL_HEADER}{AGENT_INSTRUCTIONS}");
    if args.print {
        print!("{AGENT_INSTRUCTIONS}");
        return Ok(());
    }
    let agents_path = args.dir.join("AGENTS.md");
    let existing = match fs::read_to_string(&agents_path) {
        Ok(existing) => existing,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", agents_path.display()));
        }
    };
    let merged = merge_agents_section(&existing);
    fs::write(&agents_path, &merged)
        .with_context(|| format!("failed to write {}", agents_path.display()))?;
    let skill_path = args.dir.join(".claude/skills/oore/SKILL.md");
    if let Some(parent) = skill_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&skill_path, skill)
        .with_context(|| format!("failed to write {}", skill_path.display()))?;
    println!(
        "{} the Oore section in {} and wrote {}.",
        if existing.contains(AGENTS_BEGIN) {
            "Updated"
        } else {
            "Added"
        },
        agents_path.display(),
        skill_path.display()
    );
    println!("Commit both so every agent working in this repository finds them.");
    Ok(())
}

/// What `oore init` found in the folder.
#[derive(Debug, PartialEq, Eq)]
pub struct DetectedApp {
    pub name: Option<String>,
    pub platforms: Vec<BuildPlatform>,
    /// A `test/` folder exists, so the file runs the tests.
    pub has_tests: bool,
}

/// Files whose presence says a platform folder holds a buildable app; the
/// same markers the backend reads for app detection.
const PLATFORM_MARKERS: [(BuildPlatform, &[&str]); 3] = [
    (
        BuildPlatform::Android,
        &["android/app/build.gradle", "android/app/build.gradle.kts"],
    ),
    (
        BuildPlatform::Ios,
        &["ios/Runner/Info.plist", "ios/Podfile"],
    ),
    (
        BuildPlatform::Macos,
        &["macos/Runner/Info.plist", "macos/Podfile"],
    ),
];

fn read_pubspec(raw: &str) -> (Option<String>, bool) {
    let name = raw.lines().find_map(|line| {
        let value = line.strip_prefix("name:")?.trim();
        let value = value.trim_matches(|c| c == '"' || c == '\'');
        (!value.is_empty()).then(|| value.to_string())
    });
    let flutter = raw.lines().any(|line| {
        let line = line.trim_end();
        line == "flutter:" || line.trim_start().starts_with("sdk: flutter")
    });
    (name, flutter)
}

pub fn detect(dir: &Path) -> anyhow::Result<DetectedApp> {
    let pubspec_path = dir.join("pubspec.yaml");
    let raw = fs::read_to_string(&pubspec_path).with_context(|| {
        format!(
            "{} has no pubspec.yaml; run oore init in a Flutter app's folder",
            dir.display()
        )
    })?;
    let (name, flutter) = read_pubspec(&raw);
    if !flutter {
        bail!(
            "{} does not depend on Flutter; oore init writes Flutter pipelines",
            pubspec_path.display()
        );
    }
    let platforms = PLATFORM_MARKERS
        .into_iter()
        .filter(|(_, markers)| markers.iter().any(|marker| dir.join(marker).is_file()))
        .map(|(platform, _)| platform)
        .collect::<Vec<_>>();
    if platforms.is_empty() {
        bail!(
            "{} has no android/, ios/ or macos/ app folder; run flutter create --platforms=android,ios . first",
            dir.display()
        );
    }
    Ok(DetectedApp {
        name,
        platforms,
        has_tests: dir.join("test").is_dir(),
    })
}

/// The starter file for what `detect` found.
pub fn starter_file(app: &DetectedApp) -> String {
    let mut out = String::from(SCHEMA_LINE);
    out.push_str(&format!(
        "# Oore pipeline for {}, written by oore init.\n",
        app.name.as_deref().unwrap_or("this app")
    ));
    out.push_str(
        "# Oore reads this file from the commit it builds, so changes ride with the code.\n\
         # Reference: https://docs.oore.build/reference/config/oore-yaml-v2\n\
         version: 2\n\ntargets:\n",
    );
    for platform in &app.platforms {
        out.push_str(match platform {
            BuildPlatform::Android => {
                "  android:\n\
                 \x20   # A debug APK anyone on the team can install. No signing needed.\n\
                 \x20   build: flutter build apk --debug\n\
                 \x20   artifacts: [build/app/outputs/flutter-apk/*.apk]\n"
            }
            BuildPlatform::Ios => {
                "  ios:\n\
                 \x20   # An ad hoc IPA for registered iPhones. Turn on iOS signing for the\n\
                 \x20   # pipeline in Oore before the first iOS build.\n\
                 \x20   build: flutter build ipa --release --export-method ad-hoc\n\
                 \x20   artifacts: [build/ios/ipa/*.ipa]\n"
            }
            BuildPlatform::Macos => {
                "  macos:\n\
                 \x20   build: flutter build macos --release\n\
                 \x20   artifacts: [build/macos/Build/Products/Release/*.app]\n"
            }
        });
    }
    out.push_str("\nsteps:\n  - name: Get packages\n    run: flutter pub get\n");
    if app.has_tests {
        out.push_str("  - name: Tests\n    run: flutter test\n");
    }
    out.push_str("  - build\n");
    out
}

fn platform_names(platforms: &[BuildPlatform]) -> String {
    let names = platforms
        .iter()
        .map(|platform| match platform {
            BuildPlatform::Android => "Android",
            BuildPlatform::Ios => "iOS",
            BuildPlatform::Macos => "macOS",
        })
        .collect::<Vec<_>>();
    match names.as_slice() {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

pub fn handle_init(args: InitArgs) -> anyhow::Result<()> {
    if args.agents {
        return handle_agents(&args);
    }
    let app = detect(&args.dir)?;
    let file = starter_file(&app);
    // The runner's schema has the last word, so a bad template never lands.
    parse_repository_config(&file)
        .map_err(|error| anyhow::anyhow!("the starter file is invalid: {error}"))?;
    if args.print {
        print!("{file}");
        return Ok(());
    }
    let path = args.dir.join(".oore.yaml");
    for existing in [path.clone(), args.dir.join(".oore.yml")] {
        if existing.exists() && !args.force {
            bail!(
                "{} already exists; pass --force to replace it, or --print to compare",
                existing.display()
            );
        }
    }
    fs::write(&path, &file).with_context(|| format!("failed to write {}", path.display()))?;
    println!(
        "Wrote {} for {} ({}). It is valid.",
        path.display(),
        app.name.as_deref().unwrap_or("the app"),
        platform_names(&app.platforms)
    );
    println!(
        "Next: commit and push it, then add the app in Oore. Oore finds the file and builds from it."
    );
    if app.platforms.contains(&BuildPlatform::Ios) {
        println!("iOS builds need signing: add a certificate and profiles to the pipeline first.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_dir(files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "oore-init-{}-{}",
            std::process::id(),
            rand::random::<u32>()
        ));
        fs::create_dir_all(&dir).unwrap();
        for file in files {
            let path = dir.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }
        fs::write(
            dir.join("pubspec.yaml"),
            "name: coin\ndependencies:\n  flutter:\n    sdk: flutter\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn a_flutter_app_gets_a_valid_file_per_platform() {
        let dir = app_dir(&["android/app/build.gradle.kts", "ios/Podfile", "test/a.dart"]);
        let app = detect(&dir).unwrap();
        assert_eq!(app.name.as_deref(), Some("coin"));
        assert_eq!(
            app.platforms,
            vec![BuildPlatform::Android, BuildPlatform::Ios]
        );
        let file = starter_file(&app);
        let config = parse_repository_config(&file).unwrap();
        assert_eq!(config.version(), 2);
        assert!(file.contains("flutter build apk --debug"));
        assert!(file.contains("run: flutter test"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn init_does_not_replace_a_file_without_force() {
        let dir = app_dir(&["android/app/build.gradle"]);
        handle_init(InitArgs {
            dir: dir.clone(),
            force: false,
            print: false,
            agents: false,
        })
        .unwrap();
        assert!(
            !fs::read_to_string(dir.join(".oore.yaml"))
                .unwrap()
                .contains("Tests")
        );
        assert!(
            handle_init(InitArgs {
                dir: dir.clone(),
                force: false,
                print: false,
                agents: false,
            })
            .is_err()
        );
        handle_init(InitArgs {
            dir: dir.clone(),
            force: true,
            print: false,
            agents: false,
        })
        .unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_starter_file_names_its_schema_first() {
        let dir = app_dir(&["android/app/build.gradle"]);
        let file = starter_file(&detect(&dir).unwrap());
        assert!(file.starts_with(SCHEMA_LINE));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_agents_section_is_added_once_and_updated_in_place() {
        let added = merge_agents_section("# My app\n\nRun tests first.\n");
        assert!(added.starts_with("# My app\n\nRun tests first.\n\n<!-- oore:begin"));
        assert_eq!(merge_agents_section(&added), added);
        let stale = added.replace("oore build why", "old text");
        let edited = format!("{stale}\n## Mine\n");
        let updated = merge_agents_section(&edited);
        assert_eq!(updated.matches(AGENTS_BEGIN).count(), 1);
        assert!(updated.contains("oore build why"));
        assert!(updated.ends_with("\n## Mine\n"));
        assert!(merge_agents_section("").starts_with(AGENTS_BEGIN));
    }

    #[test]
    fn init_agents_writes_agents_md_and_the_skill() {
        let dir = app_dir(&[]);
        fs::write(dir.join("AGENTS.md"), "# Rules\n").unwrap();
        let args = || InitArgs {
            dir: dir.clone(),
            force: false,
            print: false,
            agents: true,
        };
        handle_init(args()).unwrap();
        handle_init(args()).unwrap();
        let agents = fs::read_to_string(dir.join("AGENTS.md")).unwrap();
        assert!(agents.starts_with("# Rules\n"));
        assert_eq!(agents.matches(AGENTS_BEGIN).count(), 1);
        let skill = fs::read_to_string(dir.join(".claude/skills/oore/SKILL.md")).unwrap();
        assert!(skill.starts_with("---\nname: oore\n"));
        assert!(!dir.join(".oore.yaml").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_folder_without_flutter_is_refused() {
        let dir = app_dir(&[]);
        fs::write(dir.join("pubspec.yaml"), "name: tool\n").unwrap();
        assert!(detect(&dir).unwrap_err().to_string().contains("Flutter"));
        let _ = fs::remove_dir_all(dir);
    }
}
