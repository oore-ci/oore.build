use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::Command;

use anyhow::Context;

pub(crate) struct InstallLock {
    file: File,
}

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
        recover_interrupted_frontend_update(install_root)?;
        let root_name = install_root
            .file_name()
            .context("the Oore install root has no directory name")?;
        let mut lock_name = OsString::from(".");
        lock_name.push(root_name);
        lock_name.push(".oore-lifecycle.lock");
        let path = parent.join(lock_name);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
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

fn recover_interrupted_frontend_update(install_root: &Path) -> anyhow::Result<()> {
    let transaction = install_root.join(".oore-web-update-transaction");
    match fs::symlink_metadata(&transaction) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect frontend update recovery state {}",
                    transaction.display()
                )
            });
        }
    }

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

    let status = Command::new(&launcher)
        .arg("recover-update")
        .env("OORE_INSTALL_ROOT", install_root)
        .status()
        .with_context(|| {
            format!(
                "failed to run frontend update recovery with {}",
                launcher.display()
            )
        })?;
    anyhow::ensure!(
        status.success(),
        "frontend update recovery failed with status {status}"
    );
    Ok(())
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
}
