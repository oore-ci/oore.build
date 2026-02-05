use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use anyhow::{Context, bail};
use oore_contract::{SetupState, SetupStateFile};
use uuid::Uuid;

/// File-backed state store for the setup state machine.
///
/// All writes are atomic: data is written to a `.tmp` sibling file first,
/// then renamed into place so readers never see a partial write.
pub struct SetupStore {
    path: PathBuf,
}

impl SetupStore {
    /// Create a new store pointing at the given path.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Resolve the state file path from (in priority order):
    /// 1. Explicit `override_path` argument
    /// 2. `OORE_SETUP_STATE_FILE` env var
    /// 3. Default: `~/Library/Application Support/oore/setup-state.json` (via `dirs::data_dir()`)
    pub fn resolve_path(override_path: Option<&str>) -> anyhow::Result<PathBuf> {
        if let Some(p) = override_path {
            return Ok(PathBuf::from(p));
        }

        if let Ok(p) = env::var("OORE_SETUP_STATE_FILE") {
            return Ok(PathBuf::from(p));
        }

        let data_dir = dirs::data_dir()
            .context("could not determine platform data directory (dirs::data_dir)")?;
        Ok(data_dir.join("oore").join("setup-state.json"))
    }

    /// Load and deserialize the state file from disk.
    pub fn load(&self) -> anyhow::Result<SetupStateFile> {
        let data = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read state file: {}", self.path.display()))?;
        let state: SetupStateFile = serde_json::from_str(&data)
            .with_context(|| format!("failed to parse state file: {}", self.path.display()))?;
        Ok(state)
    }

    /// Atomically write the state file: write to `<path>.tmp`, then rename.
    pub fn save(&self, state: &SetupStateFile) -> anyhow::Result<()> {
        let parent = self.path.parent().context("state file path has no parent")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;

        let tmp_path = self.path.with_extension("json.tmp");
        let data = serde_json::to_string_pretty(state)
            .context("failed to serialize state file")?;

        fs::write(&tmp_path, data.as_bytes())
            .with_context(|| format!("failed to write tmp state file: {}", tmp_path.display()))?;

        fs::rename(&tmp_path, &self.path).with_context(|| {
            format!(
                "failed to rename {} -> {}",
                tmp_path.display(),
                self.path.display()
            )
        })?;

        Ok(())
    }

    /// Check whether the state file exists on disk.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// If the state file does not exist, create it with initial
    /// `BootstrapPending` state, a fresh UUID instance id, and the current
    /// timestamp. If it already exists, just load and return it.
    pub fn init_if_missing(&self) -> anyhow::Result<SetupStateFile> {
        if self.exists() {
            return self.load();
        }

        let now = now_unix();
        let state = SetupStateFile {
            schema_version: SetupStateFile::CURRENT_SCHEMA_VERSION,
            instance_id: Uuid::new_v4().to_string(),
            setup_state: SetupState::BootstrapPending,
            bootstrap_token: None,
            setup_session: None,
            oidc_config: None,
            owner: None,
            created_at: now,
            updated_at: now,
        };

        self.save(&state)?;

        // Verify the round-trip so we fail loudly on serialization bugs.
        let loaded = self.load().context("failed to reload newly created state file")?;
        if loaded.instance_id != state.instance_id {
            bail!("state file round-trip verification failed");
        }

        Ok(state)
    }
}

/// Current UNIX timestamp in seconds.
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
