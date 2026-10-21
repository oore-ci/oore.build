use std::ffi::OsStr;
use std::fs;
use std::io::Write;
#[cfg(target_os = "macos")]
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Context;
use oore_contract::{RuntimeUpdatePhase, RuntimeUpdateStatus};

const DAEMON_SERVICE: &str = "system/build.oore.oored";

pub(crate) fn write_status(
    path: &Path,
    phase: RuntimeUpdatePhase,
    error: Option<String>,
) -> anyhow::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let parent = path
        .parent()
        .context("runtime update status has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}-{}.tmp",
        path.file_name()
            .unwrap_or_else(|| OsStr::new("update-status"))
            .to_string_lossy(),
        std::process::id(),
        crate::now_epoch_secs()
    ));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)
        .with_context(|| format!("failed to stage runtime update status {}", path.display()))?;
    serde_json::to_writer_pretty(
        &mut file,
        &RuntimeUpdateStatus {
            phase,
            error,
            managed_service: true,
        },
    )?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("failed to publish runtime update status {}", path.display()))?;
    Ok(())
}

pub(crate) async fn record_owned_result<F>(status_path: &Path, operation: F) -> anyhow::Result<()>
where
    F: std::future::Future<Output = anyhow::Result<()>>,
{
    write_status(status_path, RuntimeUpdatePhase::Updating, None)?;
    match operation.await {
        Ok(()) => write_status(status_path, RuntimeUpdatePhase::Idle, None),
        Err(error) => record_failure(status_path, &error),
    }
}

pub(crate) fn record_failure(status_path: &Path, error: &anyhow::Error) -> anyhow::Result<()> {
    let diagnostic = format!("{error:#}");
    write_status(
        status_path,
        RuntimeUpdatePhase::Failed,
        Some(diagnostic.chars().take(2_000).collect()),
    )
}

