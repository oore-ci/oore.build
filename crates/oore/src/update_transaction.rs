//! Crash-safe bookkeeping for an interactive `oore update`.
//!
//! The update stages the new release and a snapshot of the installed one in
//! `<install root>/.update-*`, and records how far it got in `journal.json`
//! there. When the process dies before it finishes or rolls back, the next
//! command that takes the lifecycle lock completes the work from that record:
//! it rolls back an install that was still swapping files, keeps one whose
//! files were all in place, and loads the services the update had stopped.

use std::fs;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::Context;

const TRANSACTION_PREFIX: &str = ".update-";
const JOURNAL_FILE: &str = "journal.json";
const JOURNAL_SCHEMA_VERSION: u32 = 1;
const JOURNAL_MAX_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UpdatePhase {
    /// The release is staged and services may be stopped, but no installed
    /// file has changed.
    Staged,
    /// Installed files are being replaced. The database is untouched.
    Installing,
    /// Every file of the new release is in place, including the profile
    /// metadata. Services may still be restarting.
    Installed,
    /// A rollback is restoring the previous files, and the database when
    /// `database` is set.
    RollingBack,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatabaseRestore {
    pub(crate) backup: PathBuf,
    pub(crate) database: PathBuf,
    pub(crate) key: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateJournal {
    schema_version: u32,
    owner_pid: u32,
    phase: UpdatePhase,
    from_version: String,
    to_version: String,
    /// Managed launchd services the update stops or restarts. Recovery loads
    /// any of them that is installed but not running.
    services: Vec<String>,
    /// The pre-update backup a rollback restores once the daemon is stopped.
    database: Option<DatabaseRestore>,
    /// Whether the update also migrated service definitions, which recovery
    /// cannot redo or undo by itself.
    service_migration: bool,
}

/// An `.update-*` staging directory and its journal. Dropping it removes the
/// directory once the update finished or rolled back, or when it never got
/// past staging. Otherwise the directory stays for recovery.
pub(crate) struct UpdateTransaction {
    dir: PathBuf,
    journal: UpdateJournal,
    finished: bool,
}

impl UpdateTransaction {
    pub(crate) fn begin(
        install_root: &Path,
        from_version: &str,
        to_version: &str,
        services: Vec<String>,
        service_migration: bool,
    ) -> anyhow::Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix(TRANSACTION_PREFIX)
            .tempdir_in(install_root)
            .context("failed to create update staging directory")?
            .keep();
        let transaction = Self {
            dir,
            journal: UpdateJournal {
                schema_version: JOURNAL_SCHEMA_VERSION,
                owner_pid: std::process::id(),
                phase: UpdatePhase::Staged,
                from_version: from_version.to_string(),
                to_version: to_version.to_string(),
                services,
                database: None,
                service_migration,
            },
            finished: false,
        };
        transaction.write()?;
        Ok(transaction)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn phase(&self) -> UpdatePhase {
        self.journal.phase
    }

    pub(crate) fn record(&mut self, phase: UpdatePhase) -> anyhow::Result<()> {
        self.journal.phase = phase;
        self.write()
    }

    pub(crate) fn record_rollback(
        &mut self,
        database: Option<DatabaseRestore>,
    ) -> anyhow::Result<()> {
        if database.is_some() {
            self.journal.database = database;
        }
        self.record(UpdatePhase::RollingBack)
    }

    /// Marks the update as finished or fully rolled back, so dropping the
    /// transaction removes its directory.
    pub(crate) fn finish(&mut self) {
        self.finished = true;
    }

    fn write(&self) -> anyhow::Result<()> {
        let path = self.dir.join(JOURNAL_FILE);
        let temporary = self
            .dir
            .join(format!(".{JOURNAL_FILE}.{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> anyhow::Result<()> {
            let mut contents = serde_json::to_vec_pretty(&self.journal)
                .context("failed to serialize the update journal")?;
            contents.push(b'\n');
            crate::write_private_contents(&temporary, &contents)?;
            fs::rename(&temporary, &path)
                .with_context(|| format!("failed to publish update journal {}", path.display()))?;
            crate::sync_directory(&self.dir)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

impl Drop for UpdateTransaction {
    fn drop(&mut self) {
        let untouched = self.journal.phase == UpdatePhase::Staged && !std::thread::panicking();
        if self.finished || untouched {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

/// Finishes or rolls back every `.update-*` transaction a dead `oore update`
/// left behind. The caller holds the lifecycle lock, which every update holds
/// while its transaction exists.
pub(crate) fn recover_interrupted_updates(install_root: &Path) -> anyhow::Result<()> {
    let entries = match fs::read_dir(install_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {}", install_root.display()));
        }
    };
    let mut transactions = Vec::new();
    for entry in entries {
        let entry = entry.context("failed to inspect update state")?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(TRANSACTION_PREFIX))
            && entry.file_type()?.is_dir()
        {
            transactions.push(entry.path());
        }
    }
    transactions.sort();
    for dir in transactions {
        recover_transaction(install_root, &dir)?;
    }
    Ok(())
}

fn recover_transaction(install_root: &Path, dir: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(dir)
        .with_context(|| format!("failed to inspect {}", dir.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_dir() && metadata.uid() == crate::current_effective_uid(),
        "update transaction has an unexpected owner or type: {}",
        dir.display()
    );
    let journal = match read_journal(&dir.join(JOURNAL_FILE)) {
        Ok(Some(journal)) => journal,
        Ok(None) => return recover_unjournaled_transaction(install_root, dir),
        Err(error) => {
            eprintln!(
                "Warning: cannot read the interrupted update in {}: {error:#}. Rerun the verified installer for your channel, then delete that directory.",
                dir.display()
            );
            return Ok(());
        }
    };
    if journal.owner_pid != std::process::id() && process_is_oore(journal.owner_pid) {
        eprintln!(
            "Warning: the update in {} belongs to running process {}; leaving it alone.",
            dir.display(),
            journal.owner_pid
        );
        return Ok(());
    }

    eprintln!(
        "Recovering the interrupted update from {} to {}...",
        journal.from_version, journal.to_version
    );
    // Ask for administrator access before changing anything. A refusal is the
    // one failure that stops the caller, because granting it lets the next
    // attempt succeed.
    let _sudo = authorize_recovery(&journal)?;
    // Any other failure is reported and leaves the record in place. Blocking
    // every lifecycle command on it would also block the installer that can
    // repair the installation.
    match apply_recovery(install_root, dir, &journal) {
        Ok(outcome) => eprintln!("{outcome}"),
        Err(error) => eprintln!(
            "Warning: could not finish recovering the interrupted update in {}: {error:#}. Oore retries on the next install, setup, update or uninstall command; if it keeps failing, rerun the verified installer for your channel.",
            dir.display()
        ),
    }
    Ok(())
}

fn apply_recovery(
    install_root: &Path,
    dir: &Path,
    journal: &UpdateJournal,
) -> anyhow::Result<String> {
    let outcome = match journal.phase {
        UpdatePhase::Staged => format!(
            "The update to {} stopped before it changed any files; {} remains installed.",
            journal.to_version, journal.from_version
        ),
        UpdatePhase::Installed => {
            crate::publish_runner_release_marker(install_root)?;
            format!(
                "Every file of {} was already installed, so the update was kept.",
                journal.to_version
            )
        }
        UpdatePhase::Installing | UpdatePhase::RollingBack => {
            let previous = dir.join("previous");
            anyhow::ensure!(
                previous.join("VERSION").is_file(),
                "it has no snapshot of the previous release to restore"
            );
            if journal.phase == UpdatePhase::RollingBack
                && let Some(database) = &journal.database
            {
                if crate::launchd_service_loaded(crate::DAEMON_SERVICE_LABEL) {
                    crate::stop_launchd_service(crate::DAEMON_SERVICE_LABEL)?;
                }
                restore_database(database)?;
            }
            crate::restore_release_snapshot(install_root, &previous)?;
            crate::restore_runner_release_marker(install_root)?;
            let mut outcome = format!(
                "Restored {} after the update to {} was interrupted.",
                journal.from_version, journal.to_version
            );
            if journal.service_migration {
                outcome.push_str(" That update was also migrating service definitions; rerun the verified installer for your channel to finish the migration.");
            }
            outcome
        }
    };
    for label in &journal.services {
        if crate::managed_service_plist(label)?.0.is_file() && !crate::launchd_service_loaded(label)
        {
            crate::restart_launchd_service(label)?;
        }
    }
    fs::remove_dir_all(dir)
        .with_context(|| format!("failed to remove recovered update {}", dir.display()))?;
    Ok(outcome)
}

/// Requests administrator access when recovery has to stop or load a system
/// service.
fn authorize_recovery(journal: &UpdateJournal) -> anyhow::Result<Option<crate::SudoKeepalive>> {
    let system_service = |label: &str| -> anyhow::Result<bool> {
        let (plist, system) = crate::managed_service_plist(label)?;
        Ok(system && plist.is_file())
    };
    let mut needed = journal.phase == UpdatePhase::RollingBack
        && journal.database.is_some()
        && system_service(crate::DAEMON_SERVICE_LABEL)?
        && crate::launchd_service_loaded(crate::DAEMON_SERVICE_LABEL);
    for label in &journal.services {
        needed |= system_service(label)? && !crate::launchd_service_loaded(label);
    }
    if !needed {
        return Ok(None);
    }
    crate::authorize_system_service_restart()
        .context("recovering the interrupted update needs administrator access to restart Oore's services")
        .map(Some)
}

/// An `.update-*` directory without a journal comes from an older Oore, which
/// kept its progress only in memory. It is safe to remove when the installed
/// release still matches its snapshot. Otherwise the files may be mixed, so it
/// is left for the operator rather than guessed at.
fn recover_unjournaled_transaction(install_root: &Path, dir: &Path) -> anyhow::Result<()> {
    let installed = crate::read_trimmed_file(&install_root.join("VERSION"));
    let snapshot = crate::read_trimmed_file(&dir.join("previous/VERSION"));
    if snapshot.is_none() || installed == snapshot {
        fs::remove_dir_all(dir)
            .with_context(|| format!("failed to remove abandoned update {}", dir.display()))?;
        eprintln!(
            "Removed an abandoned update staging directory: {}",
            dir.display()
        );
        return Ok(());
    }
    eprintln!(
        "Warning: {} holds an update interrupted by an older Oore, and the installed release no longer matches its snapshot. Rerun the verified installer for your channel, then delete that directory.",
        dir.display()
    );
    Ok(())
}

fn read_journal(path: &Path) -> anyhow::Result<Option<UpdateJournal>> {
    let mut file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to open update journal {}", path.display()));
        }
    };
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && metadata.uid() == crate::current_effective_uid()
            && metadata.permissions().mode() & 0o077 == 0
            && metadata.len() <= JOURNAL_MAX_BYTES,
        "update journal has unsafe ownership, permissions, type, or size: {}",
        path.display()
    );
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    let journal: UpdateJournal = serde_json::from_slice(&contents)
        .with_context(|| format!("failed to parse update journal {}", path.display()))?;
    anyhow::ensure!(
        journal.schema_version == JOURNAL_SCHEMA_VERSION,
        "unsupported update journal schema: {}",
        journal.schema_version
    );
    Ok(Some(journal))
}

fn process_is_oore(pid: u32) -> bool {
    // SAFETY: Signal zero checks the scalar PID without sending a signal.
    if pid <= 1 || unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
        return false;
    }
    std::process::Command::new("/bin/ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .is_ok_and(|output| {
            Path::new(String::from_utf8_lossy(&output.stdout).trim())
                .file_name()
                .is_some_and(|name| name == "oore")
        })
}

/// Restores the pre-update backup on a thread of its own, because the commit
/// starts its own Tokio runtime and lifecycle locks are taken from async code.
fn restore_database(restore: &DatabaseRestore) -> anyhow::Result<()> {
    let restore = restore.clone();
    std::thread::spawn(move || -> anyhow::Result<()> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to start the database restore")?
            .block_on(crate::restore_bounded_update_backup(
                &restore.backup,
                &restore.database,
                &restore.key,
            ))
    })
    .join()
    .map_err(|_| anyhow::anyhow!("the database restore stopped unexpectedly"))?
    .context("failed to restore the pre-update database backup")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install_root() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for (path, contents) in [
            ("VERSION", "1.0.0"),
            ("CHANNEL", "stable"),
            ("bin/oore", "old oore"),
            ("bin/oored", "old oored"),
        ] {
            let path = root.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        root
    }

    /// Starts a transaction like `oore update` does, with a snapshot of the
    /// installed release, then installs the new files.
    fn interrupted_update(root: &Path, phase: UpdatePhase) -> PathBuf {
        let mut transaction =
            UpdateTransaction::begin(root, "1.0.0", "1.1.0", Vec::new(), false).unwrap();
        crate::copy_release_snapshot(root, &transaction.path().join("previous")).unwrap();
        if phase != UpdatePhase::Staged {
            fs::write(root.join("VERSION"), "1.1.0").unwrap();
            fs::write(root.join("bin/oored"), "new oored").unwrap();
            transaction.record(phase).unwrap();
        }
        let dir = transaction.path().to_path_buf();
        // A process that dies runs no destructors.
        std::mem::forget(transaction);
        dir
    }

    fn read(root: &Path, path: &str) -> String {
        fs::read_to_string(root.join(path)).unwrap()
    }

    #[test]
    fn an_interrupted_install_is_rolled_back_to_the_snapshot() {
        let root = install_root();
        let dir = interrupted_update(root.path(), UpdatePhase::Installing);

        recover_interrupted_updates(root.path()).unwrap();

        assert_eq!(read(root.path(), "VERSION"), "1.0.0");
        assert_eq!(read(root.path(), "bin/oored"), "old oored");
        assert!(!dir.exists());
    }

    #[test]
    fn a_fully_installed_update_is_kept() {
        let root = install_root();
        let dir = interrupted_update(root.path(), UpdatePhase::Installed);

        recover_interrupted_updates(root.path()).unwrap();

        assert_eq!(read(root.path(), "VERSION"), "1.1.0");
        assert_eq!(read(root.path(), "bin/oored"), "new oored");
        assert!(!dir.exists());
    }

    #[test]
    fn an_update_interrupted_while_staging_is_discarded() {
        let root = install_root();
        let dir = interrupted_update(root.path(), UpdatePhase::Staged);

        recover_interrupted_updates(root.path()).unwrap();

        assert_eq!(read(root.path(), "VERSION"), "1.0.0");
        assert!(!dir.exists());
    }

    #[test]
    fn an_unjournaled_directory_is_removed_only_when_nothing_changed() {
        let root = install_root();
        let unchanged = root.path().join(".update-old-unchanged");
        crate::copy_release_snapshot(root.path(), &unchanged.join("previous")).unwrap();

        recover_interrupted_updates(root.path()).unwrap();
        assert!(!unchanged.exists());

        let mixed = root.path().join(".update-old-mixed");
        crate::copy_release_snapshot(root.path(), &mixed.join("previous")).unwrap();
        fs::write(root.path().join("VERSION"), "1.1.0").unwrap();

        recover_interrupted_updates(root.path()).unwrap();
        assert!(mixed.exists());
        assert_eq!(read(root.path(), "VERSION"), "1.1.0");
    }

    #[test]
    fn a_settled_or_untouched_transaction_cleans_up_and_an_unfinished_one_stays() {
        let root = install_root();
        let untouched =
            UpdateTransaction::begin(root.path(), "1.0.0", "1.1.0", Vec::new(), false).unwrap();
        let untouched_dir = untouched.path().to_path_buf();
        drop(untouched);
        assert!(!untouched_dir.exists());

        let mut unfinished =
            UpdateTransaction::begin(root.path(), "1.0.0", "1.1.0", Vec::new(), false).unwrap();
        unfinished.record(UpdatePhase::RollingBack).unwrap();
        let unfinished_dir = unfinished.path().to_path_buf();
        drop(unfinished);
        assert!(unfinished_dir.join(JOURNAL_FILE).is_file());

        let mut finished =
            UpdateTransaction::begin(root.path(), "1.0.0", "1.1.0", Vec::new(), false).unwrap();
        finished.record(UpdatePhase::Installed).unwrap();
        finished.finish();
        let finished_dir = finished.path().to_path_buf();
        drop(finished);
        assert!(!finished_dir.exists());
    }
}
