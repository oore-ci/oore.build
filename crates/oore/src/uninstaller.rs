use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::Context;
use clap::Args;
use oore_cli_ui::{PromptResult, Terminal};
use oore_contract::{RuntimeUpdatePhase, RuntimeUpdateStatus};

use crate::install_lock::InstallLock;
use crate::install_manifest::{InstallComponent, InstallManifest, InstallProfile, InstallService};
use crate::install_path_exists as path_exists;
use crate::managed_services::{
    legacy_v0141_service_is_owned, legacy_v0141_updater_is_owned, remove_legacy_v0141_updater,
    remove_service, service_is_owned,
};

const PATH_BLOCK_START: &str = "# >>> oore PATH >>>";
const PATH_BLOCK_END: &str = "# <<< oore PATH <<<";
const UPDATER_SERVICE_LABEL: &str = "build.oore.oore-updater";
const MAX_SHELL_CONFIG_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Args)]
pub(crate) struct UninstallArgs {
    /// Remove Oore data as well as installed components.
    #[arg(long, default_value_t = false)]
    purge: bool,

    /// Apply the displayed plan without an interactive confirmation.
    #[arg(long, default_value_t = false)]
    yes: bool,

    /// Remove an exact service-managed v0.1.41 installation without deleting its data.
    #[arg(
        long = "legacy-v0-1-41",
        default_value_t = false,
        conflicts_with = "purge"
    )]
    legacy_v0_1_41: bool,

    /// Uninstall even though the installation manifest cannot be read. Oore
    /// then removes every component path and owned service it can prove
    /// belongs to this install root.
    #[arg(
        long = "ignore-invalid-manifest",
        default_value_t = false,
        conflicts_with = "legacy_v0_1_41"
    )]
    ignore_invalid_manifest: bool,
}

struct RemovalPlan {
    install_root: PathBuf,
    installation: InstallationKind,
    services: Vec<InstallService>,
    path_edits: Vec<PathEdit>,
    component_targets: Vec<OwnedTarget>,
    purge_roots: Vec<PathBuf>,
    legacy_updater: bool,
    updater_preserved: bool,
}

#[derive(Clone, Copy)]
enum InstallationKind {
    Profile(InstallProfile),
    InvalidManifest,
    BootstrapOnly,
    LegacyV0141(LegacyInstallMode),
}

#[derive(Clone, Copy)]
enum LegacyInstallMode {
    All,
    Backend,
    Frontend,
}

struct BootstrapInstall {
    shell_path_files: Vec<String>,
    targets: Vec<OwnedTarget>,
}

struct LegacyInstall {
    mode: LegacyInstallMode,
    services: Vec<InstallService>,
    updater: bool,
    shell_path_files: Vec<String>,
    targets: Vec<OwnedTarget>,
}

struct OwnedTarget {
    relative: PathBuf,
    identity: FileIdentity,
    directory: bool,
    mutable_runtime_state: bool,
}

struct PathEdit {
    path: PathBuf,
    original: Vec<u8>,
    updated: Vec<u8>,
    mode: u32,
    identity: FileIdentity,
    published_identity: Option<FileIdentity>,
}

#[derive(Clone, Copy)]
struct FileIdentity {
    dev: u64,
    ino: u64,
}

struct ComponentRemovalFailure {
    error: anyhow::Error,
    files_restored: bool,
}

