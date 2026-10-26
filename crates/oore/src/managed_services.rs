use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use anyhow::Context;
use serde_json::Value;
use url::Url;

use crate::install_manifest::InstallService;
use crate::{launchd_xml_escape as xml_escape, paths_refer_to_same_file};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(500);
const SERVICE_PATH: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";
const LEGACY_UPDATER_LABEL: &str = "build.oore.oore-updater";
const UPDATE_REQUEST_FILE: &str = "run/runtime-update-queue/request.json";
const RECOVERY_SUDOERS: &str = "/etc/sudoers.d/oore-update-recovery";

#[derive(Debug)]
struct ServiceOwner {
    uid: u32,
    name: String,
    home: PathBuf,
}

#[derive(Debug)]
struct ServiceDefinition {
    service: InstallService,
    root: PathBuf,
    owner: ServiceOwner,
    arguments: Vec<String>,
    log_path: PathBuf,
    state_parent: Option<PathBuf>,
}

#[derive(Debug)]
struct DefinitionSnapshot {
    contents: Vec<u8>,
    mode: u32,
}

#[derive(Debug)]
struct LegacyWebSnapshot {
    path: PathBuf,
    definition: DefinitionSnapshot,
    was_loaded: bool,
    owner_uid: u32,
}

#[derive(Debug)]
struct ServiceSnapshot {
    definition: Option<DefinitionSnapshot>,
    was_loaded: bool,
    legacy_web: Option<LegacyWebSnapshot>,
}

#[derive(Debug)]
struct LegacyUpdaterSnapshot {
    definition: DefinitionSnapshot,
    was_loaded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreparedPathKind {
    Directory,
    File,
}

#[derive(Debug)]
struct PreparedPathSnapshot {
    path: PathBuf,
    kind: PreparedPathKind,
    existed: bool,
    mode: Option<u32>,
}

#[derive(Debug)]
struct ServicePathSnapshot {
    paths: Vec<PreparedPathSnapshot>,
}

#[derive(Debug)]
enum LaunchdJobState {
    Absent,
    Loaded {
        program: PathBuf,
        running: bool,
        pid: Option<u32>,
    },
}

pub(crate) async fn install_daemon(
    root: &Path,
    listen: &str,
    state_file: &Path,
) -> anyhow::Result<()> {
    require_macos()?;
    let root = validate_install_root(root)?;
    let owner = service_owner(&root)?;
    let listen_address = parse_listen_address(listen)?;
    anyhow::ensure!(
        state_file.is_absolute(),
        "daemon state file must be absolute"
    );
    if let Some(parent) = state_file.parent() {
        validate_user_directory_preflight(parent, owner.uid, false)?;
    }

    let executable = validate_installed_executable(&root, owner.uid, "oored")?;
    verify_daemon_candidate(&executable)?;
    let log_path = service_log_path(&root, InstallService::Daemon);
    validate_log_path_preflight(&root, owner.uid, InstallService::Daemon)?;
    let definition = ServiceDefinition {
        service: InstallService::Daemon,
        root,
        owner,
        arguments: vec![
            executable.display().to_string(),
            "run".to_string(),
            "--listen".to_string(),
            listen_address.to_string(),
            "--state-file".to_string(),
            state_file.display().to_string(),
        ],
        log_path,
        state_parent: state_file.parent().map(Path::to_path_buf),
    };
    install_service(definition).await
}

pub(crate) async fn install_update_supervisor(root: &Path) -> anyhow::Result<()> {
    require_macos()?;
    let root = validate_install_root(root)?;
    let owner = service_owner(&root)?;
    validate_installed_executable(&root, owner.uid, "oore")?;
    validate_log_path_preflight(&root, owner.uid, InstallService::Updater)?;
    let user = owner.name.clone();
    install_service(update_supervisor_definition(root, owner)).await?;
    install_recovery_sudoers(&user)
        .context("one-click updates are installed, but granting the rollback rule failed")
}

/// Lets the update supervisor, which runs as the service account without a
/// terminal, reload `oored` when an update fails mid restart. It names the
/// three exact `launchctl` command lines the rollback runs, and nothing else.
fn render_recovery_sudoers(user: &str) -> anyhow::Result<String> {
    anyhow::ensure!(
        !user.is_empty()
            && user
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')),
        "service account name {user:?} cannot be written to a sudoers rule"
    );
    let label = InstallService::Daemon.label();
    let plist = InstallService::Daemon.definition();
    Ok(format!(
        "# Managed by Oore. Removed by `oore uninstall`.\n\
         # Lets the update supervisor reload oored when an update fails mid restart.\n\
         {user} ALL=(root) NOPASSWD: /bin/launchctl bootout system/{label}, \
         /bin/launchctl bootstrap system {plist}, \
         /bin/launchctl kickstart -k system/{label}\n",
        plist = plist.display()
    ))
}

fn install_recovery_sudoers(user: &str) -> anyhow::Result<()> {
    let contents = render_recovery_sudoers(user)?;
    let target = Path::new(RECOVERY_SUDOERS);
    if read_system_definition_bytes(target).is_ok_and(|current| current == contents.as_bytes()) {
        return Ok(());
    }
    let mut source = tempfile::NamedTempFile::new().context("failed to stage the sudoers rule")?;
    source
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    source.write_all(contents.as_bytes())?;
    source.as_file_mut().sync_all()?;
    // sudo skips sudoers.d files whose names contain a dot, so the stage stays
    // inert until the final rename.
    let stage = Path::new("/etc/sudoers.d").join(format!(
        ".oore-update-recovery.{}",
        uuid::Uuid::new_v4().simple()
    ));
    let publish = (|| -> anyhow::Result<()> {
        privileged_checked(
            "/usr/bin/install",
            &[
                OsStr::new("-o"),
                OsStr::new("root"),
                OsStr::new("-g"),
                OsStr::new("wheel"),
                OsStr::new("-m"),
                OsStr::new("0440"),
                source.path().as_os_str(),
                stage.as_os_str(),
            ],
            "staging the sudoers rule",
        )?;
        privileged_checked(
            "/usr/sbin/visudo",
            &[OsStr::new("-c"), OsStr::new("-f"), stage.as_os_str()],
            "validating the sudoers rule",
        )?;
        privileged_checked(
            "/bin/mv",
            &[OsStr::new("-f"), stage.as_os_str(), target.as_os_str()],
            "publishing the sudoers rule",
        )
    })();
    if publish.is_err() {
        let _ = privileged_output("/bin/rm", &[OsStr::new("-f"), stage.as_os_str()]);
    }
    publish
}

fn remove_recovery_sudoers() -> anyhow::Result<()> {
    if fs::symlink_metadata(RECOVERY_SUDOERS).is_err() {
        return Ok(());
    }
    privileged_checked(
        "/bin/rm",
        &[OsStr::new("-f"), OsStr::new(RECOVERY_SUDOERS)],
        "removing the sudoers rule",
    )
}

fn update_supervisor_definition(root: PathBuf, owner: ServiceOwner) -> ServiceDefinition {
    let request = root.join(UPDATE_REQUEST_FILE);
    ServiceDefinition {
        service: InstallService::Updater,
        arguments: vec![
            root.join("bin/oore").display().to_string(),
            "update-supervisor".to_string(),
            "--request-file".to_string(),
            request.display().to_string(),
        ],
        log_path: root.join("logs/update-supervisor.log"),
        state_parent: request.parent().map(Path::to_path_buf),
        root,
        owner,
    }
}

pub(crate) fn daemon_configuration(root: &Path) -> anyhow::Result<Option<(String, PathBuf)>> {
    require_macos()?;
    let root = validate_install_root(root)?;
    let definition = InstallService::Daemon.definition();
    if let Ok(metadata) = fs::symlink_metadata(&definition)
        && metadata.permissions().mode() & 0o777 == 0o600
        && matches!(
            OpenOptions::new().read(true).open(&definition),
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied
        )
    {
        require_privileged_access().context(
            "administrator authorization is required to inspect the legacy daemon configuration; run `sudo -v`, then retry",
        )?;
    }
    if !service_is_owned(&root, InstallService::Daemon)? {
        return Ok(None);
    }

    let document = read_plist_document(&definition)?;
    if validate_owned_definition(&root, InstallService::Daemon).is_ok() {
        let arguments = plist_arguments(&document)?;
        let listen = option_value(&arguments, "--listen")
            .context("managed daemon service has no listen address")?;
        let state_file = option_value(&arguments, "--state-file")
            .map(PathBuf::from)
            .context("managed daemon service has no state file")?;
        return Ok(Some((listen.to_string(), state_file)));
    }

    let owner = service_owner(&root)?;
    validate_legacy_daemon_definition(&root, &owner)?;
    let arguments = plist_arguments(&document)?;
    let state_file = owner.home.join("Library/Application Support/oore/oore.db");
    Ok(Some((arguments[3].clone(), state_file)))
}

pub(crate) async fn install_web(
    root: &Path,
    listen: &str,
    backend_url: &str,
    browser_transport_protected: bool,
    backend_transport_protected: bool,
) -> anyhow::Result<()> {
    require_macos()?;
    let root = validate_install_root(root)?;
    let owner = service_owner(&root)?;
    validate_web_transport(
        listen,
        backend_url,
        browser_transport_protected,
        backend_transport_protected,
    )?;

    let executable = validate_installed_executable(&root, owner.uid, "oore-web")?;
    let dist_dir = root.join("web-dist");
    anyhow::ensure!(
        dist_dir.join("index.html").is_file(),
        "web assets are missing from {}",
        dist_dir.display()
    );
    let mut arguments = vec![
        executable.display().to_string(),
        "serve".to_string(),
        "--listen".to_string(),
        listen.to_string(),
        "--backend-url".to_string(),
        backend_url.to_string(),
        "--dist-dir".to_string(),
        dist_dir.display().to_string(),
    ];
    add_web_pairing_arguments(&root, owner.uid, &mut arguments)?;
    if browser_transport_protected {
        arguments.push("--browser-transport-protected".to_string());
    }
    if backend_transport_protected {
        arguments.push("--backend-transport-protected".to_string());
    }
    verify_web_candidate(&root, &owner, &arguments)?;

    let log_path = service_log_path(&root, InstallService::Web);
    validate_log_path_preflight(&root, owner.uid, InstallService::Web)?;
    let definition = ServiceDefinition {
        service: InstallService::Web,
        root,
        owner,
        arguments,
        log_path,
        state_parent: None,
    };
    install_service(definition).await
}

pub(crate) async fn verify_service(root: &Path, service: InstallService) -> anyhow::Result<()> {
    require_macos()?;
    let root = validate_install_root(root)?;
    anyhow::ensure!(
        service_is_owned(&root, service)?,
        "{} is not installed",
        service.label()
    );
    validate_owned_definition(&root, service).with_context(|| {
        format!(
            "{} still uses a legacy service definition and requires migration",
            service.label()
        )
    })?;
    let document = read_plist_document(&service.definition())?;
    let executable = expected_launchd_program(&root, service);
    if service == InstallService::Updater {
        return match launchd_job_state(service)? {
            LaunchdJobState::Loaded { program, .. }
                if paths_refer_to_same_file(&program, &executable) =>
            {
                Ok(())
            }
            LaunchdJobState::Loaded { program, .. } => anyhow::bail!(
                "loaded {} job uses unexpected executable {}",
                service.label(),
                program.display()
            ),
            LaunchdJobState::Absent => {
                anyhow::bail!("launchd does not have {} loaded", service.label())
            }
        };
    }
    let deadline = Instant::now() + HEALTH_TIMEOUT;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .context("failed to create the local service health client")?;
    let mut last_error = "the service did not start".to_string();

    while Instant::now() < deadline {
        match launchd_job_state(service)? {
            LaunchdJobState::Absent => {
                last_error = "launchd does not have the service loaded".to_string();
            }
            LaunchdJobState::Loaded {
                program,
                running,
                pid,
            } => {
                if !paths_refer_to_same_file(&program, &executable) {
                    anyhow::bail!(
                        "loaded {} job uses unexpected executable {}",
                        service.label(),
                        program.display()
                    );
                }
                if !running || pid.is_none() {
                    last_error = "launchd loaded the service, but it is not running".to_string();
                } else {
                    match verify_endpoint_or_runner(&client, &root, service, &document, pid).await {
                        Ok(()) => return Ok(()),
                        Err(error) => last_error = format!("{error:#}"),
                    }
                }
            }
        }
        std::thread::sleep(HEALTH_POLL_INTERVAL);
    }

    anyhow::bail!(
        "{} did not become healthy within {} seconds: {last_error}",
        service.label(),
        HEALTH_TIMEOUT.as_secs()
    )
}

pub(crate) fn remove_service(root: &Path, service: InstallService) -> anyhow::Result<()> {
    require_macos()?;
    let root = validate_install_root(root)?;
    if !service_is_owned(&root, service)? {
        return Ok(());
    }
    if service == InstallService::Updater {
        require_privileged_access()?;
        remove_recovery_sudoers()?;
    }
    let snapshot = capture_snapshot(&root, service)?;
    if snapshot.definition.is_some() {
        require_privileged_access()?;
    }
    ensure_snapshot_unchanged(&root, service, &snapshot)?;
    let removal = (|| -> anyhow::Result<()> {
        if snapshot.definition.is_some() {
            stop_service(service)?;
        }
        if let Some(legacy) = &snapshot.legacy_web {
            if legacy.was_loaded {
                stop_user_service(legacy.owner_uid, service)?;
            }
            remove_legacy_web_definition(legacy)?;
        }
        if snapshot.definition.is_some() {
            remove_definition(service)?;
        }
        anyhow::ensure!(
            !service.definition().exists(),
            "failed to remove {}",
            service.definition().display()
        );
        if let Some(legacy) = &snapshot.legacy_web {
            anyhow::ensure!(
                !legacy.path.exists(),
                "failed to remove {}",
                legacy.path.display()
            );
        }
        Ok(())
    })();
    if let Err(error) = removal {
        let rollback = if snapshot.definition.is_none() && snapshot.legacy_web.is_some() {
            restore_legacy_web_snapshot(service, &snapshot)
        } else {
            restore_snapshot(service, &snapshot)
        };
        if let Err(rollback_error) = rollback {
            anyhow::bail!(
                "{error:#}; restoring the previous service also failed: {rollback_error:#}"
            );
        }
        return Err(error);
    }
    Ok(())
}

fn restore_legacy_web_snapshot(
    service: InstallService,
    snapshot: &ServiceSnapshot,
) -> anyhow::Result<()> {
    let legacy = snapshot
        .legacy_web
        .as_ref()
        .context("legacy web rollback has no user service snapshot")?;
    restore_user_definition(legacy)?;
    if legacy.was_loaded {
        start_user_service(legacy.owner_uid, service, &legacy.path)?;
        wait_for_user_service_running(legacy.owner_uid, service)?;
    }
    Ok(())
}

pub(crate) fn legacy_v0141_service_is_owned(
    root: &Path,
    service: InstallService,
) -> anyhow::Result<bool> {
    require_macos()?;
    let root = validate_install_root(root)?;
    let owner = service_owner(&root)?;

    if service == InstallService::Web {
        let definition_exists = service
            .definition()
            .try_exists()
            .with_context(|| format!("failed to inspect {}", service.definition().display()))?;
        let system_job = launchd_job_state(service)?;
        anyhow::ensure!(
            !definition_exists && matches!(system_job, LaunchdJobState::Absent),
            "the v0.1.41 web service must use its exact login-session definition"
        );
        return Ok(capture_legacy_web_snapshot(&root, &owner)?.is_some());
    }

    if service == InstallService::Daemon {
        reject_legacy_user_daemon(&owner)?;
    } else {
        reject_legacy_user_runner(&owner)?;
    }

    let definition = service.definition();
    let definition_exists = definition
        .try_exists()
        .with_context(|| format!("failed to inspect {}", definition.display()))?;
    let job = launchd_job_state(service)?;
    if !definition_exists {
        return match job {
            LaunchdJobState::Absent => Ok(false),
            LaunchdJobState::Loaded { .. } => anyhow::bail!(
                "{} is loaded without its service definition; refusing an ambiguous operation",
                service.label()
            ),
        };
    }

    let expected_program = match service {
        InstallService::Daemon => {
            validate_legacy_daemon_definition(&root, &owner)?;
            root.join("bin/oored")
        }
        InstallService::Updater => {
            validate_legacy_updater_definition(&root, &owner)?;
            root.join("bin/oore")
        }
        InstallService::Runner => {
            validate_legacy_runner_definition(&root, &owner)?;
            PathBuf::from("/bin/launchctl")
        }
        InstallService::Web => unreachable!("web was handled above"),
    };
    if let LaunchdJobState::Loaded { program, .. } = job {
        anyhow::ensure!(
            paths_refer_to_same_file(&program, &expected_program),
            "loaded legacy {} job uses foreign executable {}",
            service.label(),
            program.display()
        );
    }
    Ok(true)
}

pub(crate) fn launchd_job_exists(label: &str) -> anyhow::Result<bool> {
    require_macos()?;
    if matches!(
        launchd_job_state_for_label("system", label)?,
        LaunchdJobState::Loaded { .. }
    ) {
        return Ok(true);
    }
    let domain = format!("gui/{}", current_effective_uid());
    Ok(matches!(
        launchd_job_state_for_label(&domain, label)?,
        LaunchdJobState::Loaded { .. }
    ))
}

pub(crate) fn legacy_v0141_updater_is_owned(root: &Path) -> anyhow::Result<bool> {
    require_macos()?;
    let root = validate_install_root(root)?;
    let definition = legacy_updater_definition();
    let definition_exists = definition
        .try_exists()
        .with_context(|| format!("failed to inspect {}", definition.display()))?;
    let job = launchd_job_state_for_label("system", LEGACY_UPDATER_LABEL)?;
    if !definition_exists {
        return match job {
            LaunchdJobState::Absent => Ok(false),
            LaunchdJobState::Loaded { .. } => anyhow::bail!(
                "{LEGACY_UPDATER_LABEL} is loaded without its service definition; refusing an ambiguous operation"
            ),
        };
    }

    let owner = service_owner(&root)?;
    validate_legacy_updater_definition(&root, &owner)?;
    if let LaunchdJobState::Loaded { program, .. } = job {
        let expected = root.join("bin/oore");
        anyhow::ensure!(
            paths_refer_to_same_file(&program, &expected),
            "loaded legacy updater job uses foreign executable {}",
            program.display()
        );
    }
    Ok(true)
}

pub(crate) fn remove_legacy_v0141_updater(root: &Path) -> anyhow::Result<()> {
    require_macos()?;
    let root = validate_install_root(root)?;
    if !legacy_v0141_updater_is_owned(&root)? {
        return Ok(());
    }
    require_privileged_access()?;

    let definition_path = legacy_updater_definition();
    let metadata = fs::symlink_metadata(&definition_path)?;
    let snapshot = LegacyUpdaterSnapshot {
        definition: DefinitionSnapshot {
            contents: read_system_definition_bytes(&definition_path)?,
            mode: metadata.permissions().mode() & 0o777,
        },
        was_loaded: matches!(
            launchd_job_state_for_label("system", LEGACY_UPDATER_LABEL)?,
            LaunchdJobState::Loaded { .. }
        ),
    };
    ensure_legacy_updater_snapshot_unchanged(&root, &snapshot)?;

    let removal = (|| -> anyhow::Result<()> {
        stop_system_job(LEGACY_UPDATER_LABEL)?;
        remove_system_definition(&definition_path)?;
        anyhow::ensure!(
            !definition_path.exists(),
            "failed to remove {}",
            definition_path.display()
        );
        Ok(())
    })();
    if let Err(error) = removal {
        if let Err(rollback_error) = restore_legacy_updater_snapshot(&snapshot) {
            anyhow::bail!(
                "{error:#}; restoring the previous updater service also failed: {rollback_error:#}"
            );
        }
        return Err(error);
    }
    Ok(())
}

fn ensure_legacy_updater_snapshot_unchanged(
    root: &Path,
    snapshot: &LegacyUpdaterSnapshot,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        legacy_v0141_updater_is_owned(root)?,
        "legacy updater ownership changed before removal"
    );
    let path = legacy_updater_definition();
    let metadata = fs::symlink_metadata(&path)?;
    anyhow::ensure!(
        metadata.permissions().mode() & 0o777 == snapshot.definition.mode
            && read_system_definition_bytes(&path)? == snapshot.definition.contents,
        "legacy updater definition changed before removal"
    );
    let loaded = matches!(
        launchd_job_state_for_label("system", LEGACY_UPDATER_LABEL)?,
        LaunchdJobState::Loaded { .. }
    );
    anyhow::ensure!(
        loaded == snapshot.was_loaded,
        "legacy updater loaded state changed before removal"
    );
    Ok(())
}

