use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Context;

pub(crate) struct InstallLock {
    file: File,
}

/// How `oore-web recover-update` is asked to coordinate with the lifecycle lock.
#[derive(Clone, Copy)]
enum FrontendRecoveryLock {
    /// This process already holds the lifecycle lock for the child.
    HeldByCaller,
    /// The child takes the lifecycle lock itself. Launchers released before
    /// `--lifecycle-lock-held` existed only support this mode.
    TakenByChild,
}

/// `oore-web` exits with this status when it rejects its arguments.
const FRONTEND_LAUNCHER_USAGE_EXIT: i32 = 2;

impl InstallLock {
    pub(crate) fn acquire(install_root: &Path) -> anyhow::Result<Self> {
        let parent = install_root
            .parent()
            .context("the Oore install root has no parent directory")?;
        let parent_metadata = fs::symlink_metadata(parent)
            .with_context(|| format!("failed to inspect lock directory {}", parent.display()))?;
        anyhow::ensure!(
            parent_metadata.file_type().is_dir()
                && parent_metadata.uid() == current_effective_uid()
                && parent_metadata.permissions().mode() & 0o022 == 0,
            "installation lock directory has unsafe ownership, type, or permissions: {}",
            parent.display()
        );
        let root_name = install_root
            .file_name()
            .context("the Oore install root has no directory name")?;
        let mut lock_name = OsString::from(".");
        lock_name.push(root_name);
        lock_name.push(".oore-lifecycle.lock");
        let path = parent.join(lock_name);

        // Take the lock before any recovery, so a command that races another
        // lifecycle operation reports it as busy instead of failing recovery.
        let mut lock = Self::lock(&path)?;
        if frontend_update_needs_recovery(install_root)?
            && !recover_interrupted_frontend_update(
                install_root,
                FrontendRecoveryLock::HeldByCaller,
            )?
        {
            // An older launcher takes the lifecycle lock itself. Hand it over
            // for the recovery, then take it back.
            drop(lock);
            recover_interrupted_frontend_update(install_root, FrontendRecoveryLock::TakenByChild)?;
            lock = Self::lock(&path)?;
        }
        crate::update_transaction::recover_interrupted_updates(install_root)?;
        // A lifecycle command is often the first thing an operator runs after a
        // web UI update was interrupted, so put a held daemon back here.
        if let Err(error) =
            crate::update_supervisor::recover_interrupted_update_and_restart(install_root)
        {
            eprintln!("Could not recover an interrupted backend update: {error:#}");
        }
        Ok(lock)
    }

    fn lock(path: &Path) -> anyhow::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .with_context(|| format!("failed to open installation lock {}", path.display()))?;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to secure installation lock {}", path.display()))?;
        let metadata = file
            .metadata()
            .with_context(|| format!("failed to inspect installation lock {}", path.display()))?;
        anyhow::ensure!(
            metadata.file_type().is_file() && metadata.uid() == current_effective_uid(),
            "installation lock must be a regular file owned by the current user"
        );

        // SAFETY: `file` owns a valid descriptor for this call and remains open
        // for the lifetime of the returned guard. The macOS `lockf` utility used
        // by the bootstrap script also takes a BSD `flock(2)` lock.
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if matches!(
                error.raw_os_error(),
                Some(code) if code == libc::EWOULDBLOCK || code == libc::EAGAIN
            ) {
                anyhow::bail!(
                    "another Oore install, setup, update, or uninstall operation is active"
                );
            }
            return Err(error).context("failed to lock the Oore installation");
        }

        Ok(Self { file })
    }
}

fn frontend_update_needs_recovery(install_root: &Path) -> anyhow::Result<bool> {
    let transaction = install_root.join(".oore-web-update-transaction");
    match fs::symlink_metadata(&transaction) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to inspect frontend update recovery state {}",
                transaction.display()
            )
        }),
    }
}

