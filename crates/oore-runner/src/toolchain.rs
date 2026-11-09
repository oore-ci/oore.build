//! The build tools a runner has, checked the way `oore doctor` checks them,
//! so the web UI can say what a runner can build and how to fix the rest.
//!
//! The runner reports the latest checks under `toolchain` in its
//! capabilities. They run at start and again every `REFRESH_INTERVAL`, so a
//! tool installed while the runner runs shows up without a restart.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use oore_contract::{BuildPlatform, RunnerToolchainCheck, RunnerToolchainStatus};

/// How often the runner checks its tools again.
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// A tool that does not answer in this time counts as missing.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

static LATEST: Mutex<Option<Vec<RunnerToolchainCheck>>> = Mutex::new(None);

/// The checks from the last refresh, as capabilities JSON.
pub fn snapshot() -> Option<serde_json::Value> {
    let checks = LATEST.lock().ok()?.clone()?;
    serde_json::to_value(checks).ok()
}

/// Runs every check and keeps the result for `snapshot`.
pub async fn refresh() -> Vec<RunnerToolchainCheck> {
    let checks = check_all().await;
    if let Ok(mut latest) = LATEST.lock() {
        *latest = Some(checks.clone());
    }
    checks
}

/// First line of a command's output, stdout before stderr, when it exits 0.
async fn command_version(program: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Option<String> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    String::from_utf8(text)
        .ok()?
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// Whether a command exits 0.
async fn command_succeeds(program: &str, args: &[&str]) -> bool {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    matches!(
        tokio::time::timeout(COMMAND_TIMEOUT, command.status()).await,
        Ok(Ok(status)) if status.success()
    )
}

fn check(
    id: &str,
    label: &str,
    platforms: &[BuildPlatform],
    status: RunnerToolchainStatus,
    detail: Option<String>,
    fix: Option<&str>,
) -> RunnerToolchainCheck {
    RunnerToolchainCheck {
        id: id.to_string(),
        label: label.to_string(),
        platforms: platforms.to_vec(),
        status,
        detail,
        fix: fix.map(str::to_string),
    }
}

/// The Android SDK the build would find, as `oore doctor` looks for it.
pub fn android_sdk_root() -> Option<PathBuf> {
    let mut roots = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(PathBuf::from))
        .collect::<Vec<_>>();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Library/Android/sdk"));
    }
    roots
        .into_iter()
        .find(|path| path.join("platform-tools/adb").is_file())
}

fn java_home_has_java(path: &Path) -> bool {
    path.join("bin/java").is_file()
}

/// The JDK Gradle would use: `JAVA_HOME`, Android Studio's, or the system's.
async fn java_home() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("JAVA_HOME").map(PathBuf::from)
        && java_home_has_java(&path)
    {
        return Some(path);
    }
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    roots.push(PathBuf::from("/Applications"));
    for root in roots {
        for app in ["Android Studio.app", "Android Studio Preview.app"] {
            let path = root.join(app).join("Contents/jbr/Contents/Home");
            if java_home_has_java(&path) {
                return Some(path);
            }
        }
    }
    let path = command_version("/usr/libexec/java_home", &[])
        .await
        .map(PathBuf::from)?;
    java_home_has_java(&path).then_some(path)
}