fn restore_legacy_updater_snapshot(snapshot: &LegacyUpdaterSnapshot) -> anyhow::Result<()> {
    let mut failures = Vec::new();
    if let Err(error) = stop_system_job(LEGACY_UPDATER_LABEL) {
        failures.push(format!("failed to stop the replacement updater: {error:#}"));
    }
    let path = legacy_updater_definition();
    if let Err(error) = publish_system_definition(
        LEGACY_UPDATER_LABEL,
        &path,
        &snapshot.definition.contents,
        snapshot.definition.mode,
    ) {
        failures.push(format!(
            "failed to restore the updater definition: {error:#}"
        ));
    }
    if failures.is_empty()
        && snapshot.was_loaded
        && let Err(error) = load_system_definition(LEGACY_UPDATER_LABEL, &path)
    {
        failures.push(format!("failed to reload the previous updater: {error:#}"));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(failures.join("; "))
    }
}

pub(crate) fn service_is_owned(root: &Path, service: InstallService) -> anyhow::Result<bool> {
    require_macos()?;
    let root = validate_install_root(root)?;
    let definition = service.definition();
    let definition_exists = match fs::symlink_metadata(&definition) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {}", definition.display()));
        }
    };
    let job = launchd_job_state(service)?;
    let owner = service_owner(&root)?;
    if service == InstallService::Daemon {
        reject_legacy_user_daemon(&owner)?;
    }
    let legacy_web = if service == InstallService::Web {
        capture_legacy_web_snapshot(&root, &owner)?
    } else {
        None
    };

    if !definition_exists {
        return match (job, legacy_web) {
            (LaunchdJobState::Absent, Some(_)) => Ok(true),
            (LaunchdJobState::Absent, None) => Ok(false),
            (LaunchdJobState::Loaded { .. }, _) => anyhow::bail!(
                "{} is loaded without its service definition; refusing an ambiguous operation",
                service.label()
            ),
        };
    }

    anyhow::ensure!(
        legacy_web.is_none(),
        "{} has both system and legacy user definitions; refusing an ambiguous operation",
        service.label()
    );
    if let Err(managed_error) = validate_owned_definition(&root, service) {
        match service {
            InstallService::Daemon => {
                validate_legacy_daemon_definition(&root, &owner).with_context(|| {
                    format!(
                        "daemon definition matches neither the managed contract ({managed_error:#}) nor the exact v0.1.41 contract"
                    )
                })?;
            }
            InstallService::Runner => {
                validate_legacy_runner_definition(&root, &owner).with_context(|| {
                    format!(
                        "runner definition matches neither the managed contract ({managed_error:#}) nor the exact v0.1.41 contract"
                    )
                })?;
            }
            InstallService::Updater => {
                validate_legacy_updater_definition(&root, &owner).with_context(|| {
                    format!(
                        "updater definition matches neither the managed contract ({managed_error:#}) nor the exact v0.1.41 contract"
                    )
                })?;
            }
            InstallService::Web => return Err(managed_error),
        }
    }
    if let LaunchdJobState::Loaded { program, .. } = job {
        let expected = expected_launchd_program(&root, service);
        anyhow::ensure!(
            paths_refer_to_same_file(&program, &expected),
            "loaded {} job uses foreign executable {}",
            service.label(),
            program.display()
        );
    }
    Ok(true)
}

