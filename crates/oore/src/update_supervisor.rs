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
pub(crate) const TRANSACTION_PREFIX: &str = ".supervised-update-";
const HELD_DAEMON: &str = "oored.held";
const FAILED_CANDIDATE: &str = "oored.failed-candidate";
const ACTIVE_REQUEST_PREFIX: &str = "runtime-update-active-";
const STATUS_FILE: &str = ".runtime-update-status.json";

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
        let mut guard = Self::hold(installed, transaction_dir)?;
        if let Err(error) = stop_pid(parent_pid) {
            guard.restore()?;
            return Err(error);
        }
        Ok(guard)
    }

    fn hold(installed: PathBuf, transaction_dir: &Path) -> anyhow::Result<Self> {
        let held = transaction_dir.join(HELD_DAEMON);
        fs::rename(&installed, &held).context("failed to hold the current daemon executable")?;
        Ok(Self {
            installed,
            held,
            restore_on_drop: true,
        })
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
    let held = transaction_dir.join(FAILED_CANDIDATE);
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

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RecoveredDaemon {
    /// The supervisor stopped before it installed the new release, so the
    /// previous release is back in place and its transaction is gone.
    Previous,
    /// The supervisor stopped while it rolled back a failed release. The
    /// candidate is back in place, and the transaction keeps the previous
    /// release snapshot.
    Candidate { transaction: PathBuf },
}

/// Put `bin/oored` back when a supervised update stopped while it held the
/// daemon executable in its transaction directory, for example because the
/// supervisor was killed or the Mac lost power. Without this, launchd has
/// nothing to start and the web UI stays down.
///
/// The caller must hold the lifecycle lock. A live supervisor holds that lock
/// for its whole transaction, so every transaction directory found here
/// belongs to a supervisor that no longer runs.
pub(crate) fn recover_interrupted_update(
    install_root: &Path,
) -> anyhow::Result<Option<RecoveredDaemon>> {
    clear_abandoned_claims(install_root)?;
    let Some(recovered) = restore_held_daemon(install_root)? else {
        return Ok(None);
    };
    let error = match &recovered {
        RecoveredDaemon::Previous => {
            "The update stopped before it installed the new release. Oore restored the previous backend, so you can try the update again."
        }
        RecoveredDaemon::Candidate { .. } => {
            "The update stopped while it rolled back a failed release. Oore restored the new backend so that the service can start. Check logs/update-supervisor.log before you try again."
        }
    };
    fail_in_flight_status(&install_root.join(STATUS_FILE), error)?;
    Ok(Some(recovered))
}

/// Run [`recover_interrupted_update`] and restart the daemon when it put the
/// executable back.
pub(crate) fn recover_interrupted_update_and_restart(install_root: &Path) -> anyhow::Result<()> {
    let Some(recovered) = recover_interrupted_update(install_root)? else {
        return Ok(());
    };
    let installed = install_root.join("bin/oored");
    match recovered {
        RecoveredDaemon::Previous => eprintln!(
            "Restored {} from an interrupted backend update.",
            installed.display()
        ),
        RecoveredDaemon::Candidate { transaction } => eprintln!(
            "Restored the candidate daemon at {} after an interrupted rollback. The previous release snapshot is kept in {}.",
            installed.display(),
            transaction.display()
        ),
    }
    if let Err(error) = kickstart_daemon() {
        eprintln!(
            "Could not restart the daemon ({error:#}). Run `sudo launchctl kickstart -k {DAEMON_SERVICE}`."
        );
    }
    Ok(())
}

fn restore_held_daemon(install_root: &Path) -> anyhow::Result<Option<RecoveredDaemon>> {
    let installed = install_root.join("bin/oored");
    if fs::symlink_metadata(&installed).is_ok() {
        return Ok(None);
    }
    let mut transactions = Vec::new();
    let entries = match fs::read_dir(install_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {}", install_root.display()));
        }
    };
    for entry in entries {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir()
            && entry
                .file_name()
                .to_string_lossy()
                .starts_with(TRANSACTION_PREFIX)
        {
            transactions.push((metadata.modified()?, entry.path()));
        }
    }
    // Newest first: an older directory can only hold an executable that a
    // later update already replaced.
    transactions.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (_, transaction) in transactions {
        let candidate = transaction.join(FAILED_CANDIDATE);
        if candidate.is_file() {
            fs::rename(&candidate, &installed).with_context(|| {
                format!(
                    "failed to restore the candidate daemon from {}",
                    candidate.display()
                )
            })?;
            return Ok(Some(RecoveredDaemon::Candidate { transaction }));
        }
        let held = transaction.join(HELD_DAEMON);
        if held.is_file() {
            // `install_staged_release` replaces `bin/oore` just before
            // `bin/oored`, so put the previous CLI back as well.
            let previous_cli = transaction.join("previous/bin/oore");
            if previous_cli.is_file() {
                replace_from_snapshot(&previous_cli, &install_root.join("bin/oore"))?;
            }
            fs::rename(&held, &installed).with_context(|| {
                format!("failed to restore the held daemon from {}", held.display())
            })?;
            if let Err(error) = fs::remove_dir_all(&transaction) {
                eprintln!(
                    "Could not remove the interrupted update files in {}: {error}",
                    transaction.display()
                );
            }
            return Ok(Some(RecoveredDaemon::Previous));
        }
    }
    Ok(None)
}