#[cfg(target_os = "macos")]
fn process_executable(pid: u32) -> anyhow::Result<PathBuf> {
    let mut buffer = vec![0_u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: The buffer is writable for its declared length during this call.
    let length = unsafe {
        libc::proc_pidpath(
            pid as libc::c_int,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
        )
    };
    anyhow::ensure!(length > 0, "failed to inspect process {pid}");
    buffer.truncate(length as usize);
    let end = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    buffer.truncate(end);
    Ok(PathBuf::from(std::ffi::OsString::from_vec(buffer)))
}

#[cfg(not(target_os = "macos"))]
fn process_executable(_pid: u32) -> anyhow::Result<PathBuf> {
    anyhow::bail!("deferred backend updates are supported on macOS only")
}

fn process_is_alive(pid: u32) -> bool {
    // SAFETY: Signal zero checks the scalar PID without sending a signal.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

fn wait_for_process_exit(pid: u32, timeout: Duration) -> anyhow::Result<()> {
    let deadline = Instant::now() + timeout;
    while process_is_alive(pid) {
        anyhow::ensure!(
            Instant::now() < deadline,
            "process {pid} did not stop within {} seconds",
            timeout.as_secs()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

pub(crate) fn launchd_pid(service: &str) -> anyhow::Result<Option<u32>> {
    let output = std::process::Command::new("/bin/launchctl")
        .args(["print", service])
        .output()
        .with_context(|| format!("failed to inspect launchd service {service}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix("pid = ")?.trim().parse().ok()
    }))
}

fn stop_pid(pid: u32) -> anyhow::Result<()> {
    // SAFETY: `kill` receives a scalar PID and the valid SIGTERM constant.
    let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    anyhow::ensure!(result == 0, "failed to stop daemon process {pid}");
    wait_for_process_exit(pid, Duration::from_secs(30))
}

pub(crate) struct HeldDaemonExecutable {
    installed: PathBuf,
    held: PathBuf,
    restore_on_drop: bool,
}

impl HeldDaemonExecutable {
    pub(crate) fn hold_parent(
        install_root: &Path,
        transaction_dir: &Path,
        parent_pid: u32,
    ) -> anyhow::Result<Self> {
        let installed = fs::canonicalize(install_root.join("bin/oored"))
            .context("failed to resolve the installed daemon")?;
        let running = fs::canonicalize(process_executable(parent_pid)?)
            .context("failed to resolve the running daemon executable")?;
        anyhow::ensure!(
            running == installed,
            "runtime update parent runs {}, not the selected install at {}",
            running.display(),
            installed.display()
        );
        let held = transaction_dir.join("oored.held");
        fs::rename(&installed, &held).context("failed to hold the current daemon executable")?;
        let mut guard = Self {
            installed,
            held,
            restore_on_drop: true,
        };
        if let Err(error) = stop_pid(parent_pid) {
            guard.restore()?;
            return Err(error);
        }
        Ok(guard)
    }

    pub(crate) fn release_to_transaction(&mut self) {
        self.restore_on_drop = false;
    }

    pub(crate) fn restore(&mut self) -> anyhow::Result<()> {
        if self.held.is_file() && !self.installed.exists() {
            fs::rename(&self.held, &self.installed)
                .context("failed to restore the held daemon executable")?;
        }
        self.restore_on_drop = false;
        Ok(())
    }
}

impl Drop for HeldDaemonExecutable {
    fn drop(&mut self) {
        if self.restore_on_drop {
            let _ = self.restore();
        }
    }
}

/// A candidate daemon executable moved out of `bin/` so that launchd cannot
/// respawn it while the previous release is restored. Dropping the guard puts
/// the executable back when nothing has replaced it, so a rollback that fails or
/// is interrupted never leaves the install without a daemon to run.
pub(crate) struct QuiescedCandidateDaemon {
    installed: PathBuf,
    held: PathBuf,
}

impl QuiescedCandidateDaemon {
    /// Put the candidate back when `bin/oored` is still missing. Returns
    /// whether a file was moved.
    pub(crate) fn restore(&self) -> anyhow::Result<bool> {
        if self.held.is_file() && !self.installed.exists() {
            fs::rename(&self.held, &self.installed).with_context(|| {
                format!(
                    "failed to restore the candidate daemon executable to {}",
                    self.installed.display()
                )
            })?;
            return Ok(true);
        }
        Ok(false)
    }
}

impl Drop for QuiescedCandidateDaemon {
    fn drop(&mut self) {
        if let Ok(true) = self.restore() {
            eprintln!(
                "Restored the candidate daemon executable at {} because no other release replaced it.",
                self.installed.display()
            );
        }
    }
}

pub(crate) fn quiesce_candidate_daemon(
    install_root: &Path,
    transaction_dir: &Path,
) -> anyhow::Result<QuiescedCandidateDaemon> {
    let installed = install_root.join("bin/oored");
    let held = transaction_dir.join("oored.failed-candidate");
    if installed.is_file() {
        fs::rename(&installed, &held)
            .context("failed to hold the failed candidate daemon executable")?;
    }
    let guard = QuiescedCandidateDaemon { installed, held };
    if let Some(pid) = launchd_pid(DAEMON_SERVICE)?
        && let Err(error) = stop_pid(pid)
    {
        guard.restore().with_context(|| {
            format!("failed to restore candidate daemon after quiesce failed: {error:#}")
        })?;
        return Err(error);
    }
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::QuiescedCandidateDaemon;
    use std::fs;

    #[test]
    fn dropping_a_quiesced_candidate_restores_the_missing_daemon() {
        let root = tempfile::tempdir().expect("tempdir");
        let installed = root.path().join("bin/oored");
        let held = root.path().join("transaction/oored.failed-candidate");
        fs::create_dir_all(installed.parent().unwrap()).unwrap();
        fs::create_dir_all(held.parent().unwrap()).unwrap();
        fs::write(&held, b"candidate").unwrap();

        drop(QuiescedCandidateDaemon {
            installed: installed.clone(),
            held: held.clone(),
        });

        assert_eq!(fs::read(&installed).unwrap(), b"candidate");
        assert!(!held.exists());
    }

    #[test]
    fn a_quiesced_candidate_never_replaces_a_restored_daemon() {
        let root = tempfile::tempdir().expect("tempdir");
        let installed = root.path().join("bin/oored");
        let held = root.path().join("transaction/oored.failed-candidate");
        fs::create_dir_all(installed.parent().unwrap()).unwrap();
        fs::create_dir_all(held.parent().unwrap()).unwrap();
        fs::write(&installed, b"previous release").unwrap();
        fs::write(&held, b"candidate").unwrap();

        let guard = QuiescedCandidateDaemon {
            installed: installed.clone(),
            held: held.clone(),
        };
        assert!(!guard.restore().unwrap());
        drop(guard);

        assert_eq!(fs::read(&installed).unwrap(), b"previous release");
        assert_eq!(fs::read(&held).unwrap(), b"candidate");
    }

    #[test]
    fn a_quiesced_candidate_with_nothing_held_is_a_no_op() {
        let root = tempfile::tempdir().expect("tempdir");
        let installed = root.path().join("bin/oored");
        let held = root.path().join("transaction/oored.failed-candidate");
        let guard = QuiescedCandidateDaemon {
            installed: installed.clone(),
            held,
        };
        assert!(!guard.restore().unwrap());
        drop(guard);
        assert!(!installed.exists());
    }
}