pub(crate) fn handle(args: UninstallArgs, terminal: Terminal) -> anyhow::Result<()> {
    terminal.intro("Uninstall")?;

    let install_root = resolve_install_root()?;
    validate_install_root(&install_root, !args.legacy_v0_1_41)?;
    let _install_lock = InstallLock::acquire(&install_root)?;

    refuse_update_state(&install_root)?;
    let manifest_path = install_root.join("install-manifest.json");
    let mut sudo_keepalive = None;
    let (installation, recorded_services, shell_path_files, component_targets, legacy_updater) =
        match fs::symlink_metadata(&manifest_path) {
            Ok(_) => {
                anyhow::ensure!(
                    !args.legacy_v0_1_41,
                    "--legacy-v0-1-41 requires a pre-manifest v0.1.41 installation"
                );
                match InstallManifest::load(&manifest_path) {
                    Ok(manifest) => {
                        let mut services = manifest
                            .lifecycle
                            .services
                            .iter()
                            .map(|managed| managed.service)
                            .collect::<Vec<_>>();
                        if matches!(
                            manifest.profile,
                            InstallProfile::Complete | InstallProfile::ControlPlane
                        ) {
                            services.push(InstallService::Updater);
                        }
                        let targets =
                            preflight_component_targets(&install_root, &manifest.components)?;
                        (
                            InstallationKind::Profile(manifest.profile),
                            services,
                            manifest.shell_path_files,
                            targets,
                            false,
                        )
                    }
                    Err(error) if args.ignore_invalid_manifest => {
                        // Refuse a manifest that is foreign-owned or a link: only
                        // unreadable contents are safe to set aside.
                        crate::install_manifest::ensure_private_manifest_file(&manifest_path)
                            .map_err(|metadata_error| {
                                metadata_error.context(format!("{error:#}"))
                            })?;
                        let targets = preflight_component_targets(
                            &install_root,
                            InstallProfile::Complete.components(),
                        )?;
                        (
                            InstallationKind::InvalidManifest,
                            ALL_SERVICES.to_vec(),
                            SHELL_PATH_FILES.map(String::from).to_vec(),
                            targets,
                            false,
                        )
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                anyhow::ensure!(
                    !args.purge,
                    "--purge requires an installation manifest; rerun without --purge to remove this bootstrap-only CLI"
                );
                if args.legacy_v0_1_41 {
                    let legacy_mode = detect_legacy_v0141_layout(&install_root)?.context(
                        "--legacy-v0-1-41 requires an exact pre-manifest v0.1.41 installation",
                    )?;
                    if !matches!(legacy_mode, LegacyInstallMode::Frontend) {
                        terminal.note(
                            "Administrator access",
                            "Oore must inspect the protected v0.1.41 launchd definitions before it can show a safe removal plan. Your password is requested by sudo and is not stored by Oore.",
                        )?;
                        sudo_keepalive = Some(crate::authorize_system_service_restart()?);
                    }
                    let legacy = preflight_legacy_v0141(&install_root)?;
                    (
                        InstallationKind::LegacyV0141(legacy.mode),
                        legacy.services,
                        legacy.shell_path_files,
                        legacy.targets,
                        legacy.updater,
                    )
                } else {
                    let bootstrap = preflight_bootstrap_only(&install_root)?;
                    (
                        InstallationKind::BootstrapOnly,
                        Vec::new(),
                        bootstrap.shell_path_files,
                        bootstrap.targets,
                        false,
                    )
                }
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", manifest_path.display()));
            }
        };
    let services = preflight_services(&install_root, &recorded_services)?;
    let path_edits = if matches!(installation, InstallationKind::LegacyV0141(_)) {
        preflight_legacy_path_edits(&install_root)?
    } else {
        preflight_path_edits(&install_root, &shell_path_files)?
    };
    let updater_preserved = updater_service_present()?
        && !legacy_updater
        && !services.contains(&InstallService::Updater);
    let purge_roots = if args.purge {
        preflight_purge_roots(&install_root)?
    } else {
        Vec::new()
    };

    let mut plan = RemovalPlan {
        install_root,
        installation,
        services,
        path_edits,
        component_targets,
        purge_roots,
        legacy_updater,
        updater_preserved,
    };
    terminal.note("Removal plan", format_plan(&plan, args.purge))?;
    if !confirm_removal(&args, terminal)? {
        terminal.outro("No changes were made.")?;
        return Ok(());
    }
    if (!plan.services.is_empty() || plan.legacy_updater) && !args.legacy_v0_1_41 {
        sudo_keepalive = Some(crate::authorize_system_service_restart()?);
    }
    let _sudo_keepalive = sudo_keepalive;

    apply_path_edits(&mut plan.path_edits)?;
    if plan.legacy_updater
        && let Err(error) = remove_legacy_updater(&plan.install_root, terminal)
    {
        return match rollback_path_edits(&mut plan.path_edits) {
            Ok(()) => Err(error.context("installed component files remain")),
            Err(rollback_error) => Err(error.context(format!(
                "updater removal failed, and restoring the shell PATH also failed: {rollback_error:#}"
            ))),
        };
    }
    if plan.legacy_updater
        && let Err(error) = verify_legacy_updater_absent(&plan.install_root)
    {
        return match rollback_path_edits(&mut plan.path_edits) {
            Ok(()) => Err(error.context(
                "legacy updater removal could not be verified; installed component files remain",
            )),
            Err(rollback_error) => Err(error.context(format!(
                "legacy updater removal could not be verified, and restoring the shell PATH also failed: {rollback_error:#}"
            ))),
        };
    }
    if let Err(error) = remove_services(&plan.install_root, &plan.services, terminal) {
        return match rollback_path_edits(&mut plan.path_edits) {
            Ok(()) if plan.legacy_updater => Err(error.context(
                "the legacy updater was already removed; installed component files remain",
            )),
            Ok(()) => Err(error),
            Err(rollback_error) if plan.legacy_updater => Err(error.context(format!(
                "the legacy updater was already removed, and restoring the shell PATH also failed: {rollback_error:#}"
            ))),
            Err(rollback_error) => Err(error.context(format!(
                "restoring the shell PATH also failed: {rollback_error:#}"
            ))),
        };
    }
    if let Err(error) = verify_services_absent(&plan.install_root, &plan.services) {
        return match rollback_path_edits(&mut plan.path_edits) {
            Ok(()) if plan.legacy_updater => Err(error.context(
                "service removal could not be verified; the legacy updater was already removed, and installed component files remain",
            )),
            Ok(()) => Err(error.context(
                "service removal could not be verified; installed component files remain",
            )),
            Err(rollback_error) if plan.legacy_updater => Err(error.context(format!(
                "service removal could not be verified; the legacy updater was already removed, and restoring the shell PATH also failed: {rollback_error:#}"
            ))),
            Err(rollback_error) => Err(error.context(format!(
                "service removal could not be verified, and restoring the shell PATH also failed: {rollback_error:#}"
            ))),
        };
    }
    if args.purge {
        purge(&plan, terminal)?;
    } else if let Err(failure) = remove_component_targets(&plan, terminal) {
        if !failure.files_restored {
            return Err(failure.error.context(
                "component removal is partial; the managed shell PATH block remains removed because installed paths can already be absent",
            ));
        }
        return match rollback_path_edits(&mut plan.path_edits) {
            Ok(()) => Err(failure.error.context(
                "managed services were already removed; component files and the shell PATH were restored",
            )),
            Err(rollback_error) => Err(failure.error.context(format!(
                "managed services were already removed, and restoring the shell PATH also failed: {rollback_error:#}"
            ))),
        };
    }

    verify_path_edits(&plan.path_edits)?;
    if args.purge {
        verify_paths_absent(&plan.purge_roots)?;
    } else {
        verify_targets_absent(&plan.install_root, &plan.component_targets)?;
        prune_empty_component_directories(&plan.install_root)?;
    }

    let outcome = if args.purge {
        "Oore and its canonical data were removed."
    } else {
        "Oore components were removed. Your data and unlisted files remain."
    };
    if plan.updater_preserved {
        terminal.note(
            "Preserved service",
            "An updater service exists outside this installation manifest. Oore left it unchanged.",
        )?;
    }
    terminal.outro(outcome)?;
    Ok(())
}

fn resolve_install_root() -> anyhow::Result<PathBuf> {
    if let Ok(value) = std::env::var("OORE_INSTALL_ROOT")
        && !value.is_empty()
    {
        return validate_install_root_text(&value).map(PathBuf::from);
    }

    if let Some(root) = oore_contract::install_root::install_root_for_current_executable() {
        return Ok(root);
    }

    let home = dirs::home_dir().context("could not determine the home directory")?;
    Ok(home.join(".oore"))
}

fn validate_install_root_text(value: &str) -> anyhow::Result<&str> {
    anyhow::ensure!(!value.is_empty(), "OORE_INSTALL_ROOT cannot be empty");
    anyhow::ensure!(
        !value.contains(['\n', '\r']),
        "OORE_INSTALL_ROOT cannot contain a newline"
    );
    anyhow::ensure!(
        !value.contains(':'),
        "OORE_INSTALL_ROOT cannot contain a colon"
    );
    anyhow::ensure!(
        Path::new(value).is_absolute(),
        "OORE_INSTALL_ROOT must be an absolute path"
    );
    Ok(value)
}

fn validate_install_root(root: &Path, require_installed_cli: bool) -> anyhow::Result<()> {
    let root_text = root
        .to_str()
        .context("the install root is not valid UTF-8")?;
    validate_install_root_text(root_text)?;
    anyhow::ensure!(
        root.components()
            .all(|component| matches!(component, Component::RootDir | Component::Normal(_))),
        "the install root contains an unsupported path component"
    );

    let home = dirs::home_dir().context("could not determine the home directory")?;
    anyhow::ensure!(
        root != Path::new("/"),
        "refusing to use / as an install root"
    );
    anyhow::ensure!(
        root != home,
        "refusing to use the home directory as an install root"
    );

    let metadata = owned_directory_metadata(root, "install root")?;
    anyhow::ensure!(
        metadata.permissions().mode() & 0o022 == 0,
        "the install root is writable by another user: {}",
        root.display()
    );
    let canonical = fs::canonicalize(root)
        .with_context(|| format!("failed to resolve install root {}", root.display()))?;
    anyhow::ensure!(
        canonical == root,
        "the install root contains a symbolic link or non-canonical component: {}",
        root.display()
    );

    let bin = root.join("bin");
    let bin_metadata = owned_directory_metadata(&bin, "Oore bin directory")?;
    anyhow::ensure!(
        bin_metadata.permissions().mode() & 0o022 == 0,
        "the Oore bin directory is writable by another user: {}",
        bin.display()
    );
    if require_installed_cli {
        let cli = bin.join("oore");
        let cli_metadata = owned_regular_file_metadata(&cli, "installed Oore CLI")?;
        anyhow::ensure!(
            cli_metadata.permissions().mode() & 0o111 != 0,
            "the installed Oore CLI is not executable: {}",
            cli.display()
        );
    }
    Ok(())
}

fn refuse_update_state(root: &Path) -> anyhow::Result<()> {
    for path in [
        root.join("run/runtime-update-queue/request.json"),
        root.join(".runtime-update-status.json"),
    ] {
        if path
            .file_name()
            .is_some_and(|name| name == ".runtime-update-status.json")
        {
            inspect_runtime_update_status(&path)?;
        } else if path_exists(&path)? {
            anyhow::bail!(
                "runtime update state exists at {}; finish or recover the update before uninstalling",
                path.display()
            );
        }
    }

    for (directory, prefixes) in [
        (root.to_path_buf(), &[".update-", ".supervised-update-"][..]),
        (root.join("run"), &["runtime-update-active-"][..]),
    ] {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", directory.display()));
            }
        };
        for entry in entries {
            let entry = entry.context("failed to inspect update state")?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("update state contains a non-UTF-8 file name"))?;
            if prefixes.iter().any(|prefix| name.starts_with(prefix)) {
                anyhow::bail!(
                    "update transaction state exists at {}; finish or recover the update before uninstalling",
                    entry.path().display()
                );
            }
        }
    }
    Ok(())
}

fn inspect_runtime_update_status(path: &Path) -> anyhow::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    };
    validate_owned_regular_metadata(path, &metadata, "runtime update status")?;
    let file = open_owned_file(path, &metadata, "runtime update status")?;
    let status: RuntimeUpdateStatus = serde_json::from_reader(file)
        .with_context(|| format!("invalid runtime update status at {}", path.display()))?;
    anyhow::ensure!(
        !matches!(
            status.phase,
            RuntimeUpdatePhase::Updating | RuntimeUpdatePhase::Restarting
        ),
        "a runtime update is active; wait for it to finish before uninstalling"
    );
    Ok(())
}

/// Every service Oore can register. Each one is removed only when its launchd
/// definition proves that it belongs to the install root.
const ALL_SERVICES: [InstallService; 4] = [
    InstallService::Updater,
    InstallService::Web,
    InstallService::Runner,
    InstallService::Daemon,
];

/// Every shell file that can hold the Oore PATH block. Files without the exact
/// block are left unchanged.
const SHELL_PATH_FILES: [&str; 3] = [".zshrc", ".bashrc", ".bash_profile"];

fn preflight_services(
    root: &Path,
    recorded: &[InstallService],
) -> anyhow::Result<Vec<InstallService>> {
    let mut present = Vec::new();

    for service in ALL_SERVICES {
        let owned = service_is_owned(root, service).with_context(|| {
            format!(
                "cannot prove ownership of service {}; no services or files were removed",
                service.label()
            )
        })?;
        if recorded.contains(&service) {
            if owned {
                present.push(service);
            }
        } else {
            anyhow::ensure!(
                !owned,
                "service {} exists but the installation manifest does not own it; no services or files were removed",
                service.label()
            );
        }
    }
    Ok(present)
}