fn replace_from_snapshot(snapshot: &Path, destination: &Path) -> anyhow::Result<()> {
    let parent = destination
        .parent()
        .context("restored file has no parent directory")?;
    let staged = parent.join(format!(
        ".{}.restore-{}",
        destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        std::process::id()
    ));
    fs::copy(snapshot, &staged)
        .with_context(|| format!("failed to stage {}", destination.display()))?;
    fs::rename(&staged, destination)
        .with_context(|| format!("failed to restore {}", destination.display()))
}

/// Remove request claims whose supervisor process is gone. A supervisor claims
/// its request before it takes the lifecycle lock, so a live owner is skipped.
fn clear_abandoned_claims(install_root: &Path) -> anyhow::Result<()> {
    let run = install_root.join("run");
    let entries = match fs::read_dir(&run) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", run.display()));
        }
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(pid) = name
            .to_string_lossy()
            .strip_prefix(ACTIVE_REQUEST_PREFIX)
            .and_then(|rest| rest.strip_suffix(".json"))
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        if pid != std::process::id() && !process_is_alive(pid) {
            match fs::remove_file(entry.path()) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "failed to clear abandoned update claim {}",
                            entry.path().display()
                        )
                    });
                }
            }
        }
    }
    Ok(())
}

fn fail_in_flight_status(status_path: &Path, error: &str) -> anyhow::Result<()> {
    let status: RuntimeUpdateStatus = match fs::read(status_path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(status) => status,
            // An unreadable status cannot report an update that is still running.
            Err(_) => {
                return write_status(
                    status_path,
                    RuntimeUpdatePhase::Failed,
                    Some(error.to_string()),
                );
            }
        },
        Err(read_error) if read_error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(read_error) => {
            return Err(read_error)
                .with_context(|| format!("failed to read {}", status_path.display()));
        }
    };
    if matches!(
        status.phase,
        RuntimeUpdatePhase::Updating | RuntimeUpdatePhase::Restarting
    ) {
        write_status(
            status_path,
            RuntimeUpdatePhase::Failed,
            Some(error.to_string()),
        )?;
    }
    Ok(())
}