async fn install_service(definition: ServiceDefinition) -> anyhow::Result<()> {
    let owned = service_is_owned(&definition.root, definition.service)?;
    let snapshot = capture_snapshot(&definition.root, definition.service)?;
    let path_snapshot = ServicePathSnapshot::capture(&definition)?;
    let normalizes_legacy_daemon_path = definition.service == InstallService::Daemon
        && snapshot.definition.is_some()
        && validate_owned_definition(&definition.root, definition.service).is_err();
    if snapshot.definition.is_some() || snapshot.legacy_web.is_some() {
        anyhow::ensure!(owned, "refusing to replace an unowned service");
    } else if snapshot.was_loaded {
        anyhow::bail!(
            "{} is loaded without a restorable definition",
            definition.service.label()
        );
    }
    require_privileged_access()?;
    if normalizes_legacy_daemon_path {
        eprintln!(
            "Note: legacy daemon migration uses the standard service PATH ({SERVICE_PATH}). Optional toolchains remain checked when their features run."
        );
    }
    let rendered = render_launch_daemon(&definition);

    ensure_snapshot_unchanged(&definition.root, definition.service, &snapshot)?;
    path_snapshot.ensure_unchanged(definition.owner.uid)?;
    let path_preparation = (|| -> anyhow::Result<()> {
        if let Some(state_parent) = definition.state_parent.as_deref() {
            ensure_user_directory(state_parent, definition.owner.uid, false)?;
        }
        let prepared_log =
            prepare_log_path(&definition.root, definition.owner.uid, definition.service)?;
        anyhow::ensure!(
            prepared_log == definition.log_path,
            "prepared an unexpected service log path"
        );
        Ok(())
    })();
    if let Err(error) = path_preparation {
        return finish_prepared_path_rollback(error, path_snapshot.restore(definition.owner.uid));
    }
    if let Err(error) = ensure_snapshot_unchanged(&definition.root, definition.service, &snapshot) {
        return finish_prepared_path_rollback(error, path_snapshot.restore(definition.owner.uid));
    }

    let activation = async {
        publish_definition(definition.service, rendered.as_bytes())?;
        if snapshot.was_loaded {
            stop_service(definition.service)?;
        }
        if let Some(legacy) = &snapshot.legacy_web {
            if legacy.was_loaded {
                stop_user_service(legacy.owner_uid, definition.service)?;
            }
            remove_legacy_web_definition(legacy)?;
        }
        if definition.service == InstallService::Updater {
            load_system_definition(definition.service.label(), &definition.service.definition())?;
        } else {
            start_service(definition.service)?;
        }
        verify_service(&definition.root, definition.service).await
    }
    .await;

    if let Err(error) = activation {
        let service_rollback = restore_snapshot(definition.service, &snapshot);
        let path_rollback = path_snapshot.restore(definition.owner.uid);
        let mut rollback_failures = Vec::new();
        if let Err(rollback_error) = service_rollback {
            rollback_failures.push(format!(
                "restoring the previous service failed: {rollback_error:#}"
            ));
        }
        return match path_rollback {
            Ok(preserved) if rollback_failures.is_empty() => {
                finish_prepared_path_rollback(error, Ok(preserved))
            }
            Ok(preserved) => {
                if !preserved.is_empty() {
                    rollback_failures.push(format!(
                        "preserved non-empty directories: {}",
                        display_paths(&preserved)
                    ));
                }
                Err(error.context(rollback_failures.join("; ")))
            }
            Err(rollback_error) => {
                rollback_failures.push(format!(
                    "restoring service file permissions failed: {rollback_error:#}"
                ));
                Err(error.context(rollback_failures.join("; ")))
            }
        };
    }
    Ok(())
}

fn finish_prepared_path_rollback(
    error: anyhow::Error,
    rollback: anyhow::Result<Vec<PathBuf>>,
) -> anyhow::Result<()> {
    match rollback {
        Ok(preserved) if preserved.is_empty() => Err(error),
        Ok(preserved) => Err(error.context(format!(
            "Oore preserved non-empty directories created during the failed activation: {}",
            display_paths(&preserved)
        ))),
        Err(rollback_error) => Err(error.context(format!(
            "restoring service file permissions also failed: {rollback_error:#}"
        ))),
    }
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn capture_snapshot(root: &Path, service: InstallService) -> anyhow::Result<ServiceSnapshot> {
    let definition = match fs::symlink_metadata(service.definition()) {
        Ok(metadata) => {
            let contents = read_system_definition_bytes(&service.definition())?;
            Some(DefinitionSnapshot {
                contents,
                mode: metadata.permissions().mode() & 0o777,
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to preserve {}", service.definition().display()));
        }
    };
    let was_loaded = matches!(launchd_job_state(service)?, LaunchdJobState::Loaded { .. });
    let legacy_web = if service == InstallService::Web {
        let owner = service_owner(root)?;
        capture_legacy_web_snapshot(root, &owner)?
    } else {
        None
    };
    Ok(ServiceSnapshot {
        definition,
        was_loaded,
        legacy_web,
    })
}

fn ensure_snapshot_unchanged(
    root: &Path,
    service: InstallService,
    snapshot: &ServiceSnapshot,
) -> anyhow::Result<()> {
    let expected_owned = snapshot.definition.is_some() || snapshot.legacy_web.is_some();
    anyhow::ensure!(
        service_is_owned(root, service)? == expected_owned,
        "{} ownership changed before activation",
        service.label()
    );
    match &snapshot.definition {
        Some(expected) => {
            let metadata = fs::symlink_metadata(service.definition())?;
            anyhow::ensure!(
                metadata.permissions().mode() & 0o777 == expected.mode
                    && read_system_definition_bytes(&service.definition())? == expected.contents,
                "{} definition changed before activation",
                service.label()
            );
        }
        None => anyhow::ensure!(
            !service.definition().exists(),
            "{} definition appeared before activation",
            service.label()
        ),
    }
    let is_loaded = matches!(launchd_job_state(service)?, LaunchdJobState::Loaded { .. });
    anyhow::ensure!(
        is_loaded == snapshot.was_loaded,
        "{} loaded state changed before activation",
        service.label()
    );

    let current_legacy = if service == InstallService::Web {
        let owner = service_owner(root)?;
        capture_legacy_web_snapshot(root, &owner)?
    } else {
        None
    };
    match (&snapshot.legacy_web, current_legacy) {
        (None, None) => {}
        (Some(expected), Some(current)) => anyhow::ensure!(
            current.path == expected.path
                && current.definition.mode == expected.definition.mode
                && current.definition.contents == expected.definition.contents
                && current.was_loaded == expected.was_loaded
                && current.owner_uid == expected.owner_uid,
            "legacy web definition changed before activation"
        ),
        _ => anyhow::bail!("legacy web state changed before activation"),
    }
    Ok(())
}

fn restore_snapshot(service: InstallService, snapshot: &ServiceSnapshot) -> anyhow::Result<()> {
    let mut failures = Vec::new();
    if let Err(error) = stop_service(service) {
        failures.push(format!("failed to stop the replacement service: {error:#}"));
    }
    let restore_definition = match snapshot.definition.as_ref() {
        Some(definition) => {
            publish_definition_with_mode(service, &definition.contents, definition.mode)
        }
        None => remove_definition(service),
    };
    if let Err(error) = restore_definition {
        failures.push(format!(
            "failed to restore the service definition: {error:#}"
        ));
    }
    if let Some(legacy) = &snapshot.legacy_web
        && let Err(error) = restore_user_definition(legacy)
    {
        failures.push(format!(
            "failed to restore the legacy web definition: {error:#}"
        ));
    }
    if failures.is_empty()
        && snapshot.was_loaded
        && let Err(error) = start_service(service).and_then(|()| wait_for_service_running(service))
    {
        failures.push(format!("failed to restart the previous service: {error:#}"));
    }
    if failures.is_empty()
        && let Some(legacy) = &snapshot.legacy_web
        && legacy.was_loaded
        && let Err(error) = start_user_service(legacy.owner_uid, service, &legacy.path)
            .and_then(|()| wait_for_user_service_running(legacy.owner_uid, service))
    {
        failures.push(format!(
            "failed to restart the legacy web service: {error:#}"
        ));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(failures.join("; "))
    }
}

fn render_launch_daemon(definition: &ServiceDefinition) -> String {
    let arguments = definition
        .arguments
        .iter()
        .map(|argument| format!("        <string>{}</string>", xml_escape(argument)))
        .collect::<Vec<_>>()
        .join("\n");
    let run_at_load = if definition.service == InstallService::Updater {
        "false"
    } else {
        "true"
    };
    let keep_alive = run_at_load;
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>UserName</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
{arguments}
    </array>
    <key>WorkingDirectory</key>
    <string>{}</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>HOME</key>
        <string>{}</string>
        <key>PATH</key>
        <string>{SERVICE_PATH}</string>
        <key>OORE_INSTALL_ROOT</key>
        <string>{}</string>
    </dict>
    <key>Umask</key>
    <integer>63</integer>
    <key>RunAtLoad</key>
    <{run_at_load}/>
    <key>KeepAlive</key>
    <{keep_alive}/>
    <key>StandardOutPath</key>
    <string>{}</string>
    <key>StandardErrorPath</key>
    <string>{}</string>
</dict>
</plist>
"#,
        definition.service.label(),
        xml_escape(&definition.owner.name),
        xml_escape(&definition.root.display().to_string()),
        xml_escape(&definition.owner.home.display().to_string()),
        xml_escape(&definition.root.display().to_string()),
        xml_escape(&definition.log_path.display().to_string()),
        xml_escape(&definition.log_path.display().to_string()),
    )
}

fn validate_legacy_daemon_definition(root: &Path, owner: &ServiceOwner) -> anyhow::Result<()> {
    let path = InstallService::Daemon.definition();
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == 0
            && metadata.gid() == 0
            && metadata.permissions().mode() & 0o777 == 0o600,
        "legacy daemon definition has unexpected ownership, type, or permissions"
    );

    let document = read_plist_document(&path)?;
    validate_dictionary_keys(
        &document,
        &[
            "Label",
            "UserName",
            "ProgramArguments",
            "WorkingDirectory",
            "EnvironmentVariables",
            "RunAtLoad",
            "KeepAlive",
            "StandardOutPath",
            "StandardErrorPath",
        ],
        &[
            "Label",
            "UserName",
            "ProgramArguments",
            "WorkingDirectory",
            "EnvironmentVariables",
            "RunAtLoad",
            "KeepAlive",
            "StandardOutPath",
            "StandardErrorPath",
        ],
        "legacy daemon definition",
    )?;
    anyhow::ensure!(
        document.get("Label").and_then(Value::as_str) == Some(InstallService::Daemon.label())
            && document.get("UserName").and_then(Value::as_str) == Some(owner.name.as_str())
            && document.get("WorkingDirectory").and_then(Value::as_str)
                == Some(root.to_string_lossy().as_ref())
            && document.get("RunAtLoad").and_then(Value::as_bool) == Some(true)
            && document.get("KeepAlive").and_then(Value::as_bool) == Some(true),
        "legacy daemon definition does not match the v0.1.41 launch policy"
    );
    let expected_log = service_log_path(root, InstallService::Daemon);
    anyhow::ensure!(
        ["StandardOutPath", "StandardErrorPath"]
            .into_iter()
            .all(|key| document.get(key).and_then(Value::as_str)
                == Some(expected_log.to_string_lossy().as_ref())),
        "legacy daemon definition writes to an unexpected log path"
    );

    let arguments = plist_arguments(&document)?;
    anyhow::ensure!(
        arguments.len() == 4
            && paths_refer_to_same_file(Path::new(&arguments[0]), &root.join("bin/oored"))
            && arguments[1] == "run"
            && arguments[2] == "--listen",
        "legacy daemon command does not match the v0.1.41 contract"
    );
    anyhow::ensure!(
        arguments[3] == "127.0.0.1:8787",
        "legacy daemon uses a custom listen address; record the new access settings and restore the v0.1.41 local default before retrying"
    );

    let environment = document
        .get("EnvironmentVariables")
        .and_then(Value::as_object)
        .context("legacy daemon definition has no environment")?;
    anyhow::ensure!(
        environment.len() == 3
            && environment.contains_key("HOME")
            && environment.contains_key("PATH")
            && environment.contains_key("RUST_LOG"),
        "legacy daemon uses env-only external access settings that cannot be migrated safely; save those settings in Oore and remove the legacy environment overrides before retrying"
    );
    anyhow::ensure!(
        environment.get("HOME").and_then(Value::as_str)
            == Some(owner.home.to_string_lossy().as_ref())
            && environment
                .get("PATH")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty())
            && environment.get("RUST_LOG").and_then(Value::as_str) == Some("info")
            && environment
                .values()
                .all(|value| value.as_str().is_some_and(|value| !value.is_empty())),
        "legacy daemon has custom runtime environment values that cannot be migrated safely; restore the v0.1.41 defaults before retrying"
    );
    Ok(())
}