async fn check_all() -> Vec<RunnerToolchainCheck> {
    use RunnerToolchainStatus::{Missing, Ok, Warning};
    const ANDROID: &[BuildPlatform] = &[BuildPlatform::Android];
    const APPLE: &[BuildPlatform] = &[BuildPlatform::Ios, BuildPlatform::Macos];
    let mut checks = Vec::new();

    checks.push(match command_version("git", &["--version"]).await {
        Some(version) => check("git", "Git", &[], Ok, Some(version), None),
        None => check(
            "git",
            "Git",
            &[],
            Missing,
            None,
            Some("xcode-select --install"),
        ),
    });

    checks.push(match command_version("flutter", &["--version"]).await {
        Some(version) => check("flutter", "Flutter", &[], Ok, Some(version), None),
        None => check(
            "flutter",
            "Flutter",
            &[],
            Warning,
            Some("Not on the runner's PATH. Builds on this Mac need it; VM builds use the image's.".to_string()),
            Some("Install Flutter (https://docs.flutter.dev/get-started/install/macos) and add it to the PATH of the account the runner runs as."),
        ),
    });

    match java_home().await {
        Some(home) => {
            let version = command_version(home.join("bin/java"), &["-version"]).await;
            checks.push(check(
                "java",
                "Java (JDK)",
                ANDROID,
                Ok,
                Some(match version {
                    Some(version) => format!("{version} ({})", home.display()),
                    None => home.display().to_string(),
                }),
                None,
            ));
        }
        None => checks.push(check(
            "java",
            "Java (JDK)",
            ANDROID,
            Missing,
            None,
            Some("Install Android Studio, or a JDK 17 with: brew install openjdk@17, then set JAVA_HOME."),
        )),
    }

    checks.push(match android_sdk_root() {
        Some(root) => check(
            "android_sdk",
            "Android SDK",
            ANDROID,
            Ok,
            Some(root.display().to_string()),
            None,
        ),
        None => check(
            "android_sdk",
            "Android SDK",
            ANDROID,
            Missing,
            None,
            Some("Install Android Studio and its SDK Platform-Tools, or set ANDROID_HOME to an SDK that has them."),
        ),
    });

    if std::env::consts::OS != "macos" {
        checks.push(check(
            "xcode",
            "Xcode",
            APPLE,
            Missing,
            Some("iOS and macOS apps build on a Mac.".to_string()),
            Some("Add a macOS runner for iOS and macOS builds."),
        ));
        return checks;
    }

    let developer_dir = command_version("xcode-select", &["-p"])
        .await
        .filter(|path| Path::new(path).is_dir());
    let xcode = command_version("xcodebuild", &["-version"]).await;
    match (developer_dir, xcode) {
        (Some(developer_dir), Some(xcode)) => {
            checks.push(check(
                "xcode",
                "Xcode",
                APPLE,
                Ok,
                Some(format!("{xcode} ({developer_dir})")),
                None,
            ));
            if !command_succeeds("xcodebuild", &["-checkFirstLaunchStatus"]).await {
                checks.push(check(
                    "xcode_first_launch",
                    "Xcode first launch",
                    APPLE,
                    Missing,
                    Some("Xcode needs its license accepted and its components installed.".to_string()),
                    Some("sudo xcodebuild -runFirstLaunch"),
                ));
            }
        }
        _ => checks.push(check(
            "xcode",
            "Xcode",
            APPLE,
            Missing,
            None,
            Some("Install Xcode from the App Store, then run: sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer"),
        )),
    }

    checks.push(match command_version("pod", &["--version"]).await {
        Some(version) => check("cocoapods", "CocoaPods", APPLE, Ok, Some(version), None),
        None => check(
            "cocoapods",
            "CocoaPods",
            APPLE,
            Warning,
            Some("Apps with plugins that use CocoaPods need it.".to_string()),
            Some("brew install cocoapods"),
        ),
    });

    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_reach_the_backend_as_capabilities() {
        let missing = check(
            "xcode",
            "Xcode",
            &[BuildPlatform::Ios],
            RunnerToolchainStatus::Missing,
            None,
            Some("sudo xcodebuild -runFirstLaunch"),
        );
        *LATEST.lock().unwrap() = Some(vec![missing.clone()]);
        let value = snapshot().unwrap();
        assert_eq!(value[0]["status"], "missing");
        assert_eq!(value[0]["platforms"][0], "ios");
        let back: Vec<RunnerToolchainCheck> = serde_json::from_value(value).unwrap();
        assert_eq!(back, vec![missing]);
    }
}