fn preflight_path_edits(root: &Path, shell_path_files: &[String]) -> anyhow::Result<Vec<PathEdit>> {
    let home = dirs::home_dir().context("could not determine the home directory")?;
    let expected_export = format!(
        "export PATH=\"{}:$PATH\"",
        escape_double_quoted(&root.join("bin").to_string_lossy())
    );
    let mut edits = Vec::new();

    for name in shell_path_files {
        anyhow::ensure!(
            matches!(name.as_str(), ".zshrc" | ".bashrc" | ".bash_profile"),
            "unsupported shell PATH file {name}"
        );
        let path = home.join(name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
            }
        };
        validate_owned_regular_metadata(&path, &metadata, "shell configuration")?;
        anyhow::ensure!(
            metadata.permissions().mode() & 0o022 == 0,
            "shell configuration is writable by another user: {}",
            path.display()
        );
        anyhow::ensure!(
            metadata.len() <= MAX_SHELL_CONFIG_BYTES,
            "shell configuration is too large to edit safely: {}",
            path.display()
        );
        validate_writable_parent(&path)?;
        let mut file = open_owned_file(&path, &metadata, "shell configuration")?;
        let mut original = Vec::with_capacity(metadata.len() as usize);
        file.read_to_end(&mut original)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let contents = std::str::from_utf8(&original)
            .with_context(|| format!("shell configuration is not UTF-8: {}", path.display()))?;
        let Some(updated) = remove_exact_path_block(contents, &expected_export, &path)? else {
            continue;
        };
        edits.push(PathEdit {
            path,
            original,
            updated: updated.into_bytes(),
            mode: metadata.permissions().mode(),
            identity: identity(&metadata),
            published_identity: None,
        });
    }
    Ok(edits)
}

fn preflight_legacy_path_edits(root: &Path) -> anyhow::Result<Vec<PathEdit>> {
    let home = dirs::home_dir().context("could not determine the home directory")?;
    let expected_export = format!("export PATH=\"{}:$PATH\"", root.join("bin").display());
    let mut edits = Vec::new();

    for name in [".zshrc", ".bashrc", ".bash_profile", ".profile"] {
        let path = home.join(name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
            }
        };
        validate_owned_regular_metadata(&path, &metadata, "shell configuration")?;
        anyhow::ensure!(
            metadata.permissions().mode() & 0o022 == 0,
            "shell configuration is writable by another user: {}",
            path.display()
        );
        anyhow::ensure!(
            metadata.len() <= MAX_SHELL_CONFIG_BYTES,
            "shell configuration is too large to edit safely: {}",
            path.display()
        );
        validate_writable_parent(&path)?;
        let mut file = open_owned_file(&path, &metadata, "shell configuration")?;
        let mut original = Vec::with_capacity(metadata.len() as usize);
        file.read_to_end(&mut original)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let contents = std::str::from_utf8(&original)
            .with_context(|| format!("shell configuration is not UTF-8: {}", path.display()))?;
        let Some(updated) = remove_exact_legacy_path_block(contents, &expected_export, &path)?
        else {
            continue;
        };
        edits.push(PathEdit {
            path,
            original,
            updated: updated.into_bytes(),
            mode: metadata.permissions().mode(),
            identity: identity(&metadata),
            published_identity: None,
        });
    }
    Ok(edits)
}

fn remove_exact_legacy_path_block(
    contents: &str,
    expected_export: &str,
    path: &Path,
) -> anyhow::Result<Option<String>> {
    const LEGACY_PATH_MARKER: &str = "# Oore CI";

    fn line_text(line: &str) -> &str {
        line.strip_suffix('\n').unwrap_or(line)
    }

    let lines = contents.split_inclusive('\n').collect::<Vec<_>>();
    let markers = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| (line_text(line) == LEGACY_PATH_MARKER).then_some(index))
        .collect::<Vec<_>>();
    if markers.is_empty() {
        return Ok(None);
    }
    anyhow::ensure!(
        markers.len() == 1,
        "the legacy Oore PATH block in {} is ambiguous",
        path.display()
    );
    let marker = markers[0];
    anyhow::ensure!(
        lines
            .get(marker + 1)
            .is_some_and(|line| line_text(line) == expected_export),
        "the legacy Oore PATH block in {} is not the exact v0.1.41 two-line block",
        path.display()
    );

    let start_byte = lines[..marker].iter().map(|line| line.len()).sum::<usize>();
    let end_byte = lines[..=marker + 1]
        .iter()
        .map(|line| line.len())
        .sum::<usize>();
    let mut updated = String::with_capacity(contents.len() - (end_byte - start_byte));
    updated.push_str(&contents[..start_byte]);
    updated.push_str(&contents[end_byte..]);
    Ok(Some(updated))
}

fn remove_exact_path_block(
    contents: &str,
    expected_export: &str,
    path: &Path,
) -> anyhow::Result<Option<String>> {
    let starts = contents.matches(PATH_BLOCK_START).count();
    let ends = contents.matches(PATH_BLOCK_END).count();
    if starts == 0 && ends == 0 {
        return Ok(None);
    }
    anyhow::ensure!(
        starts == 1 && ends == 1,
        "the managed Oore PATH block in {} is ambiguous",
        path.display()
    );

    fn line_text(line: &str) -> &str {
        line.strip_suffix('\n').unwrap_or(line)
    }

    let lines = contents.split_inclusive('\n').collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| line_text(line) == PATH_BLOCK_START)
        .with_context(|| {
            format!(
                "the Oore PATH start marker is not on its own line in {}",
                path.display()
            )
        })?;
    let end = lines
        .iter()
        .position(|line| line_text(line) == PATH_BLOCK_END)
        .with_context(|| {
            format!(
                "the Oore PATH end marker is not on its own line in {}",
                path.display()
            )
        })?;
    anyhow::ensure!(
        end == start + 2
            && lines
                .get(start + 1)
                .is_some_and(|line| line_text(line) == expected_export),
        "the managed Oore PATH block in {} is not the exact recorded three-line block",
        path.display()
    );

    let start_byte = lines[..start].iter().map(|line| line.len()).sum::<usize>();
    let end_byte = lines[..=end].iter().map(|line| line.len()).sum::<usize>();
    let mut updated = String::with_capacity(contents.len() - (end_byte - start_byte));
    updated.push_str(&contents[..start_byte]);
    updated.push_str(&contents[end_byte..]);
    Ok(Some(updated))
}

fn escape_double_quoted(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\\' | '"' | '$' | '`') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn preflight_component_targets(
    root: &Path,
    components: &[InstallComponent],
) -> anyhow::Result<Vec<OwnedTarget>> {
    let root_dev = fs::symlink_metadata(root)?.dev();
    let mut paths = BTreeSet::from([
        "bin/oore",
        "VERSION",
        "CHANNEL",
        "GITHUB_REPO",
        "BOOTSTRAP_ARCHIVE",
        "BOOTSTRAP_SHA256",
        "BOOTSTRAP_MANIFEST_SHA256",
        "SHELL_PATH_FILE",
        "install-manifest.json",
    ]);
    let control_plane = components.contains(&InstallComponent::ControlPlane);
    let web = components.contains(&InstallComponent::Web);
    let runner = components.contains(&InstallComponent::Runner);
    if control_plane {
        paths.insert("bin/oored");
    }
    if web {
        paths.extend([
            "bin/oore-web",
            "web-dist",
            "WEB_VERSION",
            "WEB_CHANNEL",
            "WEB_GITHUB_REPO",
        ]);
    }
    if control_plane || web {
        paths.insert("LICENSE");
    }
    if runner {
        paths.insert("RUNNER_RELEASE");
    }

    let mut targets = Vec::new();
    for relative in paths {
        let relative = PathBuf::from(relative);
        validate_relative_path(&relative)?;
        validate_existing_ancestors(root, &relative)?;
        let path = root.join(&relative);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
            }
        };
        anyhow::ensure!(
            metadata.dev() == root_dev,
            "component path crosses a filesystem boundary: {}",
            path.display()
        );
        let directory = if metadata.file_type().is_dir() {
            validate_owned_tree(&path, root_dev)?;
            true
        } else {
            validate_owned_regular_metadata(&path, &metadata, "installed component")?;
            false
        };
        targets.push(OwnedTarget {
            relative,
            identity: identity(&metadata),
            directory,
            mutable_runtime_state: false,
        });
    }
    if runner {
        let relative = PathBuf::from("run/runner-service-ack.json");
        validate_relative_path(&relative)?;
        validate_existing_ancestors(root, &relative)?;
        let path = root.join(&relative);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                anyhow::ensure!(
                    metadata.dev() == root_dev,
                    "component path crosses a filesystem boundary: {}",
                    path.display()
                );
                validate_owned_regular_metadata(&path, &metadata, "runner acknowledgement")?;
                targets.push(OwnedTarget {
                    relative,
                    identity: identity(&metadata),
                    directory: false,
                    mutable_runtime_state: true,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
            }
        }
    }
    Ok(targets)
}

fn preflight_bootstrap_only(root: &Path) -> anyhow::Result<BootstrapInstall> {
    preflight_bootstrap_files(root)
}