fn validate_legacy_runner_definition(root: &Path, owner: &ServiceOwner) -> anyhow::Result<()> {
    let path = InstallService::Runner.definition();
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == 0
            && metadata.gid() == 0
            && metadata.permissions().mode() & 0o777 == 0o644,
        "legacy runner definition has unexpected ownership, type, or permissions"
    );

    let document = read_plist_document(&path)?;
    validate_dictionary_keys(
        &document,
        &[
            "Label",
            "ProgramArguments",
            "EnvironmentVariables",
            "WorkingDirectory",
            "RunAtLoad",
            "KeepAlive",
            "StandardOutPath",
            "StandardErrorPath",
        ],
        &[
            "Label",
            "ProgramArguments",
            "EnvironmentVariables",
            "WorkingDirectory",
            "RunAtLoad",
            "KeepAlive",
            "StandardOutPath",
            "StandardErrorPath",
        ],
        "legacy runner definition",
    )?;
    anyhow::ensure!(
        document.get("Label").and_then(Value::as_str) == Some(InstallService::Runner.label())
            && document.get("WorkingDirectory").and_then(Value::as_str)
                == Some(root.to_string_lossy().as_ref())
            && document.get("RunAtLoad").and_then(Value::as_bool) == Some(true)
            && document.get("KeepAlive").and_then(Value::as_bool) == Some(true),
        "legacy runner definition does not match the v0.1.41 launch policy"
    );
    let expected_log = service_log_path(root, InstallService::Runner);
    anyhow::ensure!(
        ["StandardOutPath", "StandardErrorPath"]
            .into_iter()
            .all(|key| document.get(key).and_then(Value::as_str)
                == Some(expected_log.to_string_lossy().as_ref())),
        "legacy runner definition writes to an unexpected log path"
    );

    let arguments = plist_arguments(&document)?;
    anyhow::ensure!(
        arguments.len() == 13
            && arguments[0] == "/bin/launchctl"
            && arguments[1] == "asuser"
            && arguments[2] == owner.uid.to_string()
            && arguments[3] == "/usr/bin/sudo"
            && arguments[4] == "-E"
            && arguments[5] == "-H"
            && arguments[6] == "-u"
            && arguments[7] == owner.name
            && paths_refer_to_same_file(Path::new(&arguments[8]), &root.join("bin/oore"))
            && arguments[9] == "runner"
            && arguments[10] == "start"
            && arguments[11] == "--config"
            && Path::new(&arguments[12]) == owner.home.join(".oore/managed-runner.json"),
        "legacy runner command does not match the v0.1.41 contract"
    );

    let environment = document
        .get("EnvironmentVariables")
        .and_then(Value::as_object)
        .context("legacy runner definition has no environment")?;
    anyhow::ensure!(
        environment.len() == 4
            && environment.get("HOME").and_then(Value::as_str)
                == Some(owner.home.to_string_lossy().as_ref())
            && environment
                .get("PATH")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty())
            && environment.get("FVM_CACHE_PATH").and_then(Value::as_str)
                == Some(root.join("toolchains/flutter").to_string_lossy().as_ref())
            && environment
                .get(oore_runner::RUNNER_SERVICE_ACK_PATH_ENV)
                .and_then(Value::as_str)
                == Some(
                    root.join("run")
                        .join(oore_runner::RUNNER_SERVICE_ACK_FILE)
                        .to_string_lossy()
                        .as_ref()
                ),
        "legacy runner environment does not match the v0.1.41 contract"
    );
    Ok(())
}

fn legacy_updater_definition() -> PathBuf {
    Path::new("/Library/LaunchDaemons").join(format!("{LEGACY_UPDATER_LABEL}.plist"))
}

fn validate_legacy_updater_definition(root: &Path, owner: &ServiceOwner) -> anyhow::Result<()> {
    let path = legacy_updater_definition();
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == 0
            && metadata.gid() == 0
            && metadata.permissions().mode() & 0o777 == 0o600,
        "legacy updater definition has unexpected ownership, type, or permissions"
    );

    let document = read_plist_document(&path)?;
    validate_dictionary_keys(
        &document,
        &[
            "Label",
            "UserName",
            "SessionCreate",
            "ProgramArguments",
            "EnvironmentVariables",
            "WorkingDirectory",
            "StandardOutPath",
            "StandardErrorPath",
        ],
        &[
            "Label",
            "UserName",
            "SessionCreate",
            "ProgramArguments",
            "EnvironmentVariables",
            "WorkingDirectory",
            "StandardOutPath",
            "StandardErrorPath",
        ],
        "legacy updater definition",
    )?;
    anyhow::ensure!(
        document.get("Label").and_then(Value::as_str) == Some(LEGACY_UPDATER_LABEL)
            && document.get("UserName").and_then(Value::as_str) == Some(owner.name.as_str())
            && document.get("SessionCreate").and_then(Value::as_bool) == Some(true)
            && document.get("WorkingDirectory").and_then(Value::as_str)
                == Some(root.to_string_lossy().as_ref()),
        "legacy updater definition does not match the v0.1.41 launch policy"
    );
    let expected_log = root.join("logs/runtime-update.log");
    anyhow::ensure!(
        ["StandardOutPath", "StandardErrorPath"]
            .into_iter()
            .all(|key| document.get(key).and_then(Value::as_str)
                == Some(expected_log.to_string_lossy().as_ref())),
        "legacy updater definition writes to an unexpected log path"
    );

    let arguments = plist_arguments(&document)?;
    anyhow::ensure!(
        arguments.len() == 4
            && paths_refer_to_same_file(Path::new(&arguments[0]), &root.join("bin/oore"))
            && arguments[1] == "update-supervisor"
            && arguments[2] == "--request-file"
            && Path::new(&arguments[3]) == root.join("run/runtime-update-queue/request.json"),
        "legacy updater command does not match the v0.1.41 contract"
    );

    let environment = document
        .get("EnvironmentVariables")
        .and_then(Value::as_object)
        .context("legacy updater definition has no environment")?;
    anyhow::ensure!(
        environment.len() == 3
            && environment.get("HOME").and_then(Value::as_str)
                == Some(owner.home.to_string_lossy().as_ref())
            && environment
                .get("PATH")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty())
            && environment.get("OORE_INSTALL_ROOT").and_then(Value::as_str)
                == Some(root.to_string_lossy().as_ref()),
        "legacy updater environment does not match the v0.1.41 contract"
    );
    Ok(())
}

fn reject_legacy_user_daemon(owner: &ServiceOwner) -> anyhow::Result<()> {
    let path = owner
        .home
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", InstallService::Daemon.label()));
    let definition_exists = match fs::symlink_metadata(&path) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    };
    let loaded = matches!(
        user_launchd_job_state(owner.uid, InstallService::Daemon)?,
        LaunchdJobState::Loaded { .. }
    );
    anyhow::ensure!(
        !definition_exists && !loaded,
        "an older login-session daemon was found at {}; this release migrates only the v0.1.41 system daemon. Stop and remove that legacy LaunchAgent before retrying setup",
        path.display()
    );
    Ok(())
}

fn reject_legacy_user_runner(owner: &ServiceOwner) -> anyhow::Result<()> {
    let path = owner
        .home
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", InstallService::Runner.label()));
    let definition_exists = path
        .try_exists()
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    let job = user_launchd_job_state(owner.uid, InstallService::Runner)?;
    anyhow::ensure!(
        !definition_exists && matches!(job, LaunchdJobState::Absent),
        "an older login-session runner exists; the strict v0.1.41 recovery path accepts only the boot-time runner definition"
    );
    Ok(())
}

fn capture_legacy_web_snapshot(
    root: &Path,
    owner: &ServiceOwner,
) -> anyhow::Result<Option<LegacyWebSnapshot>> {
    let path = owner
        .home
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", InstallService::Web.label()));
    let parent = path
        .parent()
        .context("legacy web definition has no parent directory")?;
    let parent_metadata = match fs::symlink_metadata(parent) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", parent.display()));
        }
    };
    let job = user_launchd_job_state(owner.uid, InstallService::Web)?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return match job {
                LaunchdJobState::Absent => Ok(None),
                LaunchdJobState::Loaded { .. } => anyhow::bail!(
                    "legacy web job is loaded without {}; refusing an ambiguous operation",
                    path.display()
                ),
            };
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    };
    let parent_metadata = parent_metadata.context("legacy web definition parent is missing")?;
    anyhow::ensure!(
        parent_metadata.file_type().is_dir()
            && !parent_metadata.file_type().is_symlink()
            && parent_metadata.uid() == owner.uid
            && parent_metadata.permissions().mode() & 0o022 == 0,
        "legacy LaunchAgents directory has unsafe ownership or permissions"
    );
    validate_legacy_web_definition(root, owner, &path, &metadata)?;
    if let LaunchdJobState::Loaded { program, .. } = &job {
        anyhow::ensure!(
            paths_refer_to_same_file(program, &root.join("bin/oore-web")),
            "loaded legacy web job uses foreign executable {}",
            program.display()
        );
    }
    let contents = fs::read(&path).with_context(|| {
        format!(
            "failed to preserve legacy web definition {}",
            path.display()
        )
    })?;
    Ok(Some(LegacyWebSnapshot {
        path,
        definition: DefinitionSnapshot {
            contents,
            mode: metadata.permissions().mode() & 0o777,
        },
        was_loaded: matches!(job, LaunchdJobState::Loaded { .. }),
        owner_uid: owner.uid,
    }))
}

fn validate_legacy_web_definition(
    root: &Path,
    owner: &ServiceOwner,
    path: &Path,
    metadata: &fs::Metadata,
) -> anyhow::Result<()> {
    let mode = metadata.permissions().mode() & 0o777;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == owner.uid
            && matches!(mode, 0o600 | 0o644),
        "legacy web definition has unexpected ownership, type, or permissions"
    );
    let document = read_plist_document(path)?;
    validate_dictionary_keys(
        &document,
        &[
            "Label",
            "ProgramArguments",
            "RunAtLoad",
            "KeepAlive",
            "StandardOutPath",
            "StandardErrorPath",
        ],
        &[
            "Label",
            "ProgramArguments",
            "EnvironmentVariables",
            "RunAtLoad",
            "KeepAlive",
            "StandardOutPath",
            "StandardErrorPath",
        ],
        "legacy web definition",
    )?;
    anyhow::ensure!(
        document.get("Label").and_then(Value::as_str) == Some(InstallService::Web.label())
            && document.get("RunAtLoad").and_then(Value::as_bool) == Some(true)
            && document.get("KeepAlive").and_then(Value::as_bool) == Some(true),
        "legacy web definition does not match the v0.1.41 launch policy"
    );
    let expected_log = service_log_path(root, InstallService::Web);
    anyhow::ensure!(
        ["StandardOutPath", "StandardErrorPath"]
            .into_iter()
            .all(|key| document.get(key).and_then(Value::as_str)
                == Some(expected_log.to_string_lossy().as_ref())),
        "legacy web definition writes to an unexpected log path"
    );

    let arguments = plist_arguments(&document)?;
    anyhow::ensure!(
        arguments.len() >= 7
            && paths_refer_to_same_file(Path::new(&arguments[0]), &root.join("bin/oore-web"))
            && arguments[1] == "--listen"
            && arguments[3] == "--backend-url"
            && arguments[5] == "--dist-dir"
            && Path::new(&arguments[6]) == root.join("web-dist"),
        "legacy web command does not match the v0.1.41 contract"
    );
    anyhow::ensure!(
        arguments[2] == "127.0.0.1:4173"
            && arguments[4] == "http://127.0.0.1:8787"
            && arguments.len() == 7,
        "legacy web uses custom network or transport settings; record the new access settings and restore the v0.1.41 local defaults before retrying"
    );
    validate_web_transport(&arguments[2], &arguments[4], false, false)?;

    let environment = document
        .get("EnvironmentVariables")
        .and_then(Value::as_object)
        .context("legacy web definition has no environment")?;
    let upstream_header = environment
        .get("OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER")
        .and_then(Value::as_str)
        .context("legacy web definition has no upstream proof header")?;
    anyhow::ensure!(
        valid_http_header_name(upstream_header),
        "legacy web upstream proof header is invalid"
    );
    anyhow::ensure!(
        environment.len() == 1 && upstream_header == "x-oore-web-trusted-proxy-secret",
        "this release migrates only the local default v0.1.41 web service; record and remove the legacy Trusted Proxy environment settings before retrying setup"
    );
    Ok(())
}