fn kickstart_daemon() -> anyhow::Result<()> {
    anyhow::ensure!(
        cfg!(target_os = "macos"),
        "managed service restart is supported on macOS only"
    );
    // SAFETY: `geteuid` has no preconditions.
    let mut command = if unsafe { libc::geteuid() } == 0 {
        std::process::Command::new("/bin/launchctl")
    } else {
        // The setup recovery rule allows exactly this command without a password.
        let mut command = std::process::Command::new("/usr/bin/sudo");
        command.args(["-n", "/bin/launchctl"]);
        command
    };
    command.args(["kickstart", "-k", DAEMON_SERVICE]);
    let status = crate::run_command_with_timeout(
        &mut command,
        "launchd kickstart",
        Duration::from_secs(15),
    )?;
    anyhow::ensure!(status.success(), "launchctl kickstart exited with {status}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        HeldDaemonExecutable, QuiescedCandidateDaemon, RecoveredDaemon, recover_interrupted_update,
        write_status,
    };
    use oore_contract::{RuntimeUpdatePhase, RuntimeUpdateStatus};
    use std::fs;
    use std::path::Path;

    fn install_with_daemon(root: &Path) {
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("bin/oored"), b"previous daemon").unwrap();
        fs::write(root.join("bin/oore"), b"previous cli").unwrap();
    }

    /// Hold the daemon the way a supervisor does, then stop as if SIGKILLed:
    /// neither the guard nor the transaction directory runs its destructor.
    fn interrupt_after_hold(root: &Path) -> std::path::PathBuf {
        let transaction = tempfile::Builder::new()
            .prefix(super::TRANSACTION_PREFIX)
            .tempdir_in(root)
            .unwrap();
        let previous = transaction.path().join("previous/bin");
        fs::create_dir_all(&previous).unwrap();
        fs::copy(root.join("bin/oore"), previous.join("oore")).unwrap();
        fs::copy(root.join("bin/oored"), previous.join("oored")).unwrap();
        let guard = HeldDaemonExecutable::hold(root.join("bin/oored"), transaction.path()).unwrap();
        std::mem::forget(guard);
        transaction.keep()
    }

    fn status(root: &Path) -> RuntimeUpdateStatus {
        serde_json::from_slice(&fs::read(root.join(".runtime-update-status.json")).unwrap())
            .unwrap()
    }

    #[test]
    fn an_update_killed_after_holding_the_daemon_is_restored() {
        let root = tempfile::tempdir().unwrap();
        install_with_daemon(root.path());
        let status_path = root.path().join(".runtime-update-status.json");
        write_status(&status_path, RuntimeUpdatePhase::Restarting, None).unwrap();
        let transaction = interrupt_after_hold(root.path());
        // The supervisor had already replaced the CLI when it stopped.
        fs::write(root.path().join("bin/oore"), b"candidate cli").unwrap();
        assert!(!root.path().join("bin/oored").exists());

        assert_eq!(
            recover_interrupted_update(root.path()).unwrap(),
            Some(RecoveredDaemon::Previous)
        );

        assert_eq!(
            fs::read(root.path().join("bin/oored")).unwrap(),
            b"previous daemon"
        );
        assert_eq!(
            fs::read(root.path().join("bin/oore")).unwrap(),
            b"previous cli"
        );
        assert!(!transaction.exists());
        let status = status(root.path());
        assert_eq!(status.phase, RuntimeUpdatePhase::Failed);
        assert!(
            status
                .error
                .unwrap()
                .contains("restored the previous backend")
        );
    }

    #[test]
    fn an_interrupted_rollback_restores_the_candidate_and_keeps_the_snapshot() {
        let root = tempfile::tempdir().unwrap();
        install_with_daemon(root.path());
        let transaction = interrupt_after_hold(root.path());
        fs::write(root.path().join("bin/oored"), b"candidate daemon").unwrap();
        fs::rename(
            root.path().join("bin/oored"),
            transaction.join("oored.failed-candidate"),
        )
        .unwrap();

        assert_eq!(
            recover_interrupted_update(root.path()).unwrap(),
            Some(RecoveredDaemon::Candidate {
                transaction: transaction.clone()
            })
        );

        assert_eq!(
            fs::read(root.path().join("bin/oored")).unwrap(),
            b"candidate daemon"
        );
        assert!(transaction.join("previous/bin/oored").is_file());
    }

    #[test]
    fn a_present_daemon_and_a_finished_status_are_left_alone() {
        let root = tempfile::tempdir().unwrap();
        install_with_daemon(root.path());
        let transaction = interrupt_after_hold(root.path());
        fs::write(root.path().join("bin/oored"), b"candidate daemon").unwrap();
        let status_path = root.path().join(".runtime-update-status.json");
        write_status(&status_path, RuntimeUpdatePhase::Idle, None).unwrap();

        assert_eq!(recover_interrupted_update(root.path()).unwrap(), None);

        assert_eq!(
            fs::read(root.path().join("bin/oored")).unwrap(),
            b"candidate daemon"
        );
        assert!(transaction.join("oored.held").is_file());
        assert_eq!(status(root.path()).phase, RuntimeUpdatePhase::Idle);
    }

    #[test]
    fn claims_from_stopped_supervisors_are_cleared() {
        let root = tempfile::tempdir().unwrap();
        let run = root.path().join("run");
        fs::create_dir_all(&run).unwrap();
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let stopped = child.id();
        child.wait().unwrap();
        let stale = run.join(format!("runtime-update-active-{stopped}.json"));
        let live = run.join(format!("runtime-update-active-{}.json", std::process::id()));
        fs::write(&stale, b"{}").unwrap();
        fs::write(&live, b"{}").unwrap();

        recover_interrupted_update(root.path()).unwrap();

        assert!(!stale.exists());
        assert!(live.exists());
    }

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