fn preflight_bootstrap_files(root: &Path) -> anyhow::Result<BootstrapInstall> {
    let required = [
        "bin/oore",
        "VERSION",
        "CHANNEL",
        "GITHUB_REPO",
        "BOOTSTRAP_ARCHIVE",
        "BOOTSTRAP_SHA256",
    ];

    let version = read_bootstrap_metadata(root, "VERSION")?;
    semver::Version::parse(&version)
        .with_context(|| format!("bootstrap VERSION is not supported: {version}"))?;
    let channel = read_bootstrap_metadata(root, "CHANNEL")?;
    anyhow::ensure!(
        matches!(channel.as_str(), "stable" | "beta" | "alpha"),
        "bootstrap CHANNEL must be stable, beta, or alpha"
    );
    let repository = read_bootstrap_metadata(root, "GITHUB_REPO")?;
    validate_bootstrap_repository(&repository)?;
    let archive = read_bootstrap_metadata(root, "BOOTSTRAP_ARCHIVE")?;
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        other => anyhow::bail!("unsupported bootstrap architecture {other}"),
    };
    let cli_archive = format!("oore-cli_{version}_darwin_{arch}.tar.gz");
    let full_archive = format!("oore_{version}_darwin_{arch}.tar.gz");
    anyhow::ensure!(
        archive == cli_archive || archive == full_archive,
        "bootstrap archive does not match VERSION and architecture"
    );
    let sha256 = read_bootstrap_metadata(root, "BOOTSTRAP_SHA256")?;
    anyhow::ensure!(
        sha256.len() == 64
            && sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "bootstrap SHA-256 must contain 64 lowercase hexadecimal characters"
    );
    if path_exists(&root.join("BOOTSTRAP_MANIFEST_SHA256"))? {
        let manifest_sha256 = read_bootstrap_metadata(root, "BOOTSTRAP_MANIFEST_SHA256")?;
        anyhow::ensure!(
            manifest_sha256.len() == 64
                && manifest_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "bootstrap release manifest SHA-256 must contain 64 lowercase hexadecimal characters"
        );
    }

    let cli = root.join("bin/oore");
    let cli_metadata = owned_regular_file_metadata(&cli, "installed Oore CLI")?;
    let executable = std::env::current_exe().context("failed to identify the current Oore CLI")?;
    let executable_metadata = fs::metadata(&executable).with_context(|| {
        format!(
            "failed to inspect current executable {}",
            executable.display()
        )
    })?;
    anyhow::ensure!(
        same_identity(&cli_metadata, &executable_metadata),
        "bootstrap-only uninstall must run from {}",
        cli.display()
    );

    let shell_path_files = if path_exists(&root.join("SHELL_PATH_FILE"))? {
        let name = read_bootstrap_metadata(root, "SHELL_PATH_FILE")?;
        anyhow::ensure!(
            matches!(name.as_str(), ".zshrc" | ".bashrc" | ".bash_profile"),
            "unsupported bootstrap shell PATH file {name}"
        );
        vec![name]
    } else {
        Vec::new()
    };

    let root_dev = fs::symlink_metadata(root)?.dev();
    let mut targets = Vec::new();
    for relative in required
        .into_iter()
        .chain((!shell_path_files.is_empty()).then_some("SHELL_PATH_FILE"))
    {
        let relative = PathBuf::from(relative);
        validate_existing_ancestors(root, &relative)?;
        let path = root.join(&relative);
        let metadata = owned_regular_file_metadata(&path, "bootstrap file")?;
        anyhow::ensure!(
            metadata.dev() == root_dev,
            "bootstrap file crosses a filesystem boundary: {}",
            path.display()
        );
        targets.push(OwnedTarget {
            relative,
            identity: identity(&metadata),
            directory: false,
            mutable_runtime_state: false,
        });
    }
    Ok(BootstrapInstall {
        shell_path_files,
        targets,
    })
}

fn preflight_legacy_v0141(root: &Path) -> anyhow::Result<LegacyInstall> {
    let mode = detect_legacy_v0141_layout(root)?
        .context("--legacy-v0-1-41 requires an exact pre-manifest v0.1.41 installation")?;

    let has_control_plane = matches!(mode, LegacyInstallMode::All | LegacyInstallMode::Backend);
    let has_web = matches!(mode, LegacyInstallMode::All | LegacyInstallMode::Frontend);

    let expected_services = match mode {
        LegacyInstallMode::All => &[
            InstallService::Web,
            InstallService::Runner,
            InstallService::Daemon,
        ][..],
        LegacyInstallMode::Backend => &[InstallService::Runner, InstallService::Daemon][..],
        LegacyInstallMode::Frontend => &[InstallService::Web][..],
    };
    let mut services = Vec::new();
    for service in [
        InstallService::Web,
        InstallService::Runner,
        InstallService::Daemon,
    ] {
        let owned = legacy_v0141_service_is_owned(root, service).with_context(|| {
            format!(
                "cannot prove exact v0.1.41 ownership of service {}",
                service.label()
            )
        })?;
        if expected_services.contains(&service) {
            anyhow::ensure!(
                owned,
                "the service-managed v0.1.41 {} installation requires the exact {} definition",
                legacy_mode_label(mode),
                service.label()
            );
            services.push(service);
        } else {
            anyhow::ensure!(
                !owned,
                "the v0.1.41 {} installation does not own unexpected service {}",
                legacy_mode_label(mode),
                service.label()
            );
        }
    }

    let updater = legacy_v0141_updater_is_owned(root)
        .context("cannot prove exact v0.1.41 ownership of the updater service")?;
    if has_control_plane {
        anyhow::ensure!(
            updater,
            "the service-managed v0.1.41 {} installation requires the exact {UPDATER_SERVICE_LABEL} definition",
            legacy_mode_label(mode)
        );
    } else {
        anyhow::ensure!(
            !updater,
            "the v0.1.41 frontend installation does not own an updater service"
        );
    }

    let mut paths = BTreeSet::from(["INSTALL_MODE"]);
    if path_exists(&root.join("LICENSE"))? {
        paths.insert("LICENSE");
    }
    if has_control_plane {
        paths.extend(["bin/oore", "bin/oored", "VERSION", "CHANNEL", "GITHUB_REPO"]);
    }
    if has_web {
        paths.extend([
            "bin/oore-web",
            "web-dist",
            "WEB_VERSION",
            "WEB_CHANNEL",
            "WEB_GITHUB_REPO",
        ]);
    }
    let mut targets = Vec::new();
    for relative in paths {
        targets.push(preflight_owned_target(root, Path::new(relative))?);
    }

    Ok(LegacyInstall {
        mode,
        services,
        updater,
        shell_path_files: Vec::new(),
        targets,
    })
}

pub(crate) fn detect_legacy_v0141(root: &Path) -> anyhow::Result<bool> {
    if !path_exists(&root.join("INSTALL_MODE"))? {
        return Ok(false);
    }
    validate_install_root(root, false)?;
    Ok(detect_legacy_v0141_layout(root)?.is_some())
}

fn detect_legacy_v0141_layout(root: &Path) -> anyhow::Result<Option<LegacyInstallMode>> {
    anyhow::ensure!(
        cfg!(target_os = "macos"),
        "--legacy-v0-1-41 supports the exact macOS v0.1.41 service installation only"
    );
    if !path_exists(&root.join("INSTALL_MODE"))? {
        return Ok(None);
    }
    let mode = match read_bootstrap_metadata(root, "INSTALL_MODE")?.as_str() {
        "all" => LegacyInstallMode::All,
        "backend" => LegacyInstallMode::Backend,
        "frontend" => LegacyInstallMode::Frontend,
        other => anyhow::bail!(
            "the legacy INSTALL_MODE must be exactly all, backend, or frontend; found {other}"
        ),
    };

    let has_control_plane = matches!(mode, LegacyInstallMode::All | LegacyInstallMode::Backend);
    let has_web = matches!(mode, LegacyInstallMode::All | LegacyInstallMode::Frontend);
    ensure_legacy_paths_absent(
        root,
        &[
            "BOOTSTRAP_ARCHIVE",
            "BOOTSTRAP_SHA256",
            "BOOTSTRAP_MANIFEST_SHA256",
            "SHELL_PATH_FILE",
        ],
    )?;
    if has_control_plane {
        verify_legacy_daemon_payload(root)?;
        verify_legacy_runner_state()?;
    } else {
        ensure_legacy_paths_absent(
            root,
            &[
                "bin/oore",
                "bin/oored",
                "managed-runner.json",
                "VERSION",
                "CHANNEL",
                "GITHUB_REPO",
            ],
        )?;
    }
    if has_web {
        verify_legacy_web_payload(root, has_control_plane)?;
    } else {
        ensure_legacy_paths_absent(
            root,
            &[
                "bin/oore-web",
                "web-dist",
                "WEB_VERSION",
                "WEB_CHANNEL",
                "WEB_GITHUB_REPO",
            ],
        )?;
    }
    Ok(Some(mode))
}