fn validate_dictionary_keys(
    document: &Value,
    required: &[&str],
    allowed: &[&str],
    context: &str,
) -> anyhow::Result<()> {
    let object = document
        .as_object()
        .with_context(|| format!("{context} is not a dictionary"))?;
    anyhow::ensure!(
        required.iter().all(|key| object.contains_key(*key))
            && object.keys().all(|key| allowed.contains(&key.as_str())),
        "{context} contains missing or unexpected keys"
    );
    Ok(())
}

fn valid_http_header_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

fn validate_owned_definition(root: &Path, service: InstallService) -> anyhow::Result<()> {
    let path = service.definition();
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
        "service definition is not a regular file: {}",
        path.display()
    );
    anyhow::ensure!(
        metadata.uid() == 0 && metadata.gid() == 0,
        "service definition is not owned by root:wheel"
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o777 == 0o644,
        "service definition permissions are not 0644: {}",
        path.display()
    );

    let document = read_plist_document(&path)?;
    let owner = service_owner(root)?;
    validate_owned_document(root, service, &owner, &document)
}

fn validate_owned_document(
    root: &Path,
    service: InstallService,
    owner: &ServiceOwner,
    document: &Value,
) -> anyhow::Result<()> {
    let keys: &[&str] = match service {
        InstallService::Runner => &[
            "Label",
            "ProgramArguments",
            "WorkingDirectory",
            "EnvironmentVariables",
            "Umask",
            "RunAtLoad",
            "KeepAlive",
            "StandardOutPath",
            "StandardErrorPath",
        ],
        InstallService::Daemon | InstallService::Updater | InstallService::Web => &[
            "Label",
            "UserName",
            "ProgramArguments",
            "WorkingDirectory",
            "EnvironmentVariables",
            "Umask",
            "RunAtLoad",
            "KeepAlive",
            "StandardOutPath",
            "StandardErrorPath",
        ],
    };
    validate_dictionary_keys(document, keys, keys, "service definition")?;
    anyhow::ensure!(
        document.get("Label").and_then(Value::as_str) == Some(service.label()),
        "service definition has an unexpected label"
    );
    match service {
        InstallService::Runner => anyhow::ensure!(
            document.get("UserName").is_none(),
            "runner service must enter the managed account session through its wrapper"
        ),
        InstallService::Daemon | InstallService::Updater | InstallService::Web => anyhow::ensure!(
            document.get("UserName").and_then(Value::as_str) == Some(owner.name.as_str()),
            "service definition runs as an unexpected account"
        ),
    }
    anyhow::ensure!(
        document.get("WorkingDirectory").and_then(Value::as_str)
            == Some(root.to_string_lossy().as_ref()),
        "service definition has an unexpected working directory"
    );
    anyhow::ensure!(
        document.get("Umask").and_then(Value::as_u64) == Some(0o77),
        "service definition does not set Umask 0077"
    );
    let should_stay_running = service != InstallService::Updater;
    anyhow::ensure!(
        document.get("RunAtLoad").and_then(Value::as_bool) == Some(should_stay_running)
            && document.get("KeepAlive").and_then(Value::as_bool) == Some(should_stay_running),
        "service definition does not use the managed launch policy"
    );
    let expected_log = service_log_path(root, service);
    anyhow::ensure!(
        ["StandardOutPath", "StandardErrorPath"]
            .into_iter()
            .all(|key| document.get(key).and_then(Value::as_str)
                == Some(expected_log.to_string_lossy().as_ref())),
        "service definition writes to an unexpected log path"
    );
    let environment = document
        .get("EnvironmentVariables")
        .and_then(Value::as_object)
        .context("service definition has no environment")?;
    match service {
        InstallService::Runner => validate_runner_environment(root, owner, environment)?,
        InstallService::Daemon | InstallService::Updater | InstallService::Web => {
            anyhow::ensure!(
                environment.len() == 3
                    && environment.get("PATH").and_then(Value::as_str) == Some(SERVICE_PATH)
                    && environment.get("HOME").and_then(Value::as_str)
                        == Some(owner.home.to_string_lossy().as_ref())
                    && environment.get("OORE_INSTALL_ROOT").and_then(Value::as_str)
                        == Some(root.to_string_lossy().as_ref()),
                "service definition has an unexpected environment"
            );
        }
    }
    validate_definition_arguments(root, service, owner, document)
}

// Runner services installed before FVM was removed also set FVM_CACHE_PATH.
// The runner ignores it now, so accept it until `oore setup` rewrites the
// service, instead of treating an Oore-written service as foreign.
fn validate_runner_environment(
    root: &Path,
    owner: &ServiceOwner,
    environment: &serde_json::Map<String, Value>,
) -> anyhow::Result<()> {
    let legacy_fvm_cache = environment.get("FVM_CACHE_PATH").map(|value| {
        value.as_str() == Some(root.join("toolchains/flutter").to_string_lossy().as_ref())
    });
    anyhow::ensure!(
        legacy_fvm_cache != Some(false)
            && environment.len() == 3 + usize::from(legacy_fvm_cache.is_some())
            && environment.get("PATH").and_then(Value::as_str) == Some(SERVICE_PATH)
            && environment.get("HOME").and_then(Value::as_str)
                == Some(owner.home.to_string_lossy().as_ref())
            && environment
                .get(oore_runner::RUNNER_SERVICE_ACK_PATH_ENV)
                .and_then(Value::as_str)
                == Some(
                    root.join("run")
                        .join(oore_runner::RUNNER_SERVICE_ACK_FILE)
                        .to_string_lossy()
                        .as_ref(),
                ),
        "runner service definition has an unexpected environment"
    );
    Ok(())
}

fn validate_definition_arguments(
    root: &Path,
    service: InstallService,
    owner: &ServiceOwner,
    document: &Value,
) -> anyhow::Result<()> {
    let arguments = document
        .get("ProgramArguments")
        .and_then(Value::as_array)
        .context("service definition has no ProgramArguments")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .context("service ProgramArguments must contain only strings")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    match service {
        InstallService::Daemon => {
            let expected = expected_executable(root, service);
            anyhow::ensure!(
                arguments.first().map(Path::new) == Some(expected.as_path())
                    && arguments.len() == 6
                    && arguments[1] == "run"
                    && arguments[2] == "--listen"
                    && arguments[4] == "--state-file",
                "daemon service arguments do not match the managed contract"
            );
            parse_listen_address(&arguments[3])?;
            anyhow::ensure!(
                Path::new(&arguments[5]).is_absolute(),
                "daemon state file is not absolute"
            );
        }
        InstallService::Web => {
            let expected = expected_executable(root, service);
            anyhow::ensure!(
                arguments.first().map(Path::new) == Some(expected.as_path()),
                "service definition uses an unexpected executable"
            );
            validate_web_arguments(root, &arguments)?;
        }
        InstallService::Updater => {
            let expected = expected_executable(root, service);
            anyhow::ensure!(
                arguments.len() == 4
                    && arguments.first().map(Path::new) == Some(expected.as_path())
                    && arguments[1] == "update-supervisor"
                    && arguments[2] == "--request-file"
                    && Path::new(&arguments[3]) == root.join(UPDATE_REQUEST_FILE),
                "update supervisor arguments do not match the managed contract"
            );
        }
        InstallService::Runner => {
            let expected = expected_executable(root, service);
            anyhow::ensure!(
                arguments.len() == 13
                    && arguments[0] == "/bin/launchctl"
                    && arguments[1] == "asuser"
                    && arguments[2] == owner.uid.to_string()
                    && arguments[3] == "/usr/bin/sudo"
                    && arguments[4] == "-E"
                    && arguments[5] == "-H"
                    && arguments[6] == "-u"
                    && arguments[7] == owner.name
                    && Path::new(&arguments[8]) == expected
                    && arguments[9] == "runner"
                    && arguments[10] == "start"
                    && arguments[11] == "--config",
                "runner service arguments do not match the managed contract"
            );
            let config = Path::new(&arguments[12]);
            let resolved_config = fs::canonicalize(config)
                .with_context(|| format!("failed to resolve runner config {}", config.display()))?;
            anyhow::ensure!(
                config.is_absolute() && resolved_config.starts_with(root),
                "runner config is outside the install root"
            );
            verify_private_user_file(&resolved_config, fs::symlink_metadata(root)?.uid())?;
        }
    }
    Ok(())
}

fn validate_web_arguments(root: &Path, arguments: &[String]) -> anyhow::Result<()> {
    anyhow::ensure!(
        arguments.len() >= 8
            && arguments[1] == "serve"
            && arguments[2] == "--listen"
            && arguments[4] == "--backend-url"
            && arguments[6] == "--dist-dir"
            && Path::new(&arguments[7]) == root.join("web-dist"),
        "web service arguments do not match the managed contract"
    );
    let mut index = 8;
    let proof = root.join("secrets/web-backend-proof");
    let header_file = root.join("secrets/web-user-email-header");
    let upstream_proof = root.join("secrets/web-upstream-proof");
    if arguments.get(index).map(String::as_str) == Some("--trusted-proxy-secret-file") {
        anyhow::ensure!(
            arguments.get(index + 1).map(Path::new) == Some(proof.as_path())
                && arguments.get(index + 2).map(String::as_str)
                    == Some("--trusted-proxy-user-email-header"),
            "web pairing arguments do not match the managed contract"
        );
        let configured_header = arguments
            .get(index + 3)
            .context("web pairing header has no value")?;
        let stored_header = read_private_header(&header_file, fs::symlink_metadata(root)?.uid())?;
        anyhow::ensure!(
            configured_header == &stored_header,
            "web pairing header does not match its protected metadata"
        );
        verify_private_user_file(&proof, fs::symlink_metadata(root)?.uid())?;
        index += 4;
    }
    if arguments.get(index).map(String::as_str) == Some("--upstream-trusted-proxy-secret-file") {
        anyhow::ensure!(
            arguments.get(index + 1).map(Path::new) == Some(upstream_proof.as_path()),
            "web upstream proof argument does not match the managed contract"
        );
        verify_private_user_file(&upstream_proof, fs::symlink_metadata(root)?.uid())?;
        index += 2;
    }
    let flags = &arguments[index..];
    let flags_are_valid = match flags {
        [] => true,
        [one] => one == "--browser-transport-protected" || one == "--backend-transport-protected",
        [one, two] => {
            one == "--browser-transport-protected" && two == "--backend-transport-protected"
        }
        _ => false,
    };
    anyhow::ensure!(flags_are_valid, "web service contains unexpected flags");
    validate_web_transport(
        &arguments[3],
        &arguments[5],
        flags
            .iter()
            .any(|flag| flag == "--browser-transport-protected"),
        flags
            .iter()
            .any(|flag| flag == "--backend-transport-protected"),
    )
}

fn add_web_pairing_arguments(
    root: &Path,
    uid: u32,
    arguments: &mut Vec<String>,
) -> anyhow::Result<()> {
    let proof = root.join("secrets/web-backend-proof");
    let header_file = root.join("secrets/web-user-email-header");
    let proof_exists = proof.try_exists()?;
    let header_exists = header_file.try_exists()?;
    anyhow::ensure!(
        proof_exists == header_exists,
        "web pairing requires both {} and {}",
        proof.display(),
        header_file.display()
    );
    if !proof_exists {
        add_web_upstream_proof_argument(root, uid, arguments)?;
        return Ok(());
    }
    verify_private_user_file(&proof, uid)?;
    let header = read_private_header(&header_file, uid)?;
    arguments.extend([
        "--trusted-proxy-secret-file".to_string(),
        proof.display().to_string(),
        "--trusted-proxy-user-email-header".to_string(),
        header,
    ]);
    add_web_upstream_proof_argument(root, uid, arguments)?;
    Ok(())
}

fn add_web_upstream_proof_argument(
    root: &Path,
    uid: u32,
    arguments: &mut Vec<String>,
) -> anyhow::Result<()> {
    let path = root.join("secrets/web-upstream-proof");
    if !path.try_exists()? {
        return Ok(());
    }
    verify_private_user_file(&path, uid)?;
    arguments.extend([
        "--upstream-trusted-proxy-secret-file".to_string(),
        path.display().to_string(),
    ]);
    Ok(())
}

fn read_private_header(path: &Path, uid: u32) -> anyhow::Result<String> {
    verify_private_user_file(path, uid)?;
    let header = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .trim()
        .to_ascii_lowercase();
    anyhow::ensure!(
        valid_http_header_name(&header),
        "web user email header metadata is invalid"
    );
    Ok(header)
}

