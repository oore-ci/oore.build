//! Runner settings that a managed runner service cannot take as environment
//! variables, because `oore setup` and `oore update` keep its launchd
//! definition fixed. They live in `runner.env` beside `runner.json`, one
//! `KEY=value` per line, and only the keys below are read. A variable set in
//! the environment still wins.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context;

/// The settings file's name, next to the runner config.
pub const SETTINGS_FILE: &str = "runner.env";

/// Keys the settings file may set, with what each does.
pub const SETTINGS_KEYS: &[(&str, &str)] = &[
    (
        "OORE_RUNNER_CACHE_DIR",
        "Where the runner keeps its build caches",
    ),
    (
        "OORE_RUNNER_CACHE_MAX_GB",
        "Largest size of the build caches, in GB",
    ),
    ("OORE_TART_BIN", "Path to the tart binary"),
    ("OORE_TART_IMAGE", "VM image to clone for each build"),
    (
        "OORE_TART_BASE_IMAGE",
        "Base image the runner builds its VM images from",
    ),
    (
        "OORE_TART_IMAGE_MAX_AGE_DAYS",
        "Days before the runner rebuilds a VM image",
    ),
    (
        "OORE_TART_WARM_IMAGE_MAX_AGE_DAYS",
        "Days before the runner rebuilds a warm per-app VM image",
    ),
];

/// The settings file for a runner config path.
pub fn settings_path(runner_config: &Path) -> PathBuf {
    runner_config
        .parent()
        .map(|dir| dir.join(SETTINGS_FILE))
        .unwrap_or_else(|| PathBuf::from(SETTINGS_FILE))
}

fn known(key: &str) -> bool {
    SETTINGS_KEYS.iter().any(|(name, _)| *name == key)
}

/// Parse the file. Blank lines and `#` comments are skipped; an unknown key
/// or a line without `=` is an error, so a typo is never silently ignored.
pub fn parse_settings(raw: &str) -> anyhow::Result<BTreeMap<String, String>> {
    let mut settings = BTreeMap::new();
    for (index, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .with_context(|| format!("line {} is not KEY=value", index + 1))?;
        let key = key.trim();
        anyhow::ensure!(
            known(key),
            "line {}: {key} is not a runner setting",
            index + 1
        );
        settings.insert(key.to_string(), value.trim().to_string());
    }
    Ok(settings)
}

/// Read the settings next to `runner_config`. A missing file is empty.
pub fn read_settings(runner_config: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    let path = settings_path(runner_config);
    match std::fs::read_to_string(&path) {
        Ok(raw) => parse_settings(&raw).with_context(|| format!("in {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

/// Set (`Some`) or remove (`None`) one setting, keeping the others.
pub fn write_setting(runner_config: &Path, key: &str, value: Option<&str>) -> anyhow::Result<()> {
    anyhow::ensure!(
        known(key),
        "{key} is not a runner setting; see `oore runner config show`"
    );
    if let Some(value) = value {
        anyhow::ensure!(!value.contains('\n'), "a runner setting is one line");
    }
    let mut settings = read_settings(runner_config)?;
    match value {
        Some(value) => settings.insert(key.to_string(), value.trim().to_string()),
        None => settings.remove(key),
    };
    let path = settings_path(runner_config);
    let mut body = String::from("# Runner settings. Edit with `oore runner config`.\n");
    for (key, value) in &settings {
        body.push_str(&format!("{key}={value}\n"));
    }
    std::fs::write(&path, body).with_context(|| format!("failed to write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to restrict {}", path.display()))?;
        // Written with sudo, the file must still belong to the account the
        // runner runs as, which owns runner.json.
        if let Ok(owner) = std::fs::metadata(runner_config) {
            let current = std::fs::metadata(&path)?;
            if (current.uid(), current.gid()) != (owner.uid(), owner.gid()) {
                std::os::unix::fs::chown(&path, Some(owner.uid()), Some(owner.gid()))
                    .with_context(|| {
                        format!("failed to give {} to the runner's account", path.display())
                    })?;
            }
        }
    }
    Ok(())
}

/// The settings the environment does not already set: what applying the
/// file would change.
pub fn settings_to_apply(
    settings: &BTreeMap<String, String>,
    environment: impl Fn(&str) -> Option<String>,
) -> Vec<(String, String)> {
    settings
        .iter()
        .filter(|(key, value)| {
            !value.is_empty() && environment(key).is_none_or(|current| current.is_empty())
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_keys_and_refuses_typos() {
        let settings = parse_settings(
            "# comment\n\nOORE_TART_IMAGE = oore-15 \nOORE_RUNNER_CACHE_MAX_GB=40\n",
        )
        .unwrap();
        assert_eq!(settings["OORE_TART_IMAGE"], "oore-15");
        assert_eq!(settings["OORE_RUNNER_CACHE_MAX_GB"], "40");
        assert!(parse_settings("OORE_TART_IMAG=x").is_err());
        assert!(parse_settings("OORE_TART_IMAGE").is_err());
    }

    #[test]
    fn the_environment_wins() {
        let settings = parse_settings("OORE_TART_IMAGE=file\nOORE_TART_BIN=/opt/tart\n").unwrap();
        let apply = settings_to_apply(&settings, |key| {
            (key == "OORE_TART_IMAGE").then(|| "env".to_string())
        });
        assert_eq!(
            apply,
            [("OORE_TART_BIN".to_string(), "/opt/tart".to_string())]
        );
    }

    #[test]
    fn set_and_unset_keep_the_other_settings() {
        let dir = std::env::temp_dir().join(format!(
            "oore-runner-settings-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("runner.json");
        write_setting(&config, "OORE_TART_IMAGE", Some("oore-15")).unwrap();
        write_setting(&config, "OORE_RUNNER_CACHE_MAX_GB", Some("40")).unwrap();
        write_setting(&config, "OORE_TART_IMAGE", None).unwrap();
        let settings = read_settings(&config).unwrap();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings["OORE_RUNNER_CACHE_MAX_GB"], "40");
        assert!(write_setting(&config, "PATH", Some("/bin")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
