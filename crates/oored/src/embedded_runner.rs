use std::env::VarError;

use sqlx::SqlitePool;
use tracing::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerMode {
    External,
}

impl RunnerMode {
    fn from_value(raw: Option<&str>) -> anyhow::Result<Self> {
        match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            None | Some("external") => Ok(Self::External),
            Some("embedded" | "hybrid") => anyhow::bail!(
                "embedded runner execution is disabled; use a dedicated external runner account"
            ),
            Some(raw) => anyhow::bail!("invalid OORED_RUNNER_MODE: {raw:?}"),
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        match std::env::var("OORED_RUNNER_MODE") {
            Ok(raw) => Self::from_value(Some(&raw)),
            Err(VarError::NotPresent) => Self::from_value(None),
            Err(error) => Err(error.into()),
        }
    }
}

pub async fn start_if_enabled(
    _pool: SqlitePool,
    _daemon_url: String,
) -> anyhow::Result<Option<tokio::task::JoinHandle<()>>> {
    RunnerMode::from_env()?;
    info!("embedded runner disabled; use a dedicated external runner account");
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runner_modes_fail_closed_without_os_isolation() {
        assert_eq!(RunnerMode::from_value(None).unwrap(), RunnerMode::External);
        assert_eq!(
            RunnerMode::from_value(Some(" external ")).unwrap(),
            RunnerMode::External
        );
        assert!(RunnerMode::from_value(Some("embedded")).is_err());
        assert!(RunnerMode::from_value(Some("hybrid")).is_err());
        assert!(RunnerMode::from_value(Some("invalid")).is_err());
    }
}