fn verify_private_user_file(path: &Path, uid: u32) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == uid
            && metadata.permissions().mode() & 0o777 == 0o600,
        "protected file has unsafe ownership or permissions: {}",
        path.display()
    );
    Ok(())
}

async fn verify_endpoint_or_runner(
    client: &reqwest::Client,
    root: &Path,
    service: InstallService,
    document: &Value,
    pid: Option<u32>,
) -> anyhow::Result<()> {
    match service {
        InstallService::Daemon => verify_daemon_endpoint(client, root, document).await,
        InstallService::Web => verify_web_endpoint(client, root, document).await,
        InstallService::Runner => verify_runner_endpoint(root, document, pid),
        InstallService::Updater => Ok(()),
    }
}

async fn verify_daemon_endpoint(
    client: &reqwest::Client,
    root: &Path,
    document: &Value,
) -> anyhow::Result<()> {
    let arguments = plist_arguments(document)?;
    let listen =
        option_value(&arguments, "--listen").context("daemon service has no listen address")?;
    let base = local_daemon_service_url(listen)?;
    let health = get_json(client, base.join("healthz")?).await?;
    anyhow::ensure!(
        health.get("ok").and_then(Value::as_bool) == Some(true),
        "daemon liveness check returned an unhealthy response"
    );
    let expected_version = installed_version(root);
    anyhow::ensure!(
        health.get("version").and_then(Value::as_str) == Some(expected_version.as_str())
            && health.get("package_version").and_then(Value::as_str)
                == Some(env!("CARGO_PKG_VERSION")),
        "daemon health response does not match the installed release"
    );
    let ready = get_json(client, base.join("readyz")?).await?;
    anyhow::ensure!(
        ready.get("ok").and_then(Value::as_bool) == Some(true),
        "daemon readiness check failed"
    );
    Ok(())
}

async fn verify_web_endpoint(
    client: &reqwest::Client,
    root: &Path,
    document: &Value,
) -> anyhow::Result<()> {
    let arguments = plist_arguments(document)?;
    let listen =
        option_value(&arguments, "--listen").context("web service has no listen address")?;
    let base = local_service_url(listen)?;
    let health = get_json(client, base.join("__oore_web_healthz")?).await?;
    anyhow::ensure!(
        health.get("ok").and_then(Value::as_bool) == Some(true),
        "web health check returned an unhealthy response"
    );
    let expected_version = installed_version(root);
    anyhow::ensure!(
        health.get("version").and_then(Value::as_str) == Some(expected_version.as_str()),
        "web health response does not match the installed release"
    );
    let setup_status = get_json(client, base.join("v1/public/setup-status")?).await?;
    serde_json::from_value::<oore_contract::SetupStatus>(setup_status)
        .context("web could not return a valid control-plane setup status")?;
    Ok(())
}

fn verify_runner_endpoint(root: &Path, document: &Value, pid: Option<u32>) -> anyhow::Result<()> {
    let service_pid = pid.context("managed runner has no process id")?;
    let arguments = plist_arguments(document)?;
    let config_path =
        option_value(&arguments, "--config").context("runner service has no config path")?;
    let config: oore_runner::RunnerConfig = serde_json::from_slice(
        &fs::read(config_path)
            .with_context(|| format!("failed to read runner config {config_path}"))?,
    )
    .context("managed runner config is invalid")?;
    let acknowledgement = document
        .get("EnvironmentVariables")
        .and_then(Value::as_object)
        .and_then(|environment| environment.get(oore_runner::RUNNER_SERVICE_ACK_PATH_ENV))
        .and_then(Value::as_str)
        .context("runner service has no acknowledgement path")?;
    let acknowledgement_path = Path::new(acknowledgement);
    let acknowledgement_value: oore_runner::RunnerServiceAck =
        serde_json::from_slice(&fs::read(acknowledgement_path).with_context(|| {
            format!(
                "failed to read runner service acknowledgement {}",
                acknowledgement_path.display()
            )
        })?)
        .context("runner service acknowledgement is invalid")?;
    anyhow::ensure!(
        crate::process_is_descendant_of(acknowledgement_value.pid, service_pid)?,
        "runner acknowledgement process is not owned by the active service"
    );
    oore_runner::verify_runner_service_ack(
        acknowledgement_path,
        &config,
        &expected_executable(root, InstallService::Runner),
        acknowledgement_value.pid,
        None,
        Duration::from_secs(oore_runner::RUNNER_SERVICE_ACK_MAX_AGE_SECS),
    )
    .map(|_| ())
}

fn plist_arguments(document: &Value) -> anyhow::Result<Vec<String>> {
    document
        .get("ProgramArguments")
        .and_then(Value::as_array)
        .context("service definition has no ProgramArguments")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .context("service argument is not a string")
        })
        .collect()
}

fn option_value<'a>(arguments: &'a [String], option: &str) -> Option<&'a str> {
    arguments
        .windows(2)
        .find(|pair| pair[0] == option)
        .map(|pair| pair[1].as_str())
}

async fn get_json(client: &reqwest::Client, url: Url) -> anyhow::Result<Value> {
    let response = client
        .get(url.clone())
        .send()
        .await
        .with_context(|| format!("failed to reach {url}"))?;
    anyhow::ensure!(
        response.status().is_success(),
        "{url} returned HTTP {}",
        response.status()
    );
    response
        .json::<Value>()
        .await
        .with_context(|| format!("{url} returned invalid JSON"))
}