/// Runs `oore-web recover-update`. Returns `false` when the launcher predates
/// `--lifecycle-lock-held` and rejected it, so the caller must retry with
/// `FrontendRecoveryLock::TakenByChild`.
fn recover_interrupted_frontend_update(
    install_root: &Path,
    lock: FrontendRecoveryLock,
) -> anyhow::Result<bool> {
    let root_metadata = fs::symlink_metadata(install_root)
        .with_context(|| format!("failed to inspect install root {}", install_root.display()))?;
    anyhow::ensure!(
        root_metadata.file_type().is_dir()
            && root_metadata.uid() == current_effective_uid()
            && root_metadata.permissions().mode() & 0o022 == 0,
        "install root has unsafe ownership, type, or permissions: {}",
        install_root.display()
    );

    let launcher = install_root.join("bin/oore-web");
    let launcher_metadata = fs::symlink_metadata(&launcher).with_context(|| {
        format!(
            "frontend update recovery requires the installed launcher {}",
            launcher.display()
        )
    })?;
    anyhow::ensure!(
        launcher_metadata.file_type().is_file()
            && launcher_metadata.uid() == current_effective_uid()
            && launcher_metadata.permissions().mode() & 0o022 == 0
            && launcher_metadata.permissions().mode() & 0o111 != 0,
        "frontend update recovery launcher has unsafe ownership, type, or permissions: {}",
        launcher.display()
    );

    let mut command = Command::new(&launcher);
    command
        .arg("recover-update")
        .env("OORE_INSTALL_ROOT", install_root);
    if matches!(lock, FrontendRecoveryLock::HeldByCaller) {
        command.arg("--lifecycle-lock-held");
    }
    let output = command.stdin(Stdio::null()).output().with_context(|| {
        format!(
            "failed to run frontend update recovery with {}",
            launcher.display()
        )
    })?;
    let status = output.status;
    if matches!(lock, FrontendRecoveryLock::HeldByCaller)
        && status.code() == Some(FRONTEND_LAUNCHER_USAGE_EXIT)
    {
        return Ok(false);
    }
    let _ = std::io::stdout().write_all(&output.stdout);
    let _ = std::io::stderr().write_all(&output.stderr);
    anyhow::ensure!(
        status.success(),
        "frontend update recovery failed with status {status}"
    );
    Ok(true)
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        // SAFETY: `self.file` still owns a valid descriptor during `drop`.
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

fn current_effective_uid() -> u32 {
    // SAFETY: `geteuid` has no arguments, pointer requirements, or failure state.
    unsafe { libc::geteuid() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn private_parent() -> tempfile::TempDir {
        let parent = tempfile::tempdir().unwrap();
        fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o700)).unwrap();
        parent
    }

    fn lock_error(install_root: &Path) -> String {
        match InstallLock::acquire(install_root) {
            Ok(_) => panic!("the installation lock was acquired"),
            Err(error) => format!("{error:#}"),
        }
    }

    #[test]
    fn only_one_lifecycle_operation_holds_the_lock() {
        let parent = private_parent();
        let install_root = parent.path().join(".oore");

        let held = InstallLock::acquire(&install_root).unwrap();
        let lock_path = parent.path().join("..oore.oore-lifecycle.lock");
        assert_eq!(
            fs::metadata(&lock_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(lock_error(&install_root).contains("operation is active"));

        drop(held);
        drop(InstallLock::acquire(&install_root).unwrap());
    }

    #[test]
    fn a_lock_file_left_by_an_exited_process_is_reused() {
        let parent = private_parent();
        let install_root = parent.path().join("oore");
        let lock_path = parent.path().join(".oore.oore-lifecycle.lock");
        fs::write(&lock_path, b"stale").unwrap();
        fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o644)).unwrap();

        let _lock = InstallLock::acquire(&install_root).unwrap();
        assert_eq!(
            fs::metadata(&lock_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn unsafe_lock_locations_are_refused() {
        let parent = private_parent();
        let install_root = parent.path().join("oore");

        fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(lock_error(&install_root).contains("unsafe ownership, type, or permissions"));
        fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o700)).unwrap();

        let target = parent.path().join("elsewhere");
        fs::write(&target, b"").unwrap();
        std::os::unix::fs::symlink(&target, parent.path().join(".oore.oore-lifecycle.lock"))
            .unwrap();
        assert!(lock_error(&install_root).contains("failed to open installation lock"));

        assert!(lock_error(Path::new("/")).contains("no parent directory"));
    }

    #[test]
    fn an_interrupted_frontend_update_needs_its_launcher_to_recover() {
        let parent = private_parent();
        let install_root = parent.path().join("oore");
        fs::create_dir(&install_root).unwrap();
        fs::set_permissions(&install_root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(install_root.join(".oore-web-update-transaction"), b"{}").unwrap();

        assert!(lock_error(&install_root).contains("requires the installed launcher"));

        let launcher = install_root.join("bin/oore-web");
        fs::create_dir(launcher.parent().unwrap()).unwrap();
        fs::write(&launcher, "#!/bin/sh\nexit 3\n").unwrap();
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(lock_error(&install_root).contains("frontend update recovery failed"));

        fs::write(
            &launcher,
            "#!/bin/sh\n[ \"$1\" = recover-update ] || exit 4\nrm \"$OORE_INSTALL_ROOT/.oore-web-update-transaction\"\n",
        )
        .unwrap();
        let _lock = InstallLock::acquire(&install_root).unwrap();
        assert!(!install_root.join(".oore-web-update-transaction").exists());
    }

    /// An install root with a pending frontend transaction and a fake
    /// `oore-web` that logs each invocation's arguments, one line per run.
    fn install_with_frontend_transaction(launcher: &str) -> (tempfile::TempDir, PathBuf) {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("oore");
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(root.join(".oore-web-update-transaction"), b"").unwrap();
        let script = root.join("bin/oore-web");
        fs::write(&script, launcher).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        (parent, root)
    }

    fn launcher_runs(root: &Path) -> Vec<String> {
        fs::read_to_string(root.join("runs"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn a_busy_lock_is_reported_before_frontend_recovery_runs() {
        let (_parent, root) = install_with_frontend_transaction(
            "#!/bin/sh\necho \"$*\" >> \"$OORE_INSTALL_ROOT/runs\"\n",
        );
        fs::remove_file(root.join(".oore-web-update-transaction")).unwrap();
        let held = InstallLock::acquire(&root).unwrap();
        fs::write(root.join(".oore-web-update-transaction"), b"").unwrap();

        let error = InstallLock::acquire(&root).err().unwrap();

        assert_eq!(
            error.to_string(),
            "another Oore install, setup, update, or uninstall operation is active"
        );
        assert!(launcher_runs(&root).is_empty());
        drop(held);
    }

    #[test]
    fn frontend_recovery_runs_under_the_held_lifecycle_lock() {
        let (_parent, root) = install_with_frontend_transaction(
            "#!/bin/sh\necho \"$*\" >> \"$OORE_INSTALL_ROOT/runs\"\nrm \"$OORE_INSTALL_ROOT/.oore-web-update-transaction\"\n",
        );

        let lock = InstallLock::acquire(&root).unwrap();

        assert_eq!(
            launcher_runs(&root),
            ["recover-update --lifecycle-lock-held"]
        );
        assert!(InstallLock::acquire(&root).is_err());
        drop(lock);
        assert!(InstallLock::acquire(&root).is_ok());
    }

    #[test]
    fn an_older_launcher_recovers_with_its_own_lifecycle_lock() {
        let (_parent, root) = install_with_frontend_transaction(
            "#!/bin/sh\necho \"$*\" >> \"$OORE_INSTALL_ROOT/runs\"\n[ \"$#\" -gt 1 ] && exit 2\nrm \"$OORE_INSTALL_ROOT/.oore-web-update-transaction\"\n",
        );

        let lock = InstallLock::acquire(&root).unwrap();

        assert_eq!(
            launcher_runs(&root),
            ["recover-update --lifecycle-lock-held", "recover-update"]
        );
        assert!(InstallLock::acquire(&root).is_err());
        drop(lock);
    }
}