fn ensure_legacy_paths_absent(root: &Path, paths: &[&str]) -> anyhow::Result<()> {
    for relative in paths {
        let path = root.join(relative);
        anyhow::ensure!(
            !path_exists(&path)?,
            "the v0.1.41 layout contains unexpected payload path {}",
            path.display()
        );
    }
    Ok(())
}

fn legacy_mode_label(mode: LegacyInstallMode) -> &'static str {
    match mode {
        LegacyInstallMode::All => "all",
        LegacyInstallMode::Backend => "backend",
        LegacyInstallMode::Frontend => "frontend",
    }
}

fn verify_legacy_daemon_payload(root: &Path) -> anyhow::Result<()> {
    let release_version = read_bootstrap_metadata(root, "VERSION")?;
    anyhow::ensure!(
        release_version == "0.1.41",
        "--legacy-v0-1-41 requires VERSION 0.1.41; found {release_version}"
    );
    let channel = read_bootstrap_metadata(root, "CHANNEL")?;
    anyhow::ensure!(
        matches!(channel.as_str(), "stable" | "beta" | "alpha"),
        "the v0.1.41 CHANNEL must be stable, beta, or alpha"
    );
    validate_bootstrap_repository(&read_bootstrap_metadata(root, "GITHUB_REPO")?)?;

    let cli = root.join("bin/oore");
    let cli_metadata = owned_regular_file_metadata(&cli, "v0.1.41 Oore CLI")?;
    anyhow::ensure!(
        cli_metadata.permissions().mode() & 0o111 != 0,
        "v0.1.41 Oore CLI is not executable: {}",
        cli.display()
    );
    let cli_status = Command::new(&cli)
        .arg("--help")
        .env("OORE_INSTALL_ROOT", root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .with_context(|| format!("failed to inspect legacy CLI {}", cli.display()))?;
    anyhow::ensure!(cli_status.success(), "v0.1.41 Oore CLI did not start");

    let path = root.join("bin/oored");
    let metadata = owned_regular_file_metadata(&path, "v0.1.41 control-plane executable")?;
    anyhow::ensure!(
        metadata.permissions().mode() & 0o111 != 0,
        "v0.1.41 control-plane executable is not executable: {}",
        path.display()
    );
    let output = Command::new(&path)
        .arg("package-version")
        .env_clear()
        .output()
        .with_context(|| format!("failed to inspect legacy daemon {}", path.display()))?;
    anyhow::ensure!(
        output.status.success(),
        "legacy daemon did not report its package version"
    );
    let version = String::from_utf8(output.stdout)
        .context("legacy daemon package version is not valid UTF-8")?;
    anyhow::ensure!(
        version.trim() == "0.1.10",
        "the v0.1.41 daemon must use compiled package version 0.1.10; found {}",
        version.trim()
    );
    Ok(())
}

fn verify_legacy_runner_state() -> anyhow::Result<()> {
    let path = dirs::home_dir()
        .context("could not determine the v0.1.41 runner account home")?
        .join(".oore/managed-runner.json");
    let metadata = owned_regular_file_metadata(&path, "v0.1.41 managed runner configuration")?;
    anyhow::ensure!(
        metadata.permissions().mode() & 0o077 == 0,
        "v0.1.41 managed runner configuration is readable by another account: {}",
        path.display()
    );
    Ok(())
}

fn verify_legacy_web_payload(root: &Path, compare_control_plane: bool) -> anyhow::Result<()> {
    let path = root.join("bin/oore-web");
    let metadata = owned_regular_file_metadata(&path, "v0.1.41 web executable")?;
    anyhow::ensure!(
        metadata.permissions().mode() & 0o111 != 0,
        "v0.1.41 web executable is not executable: {}",
        path.display()
    );
    let root_dev = fs::symlink_metadata(root)?.dev();
    validate_owned_tree(&root.join("web-dist"), root_dev)?;
    anyhow::ensure!(
        root.join("web-dist/index.html").is_file(),
        "v0.1.41 web payload is missing web-dist/index.html"
    );
    let version = read_bootstrap_metadata(root, "WEB_VERSION")?;
    anyhow::ensure!(
        version == "0.1.41",
        "--legacy-v0-1-41 requires WEB_VERSION 0.1.41; found {version}"
    );
    let channel = read_bootstrap_metadata(root, "WEB_CHANNEL")?;
    anyhow::ensure!(
        matches!(channel.as_str(), "stable" | "beta" | "alpha"),
        "the v0.1.41 WEB_CHANNEL must be stable, beta, or alpha"
    );
    let repository = read_bootstrap_metadata(root, "WEB_GITHUB_REPO")?;
    validate_bootstrap_repository(&repository)?;
    if compare_control_plane {
        anyhow::ensure!(
            channel == read_bootstrap_metadata(root, "CHANNEL")?,
            "the legacy web channel does not match the control-plane channel"
        );
        anyhow::ensure!(
            repository == read_bootstrap_metadata(root, "GITHUB_REPO")?,
            "the legacy web repository does not match the control-plane repository"
        );
    }
    Ok(())
}

fn preflight_owned_target(root: &Path, relative: &Path) -> anyhow::Result<OwnedTarget> {
    validate_relative_path(relative)?;
    validate_existing_ancestors(root, relative)?;
    let path = root.join(relative);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(error).with_context(|| {
                format!("required legacy payload is missing: {}", path.display())
            });
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    };
    let root_dev = fs::symlink_metadata(root)?.dev();
    anyhow::ensure!(
        metadata.dev() == root_dev,
        "component path crosses a filesystem boundary: {}",
        path.display()
    );
    let directory = if metadata.file_type().is_dir() {
        validate_owned_tree(&path, root_dev)?;
        true
    } else {
        validate_owned_regular_metadata(&path, &metadata, "installed component")?;
        false
    };
    Ok(OwnedTarget {
        relative: relative.to_path_buf(),
        identity: identity(&metadata),
        directory,
        mutable_runtime_state: false,
    })
}

fn read_bootstrap_metadata(root: &Path, name: &str) -> anyhow::Result<String> {
    let path = root.join(name);
    let metadata = owned_regular_file_metadata(&path, "bootstrap metadata")?;
    anyhow::ensure!(
        metadata.permissions().mode() & 0o022 == 0 && metadata.len() <= 4096,
        "bootstrap metadata has unsafe permissions or size: {}",
        path.display()
    );
    let mut file = open_owned_file(&path, &metadata, "bootstrap metadata")?;
    let mut value = String::new();
    file.read_to_string(&mut value)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let value = value.strip_suffix('\n').unwrap_or(&value);
    anyhow::ensure!(
        !value.is_empty() && !value.contains(['\n', '\r']) && value == value.trim(),
        "bootstrap metadata must contain one non-empty line: {}",
        path.display()
    );
    Ok(value.to_string())
}