fn installed_version(root: &Path) -> String {
    fs::read_to_string(root.join("VERSION"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

fn publish_definition(service: InstallService, contents: &[u8]) -> anyhow::Result<()> {
    publish_definition_with_mode(service, contents, 0o644)
}

fn publish_definition_with_mode(
    service: InstallService,
    contents: &[u8],
    mode: u32,
) -> anyhow::Result<()> {
    publish_system_definition(service.label(), &service.definition(), contents, mode)
}

fn publish_system_definition(
    label: &str,
    target: &Path,
    contents: &[u8],
    mode: u32,
) -> anyhow::Result<()> {
    let mode_argument = match mode {
        0o600 => "0600",
        0o644 => "0644",
        _ => anyhow::bail!("refusing to publish an unsupported service definition mode {mode:o}"),
    };
    let mut source =
        tempfile::NamedTempFile::new().context("failed to stage service definition")?;
    source
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    source.write_all(contents)?;
    source.as_file_mut().sync_all()?;
    let protected_stage = Path::new("/Library/LaunchDaemons").join(format!(
        ".{}.{}.plist",
        label,
        uuid::Uuid::new_v4()
    ));
    let publish = (|| -> anyhow::Result<()> {
        privileged_checked(
            "/usr/bin/install",
            &[
                OsStr::new("-o"),
                OsStr::new("root"),
                OsStr::new("-g"),
                OsStr::new("wheel"),
                OsStr::new("-m"),
                OsStr::new(mode_argument),
                source.path().as_os_str(),
                protected_stage.as_os_str(),
            ],
            "staging the launchd service definition",
        )?;
        privileged_checked(
            "/usr/bin/plutil",
            &[OsStr::new("-lint"), protected_stage.as_os_str()],
            "validating the launchd service definition",
        )?;
        privileged_checked(
            "/bin/mv",
            &[
                OsStr::new("-f"),
                protected_stage.as_os_str(),
                target.as_os_str(),
            ],
            "publishing the launchd service definition",
        )
    })();
    if publish.is_err() {
        let _ = privileged_output("/bin/rm", &[OsStr::new("-f"), protected_stage.as_os_str()]);
    }
    publish?;
    let metadata = fs::symlink_metadata(target)
        .with_context(|| format!("failed to inspect {}", target.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == 0
            && metadata.gid() == 0
            && metadata.permissions().mode() & 0o777 == mode,
        "published service definition has unsafe ownership or permissions"
    );
    anyhow::ensure!(
        read_system_definition_bytes(target)? == contents,
        "published service definition changed during installation"
    );
    Ok(())
}

fn read_system_definition_bytes(path: &Path) -> anyhow::Result<Vec<u8>> {
    match fs::read(path) {
        Ok(contents) => Ok(contents),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            let output = privileged_output("/bin/cat", &[path.as_os_str()])?;
            ensure_command_success(&output, "reading the protected service definition")?;
            Ok(output.stdout)
        }
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn remove_definition(service: InstallService) -> anyhow::Result<()> {
    let target = service.definition();
    remove_system_definition(&target)
}

fn remove_system_definition(target: &Path) -> anyhow::Result<()> {
    privileged_checked(
        "/bin/rm",
        &[OsStr::new("-f"), target.as_os_str()],
        "removing the launchd service definition",
    )
}

fn start_service(service: InstallService) -> anyhow::Result<()> {
    let target = service.definition();
    load_system_definition(service.label(), &target)?;
    privileged_checked(
        "/bin/launchctl",
        &[
            OsStr::new("kickstart"),
            OsStr::new("-k"),
            OsStr::new(&format!("system/{}", service.label())),
        ],
        "starting the launchd service",
    )
}

fn load_system_definition(label: &str, target: &Path) -> anyhow::Result<()> {
    let first = privileged_output(
        "/bin/launchctl",
        &[
            OsStr::new("bootstrap"),
            OsStr::new("system"),
            target.as_os_str(),
        ],
    )?;
    if !first.status.success() {
        std::thread::sleep(Duration::from_millis(250));
        privileged_checked(
            "/bin/launchctl",
            &[
                OsStr::new("bootstrap"),
                OsStr::new("system"),
                target.as_os_str(),
            ],
            "loading the launchd service",
        )?;
    }
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    while Instant::now() < deadline {
        if matches!(
            launchd_job_state_for_label("system", label)?,
            LaunchdJobState::Loaded { .. }
        ) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("{label} did not reload after rollback")
}

fn stop_service(service: InstallService) -> anyhow::Result<()> {
    stop_system_job(service.label())
}

fn stop_system_job(label: &str) -> anyhow::Result<()> {
    if matches!(
        launchd_job_state_for_label("system", label)?,
        LaunchdJobState::Absent
    ) {
        return Ok(());
    }
    let name = format!("system/{label}");
    let output = privileged_output(
        "/bin/launchctl",
        &[OsStr::new("bootout"), OsStr::new(&name)],
    )?;
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    while Instant::now() < deadline {
        if matches!(
            launchd_job_state_for_label("system", label)?,
            LaunchdJobState::Absent
        ) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let detail = command_detail(&output);
    anyhow::bail!("failed to stop {name}: {detail}")
}

fn wait_for_service_running(service: InstallService) -> anyhow::Result<()> {
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    while Instant::now() < deadline {
        if matches!(
            launchd_job_state(service)?,
            LaunchdJobState::Loaded {
                running: true,
                pid: Some(_),
                ..
            }
        ) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("{} did not resume after rollback", service.label())
}

fn remove_legacy_web_definition(snapshot: &LegacyWebSnapshot) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(&snapshot.path)
        .with_context(|| format!("failed to inspect {}", snapshot.path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == snapshot.owner_uid
            && metadata.permissions().mode() & 0o777 == snapshot.definition.mode
            && fs::read(&snapshot.path)? == snapshot.definition.contents,
        "legacy web definition changed before migration"
    );
    fs::remove_file(&snapshot.path)
        .with_context(|| format!("failed to remove {}", snapshot.path.display()))
}

fn restore_user_definition(snapshot: &LegacyWebSnapshot) -> anyhow::Result<()> {
    let parent = snapshot
        .path
        .parent()
        .context("legacy web definition has no parent directory")?;
    let parent_metadata = fs::symlink_metadata(parent)
        .with_context(|| format!("failed to inspect {}", parent.display()))?;
    anyhow::ensure!(
        parent_metadata.file_type().is_dir()
            && !parent_metadata.file_type().is_symlink()
            && parent_metadata.uid() == snapshot.owner_uid
            && parent_metadata.permissions().mode() & 0o022 == 0,
        "legacy LaunchAgents directory has unsafe ownership or permissions"
    );
    if snapshot.path.exists() {
        let metadata = fs::symlink_metadata(&snapshot.path)?;
        anyhow::ensure!(
            metadata.file_type().is_file()
                && !metadata.file_type().is_symlink()
                && metadata.uid() == snapshot.owner_uid
                && fs::read(&snapshot.path)? == snapshot.definition.contents,
            "refusing to overwrite a changed legacy web definition during rollback"
        );
    }

    let stage = parent.join(format!(
        ".{}.{}.restore",
        InstallService::Web.label(),
        uuid::Uuid::new_v4()
    ));
    let restore = (|| -> anyhow::Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(snapshot.definition.mode)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&stage)
            .with_context(|| format!("failed to stage {}", stage.display()))?;
        file.write_all(&snapshot.definition.contents)?;
        file.set_permissions(fs::Permissions::from_mode(snapshot.definition.mode))?;
        file.sync_all()?;
        let mut lint = Command::new("/usr/bin/plutil");
        lint.args(["-lint"]).arg(&stage);
        let output = command_output(lint, "validating the restored legacy web definition")?;
        ensure_command_success(&output, "validating the restored legacy web definition")?;
        fs::rename(&stage, &snapshot.path).with_context(|| {
            format!(
                "failed to restore legacy web definition {}",
                snapshot.path.display()
            )
        })
    })();
    if restore.is_err() {
        let _ = fs::remove_file(&stage);
    }
    restore?;
    let metadata = fs::symlink_metadata(&snapshot.path)?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == snapshot.owner_uid
            && metadata.permissions().mode() & 0o777 == snapshot.definition.mode
            && fs::read(&snapshot.path)? == snapshot.definition.contents,
        "restored legacy web definition changed during rollback"
    );
    Ok(())
}

fn start_user_service(uid: u32, service: InstallService, plist: &Path) -> anyhow::Result<()> {
    if matches!(
        user_launchd_job_state(uid, service)?,
        LaunchdJobState::Loaded { .. }
    ) {
        return Ok(());
    }
    let domain = format!("gui/{uid}");
    let mut bootstrap = Command::new("/bin/launchctl");
    bootstrap.arg("bootstrap").arg(&domain).arg(plist);
    let first = command_output(bootstrap, "restoring the legacy user service")?;
    if !first.status.success() {
        let mut load = Command::new("/bin/launchctl");
        load.args(["load", "-w"]).arg(plist);
        let output = command_output(load, "restoring the legacy user service")?;
        ensure_command_success(&output, "restoring the legacy user service")?;
    }
    let name = format!("{domain}/{}", service.label());
    let mut kickstart = Command::new("/bin/launchctl");
    kickstart.args(["kickstart", "-k", &name]);
    let output = command_output(kickstart, "restarting the legacy user service")?;
    ensure_command_success(&output, "restarting the legacy user service")
}

fn stop_user_service(uid: u32, service: InstallService) -> anyhow::Result<()> {
    if matches!(
        user_launchd_job_state(uid, service)?,
        LaunchdJobState::Absent
    ) {
        return Ok(());
    }
    let name = format!("gui/{uid}/{}", service.label());
    let mut bootout = Command::new("/bin/launchctl");
    bootout.args(["bootout", &name]);
    let output = command_output(bootout, "stopping the legacy user service")?;
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    while Instant::now() < deadline {
        if matches!(
            user_launchd_job_state(uid, service)?,
            LaunchdJobState::Absent
        ) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("failed to stop {name}: {}", command_detail(&output))
}

fn wait_for_user_service_running(uid: u32, service: InstallService) -> anyhow::Result<()> {
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    while Instant::now() < deadline {
        if matches!(
            user_launchd_job_state(uid, service)?,
            LaunchdJobState::Loaded {
                running: true,
                pid: Some(_),
                ..
            }
        ) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("{} did not resume after rollback", service.label())
}

fn launchd_job_state(service: InstallService) -> anyhow::Result<LaunchdJobState> {
    launchd_job_state_in_domain("system", service)
}

fn user_launchd_job_state(uid: u32, service: InstallService) -> anyhow::Result<LaunchdJobState> {
    launchd_job_state_in_domain(&format!("gui/{uid}"), service)
}

fn launchd_job_state_in_domain(
    domain: &str,
    service: InstallService,
) -> anyhow::Result<LaunchdJobState> {
    launchd_job_state_for_label(domain, service.label())
}

fn launchd_job_state_for_label(domain: &str, label: &str) -> anyhow::Result<LaunchdJobState> {
    let name = format!("{domain}/{label}");
    let mut command = Command::new("/bin/launchctl");
    command.args(["print", &name]);
    let output = command_output(command, "inspecting the launchd service")?;
    if !output.status.success() {
        let detail = command_detail(&output).to_ascii_lowercase();
        if detail.contains("could not find service")
            || detail.contains("service not found")
            || detail.contains("no such process")
            || detail.contains("domain does not exist")
        {
            return Ok(LaunchdJobState::Absent);
        }
        anyhow::bail!("failed to inspect {name}: {detail}");
    }
    let output = String::from_utf8(output.stdout).context("launchd output was not valid UTF-8")?;
    let program = output
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("program = "))
        .map(PathBuf::from)
        .context("loaded launchd job does not report its executable")?;
    let running = output
        .lines()
        .map(str::trim)
        .any(|line| line == "state = running");
    let pid = output
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("pid = "))
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|pid| *pid > 0);
    Ok(LaunchdJobState::Loaded {
        program,
        running,
        pid,
    })
}

fn read_plist_document(path: &Path) -> anyhow::Result<Value> {
    let mut command = Command::new("/usr/bin/plutil");
    command.args(["-convert", "json", "-o", "-"]).arg(path);
    let direct = command_output(command, "reading the launchd service definition")?;
    let output = if direct.status.success() {
        direct
    } else if OpenOptions::new().read(true).open(path).is_ok() {
        ensure_command_success(&direct, "reading the launchd service definition")?;
        unreachable!("a failed plist read cannot pass success validation")
    } else {
        privileged_output(
            "/usr/bin/plutil",
            &[
                OsStr::new("-convert"),
                OsStr::new("json"),
                OsStr::new("-o"),
                OsStr::new("-"),
                path.as_os_str(),
            ],
        )?
    };
    ensure_command_success(&output, "reading the launchd service definition")?;
    serde_json::from_slice(&output.stdout).context("launchd service definition is invalid")
}

fn require_privileged_access() -> anyhow::Result<()> {
    let output = privileged_output("/usr/bin/true", &[])?;
    anyhow::ensure!(
        output.status.success(),
        "administrator access is not active; run `sudo -v` in this terminal, then retry"
    );
    Ok(())
}

fn privileged_checked(tool: &str, args: &[&OsStr], action: &str) -> anyhow::Result<()> {
    let output = privileged_output(tool, args)?;
    ensure_command_success(&output, action)
}

fn privileged_output(tool: &str, args: &[&OsStr]) -> anyhow::Result<Output> {
    let mut command = Command::new("/usr/bin/sudo");
    command.arg("-n").arg(tool).args(args);
    command_output(command, &format!("running privileged {tool}"))
}

fn command_output(mut command: Command, action: &str) -> anyhow::Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("failed while {action}"))?;
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    loop {
        if child
            .try_wait()
            .with_context(|| format!("failed while {action}"))?
            .is_some()
        {
            return child
                .wait_with_output()
                .with_context(|| format!("failed while {action}"));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("timed out while {action}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn ensure_command_success(output: &Output, action: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        output.status.success(),
        "{action} failed: {}",
        command_detail(output)
    );
    Ok(())
}

fn command_detail(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    if detail.is_empty() {
        format!("exit status {}", output.status)
    } else {
        detail.to_string()
    }
}

fn validate_install_root(root: &Path) -> anyhow::Result<PathBuf> {
    let metadata = fs::symlink_metadata(root)
        .with_context(|| format!("failed to inspect install root {}", root.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
        "install root is not a regular directory: {}",
        root.display()
    );
    anyhow::ensure!(
        metadata.uid() != 0,
        "install root must belong to a non-root account"
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o022 == 0,
        "install root is writable by another account"
    );
    anyhow::ensure!(
        metadata.uid() == current_effective_uid(),
        "run Oore as the account that owns {} without sudo",
        root.display()
    );
    fs::canonicalize(root)
        .with_context(|| format!("failed to resolve install root {}", root.display()))
}

fn service_owner(root: &Path) -> anyhow::Result<ServiceOwner> {
    let uid = fs::symlink_metadata(root)?.uid();
    let mut command = Command::new("/usr/bin/id");
    command.args(["-nu", &uid.to_string()]);
    let output = command_output(command, "resolving the service account")?;
    ensure_command_success(&output, "resolving the service account")?;
    let name = String::from_utf8(output.stdout)
        .context("service account name was not valid UTF-8")?
        .trim()
        .to_string();
    anyhow::ensure!(
        !name.is_empty() && name != "root",
        "service account is invalid"
    );
    let home =
        dirs::home_dir().context("could not determine the service account home directory")?;
    let home_metadata = fs::symlink_metadata(&home)
        .with_context(|| format!("failed to inspect service account home {}", home.display()))?;
    anyhow::ensure!(
        home_metadata.file_type().is_dir()
            && !home_metadata.file_type().is_symlink()
            && home_metadata.uid() == uid,
        "service account home has an unexpected owner or type"
    );
    Ok(ServiceOwner { uid, name, home })
}

fn validate_installed_executable(root: &Path, uid: u32, name: &str) -> anyhow::Result<PathBuf> {
    let path = root.join("bin").join(name);
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to inspect installed executable {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == uid
            && metadata.permissions().mode() & 0o111 != 0
            && metadata.permissions().mode() & 0o022 == 0,
        "installed executable has unsafe ownership or permissions: {}",
        path.display()
    );
    fs::canonicalize(&path).with_context(|| format!("failed to resolve {}", path.display()))
}

fn expected_executable(root: &Path, service: InstallService) -> PathBuf {
    let name = match service {
        InstallService::Daemon => "oored",
        InstallService::Updater => "oore",
        InstallService::Runner => "oore",
        InstallService::Web => "oore-web",
    };
    root.join("bin").join(name)
}

fn expected_launchd_program(root: &Path, service: InstallService) -> PathBuf {
    match service {
        InstallService::Runner => PathBuf::from("/bin/launchctl"),
        InstallService::Daemon | InstallService::Updater | InstallService::Web => {
            expected_executable(root, service)
        }
    }
}

impl ServicePathSnapshot {
    fn capture(definition: &ServiceDefinition) -> anyhow::Result<Self> {
        let mut paths = Vec::new();
        if let Some(state_parent) = definition.state_parent.as_deref() {
            capture_directory_chain(state_parent, &mut paths)?;
        }
        let logs = definition
            .log_path
            .parent()
            .context("service log path has no parent directory")?;
        capture_directory_chain(logs, &mut paths)?;
        capture_prepared_path(&definition.log_path, PreparedPathKind::File, &mut paths)?;
        Ok(Self { paths })
    }

    fn ensure_unchanged(&self, uid: u32) -> anyhow::Result<()> {
        for snapshot in &self.paths {
            match fs::symlink_metadata(&snapshot.path) {
                Ok(metadata) => {
                    anyhow::ensure!(
                        snapshot.existed
                            && metadata.uid() == uid
                            && metadata.permissions().mode() & 0o777 == snapshot.mode.unwrap_or(0)
                            && prepared_path_kind(&metadata) == Some(snapshot.kind),
                        "service path changed before activation: {}",
                        snapshot.path.display()
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    anyhow::ensure!(
                        !snapshot.existed,
                        "service path disappeared before activation: {}",
                        snapshot.path.display()
                    );
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to inspect service path {}", snapshot.path.display())
                    });
                }
            }
        }
        Ok(())
    }

    fn restore(&self, uid: u32) -> anyhow::Result<Vec<PathBuf>> {
        let mut failures = Vec::new();
        let mut preserved = Vec::new();

        for snapshot in self.paths.iter().filter(|snapshot| snapshot.existed) {
            let result = fs::symlink_metadata(&snapshot.path)
                .with_context(|| format!("failed to inspect {}", snapshot.path.display()))
                .and_then(|metadata| {
                    anyhow::ensure!(
                        metadata.uid() == uid
                            && prepared_path_kind(&metadata) == Some(snapshot.kind),
                        "refusing to restore permissions on a changed path: {}",
                        snapshot.path.display()
                    );
                    fs::set_permissions(
                        &snapshot.path,
                        fs::Permissions::from_mode(snapshot.mode.unwrap_or(0)),
                    )
                    .with_context(|| {
                        format!(
                            "failed to restore permissions on {}",
                            snapshot.path.display()
                        )
                    })
                });
            if let Err(error) = result {
                failures.push(format!("{error:#}"));
            }
        }

        for snapshot in self
            .paths
            .iter()
            .filter(|snapshot| !snapshot.existed && snapshot.kind == PreparedPathKind::File)
        {
            let result = match fs::symlink_metadata(&snapshot.path) {
                Ok(metadata) => {
                    if metadata.uid() != uid || prepared_path_kind(&metadata) != Some(snapshot.kind)
                    {
                        Err(anyhow::anyhow!(
                            "refusing to remove a changed path: {}",
                            snapshot.path.display()
                        ))
                    } else {
                        fs::remove_file(&snapshot.path).with_context(|| {
                            format!("failed to remove {}", snapshot.path.display())
                        })
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.into()),
            };
            if let Err(error) = result {
                failures.push(format!("{error:#}"));
            }
        }

        let mut created_directories = self
            .paths
            .iter()
            .filter(|snapshot| !snapshot.existed && snapshot.kind == PreparedPathKind::Directory)
            .collect::<Vec<_>>();
        created_directories
            .sort_by_key(|snapshot| std::cmp::Reverse(snapshot.path.components().count()));
        for snapshot in created_directories {
            match fs::remove_dir(&snapshot.path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                    preserved.push(snapshot.path.clone());
                }
                Err(error) => failures.push(format!(
                    "failed to remove newly created directory {}: {error}",
                    snapshot.path.display()
                )),
            }
        }

        if failures.is_empty() {
            Ok(preserved)
        } else {
            anyhow::bail!(failures.join("; "))
        }
    }
}

fn capture_directory_chain(
    path: &Path,
    snapshots: &mut Vec<PreparedPathSnapshot>,
) -> anyhow::Result<()> {
    let mut current = path.to_path_buf();
    loop {
        match fs::symlink_metadata(&current) {
            Ok(_) => {
                capture_prepared_path(&current, PreparedPathKind::Directory, snapshots)?;
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                capture_prepared_path(&current, PreparedPathKind::Directory, snapshots)?;
                current = current
                    .parent()
                    .context("service directory has no existing ancestor")?
                    .to_path_buf();
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn capture_prepared_path(
    path: &Path,
    kind: PreparedPathKind,
    snapshots: &mut Vec<PreparedPathSnapshot>,
) -> anyhow::Result<()> {
    if snapshots.iter().any(|snapshot| snapshot.path == path) {
        return Ok(());
    }
    let (existed, mode) = match fs::symlink_metadata(path) {
        Ok(metadata) => {
            anyhow::ensure!(
                prepared_path_kind(&metadata) == Some(kind),
                "service path has an unexpected type: {}",
                path.display()
            );
            (true, Some(metadata.permissions().mode() & 0o777))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (false, None),
        Err(error) => return Err(error.into()),
    };
    snapshots.push(PreparedPathSnapshot {
        path: path.to_path_buf(),
        kind,
        existed,
        mode,
    });
    Ok(())
}

fn prepared_path_kind(metadata: &fs::Metadata) -> Option<PreparedPathKind> {
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        Some(PreparedPathKind::Directory)
    } else if metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
        Some(PreparedPathKind::File)
    } else {
        None
    }
}

fn validate_user_directory_preflight(
    path: &Path,
    uid: u32,
    will_secure_existing: bool,
) -> anyhow::Result<()> {
    let mut current = path;
    loop {
        match fs::symlink_metadata(current) {
            Ok(metadata) => {
                anyhow::ensure!(
                    metadata.file_type().is_dir()
                        && !metadata.file_type().is_symlink()
                        && metadata.uid() == uid,
                    "directory has an unexpected owner or type: {}",
                    current.display()
                );
                if current == path && !will_secure_existing {
                    anyhow::ensure!(
                        metadata.permissions().mode() & 0o022 == 0,
                        "existing directory is writable by another account: {}",
                        path.display()
                    );
                }
                if current != path {
                    anyhow::ensure!(
                        metadata.permissions().mode() & 0o022 == 0,
                        "directory ancestor is writable by another account: {}",
                        current.display()
                    );
                }
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                current = current
                    .parent()
                    .context("directory has no existing ancestor")?;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn validate_log_path_preflight(
    root: &Path,
    uid: u32,
    service: InstallService,
) -> anyhow::Result<()> {
    let logs = root.join("logs");
    validate_user_directory_preflight(&logs, uid, true)?;
    let path = service_log_path(root, service);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => anyhow::ensure!(
            metadata.file_type().is_file()
                && !metadata.file_type().is_symlink()
                && metadata.uid() == uid,
            "service log has an unexpected owner or type: {}",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn ensure_user_directory(path: &Path, uid: u32, secure_existing: bool) -> anyhow::Result<()> {
    let existed = path
        .try_exists()
        .with_context(|| format!("failed to inspect directory {}", path.display()))?;
    fs::create_dir_all(path)
        .with_context(|| format!("failed to create directory {}", path.display()))?;
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect directory {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_dir()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == uid,
        "directory has an unexpected owner or type: {}",
        path.display()
    );
    if !existed || secure_existing {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("failed to secure directory {}", path.display()))?;
    } else {
        anyhow::ensure!(
            metadata.permissions().mode() & 0o022 == 0,
            "existing directory is writable by another account: {}",
            path.display()
        );
    }
    Ok(())
}

fn prepare_log_path(root: &Path, uid: u32, service: InstallService) -> anyhow::Result<PathBuf> {
    let logs = root.join("logs");
    ensure_user_directory(&logs, uid, true)?;
    let path = service_log_path(root, service);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .with_context(|| format!("failed to prepare service log {}", path.display()))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.file_type().is_file() && metadata.uid() == uid,
        "service log has an unexpected owner or type: {}",
        path.display()
    );
    Ok(path)
}

fn service_log_path(root: &Path, service: InstallService) -> PathBuf {
    let name = match service {
        InstallService::Daemon => "oored.log",
        InstallService::Updater => "update-supervisor.log",
        InstallService::Runner => "oore-runner.log",
        InstallService::Web => "oore-web.log",
    };
    root.join("logs").join(name)
}

fn verify_daemon_candidate(executable: &Path) -> anyhow::Result<()> {
    let mut command = Command::new(executable);
    command.arg("package-version");
    let output = command_output(command, "checking the installed daemon")?;
    ensure_command_success(&output, "checking the installed daemon")?;
    anyhow::ensure!(
        String::from_utf8_lossy(&output.stdout).trim() == env!("CARGO_PKG_VERSION"),
        "installed daemon package version does not match this CLI"
    );
    Ok(())
}

fn verify_web_candidate(
    root: &Path,
    owner: &ServiceOwner,
    arguments: &[String],
) -> anyhow::Result<()> {
    let executable = arguments.first().context("web service has no executable")?;
    let mut command = Command::new(executable);
    command
        .env_clear()
        .env("HOME", &owner.home)
        .env("PATH", SERVICE_PATH)
        .env("OORE_INSTALL_ROOT", root)
        .arg("validate-config")
        .args(&arguments[2..]);
    let output = command_output(command, "validating the web service configuration")?;
    ensure_command_success(&output, "validating the web service configuration")
}

fn parse_listen_address(listen: &str) -> anyhow::Result<SocketAddr> {
    listen
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid service listen address: {listen}"))
}

fn local_service_url(listen: &str) -> anyhow::Result<Url> {
    let address = parse_listen_address(listen)?;
    let health_ip = match address.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    Url::parse(&format!(
        "http://{}/",
        SocketAddr::new(health_ip, address.port())
    ))
    .context("failed to build local service health URL")
}

pub(super) fn local_daemon_service_url(listen: &str) -> anyhow::Result<Url> {
    let address = parse_listen_address(listen)?;
    let health_ip = match address.ip() {
        IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
    };
    Url::parse(&format!(
        "http://{}/",
        SocketAddr::new(health_ip, address.port())
    ))
    .context("failed to build local daemon health URL")
}

fn validate_web_transport(
    listen: &str,
    backend_url: &str,
    browser_transport_protected: bool,
    backend_transport_protected: bool,
) -> anyhow::Result<()> {
    let listen_url = Url::parse(&format!("http://{listen}/"))
        .with_context(|| format!("invalid web listen address: {listen}"))?;
    let listen_host = listen_url
        .host_str()
        .context("web listen address has no host")?;
    anyhow::ensure!(
        listen_url.port().is_some(),
        "web listen address has no port"
    );
    anyhow::ensure!(
        is_loopback_host(listen_host) || browser_transport_protected,
        "non-loopback HTTP listen requires protected browser transport"
    );

    let backend = Url::parse(backend_url)
        .with_context(|| format!("invalid web backend URL: {backend_url}"))?;
    anyhow::ensure!(
        matches!(backend.scheme(), "http" | "https"),
        "web backend must use HTTP or HTTPS"
    );
    anyhow::ensure!(
        backend.username().is_empty() && backend.password().is_none(),
        "web backend URL must not contain credentials"
    );
    let backend_host = backend.host_str().context("web backend URL has no host")?;
    anyhow::ensure!(
        backend.scheme() == "https"
            || is_loopback_host(backend_host)
            || backend_transport_protected,
        "non-loopback HTTP backend requires protected backend transport"
    );
    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn current_effective_uid() -> u32 {
    // SAFETY: `geteuid` has no pointer arguments or failure state.
    unsafe { libc::geteuid() }
}

fn require_macos() -> anyhow::Result<()> {
    anyhow::ensure!(
        cfg!(target_os = "macos"),
        "managed services are supported on macOS only"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_sudoers_names_only_the_rollback_commands() {
        let rule = render_recovery_sudoers("appbuilder").expect("valid account");
        assert_eq!(
            rule.lines().last(),
            Some(
                "appbuilder ALL=(root) NOPASSWD: /bin/launchctl bootout system/build.oore.oored, /bin/launchctl bootstrap system /Library/LaunchDaemons/build.oore.oored.plist, /bin/launchctl kickstart -k system/build.oore.oored"
            )
        );
        assert!(render_recovery_sudoers("app builder").is_err());
        assert!(render_recovery_sudoers("ALL,root").is_err());
        assert!(render_recovery_sudoers("").is_err());
    }

    #[test]
    fn runner_environment_accepts_the_pre_fvm_removal_service() {
        let root = PathBuf::from("/Users/appbuilder/.oore");
        let owner = ServiceOwner {
            uid: 501,
            name: "appbuilder".to_string(),
            home: PathBuf::from("/Users/appbuilder"),
        };
        let mut environment = serde_json::Map::new();
        environment.insert("PATH".to_string(), SERVICE_PATH.into());
        environment.insert("HOME".to_string(), "/Users/appbuilder".into());
        environment.insert(
            oore_runner::RUNNER_SERVICE_ACK_PATH_ENV.to_string(),
            format!(
                "/Users/appbuilder/.oore/run/{}",
                oore_runner::RUNNER_SERVICE_ACK_FILE
            )
            .into(),
        );
        assert!(validate_runner_environment(&root, &owner, &environment).is_ok());

        let mut legacy = environment.clone();
        legacy.insert(
            "FVM_CACHE_PATH".to_string(),
            "/Users/appbuilder/.oore/toolchains/flutter".into(),
        );
        assert!(validate_runner_environment(&root, &owner, &legacy).is_ok());

        legacy.insert("FVM_CACHE_PATH".to_string(), "/tmp/flutter".into());
        assert!(validate_runner_environment(&root, &owner, &legacy).is_err());

        let mut extra = environment;
        extra.insert("EXTRA".to_string(), "1".into());
        assert!(validate_runner_environment(&root, &owner, &extra).is_err());
    }

    #[test]
    fn backend_update_supervisor_is_demand_only() {
        let root = PathBuf::from("/Users/appbuilder/.oore");
        let definition = update_supervisor_definition(
            root,
            ServiceOwner {
                uid: 501,
                name: "appbuilder".to_string(),
                home: PathBuf::from("/Users/appbuilder"),
            },
        );
        let plist = render_launch_daemon(&definition);

        assert_eq!(definition.service, InstallService::Updater);
        assert_eq!(
            definition.arguments,
            [
                "/Users/appbuilder/.oore/bin/oore",
                "update-supervisor",
                "--request-file",
                "/Users/appbuilder/.oore/run/runtime-update-queue/request.json",
            ]
        );
        assert!(plist.contains("<string>build.oore.oore-updater</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n    <false/>"));
        assert!(plist.contains("<key>KeepAlive</key>\n    <false/>"));
    }
}
