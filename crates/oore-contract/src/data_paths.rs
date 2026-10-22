//! Where `oore` and `oored` keep their data.
//!
//! Both binaries resolve paths through these functions so the CLI never
//! encrypts with a key the daemon does not load.

use std::path::{Path, PathBuf};

/// Environment variables that override the data root, highest priority first.
pub const DATA_DIR_ENV_VARS: [&str; 2] = ["OORED_DATA_DIR", "OORE_DATA_DIR"];

/// File name of the instance encryption key inside the data root.
pub const ENCRYPTION_KEY_FILE: &str = "encryption.key";

/// The plaintext `oored` seals in its `encryption_key_check` table, so either
/// binary can tell whether a key file belongs to a database.
pub const ENCRYPTION_KEY_CHECK_PLAINTEXT: &str = "oore-encryption-key-check-v1";

/// Resolve the data root from `env` and the platform data directory.
///
/// Resolution order:
/// 1. `OORED_DATA_DIR`
/// 2. `OORE_DATA_DIR`
/// 3. `{platform_data_dir}/oore`
///
/// Blank values are ignored. Returns `None` only when no variable is set and
/// the platform has no data directory.
pub fn resolve_data_dir(
    env: impl Fn(&str) -> Option<String>,
    platform_data_dir: Option<PathBuf>,
) -> Option<PathBuf> {
    DATA_DIR_ENV_VARS
        .iter()
        .find_map(|key| {
            env(key)
                .map(|raw| raw.trim().to_string())
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| platform_data_dir.map(|dir| dir.join("oore")))
}

/// The encryption key path for a data root.
pub fn encryption_key_path(data_dir: &Path) -> PathBuf {
    data_dir.join(ENCRYPTION_KEY_FILE)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn resolve(vars: &[(&str, &str)]) -> Option<PathBuf> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        resolve_data_dir(
            |key| vars.get(key).cloned(),
            Some(PathBuf::from("/platform")),
        )
    }

    #[test]
    fn data_dir_honours_overrides_in_order() {
        assert_eq!(resolve(&[]), Some(PathBuf::from("/platform/oore")));
        assert_eq!(
            resolve(&[("OORE_DATA_DIR", "/b")]),
            Some(PathBuf::from("/b"))
        );
        assert_eq!(
            resolve(&[("OORED_DATA_DIR", " /a "), ("OORE_DATA_DIR", "/b")]),
            Some(PathBuf::from("/a"))
        );
        assert_eq!(
            resolve(&[("OORED_DATA_DIR", "  "), ("OORE_DATA_DIR", "/b")]),
            Some(PathBuf::from("/b"))
        );
    }

    #[test]
    fn platform_without_data_dir_resolves_only_from_env() {
        assert_eq!(resolve_data_dir(|_| None, None), None);
        assert_eq!(
            resolve_data_dir(|_| Some("/a".to_string()), None),
            Some(PathBuf::from("/a"))
        );
    }
}