fn validate_bootstrap_repository(repository: &str) -> anyhow::Result<()> {
    let mut segments = repository.split('/');
    let owner = segments.next().unwrap_or_default();
    let name = segments.next().unwrap_or_default();
    anyhow::ensure!(
        !owner.is_empty() && !name.is_empty() && segments.next().is_none(),
        "bootstrap repository must use owner/name format"
    );
    anyhow::ensure!(
        [owner, name].into_iter().all(|segment| segment
            .bytes()
            .all(|byte| { byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') })),
        "bootstrap repository contains unsupported characters"
    );
    Ok(())
}

fn preflight_purge_roots(install_root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let data_root = dirs::data_dir()
        .context("could not determine the platform data directory")?
        .join("oore");
    let mut roots = vec![install_root.to_path_buf()];
    if path_exists(&data_root)? {
        roots.push(data_root);
    }

    for root in &roots {
        validate_purge_root(root)?;
    }
    roots.sort_by_key(|path| path.components().count());
    let mut reduced = Vec::<PathBuf>::new();
    for root in roots {
        if reduced.iter().any(|parent| root.starts_with(parent)) {
            continue;
        }
        reduced.push(root);
    }
    Ok(reduced)
}

fn validate_purge_root(path: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(path.is_absolute(), "purge path must be absolute");
    let home = dirs::home_dir().context("could not determine the home directory")?;
    anyhow::ensure!(path != Path::new("/"), "refusing to purge /");
    anyhow::ensure!(path != home, "refusing to purge the home directory");
    let canonical = fs::canonicalize(path)
        .with_context(|| format!("failed to resolve purge path {}", path.display()))?;
    anyhow::ensure!(
        canonical == path,
        "purge path contains a symbolic link or non-canonical component: {}",
        path.display()
    );
    let current = std::env::current_dir().context("could not determine the current directory")?;
    anyhow::ensure!(
        current != path && !current.starts_with(path),
        "change out of {} before purging it",
        path.display()
    );
    let metadata = owned_directory_metadata(path, "purge root")?;
    validate_owned_tree(path, metadata.dev())
}

fn format_plan(plan: &RemovalPlan, purge: bool) -> String {
    let profile = match plan.installation {
        InstallationKind::Profile(InstallProfile::Complete) => "complete",
        InstallationKind::Profile(InstallProfile::ControlPlane) => "control-plane",
        InstallationKind::Profile(InstallProfile::Runner) => "runner",
        InstallationKind::Profile(InstallProfile::WebNode) => "web-node",
        InstallationKind::Profile(InstallProfile::CliOnly) => "cli-only",
        InstallationKind::InvalidManifest => "unknown (invalid installation manifest)",
        InstallationKind::BootstrapOnly => "bootstrap-only",
        InstallationKind::LegacyV0141(LegacyInstallMode::All) => "v0.1.41 all",
        InstallationKind::LegacyV0141(LegacyInstallMode::Backend) => "v0.1.41 backend",
        InstallationKind::LegacyV0141(LegacyInstallMode::Frontend) => "v0.1.41 frontend",
    };
    let mut lines = vec![
        format!("Install root  {}", plan.install_root.display()),
        format!("Device role   {profile}"),
        String::new(),
        "Remove".to_string(),
        format!(
            "  {} installed component path(s)",
            plan.component_targets.len()
        ),
        format!(
            "  {} managed service(s)",
            plan.services.len() + usize::from(plan.legacy_updater)
        ),
        format!("  {} verified shell PATH block(s)", plan.path_edits.len()),
    ];
    if purge {
        lines.push(String::new());
        lines.push("Purge".to_string());
        for root in &plan.purge_roots {
            lines.push(format!("  {}", root.display()));
        }
        lines.push("  All files in these validated directories will be deleted.".to_string());
    } else {
        lines.push(String::new());
        lines.push("Preserve".to_string());
        lines.push("  Oore data and every unlisted file".to_string());
        if matches!(plan.installation, InstallationKind::LegacyV0141(_)) {
            lines.push(
                "  Legacy logs, configuration, runner registration, and toolchains".to_string(),
            );
        }
    }
    if plan.updater_preserved {
        if purge {
            lines.push(String::new());
            lines.push("Preserve".to_string());
        }
        lines.push("  Unrecorded updater service (left unchanged)".to_string());
    }
    lines.join("\n")
}

fn confirm_removal(args: &UninstallArgs, terminal: Terminal) -> anyhow::Result<bool> {
    if args.yes {
        return Ok(true);
    }
    anyhow::ensure!(
        terminal.is_interactive(),
        "no interactive terminal was detected; review the plan and rerun with --yes"
    );
    let prompt = if args.purge {
        "Remove Oore and permanently delete the listed data?"
    } else {
        "Remove the listed Oore components?"
    };
    match terminal.confirm(prompt, false)? {
        PromptResult::Submitted(true) => Ok(true),
        PromptResult::Submitted(false) | PromptResult::Cancelled => Ok(false),
    }
}

fn apply_path_edits(edits: &mut [PathEdit]) -> anyhow::Result<()> {
    for index in 0..edits.len() {
        let result = {
            let edit = &mut edits[index];
            write_atomic_checked(
                &edit.path,
                &edit.updated,
                edit.mode,
                edit.identity,
                &edit.original,
            )
            .map(|identity| edit.published_identity = Some(identity))
        };
        if let Err(error) = result {
            return match rollback_path_edits(&mut edits[..index]) {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(error.context(format!(
                    "restoring earlier shell PATH edits also failed: {rollback_error:#}"
                ))),
            };
        }
    }
    Ok(())
}

fn rollback_path_edits(edits: &mut [PathEdit]) -> anyhow::Result<()> {
    let mut failures = Vec::new();
    for edit in edits.iter_mut().rev() {
        let Some(current) = edit.published_identity else {
            continue;
        };
        match write_atomic_checked(
            &edit.path,
            &edit.original,
            edit.mode,
            current,
            &edit.updated,
        ) {
            Ok(identity) => {
                edit.identity = identity;
                edit.published_identity = None;
            }
            Err(error) => failures.push(format!("{}: {error:#}", edit.path.display())),
        }
    }
    anyhow::ensure!(
        failures.is_empty(),
        "failed to restore {}",
        failures.join("; ")
    );
    Ok(())
}

fn write_atomic_checked(
    path: &Path,
    contents: &[u8],
    mode: u32,
    expected: FileIdentity,
    expected_contents: &[u8],
) -> anyhow::Result<FileIdentity> {
    require_file_contents(path, expected, expected_contents, "shell configuration")?;
    let parent = path
        .parent()
        .context("shell configuration has no parent directory")?;
    let mut staged = tempfile::NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "failed to stage shell configuration in {}",
            parent.display()
        )
    })?;
    staged
        .as_file()
        .set_permissions(fs::Permissions::from_mode(mode & 0o7777))
        .context("failed to preserve shell configuration permissions")?;
    staged.write_all(contents).with_context(|| {
        format!(
            "failed to write staged shell configuration for {}",
            path.display()
        )
    })?;
    staged.as_file_mut().sync_all().with_context(|| {
        format!(
            "failed to sync staged shell configuration for {}",
            path.display()
        )
    })?;
    require_file_contents(path, expected, expected_contents, "shell configuration")?;
    let published = staged
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to publish shell configuration {}", path.display()))?;
    published
        .sync_all()
        .with_context(|| format!("failed to sync shell configuration {}", path.display()))?;
    File::open(parent)
        .with_context(|| format!("failed to open {}", parent.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync {}", parent.display()))?;
    let metadata = published.metadata()?;
    validate_owned_regular_metadata(path, &metadata, "shell configuration")?;
    Ok(identity(&metadata))
}

fn require_file_contents(
    path: &Path,
    expected: FileIdentity,
    expected_contents: &[u8],
    description: &str,
) -> anyhow::Result<()> {
    let metadata = require_identity(path, expected, description)?;
    let mut file = open_owned_file(path, &metadata, description)?;
    let mut current = Vec::new();
    file.read_to_end(&mut current)
        .with_context(|| format!("failed to read {description} {}", path.display()))?;
    anyhow::ensure!(
        current == expected_contents,
        "{description} contents changed before removal: {}",
        path.display()
    );
    Ok(())
}

fn remove_services(
    root: &Path,
    services: &[InstallService],
    terminal: Terminal,
) -> anyhow::Result<()> {
    for service in services {
        let operation = terminal.operation(format!("Removing service {}", service.label()));
        match remove_service(root, *service) {
            Ok(()) => operation.done(format!("Removed service {}", service.label())),
            Err(error) => {
                operation.failed(format!("Could not remove service {}", service.label()));
                return Err(error).with_context(|| {
                    format!(
                        "service removal stopped at {}; earlier services can already be absent",
                        service.label()
                    )
                });
            }
        }
    }
    Ok(())
}

fn remove_legacy_updater(root: &Path, terminal: Terminal) -> anyhow::Result<()> {
    let operation = terminal.operation(format!("Removing service {UPDATER_SERVICE_LABEL}"));
    match remove_legacy_v0141_updater(root) {
        Ok(()) => {
            operation.done(format!("Removed service {UPDATER_SERVICE_LABEL}"));
            Ok(())
        }
        Err(error) => {
            operation.failed(format!("Could not remove service {UPDATER_SERVICE_LABEL}"));
            Err(error).context("legacy updater removal stopped")
        }
    }
}

fn remove_component_targets(
    plan: &RemovalPlan,
    terminal: Terminal,
) -> Result<(), ComponentRemovalFailure> {
    let operation = terminal.operation("Removing installed component files");
    let staging = tempfile::Builder::new()
        .prefix(".uninstall-")
        .tempdir_in(&plan.install_root)
        .context("failed to create the uninstall transaction directory")
        .map_err(|error| ComponentRemovalFailure {
            error,
            files_restored: true,
        })?;
    let staging_path = staging.path().to_path_buf();
    let mut moved = Vec::<PathBuf>::new();

    let staging_result = stage_component_targets(plan, &staging_path, &mut moved);
    if let Err(error) = staging_result {
        operation.failed("Component removal stopped");
        if let Err(rollback_error) =
            restore_moved_targets(&plan.install_root, &staging_path, &moved)
        {
            let preserved = staging.keep();
            return Err(ComponentRemovalFailure {
                error: error.context(format!(
                    "restoring component paths also failed: {rollback_error:#}; remaining staged files were preserved at {}",
                    preserved.display()
                )),
                files_restored: false,
            });
        }
        return Err(ComponentRemovalFailure {
            error,
            files_restored: true,
        });
    }

    if let Err(error) = staging.close() {
        operation.failed("Component files were staged but cleanup failed");
        return Err(ComponentRemovalFailure {
            error: anyhow::Error::new(error).context(format!(
                "installed paths were removed, but transaction files can remain under {}",
                staging_path.display()
            )),
            files_restored: false,
        });
    }
    operation.done("Installed component files removed");
    Ok(())
}

fn stage_component_targets(
    plan: &RemovalPlan,
    staging_path: &Path,
    moved: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    for target in &plan.component_targets {
        let source = plan.install_root.join(&target.relative);
        if !revalidate_target(&plan.install_root, target)? {
            continue;
        }
        let destination = staging_path.join(&target.relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to stage component path {}", source.display()))?;
        }
        fs::rename(&source, &destination)
            .with_context(|| format!("failed to stage component path {}", source.display()))?;
        moved.push(target.relative.clone());
        if target.mutable_runtime_state {
            let metadata = fs::symlink_metadata(&destination).with_context(|| {
                format!(
                    "failed to inspect staged runner acknowledgement {}",
                    destination.display()
                )
            })?;
            anyhow::ensure!(
                metadata.dev() == fs::symlink_metadata(&plan.install_root)?.dev(),
                "staged runner acknowledgement crosses a filesystem boundary: {}",
                destination.display()
            );
            validate_owned_regular_metadata(
                &destination,
                &metadata,
                "staged runner acknowledgement",
            )?;
        }
    }
    Ok(())
}

fn restore_moved_targets(root: &Path, staging: &Path, moved: &[PathBuf]) -> anyhow::Result<()> {
    let mut failures = Vec::new();
    for relative in moved.iter().rev() {
        let source = staging.join(relative);
        let destination = root.join(relative);
        if let Err(error) = fs::rename(&source, &destination) {
            failures.push(format!("{}: {error}", destination.display()));
        }
    }
    anyhow::ensure!(
        failures.is_empty(),
        "failed to restore component paths: {}",
        failures.join("; ")
    );
    Ok(())
}

fn purge(plan: &RemovalPlan, terminal: Terminal) -> anyhow::Result<()> {
    let operation = terminal.operation("Deleting Oore data");
    for path in purge_order(plan) {
        validate_purge_root(path)?;
        fs::remove_dir_all(path).with_context(|| {
            format!(
                "failed to purge {}; deletion can be partial",
                path.display()
            )
        })?;
    }
    operation.done("Oore files and data removed");
    Ok(())
}

fn purge_order(plan: &RemovalPlan) -> Vec<&Path> {
    let mut roots = plan
        .purge_roots
        .iter()
        .map(PathBuf::as_path)
        .collect::<Vec<_>>();
    roots.sort_by_key(|root| if *root == plan.install_root { 1 } else { 0 });
    roots
}

fn verify_services_absent(root: &Path, services: &[InstallService]) -> anyhow::Result<()> {
    for service in services {
        anyhow::ensure!(
            !service_is_owned(root, *service).with_context(|| format!(
                "could not verify removal of service {}",
                service.label()
            ))?,
            "service {} still exists after removal",
            service.label()
        );
    }
    Ok(())
}

fn verify_legacy_updater_absent(root: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        !legacy_v0141_updater_is_owned(root)
            .context("could not verify removal of the legacy updater")?,
        "legacy updater still exists after removal"
    );
    Ok(())
}

fn verify_path_edits(edits: &[PathEdit]) -> anyhow::Result<()> {
    for edit in edits {
        let expected = edit
            .published_identity
            .context("shell PATH edit was not published")?;
        let metadata = require_identity(&edit.path, expected, "shell configuration")?;
        let mut file = open_owned_file(&edit.path, &metadata, "shell configuration")?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        anyhow::ensure!(
            contents == edit.updated,
            "shell configuration changed before uninstall verification: {}",
            edit.path.display()
        );
    }
    Ok(())
}

fn verify_paths_absent(paths: &[PathBuf]) -> anyhow::Result<()> {
    for path in paths {
        anyhow::ensure!(
            !path_exists(path)?,
            "purge path still exists after removal: {}",
            path.display()
        );
    }
    Ok(())
}

fn verify_targets_absent(root: &Path, targets: &[OwnedTarget]) -> anyhow::Result<()> {
    for target in targets {
        let path = root.join(&target.relative);
        anyhow::ensure!(
            !path_exists(&path)?,
            "installed component path still exists after removal: {}",
            path.display()
        );
    }
    Ok(())
}

fn prune_empty_component_directories(root: &Path) -> anyhow::Result<()> {
    for relative in ["run", "libexec", "bin"] {
        let path = root.join(relative);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
            }
        };
        anyhow::ensure!(
            metadata.file_type().is_dir() && metadata.uid() == current_effective_uid(),
            "refusing to prune unsafe component directory {}",
            path.display()
        );
        match fs::remove_dir(&path) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::NotFound
                ) => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to prune empty directory {}", path.display())
                });
            }
        }
    }
    let metadata = owned_directory_metadata(root, "install root")?;
    anyhow::ensure!(
        metadata.permissions().mode() & 0o022 == 0,
        "refusing to prune an unsafe install root: {}",
        root.display()
    );
    match fs::remove_dir(root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to prune empty install root {}", root.display()));
        }
    }
    Ok(())
}

