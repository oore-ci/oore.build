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

        // A lifecycle command is often the first thing an operator runs after a
        // web UI update was interrupted, so put a held daemon back here.
        if let Err(error) =
            crate::update_supervisor::recover_interrupted_update_and_restart(install_root)
        {
            eprintln!("Could not recover an interrupted backend update: {error:#}");
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