fn revalidate_target(root: &Path, target: &OwnedTarget) -> anyhow::Result<bool> {
    validate_existing_ancestors(root, &target.relative)?;
    let path = root.join(&target.relative);
    if target.mutable_runtime_state {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
            }
        };
        anyhow::ensure!(
            metadata.dev() == fs::symlink_metadata(root)?.dev(),
            "runner acknowledgement crosses a filesystem boundary: {}",
            path.display()
        );
        validate_owned_regular_metadata(&path, &metadata, "runner acknowledgement")?;
        return Ok(true);
    }
    let metadata = require_identity(&path, target.identity, "installed component")?;
    anyhow::ensure!(
        metadata.file_type().is_dir() == target.directory,
        "component path changed type before removal: {}",
        path.display()
    );
    if target.directory {
        validate_owned_tree(&path, fs::symlink_metadata(root)?.dev())?;
    } else {
        validate_owned_regular_metadata(&path, &metadata, "installed component")?;
    }
    Ok(true)
}

fn validate_relative_path(path: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "invalid installed component path {}",
        path.display()
    );
    Ok(())
}

fn validate_existing_ancestors(root: &Path, relative: &Path) -> anyhow::Result<()> {
    let mut current = root.to_path_buf();
    let count = relative.components().count();
    for component in relative.components().take(count.saturating_sub(1)) {
        let Component::Normal(name) = component else {
            anyhow::bail!("invalid installed component path {}", relative.display());
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                anyhow::ensure!(
                    metadata.file_type().is_dir() && metadata.uid() == current_effective_uid(),
                    "component parent is not an owned directory: {}",
                    current.display()
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", current.display()));
            }
        }
    }
    Ok(())
}

fn validate_owned_tree(root: &Path, expected_dev: u64) -> anyhow::Result<()> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        anyhow::ensure!(
            metadata.uid() == current_effective_uid(),
            "purge or component tree contains a path owned by another user: {}",
            path.display()
        );
        anyhow::ensure!(
            metadata.permissions().mode() & 0o022 == 0,
            "purge or component tree contains a path writable by another user: {}",
            path.display()
        );
        anyhow::ensure!(
            metadata.dev() == expected_dev,
            "purge or component tree crosses a filesystem boundary: {}",
            path.display()
        );
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            for entry in
                fs::read_dir(&path).with_context(|| format!("failed to read {}", path.display()))?
            {
                pending.push(entry.context("failed to read directory entry")?.path());
            }
        } else if file_type.is_file() {
            anyhow::ensure!(
                metadata.nlink() == 1,
                "purge or component tree contains a hard-linked file: {}",
                path.display()
            );
        } else {
            anyhow::ensure!(
                file_type.is_socket(),
                "purge or component tree contains a symbolic link or special entry: {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn owned_directory_metadata(path: &Path, description: &str) -> anyhow::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {description} {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_dir(),
        "{description} is not a real directory: {}",
        path.display()
    );
    anyhow::ensure!(
        metadata.uid() == current_effective_uid(),
        "{description} is owned by another user: {}",
        path.display()
    );
    Ok(metadata)
}

fn owned_regular_file_metadata(path: &Path, description: &str) -> anyhow::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {description} {}", path.display()))?;
    validate_owned_regular_metadata(path, &metadata, description)?;
    Ok(metadata)
}

fn validate_owned_regular_metadata(
    path: &Path,
    metadata: &fs::Metadata,
    description: &str,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        metadata.file_type().is_file(),
        "{description} is not a regular file: {}",
        path.display()
    );
    anyhow::ensure!(
        metadata.uid() == current_effective_uid(),
        "{description} is owned by another user: {}",
        path.display()
    );
    anyhow::ensure!(
        metadata.nlink() == 1,
        "{description} is hard-linked: {}",
        path.display()
    );
    Ok(())
}

fn open_owned_file(
    path: &Path,
    path_metadata: &fs::Metadata,
    description: &str,
) -> anyhow::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("failed to open {description} {}", path.display()))?;
    let opened = file
        .metadata()
        .with_context(|| format!("failed to inspect open {description} {}", path.display()))?;
    validate_owned_regular_metadata(path, &opened, description)?;
    anyhow::ensure!(
        same_identity(path_metadata, &opened),
        "{description} changed while it was opened: {}",
        path.display()
    );
    Ok(file)
}

fn validate_writable_parent(path: &Path) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .context("shell configuration has no parent directory")?;
    let metadata = owned_directory_metadata(parent, "shell configuration directory")?;
    anyhow::ensure!(
        metadata.permissions().mode() & 0o022 == 0,
        "shell configuration directory is writable by another user: {}",
        parent.display()
    );
    Ok(())
}

fn require_identity(
    path: &Path,
    expected: FileIdentity,
    description: &str,
) -> anyhow::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {description} {}", path.display()))?;
    anyhow::ensure!(
        metadata.dev() == expected.dev && metadata.ino() == expected.ino,
        "{description} changed before removal: {}",
        path.display()
    );
    Ok(metadata)
}

fn identity(metadata: &fs::Metadata) -> FileIdentity {
    FileIdentity {
        dev: metadata.dev(),
        ino: metadata.ino(),
    }
}

fn same_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

fn updater_service_present() -> anyhow::Result<bool> {
    let system_definition =
        Path::new("/Library/LaunchDaemons").join(format!("{UPDATER_SERVICE_LABEL}.plist"));
    if path_exists(&system_definition)? {
        return Ok(true);
    }
    if let Some(home) = dirs::home_dir() {
        let user_definition = home
            .join("Library/LaunchAgents")
            .join(format!("{UPDATER_SERVICE_LABEL}.plist"));
        if path_exists(&user_definition)? {
            return Ok(true);
        }
    }
    if !cfg!(target_os = "macos") {
        return Ok(false);
    }
    if launchd_job_loaded(&format!("system/{UPDATER_SERVICE_LABEL}"))? {
        return Ok(true);
    }
    launchd_job_loaded(&format!(
        "gui/{}/{}",
        current_effective_uid(),
        UPDATER_SERVICE_LABEL
    ))
}

fn launchd_job_loaded(service: &str) -> anyhow::Result<bool> {
    let output = Command::new("/bin/launchctl")
        .args(["print", service])
        .output()
        .with_context(|| format!("failed to inspect launchd service {service}"))?;
    Ok(output.status.success())
}

fn current_effective_uid() -> u32 {
    // SAFETY: `geteuid` has no arguments, pointer requirements, or failure state.
    unsafe { libc::geteuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install_root() -> (tempfile::TempDir, PathBuf) {
        let temporary = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap().join("oore");
        fs::create_dir_all(root.join("bin")).unwrap();
        for directory in [root.parent().unwrap(), &root, &root.join("bin")] {
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let cli = root.join("bin/oore");
        fs::write(&cli, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
        (temporary, root)
    }

    fn error_text(result: anyhow::Result<impl std::fmt::Debug>) -> String {
        format!("{:#}", result.unwrap_err())
    }

    #[test]
    fn install_root_text_must_be_a_plain_absolute_path() {
        assert_eq!(
            validate_install_root_text("/opt/oore").unwrap(),
            "/opt/oore"
        );
        assert!(error_text(validate_install_root_text("")).contains("cannot be empty"));
        assert!(error_text(validate_install_root_text("oore")).contains("absolute path"));
        assert!(error_text(validate_install_root_text("/opt/a:b")).contains("colon"));
        assert!(error_text(validate_install_root_text("/opt/a\nb")).contains("newline"));
    }

    #[test]
    fn uninstall_refuses_unsafe_install_roots() {
        let (_temporary, root) = install_root();
        validate_install_root(&root, true).unwrap();

        assert!(error_text(validate_install_root(Path::new("/"), false)).contains("refusing"));
        let home = dirs::home_dir().unwrap();
        assert!(error_text(validate_install_root(&home, false)).contains("home directory"));
        assert!(
            error_text(validate_install_root(&root.join("bin/../bin"), false))
                .contains("unsupported path component")
        );

        let link = root.parent().unwrap().join("linked");
        std::os::unix::fs::symlink(&root, &link).unwrap();
        assert!(error_text(validate_install_root(&link, false)).contains("not a real directory"));

        fs::set_permissions(&root, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(error_text(validate_install_root(&root, false)).contains("writable by another"));
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();

        fs::set_permissions(root.join("bin/oore"), fs::Permissions::from_mode(0o644)).unwrap();
        validate_install_root(&root, false).unwrap();
        assert!(error_text(validate_install_root(&root, true)).contains("not executable"));
        fs::remove_file(root.join("bin/oore")).unwrap();
        assert!(validate_install_root(&root, true).is_err());
    }

    #[test]
    fn uninstall_waits_for_pending_or_running_updates() {
        let (_temporary, root) = install_root();
        refuse_update_state(&root).unwrap();

        let write_status = |phase| {
            let status = RuntimeUpdateStatus {
                phase,
                error: None,
                managed_service: true,
            };
            let path = root.join(".runtime-update-status.json");
            fs::write(&path, serde_json::to_vec(&status).unwrap()).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        };
        write_status(RuntimeUpdatePhase::Idle);
        refuse_update_state(&root).unwrap();
        write_status(RuntimeUpdatePhase::Failed);
        refuse_update_state(&root).unwrap();
        write_status(RuntimeUpdatePhase::Updating);
        assert!(error_text(refuse_update_state(&root)).contains("runtime update is active"));
        write_status(RuntimeUpdatePhase::Restarting);
        assert!(error_text(refuse_update_state(&root)).contains("runtime update is active"));
        fs::write(root.join(".runtime-update-status.json"), b"{").unwrap();
        assert!(error_text(refuse_update_state(&root)).contains("invalid runtime update status"));
        fs::remove_file(root.join(".runtime-update-status.json")).unwrap();

        let request = root.join("run/runtime-update-queue/request.json");
        fs::create_dir_all(request.parent().unwrap()).unwrap();
        fs::write(&request, b"{}").unwrap();
        assert!(error_text(refuse_update_state(&root)).contains("runtime update state exists"));
        fs::remove_file(&request).unwrap();

        for transaction in [
            root.join(".update-1234"),
            root.join(".supervised-update-1234"),
            root.join("run/runtime-update-active-1234"),
        ] {
            fs::create_dir(&transaction).unwrap();
            assert!(
                error_text(refuse_update_state(&root)).contains("update transaction state exists")
            );
            fs::remove_dir(&transaction).unwrap();
        }
        refuse_update_state(&root).unwrap();
    }

    #[test]
    fn only_the_exact_managed_path_block_is_removed() {
        let path = Path::new("/home/user/.zshrc");
        let export = "export PATH=\"/home/user/.oore/bin:$PATH\"";
        let block = format!("{PATH_BLOCK_START}\n{export}\n{PATH_BLOCK_END}\n");

        assert_eq!(
            remove_exact_path_block("alias ll='ls -l'\n", export, path).unwrap(),
            None
        );
        assert_eq!(
            remove_exact_path_block(&format!("before\n{block}after\n"), export, path).unwrap(),
            Some("before\nafter\n".to_string())
        );
        assert_eq!(
            remove_exact_path_block(block.trim_end(), export, path).unwrap(),
            Some(String::new())
        );

        for contents in [
            format!("{block}{block}"),
            format!("{PATH_BLOCK_START}\n{export}\n"),
            format!("{PATH_BLOCK_START}\nexport PATH=\"/elsewhere:$PATH\"\n{PATH_BLOCK_END}\n"),
            format!("{PATH_BLOCK_START}\n{export}\necho extra\n{PATH_BLOCK_END}\n"),
            format!("# {PATH_BLOCK_START}\n{export}\n{PATH_BLOCK_END}\n"),
        ] {
            assert!(
                remove_exact_path_block(&contents, export, path).is_err(),
                "{contents:?} was edited"
            );
        }
    }

    #[test]
    fn shell_paths_are_escaped_for_double_quotes() {
        assert_eq!(
            escape_double_quoted(r#"/Users/a "b"/$HOME/`x`\y"#),
            r#"/Users/a \"b\"/\$HOME/\`x\`\\y"#
        );
    }

    #[test]
    fn purge_refuses_roots_it_must_never_delete() {
        assert!(error_text(validate_purge_root(Path::new("relative"))).contains("absolute"));
        assert!(error_text(validate_purge_root(Path::new("/"))).contains("refusing to purge /"));
        let home = dirs::home_dir().unwrap();
        assert!(error_text(validate_purge_root(&home)).contains("home directory"));

        let (_temporary, root) = install_root();
        let link = root.parent().unwrap().join("linked");
        std::os::unix::fs::symlink(&root, &link).unwrap();
        assert!(error_text(validate_purge_root(&link)).contains("symbolic link"));
        validate_purge_root(&root).unwrap();
    }
}
