use std::collections::{BTreeSet, HashMap};
use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::{Local, TimeZone};
use clap::{Args, Parser, Subcommand, ValueEnum};
use oore_cli_ui::{PromptResult, SelectChoice, Terminal};
use oore_contract::{
    ApiError, BootstrapTokenRecord, BootstrapTokenVerifyRequest, BootstrapTokenVerifyResponse,
    DeferredRuntimeUpdateRequest, LOCAL_RECOVERY_MAX_TTL_SECS, LOCAL_RECOVERY_MIN_TTL_SECS,
    ListBuildsResponse, ListRunnersResponse, LocalLoginRequest, LocalLoginResponse,
    LocalRecoveryMintRequest, LocalRecoveryMintResponse, ManagedRunnerCapabilities,
    ManagedRunnerRollback, OidcConfigureRequest, OidcConfigureResponse, OperatorRequest,
    OperatorRequestEnvelope, OperatorResponse, RegisterRunnerResponse, RemoteAuthMode, RuntimeMode,
    SetupCompleteResponse, SetupLocalOwnerCreateRequest, SetupOidcStartRequest,
    SetupOidcStartResponse, SetupOidcVerifyRequest, SetupOidcVerifyResponse,
    SetupPreferencesRequest, SetupState, SetupStateFile, SetupStatus, SetupSummaryResponse,
    SetupTrustedProxyConfigureRequest, UserProfileResponse, local_recovery_socket_path,
    parse_repository_pipeline_yaml,
};
use oore_runner::RunnerConfig;
use ring::aead::{self, AES_256_GCM, Aad, BoundKey, NONCE_LEN, Nonce, NonceSequence, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UnixStream};

mod install_lock;
mod install_manifest;
mod installer;
mod managed_services;
mod setup_journey;
mod uninstaller;
mod update_supervisor;

const FAVICON_DATA_URI: &str = "data:image/svg+xml;base64,PHN2ZwogIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIKICB2aWV3Qm94PSIxMDAgMTAwIDMyMCAzMjAiCiAgcm9sZT0iaW1nIgogIGFyaWEtbGFiZWxsZWRieT0idGl0bGUgZGVzY3JpcHRpb24iCj4KICA8dGl0bGUgaWQ9InRpdGxlIj5Pb3JlPC90aXRsZT4KICA8ZGVzYyBpZD0iZGVzY3JpcHRpb24iPgogICAgQSBjaXJjdWxhciBidWlsZCBzd2VlcCBjb25uZWN0aW5nIGEgc291cmNlIGNvbW1pdCB0byBhbiBhcnRpZmFjdC4KICA8L2Rlc2M+CiAgPGRlZnM+CiAgICA8bWFzawogICAgICBpZD0iY3V0b3V0cyIKICAgICAgeD0iMCIKICAgICAgeT0iMCIKICAgICAgd2lkdGg9IjUxMiIKICAgICAgaGVpZ2h0PSI1MTIiCiAgICAgIG1hc2tVbml0cz0idXNlclNwYWNlT25Vc2UiCiAgICA+CiAgICAgIDxyZWN0IHdpZHRoPSI1MTIiIGhlaWdodD0iNTEyIiBmaWxsPSJ3aGl0ZSIgLz4KICAgICAgPGNpcmNsZSBjeD0iMTczIiBjeT0iMzQ5IiByPSIxNiIgZmlsbD0iYmxhY2siIC8+CiAgICAgIDxyZWN0IHg9IjMyOSIgeT0iMTk3IiB3aWR0aD0iNjYiIGhlaWdodD0iMTEiIGZpbGw9ImJsYWNrIiAvPgogICAgPC9tYXNrPgogIDwvZGVmcz4KICA8ZyBmaWxsPSIjYmI0ZDAwIiBtYXNrPSJ1cmwoI2N1dG91dHMpIj4KICAgIDxjaXJjbGUKICAgICAgY3g9IjI1NCIKICAgICAgY3k9IjI2MCIKICAgICAgcj0iMTE5IgogICAgICBmaWxsPSJub25lIgogICAgICBzdHJva2U9IiNiYjRkMDAiCiAgICAgIHN0cm9rZS13aWR0aD0iMjIiCiAgICAvPgogICAgPGNpcmNsZSBjeD0iMTczIiBjeT0iMzQ5IiByPSIzNSIgLz4KICAgIDxyZWN0IHg9IjMxNSIgeT0iMTcwIiB3aWR0aD0iNzkiIGhlaWdodD0iOTAiIHJ4PSIxNSIgLz4KICA8L2c+Cjwvc3ZnPgo=";
const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:8787";
const CONFIG_KEY_DAEMON_URL: &str = "daemon_url";
const CONFIG_KEY_SESSION_TOKEN: &str = "session_token";
const TERMINAL_OIDC_CALLBACK_PORT: u16 = 4174;
const TERMINAL_OIDC_REDIRECT_URI: &str = "http://localhost:4174/auth/callback";

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct CliConfigFile {
    daemon_url: Option<String>,
    session_token: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct StatusSummary {
    daemon_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    web_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    installation_state: Option<install_manifest::InstallState>,
    setup_status: SetupStatus,
    authenticated: bool,
    queue_depth: Option<i64>,
    active_builds: Option<i64>,
    recent_builds: Option<Vec<oore_contract::Build>>,
    runners: Option<Vec<oore_contract::Runner>>,
}

#[derive(Debug, serde::Serialize)]
struct DoctorCheckResult {
    name: String,
    status: String,
    detail: Option<String>,
    install_hint: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct DoctorReport {
    checks: Vec<DoctorCheckResult>,
    missing_count: usize,
    warning_count: usize,
}

#[derive(Debug, Parser)]
#[command(name = "oore")]
#[command(about = "oore operator CLI")]
struct Cli {
    /// Control color in human-readable output.
    #[arg(long, global = true, value_enum, default_value_t = CliColorMode::Auto)]
    color: CliColorMode,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
enum CliColorMode {
    #[default]
    Auto,
    Always,
    Never,
}

impl From<CliColorMode> for oore_cli_ui::ColorMode {
    fn from(value: CliColorMode) -> Self {
        match value {
            CliColorMode::Auto => Self::Auto,
            CliColorMode::Always => Self::Always,
            CliColorMode::Never => Self::Never,
        }
    }
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Choose and install the role for this device
    Install(InstallArgs),
    #[command(hide = true)]
    BootstrapGuard(BootstrapGuardArgs),
    /// Remove this Oore installation
    Uninstall(uninstaller::UninstallArgs),
    /// Configure this device and finish first-run setup
    Setup(Box<SetupArgs>),
    Frontend(FrontendArgs),
    Login(LoginArgs),
    /// Mint a single-use browser recovery link over the local management socket.
    Recovery(RecoveryArgs),
    Status(StatusArgs),
    Runner(RunnerArgs),
    Config(ConfigArgs),
    Doctor(DoctorArgs),
    Pipeline(PipelineArgs),
    /// Print the installed oore version
    Version,
    /// Update oore and oored to the latest release
    Update(UpdateArgs),
    #[command(hide = true)]
    UpdateSupervisor(UpdateSupervisorArgs),
    /// Create, verify, or restore an encrypted-state backup
    Backup(BackupArgs),
}

#[derive(Debug, Args)]
struct InstallArgs {
    /// Device role. Omit this option for guided terminal setup.
    #[arg(long, value_enum)]
    profile: Option<InstallProfile>,

    /// Print the installation plan without changing this device.
    #[arg(long, default_value = "false")]
    plan: bool,

    /// Use one local release archive. This option is for release acceptance.
    #[arg(long, hide = true, value_name = "ARCHIVE")]
    staged_archive: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct BootstrapGuardArgs {
    #[arg(long)]
    target_version: String,

    #[arg(long)]
    target_channel: String,

    #[arg(long)]
    target_repository: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum InstallProfile {
    /// Install the control plane, local web UI, and runner.
    Complete,
    /// Install the control plane without a web UI or runner service.
    ControlPlane,
    /// Use this device only as a build runner.
    Runner,
    /// Install the web UI without a local control plane.
    WebNode,
    /// Add no control plane, web UI, or runner components.
    CliOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallComponent {
    Cli,
    ControlPlane,
    WebUi,
    Runner,
}

#[derive(Debug, PartialEq, Eq)]
struct InstallPlan {
    profile: InstallProfile,
    components: &'static [InstallComponent],
}

impl InstallProfile {
    const ALL: [Self; 5] = [
        Self::Complete,
        Self::ControlPlane,
        Self::Runner,
        Self::WebNode,
        Self::CliOnly,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Complete => "Complete",
            Self::ControlPlane => "Control plane",
            Self::Runner => "Runner",
            Self::WebNode => "Web node",
            Self::CliOnly => "CLI only",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Complete => "Run Oore, its local web UI, and builds on this device.",
            Self::ControlPlane => "Manage pipelines and runners from this device.",
            Self::Runner => "Run builds from a control plane on another device.",
            Self::WebNode => "Serve the web UI for a control plane on another device.",
            Self::CliOnly => "Use the oore command without adding role-specific components.",
        }
    }

    fn plan(self) -> InstallPlan {
        const COMPLETE: &[InstallComponent] = &[
            InstallComponent::Cli,
            InstallComponent::ControlPlane,
            InstallComponent::WebUi,
            InstallComponent::Runner,
        ];
        const CONTROL_PLANE: &[InstallComponent] =
            &[InstallComponent::Cli, InstallComponent::ControlPlane];
        const RUNNER: &[InstallComponent] = &[InstallComponent::Cli, InstallComponent::Runner];
        const WEB_NODE: &[InstallComponent] = &[InstallComponent::Cli, InstallComponent::WebUi];
        const CLI_ONLY: &[InstallComponent] = &[InstallComponent::Cli];

        let components = match self {
            Self::Complete => COMPLETE,
            Self::ControlPlane => CONTROL_PLANE,
            Self::Runner => RUNNER,
            Self::WebNode => WEB_NODE,
            Self::CliOnly => CLI_ONLY,
        };
        InstallPlan {
            profile: self,
            components,
        }
    }
}

impl InstallComponent {
    fn label(self) -> &'static str {
        match self {
            Self::Cli => "Oore CLI",
            Self::ControlPlane => "Control plane",
            Self::WebUi => "Web UI",
            Self::Runner => "Build runner",
        }
    }
}

fn choose_install_profile(terminal: Terminal) -> anyhow::Result<Option<InstallProfile>> {
    let choices = InstallProfile::ALL
        .map(|profile| SelectChoice::new(profile, profile.label(), profile.description()));
    match terminal
        .select(
            "What will this device do?",
            choices,
            InstallProfile::Complete,
        )
        .context("failed to read the device profile")?
    {
        PromptResult::Submitted(profile) => Ok(Some(profile)),
        PromptResult::Cancelled => Ok(None),
    }
}

fn format_install_plan(plan: &InstallPlan) -> String {
    let mut lines = vec![
        format!("Device role   {}", plan.profile.label()),
        String::new(),
        "Components".to_string(),
    ];
    for component in plan.components.iter().copied() {
        let suffix = match component {
            InstallComponent::Cli => " (ready)",
            InstallComponent::Runner => " (built in)",
            InstallComponent::ControlPlane | InstallComponent::WebUi => "",
        };
        lines.push(format!("  {}{suffix}", component.label()));
    }
    lines.join("\n")
}

fn manifest_profile(profile: InstallProfile) -> install_manifest::InstallProfile {
    match profile {
        InstallProfile::Complete => install_manifest::InstallProfile::Complete,
        InstallProfile::ControlPlane => install_manifest::InstallProfile::ControlPlane,
        InstallProfile::Runner => install_manifest::InstallProfile::Runner,
        InstallProfile::WebNode => install_manifest::InstallProfile::WebNode,
        InstallProfile::CliOnly => install_manifest::InstallProfile::CliOnly,
    }
}

fn path_entry_exists(path: &Path) -> anyhow::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => {
            Err(error).with_context(|| format!("failed to inspect legacy path {}", path.display()))
        }
    }
}

fn legacy_service_artifact_exists(label: &str) -> anyhow::Result<bool> {
    let system = PathBuf::from("/Library/LaunchDaemons").join(format!("{label}.plist"));
    if path_entry_exists(&system)? {
        return Ok(true);
    }
    if path_entry_exists(&launch_agent_plist(label)?)? {
        return Ok(true);
    }
    if cfg!(target_os = "macos") {
        return managed_services::launchd_job_exists(label);
    }
    Ok(false)
}

fn legacy_component_artifact_exists(
    install_root: &Path,
    component: install_manifest::InstallComponent,
) -> anyhow::Result<bool> {
    let (paths, service_label) = match component {
        install_manifest::InstallComponent::Cli => return Ok(false),
        install_manifest::InstallComponent::ControlPlane => (
            vec![
                install_root.join("bin/oored"),
                PathBuf::from("/Library/LaunchDaemons/build.oore.oore-updater.plist"),
            ],
            DAEMON_SERVICE_LABEL,
        ),
        install_manifest::InstallComponent::Runner => (
            vec![
                install_root.join("runner.json"),
                install_root.join("managed-runner.json"),
                install_root.join(oore_runner::RUNNER_RELEASE_MARKER_FILE),
            ],
            RUNNER_SERVICE_LABEL,
        ),
        install_manifest::InstallComponent::Web => (
            vec![
                install_root.join("bin/oore-web"),
                install_root.join("web-dist"),
                install_root.join("WEB_VERSION"),
            ],
            WEB_SERVICE_LABEL,
        ),
    };

    for path in paths {
        if path_entry_exists(&path)? {
            return Ok(true);
        }
    }
    if component == install_manifest::InstallComponent::ControlPlane
        && cfg!(target_os = "macos")
        && managed_services::launchd_job_exists(UPDATER_SERVICE_LABEL)?
    {
        return Ok(true);
    }
    legacy_service_artifact_exists(service_label)
}

fn legacy_live_component_artifact_exists(
    install_root: &Path,
    component: install_manifest::InstallComponent,
) -> anyhow::Result<bool> {
    let (paths, service_label) = match component {
        install_manifest::InstallComponent::Cli => return Ok(false),
        install_manifest::InstallComponent::ControlPlane => (
            vec![
                install_root.join("bin/oored"),
                PathBuf::from("/Library/LaunchDaemons/build.oore.oore-updater.plist"),
            ],
            DAEMON_SERVICE_LABEL,
        ),
        install_manifest::InstallComponent::Runner => (Vec::new(), RUNNER_SERVICE_LABEL),
        install_manifest::InstallComponent::Web => (
            vec![
                install_root.join("bin/oore-web"),
                install_root.join("web-dist"),
                install_root.join("WEB_VERSION"),
            ],
            WEB_SERVICE_LABEL,
        ),
    };

    for path in paths {
        if path_entry_exists(&path)? {
            return Ok(true);
        }
    }
    if component == install_manifest::InstallComponent::ControlPlane
        && cfg!(target_os = "macos")
        && managed_services::launchd_job_exists(UPDATER_SERVICE_LABEL)?
    {
        return Ok(true);
    }
    legacy_service_artifact_exists(service_label)
}

fn validate_install_profile_transition(
    install_root: &Path,
    requested: install_manifest::InstallProfile,
) -> anyhow::Result<Option<install_manifest::InstallManifest>> {
    let path = install_root.join("install-manifest.json");
    let existing = match fs::symlink_metadata(&path) {
        Ok(_) => Some(install_manifest::InstallManifest::load(&path)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    };
    if let Some(existing) = existing.as_ref() {
        anyhow::ensure!(
            !(existing.profile == install_manifest::InstallProfile::Runner
                && requested == install_manifest::InstallProfile::Complete),
            "a Runner profile cannot be changed directly to Complete because its service is registered with another control plane; run `oore uninstall --purge`, rerun the CLI bootstrap, then choose the Complete profile"
        );
        let removes_component = existing
            .components
            .iter()
            .any(|component| !requested.components().contains(component));
        anyhow::ensure!(
            !removes_component,
            "changing this device to that role would remove an installed component; back up required data, run `oore uninstall --purge`, rerun the CLI bootstrap, then choose the new role"
        );
    } else {
        let mut live_components = Vec::new();
        for (component, label) in [
            (
                install_manifest::InstallComponent::ControlPlane,
                "control plane",
            ),
            (install_manifest::InstallComponent::Runner, "runner"),
            (install_manifest::InstallComponent::Web, "web"),
        ] {
            if legacy_live_component_artifact_exists(install_root, component)? {
                live_components.push(label);
            }
        }
        anyhow::ensure!(
            live_components.is_empty(),
            "a pre-manifest Oore installation still has live {} components or services. v0.1.42 cannot replace them safely. Run the verified v0.1.42 bootstrap and accept `oore uninstall --legacy-v0-1-41` when offered. This preserves Oore data. Then rerun the bootstrap and install the same role",
            live_components.join(", ")
        );

        let mut conflicts = Vec::new();
        for (component, label) in [
            (
                install_manifest::InstallComponent::ControlPlane,
                "control plane",
            ),
            (install_manifest::InstallComponent::Runner, "runner"),
            (install_manifest::InstallComponent::Web, "web"),
        ] {
            if !requested.components().contains(&component)
                && legacy_component_artifact_exists(install_root, component)?
            {
                conflicts.push(label);
            }
        }
        anyhow::ensure!(
            conflicts.is_empty(),
            "existing preserved or legacy component data was found for {}; choose a role that includes it, or remove it with `oore uninstall --purge` before changing roles",
            conflicts.join(", ")
        );
    }
    Ok(existing)
}

fn manifest_owns_non_cli_component(manifest: &install_manifest::InstallManifest) -> bool {
    manifest
        .components
        .iter()
        .any(|component| *component != install_manifest::InstallComponent::Cli)
}

fn release_lineage_matches(
    recorded: &install_manifest::InstallRelease,
    current: &installer::ReleaseProvenance,
) -> bool {
    recorded.version == current.version
        && recorded.channel == current.channel
        && recorded.repository == current.repository
}

fn release_identity_matches(
    recorded: &install_manifest::InstallRelease,
    current: &installer::ReleaseProvenance,
) -> bool {
    release_lineage_matches(recorded, current)
        && recorded.archive == current.archive
        && recorded.sha256 == current.sha256
}

fn release_boundary_error(
    manifest: &install_manifest::InstallManifest,
    detail: &str,
) -> anyhow::Error {
    let profile = match manifest.profile {
        install_manifest::InstallProfile::Complete => "complete",
        install_manifest::InstallProfile::ControlPlane => "control-plane",
        install_manifest::InstallProfile::Runner => "runner",
        install_manifest::InstallProfile::WebNode => "web-node",
        install_manifest::InstallProfile::CliOnly => "cli-only",
    };
    anyhow::anyhow!(
        "{detail}.\nv0.1.42 cannot replace live profile components across releases.\nRun `oore uninstall` without `--purge`. This preserves Oore data.\nThen rerun the CLI bootstrap and run `oore install --profile {profile}`."
    )
}

fn missing_install_payload(
    requested: install_manifest::InstallProfile,
    previous: Option<&install_manifest::InstallManifest>,
) -> installer::PayloadSelection {
    let has_component =
        |component| previous.is_some_and(|manifest| manifest.components.contains(&component));
    installer::PayloadSelection::new(
        requested
            .components()
            .contains(&install_manifest::InstallComponent::ControlPlane)
            && !has_component(install_manifest::InstallComponent::ControlPlane),
        requested
            .components()
            .contains(&install_manifest::InstallComponent::Web)
            && !has_component(install_manifest::InstallComponent::Web),
    )
}

fn format_install_outcome(plan: &InstallPlan, version: &str) -> String {
    let components = plan
        .components
        .iter()
        .map(|component| format!("  {}", component.label()))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Device role   {}\nRelease       {}\n\nComponents\n{}",
        plan.profile.label(),
        version,
        components
    )
}

fn verify_existing_profile(
    install_root: &Path,
    profile: InstallProfile,
    plan: &InstallPlan,
    version: &str,
    terminal: Terminal,
) -> anyhow::Result<()> {
    let operation = terminal.operation("Checking components");
    if let Err(error) = installer::verify_profile(install_root, profile) {
        operation.failed("Installation needs repair");
        return Err(error);
    }
    operation.done("Existing component files verified");
    terminal.note("Verified", format_install_outcome(plan, version))?;
    Ok(())
}

async fn continue_to_device_setup(
    install_lock: install_lock::InstallLock,
    terminal: Terminal,
) -> anyhow::Result<()> {
    if !terminal.is_interactive() {
        terminal.outro("Component files are ready. Next: run `oore setup --help`.")?;
        return Ok(());
    }

    terminal.outro("Component files are ready. Continuing to device setup.")?;
    drop(install_lock);
    setup_journey::handle(SetupArgs::guided_defaults(), terminal).await
}

async fn handle_install(args: InstallArgs, terminal: Terminal) -> anyhow::Result<()> {
    terminal.intro("Install")?;
    let profile = match args.profile {
        Some(profile) => profile,
        None if terminal.is_interactive() => match choose_install_profile(terminal)? {
            Some(profile) => profile,
            None => return Ok(()),
        },
        None => anyhow::bail!(
            "no interactive terminal was detected; choose a device role with --profile\nRun 'oore install --help' to see the available roles."
        ),
    };
    let plan = profile.plan();
    terminal.note("Installation plan", format_install_plan(&plan))?;

    if args.plan {
        terminal.outro("No changes were made.")?;
        return Ok(());
    }

    let install_root = resolve_install_root()?;
    let install_lock = install_lock::InstallLock::acquire(&install_root)?;
    let persisted_profile = manifest_profile(profile);
    let previous_manifest = validate_install_profile_transition(&install_root, persisted_profile)?;
    let bootstrap_release = installer::read_bootstrap_release(&install_root)?;
    if let Some(previous) = previous_manifest.as_ref()
        && manifest_owns_non_cli_component(previous)
        && !release_lineage_matches(&previous.release, &bootstrap_release)
    {
        return Err(release_boundary_error(
            previous,
            "the bootstrap release does not match the installed profile release",
        ));
    }

    if args.staged_archive.is_none()
        && previous_manifest.as_ref().is_some_and(|previous| {
            previous.profile == persisted_profile
                && release_lineage_matches(&previous.release, &bootstrap_release)
        })
    {
        let previous = previous_manifest
            .as_ref()
            .context("the existing installation manifest disappeared")?;
        verify_existing_profile(
            &install_root,
            profile,
            &plan,
            &previous.release.version,
            terminal,
        )?;
        return continue_to_device_setup(install_lock, terminal).await;
    }

    let payload = missing_install_payload(persisted_profile, previous_manifest.as_ref());
    let shell_path_files = installer::shell_path_files(&install_root)?;
    let operation = terminal.operation("Preparing components");
    let prepared = match installer::prepare(
        profile,
        &install_root,
        args.staged_archive.as_deref(),
        &operation,
    )
    .await
    {
        Ok(prepared) => prepared,
        Err(error) => {
            operation.failed("Installation stopped");
            return Err(error);
        }
    };
    if let Some(previous) = previous_manifest.as_ref()
        && manifest_owns_non_cli_component(previous)
    {
        let prepared_release = prepared.provenance();
        if !release_lineage_matches(&previous.release, prepared_release) {
            operation.failed("Installation stopped");
            return Err(release_boundary_error(
                previous,
                "the prepared release does not match the installed profile release",
            ));
        }
        if previous.profile == persisted_profile
            && !release_identity_matches(&previous.release, prepared_release)
        {
            operation.failed("Installation stopped");
            return Err(release_boundary_error(
                previous,
                "the prepared component archive does not match the installed profile archive",
            ));
        }
        if previous.profile != persisted_profile
            && previous.release.archive == prepared_release.archive
            && previous.release.sha256 != prepared_release.sha256
        {
            operation.failed("Installation stopped");
            return Err(release_boundary_error(
                previous,
                "the prepared component archive checksum does not match the installed profile archive",
            ));
        }
        if previous.profile == persisted_profile {
            operation.done("Release archive verified");
            verify_existing_profile(
                &install_root,
                profile,
                &plan,
                &previous.release.version,
                terminal,
            )?;
            return continue_to_device_setup(install_lock, terminal).await;
        }
    }
    operation.update("Installing components");
    let manifest_path = install_root.join("install-manifest.json");
    let release = match installer::apply(&prepared, profile, payload, &install_root, |release| {
        let release = install_manifest::InstallRelease::new(
            release.version.clone(),
            release.channel.clone(),
            release.repository.clone(),
            release.archive.clone(),
            release.sha256.clone(),
        )?;
        let mut manifest = install_manifest::InstallManifest::new(
            persisted_profile,
            release,
            shell_path_files.clone(),
        )?;
        if let Some(previous) = previous_manifest.as_ref() {
            manifest.lifecycle = previous.lifecycle.clone();
            if previous.profile == install_manifest::InstallProfile::WebNode
                && persisted_profile == install_manifest::InstallProfile::Complete
            {
                manifest.lifecycle.web_listen = None;
                manifest.lifecycle.browser_transport_protected = false;
            }
            manifest
                .lifecycle
                .services
                .retain(|service| persisted_profile.services().contains(&service.service));
        }
        manifest.lifecycle.state = install_manifest::InstallState::Configuring;
        manifest.write_atomic(&manifest_path)
    }) {
        Ok(release) => release,
        Err(error) => {
            operation.failed("Installation stopped");
            return Err(error);
        }
    };
    operation.done("Component files installed");
    terminal.note("Installed", format_install_outcome(&plan, &release.version))?;
    continue_to_device_setup(install_lock, terminal).await
}

#[derive(Debug, Args)]
struct FrontendArgs {
    #[command(subcommand)]
    command: FrontendSubcommand,
}

#[derive(Debug, Subcommand)]
enum FrontendSubcommand {
    /// Create a short-lived, single-use frontend pairing code.
    Invite(FrontendInviteArgs),
}

#[derive(Debug, Args)]
struct FrontendInviteArgs {
    /// How long the pairing code remains valid.
    #[arg(long, default_value = "10m")]
    ttl: String,

    /// Path to the setup database file.
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,

    /// Print machine-readable output.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
struct BackupArgs {
    #[command(subcommand)]
    command: BackupSubcommand,
}

#[derive(Debug, Subcommand)]
enum BackupSubcommand {
    /// Create a consistent SQLite snapshot and package it with the encryption key.
    Create(BackupCreateArgs),
    /// Verify a backup manifest, checksums, and SQLite integrity.
    Verify(BackupVerifyArgs),
    /// Atomically restore a verified backup while the daemon is stopped.
    Restore(BackupRestoreArgs),
}

#[derive(Debug, Args)]
struct BackupCreateArgs {
    /// Destination .tar.gz file.
    #[arg(long)]
    output: PathBuf,
    /// Path to the SQLite state file.
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,
    /// Path to the encryption key. This option is reserved for update recovery.
    #[arg(long, hide = true)]
    key_file: Option<PathBuf>,
    /// Parent-owned staging directory. This option is reserved for updates.
    #[arg(long, hide = true)]
    stage_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct BackupVerifyArgs {
    /// Backup .tar.gz file.
    #[arg(long)]
    input: PathBuf,
}

#[derive(Debug, Args)]
struct BackupRestoreArgs {
    /// Backup .tar.gz file.
    #[arg(long)]
    input: PathBuf,
    /// Path to the SQLite state file to restore.
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,
    /// Path to the encryption key. This option is reserved for update recovery.
    #[arg(long, hide = true)]
    key_file: Option<PathBuf>,
    /// Parent-owned staging directory. This option is reserved for updates.
    #[arg(long, hide = true)]
    stage_dir: Option<PathBuf>,
    /// Prepare verified restore files without mutating live state.
    #[arg(long, hide = true, requires = "stage_dir")]
    prepare_only: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct BackupManifest {
    format: String,
    created_at: i64,
    files: HashMap<String, String>,
}

#[derive(Debug, Args)]
struct PipelineArgs {
    #[command(subcommand)]
    command: PipelineSubcommand,
}

#[derive(Debug, Subcommand)]
enum PipelineSubcommand {
    /// Validate repository pipeline YAML using the runner's schema.
    Validate(PipelineValidateArgs),
}

#[derive(Debug, Args)]
struct PipelineValidateArgs {
    /// Repository pipeline file to validate.
    #[arg(default_value = ".oore.yaml")]
    path: PathBuf,
}

fn handle_pipeline_validate(args: PipelineValidateArgs) -> anyhow::Result<()> {
    let raw = fs::read_to_string(&args.path)
        .with_context(|| format!("failed to read {}", args.path.display()))?;
    parse_repository_pipeline_yaml(&raw).map_err(anyhow::Error::msg)?;
    println!("{} is valid", args.path.display());
    Ok(())
}

#[derive(Debug, Args)]
struct UpdateArgs {
    /// Only check for updates, don't install
    #[arg(long, default_value = "false")]
    check: bool,

    /// Reinstall even if already on the latest version
    #[arg(long, default_value = "false")]
    force: bool,

    /// Release channel to update from: stable, beta, alpha
    ///
    /// If not specified, this defaults to the installed channel (if available) or infers it from
    /// the installed VERSION file; falls back to stable.
    #[arg(long)]
    channel: Option<String>,

    /// Move this installation to the --channel release channel in place
    ///
    /// The target release must be the same as or newer than the installed version.
    #[arg(long, default_value = "false", requires = "channel")]
    switch_channel: bool,

    /// GitHub repository in `owner/name` format
    ///
    /// If not specified, this defaults to the installed GITHUB_REPO file (if available) or
    /// `oore-ci/oore.build`.
    #[arg(long, env = "OORE_GITHUB_REPO")]
    repo: Option<String>,

    /// Install an already verified and extracted release using the normal update transaction.
    #[arg(long, hide = true, value_name = "DIRECTORY", conflicts_with = "check")]
    staged_release: Option<PathBuf>,

    /// Migrate a legacy embedded runner into the managed local runner service.
    #[arg(long, hide = true, requires = "staged_release")]
    ensure_managed_runner: bool,

    #[arg(long, hide = true)]
    deferred_parent_pid: Option<u32>,

    #[arg(long, hide = true)]
    deferred_state_file: Option<PathBuf>,

    #[arg(long, hide = true)]
    deferred_key_file: Option<PathBuf>,

    #[arg(long, hide = true)]
    deferred_daemon_url: Option<String>,

    #[arg(long, hide = true)]
    deferred_status_file: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct UpdateSupervisorArgs {
    #[arg(long)]
    request_file: PathBuf,
}

#[derive(Debug, Args)]
struct SetupArgs {
    #[arg(long, env = "OORE_DAEMON_URL")]
    daemon_url: Option<String>,

    /// Read setup values from a YAML or JSON file.
    #[arg(long = "config", value_name = "PATH")]
    config_file: Option<PathBuf>,

    /// Show the setup plan without changing this device.
    #[arg(long, default_value = "false")]
    plan: bool,

    /// Print the final setup state as JSON.
    #[arg(long, default_value = "false")]
    json: bool,

    /// Path to the local control-plane database.
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,

    /// Choose the setup interface. Auto uses the terminal over SSH.
    #[arg(long, value_enum, default_value_t = SetupInterface::Auto)]
    interface: SetupInterface,

    /// Choose an access outcome without a prompt.
    #[arg(long, value_enum)]
    access: Option<SetupAccess>,

    /// Initial owner email for terminal setup.
    #[arg(long)]
    owner_email: Option<String>,

    /// Address for the managed control-plane service.
    #[arg(long, default_value = "127.0.0.1:8787")]
    daemon_listen: String,

    /// Address for the local web service.
    #[arg(long, default_value = "127.0.0.1:4173")]
    web_listen: String,

    /// Control-plane URL for a Runner, Web node, or CLI-only profile.
    #[arg(long)]
    backend_url: Option<String>,

    /// Oore API or session token used to register a Runner profile.
    #[arg(long, env = "OORE_SESSION_TOKEN")]
    runner_token: Option<String>,

    /// Name used to register a Runner profile.
    #[arg(long)]
    runner_name: Option<String>,

    /// Single-use code from `oore frontend invite` for a Web node.
    #[arg(long)]
    pairing_code: Option<String>,

    /// Confirm that non-loopback browser traffic has separate encrypted ingress.
    #[arg(long, default_value = "false")]
    browser_transport_protected: bool,

    /// Confirm that non-loopback control-plane traffic uses a protected private network.
    #[arg(long, default_value = "false")]
    backend_transport_protected: bool,

    #[command(subcommand)]
    command: Option<SetupSubcommand>,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
enum SetupInterface {
    #[default]
    Auto,
    Terminal,
    Browser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
enum SetupAccess {
    /// Keep this instance on loopback. No identity provider is required.
    ThisDevice,
    /// Let users sign in through an OIDC identity provider.
    IdentityProvider,
    /// Accept identity from an existing trusted access proxy.
    TrustedProxy,
}

impl SetupArgs {
    fn guided_defaults() -> Self {
        Self {
            daemon_url: None,
            config_file: None,
            plan: false,
            json: false,
            state_file: None,
            interface: SetupInterface::Auto,
            access: None,
            owner_email: None,
            daemon_listen: "127.0.0.1:8787".to_string(),
            web_listen: "127.0.0.1:4173".to_string(),
            backend_url: None,
            runner_token: None,
            runner_name: None,
            pairing_code: None,
            browser_transport_protected: false,
            backend_transport_protected: false,
            command: None,
        }
    }
}

#[derive(Debug, Subcommand)]
enum SetupSubcommand {
    /// Initialize setup directly on the backend host
    Init(SetupInitArgs),
    /// Generate a bootstrap token for web UI setup
    Token(SetupTokenArgs),
    /// Alias for 'token' (deprecated, use 'token' instead)
    #[command(hide = true)]
    Open(SetupTokenArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SetupInitMode {
    /// Loopback-only local mode; creates the local owner and completes setup.
    Local,
    /// Remote mode where an upstream trusted proxy authenticates users.
    TrustedProxy,
}

#[derive(Debug, Args)]
struct SetupInitArgs {
    /// Setup mode to initialize.
    #[arg(long, value_enum)]
    mode: SetupInitMode,

    /// Initial owner email.
    #[arg(long)]
    owner_email: String,

    /// Trusted proxy identity header. Required only for trusted-proxy mode.
    #[arg(long, default_value = "x-oore-user-email")]
    user_email_header: String,

    /// Trusted proxy peer CIDR. Repeat for multiple frontend/proxy networks.
    #[arg(long = "trusted-proxy-cidr")]
    trusted_proxy_cidrs: Vec<String>,

    /// Shared secret expected from the trusted proxy/oore-web hop.
    #[arg(long, env = "OORE_TRUSTED_PROXY_SHARED_SECRET")]
    shared_secret: Option<String>,

    /// File containing the shared secret expected from the trusted proxy/oore-web hop.
    #[arg(long, env = "OORE_TRUSTED_PROXY_SHARED_SECRET_FILE")]
    shared_secret_file: Option<String>,

    /// Path to the setup database file.
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,

    /// Replace an unfinished access choice before owner creation.
    #[arg(long, default_value = "false")]
    force: bool,

    /// Print machine-readable output.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
struct SetupTokenArgs {
    #[arg(long, default_value = "15m")]
    ttl: String,

    #[arg(long, default_value = "false")]
    json: bool,

    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,
}

#[derive(Debug, Args)]
struct StatusArgs {
    #[arg(long, env = "OORE_DAEMON_URL")]
    daemon_url: Option<String>,

    /// Session token for authenticated status details.
    /// If omitted, falls back to OORE_SESSION_TOKEN or stored CLI config.
    #[arg(long, env = "OORE_SESSION_TOKEN")]
    token: Option<String>,

    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
struct LoginArgs {
    #[arg(long, env = "OORE_DAEMON_URL")]
    daemon_url: Option<String>,

    /// Import an existing session token and validate it against /v1/users/me.
    #[arg(long)]
    token: Option<String>,

    /// Optional local-login email (local mode). Defaults to owner@local server-side.
    #[arg(long)]
    email: Option<String>,

    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
struct RecoveryArgs {
    /// Account to recover. Required when more than one active account exists.
    #[arg(long)]
    email: Option<String>,

    /// Web UI base URL placed before the non-leaking recovery fragment.
    #[arg(long, env = "OORE_WEB_URL", default_value = "http://127.0.0.1:4173")]
    web_url: String,

    /// How long the single-use capability remains valid (maximum 5m).
    #[arg(long, default_value = "5m")]
    ttl: String,

    /// Path to the setup database file used to locate the management socket.
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,

    /// Print machine-readable output.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
struct RunnerArgs {
    #[command(subcommand)]
    command: RunnerSubcommand,
}

#[derive(Debug, Subcommand)]
enum RunnerSubcommand {
    /// Register this host as a build runner
    Register(RunnerRegisterArgs),
    /// Start the runner daemon (poll for jobs, execute builds)
    Start(RunnerStartArgs),
    /// Install and start the runner as a boot-time macOS service
    InstallService(RunnerServiceArgs),
    /// Rotate this device's managed local runner token
    RotateToken,
    /// Stop and remove the managed macOS runner service
    UninstallService,
}

#[derive(Debug, Args)]
struct RunnerRegisterArgs {
    /// Daemon URL to register with
    #[arg(long, env = "OORE_DAEMON_URL", default_value = "http://127.0.0.1:8787")]
    daemon_url: String,
    /// Runner name (defaults to hostname)
    #[arg(long)]
    name: Option<String>,
    /// Extra label for pipelines to select with `runs_on`, added to the
    /// detected labels such as `macos-15` and `xcode-16`. Repeatable.
    #[arg(long = "label")]
    labels: Vec<String>,
    /// Auth session token for registration
    #[arg(long, env = "OORE_SESSION_TOKEN")]
    token: String,
}

#[derive(Debug, Args)]
struct RunnerStartArgs {
    /// Daemon URL (loaded from config if not specified)
    #[arg(long, env = "OORE_DAEMON_URL")]
    daemon_url: Option<String>,
    /// Path to runner config file
    #[arg(long, default_value = "~/.oore/runner.json")]
    config: Option<String>,
}

#[derive(Debug, Args)]
struct RunnerServiceArgs {
    /// Path to runner config file
    #[arg(long)]
    config: Option<String>,
    /// Install or repair the runner managed by this local Oore backend
    #[arg(long, default_value = "false")]
    managed_local: bool,
    /// Daemon URL stored when this local installation is enrolled
    #[arg(long, env = "OORE_DAEMON_URL")]
    daemon_url: Option<String>,
    /// Path to the local Oore SQLite database
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,
    /// Managed runner name (defaults to hostname)
    #[arg(long)]
    name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ManagedRunnerCredentialAction {
    Reuse,
    Rotate { expected_runner_id: String },
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
enum ConfigSubcommand {
    Set(ConfigSetArgs),
    Get(ConfigGetArgs),
}

#[derive(Debug, Args)]
struct ConfigSetArgs {
    key: String,
    value: String,
}

#[derive(Debug, Args)]
struct ConfigGetArgs {
    key: String,
}

#[derive(Debug, Args)]
struct DoctorArgs {
    #[arg(long, default_value = "false")]
    json: bool,

    /// Check requirements for a build platform. Repeat for multiple platforms.
    #[arg(long, value_enum)]
    platform: Vec<DoctorPlatform>,

    /// Check Android, iOS, and macOS requirements.
    #[arg(long, conflicts_with = "platform")]
    all: bool,

    /// Run the one-click backend update preflight that the web UI shows.
    /// Run it as the account that owns the install, without sudo.
    #[arg(long, conflicts_with_all = ["platform", "all"])]
    update: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DoctorPlatform {
    Android,
    Ios,
    Macos,
}

fn parse_ttl(raw: &str) -> anyhow::Result<Duration> {
    humantime::parse_duration(raw).with_context(|| format!("invalid ttl value: {raw}"))
}

fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn resolve_data_dir() -> anyhow::Result<PathBuf> {
    oore_contract::data_paths::resolve_data_dir(|key| std::env::var(key).ok(), dirs::data_dir())
        .context("could not determine platform data directory (dirs::data_dir)")
}

fn resolve_db_path(override_path: Option<&str>) -> anyhow::Result<PathBuf> {
    if let Some(p) = override_path {
        return Ok(PathBuf::from(p));
    }

    if let Ok(p) = std::env::var("OORE_SETUP_STATE_FILE") {
        return Ok(PathBuf::from(p));
    }

    Ok(resolve_data_dir()?.join("oore.db"))
}

fn recovery_socket_path(override_path: Option<&str>) -> anyhow::Result<PathBuf> {
    let database_path = resolve_db_path(override_path)?;
    local_recovery_socket_path(&database_path, current_effective_uid()).with_context(|| {
        format!(
            "failed to resolve management socket path for {}",
            database_path.display()
        )
    })
}

fn current_effective_uid() -> u32 {
    // SAFETY: `geteuid` has no arguments, pointer requirements, or failure state.
    unsafe { libc::geteuid() }
}

fn validate_recovery_socket(path: &Path) -> anyhow::Result<()> {
    let expected_uid = current_effective_uid();
    let parent = path
        .parent()
        .context("management socket path has no parent directory")?;
    let parent_metadata = fs::symlink_metadata(parent).with_context(|| {
        format!(
            "failed to inspect management socket directory {}",
            parent.display()
        )
    })?;
    if parent_metadata.file_type().is_symlink()
        || !parent_metadata.is_dir()
        || parent_metadata.uid() != expected_uid
        || parent_metadata.mode() & 0o7777 != 0o700
    {
        anyhow::bail!(
            "management socket directory {} must be owned by the current user with mode 0700",
            parent.display()
        );
    }

    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect management socket {}", path.display()))?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_socket()
        || metadata.uid() != expected_uid
        || metadata.mode() & 0o7777 != 0o600
    {
        anyhow::bail!(
            "management socket {} must be owned by the current user with mode 0600",
            path.display()
        );
    }
    Ok(())
}

fn operator_response_instance_id(response: &OperatorResponse) -> Option<&str> {
    match response {
        OperatorResponse::BootstrapToken { instance_id, .. }
        | OperatorResponse::FrontendInvite { instance_id, .. }
        | OperatorResponse::ManagedRunner { instance_id, .. }
        | OperatorResponse::ManagedRunnerRestored { instance_id, .. }
        | OperatorResponse::RegistrationMatch { instance_id, .. }
        | OperatorResponse::RunnerStatus { instance_id, .. }
        | OperatorResponse::Barrier { instance_id, .. }
        | OperatorResponse::BarrierWait { instance_id, .. } => Some(instance_id),
        OperatorResponse::Error { .. } => None,
    }
}

async fn call_operator(
    daemon_url: &str,
    state_file: Option<&str>,
    request: OperatorRequest,
) -> anyhow::Result<OperatorResponse> {
    let daemon = url::Url::parse(daemon_url).context("invalid daemon URL")?;
    let host = daemon
        .host_str()
        .context("daemon URL must include a host")?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    anyhow::ensure!(
        daemon.scheme() == "http"
            && loopback
            && daemon.username().is_empty()
            && daemon.password().is_none()
            && daemon.query().is_none()
            && daemon.fragment().is_none()
            && matches!(daemon.path(), "" | "/"),
        "host-authorized operator actions require an HTTP loopback daemon URL without credentials, a path, query, or fragment"
    );

    let client = endpoint_http_client_builder(daemon_url)
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to prepare the operator connection")?;
    let status = fetch_setup_status(&client, daemon_url).await?;
    let request = OperatorRequestEnvelope {
        expected_instance_id: status.instance_id.clone(),
        request,
    };
    let socket_path = recovery_socket_path(state_file)?;
    validate_recovery_socket(&socket_path)?;
    let mut stream = UnixStream::connect(&socket_path).await.with_context(|| {
        format!(
            "failed to connect to oored management socket {}",
            socket_path.display()
        )
    })?;
    let mut encoded = serde_json::to_vec(&request).context("failed to encode operator request")?;
    encoded.push(b'\n');
    stream
        .write_all(&encoded)
        .await
        .context("failed to write operator request")?;
    stream
        .shutdown()
        .await
        .context("failed to finish operator request")?;

    const MAX_OPERATOR_RESPONSE_BYTES: u64 = 16 * 1024;
    let mut response_bytes = Vec::new();
    stream
        .take(MAX_OPERATOR_RESPONSE_BYTES + 1)
        .read_to_end(&mut response_bytes)
        .await
        .context("failed to read operator response")?;
    anyhow::ensure!(
        response_bytes.len() as u64 <= MAX_OPERATOR_RESPONSE_BYTES,
        "operator response exceeded {MAX_OPERATOR_RESPONSE_BYTES} bytes"
    );
    let response: OperatorResponse =
        serde_json::from_slice(&response_bytes).context("invalid oored operator response")?;
    if let OperatorResponse::Error { code, message } = &response {
        anyhow::bail!("oored operator request failed [{code}]: {message}");
    }
    let instance_id = operator_response_instance_id(&response)
        .context("oored returned an operator response without an instance identity")?;
    anyhow::ensure!(
        instance_id == status.instance_id,
        "daemon instance changed while the operator request was running; retry the command"
    );
    Ok(response)
}

fn read_env_trimmed(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn resolve_cli_config_path() -> anyhow::Result<PathBuf> {
    if let Some(path) = read_env_trimmed("OORE_CONFIG_FILE") {
        return Ok(PathBuf::from(path));
    }
    Ok(resolve_install_root()?.join("config.json"))
}

fn load_cli_config() -> anyhow::Result<CliConfigFile> {
    let path = resolve_cli_config_path()?;
    if !path.exists() {
        return Ok(CliConfigFile::default());
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read CLI config {}", path.display()))?;
    let cfg = serde_json::from_str::<CliConfigFile>(&raw)
        .with_context(|| format!("failed to parse CLI config {}", path.display()))?;
    Ok(cfg)
}

fn save_cli_config(cfg: &CliConfigFile) -> anyhow::Result<PathBuf> {
    let path = resolve_cli_config_path()?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create CLI config directory {}", parent.display()))?;

    let mut data = serde_json::to_vec_pretty(cfg).context("failed to serialize CLI config")?;
    data.push(b'\n');
    let mut staged = tempfile::Builder::new()
        .prefix(".oore-config-")
        .tempfile_in(parent)
        .with_context(|| format!("failed to stage CLI config in {}", parent.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        staged
            .as_file()
            .set_permissions(perms)
            .context("failed to set CLI config permissions")?;
    }

    staged
        .write_all(&data)
        .with_context(|| format!("failed to write staged CLI config for {}", path.display()))?;
    staged
        .as_file_mut()
        .sync_all()
        .with_context(|| format!("failed to sync staged CLI config for {}", path.display()))?;
    let published = staged
        .persist(&path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to publish CLI config {}", path.display()))?;
    published
        .sync_all()
        .with_context(|| format!("failed to sync CLI config {}", path.display()))?;
    fs::File::open(parent)
        .with_context(|| format!("failed to open CLI config directory {}", parent.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync CLI config directory {}", parent.display()))?;
    Ok(path)
}

fn save_cli_daemon_url(daemon_url: &str) -> anyhow::Result<PathBuf> {
    let mut cfg = load_cli_config()?;
    let keeps_session = cfg
        .daemon_url
        .as_deref()
        .and_then(|current| setup_journey::same_http_endpoint(current, daemon_url).ok())
        .unwrap_or(false);
    if !keeps_session {
        cfg.session_token = None;
    }
    cfg.daemon_url = Some(daemon_url.to_string());
    save_cli_config(&cfg)
}

fn resolve_daemon_url(cli_value: Option<&str>) -> anyhow::Result<String> {
    if let Some(v) = cli_value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }) {
        return Ok(v);
    }

    if let Some(v) = read_env_trimmed("OORE_DAEMON_URL") {
        return Ok(v);
    }

    let cfg = load_cli_config()?;
    if let Some(v) = cfg.daemon_url.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }) {
        return Ok(v);
    }

    Ok(DEFAULT_DAEMON_URL.to_string())
}

fn resolve_session_token(
    cli_value: Option<&str>,
    resolved_daemon_url: &str,
) -> anyhow::Result<Option<String>> {
    if let Some(v) = cli_value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }) {
        return Ok(Some(v));
    }

    if let Some(v) = read_env_trimmed("OORE_SESSION_TOKEN") {
        return Ok(Some(v));
    }

    let cfg = load_cli_config()?;
    let token = cfg.session_token.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    let belongs_to_daemon = cfg
        .daemon_url
        .as_deref()
        .and_then(|saved| setup_journey::same_http_endpoint(saved, resolved_daemon_url).ok())
        .unwrap_or(false);
    Ok(belongs_to_daemon.then_some(token).flatten())
}

// ── SQLite state helpers ────────────────────────────────────────

const CREATE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS setup_state (
    id                          INTEGER PRIMARY KEY CHECK (id = 1) DEFAULT 1,
    schema_version              INTEGER NOT NULL DEFAULT 1,
    instance_id                 TEXT    NOT NULL,
    setup_state                 TEXT    NOT NULL DEFAULT 'bootstrap_pending',
    bootstrap_token_hash        TEXT,
    bootstrap_token_expires_at  INTEGER,
    bootstrap_token_consumed_at INTEGER,
    session_hash                TEXT,
    session_expires_at          INTEGER,
    oidc_issuer_url             TEXT,
    oidc_client_id              TEXT,
    oidc_has_client_secret      INTEGER,
    oidc_authorization_endpoint TEXT,
    oidc_token_endpoint         TEXT,
    oidc_userinfo_endpoint      TEXT,
    oidc_jwks_uri               TEXT,
    oidc_configured_at          INTEGER,
    oidc_encrypted_client_secret TEXT,
    oidc_secret_stored_at        INTEGER,
    owner_email                 TEXT,
    owner_oidc_subject          TEXT,
    owner_created_at            INTEGER,
    created_at                  INTEGER NOT NULL,
    updated_at                  INTEGER NOT NULL
)
"#;

const TRUSTED_PROXY_SHARED_SECRET_HEADER: &str = "x-oore-trusted-proxy-secret";

async fn connect_db(path: &PathBuf) -> anyhow::Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .with_context(|| format!("failed to connect to database: {}", path.display()))?;

    // Create table if not exists
    sqlx::query(CREATE_TABLE_SQL)
        .execute(&pool)
        .await
        .context("failed to create setup_state table")?;

    Ok(pool)
}

struct SingleNonce(Option<[u8; NONCE_LEN]>);

impl NonceSequence for SingleNonce {
    fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
        let bytes = self.0.take().ok_or(ring::error::Unspecified)?;
        Ok(Nonce::assume_unique_for_key(bytes))
    }
}

/// Load the daemon's encryption key, generating one only for a database
/// that holds no encrypted data. An existing key must decrypt the database's
/// key check value, so the CLI never writes ciphertext the daemon cannot read.
async fn load_or_generate_encryption_key(pool: &SqlitePool) -> anyhow::Result<Vec<u8>> {
    load_or_generate_encryption_key_at(pool, &resolve_key_path()?).await
}

async fn load_or_generate_encryption_key_at(
    pool: &SqlitePool,
    path: &Path,
) -> anyhow::Result<Vec<u8>> {
    let check = load_encryption_key_check(pool).await?;
    if path.exists() {
        let key = fs::read(path)
            .with_context(|| format!("failed to read encryption key: {}", path.display()))?;
        if key.len() != 32 {
            anyhow::bail!(
                "encryption key at {} has invalid length: expected 32 bytes, got {}",
                path.display(),
                key.len()
            );
        }
        if let Some(check) = check
            && decrypt_secret(&check, &key).ok().as_deref()
                != Some(oore_contract::data_paths::ENCRYPTION_KEY_CHECK_PLAINTEXT)
        {
            anyhow::bail!(
                "encryption key {} does not match this database. Restore the original key with `oore backup restore`, or set OORED_DATA_DIR to the directory that holds it.",
                path.display()
            );
        }
        return Ok(key);
    }

    if check.is_some() || database_has_encrypted_data(pool).await? {
        anyhow::bail!(
            "encryption key {} is missing, but the database already holds data encrypted with it. Refusing to generate a new key. Restore the original key with `oore backup restore`, or set OORED_DATA_DIR to the directory that holds it.",
            path.display()
        );
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let rng = SystemRandom::new();
    let mut key = vec![0u8; 32];
    rng.fill(&mut key)
        .map_err(|_| anyhow::anyhow!("failed to generate encryption key"))?;
    fs::write(path, &key)
        .with_context(|| format!("failed to write encryption key: {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    }
    Ok(key)
}

/// The daemon's key check value, if this database has one.
async fn load_encryption_key_check(pool: &SqlitePool) -> anyhow::Result<Option<String>> {
    let has_table: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'encryption_key_check')",
    )
    .fetch_one(pool)
    .await
    .context("failed to inspect database schema for encryption_key_check")?;
    if !has_table {
        return Ok(None);
    }
    sqlx::query_scalar("SELECT check_ciphertext FROM encryption_key_check WHERE id = 1")
        .fetch_optional(pool)
        .await
        .context("failed to load the encryption key check value")
}

/// Whether any column named like `*encrypted*` holds a value.
async fn database_has_encrypted_data(pool: &SqlitePool) -> anyhow::Result<bool> {
    let columns: Vec<(String, String)> = sqlx::query_as(
        "SELECT m.name, c.name
         FROM sqlite_master m, pragma_table_info(m.name) c
         WHERE m.type = 'table'
           AND m.name NOT LIKE 'sqlite_%'
           AND c.name LIKE '%encrypted%'",
    )
    .fetch_all(pool)
    .await
    .context("failed to list encrypted columns")?;
    for (table, column) in columns {
        let table = format!("\"{}\"", table.replace('"', "\"\""));
        let column = format!("\"{}\"", column.replace('"', "\"\""));
        // Identifiers come from sqlite_master and are quoted.
        let exists: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT EXISTS (SELECT 1 FROM {table} WHERE {column} IS NOT NULL AND {column} != '')"
        )))
        .fetch_one(pool)
        .await
        .with_context(|| format!("failed to inspect {table}.{column}"))?;
        if exists {
            return Ok(true);
        }
    }
    Ok(false)
}

fn decrypt_secret(ciphertext_b64: &str, key: &[u8]) -> anyhow::Result<String> {
    let data = BASE64
        .decode(ciphertext_b64)
        .context("failed to decode base64 ciphertext")?;
    anyhow::ensure!(
        data.len() >= NONCE_LEN + AES_256_GCM.tag_len(),
        "ciphertext too short"
    );
    let (nonce_bytes, ciphertext_and_tag) = data.split_at(NONCE_LEN);
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(nonce_bytes);
    let unbound_key = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| anyhow::anyhow!("invalid encryption key"))?;
    let mut opening_key = aead::OpeningKey::new(unbound_key, SingleNonce(Some(nonce)));
    let mut in_out = ciphertext_and_tag.to_vec();
    let plaintext = opening_key
        .open_in_place(Aad::empty(), &mut in_out)
        .map_err(|_| anyhow::anyhow!("decryption failed"))?;
    String::from_utf8(plaintext.to_vec()).context("decrypted data is not valid UTF-8")
}

fn encrypt_secret(plaintext: &str, key: &[u8]) -> anyhow::Result<String> {
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| anyhow::anyhow!("failed to generate encryption nonce"))?;

    let unbound_key = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| anyhow::anyhow!("invalid encryption key"))?;
    let mut sealing_key = aead::SealingKey::new(unbound_key, SingleNonce(Some(nonce_bytes)));
    let mut in_out = plaintext.as_bytes().to_vec();
    sealing_key
        .seal_in_place_append_tag(Aad::empty(), &mut in_out)
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;

    let mut out = Vec::with_capacity(NONCE_LEN + in_out.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&in_out);
    Ok(BASE64.encode(out))
}

fn normalize_email(raw: &str) -> anyhow::Result<String> {
    let email = raw.trim().to_lowercase();
    if email.is_empty() || email.len() > 256 || !email.contains('@') {
        anyhow::bail!("owner email must be a valid email address");
    }
    Ok(email)
}

fn normalize_header_name(raw: &str) -> anyhow::Result<String> {
    let header = raw.trim().to_ascii_lowercase();
    let valid = !header.is_empty()
        && header.len() <= 128
        && header.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(
                    ch,
                    '!' | '#'
                        | '$'
                        | '%'
                        | '&'
                        | '\''
                        | '*'
                        | '+'
                        | '-'
                        | '.'
                        | '^'
                        | '_'
                        | '`'
                        | '|'
                        | '~'
                )
        });
    if !valid {
        anyhow::bail!("trusted proxy user email header is invalid");
    }
    Ok(header)
}

fn trusted_proxy_subject_for_email(email: &str) -> String {
    format!("trusted-proxy::{}", email.trim().to_lowercase())
}

fn local_subject_for_email(email: &str) -> String {
    format!("local::{}", email.trim().to_lowercase())
}

fn resolve_secret_value(
    value: Option<&str>,
    file: Option<&str>,
    label: &str,
) -> anyhow::Result<Option<String>> {
    if let Some(value) = value {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_string()));
        }
    }

    let Some(file) = file else {
        return Ok(None);
    };
    let file = file.trim();
    if file.is_empty() {
        return Ok(None);
    }
    let secret =
        fs::read_to_string(file).with_context(|| format!("failed to read {label} file: {file}"))?;
    let secret = secret.trim();
    if secret.is_empty() {
        anyhow::bail!("{label} file is empty: {file}");
    }
    Ok(Some(secret.to_string()))
}

async fn load_state<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
) -> anyhow::Result<Option<SetupStateFile>> {
    let row = sqlx::query("SELECT * FROM setup_state WHERE id = 1")
        .fetch_optional(executor)
        .await
        .context("failed to query setup_state")?;

    match row {
        None => Ok(None),
        Some(row) => {
            let schema_version: i64 = row.try_get("schema_version")?;
            let instance_id: String = row.try_get("instance_id")?;
            let state_str: String = row.try_get("setup_state")?;
            let setup_state = state_str
                .parse::<SetupState>()
                .map_err(anyhow::Error::msg)?;

            let bootstrap_token = {
                let hash: Option<String> = row.try_get("bootstrap_token_hash")?;
                hash.map(|hash| {
                    let expires_at: i64 = row.try_get("bootstrap_token_expires_at").unwrap_or(0);
                    let consumed_at: Option<i64> =
                        row.try_get("bootstrap_token_consumed_at").unwrap_or(None);
                    BootstrapTokenRecord {
                        hash,
                        expires_at,
                        consumed_at,
                    }
                })
            };

            let setup_session = {
                let hash: Option<String> = row.try_get("session_hash")?;
                hash.map(|hash| {
                    let expires_at: i64 = row.try_get("session_expires_at").unwrap_or(0);
                    oore_contract::SetupSessionRecord { hash, expires_at }
                })
            };

            let oidc_config = {
                let issuer_url: Option<String> = row.try_get("oidc_issuer_url")?;
                issuer_url.map(|issuer_url| oore_contract::OidcConfigRecord {
                    issuer_url,
                    client_id: row.try_get("oidc_client_id").unwrap_or_default(),
                    has_client_secret: row.try_get::<i32, _>("oidc_has_client_secret").unwrap_or(0)
                        != 0,
                    authorization_endpoint: row
                        .try_get("oidc_authorization_endpoint")
                        .unwrap_or_default(),
                    token_endpoint: row.try_get("oidc_token_endpoint").unwrap_or_default(),
                    userinfo_endpoint: row.try_get("oidc_userinfo_endpoint").unwrap_or(None),
                    jwks_uri: row.try_get("oidc_jwks_uri").unwrap_or_default(),
                    configured_at: row.try_get("oidc_configured_at").unwrap_or(0),
                })
            };

            let oidc_secret = {
                let encrypted: Option<String> = row.try_get("oidc_encrypted_client_secret")?;
                encrypted.map(|encrypted_client_secret| oore_contract::OidcSecretRecord {
                    encrypted_client_secret,
                    stored_at: row.try_get("oidc_secret_stored_at").unwrap_or(0),
                })
            };

            let owner = {
                let email: Option<String> = row.try_get("owner_email")?;
                email.map(|email| oore_contract::OwnerRecord {
                    email,
                    oidc_subject: row.try_get("owner_oidc_subject").unwrap_or(None),
                    created_at: row.try_get("owner_created_at").unwrap_or(0),
                })
            };

            let created_at: i64 = row.try_get("created_at")?;
            let updated_at: i64 = row.try_get("updated_at")?;

            Ok(Some(SetupStateFile {
                schema_version: u32::try_from(schema_version)
                    .context("schema_version out of u32 range")?,
                instance_id,
                setup_state,
                bootstrap_token,
                setup_session,
                oidc_config,
                oidc_secret,
                owner,
                created_at,
                updated_at,
            }))
        }
    }
}

async fn save_state<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    state: &SetupStateFile,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO setup_state (
            id, schema_version, instance_id, setup_state,
            bootstrap_token_hash, bootstrap_token_expires_at, bootstrap_token_consumed_at,
            session_hash, session_expires_at,
            oidc_issuer_url, oidc_client_id, oidc_has_client_secret,
            oidc_authorization_endpoint, oidc_token_endpoint,
            oidc_userinfo_endpoint, oidc_jwks_uri, oidc_configured_at,
            oidc_encrypted_client_secret, oidc_secret_stored_at,
            owner_email, owner_oidc_subject, owner_created_at,
            created_at, updated_at
        ) VALUES (
            1, ?1, ?2, ?3,
            ?4, ?5, ?6,
            ?7, ?8,
            ?9, ?10, ?11,
            ?12, ?13,
            ?14, ?15, ?16,
            ?17, ?18,
            ?19, ?20, ?21,
            ?22, ?23
        )
        "#,
    )
    .bind(state.schema_version as i64)
    .bind(&state.instance_id)
    .bind(state.setup_state.to_string())
    .bind(state.bootstrap_token.as_ref().map(|t| &t.hash))
    .bind(state.bootstrap_token.as_ref().map(|t| t.expires_at))
    .bind(state.bootstrap_token.as_ref().and_then(|t| t.consumed_at))
    .bind(state.setup_session.as_ref().map(|s| &s.hash))
    .bind(state.setup_session.as_ref().map(|s| s.expires_at))
    .bind(state.oidc_config.as_ref().map(|c| &c.issuer_url))
    .bind(state.oidc_config.as_ref().map(|c| &c.client_id))
    .bind(
        state
            .oidc_config
            .as_ref()
            .map(|c| c.has_client_secret as i32),
    )
    .bind(
        state
            .oidc_config
            .as_ref()
            .map(|c| &c.authorization_endpoint),
    )
    .bind(state.oidc_config.as_ref().map(|c| &c.token_endpoint))
    .bind(
        state
            .oidc_config
            .as_ref()
            .and_then(|c| c.userinfo_endpoint.as_ref()),
    )
    .bind(state.oidc_config.as_ref().map(|c| &c.jwks_uri))
    .bind(state.oidc_config.as_ref().map(|c| c.configured_at))
    .bind(
        state
            .oidc_secret
            .as_ref()
            .map(|s| &s.encrypted_client_secret),
    )
    .bind(state.oidc_secret.as_ref().map(|s| s.stored_at))
    .bind(state.owner.as_ref().map(|o| &o.email))
    .bind(state.owner.as_ref().and_then(|o| o.oidc_subject.as_ref()))
    .bind(state.owner.as_ref().map(|o| o.created_at))
    .bind(state.created_at)
    .bind(state.updated_at)
    .execute(executor)
    .await
    .context("failed to save setup state")?;

    Ok(())
}

fn new_setup_state(now: i64) -> SetupStateFile {
    SetupStateFile {
        schema_version: SetupStateFile::CURRENT_SCHEMA_VERSION,
        instance_id: uuid::Uuid::new_v4().to_string(),
        setup_state: SetupState::BootstrapPending,
        bootstrap_token: None,
        setup_session: None,
        oidc_config: None,
        oidc_secret: None,
        owner: None,
        created_at: now,
        updated_at: now,
    }
}

// ── End SQLite helpers ──────────────────────────────────────────

fn format_epoch_local(epoch: i64) -> String {
    Local::now()
        .timezone()
        .timestamp_opt(epoch, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| epoch.to_string())
}

fn format_ttl_human(ttl: &Duration) -> String {
    let total_secs = ttl.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 {
        parts.push(format!("{}m", minutes));
    }
    if seconds > 0 || parts.is_empty() {
        parts.push(format!("{}s", seconds));
    }
    parts.join("")
}

fn normalize_trusted_proxy_cidrs(values: Vec<String>) -> anyhow::Result<Vec<String>> {
    let mut normalized = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cidr = trimmed
            .parse::<ipnet::IpNet>()
            .with_context(|| format!("invalid trusted proxy CIDR: {trimmed}"))?;
        let canonical = cidr.to_string();
        if !normalized.iter().any(|existing| existing == &canonical) {
            normalized.push(canonical);
        }
    }
    Ok(normalized)
}

async fn persist_instance_preferences(
    executor: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    runtime_mode: RuntimeMode,
    remote_auth_mode: RemoteAuthMode,
    now: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO instance_preferences (id, key_storage_mode, runtime_mode, remote_auth_mode, updated_by, created_at, updated_at)
         VALUES (1, 'file', ?1, ?2, NULL, ?3, ?3)
         ON CONFLICT(id) DO UPDATE SET
            key_storage_mode = excluded.key_storage_mode,
            runtime_mode = excluded.runtime_mode,
            remote_auth_mode = excluded.remote_auth_mode,
            updated_at = excluded.updated_at",
    )
    .bind(runtime_mode.to_string())
    .bind(remote_auth_mode.to_string())
    .bind(now)
    .execute(executor)
    .await
    .context("failed to save instance preferences")?;
    Ok(())
}

async fn ensure_setup_init_schema(pool: &SqlitePool) -> anyhow::Result<()> {
    for table in ["instance_preferences", "trusted_proxy_settings", "users"] {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        )
        .bind(table)
        .fetch_optional(pool)
        .await
        .with_context(|| format!("failed to inspect database schema for {table}"))?;
        if exists.is_none() {
            anyhow::bail!(
                "setup database is not migrated yet (missing table: {table}). Start `oored run` once, then rerun `oore setup init`."
            );
        }
    }
    Ok(())
}

async fn persist_trusted_proxy_settings(
    executor: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    owner_email: &str,
    user_email_header: &str,
    trusted_proxy_cidrs: &[String],
    encrypted_shared_secret: Option<&str>,
    now: i64,
) -> anyhow::Result<()> {
    let cidrs_json = serde_json::to_string(trusted_proxy_cidrs)
        .context("failed to serialize trusted proxy CIDRs")?;
    sqlx::query(
        "INSERT INTO trusted_proxy_settings (id, user_email_header, setup_owner_email, trusted_proxy_cidrs_json, encrypted_shared_secret, updated_by, created_at, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, NULL, ?5, ?5)
         ON CONFLICT(id) DO UPDATE SET
            user_email_header = excluded.user_email_header,
            setup_owner_email = excluded.setup_owner_email,
            trusted_proxy_cidrs_json = excluded.trusted_proxy_cidrs_json,
            encrypted_shared_secret = excluded.encrypted_shared_secret,
            updated_at = excluded.updated_at",
    )
    .bind(user_email_header)
    .bind(owner_email)
    .bind(cidrs_json)
    .bind(encrypted_shared_secret)
    .bind(now)
    .execute(executor)
    .await
    .context("failed to save trusted proxy settings")?;
    Ok(())
}

async fn upsert_owner_user(
    executor: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    owner_email: &str,
    subject: &str,
    now: i64,
) -> anyhow::Result<()> {
    let user_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, oidc_subject, display_name, role, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'owner', 'active', ?5, ?5)
         ON CONFLICT(email) DO UPDATE SET
            oidc_subject = excluded.oidc_subject,
            display_name = excluded.display_name,
            role = 'owner',
            status = 'active',
            updated_at = excluded.updated_at",
    )
    .bind(&user_id)
    .bind(owner_email)
    .bind(subject)
    .bind(owner_email)
    .bind(now)
    .execute(executor)
    .await
    .context("failed to create owner user")?;
    Ok(())
}

fn ensure_setup_init_allowed(state: SetupState, force: bool) -> anyhow::Result<()> {
    if state == SetupState::Ready {
        anyhow::bail!("setup is already complete; refusing to change a ready instance");
    }
    if state == SetupState::IdpConfigured && !force {
        anyhow::bail!(
            "setup already has an unfinished access choice; pass --force to replace it before owner creation"
        );
    }
    Ok(())
}

fn ensure_offline_setup_init_allowed(state: SetupState, force: bool) -> anyhow::Result<()> {
    if state == SetupState::OwnerCreated {
        anyhow::bail!(
            "setup already created its owner; start `oored run`, then run `oore setup` to finish without changing access"
        );
    }
    ensure_setup_init_allowed(state, force)
}

async fn handle_setup_init(args: SetupInitArgs, quiet: bool) -> anyhow::Result<()> {
    let db_path = resolve_db_path(args.state_file.as_deref())?;
    let pool = connect_db(&db_path).await?;
    if let Some(state) = load_state(&pool).await? {
        ensure_offline_setup_init_allowed(state.setup_state, args.force)?;
    }

    let owner_email = normalize_email(&args.owner_email)?;
    let now = now_epoch_secs();
    ensure_setup_init_schema(&pool).await?;

    let (runtime_mode, remote_auth_mode, owner_subject) = match args.mode {
        SetupInitMode::Local => (
            RuntimeMode::Local,
            RemoteAuthMode::Oidc,
            local_subject_for_email(&owner_email),
        ),
        SetupInitMode::TrustedProxy => (
            RuntimeMode::Remote,
            RemoteAuthMode::TrustedProxy,
            trusted_proxy_subject_for_email(&owner_email),
        ),
    };

    let mut trusted_proxy_cidrs = Vec::new();
    let mut user_email_header = None;
    let mut has_shared_secret = false;
    let mut encrypted_shared_secret = None;
    if args.mode == SetupInitMode::TrustedProxy {
        let header = normalize_header_name(&args.user_email_header)?;
        let cidrs = normalize_trusted_proxy_cidrs(args.trusted_proxy_cidrs)?;
        let shared_secret = resolve_secret_value(
            args.shared_secret.as_deref(),
            args.shared_secret_file.as_deref(),
            "trusted proxy shared secret",
        )?
        .context(
            "trusted-proxy setup init requires a shared secret; pass --shared-secret-file or set OORE_TRUSTED_PROXY_SHARED_SECRET_FILE",
        )?;
        let key = load_or_generate_encryption_key(&pool).await?;
        encrypted_shared_secret = Some(encrypt_secret(&shared_secret, &key)?);
        has_shared_secret = encrypted_shared_secret.is_some();
        trusted_proxy_cidrs = cidrs;
        user_email_header = Some(header);
    }

    let mut transaction = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .context("failed to begin setup initialization transaction")?;
    let mut state = match load_state(&mut *transaction).await? {
        Some(state) => state,
        None => {
            let state = new_setup_state(now);
            save_state(&mut *transaction, &state).await?;
            state
        }
    };
    ensure_offline_setup_init_allowed(state.setup_state, args.force)?;

    persist_instance_preferences(&mut *transaction, runtime_mode, remote_auth_mode, now).await?;
    if let Some(header) = user_email_header.as_deref() {
        persist_trusted_proxy_settings(
            &mut *transaction,
            &owner_email,
            header,
            &trusted_proxy_cidrs,
            encrypted_shared_secret.as_deref(),
            now,
        )
        .await?;
    }

    state.owner = Some(oore_contract::OwnerRecord {
        email: owner_email.clone(),
        oidc_subject: Some(owner_subject.clone()),
        created_at: now,
    });
    upsert_owner_user(&mut *transaction, &owner_email, &owner_subject, now).await?;
    state.setup_state = SetupState::Ready;
    state.setup_session = None;
    state.bootstrap_token = None;
    state.updated_at = now;
    save_state(&mut *transaction, &state).await?;
    transaction
        .commit()
        .await
        .context("failed to commit setup initialization")?;

    if args.json {
        let output = serde_json::json!({
            "ok": true,
            "state": "ready",
            "mode": match args.mode {
                SetupInitMode::Local => "local",
                SetupInitMode::TrustedProxy => "trusted_proxy",
            },
            "owner_email": owner_email,
            "database": db_path.display().to_string(),
            "instance_id": state.instance_id,
            "trusted_proxy": {
                "user_email_header": user_email_header,
                "trusted_proxy_cidrs": trusted_proxy_cidrs,
                "has_shared_secret": has_shared_secret,
                "shared_secret_header": TRUSTED_PROXY_SHARED_SECRET_HEADER,
            },
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if quiet {
        return Ok(());
    }

    println!("Setup initialized.");
    println!();
    println!("State:    ready");
    println!("Instance: {}", state.instance_id);
    println!("Owner:    {}", owner_email);
    println!("DB:       {}", db_path.display());
    match args.mode {
        SetupInitMode::Local => {
            println!("Mode:     local");
            println!();
            println!("Local login is available only from loopback.");
        }
        SetupInitMode::TrustedProxy => {
            println!("Mode:     remote trusted-proxy");
            println!(
                "Header:   {}",
                user_email_header.as_deref().unwrap_or("x-oore-user-email")
            );
            if trusted_proxy_cidrs.is_empty() {
                println!("Peers:    loopback only");
            } else {
                println!("Peers:    {}", trusted_proxy_cidrs.join(", "));
            }
            println!(
                "Secret:   {}",
                if has_shared_secret {
                    "configured"
                } else {
                    "not configured"
                }
            );
            if has_shared_secret {
                println!("Proxy must forward: {}", TRUSTED_PROXY_SHARED_SECRET_HEADER);
            }
        }
    }

    Ok(())
}

async fn require_setup_response(
    response: reqwest::Response,
    action: &str,
) -> anyhow::Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let message = extract_error_message(response).await;
    anyhow::bail!("{action} failed (HTTP {}): {message}", status.as_u16())
}

fn setup_status_access_value(status: &SetupStatus) -> &'static str {
    match (status.runtime_mode, status.remote_auth_mode) {
        (RuntimeMode::Local, _) => "local",
        (RuntimeMode::Remote, RemoteAuthMode::Oidc) => "oidc",
        (RuntimeMode::Remote, RemoteAuthMode::TrustedProxy) => "trusted_proxy",
    }
}

fn setup_status_access_label(status: &SetupStatus) -> &'static str {
    match (status.runtime_mode, status.remote_auth_mode) {
        (RuntimeMode::Local, _) => "this device",
        (RuntimeMode::Remote, RemoteAuthMode::Oidc) => "identity provider (OIDC)",
        (RuntimeMode::Remote, RemoteAuthMode::TrustedProxy) => "trusted access proxy",
    }
}

fn ensure_setup_init_mode_matches_status(
    mode: SetupInitMode,
    status: &SetupStatus,
) -> anyhow::Result<()> {
    let matches = match mode {
        SetupInitMode::Local => status.runtime_mode == RuntimeMode::Local,
        SetupInitMode::TrustedProxy => {
            status.runtime_mode == RuntimeMode::Remote
                && status.remote_auth_mode == RemoteAuthMode::TrustedProxy
        }
    };
    if matches {
        return Ok(());
    }

    let requested = match mode {
        SetupInitMode::Local => "this device",
        SetupInitMode::TrustedProxy => "trusted access proxy",
    };
    anyhow::bail!(
        "setup already created its owner for {}; requested access is {requested}. Run `oore setup` without changing access to finish",
        setup_status_access_label(status)
    )
}

async fn complete_setup_via_daemon(
    client: &reqwest::Client,
    daemon_url: &str,
    session: &str,
) -> anyhow::Result<SetupCompleteResponse> {
    let response = client
        .post(format!(
            "{}/v1/setup/complete",
            daemon_url.trim_end_matches('/')
        ))
        .bearer_auth(session)
        .send()
        .await
        .context("failed to reach the daemon while completing setup")?;
    let response = require_setup_response(response, "setup completion").await?;
    let completed: SetupCompleteResponse = response
        .json()
        .await
        .context("failed to parse setup completion response")?;
    anyhow::ensure!(
        completed.state == SetupState::Ready,
        "daemon did not report ready after setup completion"
    );
    Ok(completed)
}

async fn resume_setup_completion_via_daemon(
    client: &reqwest::Client,
    daemon_url: &str,
    state_file: Option<&str>,
) -> anyhow::Result<SetupCompleteResponse> {
    let session = acquire_session(client, daemon_url, state_file).await?;
    complete_setup_via_daemon(client, daemon_url, &session).await
}

async fn handle_setup_init_via_daemon(
    args: SetupInitArgs,
    daemon_url: &str,
    quiet: bool,
) -> anyhow::Result<()> {
    let client = endpoint_http_client_builder(daemon_url)
        .timeout(Duration::from_secs(15))
        .build()
        .context("failed to prepare the setup connection")?;
    let status = fetch_setup_status(&client, daemon_url).await?;
    if status.state == SetupState::OwnerCreated {
        ensure_setup_init_mode_matches_status(args.mode, &status)?;
        let completed =
            resume_setup_completion_via_daemon(&client, daemon_url, args.state_file.as_deref())
                .await?;
        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "state": "ready",
                    "resumed": true,
                    "mode": setup_status_access_value(&status),
                    "instance_id": completed.instance_id,
                }))?
            );
        } else if !quiet {
            println!("Setup resumed.");
            println!();
            println!("State:    ready");
            println!("Instance: {}", completed.instance_id);
            println!("Daemon:   {daemon_url}");
            println!("Access:   {}", setup_status_access_label(&status));
        }
        return Ok(());
    }
    ensure_setup_init_allowed(status.state, args.force)?;
    let owner_email = normalize_email(&args.owner_email)?;
    let session = acquire_session(&client, daemon_url, args.state_file.as_deref()).await?;

    let (runtime_mode, remote_auth_mode) = match args.mode {
        SetupInitMode::Local => (RuntimeMode::Local, RemoteAuthMode::Oidc),
        SetupInitMode::TrustedProxy => (RuntimeMode::Remote, RemoteAuthMode::TrustedProxy),
    };
    save_setup_preferences(
        &client,
        daemon_url,
        &session,
        runtime_mode,
        Some(remote_auth_mode),
    )
    .await?;

    let mut trusted_proxy_cidrs = Vec::new();
    let mut user_email_header = None;
    let mut has_shared_secret = false;
    match args.mode {
        SetupInitMode::Local => {
            let response = client
                .post(format!(
                    "{}/v1/setup/local-owner/create",
                    daemon_url.trim_end_matches('/')
                ))
                .bearer_auth(&session)
                .json(&SetupLocalOwnerCreateRequest {
                    email: owner_email.clone(),
                })
                .send()
                .await
                .context("failed to reach the daemon while creating the local owner")?;
            require_setup_response(response, "local owner creation").await?;
        }
        SetupInitMode::TrustedProxy => {
            let header = normalize_header_name(&args.user_email_header)?;
            let cidrs = normalize_trusted_proxy_cidrs(args.trusted_proxy_cidrs)?;
            let shared_secret = resolve_secret_value(
                args.shared_secret.as_deref(),
                args.shared_secret_file.as_deref(),
                "trusted proxy shared secret",
            )?
            .context(
                "trusted-proxy setup requires a shared secret; pass --shared-secret-file or set OORE_TRUSTED_PROXY_SHARED_SECRET_FILE",
            )?;
            let response = client
                .post(format!(
                    "{}/v1/setup/trusted-proxy/configure",
                    daemon_url.trim_end_matches('/')
                ))
                .bearer_auth(&session)
                .json(&SetupTrustedProxyConfigureRequest {
                    user_email_header: Some(header.clone()),
                    setup_owner_email: Some(owner_email.clone()),
                    trusted_proxy_cidrs: cidrs.clone(),
                    shared_secret: Some(shared_secret.clone()),
                })
                .send()
                .await
                .context("failed to reach the daemon while configuring trusted access")?;
            require_setup_response(response, "trusted access configuration").await?;

            let identity_header = reqwest::header::HeaderName::from_bytes(header.as_bytes())
                .context("trusted proxy identity header is invalid")?;
            let identity = reqwest::header::HeaderValue::from_str(&owner_email)
                .context("owner email cannot be sent as an identity header")?;
            let response = client
                .post(format!(
                    "{}/v1/setup/owner/claim-trusted-proxy",
                    daemon_url.trim_end_matches('/')
                ))
                .bearer_auth(&session)
                .header(TRUSTED_PROXY_SHARED_SECRET_HEADER, &shared_secret)
                .header(identity_header, identity)
                .send()
                .await
                .context("failed to reach the daemon while claiming the trusted access owner")?;
            require_setup_response(response, "trusted access owner claim").await?;
            trusted_proxy_cidrs = cidrs;
            user_email_header = Some(header);
            has_shared_secret = true;
        }
    }

    let completed = complete_setup_via_daemon(&client, daemon_url, &session).await?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "state": "ready",
                "mode": match args.mode {
                    SetupInitMode::Local => "local",
                    SetupInitMode::TrustedProxy => "trusted_proxy",
                },
                "owner_email": owner_email,
                "database": resolve_db_path(args.state_file.as_deref())?.display().to_string(),
                "instance_id": completed.instance_id,
                "trusted_proxy": {
                    "user_email_header": user_email_header,
                    "trusted_proxy_cidrs": trusted_proxy_cidrs,
                    "has_shared_secret": has_shared_secret,
                    "shared_secret_header": TRUSTED_PROXY_SHARED_SECRET_HEADER,
                },
            }))?
        );
    } else if !quiet {
        println!("Setup initialized.");
        println!();
        println!("State:    ready");
        println!("Instance: {}", completed.instance_id);
        println!("Owner:    {owner_email}");
        println!("Daemon:   {daemon_url}");
        match args.mode {
            SetupInitMode::Local => println!("Mode:     local"),
            SetupInitMode::TrustedProxy => {
                println!("Mode:     remote trusted-proxy");
                println!(
                    "Header:   {}",
                    user_email_header.as_deref().unwrap_or("x-oore-user-email")
                );
                println!(
                    "Peers:    {}",
                    if trusted_proxy_cidrs.is_empty() {
                        "loopback only".to_string()
                    } else {
                        trusted_proxy_cidrs.join(", ")
                    }
                );
                println!("Secret:   configured");
                println!("Proxy must forward: {TRUSTED_PROXY_SHARED_SECRET_HEADER}");
            }
        }
    }
    Ok(())
}

async fn handle_setup_init_direct(args: SetupInitArgs) -> anyhow::Result<()> {
    let install_root = resolve_install_root()?;
    let _lifecycle_lock = install_lock::InstallLock::acquire(&install_root)?;
    let manifest_path = install_root.join("install-manifest.json");
    let profile_managed = match fs::symlink_metadata(&manifest_path) {
        Ok(_) => {
            let manifest = install_manifest::InstallManifest::load(&manifest_path)?;
            anyhow::ensure!(
                matches!(
                    manifest.profile,
                    install_manifest::InstallProfile::Complete
                        | install_manifest::InstallProfile::ControlPlane
                ),
                "`oore setup init` is not available for this device role; run `oore setup` to complete the selected profile"
            );
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {}", manifest_path.display()));
        }
    };
    let socket_path = recovery_socket_path(args.state_file.as_deref())?;
    let daemon_running = fs::symlink_metadata(&socket_path).is_ok();
    if profile_managed || daemon_running {
        let daemon_url = resolve_daemon_url(None)?;
        return handle_setup_init_via_daemon(args, &daemon_url, false).await;
    }
    handle_setup_init(args, false).await
}

async fn handle_frontend_invite(args: FrontendInviteArgs) -> anyhow::Result<()> {
    let ttl = parse_ttl(&args.ttl)?;
    let daemon_url = resolve_daemon_url(None)?;
    let (code, expires_at) =
        create_frontend_pairing_invite(&daemon_url, args.state_file.as_deref(), ttl).await?;

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "code": code,
                "expires_at": expires_at,
                "single_use": true,
            })
        );
    } else {
        println!(
            "Frontend pairing code (single-use, expires in {}):",
            args.ttl
        );
        println!("{code}");
    }
    Ok(())
}

async fn create_frontend_pairing_invite(
    daemon_url: &str,
    state_file: Option<&str>,
    ttl: Duration,
) -> anyhow::Result<(String, i64)> {
    anyhow::ensure!(
        !ttl.is_zero() && ttl <= Duration::from_secs(60 * 60),
        "frontend pairing ttl must be between 1 second and 1 hour"
    );
    let response = call_operator(
        daemon_url,
        state_file,
        OperatorRequest::CreateFrontendInvite {
            ttl_secs: ttl.as_secs(),
        },
    )
    .await?;
    match response {
        OperatorResponse::FrontendInvite {
            code, expires_at, ..
        } => Ok((code, expires_at)),
        _ => anyhow::bail!("oored returned an unexpected frontend pairing response"),
    }
}

async fn handle_setup_token(args: SetupTokenArgs, daemon_url: &str) -> anyhow::Result<()> {
    let ttl = parse_ttl(&args.ttl)?;
    anyhow::ensure!(
        !ttl.is_zero() && ttl <= Duration::from_secs(60 * 60),
        "bootstrap token ttl must be between 1 second and 1 hour"
    );
    let response = call_operator(
        daemon_url,
        args.state_file.as_deref(),
        OperatorRequest::MintBootstrapToken {
            ttl_secs: ttl.as_secs(),
        },
    )
    .await?;
    let (plaintext_token, expires_at, setup_state, instance_id) = match response {
        OperatorResponse::BootstrapToken {
            token,
            expires_at,
            state,
            instance_id,
        } => (token, expires_at, state, instance_id),
        _ => anyhow::bail!("oored returned an unexpected bootstrap token response"),
    };
    let state_display = match setup_state {
        SetupState::Uninitialized => "uninitialized",
        SetupState::BootstrapPending => "bootstrap_pending",
        SetupState::IdpConfigured => "idp_configured",
        SetupState::OwnerCreated => "owner_created",
        SetupState::Ready => anyhow::bail!("Setup is already complete. Instance is ready."),
        _ => "unknown",
    };
    let db_path = resolve_db_path(args.state_file.as_deref())?;

    if args.json {
        let output = serde_json::json!({
            "token": plaintext_token,
            "expires_at": expires_at,
            "state": state_display,
            "database": db_path.display().to_string(),
            "instance_id": instance_id,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let ttl_display = format_ttl_human(&ttl);
        println!("Bootstrap token generated.");
        println!();
        println!("Token:   {}", plaintext_token);
        println!(
            "Expires: {} ({} from now)",
            format_epoch_local(expires_at),
            ttl_display
        );
        println!("State:   {}", state_display);
        println!("DB:      {}", db_path.display());
        println!();
        println!("To complete setup, either:");
        println!("  1. Open https://ci.oore.build/setup and paste this token");
        println!("  2. Run: oore setup");
        println!();
        println!("Note: The hosted UI requires your backend to be reachable over HTTPS.");
        println!("      For local-only setups, use the CLI (option 2) or expose your");
        println!("      backend via a tunnel (e.g. cloudflared).");
    }

    Ok(())
}

// ── OIDC loopback helpers ───────────────────────────────────────

/// Open a URL in the default browser (macOS-only in V1).
fn open_browser(url: &str) -> bool {
    std::process::Command::new("open")
        .arg(url)
        .status()
        .is_ok_and(|status| status.success())
}

fn endpoint_http_client_builder(endpoint: &str) -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder();
    let bypass_system_proxies = url::Url::parse(endpoint).ok().is_some_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some_and(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host
                        .trim_matches(['[', ']'])
                        .parse::<IpAddr>()
                        .is_ok_and(|address| address.is_loopback())
            })
    });
    if bypass_system_proxies {
        builder.no_proxy()
    } else {
        builder
    }
}

fn ssh_session_target() -> anyhow::Result<(String, u16)> {
    let connection = std::env::var("SSH_CONNECTION")
        .context("SSH_CONNECTION is unavailable; use a terminal setup session instead")?;
    let fields: Vec<_> = connection.split_whitespace().collect();
    anyhow::ensure!(
        fields.len() == 4,
        "SSH_CONNECTION has an unexpected format; use a terminal setup session instead"
    );
    let server_ip: IpAddr = fields[2]
        .parse()
        .context("SSH_CONNECTION contains an invalid server address")?;
    let server_port: u16 = fields[3]
        .parse()
        .context("SSH_CONNECTION contains an invalid server port")?;
    let user = std::env::var("USER").context("USER is unavailable in this SSH session")?;
    anyhow::ensure!(
        !user.is_empty()
            && user
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)),
        "the current user name cannot be rendered safely in an SSH command"
    );
    let host = match server_ip {
        IpAddr::V4(_) => server_ip.to_string(),
        IpAddr::V6(_) => format!("[{server_ip}]"),
    };
    Ok((format!("{user}@{host}"), server_port))
}

fn ssh_loopback_tunnel_command(port: u16) -> anyhow::Result<String> {
    let (target, server_port) = ssh_session_target()?;
    Ok(format!(
        "ssh -N -o ExitOnForwardFailure=yes -p {server_port} -L {port}:127.0.0.1:{port} {target}"
    ))
}

/// Accept a single HTTP request on the listener, extract OIDC callback params.
/// Returns `(code, state)` on success.
async fn wait_for_oidc_callback(listener: TcpListener) -> anyhow::Result<(String, String)> {
    let (mut stream, _addr) = listener
        .accept()
        .await
        .context("failed to accept OIDC callback connection")?;

    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .context("failed to read OIDC callback request")?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse the GET request line: "GET /path?query HTTP/1.1"
    let first_line = request.lines().next().unwrap_or("");
    let path = first_line.split_whitespace().nth(1).unwrap_or("/");

    // Parse query params using url crate
    let fake_base = format!("http://localhost{}", path);
    let parsed = url::Url::parse(&fake_base).context("failed to parse callback URL")?;
    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();

    // Check for OIDC error
    if let Some(error) = params.get("error") {
        let desc = params
            .get("error_description")
            .map(|d| format!(": {}", d))
            .unwrap_or_default();
        let error_page = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n\
            <!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
            <link rel=\"icon\" href=\"{favicon}\"><link rel=\"apple-touch-icon\" href=\"{favicon}\">\
            <meta name=\"theme-color\" content=\"#2457c5\"><title>Authentication failed</title></head>\
            <body><h2>Authentication failed</h2><p>{error}{desc}</p><p>You can close this tab.</p></body></html>",
            favicon = FAVICON_DATA_URI,
            error = error,
            desc = desc,
        );
        stream.write_all(error_page.as_bytes()).await.ok();
        stream.shutdown().await.ok();
        anyhow::bail!("OIDC authentication error: {}{}", error, desc);
    }

    let code = params
        .get("code")
        .cloned()
        .context("OIDC callback missing 'code' parameter")?;
    let state = params
        .get("state")
        .cloned()
        .context("OIDC callback missing 'state' parameter")?;

    let returned_page = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n\
        <!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
        <link rel=\"icon\" href=\"{favicon}\"><link rel=\"apple-touch-icon\" href=\"{favicon}\">\
        <meta name=\"theme-color\" content=\"#2457c5\"><title>Sign-in returned to Oore</title></head>\
        <body><h2>Sign-in returned to Oore</h2><p>Return to the terminal while Oore verifies this sign-in. You can close this tab.</p></body></html>",
        favicon = FAVICON_DATA_URI,
    );
    stream.write_all(returned_page.as_bytes()).await.ok();
    stream.shutdown().await.ok();

    Ok((code, state))
}

// ── Interactive setup flow ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OidcSettingsAction {
    Reuse,
    Change,
}

/// Try to parse a daemon error response body into an ApiError.
/// Falls back to the raw body text if parsing fails.
async fn extract_error_message(resp: reqwest::Response) -> String {
    let status = resp.status();
    match resp.text().await {
        Ok(body) => {
            if let Ok(api_err) = serde_json::from_str::<ApiError>(&body) {
                let mut msg = format!("{} [{}]", api_err.error, api_err.code);
                if let Some(details) = api_err.details {
                    let details = details.trim();
                    if !details.is_empty() {
                        msg = format!("{msg}: {details}");
                    }
                }
                msg
            } else if body.is_empty() {
                format!("HTTP {status}")
            } else {
                body
            }
        }
        Err(_) => format!("HTTP {status}"),
    }
}

async fn fetch_setup_status(
    client: &reqwest::Client,
    daemon_url: &str,
) -> anyhow::Result<SetupStatus> {
    let status_url = format!(
        "{}/v1/public/setup-status",
        daemon_url.trim_end_matches('/')
    );
    let response = client
        .get(&status_url)
        .send()
        .await
        .with_context(|| format!("failed to reach daemon at {daemon_url}"))?;

    if response.status().is_success() {
        return response
            .json()
            .await
            .context("failed to parse setup-status response");
    }

    let status = response.status();
    let msg = extract_error_message(response).await;
    anyhow::bail!(
        "daemon status check failed at {daemon_url} (HTTP {}): {}",
        status.as_u16(),
        msg
    )
}

async fn fetch_user_profile(
    client: &reqwest::Client,
    daemon_url: &str,
    token: &str,
) -> anyhow::Result<UserProfileResponse> {
    let url = format!("{}/v1/users/me", daemon_url.trim_end_matches('/'));
    let response = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .with_context(|| format!("failed to reach daemon at {daemon_url}"))?;

    if response.status().is_success() {
        return response
            .json()
            .await
            .context("failed to parse /v1/users/me response");
    }

    let status = response.status();
    let msg = extract_error_message(response).await;
    anyhow::bail!(
        "token validation failed at {daemon_url} (HTTP {}): {}",
        status.as_u16(),
        msg
    )
}

async fn fetch_build_list(
    client: &reqwest::Client,
    daemon_url: &str,
    token: &str,
    status_filter: Option<&str>,
    limit: Option<i64>,
) -> anyhow::Result<ListBuildsResponse> {
    let url = format!("{}/v1/builds", daemon_url.trim_end_matches('/'));
    let mut query: Vec<(&str, String)> = Vec::new();
    if let Some(status) = status_filter {
        query.push(("status", status.to_string()));
    }
    if let Some(limit) = limit {
        query.push(("limit", limit.to_string()));
    }

    let response = client
        .get(&url)
        .bearer_auth(token)
        .query(&query)
        .send()
        .await
        .with_context(|| format!("failed to reach daemon at {daemon_url}"))?;

    if response.status().is_success() {
        return response
            .json()
            .await
            .context("failed to parse /v1/builds response");
    }

    let status = response.status();
    let msg = extract_error_message(response).await;
    anyhow::bail!(
        "failed to fetch builds at {daemon_url} (HTTP {}): {}",
        status.as_u16(),
        msg
    )
}

async fn fetch_runner_list(
    client: &reqwest::Client,
    daemon_url: &str,
    token: &str,
) -> anyhow::Result<ListRunnersResponse> {
    let url = format!("{}/v1/runners", daemon_url.trim_end_matches('/'));
    let response = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .with_context(|| format!("failed to reach daemon at {daemon_url}"))?;

    if response.status().is_success() {
        return response
            .json()
            .await
            .context("failed to parse /v1/runners response");
    }

    let status = response.status();
    let msg = extract_error_message(response).await;
    anyhow::bail!(
        "failed to fetch runners at {daemon_url} (HTTP {}): {}",
        status.as_u16(),
        msg
    )
}

/// Acquire a session token by generating a bootstrap token and verifying it with the daemon.
/// Used both for initial setup (step 1) and when resuming from a later state with an expired session.
async fn acquire_session(
    client: &reqwest::Client,
    daemon_url: &str,
    state_file: Option<&str>,
) -> anyhow::Result<String> {
    let minted = call_operator(
        daemon_url,
        state_file,
        OperatorRequest::MintBootstrapToken { ttl_secs: 15 * 60 },
    )
    .await?;
    let plaintext_token = match minted {
        OperatorResponse::BootstrapToken { token, .. } => token,
        _ => anyhow::bail!("oored returned an unexpected bootstrap token response"),
    };

    let verify_url = format!("{}/v1/setup/bootstrap-token/verify", daemon_url);
    let verify_resp = client
        .post(&verify_url)
        .json(&BootstrapTokenVerifyRequest {
            token: plaintext_token,
        })
        .send()
        .await
        .context("failed to reach daemon for token verification")?;

    if verify_resp.status().is_success() {
        let body: BootstrapTokenVerifyResponse = verify_resp
            .json()
            .await
            .context("failed to parse bootstrap verify response")?;
        Ok(body.session_token)
    } else {
        let sc = verify_resp.status();
        let msg = extract_error_message(verify_resp).await;
        if sc == reqwest::StatusCode::CONFLICT {
            anyhow::bail!("Setup is already complete. Instance is in ready state.");
        } else if sc == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("Bootstrap token is invalid. Please regenerate with: oore setup token");
        } else {
            anyhow::bail!(
                "Bootstrap verification failed (HTTP {}): {}",
                sc.as_u16(),
                msg
            );
        }
    }
}

async fn save_setup_preferences(
    client: &reqwest::Client,
    daemon_url: &str,
    token: &str,
    runtime_mode: RuntimeMode,
    remote_auth_mode: Option<RemoteAuthMode>,
) -> anyhow::Result<()> {
    let response = client
        .post(format!(
            "{}/v1/setup/preferences",
            daemon_url.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .json(&SetupPreferencesRequest {
            runtime_mode,
            remote_auth_mode,
        })
        .send()
        .await
        .context("failed to reach the daemon while saving access settings")?;
    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status();
    let message = extract_error_message(response).await;
    anyhow::bail!(
        "access settings were not saved (HTTP {}): {}",
        status.as_u16(),
        message
    )
}

async fn fetch_setup_summary(
    client: &reqwest::Client,
    daemon_url: &str,
    token: &str,
) -> anyhow::Result<SetupSummaryResponse> {
    let response = client
        .get(format!(
            "{}/v1/setup/summary",
            daemon_url.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .send()
        .await
        .context("failed to reach the daemon while loading saved setup settings")?;
    let response = require_setup_response(response, "setup summary").await?;
    response
        .json()
        .await
        .context("failed to parse the saved setup settings")
}

async fn handle_setup_oidc_interactive(
    daemon_url: &str,
    state_file: Option<&str>,
    terminal: Terminal,
) -> anyhow::Result<bool> {
    let client = endpoint_http_client_builder(daemon_url)
        .timeout(Duration::from_secs(15))
        .build()
        .context("failed to build HTTP client")?;

    let operation = terminal.operation("Connecting to the control plane");
    let status: SetupStatus = match fetch_setup_status(&client, daemon_url).await {
        Ok(status) => {
            operation.done("Control plane connected");
            status
        }
        Err(error) => {
            operation.failed(format!(
                "Cannot reach oored at {daemon_url}. Start it with `oored run`."
            ));
            return Err(error);
        }
    };

    terminal.note(
        "Control plane",
        format!(
            "URL: {daemon_url}\nInstance: {}\nSetup state: {}",
            status.instance_id, status.state
        ),
    )?;

    if status.state == SetupState::Ready {
        anyhow::ensure!(
            status.runtime_mode == RuntimeMode::Remote
                && status.remote_auth_mode == RemoteAuthMode::Oidc,
            "setup is complete with a different access method; change access from Oore settings"
        );
        terminal.note("Identity provider", "OIDC access is already configured.")?;
        terminal.note(
            "Network access",
            "OIDC protects sign-in. It does not publish this Mac. Connect an HTTPS proxy or tunnel before other people use Oore.",
        )?;
        return Ok(true);
    }

    anyhow::ensure!(
        status.state != SetupState::OwnerCreated
            || (status.runtime_mode == RuntimeMode::Remote
                && status.remote_auth_mode == RemoteAuthMode::Oidc),
        "owner setup already uses a different access method; finish or recover that setup before selecting OIDC"
    );

    let mut current_state = status.state;
    let mut session_token: Option<String> = None;

    if current_state == SetupState::BootstrapPending || current_state == SetupState::Uninitialized {
        let db_path = resolve_db_path(state_file)?;
        terminal.note(
            "Secure setup session",
            format!(
                "Oore will create a single-use token that expires in 15 minutes.\n\nDatabase: {}",
                db_path.display()
            ),
        )?;
        let operation = terminal.operation("Verifying one-time setup access");
        session_token = match acquire_session(&client, daemon_url, state_file).await {
            Ok(token) => {
                operation.done("Setup access verified");
                Some(token)
            }
            Err(error) => {
                operation.failed("Setup access verification failed");
                return Err(error);
            }
        };
        current_state = SetupState::BootstrapPending;
    }

    if matches!(
        current_state,
        SetupState::BootstrapPending | SetupState::IdpConfigured
    ) {
        if session_token.is_none() {
            let operation = terminal.operation("Restoring the setup session");
            session_token = match acquire_session(&client, daemon_url, state_file).await {
                Ok(token) => {
                    operation.done("Setup session restored");
                    Some(token)
                }
                Err(error) => {
                    operation.failed("Setup session could not be restored");
                    return Err(error);
                }
            };
        }
        let token = session_token
            .as_deref()
            .context("setup session is missing")?;
        let operation = terminal.operation("Saving the access method");
        let result = save_setup_preferences(
            &client,
            daemon_url,
            token,
            RuntimeMode::Remote,
            Some(RemoteAuthMode::Oidc),
        )
        .await;
        match result {
            Ok(()) => operation.done("Identity provider access selected"),
            Err(error) => {
                operation.failed("Access method could not be saved");
                return Err(error);
            }
        }
    }

    let has_saved_oidc_config = if current_state == SetupState::IdpConfigured {
        let token = session_token
            .as_deref()
            .context("setup session is missing")?;
        let summary = fetch_setup_summary(&client, daemon_url, token).await?;
        summary
            .issuer_url
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            && summary
                .client_id
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
    } else {
        false
    };

    let mut callback_listener = if matches!(
        current_state,
        SetupState::BootstrapPending | SetupState::IdpConfigured
    ) {
        let listener = match TcpListener::bind(("127.0.0.1", TERMINAL_OIDC_CALLBACK_PORT)).await {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                return Err(error).context(
                    "localhost port 4174 is already in use; close the conflicting process, then run `oore setup` again",
                );
            }
            Err(error) => {
                return Err(error)
                    .context("failed to bind the OIDC callback on localhost port 4174");
            }
        };
        terminal.note(
            "Identity provider callback",
            format!(
                "Add this exact redirect URL to your identity provider before entering its client details:\n\n  {TERMINAL_OIDC_REDIRECT_URI}\n\nKeep this URL registered for future setup retries."
            ),
        )?;
        Some(listener)
    } else {
        None
    };

    // ── Step 2: OIDC Configuration ──────────────────────────────

    let settings_action = if current_state == SetupState::IdpConfigured && has_saved_oidc_config {
        let action = match terminal
            .select(
                "How do you want to continue with the saved identity provider?",
                [
                    SelectChoice::new(
                        OidcSettingsAction::Reuse,
                        "Use saved settings",
                        "Continue to owner sign-in with the current provider.",
                    ),
                    SelectChoice::new(
                        OidcSettingsAction::Change,
                        "Change provider settings",
                        "Replace the issuer, client ID, and client secret.",
                    ),
                ],
                OidcSettingsAction::Reuse,
            )
            .context("failed to read the identity provider settings choice")?
        {
            PromptResult::Submitted(action) => action,
            PromptResult::Cancelled => return Ok(false),
        };
        Some(action)
    } else if matches!(
        current_state,
        SetupState::BootstrapPending | SetupState::IdpConfigured
    ) {
        Some(OidcSettingsAction::Change)
    } else {
        None
    };

    if settings_action == Some(OidcSettingsAction::Change) {
        terminal.note(
            "Identity provider",
            "Enter the issuer and client details from your provider. Leave the secret empty for a public client.\n\nProvider guides: https://docs.oore.build/team/access/oidc",
        )?;

        if session_token.is_none() {
            let operation = terminal.operation("Restoring the setup session");
            session_token = match acquire_session(&client, daemon_url, state_file).await {
                Ok(token) => {
                    operation.done("Setup session restored");
                    Some(token)
                }
                Err(error) => {
                    operation.failed("Setup session could not be restored");
                    return Err(error);
                }
            };
        }
        let token = session_token.as_ref().context("setup session is missing")?;

        loop {
            let issuer_url = match terminal
                .input("OIDC issuer URL", None, true)
                .context("failed to read the OIDC issuer URL")?
            {
                PromptResult::Submitted(value) => value,
                PromptResult::Cancelled => return Ok(false),
            };

            let client_id = match terminal
                .input("OIDC client ID", None, true)
                .context("failed to read the OIDC client ID")?
            {
                PromptResult::Submitted(value) => value,
                PromptResult::Cancelled => return Ok(false),
            };

            let client_secret = match terminal
                .password("OIDC client secret (optional)", true)
                .context("failed to read the OIDC client secret")?
            {
                PromptResult::Submitted(value) => value,
                PromptResult::Cancelled => return Ok(false),
            };

            let client_secret_opt = if client_secret.is_empty() {
                None
            } else {
                Some(client_secret)
            };

            let operation = terminal.operation("Checking identity provider settings");
            let oidc_url = format!("{}/v1/setup/oidc/configure", daemon_url);
            let oidc_resp = match client
                .post(&oidc_url)
                .bearer_auth(token)
                .json(&OidcConfigureRequest {
                    issuer_url,
                    client_id,
                    client_secret: client_secret_opt,
                    clear_client_secret: false,
                })
                .send()
                .await
                .context("failed to reach daemon for OIDC configuration")
            {
                Ok(response) => response,
                Err(error) => {
                    operation.failed("Identity provider settings could not be checked");
                    return Err(error);
                }
            };

            if oidc_resp.status().is_success() {
                let body: OidcConfigureResponse = match oidc_resp
                    .json()
                    .await
                    .context("failed to parse OIDC configure response")
                {
                    Ok(body) => body,
                    Err(error) => {
                        operation.failed("Identity provider response was invalid");
                        return Err(error);
                    }
                };
                current_state = SetupState::IdpConfigured;
                operation.done(format!(
                    "Identity provider connected: {}",
                    body.discovered_issuer
                ));
                break;
            } else {
                let status_code = oidc_resp.status();
                let msg = extract_error_message(oidc_resp).await;
                if status_code == reqwest::StatusCode::CONFLICT {
                    operation.done("Identity provider is already configured");
                    current_state = SetupState::IdpConfigured;
                    break;
                } else if status_code == reqwest::StatusCode::UNAUTHORIZED {
                    operation.failed("Setup session expired");
                    anyhow::bail!("Session expired or invalid. Please restart setup.");
                } else {
                    operation.failed("Identity provider rejected the settings");
                    terminal.note("Provider response", &msg)?;
                    let retry = matches!(
                        terminal
                            .confirm("Try the identity provider settings again?", true)
                            .context("failed to read the retry choice")?,
                        PromptResult::Submitted(true)
                    );
                    if !retry {
                        anyhow::bail!("OIDC configuration aborted by user.");
                    }
                }
            }
        }
    } else if settings_action == Some(OidcSettingsAction::Reuse) {
        terminal.note(
            "Identity provider",
            "Provider settings already exist. Oore will continue with owner verification.",
        )?;
    }

    // ── Step 3: Owner OIDC authentication ─────────────────────

    if current_state == SetupState::IdpConfigured {
        if session_token.is_none() {
            let operation = terminal.operation("Restoring the setup session");
            session_token = match acquire_session(&client, daemon_url, state_file).await {
                Ok(token) => {
                    operation.done("Setup session restored");
                    Some(token)
                }
                Err(error) => {
                    operation.failed("Setup session could not be restored");
                    return Err(error);
                }
            };
        }
        let token = session_token.as_ref().context("setup session is missing")?;

        let listener = callback_listener
            .take()
            .context("OIDC callback listener is unavailable")?;

        if terminal.is_remote_session() {
            let tunnel = ssh_loopback_tunnel_command(TERMINAL_OIDC_CALLBACK_PORT)?;
            terminal.note(
                "Browser connection",
                format!(
                    "Run this command on your computer in a second terminal:\n\n  {tunnel}\n\nKeep that terminal open during sign-in."
                ),
            )?;
            let tunnel_ready = matches!(
                terminal
                    .confirm("Is the SSH tunnel ready?", false)
                    .context("failed to read the SSH tunnel choice")?,
                PromptResult::Submitted(true)
            );
            if !tunnel_ready {
                terminal.note(
                    "Setup paused",
                    "Run `oore setup` when the SSH tunnel is ready.",
                )?;
                return Ok(false);
            }
        }

        let confirm = matches!(
            terminal
                .confirm("Continue to identity verification?", true)
                .context("failed to read the identity verification choice")?,
            PromptResult::Submitted(true)
        );

        if !confirm {
            terminal.note("Setup paused", "Resume later with `oore setup`.")?;
            return Ok(false);
        }

        let operation = terminal.operation("Starting identity verification");
        let start_url = format!("{}/v1/setup/owner/start-oidc", daemon_url);
        let start_resp = match client
            .post(&start_url)
            .bearer_auth(token)
            .json(&SetupOidcStartRequest {
                redirect_uri: TERMINAL_OIDC_REDIRECT_URI.to_string(),
            })
            .send()
            .await
            .context("failed to reach daemon for OIDC start")
        {
            Ok(response) => response,
            Err(error) => {
                operation.failed("Identity verification could not start");
                return Err(error);
            }
        };

        if start_resp.status().is_success() {
            let start_body: SetupOidcStartResponse = match start_resp
                .json()
                .await
                .context("failed to parse OIDC start response")
            {
                Ok(body) => body,
                Err(error) => {
                    operation.failed("Identity provider response was invalid");
                    return Err(error);
                }
            };
            operation.done("Identity provider sign-in is ready");

            if terminal.is_remote_session() {
                terminal.note("Open identity provider", &start_body.authorization_url)?;
            } else if !open_browser(&start_body.authorization_url) {
                terminal.note(
                    "Open identity provider",
                    format!(
                        "Oore could not open the browser. Open this URL:\n\n  {}",
                        start_body.authorization_url
                    ),
                )?;
            }

            let operation = terminal.operation("Waiting for identity provider sign-in");
            let callback_result = tokio::time::timeout(
                Duration::from_secs(10 * 60),
                wait_for_oidc_callback(listener),
            )
            .await
            .context("identity verification timed out after 10 minutes")
            .and_then(|result| result);
            let (code, callback_state) = match callback_result {
                Ok(callback) => {
                    operation.done("Identity provider returned to Oore");
                    callback
                }
                Err(error) => {
                    operation.failed("Identity provider sign-in did not finish");
                    return Err(error);
                }
            };

            let operation = terminal.operation("Verifying the owner identity");
            let verify_url = format!("{}/v1/setup/owner/verify-oidc", daemon_url);
            let verify_resp = match client
                .post(&verify_url)
                .bearer_auth(token)
                .json(&SetupOidcVerifyRequest {
                    code,
                    state: callback_state,
                })
                .send()
                .await
                .context("failed to reach daemon for OIDC verification")
            {
                Ok(response) => response,
                Err(error) => {
                    operation.failed("Owner identity could not be verified");
                    return Err(error);
                }
            };

            if verify_resp.status().is_success() {
                let body: SetupOidcVerifyResponse = match verify_resp
                    .json()
                    .await
                    .context("failed to parse OIDC verify response")
                {
                    Ok(body) => body,
                    Err(error) => {
                        operation.failed("Owner identity response was invalid");
                        return Err(error);
                    }
                };
                current_state = SetupState::OwnerCreated;
                operation.done("Owner identity verified");
                terminal.note(
                    "Owner account",
                    format!(
                        "Email: {}\nOIDC subject: {}",
                        body.owner_email, body.oidc_subject
                    ),
                )?;
            } else {
                let status_code = verify_resp.status();
                let msg = extract_error_message(verify_resp).await;
                if status_code == reqwest::StatusCode::CONFLICT {
                    operation.done("Owner account is already configured");
                    current_state = SetupState::OwnerCreated;
                } else if status_code == reqwest::StatusCode::UNAUTHORIZED {
                    operation.failed("Setup session expired");
                    anyhow::bail!("Session expired or invalid. Please restart setup.");
                } else {
                    operation.failed("Owner identity verification failed");
                    anyhow::bail!(
                        "OIDC verification failed (HTTP {}): {}",
                        status_code.as_u16(),
                        msg
                    );
                }
            }
        } else {
            let status_code = start_resp.status();
            let msg = extract_error_message(start_resp).await;
            if status_code == reqwest::StatusCode::CONFLICT {
                operation.done("Owner account is already configured");
                current_state = SetupState::OwnerCreated;
            } else if status_code == reqwest::StatusCode::UNAUTHORIZED {
                operation.failed("Setup session expired");
                anyhow::bail!("Session expired or invalid. Please restart setup.");
            } else {
                operation.failed("Identity verification could not start");
                anyhow::bail!("OIDC start failed (HTTP {}): {}", status_code.as_u16(), msg);
            }
        }
    } else if current_state == SetupState::OwnerCreated {
        terminal.note(
            "Owner account",
            "The owner already exists. Oore will finish the one-time setup flow.",
        )?;
    }

    // ── Step 4: Complete setup ──────────────────────────────────

    if current_state == SetupState::OwnerCreated {
        if session_token.is_none() {
            let operation = terminal.operation("Restoring the setup session");
            session_token = match acquire_session(&client, daemon_url, state_file).await {
                Ok(token) => {
                    operation.done("Setup session restored");
                    Some(token)
                }
                Err(error) => {
                    operation.failed("Setup session could not be restored");
                    return Err(error);
                }
            };
        }
        let token = session_token.as_ref().unwrap();

        let confirm = matches!(
            terminal
                .confirm("Finish setup and close the one-time setup flow?", true)
                .context("failed to read the setup completion choice")?,
            PromptResult::Submitted(true)
        );

        if !confirm {
            terminal.note("Setup paused", "Resume later with `oore setup`.")?;
            return Ok(false);
        }

        let operation = terminal.operation("Closing the one-time setup flow");
        let complete_url = format!("{}/v1/setup/complete", daemon_url);
        let complete_resp = match client
            .post(&complete_url)
            .bearer_auth(token)
            .send()
            .await
            .context("failed to reach daemon for setup completion")
        {
            Ok(response) => response,
            Err(error) => {
                operation.failed("Setup could not be finalized");
                return Err(error);
            }
        };

        if complete_resp.status().is_success() {
            let body: SetupCompleteResponse = match complete_resp
                .json()
                .await
                .context("failed to parse setup complete response")
            {
                Ok(body) => body,
                Err(error) => {
                    operation.failed("Setup completion response was invalid");
                    return Err(error);
                }
            };
            operation.done(format!(
                "Access setup is complete for instance {}",
                body.instance_id
            ));
        } else {
            let status_code = complete_resp.status();
            let msg = extract_error_message(complete_resp).await;
            if status_code == reqwest::StatusCode::CONFLICT {
                operation.done("Access setup is already complete");
            } else if status_code == reqwest::StatusCode::UNAUTHORIZED {
                operation.failed("Setup session expired");
                anyhow::bail!("Session expired or invalid. Please restart setup.");
            } else {
                operation.failed("Setup could not be finalized");
                anyhow::bail!(
                    "Setup completion failed (HTTP {}): {}",
                    status_code.as_u16(),
                    msg
                );
            }
        }
    }

    terminal.note(
        "Network access",
        "OIDC protects sign-in. It does not publish this Mac. Connect an HTTPS proxy or tunnel before other people use Oore.",
    )?;

    Ok(true)
}

async fn handle_login(args: LoginArgs, quiet: bool) -> anyhow::Result<()> {
    let daemon_url = resolve_daemon_url(args.daemon_url.as_deref())?;
    let client = endpoint_http_client_builder(&daemon_url)
        .build()
        .context("failed to build the login client")?;

    if let Some(token) = args.token.as_deref() {
        let profile = fetch_user_profile(&client, &daemon_url, token).await.map_err(|e| {
            anyhow::anyhow!(
                "{e}\nToken was rejected. Re-authenticate in web UI or run `oore login` without --token in local mode."
            )
        })?;
        let mut cfg = load_cli_config()?;
        cfg.daemon_url = Some(daemon_url.clone());
        cfg.session_token = Some(token.to_string());
        let config_path = save_cli_config(&cfg)?;

        if args.json {
            let output = serde_json::json!({
                "ok": true,
                "auth_mode": "token_import",
                "daemon_url": daemon_url,
                "config_path": config_path,
                "user": profile.user,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
            return Ok(());
        }

        if quiet {
            return Ok(());
        }

        println!("Token validated and stored.");
        println!("Daemon: {}", daemon_url);
        println!("User:   {}", profile.user.email);
        println!("Config: {}", config_path.display());
        return Ok(());
    }

    let login_url = format!("{}/v1/auth/local/login", daemon_url.trim_end_matches('/'));
    let response = client
        .post(&login_url)
        .json(&LocalLoginRequest {
            email: args.email,
            recovery_capability: None,
        })
        .send()
        .await
        .with_context(|| format!("failed to reach daemon at {daemon_url}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let msg = extract_error_message(response).await;
        anyhow::bail!(
            "local login failed at {daemon_url} (HTTP {}): {}",
            status.as_u16(),
            msg
        );
    }

    let login_body: LocalLoginResponse = response
        .json()
        .await
        .context("failed to parse local login response")?;
    let mut cfg = load_cli_config()?;
    cfg.daemon_url = Some(daemon_url.clone());
    cfg.session_token = Some(login_body.session_token.clone());
    let config_path = save_cli_config(&cfg)?;

    if args.json {
        let output = serde_json::json!({
            "ok": true,
            "auth_mode": "local_login",
            "daemon_url": daemon_url,
            "config_path": config_path,
            "expires_at": login_body.expires_at,
            "user": login_body.user,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if quiet {
        return Ok(());
    }

    println!("Login succeeded (local mode).");
    println!("Daemon:  {}", daemon_url);
    println!("User:    {}", login_body.user.email);
    println!("Expires: {}", format_epoch_local(login_body.expires_at));
    println!("Config:  {}", config_path.display());
    Ok(())
}

async fn handle_recovery(args: RecoveryArgs) -> anyhow::Result<()> {
    let ttl = parse_ttl(&args.ttl)?;
    let ttl_seconds = ttl.as_secs();
    if !(LOCAL_RECOVERY_MIN_TTL_SECS..=LOCAL_RECOVERY_MAX_TTL_SECS).contains(&ttl_seconds) {
        anyhow::bail!(
            "recovery TTL must be between {}s and {}s",
            LOCAL_RECOVERY_MIN_TTL_SECS,
            LOCAL_RECOVERY_MAX_TTL_SECS
        );
    }

    let socket_path = recovery_socket_path(args.state_file.as_deref())?;
    validate_recovery_socket(&socket_path)?;
    let mut stream = UnixStream::connect(&socket_path).await.with_context(|| {
        format!(
            "failed to connect to local recovery socket {}",
            socket_path.display()
        )
    })?;
    let request = LocalRecoveryMintRequest {
        email: args.email,
        ttl_seconds,
    };
    let mut encoded = serde_json::to_vec(&request).context("failed to encode recovery request")?;
    encoded.push(b'\n');
    stream
        .write_all(&encoded)
        .await
        .context("failed to write recovery request")?;
    stream
        .shutdown()
        .await
        .context("failed to finish recovery request")?;

    let mut response_bytes = Vec::new();
    stream
        .take(4097)
        .read_to_end(&mut response_bytes)
        .await
        .context("failed to read recovery response")?;
    if response_bytes.len() > 4096 {
        anyhow::bail!("local recovery response exceeded 4096 bytes");
    }
    let response: LocalRecoveryMintResponse =
        serde_json::from_slice(&response_bytes).context("invalid local recovery response")?;
    let (capability, expires_at, user_email) = match response {
        LocalRecoveryMintResponse::Success {
            capability,
            expires_at,
            user_email,
        } => (capability, expires_at, user_email),
        LocalRecoveryMintResponse::Error { error } => {
            anyhow::bail!("local recovery failed ({}): {}", error.code, error.error)
        }
    };

    let mut recovery_url =
        url::Url::parse(&args.web_url).context("web URL must be an absolute URL")?;
    if !matches!(recovery_url.scheme(), "http" | "https")
        || !recovery_url.username().is_empty()
        || recovery_url.password().is_some()
        || recovery_url.host_str().is_none()
    {
        anyhow::bail!("web URL must be an HTTP(S) URL without embedded credentials");
    }
    recovery_url.set_path("/login");
    recovery_url.set_query(None);
    recovery_url.set_fragment(Some(&format!("recovery={capability}")));

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "recovery_url": recovery_url.as_str(),
                "expires_at": expires_at,
                "user_email": user_email,
            }))?
        );
        return Ok(());
    }

    println!("Recovery link created for {user_email}.");
    println!("Expires: {}", format_epoch_local(expires_at));
    println!("Open this single-use URL:");
    println!("{}", recovery_url.as_str());
    Ok(())
}

async fn handle_status(args: StatusArgs) -> anyhow::Result<()> {
    let daemon_url = resolve_daemon_url(args.daemon_url.as_deref())?;
    let client = endpoint_http_client_builder(&daemon_url)
        .build()
        .context("failed to build the status client")?;
    let setup_status = fetch_setup_status(&client, &daemon_url).await?;
    let installation_state =
        matching_install_manifest(&daemon_url).map(|manifest| manifest.lifecycle.state);
    let installation_setup_in_progress =
        installation_state.is_some_and(|state| state != install_manifest::InstallState::Ready);
    let web_url = if setup_status.runtime_mode == RuntimeMode::Remote
        && setup_status.remote_auth_mode == RemoteAuthMode::TrustedProxy
    {
        None
    } else {
        ready_complete_web_url(&daemon_url)
    };
    let mut summary = StatusSummary {
        daemon_url: daemon_url.clone(),
        web_url,
        installation_state,
        setup_status,
        authenticated: false,
        queue_depth: None,
        active_builds: None,
        recent_builds: None,
        runners: None,
    };

    let session_token = resolve_session_token(args.token.as_deref(), &daemon_url)?;
    if let Some(token) = session_token.as_deref() {
        match fetch_user_profile(&client, &daemon_url, token).await {
            Ok(_) => {
                summary.authenticated = true;
                let (queued, running, recent, runners) = tokio::try_join!(
                    fetch_build_list(&client, &daemon_url, token, Some("queued"), Some(1)),
                    fetch_build_list(&client, &daemon_url, token, Some("running"), Some(1)),
                    fetch_build_list(&client, &daemon_url, token, None, Some(5)),
                    fetch_runner_list(&client, &daemon_url, token),
                )?;
                summary.queue_depth = Some(queued.total);
                summary.active_builds = Some(queued.total + running.total);
                summary.recent_builds = Some(recent.builds);
                summary.runners = Some(runners.runners);
            }
            Err(err) => {
                if args.token.is_some() {
                    return Err(err);
                }
                eprintln!("Warning: stored token is invalid or expired ({err})");
            }
        }
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    println!("oore status");
    println!();
    println!("Daemon:   {}", summary.daemon_url);
    if let Some(web_url) = summary.web_url.as_deref() {
        println!("Web UI:   {web_url}");
    }
    println!("Instance: {}", summary.setup_status.instance_id);
    println!("State:    {}", summary.setup_status.state);
    let access = match (
        summary.setup_status.runtime_mode,
        summary.setup_status.remote_auth_mode,
    ) {
        (RuntimeMode::Local, _) => "this device (passwordless loopback)",
        (RuntimeMode::Remote, RemoteAuthMode::Oidc) => "identity provider (OIDC)",
        (RuntimeMode::Remote, RemoteAuthMode::TrustedProxy) => "trusted access proxy",
    };
    println!("Access:   {access}");
    println!(
        "Setup:    {}",
        if summary.setup_status.setup_mode || installation_setup_in_progress {
            "in_progress"
        } else {
            "complete"
        }
    );
    let setup_in_progress = summary.setup_status.setup_mode || installation_setup_in_progress;
    if setup_in_progress {
        println!();
        println!("Next: run `oore setup` to continue setup.");
    }

    if summary.authenticated {
        println!();
        println!("CLI session:   signed in");
        if let Some(queue_depth) = summary.queue_depth {
            println!("Queue depth:   {}", queue_depth);
        }
        if let Some(active_builds) = summary.active_builds {
            println!("Active builds: {}", active_builds);
        }
        if let Some(runners) = &summary.runners {
            let online = runners
                .iter()
                .filter(|runner| runner.status == "online" || runner.status == "busy")
                .count();
            println!(
                "Runners:       {} total ({} online/busy)",
                runners.len(),
                online
            );
        }
        if let Some(recent) = &summary.recent_builds {
            if recent.is_empty() {
                println!("Recent builds: none");
            } else {
                println!("Recent builds:");
                for build in recent {
                    println!(
                        "  - #{} {} ({})",
                        build.build_number, build.status, build.trigger_type
                    );
                }
            }
        }
    } else {
        println!();
        println!("CLI session:   not signed in");
        if !setup_in_progress && summary.setup_status.runtime_mode == RuntimeMode::Local {
            println!(
                "Tip: run `oore login` for queue, build, and runner details in this terminal."
            );
        } else if !setup_in_progress {
            match summary.setup_status.remote_auth_mode {
                RemoteAuthMode::Oidc if summary.web_url.is_some() => {
                    println!("Tip: sign in through the Web UI shown above.");
                }
                RemoteAuthMode::Oidc => {
                    println!("Tip: sign in through a connected Web node or hosted UI.");
                }
                RemoteAuthMode::TrustedProxy => {
                    println!(
                        "Tip: sign in through the HTTPS address managed by your trusted access proxy."
                    );
                }
            }
            println!(
                "Interactive CLI sign-in is not available for Remote access in this release. `oore login --token` can import an existing session token."
            );
        }
    }

    Ok(())
}

fn ready_complete_web_url(daemon_url: &str) -> Option<String> {
    let manifest = matching_install_manifest(daemon_url)?;
    if manifest.profile != install_manifest::InstallProfile::Complete
        || manifest.lifecycle.state != install_manifest::InstallState::Ready
    {
        return None;
    }
    let mut address: SocketAddr = manifest.lifecycle.web_listen.as_deref()?.parse().ok()?;
    if address.ip().is_unspecified() {
        address.set_ip(if address.is_ipv6() {
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        } else {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        });
    }
    Some(format!("http://{address}"))
}

fn matching_install_manifest(daemon_url: &str) -> Option<install_manifest::InstallManifest> {
    let install_root = resolve_install_root().ok()?;
    let manifest =
        install_manifest::InstallManifest::load(&install_root.join("install-manifest.json"))
            .ok()?;
    let requested_daemon = url::Url::parse(daemon_url).ok()?;
    let installed_daemon = url::Url::parse(manifest.lifecycle.backend_url.as_deref()?).ok()?;
    if requested_daemon != installed_daemon {
        return None;
    }
    Some(manifest)
}

// ── Runner execution engine ─────────────────────────────────────

struct RunnerRegistrationLock {
    file: fs::File,
}

impl RunnerRegistrationLock {
    fn acquire(config_path: &Path) -> anyhow::Result<Self> {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let parent = config_path
            .parent()
            .context("runner config has no parent directory")?;
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create runner config directory {}",
                parent.display()
            )
        })?;
        let parent_metadata = fs::symlink_metadata(parent)?;
        anyhow::ensure!(
            parent_metadata.file_type().is_dir()
                && !parent_metadata.file_type().is_symlink()
                && parent_metadata.uid() == current_effective_uid()
                && parent_metadata.permissions().mode() & 0o022 == 0,
            "runner config directory has unsafe ownership or permissions: {}",
            parent.display()
        );
        let path = parent.join(".runner-registration.lock");
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .with_context(|| {
                format!("failed to open runner registration lock {}", path.display())
            })?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        let metadata = file.metadata()?;
        anyhow::ensure!(
            metadata.file_type().is_file() && metadata.uid() == current_effective_uid(),
            "runner registration lock has unsafe ownership or type"
        );
        // SAFETY: `file` owns this descriptor for the lifetime of the guard.
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if matches!(
                error.raw_os_error(),
                Some(code) if code == libc::EWOULDBLOCK || code == libc::EAGAIN
            ) {
                anyhow::bail!("another runner registration is active");
            }
            return Err(error).context("failed to lock runner registration");
        }
        Ok(Self { file })
    }
}

impl Drop for RunnerRegistrationLock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        // SAFETY: `self.file` still owns this descriptor during `drop`.
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct RunnerRegistrationConfigSnapshot {
    contents: Option<Vec<u8>>,
    mode: Option<u32>,
}

impl RunnerRegistrationConfigSnapshot {
    fn capture(path: &Path) -> anyhow::Result<Self> {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let mut file = match fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    contents: None,
                    mode: None,
                });
            }
            Err(error) => return Err(error.into()),
        };
        let metadata = file.metadata()?;
        anyhow::ensure!(
            metadata.file_type().is_file()
                && metadata.uid() == current_effective_uid()
                && metadata.permissions().mode() & 0o022 == 0,
            "runner config has unsafe ownership or permissions: {}",
            path.display()
        );
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        Ok(Self {
            contents: Some(contents),
            mode: Some(metadata.permissions().mode() & 0o777),
        })
    }

    fn ensure_unchanged(&self, path: &Path) -> anyhow::Result<()> {
        anyhow::ensure!(
            &Self::capture(path)? == self,
            "runner config changed while registration was in progress"
        );
        Ok(())
    }

    fn restore(&self, path: &Path) -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        match self.contents.as_deref() {
            Some(contents) => {
                let parent = path
                    .parent()
                    .context("runner config has no parent directory")?;
                let mut staged = tempfile::Builder::new()
                    .prefix(".runner-config-rollback-")
                    .tempfile_in(parent)?;
                let mode = self.mode.context("runner config snapshot has no mode")?;
                staged
                    .as_file()
                    .set_permissions(fs::Permissions::from_mode(mode))?;
                staged.as_file_mut().write_all(contents)?;
                staged.as_file_mut().sync_all()?;
                staged.persist(path).map_err(|error| error.error)?;
                fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
                Ok(())
            }
            None => match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.into()),
            },
        }
    }
}

async fn handle_runner_register(args: RunnerRegisterArgs, quiet: bool) -> anyhow::Result<()> {
    let install_root = resolve_install_root()?;
    let _lifecycle_lock = install_lock::InstallLock::acquire(&install_root)?;
    handle_runner_register_with_lifecycle_lock(args, quiet).await
}

async fn handle_runner_register_with_lifecycle_lock(
    args: RunnerRegisterArgs,
    quiet: bool,
) -> anyhow::Result<()> {
    oore_runner::require_safe_daemon_url(&args.daemon_url)?;
    let name = args.name.unwrap_or_else(oore_runner::get_hostname);
    let config_path = runner_config_path(None)?;
    let _registration_lock = RunnerRegistrationLock::acquire(&config_path)?;
    let previous_config = RunnerRegistrationConfigSnapshot::capture(&config_path)?;
    if let Some(contents) = previous_config.contents.as_deref() {
        serde_json::from_slice::<RunnerConfig>(contents)
            .with_context(|| format!("failed to parse runner config {}", config_path.display()))?;
    }

    let capabilities = oore_runner::detect_capabilities().await;

    let client = endpoint_http_client_builder(&args.daemon_url)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .context("failed to create the runner registration client")?;
    let resp = client
        .post(format!("{}/v1/runners/register", args.daemon_url))
        .bearer_auth(&args.token)
        .json(&serde_json::json!({
            "name": name,
            "capabilities": capabilities,
            "labels": args.labels,
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let err: ApiError = resp
            .json()
            .await
            .context("failed to parse error response")?;
        anyhow::bail!("Registration failed: {} - {}", err.code, err.error);
    }

    let result: RegisterRunnerResponse = resp.json().await?;

    let config = RunnerConfig {
        runner_id: result.runner.id.clone(),
        runner_token: result.token,
        daemon_url: args.daemon_url.clone(),
        name: result.runner.name.clone(),
    };

    if let Err(error) = previous_config.ensure_unchanged(&config_path) {
        let remote_rollback =
            rollback_registered_runner(&client, &args.daemon_url, &args.token, &result.runner.id)
                .await;
        return match remote_rollback {
            Ok(()) => Err(error.context("the remote registration was rolled back")),
            Err(rollback_error) => Err(error.context(format!(
                "remote rollback state is unknown: {rollback_error:#}; verify runner {} on the control plane",
                result.runner.id
            ))),
        };
    }

    if let Err(write_error) = write_runner_config(&config_path, &config) {
        let config_rollback = previous_config.restore(&config_path);
        let remote_rollback =
            rollback_registered_runner(&client, &args.daemon_url, &args.token, &result.runner.id)
                .await;
        return match (config_rollback, remote_rollback) {
            (Ok(()), Ok(())) => Err(write_error.context(
                "failed to save the runner configuration; the remote registration was rolled back",
            )),
            (Err(config_rollback), Ok(())) => Err(anyhow::anyhow!(
                "failed to save the runner configuration: {write_error:#}; the remote registration was rolled back, but restoring the local runner configuration failed: {config_rollback:#}"
            )),
            (Ok(()), Err(remote_rollback)) => Err(anyhow::anyhow!(
                "failed to save the runner configuration: {write_error:#}; remote rollback state for runner {} is unknown: {rollback_error:#}. Verify it on the control plane",
                result.runner.id,
                rollback_error = remote_rollback,
            )),
            (Err(config_rollback), Err(remote_rollback)) => Err(anyhow::anyhow!(
                "failed to save the runner configuration: {write_error:#}; restoring the local configuration also failed: {config_rollback:#}; remote rollback state for runner {} is unknown: {remote_rollback:#}. Verify it on the control plane",
                result.runner.id,
            )),
        };
    }

    if !quiet {
        println!("Runner registered successfully!");
        println!("  ID: {}", result.runner.id);
        println!("  Name: {}", result.runner.name);
        println!("  Config saved to: {}", config_path.display());
        println!("\nStart the runner with: oore runner start");
    }

    Ok(())
}

async fn rollback_registered_runner(
    client: &reqwest::Client,
    daemon_url: &str,
    auth_token: &str,
    runner_id: &str,
) -> anyhow::Result<()> {
    let response = client
        .delete(format!("{daemon_url}/v1/runners/{runner_id}"))
        .bearer_auth(auth_token)
        .send()
        .await
        .context("failed to request remote runner rollback")?;
    let status = response.status();
    if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
        return Ok(());
    }

    let body = response.text().await.unwrap_or_default();
    if let Ok(error) = serde_json::from_str::<ApiError>(&body) {
        anyhow::bail!("{} - {}", error.code, error.error);
    }
    anyhow::bail!("daemon returned HTTP {status}")
}

async fn handle_runner_start(args: RunnerStartArgs) -> anyhow::Result<()> {
    let config_path = runner_config_path(args.config)?;

    let config: RunnerConfig = serde_json::from_str(
        &fs::read_to_string(&config_path)
            .context("Runner not registered. Run 'oore runner register' first.")?,
    )?;

    let daemon_url = args.daemon_url.unwrap_or(config.daemon_url.clone());

    oore_runner::run_runner_forever(config, Some(daemon_url)).await
}

fn runner_config_path(value: Option<String>) -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    let path = match value {
        Some(path) if path == "~" => home,
        Some(path) if path.starts_with("~/") => home.join(&path[2..]),
        Some(path) => PathBuf::from(path),
        None => resolve_install_root()?.join("runner.json"),
    };
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()
            .context("failed to resolve the current directory")?
            .join(path))
    }
}

fn read_runner_config(path: &Path) -> anyhow::Result<Option<RunnerConfig>> {
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read runner config {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse runner config {}", path.display()))
        .map(Some)
}

fn write_runner_config(path: &Path, config: &RunnerConfig) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create runner config directory {}",
            parent.display()
        )
    })?;
    let mut staged = tempfile::Builder::new()
        .prefix(".runner-config-")
        .tempfile_in(parent)
        .with_context(|| format!("failed to stage runner config in {}", parent.display()))?;
    staged
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    serde_json::to_writer_pretty(staged.as_file_mut(), config)
        .context("failed to serialize runner config")?;
    staged.as_file_mut().write_all(b"\n")?;
    staged.as_file_mut().sync_all()?;
    staged
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to replace runner config {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[derive(Debug)]
struct ManagedRunnerEnrollmentSnapshot {
    daemon_url: String,
    state_file: Option<String>,
    rollback: ManagedRunnerRollback,
}

impl ManagedRunnerEnrollmentSnapshot {
    async fn restore(&self, runner_id: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.rollback.runner_id == runner_id,
            "managed runner rollback does not match the installed runner"
        );
        let response = call_operator(
            &self.daemon_url,
            self.state_file.as_deref(),
            OperatorRequest::RestoreManagedRunner {
                rollback: self.rollback.clone(),
            },
        )
        .await?;
        match response {
            OperatorResponse::ManagedRunnerRestored {
                runner_id: restored,
                ..
            } if restored == runner_id => Ok(()),
            _ => anyhow::bail!("oored returned an unexpected managed runner rollback response"),
        }
    }
}

fn generate_operator_runner_token() -> String {
    use rand::RngCore;

    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

async fn call_idempotent_operator_once_more(
    daemon_url: &str,
    state_file: Option<&str>,
    request: OperatorRequest,
) -> anyhow::Result<OperatorResponse> {
    match call_operator(daemon_url, state_file, request.clone()).await {
        Ok(response) => Ok(response),
        Err(first_error) => call_operator(daemon_url, state_file, request)
            .await
            .with_context(|| {
                format!(
                    "the idempotent operator request failed twice; the first attempt failed: {first_error:#}"
                )
            }),
    }
}

async fn ensure_runner_service_config(
    args: &RunnerServiceArgs,
    config_path: PathBuf,
    adopt_installed_registration: bool,
    credential_action: &ManagedRunnerCredentialAction,
) -> anyhow::Result<(
    PathBuf,
    RunnerConfig,
    bool,
    Option<ManagedRunnerEnrollmentSnapshot>,
)> {
    anyhow::ensure!(
        credential_action == &ManagedRunnerCredentialAction::Reuse || args.managed_local,
        "runner token rotation requires a managed local runner"
    );
    let existing = read_runner_config(&config_path)?;
    if let ManagedRunnerCredentialAction::Rotate { expected_runner_id } = credential_action {
        let configured_runner_id = existing
            .as_ref()
            .context("managed runner config is missing")?
            .runner_id
            .as_str();
        anyhow::ensure!(
            configured_runner_id == expected_runner_id.as_str(),
            "managed runner identity changed before token rotation"
        );
    }

    if !args.managed_local {
        let config = existing.context(
            "runner is not registered; run `oore runner register` before installing the service",
        )?;
        oore_runner::require_safe_daemon_url(&config.daemon_url)?;
        write_runner_config(&config_path, &config)?;
        return Ok((config_path, config, false, None));
    }

    let database = resolve_db_path(args.state_file.as_deref())?;
    if !database.is_file() {
        anyhow::bail!(
            "local Oore database not found at {}; start or set up the backend before installing its managed runner",
            database.display()
        );
    }

    let daemon_url = match args.daemon_url.as_deref() {
        Some(url) => url.trim().to_string(),
        None => match existing.as_ref() {
            Some(config) => config.daemon_url.clone(),
            None => resolve_daemon_url(None)?,
        },
    };
    oore_runner::require_safe_daemon_url(&daemon_url)?;

    let name = args
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(oore_runner::get_hostname);
    if name.is_empty() || name.len() > 255 {
        anyhow::bail!("runner name must be between 1 and 255 characters");
    }

    let capabilities: ManagedRunnerCapabilities =
        serde_json::from_value(oore_runner::detect_capabilities().await)
            .context("runner capabilities did not match the operator protocol")?;
    let response = call_idempotent_operator_once_more(
        &daemon_url,
        args.state_file.as_deref(),
        OperatorRequest::EnsureManagedRunner {
            operation_id: uuid::Uuid::new_v4().to_string(),
            name: name.clone(),
            capabilities,
            runner_id: existing.as_ref().map(|config| config.runner_id.clone()),
            runner_token: existing
                .as_ref()
                .filter(|_| credential_action == &ManagedRunnerCredentialAction::Reuse)
                .map(|config| config.runner_token.clone()),
            proposed_runner_id: uuid::Uuid::new_v4().to_string(),
            proposed_runner_token: generate_operator_runner_token(),
            adopt_installed_registration,
        },
    )
    .await?;
    let (runner_id, runner_name, runner_token, enrolled, rollback) = match response {
        OperatorResponse::ManagedRunner {
            runner_id,
            runner_name,
            runner_token,
            enrolled,
            rollback,
            ..
        } => (runner_id, runner_name, runner_token, enrolled, rollback),
        _ => anyhow::bail!("oored returned an unexpected managed runner response"),
    };
    let config = RunnerConfig {
        runner_id,
        runner_token,
        daemon_url: daemon_url.clone(),
        name: runner_name,
    };
    let enrollment_snapshot = rollback.map(|rollback| ManagedRunnerEnrollmentSnapshot {
        daemon_url,
        state_file: args.state_file.clone(),
        rollback,
    });
    if let ManagedRunnerCredentialAction::Rotate { expected_runner_id } = credential_action {
        anyhow::ensure!(
            enrolled && enrollment_snapshot.is_some(),
            "oored did not rotate the managed runner token"
        );
        if config.runner_id.as_str() != expected_runner_id.as_str() {
            let snapshot = enrollment_snapshot
                .as_ref()
                .context("oored did not provide a managed runner rollback")?;
            snapshot
                .restore(&config.runner_id)
                .await
                .context("managed runner identity changed and rollback failed")?;
            anyhow::bail!("managed runner identity changed during token rotation");
        }
    }
    if let Err(error) = write_runner_config(&config_path, &config) {
        if let Some(snapshot) = enrollment_snapshot.as_ref()
            && let Err(rollback_error) = snapshot.restore(&config.runner_id).await
        {
            anyhow::bail!(
                "{error:#}; restoring the managed runner enrollment also failed: {rollback_error:#}"
            );
        }
        return Err(error);
    }
    Ok((config_path, config, enrolled, enrollment_snapshot))
}

fn managed_local_runner_config_path() -> anyhow::Result<PathBuf> {
    Ok(resolve_install_root()?.join("managed-runner.json"))
}

fn seed_managed_local_runner_config(source: &Path, destination: &Path) -> anyhow::Result<()> {
    if paths_refer_to_same_file(source, destination) || destination.is_file() {
        return Ok(());
    }
    let config = read_runner_config(source)?.with_context(|| {
        format!(
            "installed managed runner config is missing: {}",
            source.display()
        )
    })?;
    write_runner_config(destination, &config)
}

fn remove_migrated_managed_runner_config(source: &Path, destination: &Path) -> anyhow::Result<()> {
    if paths_refer_to_same_file(source, destination) {
        return Ok(());
    }
    fs::remove_file(source).with_context(|| {
        format!(
            "failed to remove migrated managed runner config {}",
            source.display()
        )
    })
}

async fn runner_config_is_locally_managed(
    config_path: &Path,
    state_file: Option<&str>,
) -> anyhow::Result<bool> {
    let Some(config) = read_runner_config(config_path)? else {
        return Ok(false);
    };
    let response = call_operator(
        &config.daemon_url,
        state_file,
        OperatorRequest::RunnerRegistrationMatches {
            runner_id: config.runner_id,
            runner_token: config.runner_token,
            allow_manual_for_adoption: true,
        },
    )
    .await?;
    match response {
        OperatorResponse::RegistrationMatch { matches, .. } => Ok(matches),
        _ => anyhow::bail!("oored returned an unexpected runner registration response"),
    }
}

async fn migrate_managed_runner_service_config(
    source: &Path,
    destination: &Path,
    state_file: Option<&str>,
) -> anyhow::Result<()> {
    if !runner_config_is_locally_managed(source, state_file).await? {
        anyhow::bail!(
            "the installed runner config does not match its backend registration; uninstall it before installing this backend's managed local runner"
        );
    }
    seed_managed_local_runner_config(source, destination)
}

const RUNNER_DRAIN_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const RUNNER_CLAIM_BARRIER_RENEW_INTERVAL: Duration = Duration::from_secs(60);
const RUNNER_CLAIM_BARRIER_PLANNED_OUTAGE_LIMIT: Duration = Duration::from_secs(4 * 60);
const RUNNER_CLAIM_BARRIER_PAUSE_TIMEOUT: Duration = Duration::from_secs(10);

struct RunnerClaimBarrier {
    daemon_url: String,
    state_file: Option<String>,
    token: String,
    renewal: tokio::task::JoinHandle<()>,
    renewal_failure: tokio::sync::watch::Receiver<Option<String>>,
    renewal_pause: tokio::sync::watch::Sender<Option<Instant>>,
    renewal_paused: tokio::sync::watch::Receiver<bool>,
    released: std::sync::atomic::AtomicBool,
}

async fn renew_runner_claim_barrier(
    daemon_url: &str,
    state_file: Option<&str>,
    token: &str,
) -> anyhow::Result<()> {
    let response = call_operator(
        daemon_url,
        state_file,
        OperatorRequest::RunnerBarrierRenew {
            lease_token: token.to_string(),
        },
    )
    .await?;
    match response {
        OperatorResponse::Barrier { .. } => Ok(()),
        _ => anyhow::bail!("oored returned an unexpected runner barrier response"),
    }
}

fn spawn_runner_claim_barrier_renewal(
    daemon_url: String,
    state_file: Option<String>,
    token: String,
    renewal_failure: tokio::sync::watch::Sender<Option<String>>,
    mut renewal_pause: tokio::sync::watch::Receiver<Option<Instant>>,
    renewal_paused: tokio::sync::watch::Sender<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if renewal_pause.borrow().is_some() {
                let _ = renewal_paused.send(true);
                if renewal_pause.changed().await.is_err() {
                    return;
                }
                continue;
            }
            let _ = renewal_paused.send(false);
            tokio::select! {
                changed = renewal_pause.changed() => {
                    if changed.is_err() {
                        return;
                    }
                }
                _ = tokio::time::sleep(RUNNER_CLAIM_BARRIER_RENEW_INTERVAL) => {
                    if let Err(error) = renew_runner_claim_barrier(
                        &daemon_url,
                        state_file.as_deref(),
                        &token,
                    )
                    .await
                    {
                        let message = format!("runner claim barrier renewal failed: {error:#}");
                        eprintln!("Warning: {message}");
                        let _ = renewal_failure.send(Some(message));
                        return;
                    }
                }
            }
        }
    })
}

async fn acquire_runner_claim_barrier(
    daemon_url: &str,
    state_file: Option<&str>,
) -> anyhow::Result<RunnerClaimBarrier> {
    let response = call_operator(
        daemon_url,
        state_file,
        OperatorRequest::RunnerBarrierAcquire,
    )
    .await?;
    let token = match response {
        OperatorResponse::Barrier { lease_token, .. } => lease_token,
        _ => anyhow::bail!("oored returned an unexpected runner barrier response"),
    };
    let daemon_url = daemon_url.to_string();
    let state_file = state_file.map(str::to_string);
    let (renewal_failure_tx, renewal_failure) = tokio::sync::watch::channel(None);
    let (renewal_pause, renewal_pause_rx) = tokio::sync::watch::channel(None);
    let (renewal_paused_tx, renewal_paused) = tokio::sync::watch::channel(false);
    let renewal = spawn_runner_claim_barrier_renewal(
        daemon_url.clone(),
        state_file.clone(),
        token.clone(),
        renewal_failure_tx,
        renewal_pause_rx,
        renewal_paused_tx,
    );
    Ok(RunnerClaimBarrier {
        daemon_url,
        state_file,
        token,
        renewal,
        renewal_failure,
        renewal_pause,
        renewal_paused,
        released: std::sync::atomic::AtomicBool::new(false),
    })
}

impl RunnerClaimBarrier {
    fn planned_daemon_outage_active(&self) -> bool {
        self.renewal_pause.borrow().is_some()
    }

    fn ensure_renewal_task_healthy(&self) -> anyhow::Result<()> {
        if let Some(error) = self.renewal_failure.borrow().clone() {
            anyhow::bail!(error);
        }
        anyhow::ensure!(
            !self.renewal.is_finished(),
            "runner claim barrier renewal stopped unexpectedly"
        );
        Ok(())
    }

    async fn ensure_healthy(&self) -> anyhow::Result<()> {
        self.ensure_renewal_task_healthy()?;
        if let Some(deadline) = *self.renewal_pause.borrow() {
            anyhow::ensure!(
                Instant::now() <= deadline,
                "the planned daemon outage exceeded the runner claim barrier safety window"
            );
            return Ok(());
        }
        renew_runner_claim_barrier(&self.daemon_url, self.state_file.as_deref(), &self.token)
            .await?;
        self.ensure_renewal_task_healthy()
    }

    async fn wait_for_renewal_pause(&self, expected: bool) -> anyhow::Result<()> {
        let mut paused = self.renewal_paused.clone();
        tokio::time::timeout(RUNNER_CLAIM_BARRIER_PAUSE_TIMEOUT, async {
            while *paused.borrow() != expected {
                paused
                    .changed()
                    .await
                    .map_err(|_| anyhow::anyhow!("runner claim barrier renewal stopped"))?;
            }
            Ok::<(), anyhow::Error>(())
        })
        .await
        .context("timed out while changing runner claim barrier renewal state")??;
        self.ensure_renewal_task_healthy()
    }

    async fn begin_planned_daemon_outage(&self) -> anyhow::Result<()> {
        if self.renewal_pause.borrow().is_some() {
            return self.ensure_healthy().await;
        }
        self.ensure_healthy().await?;
        let deadline = Instant::now() + RUNNER_CLAIM_BARRIER_PLANNED_OUTAGE_LIMIT;
        self.renewal_pause
            .send(Some(deadline))
            .map_err(|_| anyhow::anyhow!("runner claim barrier renewal stopped"))?;
        if let Err(error) = self.wait_for_renewal_pause(true).await {
            let _ = self.renewal_pause.send(None);
            return Err(error);
        }
        Ok(())
    }

    async fn end_planned_daemon_outage(&self) -> anyhow::Result<()> {
        let deadline = self
            .renewal_pause
            .borrow()
            .as_ref()
            .copied()
            .context("runner claim barrier is not in a planned daemon outage")?;
        let expired = Instant::now() > deadline;
        renew_runner_claim_barrier(&self.daemon_url, self.state_file.as_deref(), &self.token)
            .await?;
        self.renewal_pause
            .send(None)
            .map_err(|_| anyhow::anyhow!("runner claim barrier renewal stopped"))?;
        self.wait_for_renewal_pause(false).await?;
        anyhow::ensure!(
            !expired,
            "the planned daemon outage exceeded the runner claim barrier safety window"
        );
        Ok(())
    }

    async fn end_planned_daemon_outage_and_release(&self) -> anyhow::Result<()> {
        let outage = self.end_planned_daemon_outage().await;
        let release = self.release().await;
        match (outage, release) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
            (Err(error), Err(release_error)) => Err(error.context(format!(
                "releasing the runner claim barrier also failed: {release_error:#}"
            ))),
        }
    }

    async fn wait_for_runner(&self, runner_id: &str) -> anyhow::Result<()> {
        self.wait_for_work(Some(runner_id)).await
    }

    async fn wait_for_all_work(&self) -> anyhow::Result<()> {
        self.wait_for_work(None).await
    }

    async fn wait_for_work(&self, runner_id: Option<&str>) -> anyhow::Result<()> {
        let mut announced = false;
        let started = Instant::now();
        let mut next_progress = Duration::ZERO;
        loop {
            self.ensure_healthy().await?;
            let response = call_operator(
                &self.daemon_url,
                self.state_file.as_deref(),
                OperatorRequest::RunnerBarrierWait {
                    lease_token: self.token.clone(),
                    runner_id: runner_id.map(str::to_string),
                },
            )
            .await?;
            let active = match response {
                OperatorResponse::BarrierWait { active, .. } => active,
                _ => anyhow::bail!("oored returned an unexpected runner barrier response"),
            };
            if !active {
                break;
            }
            if started.elapsed() >= RUNNER_DRAIN_TIMEOUT {
                anyhow::bail!(
                    "timed out after {} minutes waiting for assigned or running builds to finish; the update was not started",
                    RUNNER_DRAIN_TIMEOUT.as_secs() / 60
                );
            }
            if !announced {
                println!("Pausing new claims while running work finishes...");
                announced = true;
            }
            if started.elapsed() >= next_progress {
                println!(
                    "Still waiting for assigned or running work ({} minutes elapsed)...",
                    started.elapsed().as_secs() / 60
                );
                next_progress = started.elapsed() + Duration::from_secs(30);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        Ok(())
    }

    async fn release(&self) -> anyhow::Result<()> {
        if self.released.load(std::sync::atomic::Ordering::Acquire) {
            return Ok(());
        }
        anyhow::ensure!(
            self.renewal_pause.borrow().is_none(),
            "cannot release the runner claim barrier while the daemon outage is active"
        );
        self.ensure_healthy().await?;
        let response = call_operator(
            &self.daemon_url,
            self.state_file.as_deref(),
            OperatorRequest::RunnerBarrierRelease {
                lease_token: self.token.clone(),
            },
        )
        .await?;
        anyhow::ensure!(
            matches!(response, OperatorResponse::Barrier { .. }),
            "oored returned an unexpected runner barrier response"
        );
        self.released
            .store(true, std::sync::atomic::Ordering::Release);
        self.renewal.abort();
        Ok(())
    }
}

impl Drop for RunnerClaimBarrier {
    fn drop(&mut self) {
        self.renewal.abort();
    }
}

fn config_key_supported(key: &str) -> bool {
    matches!(key, CONFIG_KEY_DAEMON_URL | CONFIG_KEY_SESSION_TOKEN)
}

fn handle_config_set(args: ConfigSetArgs) -> anyhow::Result<()> {
    let mut cfg = load_cli_config()?;
    match args.key.as_str() {
        CONFIG_KEY_DAEMON_URL => {
            let value = args.value.trim();
            if value.is_empty() {
                anyhow::bail!("{CONFIG_KEY_DAEMON_URL} cannot be empty");
            }
            let keeps_session = cfg
                .daemon_url
                .as_deref()
                .and_then(|current| setup_journey::same_http_endpoint(current, value).ok())
                .unwrap_or(false);
            if !keeps_session {
                cfg.session_token = None;
            }
            cfg.daemon_url = Some(value.to_string());
        }
        CONFIG_KEY_SESSION_TOKEN => {
            let value = args.value.trim();
            if value.is_empty() {
                anyhow::bail!("{CONFIG_KEY_SESSION_TOKEN} cannot be empty");
            }
            cfg.session_token = Some(value.to_string());
        }
        _ => anyhow::bail!("unsupported config key"),
    }

    let path = save_cli_config(&cfg)?;
    println!("Saved {} to {}", args.key, path.display());
    Ok(())
}

fn handle_config_get(args: ConfigGetArgs) -> anyhow::Result<()> {
    let cfg = load_cli_config()?;
    match args.key.as_str() {
        CONFIG_KEY_DAEMON_URL => match cfg.daemon_url {
            Some(value) if !value.trim().is_empty() => {
                println!("{}", value);
                Ok(())
            }
            _ => anyhow::bail!("{CONFIG_KEY_DAEMON_URL} is not set"),
        },
        CONFIG_KEY_SESSION_TOKEN => match cfg.session_token {
            Some(value) if !value.trim().is_empty() => {
                println!("{}", value);
                Ok(())
            }
            _ => anyhow::bail!("{CONFIG_KEY_SESSION_TOKEN} is not set"),
        },
        _ => anyhow::bail!("unsupported config key"),
    }
}

fn command_output(cmd: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Option<std::process::Output> {
    std::process::Command::new(cmd).args(args).output().ok()
}

fn command_version(cmd: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Option<String> {
    command_output(cmd, args)
        .and_then(|out| {
            if out.status.success() {
                let output = if out.stdout.is_empty() {
                    out.stderr
                } else {
                    out.stdout
                };
                String::from_utf8(output).ok()
            } else {
                None
            }
        })
        .and_then(|s| s.lines().next().map(|line| line.trim().to_string()))
}

fn doctor_check(
    name: &str,
    status: &str,
    detail: Option<String>,
    install_hint: Option<&str>,
) -> DoctorCheckResult {
    DoctorCheckResult {
        name: name.to_string(),
        status: status.to_string(),
        detail,
        install_hint: install_hint.map(str::to_string),
    }
}

fn doctor_has_platform(args: &DoctorArgs, platform: DoctorPlatform) -> bool {
    args.all || args.platform.contains(&platform)
}

fn managed_runner_service_doctor_result(
    system_plist_present: bool,
    system_service_loaded: bool,
    system_service_running: bool,
    legacy_plist_present: bool,
    legacy_service_loaded: bool,
    readiness_error: Option<&str>,
) -> DoctorCheckResult {
    let repair_hint = Some(
        "run `oore runner install-service --managed-local` on the backend Mac, or `oore runner install-service` for a registered external runner",
    );

    if system_plist_present && system_service_loaded {
        if !system_service_running {
            return doctor_check(
                "runner_service",
                "warning",
                Some(
                    "boot-time system service is loaded but not running; it may be crash-looping (check ~/.oore/logs/oore-runner.log)"
                        .to_string(),
                ),
                repair_hint,
            );
        }
        if let Some(error) = readiness_error {
            return doctor_check(
                "runner_service",
                "warning",
                Some(format!(
                    "boot-time system service is running but has not authenticated with the backend recently: {error}"
                )),
                repair_hint,
            );
        }
        if legacy_plist_present || legacy_service_loaded {
            return doctor_check(
                "runner_service",
                "warning",
                Some(
                    "boot-time service is loaded, but a legacy login-session service remains"
                        .to_string(),
                ),
                repair_hint,
            );
        }
        return doctor_check(
            "runner_service",
            "ok",
            Some("boot-time system service is installed, running, and authenticated".to_string()),
            None,
        );
    }

    if system_plist_present {
        return doctor_check(
            "runner_service",
            "warning",
            Some("boot-time system service is installed but not loaded".to_string()),
            repair_hint,
        );
    }
    if system_service_loaded {
        let detail = if system_service_running {
            "system service is running without an installed plist and will not survive reboot"
        } else {
            "system service is loaded but not running and has no installed plist"
        };
        return doctor_check(
            "runner_service",
            "warning",
            Some(detail.to_string()),
            repair_hint,
        );
    }
    if legacy_plist_present || legacy_service_loaded {
        return doctor_check(
            "runner_service",
            "warning",
            Some(
                "legacy login-session runner detected; it cannot start before GUI login"
                    .to_string(),
            ),
            repair_hint,
        );
    }

    doctor_check(
        "runner_service",
        "warning",
        Some("managed runner service is not installed".to_string()),
        repair_hint,
    )
}

fn add_managed_runner_service_check(checks: &mut Vec<DoctorCheckResult>) {
    if std::env::consts::OS != "macos" {
        checks.push(doctor_check(
            "runner_service",
            "skipped",
            Some("managed runner services are supported on macOS".to_string()),
            None,
        ));
        return;
    }

    let system_plist = system_runner_plist();
    let system_service = format!("system/{RUNNER_SERVICE_LABEL}");
    let system_service_output =
        command_output("/bin/launchctl", &["print", system_service.as_str()]);
    let system_service_loaded = system_service_output
        .as_ref()
        .is_some_and(|output| output.status.success());
    let system_service_running = system_service_output
        .as_ref()
        .is_some_and(|output| output.status.success() && launchd_job_is_running(&output.stdout));
    let legacy_plist = launch_agent_plist(RUNNER_SERVICE_LABEL).ok();
    let legacy_service_loaded = current_user_launchd_domain()
        .ok()
        .and_then(|(_, service)| command_output("/bin/launchctl", &["print", service.as_str()]))
        .is_some_and(|output| output.status.success());
    let readiness_error = if system_plist.is_file() && system_service_running {
        match managed_runner_service_spec(&system_plist) {
            Ok(Some(spec)) => launchd_job_pid(
                &system_service_output
                    .as_ref()
                    .expect("running launchd service has output")
                    .stdout,
            )
            .context("managed runner has no process id")
            .and_then(|pid| verify_managed_runner_service(&spec, pid, None))
            .err()
            .map(|error| error.to_string()),
            Ok(None) => Some(
                "service needs reinstalling to enable authenticated readiness checks".to_string(),
            ),
            Err(error) => Some(error.to_string()),
        }
    } else {
        None
    };

    checks.push(managed_runner_service_doctor_result(
        system_plist.is_file(),
        system_service_loaded,
        system_service_running,
        legacy_plist.as_deref().is_some_and(Path::is_file),
        legacy_service_loaded,
        readiness_error.as_deref(),
    ));
}

fn android_sdk_root() -> Option<PathBuf> {
    let mut roots = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(PathBuf::from))
        .collect::<Vec<_>>();
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Library/Android/sdk"));
    }
    roots
        .into_iter()
        .find(|path| path.join("platform-tools/adb").is_file())
}

fn valid_android_signing_java_home(path: &Path) -> bool {
    path.join("bin/java").is_file() && path.join("bin/jarsigner").is_file()
}

fn android_signing_java_home() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("JAVA_HOME").map(PathBuf::from)
        && valid_android_signing_java_home(&path)
    {
        return Some(path);
    }

    #[cfg(target_os = "macos")]
    {
        let mut application_roots = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            application_roots.push(PathBuf::from(home).join("Applications"));
        }
        application_roots.push(PathBuf::from("/Applications"));
        for root in application_roots {
            for app in ["Android Studio.app", "Android Studio Preview.app"] {
                let path = root.join(app).join("Contents/jbr/Contents/Home");
                if valid_android_signing_java_home(&path) {
                    return Some(path);
                }
            }
        }

        if let Ok(output) = std::process::Command::new("/usr/libexec/java_home").output()
            && output.status.success()
        {
            let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
            if valid_android_signing_java_home(&path) {
                return Some(path);
            }
        }
    }

    None
}

fn add_android_checks(checks: &mut Vec<DoctorCheckResult>) {
    let java = android_signing_java_home()
        .map(|home| home.join("bin/java"))
        .unwrap_or_else(|| PathBuf::from("java"));
    if let Some(version) = command_version(&java, &["-version"]) {
        let detail = if java.is_absolute() {
            format!("{version} ({})", java.display())
        } else {
            version
        };
        checks.push(doctor_check("java", "ok", Some(detail), None));
    } else {
        checks.push(doctor_check(
            "java",
            "missing",
            None,
            Some("install Android Studio or a supported JDK and set JAVA_HOME"),
        ));
    }

    if let Some(sdk_root) = android_sdk_root() {
        checks.push(doctor_check(
            "android_sdk",
            "ok",
            Some(sdk_root.display().to_string()),
            None,
        ));
    } else {
        checks.push(doctor_check(
            "android_sdk",
            "missing",
            None,
            Some("install Android Studio or set ANDROID_HOME to an SDK with Platform-Tools"),
        ));
    }
}

fn add_xcode_checks(checks: &mut Vec<DoctorCheckResult>) {
    if std::env::consts::OS != "macos" {
        checks.push(doctor_check(
            "xcode",
            "missing",
            Some("iOS and macOS builds require a macOS runner".to_string()),
            Some("run this job on macOS with Xcode installed"),
        ));
        return;
    }

    let developer_dir =
        command_version("xcode-select", &["-p"]).filter(|path| Path::new(path).is_dir());
    let xcode_version = command_version("xcodebuild", &["-version"]);
    if let (Some(developer_dir), Some(xcode_version)) = (developer_dir, xcode_version) {
        checks.push(doctor_check(
            "xcode",
            "ok",
            Some(format!("{} ({})", xcode_version, developer_dir)),
            None,
        ));
    } else {
        checks.push(doctor_check(
            "xcode",
            "missing",
            None,
            Some("install Xcode, then run: sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer"),
        ));
    }
}

fn add_signing_warnings(checks: &mut Vec<DoctorCheckResult>) {
    let missing_tools = ["/usr/bin/security", "/usr/bin/codesign"]
        .into_iter()
        .filter(|tool| !Path::new(tool).is_file())
        .collect::<Vec<_>>();
    if missing_tools.is_empty() {
        checks.push(doctor_check(
            "apple_signing_tools",
            "ok",
            Some(
                "job-scoped signing tools are available; build credentials are validated from each temporary keychain"
                    .to_string(),
            ),
            None,
        ));
    } else {
        checks.push(doctor_check(
            "apple_signing_tools",
            "missing",
            Some(format!("missing {}", missing_tools.join(", "))),
            Some("install Xcode and its command-line tools before signing Apple builds"),
        ));
    }

    if let Some(version) = command_version("xcrun", &["notarytool", "--version"]) {
        checks.push(doctor_check("notarytool", "ok", Some(version), None));
    } else {
        checks.push(doctor_check(
            "notarytool",
            "warning",
            None,
            Some("install a current Xcode version before notarizing macOS apps"),
        ));
    }
}

fn run_update_doctor(json: bool) -> anyhow::Result<()> {
    use oore_contract::RuntimeUpdateCheckStatus;

    let install_root = resolve_install_root()?;
    let preflight = oore_contract::runtime_update_preflight::run(&install_root);
    if json {
        println!("{}", serde_json::to_string_pretty(&preflight)?);
    } else {
        println!(
            "oore doctor -- backend update preflight for {}",
            install_root.display()
        );
        for check in &preflight.checks {
            let status = match check.status {
                RuntimeUpdateCheckStatus::Pass => "ok",
                RuntimeUpdateCheckStatus::Warn => "warning",
                RuntimeUpdateCheckStatus::Fail => "failed",
            };
            println!(
                "  [{status}] {:<30} {}",
                check.label,
                check.detail.as_deref().unwrap_or("")
            );
            if let Some(fix) = check.fix.as_deref() {
                println!("      Fix: {fix}");
            }
        }
        if preflight.ready {
            println!("One-click backend updates are available.");
        }
    }
    match oore_contract::runtime_update_preflight::first_failure(&preflight) {
        Some(check) => anyhow::bail!("backend update preflight failed: {}", check.label),
        None => Ok(()),
    }
}

fn run_doctor_checks(args: DoctorArgs) -> anyhow::Result<()> {
    if args.update {
        return run_update_doctor(args.json);
    }
    let mut checks: Vec<DoctorCheckResult> = Vec::new();

    let base_checks: [(&str, &[&str], &str); 1] = [(
        "git",
        &["--version"],
        "install the Xcode Command Line Tools",
    )];

    for (name, command_args, install_hint) in base_checks {
        if let Some(version) = command_version(name, command_args) {
            checks.push(doctor_check(name, "ok", Some(version), None));
        } else {
            checks.push(doctor_check(name, "missing", None, Some(install_hint)));
        }
    }

    if let Some(version) = command_version("flutter", &["--version"]) {
        checks.push(doctor_check("flutter", "ok", Some(version), None));
    } else {
        checks.push(doctor_check(
            "flutter",
            "ok",
            Some("not installed; host builds need it, VM builds use the image's".to_string()),
            None,
        ));
    }

    if let Some(version) = command_version("tart", &["--version"]) {
        checks.push(doctor_check("tart", "ok", Some(version), None));
    } else {
        checks.push(doctor_check(
            "tart",
            "ok",
            Some("not installed; only pipelines with isolation: vm need it".to_string()),
            None,
        ));
    }

    add_managed_runner_service_check(&mut checks);

    if doctor_has_platform(&args, DoctorPlatform::Android) {
        add_android_checks(&mut checks);
    } else {
        checks.push(doctor_check(
            "android",
            "skipped",
            Some("run `oore doctor --platform android` for Android checks".to_string()),
            None,
        ));
    }

    let apple_selected = doctor_has_platform(&args, DoctorPlatform::Ios)
        || doctor_has_platform(&args, DoctorPlatform::Macos);
    if apple_selected {
        add_xcode_checks(&mut checks);
        add_signing_warnings(&mut checks);
    } else {
        checks.push(doctor_check(
            "apple_platforms",
            "skipped",
            Some(
                "run `oore doctor --platform ios` or `--platform macos` for Xcode checks"
                    .to_string(),
            ),
            None,
        ));
    }

    let missing_count = checks
        .iter()
        .filter(|check| check.status == "missing")
        .count();
    let warning_count = checks
        .iter()
        .filter(|check| check.status == "warning")
        .count();
    let report = DoctorReport {
        checks,
        missing_count,
        warning_count,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("oore doctor -- environment checks");
        for check in &report.checks {
            let detail = check
                .detail
                .as_deref()
                .or(check.install_hint.as_deref())
                .unwrap_or("");
            println!("  [{}] {:<18} {}", check.status, check.name, detail);
            if check.detail.is_some()
                && let Some(install_hint) = check.install_hint.as_deref()
            {
                println!("      Fix: {install_hint}");
            }
        }
        if report.missing_count == 0 {
            println!("All selected required checks passed.");
        } else {
            println!("{} required issue(s) found.", report.missing_count);
        }
        if report.warning_count > 0 {
            println!("{} optional warning(s) found.", report.warning_count);
        }
    }

    if report.missing_count == 0 {
        Ok(())
    } else {
        let missing_names: Vec<String> = report
            .checks
            .iter()
            .filter(|check| check.status == "missing")
            .map(|check| check.name.clone())
            .collect();
        anyhow::bail!("missing required tools: {}", missing_names.join(", "))
    }
}

// ── Self-update helpers ──────────────────────────────────────────

const DEFAULT_GITHUB_REPO: &str = "oore-ci/oore.build";
const LEGACY_GITHUB_REPO: &str = "devaryakjha/oore.build";
const DEFAULT_RELEASE_INDEX_BASE_URL: &str = "https://releases.oore.build";
const RELEASE_SIGNING_PUBLIC_KEY: &str = include_str!("../../../tools/release-signing-key.pub");
const RELEASE_SIGNER_IDENTITY: &str = "release@oore.build";
const RELEASE_INDEX_NAMESPACE: &str = "oore-release-index@oore.build";
const RELEASE_MANIFEST_NAMESPACE: &str = "oore-release-manifest@oore.build";
const MAX_RELEASE_METADATA_BYTES: usize = 1024 * 1024;
const MAX_RELEASE_SIGNATURE_BYTES: usize = 64 * 1024;
const MAX_RELEASE_ARCHIVE_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseChannel {
    Stable,
    Beta,
    Alpha,
}

impl ReleaseChannel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Alpha => "alpha",
        }
    }

    fn parse(raw: &str) -> anyhow::Result<Self> {
        match raw.trim().to_lowercase().as_str() {
            "stable" | "prod" | "production" => Ok(Self::Stable),
            "beta" => Ok(Self::Beta),
            "alpha" => Ok(Self::Alpha),
            other => anyhow::bail!("invalid channel '{other}', expected: stable|beta|alpha"),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct ReleaseManifest {
    schema_version: u32,
    channel: String,
    version: String,
    tag: String,
    download_base_url: String,
}

fn read_trimmed_file(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn parse_semver_loose(raw: &str) -> anyhow::Result<semver::Version> {
    let trimmed = raw.trim();
    let trimmed = trimmed.strip_prefix('v').unwrap_or(trimmed);
    semver::Version::parse(trimmed).with_context(|| format!("invalid semver version: {raw}"))
}

fn infer_channel_from_version(v: &semver::Version) -> ReleaseChannel {
    let pre = v.pre.as_str();
    if pre.starts_with("alpha") {
        ReleaseChannel::Alpha
    } else if pre.starts_with("beta") {
        ReleaseChannel::Beta
    } else {
        ReleaseChannel::Stable
    }
}

fn read_installed_channel(install_root: &Path) -> Option<ReleaseChannel> {
    let raw = read_trimmed_file(&install_root.join("CHANNEL"))?;
    ReleaseChannel::parse(&raw).ok()
}

fn read_installed_repo(install_root: &Path) -> Option<String> {
    read_trimmed_file(&install_root.join("GITHUB_REPO")).map(|repo| normalize_github_repo(&repo))
}

fn normalize_github_repo(repo: &str) -> String {
    let repo = repo.trim();
    if repo == LEGACY_GITHUB_REPO {
        DEFAULT_GITHUB_REPO.to_string()
    } else {
        repo.to_string()
    }
}

fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(format!("oore/{}/update", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to build HTTP client")
}

fn normalized_release_signing_public_key() -> anyhow::Result<String> {
    RELEASE_SIGNING_PUBLIC_KEY
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            match (fields.next(), fields.next()) {
                (Some("ssh-ed25519"), Some(key)) => Some(format!("ssh-ed25519 {key}")),
                _ => None,
            }
        })
        .context("this Oore binary has no configured Ed25519 release signing key")
}

fn verify_signed_release_metadata(
    payload: &[u8],
    signature: &[u8],
    namespace: &str,
    description: &str,
) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let public_key = normalized_release_signing_public_key()?;
    let temporary =
        tempfile::tempdir().context("failed to prepare release signature verification")?;
    let allowed_signers = temporary.path().join("allowed-signers");
    let signature_path = temporary.path().join("metadata.sig");
    fs::write(
        &allowed_signers,
        format!("{RELEASE_SIGNER_IDENTITY} namespaces=\"{namespace}\" {public_key}\n"),
    )
    .context("failed to prepare the release signer policy")?;
    fs::set_permissions(&allowed_signers, fs::Permissions::from_mode(0o600))?;
    fs::write(&signature_path, signature).context("failed to stage the release signature")?;

    let mut child = std::process::Command::new("/usr/bin/ssh-keygen")
        .args([
            "-Y",
            "verify",
            "-f",
            allowed_signers
                .to_str()
                .context("release signer policy path is not valid UTF-8")?,
            "-I",
            RELEASE_SIGNER_IDENTITY,
            "-n",
            namespace,
            "-s",
            signature_path
                .to_str()
                .context("release signature path is not valid UTF-8")?,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to start OpenSSH release signature verification")?;
    child
        .stdin
        .take()
        .context("failed to open release signature verification input")?
        .write_all(payload)
        .context("failed to provide release metadata for signature verification")?;
    let output = child
        .wait_with_output()
        .context("failed to finish release signature verification")?;
    anyhow::ensure!(
        output.status.success(),
        "{description} signature verification failed"
    );
    Ok(())
}

async fn download_release_bytes(
    client: &reqwest::Client,
    url: &str,
    description: &str,
    limit: usize,
) -> anyhow::Result<Vec<u8>> {
    let mut response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to download {description}"))?
        .error_for_status()
        .with_context(|| format!("{description} download failed: {url}"))?;
    if let Some(length) = response.content_length() {
        anyhow::ensure!(
            length <= limit as u64,
            "{description} exceeds the size limit"
        );
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("failed to read {description}"))?
    {
        anyhow::ensure!(
            bytes.len().saturating_add(chunk.len()) <= limit,
            "{description} exceeds the size limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn download_verified_release_metadata(
    client: &reqwest::Client,
    url: &str,
    namespace: &str,
    description: &str,
) -> anyhow::Result<Vec<u8>> {
    let payload =
        download_release_bytes(client, url, description, MAX_RELEASE_METADATA_BYTES).await?;
    let signature = download_release_bytes(
        client,
        &format!("{url}.sig"),
        &format!("{description} signature"),
        MAX_RELEASE_SIGNATURE_BYTES,
    )
    .await?;
    verify_signed_release_metadata(&payload, &signature, namespace, description)?;
    Ok(payload)
}

async fn fetch_latest_release(
    client: &reqwest::Client,
    repo: &str,
    channel: ReleaseChannel,
) -> anyhow::Result<ReleaseManifest> {
    normalized_release_signing_public_key()?;
    let base = std::env::var("OORE_RELEASE_INDEX_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_RELEASE_INDEX_BASE_URL.to_string());
    let url = format!(
        "{}/latest/{}.json",
        base.trim_end_matches('/'),
        channel.as_str()
    );
    let payload = download_verified_release_metadata(
        client,
        &url,
        RELEASE_INDEX_NAMESPACE,
        &format!("{} release index", channel.as_str()),
    )
    .await?;
    let release: ReleaseManifest =
        serde_json::from_slice(&payload).context("failed to parse release index JSON")?;
    if release.schema_version != 1 || release.channel != channel.as_str() {
        anyhow::bail!(
            "invalid {} release index response from {url}",
            channel.as_str()
        );
    }
    if release.tag.trim_start_matches('v') != release.version {
        anyhow::bail!(
            "release index tag and version do not match: {}",
            release.tag
        );
    }
    let expected_download_base = format!(
        "https://github.com/{repo}/releases/download/{}",
        release.tag
    );
    if release.download_base_url.trim_end_matches('/') != expected_download_base {
        anyhow::bail!("release index asset source does not match GitHub repo {repo}");
    }
    Ok(release)
}

fn find_asset_url(release: &ReleaseManifest, name: &str) -> String {
    format!("{}/{name}", release.download_base_url.trim_end_matches('/'))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if !src.is_dir() {
        anyhow::bail!("expected directory: {}", src.display());
    }
    fs::create_dir_all(dst)
        .with_context(|| format!("failed to create directory {}", dst.display()))?;

    for entry in fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))? {
        let entry = entry.context("failed to read directory entry")?;
        let ty = entry.file_type().context("failed to read file type")?;
        let from = entry.path();
        let to = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} -> {}", from.display(), to.display())
            })?;
        }
    }

    Ok(())
}

/// Map `std::env::consts::ARCH` to the archive naming convention.
fn release_arch() -> anyhow::Result<&'static str> {
    match std::env::consts::ARCH {
        "aarch64" => Ok("arm64"),
        "x86_64" => Ok("x86_64"),
        other => anyhow::bail!("unsupported architecture: {other}"),
    }
}

/// Resolve the oore install root (`OORE_INSTALL_ROOT` or `~/.oore`).
fn resolve_install_root() -> anyhow::Result<PathBuf> {
    if let Ok(val) = std::env::var("OORE_INSTALL_ROOT") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    // Prefer deriving install root from the current executable path so `oore update` works even
    // when users install to a non-default root and don't export `OORE_INSTALL_ROOT`.
    if let Ok(exe) = std::env::current_exe()
        && let Some(bin_dir) = exe.parent()
        && bin_dir.file_name() == Some(std::ffi::OsStr::new("bin"))
        && let Some(root) = bin_dir.parent()
    {
        return Ok(root.to_path_buf());
    }

    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".oore"))
}

/// Extract SHA-256 checksum for `filename` from checksums.txt content
/// (format: `<hash>  <filename>` per line, like sha256sum output).
fn parse_checksum(text: &str, filename: &str) -> anyhow::Result<String> {
    for line in text.lines() {
        // Handle both "hash  filename" and "hash filename"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == filename {
            return Ok(parts[0].to_lowercase());
        }
    }
    anyhow::bail!("checksum not found for {filename} in checksums.txt")
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("failed to open {} for hashing", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let bytes_read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to read {} for hashing", path.display()))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn endpoint_url(base_url: &str, path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), path)
}

/// Check whether the daemon is reachable at its configured address.
async fn check_daemon_running(client: &reqwest::Client, daemon_url: &str) -> bool {
    endpoint_is_healthy(client, &endpoint_url(daemon_url, "/healthz")).await
}

fn lsof_name_matches_socket(name: &str, listen: SocketAddr) -> bool {
    let name = name.strip_prefix("TCP ").unwrap_or(name);
    let name = name.strip_suffix(" (LISTEN)").unwrap_or(name);
    if name == listen.to_string() {
        return true;
    }
    listen.ip().is_unspecified() && name == format!("*:{}", listen.port())
}

fn oored_listener_pids(lsof_fields: &str, listen: SocketAddr) -> Vec<i32> {
    let mut pid = None;
    let mut command_is_oored = false;
    let mut matches = Vec::new();

    for field in lsof_fields.lines() {
        match field.as_bytes().first() {
            Some(b'p') => {
                pid = field[1..].parse().ok();
                command_is_oored = false;
            }
            Some(b'c') => command_is_oored = field.get(1..) == Some("oored"),
            Some(b'n') if command_is_oored && lsof_name_matches_socket(&field[1..], listen) => {
                if let Some(pid) = pid
                    && !matches.contains(&pid)
                {
                    matches.push(pid);
                }
            }
            _ => {}
        }
    }

    matches
}

fn process_is_oored(pid: i32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let command = String::from_utf8(output.stdout).ok()?;
            Path::new(command.trim())
                .file_name()
                .map(|name| name == "oored")
        })
        .unwrap_or(false)
}

fn terminate_oored(pid: i32) -> bool {
    if !process_is_oored(pid) {
        return false;
    }
    // SAFETY: Signal zero checks the scalar PID without sending a signal.
    let is_alive = unsafe { libc::kill(pid, 0) == 0 };
    // SAFETY: `kill` receives a scalar PID and the valid SIGTERM constant.
    is_alive && unsafe { libc::kill(pid, libc::SIGTERM) == 0 }
}

/// Stop the daemon using PID file first, then lsof fallback (mirrors uninstall.sh).
fn stop_daemon(install_root: &Path, listen: &str) -> anyhow::Result<()> {
    let pid_file = install_root.join("oored.pid");
    let mut stopped = false;

    // Try PID file first
    if pid_file.exists() {
        if let Ok(contents) = fs::read_to_string(&pid_file)
            && let Ok(pid) = contents.trim().parse::<i32>()
            && terminate_oored(pid)
        {
            stopped = true;
        }
        let _ = fs::remove_file(&pid_file);
    }

    let listen = listen
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid daemon listen address: {listen}"))?;

    // Ask by port, then filter the structured output by exact address and command.
    if let Ok(output) = std::process::Command::new("lsof")
        .args([
            "-nP",
            "-a",
            &format!("-iTCP:{}", listen.port()),
            "-sTCP:LISTEN",
            "-Fpcn",
        ])
        .output()
        && output.status.success()
    {
        for pid in oored_listener_pids(&String::from_utf8_lossy(&output.stdout), listen) {
            if terminate_oored(pid) {
                stopped = true;
            }
        }
    }
    if stopped {
        std::thread::sleep(Duration::from_secs(1));
    }

    Ok(())
}

/// Restart the daemon and verify it becomes healthy.
async fn restart_daemon(
    install_root: &Path,
    listen: &str,
    daemon_url: &str,
    client: &reqwest::Client,
) -> anyhow::Result<()> {
    let bin_dir = install_root.join("bin");
    let oored_bin = bin_dir.join("oored");
    let log_path = install_root.join("logs").join("oored.log");
    let pid_file = install_root.join("oored.pid");

    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log directory: {}", parent.display()))?;
    }

    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open log file: {}", log_path.display()))?;

    let child = std::process::Command::new(&oored_bin)
        .args(["run", "--listen", listen])
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .stdin(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("failed to start oored: {}", oored_bin.display()))?;

    fs::write(&pid_file, child.id().to_string())
        .with_context(|| format!("failed to write PID file: {}", pid_file.display()))?;

    // Poll healthz up to 15 seconds
    for _ in 0..15 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if check_daemon_running(client, daemon_url).await {
            return Ok(());
        }
    }

    anyhow::bail!(
        "Daemon failed to become healthy after restart. Check logs: {}",
        log_path.display()
    )
}

/// Set a file as executable (chmod 755).
fn set_executable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o755);
    fs::set_permissions(path, perms)
        .with_context(|| format!("failed to set permissions on {}", path.display()))
}

const BACKUP_DATABASE_FILE: &str = "oore.db";
const BACKUP_KEY_FILE: &str = "encryption.key";
const BACKUP_MANAGED_RUNNER_FILE: &str = "managed-runner.json";
const BACKUP_MANIFEST_FILE: &str = "manifest.json";
const BACKUP_FORMAT_V1: &str = "oore-backup-v1";
const BACKUP_FORMAT_V2: &str = "oore-backup-v2";
const BACKUP_MANAGED_RUNNER_MAX_BYTES: usize = 64 * 1024;
const BACKUP_RESTORE_JOURNAL_MAX_BYTES: u64 = 64 * 1024;
const BACKUP_RESTORE_JOURNAL_SCHEMA_VERSION: u32 = 1;
const UPDATE_BACKUP_TIMEOUT: Duration = Duration::from_secs(90);

struct ManagedCompleteBackupContext {
    install_root: PathBuf,
    config_path: PathBuf,
    backend_url: String,
}

struct ManagedRunnerBackupCapture {
    _install_lock: Option<install_lock::InstallLock>,
    _registration_lock: RunnerRegistrationLock,
    config_path: PathBuf,
    snapshot: RunnerRegistrationConfigSnapshot,
    config: Option<RunnerConfig>,
}

#[derive(Debug, Default)]
struct BackupRestoreOutcome {
    managed_runner_recovery: Option<ManagedRunnerRecovery>,
}

#[derive(Debug)]
struct ManagedRunnerRecovery {
    quarantined_config: Option<PathBuf>,
}

struct ManagedRunnerRestoreState {
    _install_lock: Option<install_lock::InstallLock>,
    _registration_lock: RunnerRegistrationLock,
    config_path: PathBuf,
    current: RunnerRegistrationConfigSnapshot,
    archive_has_config: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum BackupRestoreJournalPhase {
    Prepared,
    Committed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BackupRestoreJournal {
    schema_version: u32,
    transaction_id: String,
    phase: BackupRestoreJournalPhase,
    stage: PathBuf,
    database: PathBuf,
    key: PathBuf,
    managed_config: Option<PathBuf>,
    archive_has_config: bool,
    database_had_current: bool,
    key_had_current: bool,
    wal_had_current: bool,
    shm_had_current: bool,
    runner_had_current: bool,
}

struct BackupRestoreTransactionPaths {
    journal: PathBuf,
    database_old: PathBuf,
    key_old: PathBuf,
    database_new: PathBuf,
    key_new: PathBuf,
    wal: PathBuf,
    shm: PathBuf,
    wal_old: PathBuf,
    shm_old: PathBuf,
    runner_old: Option<PathBuf>,
    runner_new: Option<PathBuf>,
}

fn managed_complete_backup_context(
    database: &Path,
) -> anyhow::Result<Option<ManagedCompleteBackupContext>> {
    let install_root = resolve_install_root()?;
    let manifest_path = install_root.join("install-manifest.json");
    let manifest = match fs::symlink_metadata(&manifest_path) {
        Ok(_) => install_manifest::InstallManifest::load(&manifest_path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {}", manifest_path.display()));
        }
    };
    if manifest.profile != install_manifest::InstallProfile::Complete {
        return Ok(None);
    }
    if manifest.lifecycle.state != install_manifest::InstallState::Ready {
        return Ok(None);
    }
    let Some(state_file) = manifest.lifecycle.state_file.as_deref() else {
        return Ok(None);
    };
    if !paths_refer_to_same_location(database, Path::new(state_file)) {
        return Ok(None);
    }
    let backend_url = manifest
        .lifecycle
        .backend_url
        .context("the managed Complete installation has no control-plane URL")?;
    setup_journey::same_http_endpoint(&backend_url, &backend_url)
        .context("the managed Complete installation has an invalid control-plane URL")?;

    let runner_service = manifest
        .lifecycle
        .services
        .iter()
        .find(|service| service.service == install_manifest::InstallService::Runner)
        .context("the Ready Complete installation has no managed runner service")?;
    let (service_plist, system_service) = managed_service_plist(RUNNER_SERVICE_LABEL)?;
    anyhow::ensure!(
        paths_refer_to_same_file(&service_plist, Path::new(&runner_service.definition)),
        "the Ready Complete installation records a different runner service definition"
    );
    let installed_executable = fs::canonicalize(install_root.join("bin/oore"))
        .context("failed to resolve the installed Oore executable")?;
    validate_runner_service_definition(
        &service_plist,
        &installed_executable,
        &current_user_name()?,
        system_service,
    )?;
    let service_spec = managed_runner_service_spec(&service_plist)?.context(
        "the managed runner service uses a legacy definition that has no ownership proof",
    )?;
    anyhow::ensure!(
        paths_refer_to_same_file(&service_spec.executable, &installed_executable),
        "the managed runner service does not use this installation's Oore executable"
    );
    let config_path = install_root.join(BACKUP_MANAGED_RUNNER_FILE);
    anyhow::ensure!(
        service_spec.config.is_absolute()
            && paths_refer_to_same_named_location(&service_spec.config, &config_path),
        "the managed runner service uses {}; refusing to back up or replace the custom config",
        service_spec.config.display()
    );
    for service_url in [
        service_spec.daemon_url.as_deref(),
        service_spec.environment_daemon_url.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        oore_runner::require_safe_daemon_url(service_url)
            .context("the managed runner service has an unsafe control-plane URL override")?;
        anyhow::ensure!(
            setup_journey::same_http_endpoint(service_url, &backend_url)?,
            "the managed runner service targets a different control plane"
        );
    }
    Ok(Some(ManagedCompleteBackupContext {
        install_root,
        config_path,
        backend_url,
    }))
}

fn parse_private_runner_snapshot(
    path: &Path,
    snapshot: &RunnerRegistrationConfigSnapshot,
) -> anyhow::Result<Option<RunnerConfig>> {
    let Some(contents) = snapshot.contents.as_deref() else {
        return Ok(None);
    };
    anyhow::ensure!(
        contents.len() <= BACKUP_MANAGED_RUNNER_MAX_BYTES,
        "managed runner config is too large: {}",
        path.display()
    );
    let mode = snapshot
        .mode
        .context("runner config snapshot has no mode")?;
    anyhow::ensure!(
        mode & 0o077 == 0,
        "managed runner config must not grant group or other access: {}",
        path.display()
    );
    let config = serde_json::from_slice::<RunnerConfig>(contents)
        .with_context(|| format!("failed to parse managed runner config {}", path.display()))?;
    anyhow::ensure!(
        !config.runner_id.trim().is_empty()
            && !config.runner_token.trim().is_empty()
            && !config.name.trim().is_empty(),
        "managed runner config has an empty identity, token, or name: {}",
        path.display()
    );
    oore_runner::require_safe_daemon_url(&config.daemon_url).with_context(|| {
        format!(
            "managed runner config has an unsafe URL: {}",
            path.display()
        )
    })?;
    setup_journey::same_http_endpoint(&config.daemon_url, &config.daemon_url).with_context(
        || {
            format!(
                "managed runner config has an invalid URL: {}",
                path.display()
            )
        },
    )?;
    Ok(Some(config))
}

fn validate_managed_runner_registration(
    database: &Path,
    config: &RunnerConfig,
) -> anyhow::Result<()> {
    use std::str::FromStr;

    let source_url = format!("sqlite://{}?mode=ro", database.display());
    let options = SqliteConnectOptions::from_str(&source_url)
        .with_context(|| format!("failed to open SQLite snapshot {}", database.display()))?;
    let runner_id = config.runner_id.clone();
    let expected_token_hash = hex::encode(Sha256::digest(config.runner_token.as_bytes()));
    let runtime = tokio::runtime::Runtime::new().context("failed to create Tokio runtime")?;
    runtime.block_on(async move {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .context("failed to connect to SQLite snapshot")?;
        let token_hash =
            sqlx::query_scalar::<_, String>("SELECT token_hash FROM runners WHERE id = ?1")
                .bind(&runner_id)
                .fetch_optional(&pool)
                .await
                .context("failed to inspect the managed runner registration")?;
        pool.close().await;
        anyhow::ensure!(
            token_hash.as_deref() == Some(expected_token_hash.as_str()),
            "managed runner config does not match the selected database"
        );
        Ok::<(), anyhow::Error>(())
    })
}

fn capture_managed_runner_for_backup(
    database: &Path,
    parent_install_lock_held: bool,
) -> anyhow::Result<Option<ManagedRunnerBackupCapture>> {
    let Some(mut context) = managed_complete_backup_context(database)? else {
        return Ok(None);
    };
    let install_lock = if parent_install_lock_held {
        None
    } else {
        Some(install_lock::InstallLock::acquire(&context.install_root)?)
    };
    if install_lock.is_some() {
        context = managed_complete_backup_context(database)?
            .context("the managed Complete installation changed while the backup was started")?;
    }
    let registration_lock = RunnerRegistrationLock::acquire(&context.config_path)?;
    let snapshot = RunnerRegistrationConfigSnapshot::capture(&context.config_path)?;
    let config = parse_private_runner_snapshot(&context.config_path, &snapshot)?;
    if let Some(config) = config.as_ref() {
        anyhow::ensure!(
            setup_journey::same_http_endpoint(&config.daemon_url, &context.backend_url)?,
            "managed runner config does not target this Complete installation"
        );
    }
    Ok(Some(ManagedRunnerBackupCapture {
        _install_lock: install_lock,
        _registration_lock: registration_lock,
        config_path: context.config_path,
        snapshot,
        config,
    }))
}

fn validate_private_staged_runner_config(path: &Path) -> anyhow::Result<RunnerConfig> {
    let snapshot = RunnerRegistrationConfigSnapshot::capture(path)?;
    parse_private_runner_snapshot(path, &snapshot)?
        .with_context(|| format!("backup is missing {BACKUP_MANAGED_RUNNER_FILE}"))
}

fn write_private_contents(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    let mut file = create_private_file(path)?;
    file.write_all(contents)
        .with_context(|| format!("failed to write private file {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync private file {}", path.display()))?;
    Ok(())
}

fn sync_file_contents(path: &Path) -> anyhow::Result<()> {
    let file = fs::OpenOptions::new()
        .read(true)
        .open(path)
        .with_context(|| format!("failed to open {} for sync", path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync {}", path.display()))
}

fn sync_directory(path: &Path) -> anyhow::Result<()> {
    let directory = fs::File::open(path)
        .with_context(|| format!("failed to open directory {} for sync", path.display()))?;
    directory
        .sync_all()
        .with_context(|| format!("failed to sync directory {}", path.display()))
}

fn sync_parent_directories(paths: &[&Path]) -> anyhow::Result<()> {
    let parents = paths
        .iter()
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect::<BTreeSet<_>>();
    for parent in parents {
        sync_directory(&parent)?;
    }
    Ok(())
}

fn copy_private_durable(source: &Path, destination: &Path) -> anyhow::Result<()> {
    let source_path = source;
    let mut source = fs::File::open(source_path)
        .with_context(|| format!("failed to open private source {}", source_path.display()))?;
    let mut destination_file = create_private_file(destination)?;
    std::io::copy(&mut source, &mut destination_file).with_context(|| {
        format!(
            "failed to copy private file {} to {}",
            source_path.display(),
            destination.display()
        )
    })?;
    destination_file
        .sync_all()
        .with_context(|| format!("failed to sync private file {}", destination.display()))?;
    Ok(())
}

fn backup_restore_journal_path(database: &Path) -> PathBuf {
    sqlite_sidecar_path(database, ".restore-journal.json")
}

fn backup_restore_transaction_paths(
    journal: &BackupRestoreJournal,
) -> anyhow::Result<BackupRestoreTransactionPaths> {
    let transaction_id = uuid::Uuid::parse_str(&journal.transaction_id)
        .context("backup restore journal has an invalid transaction id")?;
    anyhow::ensure!(
        transaction_id.to_string() == journal.transaction_id,
        "backup restore journal has a non-canonical transaction id"
    );
    anyhow::ensure!(
        !journal.archive_has_config || journal.managed_config.is_some(),
        "backup restore journal cannot publish a missing managed config"
    );
    anyhow::ensure!(
        journal.managed_config.is_some() || !journal.runner_had_current,
        "backup restore journal records a runner without a managed config path"
    );
    let stage_name = journal
        .stage
        .file_name()
        .and_then(OsStr::to_str)
        .context("backup restore journal has an invalid staging directory")?;
    anyhow::ensure!(
        journal.stage.parent() == journal.database.parent()
            && (stage_name.starts_with(".restore-stage-")
                || stage_name.starts_with(".update-restore-stage-")),
        "backup restore journal has an unsafe staging directory: {}",
        journal.stage.display()
    );
    let nonce = journal.transaction_id.as_str();
    let wal = sqlite_sidecar_path(&journal.database, "-wal");
    let shm = sqlite_sidecar_path(&journal.database, "-shm");
    let runner_old = journal.managed_config.as_ref().map(|config| {
        let suffix = if journal.archive_has_config {
            format!(".restore-{nonce}.old")
        } else {
            format!(".restore-{nonce}.quarantine")
        };
        sqlite_sidecar_path(config, &suffix)
    });
    let runner_new = journal
        .managed_config
        .as_ref()
        .filter(|_| journal.archive_has_config)
        .map(|config| sqlite_sidecar_path(config, &format!(".restore-{nonce}.new")));
    Ok(BackupRestoreTransactionPaths {
        journal: backup_restore_journal_path(&journal.database),
        database_old: sqlite_sidecar_path(&journal.database, &format!(".restore-{nonce}.old")),
        key_old: sqlite_sidecar_path(&journal.key, &format!(".restore-{nonce}.old")),
        database_new: sqlite_sidecar_path(&journal.database, &format!(".restore-{nonce}.new")),
        key_new: sqlite_sidecar_path(&journal.key, &format!(".restore-{nonce}.new")),
        wal_old: sqlite_sidecar_path(&wal, &format!(".restore-{nonce}.old")),
        shm_old: sqlite_sidecar_path(&shm, &format!(".restore-{nonce}.old")),
        wal,
        shm,
        runner_old,
        runner_new,
    })
}

fn write_restore_journal(path: &Path, journal: &BackupRestoreJournal) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .context("backup restore journal has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create backup restore journal directory {}",
            parent.display()
        )
    })?;
    let temporary = sqlite_sidecar_path(path, &format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> anyhow::Result<()> {
        let mut contents = serde_json::to_vec_pretty(journal)
            .context("failed to serialize backup restore journal")?;
        contents.push(b'\n');
        anyhow::ensure!(
            contents.len() as u64 <= BACKUP_RESTORE_JOURNAL_MAX_BYTES,
            "backup restore journal is too large"
        );
        write_private_contents(&temporary, &contents)?;
        fs::rename(&temporary, path)
            .with_context(|| format!("failed to publish restore journal {}", path.display()))?;
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn read_restore_journal(path: &Path) -> anyhow::Result<Option<BackupRestoreJournal>> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to open restore journal {}", path.display()));
        }
    };
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && metadata.uid() == current_effective_uid()
            && metadata.permissions().mode() & 0o777 == 0o600
            && metadata.len() <= BACKUP_RESTORE_JOURNAL_MAX_BYTES,
        "backup restore journal has unsafe ownership, permissions, type, or size: {}",
        path.display()
    );
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    let journal: BackupRestoreJournal = serde_json::from_slice(&contents)
        .with_context(|| format!("failed to parse restore journal {}", path.display()))?;
    anyhow::ensure!(
        journal.schema_version == BACKUP_RESTORE_JOURNAL_SCHEMA_VERSION,
        "unsupported backup restore journal schema: {}",
        journal.schema_version
    );
    Ok(Some(journal))
}

/// The encryption key path, resolved exactly as `oored` resolves it.
fn resolve_key_path() -> anyhow::Result<PathBuf> {
    Ok(oore_contract::data_paths::encryption_key_path(
        &resolve_data_dir()?,
    ))
}

fn set_private_permissions(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).with_context(|| {
            format!(
                "failed to set restrictive permissions on {}",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn secure_private_directory(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect private directory {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_dir()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == current_effective_uid(),
        "private directory has unsafe ownership or type: {}",
        path.display()
    );
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("failed to secure private directory {}", path.display()))
}

fn create_private_file(path: &Path) -> anyhow::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to create private file {}", path.display()))
}

fn snapshot_sqlite_path(source: &Path, destination: &Path) -> anyhow::Result<()> {
    use std::str::FromStr;

    let source_url = format!("sqlite://{}?mode=rw", source.display());
    let options = SqliteConnectOptions::from_str(&source_url)
        .with_context(|| format!("failed to open SQLite database {}", source.display()))?;
    let destination = destination.display().to_string().replace('\'', "''");
    let runtime = tokio::runtime::Runtime::new().context("failed to create Tokio runtime")?;
    runtime.block_on(async move {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .context("failed to connect to SQLite database")?;
        sqlx::query(sqlx::AssertSqlSafe(format!("VACUUM INTO '{destination}'")))
            .execute(&pool)
            .await
            .context("failed to create a consistent SQLite snapshot")?;
        pool.close().await;
        Ok::<(), anyhow::Error>(())
    })
}

fn sqlite_integrity_check(path: &Path) -> anyhow::Result<()> {
    use std::str::FromStr;

    let source_url = format!("sqlite://{}?mode=ro", path.display());
    let options = SqliteConnectOptions::from_str(&source_url)
        .with_context(|| format!("failed to open SQLite snapshot {}", path.display()))?;
    let runtime = tokio::runtime::Runtime::new().context("failed to create Tokio runtime")?;
    runtime.block_on(async move {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .context("failed to connect to SQLite snapshot")?;
        let result: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await
            .context("failed to run SQLite integrity check")?;
        pool.close().await;
        if result != "ok" {
            anyhow::bail!("SQLite integrity check failed: {result}");
        }
        Ok::<(), anyhow::Error>(())
    })
}

fn unpack_backup(input: &Path, destination: &Path) -> anyhow::Result<BackupManifest> {
    let file = fs::File::open(input)
        .with_context(|| format!("failed to open backup {}", input.display()))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut found = HashMap::new();

    for entry in archive.entries().context("failed to read backup archive")? {
        let mut entry = entry.context("failed to read backup entry")?;
        let path = entry.path().context("invalid backup entry path")?;
        let name = path
            .to_str()
            .context("backup entry path is not UTF-8")?
            .to_string();
        if !matches!(
            name.as_str(),
            BACKUP_DATABASE_FILE
                | BACKUP_KEY_FILE
                | BACKUP_MANAGED_RUNNER_FILE
                | BACKUP_MANIFEST_FILE
        ) || path.components().count() != 1
        {
            anyhow::bail!("backup contains unsupported path: {name}");
        }
        if found.insert(name.clone(), ()).is_some() {
            anyhow::bail!("backup contains duplicate path: {name}");
        }
        if !entry.header().entry_type().is_file() {
            anyhow::bail!("backup entry {name} must be a regular file");
        }
        if name == BACKUP_MANAGED_RUNNER_FILE
            && entry.size() > BACKUP_MANAGED_RUNNER_MAX_BYTES as u64
        {
            anyhow::bail!("backup managed runner config is too large");
        }
        if name == BACKUP_MANAGED_RUNNER_FILE {
            let mode = entry
                .header()
                .mode()
                .context("backup managed runner config has an invalid file mode")?;
            anyhow::ensure!(
                mode & 0o077 == 0,
                "backup managed runner config grants group or other access"
            );
        }
        let output = destination.join(&name);
        if output.exists() {
            anyhow::bail!("backup staging path already exists: {}", output.display());
        }
        entry
            .unpack(&output)
            .with_context(|| format!("failed to unpack backup entry {name}"))?;
    }

    for name in [BACKUP_DATABASE_FILE, BACKUP_KEY_FILE, BACKUP_MANIFEST_FILE] {
        if !found.contains_key(name) {
            anyhow::bail!("backup is missing {name}");
        }
    }

    let manifest = serde_json::from_slice::<BackupManifest>(
        &fs::read(destination.join(BACKUP_MANIFEST_FILE))
            .context("failed to read backup manifest")?,
    )
    .context("failed to parse backup manifest")?;
    anyhow::ensure!(
        matches!(
            manifest.format.as_str(),
            BACKUP_FORMAT_V1 | BACKUP_FORMAT_V2
        ),
        "unsupported backup format: {}",
        manifest.format
    );
    anyhow::ensure!(
        manifest.files.keys().all(|name| matches!(
            name.as_str(),
            BACKUP_DATABASE_FILE | BACKUP_KEY_FILE | BACKUP_MANAGED_RUNNER_FILE
        )),
        "backup manifest contains an unsupported file checksum"
    );
    let archive_has_runner = found.contains_key(BACKUP_MANAGED_RUNNER_FILE);
    anyhow::ensure!(
        archive_has_runner == manifest.files.contains_key(BACKUP_MANAGED_RUNNER_FILE),
        "backup manifest and archive disagree about {BACKUP_MANAGED_RUNNER_FILE}"
    );
    match manifest.format.as_str() {
        BACKUP_FORMAT_V1 => anyhow::ensure!(
            !archive_has_runner,
            "{BACKUP_FORMAT_V1} does not support {BACKUP_MANAGED_RUNNER_FILE}"
        ),
        BACKUP_FORMAT_V2 => anyhow::ensure!(
            archive_has_runner,
            "{BACKUP_FORMAT_V2} requires {BACKUP_MANAGED_RUNNER_FILE}"
        ),
        _ => unreachable!("the backup format was validated"),
    }
    let mut checksummed_files = vec![BACKUP_DATABASE_FILE, BACKUP_KEY_FILE];
    if archive_has_runner {
        checksummed_files.push(BACKUP_MANAGED_RUNNER_FILE);
    }
    for name in checksummed_files {
        let expected = manifest
            .files
            .get(name)
            .with_context(|| format!("backup manifest is missing checksum for {name}"))?;
        let actual = sha256_file(&destination.join(name))?;
        if expected != &actual {
            anyhow::bail!("backup checksum mismatch for {name}");
        }
    }

    let key = fs::read(destination.join(BACKUP_KEY_FILE)).context("failed to read backup key")?;
    if key.len() != 32 {
        anyhow::bail!("backup encryption key has invalid length");
    }
    let database = destination.join(BACKUP_DATABASE_FILE);
    sqlite_integrity_check(&database)?;
    if archive_has_runner {
        let config =
            validate_private_staged_runner_config(&destination.join(BACKUP_MANAGED_RUNNER_FILE))?;
        validate_managed_runner_registration(&database, &config)
            .context("backup managed runner config is inconsistent")?;
    }
    Ok(manifest)
}

fn create_backup_archive_in_stage(
    database: &Path,
    key: &Path,
    output: &Path,
    stage: &Path,
    parent_install_lock_held: bool,
) -> anyhow::Result<()> {
    if let Some(recovered) =
        recover_interrupted_backup_restore(database, key, parent_install_lock_held)?
    {
        println!("Recovered an interrupted backup restore transaction.");
        print_managed_runner_recovery(&recovered);
    }
    if !database.is_file() {
        anyhow::bail!("database does not exist: {}", database.display());
    }
    if !key.is_file() {
        anyhow::bail!("encryption key does not exist: {}", key.display());
    }
    if output.exists() {
        anyhow::bail!("backup destination already exists: {}", output.display());
    }
    let parent = output
        .parent()
        .context("backup output must have a parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create backup directory {}", parent.display()))?;

    let managed_runner = capture_managed_runner_for_backup(database, parent_install_lock_held)?;
    let snapshot = stage.join(BACKUP_DATABASE_FILE);
    snapshot_sqlite_path(database, &snapshot)?;
    let staged_key = stage.join(BACKUP_KEY_FILE);
    fs::copy(key, &staged_key).context("failed to copy encryption key into backup")?;
    set_private_permissions(&staged_key)?;

    let mut archive_has_runner = false;
    if let Some(managed_runner) = managed_runner.as_ref() {
        managed_runner
            .snapshot
            .ensure_unchanged(&managed_runner.config_path)
            .context("managed runner config changed while the backup was created")?;
        if let Some(config) = managed_runner.config.as_ref() {
            validate_managed_runner_registration(&snapshot, config)
                .context("managed runner config is inconsistent with the database snapshot")?;
            let contents = managed_runner
                .snapshot
                .contents
                .as_deref()
                .context("managed runner config snapshot has no contents")?;
            write_private_contents(&stage.join(BACKUP_MANAGED_RUNNER_FILE), contents)?;
            archive_has_runner = true;
        }
    }

    let mut files = HashMap::new();
    let mut checksummed_files = vec![BACKUP_DATABASE_FILE, BACKUP_KEY_FILE];
    if archive_has_runner {
        checksummed_files.push(BACKUP_MANAGED_RUNNER_FILE);
    }
    for name in checksummed_files {
        files.insert(name.to_string(), sha256_file(&stage.join(name))?);
    }
    let manifest = BackupManifest {
        format: if archive_has_runner {
            BACKUP_FORMAT_V2
        } else {
            BACKUP_FORMAT_V1
        }
        .to_string(),
        created_at: now_epoch_secs(),
        files,
    };
    fs::write(
        stage.join(BACKUP_MANIFEST_FILE),
        serde_json::to_vec_pretty(&manifest).context("failed to serialize backup manifest")?,
    )
    .context("failed to write backup manifest")?;

    let temp_output = parent.join(format!(
        ".{}.tmp",
        output.file_name().unwrap_or_default().to_string_lossy()
    ));
    let file = create_private_file(&temp_output)
        .with_context(|| format!("failed to create backup {}", temp_output.display()))?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    let mut archive_files = vec![BACKUP_DATABASE_FILE, BACKUP_KEY_FILE];
    if archive_has_runner {
        archive_files.push(BACKUP_MANAGED_RUNNER_FILE);
    }
    archive_files.push(BACKUP_MANIFEST_FILE);
    for name in archive_files {
        archive
            .append_path_with_name(stage.join(name), name)
            .with_context(|| format!("failed to add {name} to backup"))?;
    }
    archive
        .finish()
        .context("failed to finish backup archive")?;
    let encoder = archive
        .into_inner()
        .context("failed to finalize backup archive")?;
    let archive_file = encoder
        .finish()
        .context("failed to finalize backup compression")?;
    set_private_permissions(&temp_output)?;
    archive_file
        .sync_all()
        .context("failed to sync the completed backup archive")?;
    fs::rename(&temp_output, output)
        .with_context(|| format!("failed to publish backup {}", output.display()))?;
    sync_directory(parent)?;
    Ok(())
}

fn create_backup_archive(database: &Path, key: &Path, output: &Path) -> anyhow::Result<()> {
    let parent = output
        .parent()
        .context("backup output must have a parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create backup directory {}", parent.display()))?;
    let stage =
        tempfile::tempdir_in(parent).context("failed to create backup staging directory")?;
    secure_private_directory(stage.path())?;
    create_backup_archive_in_stage(database, key, output, stage.path(), false)
}

fn backup_create(args: BackupCreateArgs) -> anyhow::Result<()> {
    let database = resolve_db_path(args.state_file.as_deref())?;
    let key = args.key_file.map_or_else(resolve_key_path, Ok)?;
    if let Some(stage) = args.stage_dir.as_deref() {
        create_backup_archive_in_stage(&database, &key, &args.output, stage, true)?;
    } else {
        create_backup_archive(&database, &key, &args.output)?;
    }
    println!("Created backup: {}", args.output.display());
    Ok(())
}

fn backup_temporary_output(output: &Path) -> anyhow::Result<PathBuf> {
    let parent = output
        .parent()
        .context("backup output must have a parent directory")?;
    Ok(parent.join(format!(
        ".{}.tmp",
        output.file_name().unwrap_or_default().to_string_lossy()
    )))
}

async fn create_bounded_update_backup(
    database: &Path,
    key: &Path,
    output: &Path,
) -> anyhow::Result<()> {
    let executable = std::env::current_exe().context("failed to identify the current Oore CLI")?;
    let database = database
        .to_str()
        .context("managed state database path is not valid UTF-8")?;
    let parent = output
        .parent()
        .context("backup output must have a parent directory")?;
    anyhow::ensure!(
        !output.exists(),
        "backup destination already exists: {}",
        output.display()
    );
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create backup directory {}", parent.display()))?;
    let stage = tempfile::Builder::new()
        .prefix(".update-backup-stage-")
        .tempdir_in(parent)
        .context("failed to create the bounded backup staging directory")?;
    secure_private_directory(stage.path())?;
    let temporary_output = backup_temporary_output(output)?;
    if temporary_output.is_file() {
        fs::remove_file(&temporary_output).with_context(|| {
            format!(
                "failed to remove stale backup file {}",
                temporary_output.display()
            )
        })?;
    }

    let mut child = tokio::process::Command::new(executable)
        .args(["--color", "never", "backup", "create", "--output"])
        .arg(output)
        .args(["--state-file", database, "--key-file"])
        .arg(key)
        .arg("--stage-dir")
        .arg(stage.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .context("failed to start the bounded pre-update backup")?;

    let status = match tokio::time::timeout(UPDATE_BACKUP_TIMEOUT, child.wait()).await {
        Ok(status) => status.context("failed to wait for the pre-update backup")?,
        Err(_) => {
            let kill_result = child.kill().await;
            let _ = fs::remove_file(&temporary_output);
            let _ = fs::remove_file(output);
            kill_result.context("the pre-update backup timed out and could not be stopped")?;
            anyhow::bail!(
                "the pre-update backup exceeded its {}-second safety limit",
                UPDATE_BACKUP_TIMEOUT.as_secs()
            );
        }
    };
    if !status.success() {
        let _ = fs::remove_file(&temporary_output);
        let _ = fs::remove_file(output);
        anyhow::bail!("the pre-update backup process failed with {status}");
    }
    anyhow::ensure!(
        output.is_file(),
        "the pre-update backup process did not publish its archive"
    );
    Ok(())
}

async fn restore_bounded_update_backup(
    input: &Path,
    database: &Path,
    key: &Path,
) -> anyhow::Result<()> {
    let executable = std::env::current_exe().context("failed to identify the current Oore CLI")?;
    let database_argument = database
        .to_str()
        .context("managed state database path is not valid UTF-8")?;
    let database_parent = database
        .parent()
        .context("database path must have a parent directory")?;
    fs::create_dir_all(database_parent).with_context(|| {
        format!(
            "failed to create restore directory {}",
            database_parent.display()
        )
    })?;
    let stage = tempfile::Builder::new()
        .prefix(".update-restore-stage-")
        .tempdir_in(database_parent)
        .context("failed to create the bounded restore staging directory")?;
    secure_private_directory(stage.path())?;
    let mut child = tokio::process::Command::new(executable)
        .args(["--color", "never", "backup", "restore", "--input"])
        .arg(input)
        .args(["--state-file", database_argument, "--key-file"])
        .arg(key)
        .arg("--stage-dir")
        .arg(stage.path())
        .arg("--prepare-only")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .context("failed to start the bounded backup restore")?;

    let status = match tokio::time::timeout(UPDATE_BACKUP_TIMEOUT, child.wait()).await {
        Ok(status) => status.context("failed to wait for the backup restore")?,
        Err(_) => {
            child
                .kill()
                .await
                .context("the backup restore timed out and could not be stopped")?;
            anyhow::bail!(
                "the backup restore exceeded its {}-second safety limit",
                UPDATE_BACKUP_TIMEOUT.as_secs()
            );
        }
    };
    anyhow::ensure!(
        status.success(),
        "the backup restore process failed with {status}"
    );
    // The commit opens SQLite through its own Tokio runtime, which panics when
    // started on a runtime worker thread. Run it on a blocking thread instead.
    let stage_path = stage.path().to_path_buf();
    let database = database.to_path_buf();
    let key = key.to_path_buf();
    let outcome = tokio::task::spawn_blocking(move || {
        commit_prepared_backup_restore(&stage_path, &database, &key, false, true)
    })
    .await
    .context("the backup restore commit stopped unexpectedly")??;
    print_managed_runner_recovery(&outcome);
    Ok(())
}

fn backup_verify(args: BackupVerifyArgs) -> anyhow::Result<()> {
    let stage = tempfile::tempdir().context("failed to create backup verification directory")?;
    secure_private_directory(stage.path())?;
    let manifest = unpack_backup(&args.input, stage.path())?;
    println!(
        "Backup verified: {} (created {})",
        args.input.display(),
        manifest.created_at
    );
    Ok(())
}

fn restore_path_is_open(path: &Path) -> anyhow::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect restore path {}", path.display()));
        }
    }
    let output = std::process::Command::new("lsof")
        .args(["-t", "--", &path.display().to_string()])
        .output()
        .context("failed to run lsof before backup restore")?;
    if output.status.success() {
        anyhow::ensure!(
            !output.stdout.is_empty(),
            "lsof reported success without an open restore process"
        );
        return Ok(true);
    }
    if output.status.code() == Some(1) && output.stderr.is_empty() {
        return Ok(false);
    }
    anyhow::bail!(
        "lsof could not prove that the restore path is closed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

fn sqlite_sidecar_path(database: &Path, suffix: &str) -> PathBuf {
    let mut path = database.as_os_str().to_os_string();
    path.push(suffix);
    PathBuf::from(path)
}

fn remove_restore_path_if_present(path: &Path) -> anyhow::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to remove restore path {}", path.display()))
        }
    }
}

fn restore_previous_path(
    label: &str,
    had_current: bool,
    current: &Path,
    old: &Path,
) -> anyhow::Result<()> {
    let old_exists = path_entry_exists(old)?;
    let current_exists = path_entry_exists(current)?;
    if old_exists {
        if current_exists {
            remove_restore_path_if_present(current)?;
        }
        fs::rename(old, current).with_context(|| {
            format!(
                "failed to restore previous {label} {} from {}",
                current.display(),
                old.display()
            )
        })?;
    } else if had_current {
        anyhow::ensure!(
            current_exists,
            "interrupted restore lost both the current and previous {label}: {}",
            current.display()
        );
    } else if current_exists {
        remove_restore_path_if_present(current)?;
    }
    Ok(())
}

fn restore_outcome_from_journal(
    journal: &BackupRestoreJournal,
    paths: &BackupRestoreTransactionPaths,
) -> BackupRestoreOutcome {
    let managed_runner_recovery = journal
        .managed_config
        .as_ref()
        .filter(|_| !journal.archive_has_config)
        .map(|_| ManagedRunnerRecovery {
            quarantined_config: journal
                .runner_had_current
                .then(|| paths.runner_old.clone())
                .flatten(),
        });
    BackupRestoreOutcome {
        managed_runner_recovery,
    }
}

fn sync_restore_transaction_directories(
    journal: &BackupRestoreJournal,
    paths: &BackupRestoreTransactionPaths,
) -> anyhow::Result<()> {
    let mut transaction_paths = vec![
        journal.database.as_path(),
        journal.key.as_path(),
        paths.database_old.as_path(),
        paths.key_old.as_path(),
        paths.database_new.as_path(),
        paths.key_new.as_path(),
        paths.wal.as_path(),
        paths.shm.as_path(),
        paths.wal_old.as_path(),
        paths.shm_old.as_path(),
        paths.journal.as_path(),
    ];
    if let Some(config) = journal.managed_config.as_deref() {
        transaction_paths.push(config);
    }
    if let Some(path) = paths.runner_old.as_deref() {
        transaction_paths.push(path);
    }
    if let Some(path) = paths.runner_new.as_deref() {
        transaction_paths.push(path);
    }
    sync_parent_directories(&transaction_paths)
}

fn remove_restore_journal(
    journal: &BackupRestoreJournal,
    paths: &BackupRestoreTransactionPaths,
) -> anyhow::Result<()> {
    remove_restore_path_if_present(&paths.journal)?;
    sync_restore_transaction_directories(journal, paths)
}

fn cleanup_restore_stage(journal: &BackupRestoreJournal) -> anyhow::Result<()> {
    match fs::symlink_metadata(&journal.stage) {
        Ok(metadata) => {
            use std::os::unix::fs::PermissionsExt;

            anyhow::ensure!(
                metadata.file_type().is_dir()
                    && !metadata.file_type().is_symlink()
                    && metadata.uid() == current_effective_uid()
                    && metadata.permissions().mode() & 0o077 == 0,
                "backup restore staging directory has unsafe ownership, permissions, or type: {}",
                journal.stage.display()
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect restore staging directory {}",
                    journal.stage.display()
                )
            });
        }
    }
    for name in [
        BACKUP_DATABASE_FILE,
        BACKUP_KEY_FILE,
        BACKUP_MANAGED_RUNNER_FILE,
        BACKUP_MANIFEST_FILE,
    ] {
        remove_restore_path_if_present(&journal.stage.join(name))?;
    }
    match fs::remove_dir(&journal.stage) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to remove restore staging directory {}",
                journal.stage.display()
            )
        }),
    }
}

fn rollback_prepared_restore(
    journal: &BackupRestoreJournal,
    paths: &BackupRestoreTransactionPaths,
) -> anyhow::Result<BackupRestoreOutcome> {
    restore_previous_path(
        "database",
        journal.database_had_current,
        &journal.database,
        &paths.database_old,
    )?;
    restore_previous_path(
        "encryption key",
        journal.key_had_current,
        &journal.key,
        &paths.key_old,
    )?;
    restore_previous_path(
        "SQLite WAL",
        journal.wal_had_current,
        &paths.wal,
        &paths.wal_old,
    )?;
    restore_previous_path(
        "SQLite shared-memory file",
        journal.shm_had_current,
        &paths.shm,
        &paths.shm_old,
    )?;
    if let (Some(config), Some(old)) = (
        journal.managed_config.as_deref(),
        paths.runner_old.as_deref(),
    ) {
        restore_previous_path(
            "managed runner config",
            journal.runner_had_current,
            config,
            old,
        )?;
    }
    for path in [
        Some(paths.database_new.as_path()),
        Some(paths.key_new.as_path()),
        paths.runner_new.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        remove_restore_path_if_present(path)?;
    }
    cleanup_restore_stage(journal)?;
    sync_restore_transaction_directories(journal, paths)?;
    remove_restore_journal(journal, paths)?;
    Ok(BackupRestoreOutcome::default())
}

fn finish_committed_restore(
    journal: &BackupRestoreJournal,
    paths: &BackupRestoreTransactionPaths,
) -> anyhow::Result<BackupRestoreOutcome> {
    anyhow::ensure!(
        journal.database.is_file() && journal.key.is_file(),
        "committed backup restore is missing its database or encryption key"
    );
    set_private_permissions(&journal.key)?;
    if journal.archive_has_config {
        let config = journal
            .managed_config
            .as_deref()
            .context("committed backup restore has no managed config path")?;
        anyhow::ensure!(
            config.is_file(),
            "committed backup restore is missing its managed runner config: {}",
            config.display()
        );
        set_private_permissions(config)?;
    } else if let Some(config) = journal.managed_config.as_deref() {
        anyhow::ensure!(
            !path_entry_exists(config)?,
            "committed legacy backup restore unexpectedly left a live managed runner config: {}",
            config.display()
        );
    }
    write_restore_journal(&paths.journal, journal)
        .context("failed to confirm the committed restore journal before cleanup")?;

    let mut cleanup = vec![
        paths.database_old.as_path(),
        paths.key_old.as_path(),
        paths.wal_old.as_path(),
        paths.shm_old.as_path(),
        paths.database_new.as_path(),
        paths.key_new.as_path(),
    ];
    if journal.archive_has_config {
        if let Some(path) = paths.runner_old.as_deref() {
            cleanup.push(path);
        }
        if let Some(path) = paths.runner_new.as_deref() {
            cleanup.push(path);
        }
    }
    for path in cleanup {
        remove_restore_path_if_present(path)?;
    }
    cleanup_restore_stage(journal)?;
    sync_restore_transaction_directories(journal, paths)?;
    let outcome = restore_outcome_from_journal(journal, paths);
    remove_restore_journal(journal, paths)?;
    Ok(outcome)
}

fn recover_restore_journal_locked(
    journal: &BackupRestoreJournal,
) -> anyhow::Result<BackupRestoreOutcome> {
    let paths = backup_restore_transaction_paths(journal)?;
    match journal.phase {
        BackupRestoreJournalPhase::Prepared => rollback_prepared_restore(journal, &paths),
        BackupRestoreJournalPhase::Committed => finish_committed_restore(journal, &paths),
    }
}

fn recover_interrupted_backup_restore(
    database: &Path,
    key: &Path,
    parent_install_lock_held: bool,
) -> anyhow::Result<Option<BackupRestoreOutcome>> {
    let journal_path = backup_restore_journal_path(database);
    let Some(mut journal) = read_restore_journal(&journal_path)? else {
        return Ok(None);
    };
    anyhow::ensure!(
        paths_refer_to_same_location(&journal.database, database)
            && paths_refer_to_same_location(&journal.key, key),
        "the interrupted restore journal does not match the selected database and key"
    );

    let mut context = managed_complete_backup_context(database)?;
    let install_lock = if !parent_install_lock_held {
        context
            .as_ref()
            .map(|context| install_lock::InstallLock::acquire(&context.install_root))
            .transpose()?
    } else {
        None
    };
    if install_lock.is_some() {
        context = managed_complete_backup_context(database)?;
        let current = read_restore_journal(&journal_path)?
            .context("the interrupted restore journal disappeared while its lock was acquired")?;
        anyhow::ensure!(
            current == journal,
            "the interrupted restore journal changed while its lock was acquired"
        );
        journal = current;
    }

    let registration_lock = match journal.managed_config.as_deref() {
        Some(config) => {
            let context = context
                .as_ref()
                .context("the restore journal targets a managed config outside a Ready Complete installation")?;
            anyhow::ensure!(
                paths_refer_to_same_named_location(config, &context.config_path),
                "the restore journal targets an unrelated runner config: {}",
                config.display()
            );
            Some(RunnerRegistrationLock::acquire(config)?)
        }
        None => None,
    };
    let _guards = (install_lock, registration_lock);
    let paths = backup_restore_transaction_paths(&journal)?;
    for path in [
        journal.database.as_path(),
        paths.database_old.as_path(),
        paths.database_new.as_path(),
        paths.wal.as_path(),
        paths.wal_old.as_path(),
        paths.shm.as_path(),
        paths.shm_old.as_path(),
    ] {
        anyhow::ensure!(
            !restore_path_is_open(path)?,
            "an interrupted backup restore needs recovery, but {} is open; stop oored and retry",
            path.display()
        );
    }
    recover_restore_journal_locked(&journal).map(Some)
}

fn prepare_verified_backup_restore(input: &Path, stage: &Path) -> anyhow::Result<()> {
    unpack_backup(input, stage)?;
    Ok(())
}

fn prepare_managed_runner_restore_state(
    stage: &Path,
    database: &Path,
    parent_install_lock_held: bool,
) -> anyhow::Result<Option<ManagedRunnerRestoreState>> {
    let archived_config_path = stage.join(BACKUP_MANAGED_RUNNER_FILE);
    let archive_has_config = match fs::symlink_metadata(&archived_config_path) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.file_type().is_file(),
                "backup managed runner config is not a regular file"
            );
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error).context("failed to inspect backup managed runner config"),
    };
    let Some(mut context) = managed_complete_backup_context(database)? else {
        anyhow::ensure!(
            !archive_has_config,
            "this backup contains a managed Complete runner config, but the restore target is not its managed Complete database"
        );
        return Ok(None);
    };
    let install_lock = if parent_install_lock_held {
        None
    } else {
        Some(install_lock::InstallLock::acquire(&context.install_root)?)
    };
    if install_lock.is_some() {
        context = managed_complete_backup_context(database)?
            .context("the managed Complete installation changed while the restore was started")?;
    }

    let registration_lock = RunnerRegistrationLock::acquire(&context.config_path)?;
    let current = RunnerRegistrationConfigSnapshot::capture(&context.config_path)?;
    let live_database_present = match fs::symlink_metadata(database) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
                "the current managed database is not a regular file: {}",
                database.display()
            );
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect database {}", database.display()));
        }
    };
    if let Some(config) = parse_private_runner_snapshot(&context.config_path, &current)? {
        anyhow::ensure!(
            setup_journey::same_http_endpoint(&config.daemon_url, &context.backend_url)?,
            "refusing to replace a managed runner config that targets another control plane"
        );
        if live_database_present {
            validate_managed_runner_registration(database, &config).context(
                "refusing to replace a managed runner config that does not match the current database",
            )?;
        }
    }
    if archive_has_config {
        let archived = validate_private_staged_runner_config(&archived_config_path)?;
        anyhow::ensure!(
            setup_journey::same_http_endpoint(&archived.daemon_url, &context.backend_url)?,
            "backup managed runner config does not target this Complete installation"
        );
        validate_managed_runner_registration(&stage.join(BACKUP_DATABASE_FILE), &archived)
            .context("backup managed runner config does not match the restored database")?;
    }
    Ok(Some(ManagedRunnerRestoreState {
        _install_lock: install_lock,
        _registration_lock: registration_lock,
        config_path: context.config_path,
        current,
        archive_has_config,
    }))
}

fn print_managed_runner_recovery(outcome: &BackupRestoreOutcome) {
    let Some(recovery) = outcome.managed_runner_recovery.as_ref() else {
        return;
    };
    println!("Managed runner recovery required: this backup has no managed-runner.json.");
    if let Some(path) = recovery.quarantined_config.as_deref() {
        println!(
            "Quarantined the previous managed runner config: {}",
            path.display()
        );
    }
    println!("Run `oore setup` before starting `build.oore.oore-runner`.");
}

fn commit_prepared_backup_restore(
    stage: &Path,
    database: &Path,
    key: &Path,
    verify_database_closed: bool,
    parent_install_lock_held: bool,
) -> anyhow::Result<BackupRestoreOutcome> {
    if let Some(recovered) =
        recover_interrupted_backup_restore(database, key, parent_install_lock_held)?
    {
        println!("Recovered an interrupted backup restore transaction.");
        print_managed_runner_recovery(&recovered);
    }
    let managed_runner =
        prepare_managed_runner_restore_state(stage, database, parent_install_lock_held)?;
    if verify_database_closed && restore_path_is_open(database)? {
        anyhow::bail!(
            "refusing restore while the state database is open; stop the managed or unmanaged oored process first"
        );
    }
    for path in [database, key] {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create restore directory {}", parent.display())
            })?;
        }
    }
    let wal = sqlite_sidecar_path(database, "-wal");
    let shm = sqlite_sidecar_path(database, "-shm");
    let mut journal = BackupRestoreJournal {
        schema_version: BACKUP_RESTORE_JOURNAL_SCHEMA_VERSION,
        transaction_id: uuid::Uuid::new_v4().to_string(),
        phase: BackupRestoreJournalPhase::Prepared,
        stage: stage.to_path_buf(),
        database: database.to_path_buf(),
        key: key.to_path_buf(),
        managed_config: managed_runner
            .as_ref()
            .map(|runner| runner.config_path.clone()),
        archive_has_config: managed_runner
            .as_ref()
            .is_some_and(|runner| runner.archive_has_config),
        database_had_current: path_entry_exists(database)?,
        key_had_current: path_entry_exists(key)?,
        wal_had_current: path_entry_exists(&wal)?,
        shm_had_current: path_entry_exists(&shm)?,
        runner_had_current: managed_runner
            .as_ref()
            .is_some_and(|runner| runner.current.contents.is_some()),
    };
    let paths = backup_restore_transaction_paths(&journal)?;
    write_restore_journal(&paths.journal, &journal)?;
    let preparation = (|| -> anyhow::Result<()> {
        fs::rename(stage.join(BACKUP_DATABASE_FILE), &paths.database_new)
            .context("failed to stage the restored database")?;
        sync_file_contents(&paths.database_new)?;
        copy_private_durable(&stage.join(BACKUP_KEY_FILE), &paths.key_new)?;
        if let Some(runner_new) = paths.runner_new.as_deref() {
            copy_private_durable(&stage.join(BACKUP_MANAGED_RUNNER_FILE), runner_new)?;
        }
        sync_restore_transaction_directories(&journal, &paths)?;
        Ok(())
    })();
    if let Err(error) = preparation {
        let mut cleanup_errors = Vec::new();
        for path in [
            Some(paths.database_new.as_path()),
            Some(paths.key_new.as_path()),
            paths.runner_new.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if let Err(cleanup_error) = remove_restore_path_if_present(path) {
                cleanup_errors.push(format!("{}: {cleanup_error}", path.display()));
            }
        }
        if paths.journal.exists()
            && let Err(cleanup_error) = recover_restore_journal_locked(&journal)
        {
            cleanup_errors.push(format!("journal recovery: {cleanup_error:#}"));
        }
        if cleanup_errors.is_empty() {
            return Err(error);
        }
        return Err(error.context(format!(
            "cleaning prepared restore files also failed: {}",
            cleanup_errors.join("; ")
        )));
    }

    let publish = (|| -> anyhow::Result<()> {
        if let Some(runner) = managed_runner.as_ref() {
            runner
                .current
                .ensure_unchanged(&runner.config_path)
                .context("managed runner config changed before restore")?;
            if runner.current.contents.is_some() {
                fs::rename(
                    &runner.config_path,
                    paths
                        .runner_old
                        .as_deref()
                        .context("managed runner restore has no prior-state path")?,
                )?;
            }
        }
        if journal.database_had_current {
            fs::rename(database, &paths.database_old)?;
        }
        if journal.key_had_current {
            fs::rename(key, &paths.key_old)?;
        }
        if journal.wal_had_current {
            fs::rename(&paths.wal, &paths.wal_old)?;
        }
        if journal.shm_had_current {
            fs::rename(&paths.shm, &paths.shm_old)?;
        }
        fs::rename(&paths.database_new, database)?;
        fs::rename(&paths.key_new, key)?;
        set_private_permissions(key)?;
        if let (Some(runner_new), Some(runner)) =
            (paths.runner_new.as_deref(), managed_runner.as_ref())
        {
            fs::rename(runner_new, &runner.config_path)?;
            set_private_permissions(&runner.config_path)?;
        }
        sync_restore_transaction_directories(&journal, &paths)?;
        journal.phase = BackupRestoreJournalPhase::Committed;
        write_restore_journal(&paths.journal, &journal)?;
        Ok(())
    })();
    if let Err(error) = publish {
        let persisted = read_restore_journal(&paths.journal)
            .context("failed to read the restore journal after publication failed")?;
        let persisted =
            persisted.context("restore publication failed and its journal is missing")?;
        if persisted.phase == BackupRestoreJournalPhase::Committed {
            return finish_committed_restore(&persisted, &paths).with_context(|| {
                format!(
                    "the restore committed after publication reported an error ({error:#}), but durable cleanup failed"
                )
            });
        }
        if let Err(recovery_error) = recover_restore_journal_locked(&persisted) {
            return Err(error.context(format!("restore recovery also failed: {recovery_error:#}")));
        }
        return Err(error);
    }

    finish_committed_restore(&journal, &paths).with_context(|| {
        if journal.archive_has_config {
            "the restore committed, but durable cleanup is incomplete; rerun the backup or restore command to recover automatically".to_string()
        } else {
            let quarantine = paths
                .runner_old
                .as_deref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "no previous config".to_string());
            format!(
                "the restore committed, but durable cleanup is incomplete; managed runner recovery is required and the previous config is at {quarantine}; rerun the command, then run `oore setup`"
            )
        }
    })
}

fn restore_verified_backup(
    input: &Path,
    database: &Path,
    key: &Path,
) -> anyhow::Result<BackupRestoreOutcome> {
    let database_parent = database
        .parent()
        .context("database path must have a parent directory")?;
    fs::create_dir_all(database_parent).with_context(|| {
        format!(
            "failed to create restore directory {}",
            database_parent.display()
        )
    })?;
    let stage = tempfile::Builder::new()
        .prefix(".restore-stage-")
        .tempdir_in(database_parent)
        .context("failed to create restore staging directory")?;
    secure_private_directory(stage.path())?;
    prepare_verified_backup_restore(input, stage.path())?;
    commit_prepared_backup_restore(stage.path(), database, key, true, false)
}

fn backup_restore(args: BackupRestoreArgs) -> anyhow::Result<()> {
    if args.prepare_only {
        let stage = args
            .stage_dir
            .as_deref()
            .context("prepared restore requires a staging directory")?;
        prepare_verified_backup_restore(&args.input, stage)?;
        return Ok(());
    }
    let database = resolve_db_path(args.state_file.as_deref())?;
    let key = args.key_file.map_or_else(resolve_key_path, Ok)?;
    if let Some(recovered) = recover_interrupted_backup_restore(&database, &key, false)? {
        println!("Recovered an interrupted backup restore transaction.");
        print_managed_runner_recovery(&recovered);
    }

    let client = http_client()?;
    let runtime = tokio::runtime::Runtime::new().context("failed to create Tokio runtime")?;
    let daemon_url = resolve_daemon_url(None)?;
    if runtime.block_on(check_daemon_running(&client, &daemon_url)) {
        anyhow::bail!(
            "refusing restore while oored is running; stop the managed service or daemon first"
        );
    }

    let outcome = if let Some(stage) = args.stage_dir.as_deref() {
        prepare_verified_backup_restore(&args.input, stage)?;
        commit_prepared_backup_restore(stage, &database, &key, true, false)?
    } else {
        restore_verified_backup(&args.input, &database, &key)?
    };
    println!("Restored backup: {}", args.input.display());
    print_managed_runner_recovery(&outcome);
    Ok(())
}

const DAEMON_SERVICE_LABEL: &str = "build.oore.oored";
const WEB_SERVICE_LABEL: &str = "build.oore.oore-web";
const RUNNER_SERVICE_LABEL: &str = "build.oore.oore-runner";
const UPDATER_SERVICE_LABEL: &str = "build.oore.oore-updater";
const RUNNER_SERVICE_PATH: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";
const RUNNER_STABLE_RUNNING_POLLS: usize = 4;
const RUNNER_AUTHENTICATED_START_POLLS: usize = 120;
const RUNNER_HANDOFF_POLLS: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedRunnerServiceSpec {
    executable: PathBuf,
    config: PathBuf,
    daemon_url: Option<String>,
    environment_daemon_url: Option<String>,
    acknowledgement: PathBuf,
    service_pid_is_parent: bool,
}

fn managed_runner_update_service_from_program_arguments(
    install_root: &Path,
    program_arguments: &[String],
) -> Option<&'static str> {
    let runner_arguments = runner_command_from_program_arguments(program_arguments);
    let configured = fs::canonicalize(Path::new(runner_arguments.first()?)).ok()?;
    let installed = fs::canonicalize(install_root.join("bin/oore")).ok()?;
    (configured == installed
        && runner_arguments.get(1).map(String::as_str) == Some("runner")
        && runner_arguments.get(2).map(String::as_str) == Some("start"))
    .then_some(RUNNER_SERVICE_LABEL)
}

fn runner_command_from_program_arguments(program_arguments: &[String]) -> &[String] {
    if runner_service_uses_user_bootstrap_wrapper(program_arguments) {
        &program_arguments[8..]
    } else {
        program_arguments
    }
}

fn runner_service_uses_user_bootstrap_wrapper(program_arguments: &[String]) -> bool {
    program_arguments.first().map(String::as_str) == Some("/bin/launchctl")
        && program_arguments.get(1).map(String::as_str) == Some("asuser")
        && program_arguments.get(3).map(String::as_str) == Some("/usr/bin/sudo")
        && program_arguments.get(4).map(String::as_str) == Some("-E")
        && program_arguments.get(5).map(String::as_str) == Some("-H")
        && program_arguments.get(6).map(String::as_str) == Some("-u")
        && program_arguments.len() > 8
}

fn value_from_program_arguments(program_arguments: &[String], option: &str) -> Option<String> {
    program_arguments
        .windows(2)
        .find(|pair| pair[0] == option)
        .map(|pair| pair[1].clone())
        .or_else(|| {
            let prefix = format!("{option}=");
            program_arguments.iter().find_map(|argument| {
                argument
                    .strip_prefix(&prefix)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
        })
}

fn runner_config_from_program_arguments(program_arguments: &[String]) -> Option<PathBuf> {
    value_from_program_arguments(
        runner_command_from_program_arguments(program_arguments),
        "--config",
    )
    .map(PathBuf::from)
}

fn managed_runner_service_spec_from_document(
    document: &serde_json::Value,
) -> anyhow::Result<Option<ManagedRunnerServiceSpec>> {
    let arguments = document
        .get("ProgramArguments")
        .and_then(serde_json::Value::as_array)
        .context("managed runner service has no ProgramArguments")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .context("managed runner ProgramArguments must contain only strings")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let runner_arguments = runner_command_from_program_arguments(&arguments);
    anyhow::ensure!(
        runner_arguments.get(1).map(String::as_str) == Some("runner")
            && runner_arguments.get(2).map(String::as_str) == Some("start"),
        "managed runner service does not launch `oore runner start`"
    );
    let executable = runner_arguments
        .first()
        .map(PathBuf::from)
        .context("managed runner service has no executable")?;
    let config = runner_config_from_program_arguments(runner_arguments)
        .context("managed runner service does not specify --config")?;
    let environment = document
        .get("EnvironmentVariables")
        .and_then(serde_json::Value::as_object);
    let acknowledgement =
        environment.and_then(|values| values.get(oore_runner::RUNNER_SERVICE_ACK_PATH_ENV));
    let Some(acknowledgement) = acknowledgement else {
        return Ok(None);
    };
    let acknowledgement = acknowledgement
        .as_str()
        .map(PathBuf::from)
        .context("managed runner acknowledgement path must be a string")?;
    anyhow::ensure!(
        acknowledgement.is_absolute(),
        "managed runner acknowledgement path must be absolute"
    );
    let environment_daemon_url = environment
        .and_then(|values| values.get("OORE_DAEMON_URL"))
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .context("managed runner OORE_DAEMON_URL must be a string")
        })
        .transpose()?;
    Ok(Some(ManagedRunnerServiceSpec {
        executable,
        config,
        daemon_url: value_from_program_arguments(runner_arguments, "--daemon-url"),
        environment_daemon_url,
        acknowledgement,
        service_pid_is_parent: runner_arguments.len() != arguments.len(),
    }))
}

fn managed_runner_service_spec(plist: &Path) -> anyhow::Result<Option<ManagedRunnerServiceSpec>> {
    managed_runner_service_spec_from_document(&plist_json(plist)?)
}

fn validate_runner_service_definition(
    plist: &Path,
    executable: &Path,
    user: &str,
    system: bool,
) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(plist)
        .with_context(|| format!("failed to inspect runner service {}", plist.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
        "runner service is not a regular file: {}",
        plist.display()
    );
    if system {
        anyhow::ensure!(
            metadata.uid() == 0
                && metadata.gid() == 0
                && metadata.permissions().mode() & 0o777 == 0o644,
            "system runner service has unsafe ownership or permissions"
        );
    } else {
        anyhow::ensure!(
            metadata.uid() == current_effective_uid() && metadata.permissions().mode() & 0o022 == 0,
            "legacy runner service has unsafe ownership or permissions"
        );
    }

    let document = plist_json(plist)?;
    anyhow::ensure!(
        document.get("Label").and_then(serde_json::Value::as_str) == Some(RUNNER_SERVICE_LABEL),
        "runner service has an unexpected label"
    );
    let arguments = document
        .get("ProgramArguments")
        .and_then(serde_json::Value::as_array)
        .context("runner service has no ProgramArguments")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .context("runner service arguments must contain only strings")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let command = runner_command_from_program_arguments(&arguments);
    let configured_executable = command
        .first()
        .map(Path::new)
        .context("runner service has no executable")?;
    anyhow::ensure!(
        paths_refer_to_same_file(configured_executable, executable)
            && command.get(1).map(String::as_str) == Some("runner")
            && command.get(2).map(String::as_str) == Some("start")
            && runner_config_from_program_arguments(command).is_some(),
        "runner service does not match the installed runner command"
    );
    if runner_service_uses_user_bootstrap_wrapper(&arguments) {
        let uid = current_effective_uid().to_string();
        anyhow::ensure!(
            arguments.get(2).map(String::as_str) == Some(uid.as_str())
                && arguments.get(7).map(String::as_str) == Some(user)
                && document.get("UserName").is_none()
                && document.get("Umask").and_then(serde_json::Value::as_u64) == Some(0o77),
            "runner service does not preserve the managed account session"
        );
    } else {
        anyhow::ensure!(
            document.get("UserName").and_then(serde_json::Value::as_str) == Some(user)
                && document.get("Umask").and_then(serde_json::Value::as_u64) == Some(0o77),
            "system runner service does not use the managed account and umask"
        );
    }
    Ok(())
}

fn ensure_runner_acknowledgement_path(install_root: &Path) -> anyhow::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let directory = install_root.join("run");
    match fs::symlink_metadata(&directory) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
                "runner runtime path is not a directory: {}",
                directory.display()
            );
            anyhow::ensure!(
                metadata.uid() == current_effective_uid(),
                "runner runtime directory is not owned by the runner account: {}",
                directory.display()
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&directory).with_context(|| {
                format!(
                    "failed to create runner runtime directory {}",
                    directory.display()
                )
            })?;
        }
        Err(error) => return Err(error.into()),
    }
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).with_context(|| {
        format!(
            "failed to secure runner runtime directory {}",
            directory.display()
        )
    })?;
    Ok(directory.join(oore_runner::RUNNER_SERVICE_ACK_FILE))
}

fn ensure_runner_log_path(directory: &Path, path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    fs::create_dir_all(directory).with_context(|| {
        format!(
            "failed to create runner log directory {}",
            directory.display()
        )
    })?;
    let directory_metadata = fs::symlink_metadata(directory)?;
    anyhow::ensure!(
        directory_metadata.file_type().is_dir()
            && !directory_metadata.file_type().is_symlink()
            && directory_metadata.uid() == current_effective_uid(),
        "runner log directory has an unexpected owner or type"
    );
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("failed to prepare runner log {}", path.display()))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.file_type().is_file() && metadata.uid() == current_effective_uid(),
        "runner log has an unexpected owner or type"
    );
    Ok(())
}

fn clear_runner_service_acknowledgement(path: &Path) -> anyhow::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to clear runner service acknowledgement {}",
                path.display()
            )
        }),
    }
}

fn verify_managed_runner_service(
    spec: &ManagedRunnerServiceSpec,
    pid: u32,
    not_before: Option<i64>,
) -> anyhow::Result<()> {
    let mut config = read_runner_config(&spec.config)?.with_context(|| {
        format!(
            "managed runner config is missing: {}",
            spec.config.display()
        )
    })?;
    if let Some(daemon_url) = &spec.daemon_url {
        config.daemon_url.clone_from(daemon_url);
    }
    let expected_pid = if spec.service_pid_is_parent {
        let ack: oore_runner::RunnerServiceAck =
            serde_json::from_slice(&fs::read(&spec.acknowledgement)?)
                .context("runner service acknowledgement is invalid")?;
        anyhow::ensure!(
            process_is_descendant_of(ack.pid, pid)?,
            "runner acknowledgement process is not owned by the active service"
        );
        ack.pid
    } else {
        pid
    };
    oore_runner::verify_runner_service_ack(
        &spec.acknowledgement,
        &config,
        &spec.executable,
        expected_pid,
        not_before,
        Duration::from_secs(oore_runner::RUNNER_SERVICE_ACK_MAX_AGE_SECS),
    )?;
    Ok(())
}

fn process_is_descendant_of(mut process: u32, ancestor: u32) -> anyhow::Result<bool> {
    for _ in 0..16 {
        if process == ancestor {
            return Ok(true);
        }
        let output = std::process::Command::new("/bin/ps")
            .args(["-o", "ppid=", "-p", &process.to_string()])
            .output()
            .context("failed to inspect runner service process ownership")?;
        if !output.status.success() {
            return Ok(false);
        }
        process = String::from_utf8(output.stdout)?
            .trim()
            .parse()
            .unwrap_or(0);
        if process <= 1 {
            return Ok(false);
        }
    }
    Ok(false)
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn paths_refer_to_same_location(left: &Path, right: &Path) -> bool {
    if paths_refer_to_same_file(left, right) {
        return true;
    }
    paths_refer_to_same_named_location(left, right)
}

fn paths_refer_to_same_named_location(left: &Path, right: &Path) -> bool {
    if left.file_name() != right.file_name() {
        return false;
    }
    if left == right {
        return true;
    }
    match (left.parent(), right.parent()) {
        (Some(left), Some(right)) => match (fs::canonicalize(left), fs::canonicalize(right)) {
            (Ok(left), Ok(right)) => left == right,
            _ => false,
        },
        _ => false,
    }
}

fn managed_runner_update_service(install_root: &Path) -> anyhow::Result<Option<&'static str>> {
    let (plist, _) = managed_service_plist(RUNNER_SERVICE_LABEL)?;
    if !plist.is_file() {
        return Ok(None);
    }
    let program_arguments = plist_program_arguments(&plist)?;
    Ok(managed_runner_update_service_from_program_arguments(
        install_root,
        &program_arguments,
    ))
}

fn managed_runner_service_requires_repair(plist: &Path) -> anyhow::Result<bool> {
    Ok(!runner_service_uses_user_bootstrap_wrapper(
        &plist_program_arguments(plist)?,
    ))
}

fn managed_runner_process_pid() -> anyhow::Result<Option<u32>> {
    let (_, system) = managed_service_plist(RUNNER_SERVICE_LABEL)?;
    let service = if system {
        format!("system/{RUNNER_SERVICE_LABEL}")
    } else {
        current_user_launchd_domain()?.1
    };
    let output = std::process::Command::new("/bin/launchctl")
        .args(["print", &service])
        .output()
        .context("failed to inspect the managed runner before update")?;
    Ok(output
        .status
        .success()
        .then(|| launchd_job_pid(&output.stdout))
        .flatten())
}

fn launchd_xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[allow(clippy::too_many_arguments)]
fn render_runner_launch_daemon(
    executable: &Path,
    config: &Path,
    acknowledgement: &Path,
    home: &Path,
    working_dir: &Path,
    log_path: &Path,
    path: &str,
    user: &str,
    uid: &str,
) -> String {
    let values = [
        "/bin/launchctl".to_string(),
        "asuser".to_string(),
        uid.to_string(),
        "/usr/bin/sudo".to_string(),
        "-E".to_string(),
        "-H".to_string(),
        "-u".to_string(),
        user.to_string(),
        executable.display().to_string(),
        "runner".to_string(),
        "start".to_string(),
        "--config".to_string(),
        config.display().to_string(),
    ];
    let arguments = values
        .iter()
        .map(|value| format!("        <string>{}</string>", launchd_xml_escape(value)))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{RUNNER_SERVICE_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
{arguments}
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>HOME</key>
        <string>{}</string>
        <key>PATH</key>
        <string>{}</string>
        <key>{}</key>
        <string>{}</string>
    </dict>
    <key>WorkingDirectory</key>
    <string>{}</string>
    <key>Umask</key>
    <integer>63</integer>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{}</string>
    <key>StandardErrorPath</key>
    <string>{}</string>
</dict>
</plist>
"#,
        launchd_xml_escape(&home.display().to_string()),
        launchd_xml_escape(path),
        oore_runner::RUNNER_SERVICE_ACK_PATH_ENV,
        launchd_xml_escape(&acknowledgement.display().to_string()),
        launchd_xml_escape(&working_dir.display().to_string()),
        launchd_xml_escape(&log_path.display().to_string()),
        launchd_xml_escape(&log_path.display().to_string()),
    )
}

fn current_user_launchd_domain() -> anyhow::Result<(String, String)> {
    let uid = current_effective_uid();
    Ok((
        format!("gui/{uid}"),
        format!("gui/{uid}/{RUNNER_SERVICE_LABEL}"),
    ))
}

fn current_user_name() -> anyhow::Result<String> {
    if current_effective_uid() == 0 {
        anyhow::bail!(
            "run `oore runner install-service` as the runner account, without sudo; Oore will request administrator access only for launchd setup"
        );
    }
    let mut command = std::process::Command::new("/usr/bin/id");
    command.arg("-un");
    let output = runner_command_output(command, "determining the current user name")?;
    if !output.status.success() {
        anyhow::bail!("failed to determine current user name");
    }
    let user = String::from_utf8(output.stdout)
        .context("current user name was not valid UTF-8")?
        .trim()
        .to_string();
    if user.is_empty() || user == "root" {
        anyhow::bail!("the managed runner must use a non-root account");
    }
    Ok(user)
}

fn runner_command_output(
    mut command: std::process::Command,
    action: &str,
) -> anyhow::Result<std::process::Output> {
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("failed while {action}"))?;
    let deadline = Instant::now() + Duration::from_secs(15);
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

fn sudo_tool_output(tool: &str, args: &[&OsStr]) -> anyhow::Result<std::process::Output> {
    let mut command = std::process::Command::new("/usr/bin/sudo");
    command.arg("-n").arg(tool).args(args);
    runner_command_output(command, &format!("running privileged {tool}"))
}

fn sudo_tool_checked(tool: &str, args: &[&OsStr], action: &str) -> anyhow::Result<()> {
    let output = sudo_tool_output(tool, args)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    anyhow::bail!("{action} failed: {detail}")
}

fn remove_staged_system_runner_plist(path: &Path) {
    let _ = sudo_tool_output("/bin/rm", &[OsStr::new("-f"), path.as_os_str()]);
}

fn system_runner_plist() -> PathBuf {
    PathBuf::from("/Library/LaunchDaemons").join(format!("{RUNNER_SERVICE_LABEL}.plist"))
}

fn write_system_runner_plist(contents: &str) -> anyhow::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let target = system_runner_plist();
    let staged = PathBuf::from("/Library/LaunchDaemons").join(format!(
        ".{RUNNER_SERVICE_LABEL}.{}.tmp",
        uuid::Uuid::new_v4()
    ));

    let result = (|| -> anyhow::Result<()> {
        let mut source = tempfile::NamedTempFile::new()
            .context("failed to stage the runner service definition")?;
        source
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
        source.write_all(contents.as_bytes())?;
        source.as_file_mut().sync_all()?;
        sudo_tool_checked(
            "/usr/bin/install",
            &[
                OsStr::new("-o"),
                OsStr::new("root"),
                OsStr::new("-g"),
                OsStr::new("wheel"),
                OsStr::new("-m"),
                OsStr::new("0644"),
                source.path().as_os_str(),
                staged.as_os_str(),
            ],
            "staging the system runner plist",
        )?;
        sudo_tool_checked(
            "/usr/bin/plutil",
            &[OsStr::new("-lint"), staged.as_os_str()],
            "validating the system runner plist",
        )?;
        sudo_tool_checked(
            "/bin/mv",
            &[OsStr::new("-f"), staged.as_os_str(), target.as_os_str()],
            "installing the system runner plist",
        )
    })();

    if result.is_err() {
        remove_staged_system_runner_plist(&staged);
    }
    result?;
    Ok(target)
}

fn user_launchd_service_loaded(service: &str) -> bool {
    let mut command = std::process::Command::new("/bin/launchctl");
    command.args(["print", service]);
    runner_command_output(command, "inspecting the legacy runner service")
        .is_ok_and(|output| output.status.success())
}

fn stop_user_launchd_service(service: &str) -> anyhow::Result<()> {
    if !user_launchd_service_loaded(service) {
        return Ok(());
    }
    let mut command = std::process::Command::new("/bin/launchctl");
    command.args(["bootout", service]);
    let output = runner_command_output(command, "stopping the legacy runner service")?;
    if !output.status.success() && user_launchd_service_loaded(service) {
        anyhow::bail!("failed to stop legacy runner service {service}");
    }
    if !wait_for_launchd_service_unloaded(|| Ok(user_launchd_service_loaded(service)))? {
        anyhow::bail!("failed to stop legacy runner service {service}");
    }
    Ok(())
}

fn wait_for_launchd_service_unloaded(
    mut is_loaded: impl FnMut() -> anyhow::Result<bool>,
) -> anyhow::Result<bool> {
    for _ in 0..20 {
        if !is_loaded()? {
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(false)
}

fn system_launchd_service_loaded(service: &str) -> anyhow::Result<bool> {
    Ok(sudo_tool_output(
        "/bin/launchctl",
        &[OsStr::new("print"), OsStr::new(service)],
    )?
    .status
    .success())
}

fn system_launchd_service_output(service: &str) -> anyhow::Result<std::process::Output> {
    let mut command = std::process::Command::new("/bin/launchctl");
    command.args(["print", service]);
    runner_command_output(
        command,
        &format!("inspecting system runner service {service}"),
    )
}

fn launchd_job_pid(output: &[u8]) -> Option<u32> {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .find_map(|line| {
            line.strip_prefix("pid = ")
                .and_then(|pid| pid.parse::<u32>().ok())
                .filter(|pid| *pid > 0)
        })
}

fn launchd_job_is_running(output: &[u8]) -> bool {
    let output = String::from_utf8_lossy(output);
    output
        .lines()
        .map(str::trim)
        .any(|line| line == "state = running")
        && launchd_job_pid(output.as_bytes()).is_some()
}

fn launchd_runner_handoff_completed(output: &[u8], previous_pid: Option<u32>) -> bool {
    launchd_job_is_running(output)
        && previous_pid.is_none_or(|pid| launchd_job_pid(output) != Some(pid))
}

fn launchd_job_is_stably_running(consecutive_running: &mut usize, running: bool) -> bool {
    if running {
        *consecutive_running += 1;
    } else {
        *consecutive_running = 0;
    }
    *consecutive_running >= RUNNER_STABLE_RUNNING_POLLS
}

fn stop_system_launchd_service(service: &str) -> anyhow::Result<()> {
    if !system_launchd_service_loaded(service)? {
        return Ok(());
    }
    let _ = sudo_tool_output(
        "/bin/launchctl",
        &[OsStr::new("bootout"), OsStr::new(service)],
    );
    if !wait_for_launchd_service_unloaded(|| system_launchd_service_loaded(service))? {
        anyhow::bail!("failed to stop system runner service {service}");
    }
    Ok(())
}

fn start_system_runner_service(
    plist: &Path,
    service: &str,
    require_authenticated_acknowledgement: bool,
) -> anyhow::Result<()> {
    let spec = managed_runner_service_spec(plist)?;
    anyhow::ensure!(
        !require_authenticated_acknowledgement || spec.is_some(),
        "managed runner service is missing its authenticated readiness configuration"
    );
    if let Some(spec) = &spec {
        clear_runner_service_acknowledgement(&spec.acknowledgement)?;
    }
    let started_at = now_epoch_secs();
    let bootstrap = || {
        sudo_tool_checked(
            "/bin/launchctl",
            &[
                OsStr::new("bootstrap"),
                OsStr::new("system"),
                plist.as_os_str(),
            ],
            "starting the system runner service",
        )
    };
    if bootstrap().is_err() {
        std::thread::sleep(Duration::from_millis(250));
        bootstrap()?;
    }
    sudo_tool_checked(
        "/bin/launchctl",
        &[
            OsStr::new("kickstart"),
            OsStr::new("-k"),
            OsStr::new(service),
        ],
        "starting the managed runner",
    )?;
    let mut consecutive_running = 0;
    let mut readiness_error = None;
    for _ in 0..RUNNER_AUTHENTICATED_START_POLLS {
        let output = sudo_tool_output(
            "/bin/launchctl",
            &[OsStr::new("print"), OsStr::new(service)],
        )?;
        let running = output.status.success() && launchd_job_is_running(&output.stdout);
        if launchd_job_is_stably_running(&mut consecutive_running, running) {
            let Some(spec) = &spec else {
                return Ok(());
            };
            let pid =
                launchd_job_pid(&output.stdout).context("managed runner has no process id")?;
            match verify_managed_runner_service(spec, pid, Some(started_at)) {
                Ok(()) => return Ok(()),
                Err(error) => readiness_error = Some(error.to_string()),
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if let Some(error) = readiness_error {
        anyhow::bail!(
            "managed runner started but did not authenticate with the backend: {error} (check ~/.oore/logs/oore-runner.log)"
        );
    }
    anyhow::bail!("managed runner did not remain running after launchd started it")
}

fn ensure_system_runner_service_active(
    plist: &Path,
    service: &str,
    previous_pid: Option<u32>,
) -> anyhow::Result<()> {
    let spec = managed_runner_service_spec(plist)?;
    if let Some(spec) = &spec {
        clear_runner_service_acknowledgement(&spec.acknowledgement)?;
    }
    let started_at = now_epoch_secs();
    let mut output = system_launchd_service_output(service)?;
    if let Some(previous_pid) = previous_pid {
        let mut waiting = false;
        for _ in 0..RUNNER_HANDOFF_POLLS {
            if !output.status.success() || launchd_job_pid(&output.stdout) != Some(previous_pid) {
                break;
            }
            if !waiting {
                println!(
                    "The runner is finishing its active build; waiting for the updated runner to take over..."
                );
                waiting = true;
            }
            std::thread::sleep(Duration::from_millis(500));
            output = system_launchd_service_output(service)?;
        }
        anyhow::ensure!(
            !output.status.success() || launchd_job_pid(&output.stdout) != Some(previous_pid),
            "timed out waiting for the previous runner process to finish its handoff"
        );
    }

    if !output.status.success() {
        let bootstrap = || {
            sudo_tool_checked(
                "/bin/launchctl",
                &[
                    OsStr::new("bootstrap"),
                    OsStr::new("system"),
                    plist.as_os_str(),
                ],
                "loading the system runner service",
            )
        };
        if bootstrap().is_err() {
            std::thread::sleep(Duration::from_millis(250));
            bootstrap()?;
        }
    }

    if !output.status.success() || !launchd_job_is_running(&output.stdout) {
        sudo_tool_checked(
            "/bin/launchctl",
            &[OsStr::new("kickstart"), OsStr::new(service)],
            "starting the managed runner",
        )?;
    }
    let mut consecutive_running = 0;
    let mut readiness_error = None;
    for _ in 0..RUNNER_AUTHENTICATED_START_POLLS {
        output = system_launchd_service_output(service)?;
        let running = output.status.success()
            && launchd_runner_handoff_completed(&output.stdout, previous_pid);
        if launchd_job_is_stably_running(&mut consecutive_running, running) {
            let Some(spec) = &spec else {
                return Ok(());
            };
            let pid =
                launchd_job_pid(&output.stdout).context("managed runner has no process id")?;
            match verify_managed_runner_service(spec, pid, Some(started_at)) {
                Ok(()) => return Ok(()),
                Err(error) => readiness_error = Some(error.to_string()),
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if let Some(error) = readiness_error {
        anyhow::bail!(
            "managed runner became active but did not authenticate with the backend: {error} (check ~/.oore/logs/oore-runner.log)"
        );
    }
    anyhow::bail!("managed runner did not become active after launchd started it")
}

fn restore_legacy_runner_service(plist: &Path, domain: &str, service: &str) -> anyhow::Result<()> {
    if !plist.is_file() {
        anyhow::bail!("legacy runner plist is missing: {}", plist.display());
    }
    let mut bootstrap = std::process::Command::new("/bin/launchctl");
    bootstrap.args(["bootstrap", domain, &plist.display().to_string()]);
    let output = runner_command_output(bootstrap, "restoring the legacy runner service")?;
    if !output.status.success() {
        anyhow::bail!("failed to restore the legacy runner service");
    }
    let mut kickstart = std::process::Command::new("/bin/launchctl");
    kickstart.args(["kickstart", "-k", service]);
    let output = runner_command_output(kickstart, "restarting the legacy runner service")?;
    if !output.status.success() || !user_launchd_service_loaded(service) {
        anyhow::bail!("failed to verify the restored legacy runner service");
    }
    Ok(())
}

fn restore_runner_config_file(path: &Path, previous: Option<&RunnerConfig>) -> anyhow::Result<()> {
    if let Some(previous) = previous {
        return write_runner_config(path, previous);
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to remove runner config {}", path.display()))
        }
    }
}

async fn rollback_runner_configuration(
    config_path: &Path,
    previous_config: Option<&RunnerConfig>,
    enrollment: Option<(&ManagedRunnerEnrollmentSnapshot, &str)>,
) -> anyhow::Result<()> {
    let config_result = restore_runner_config_file(config_path, previous_config);
    let enrollment_result = match enrollment {
        Some((snapshot, runner_id)) => snapshot.restore(runner_id).await,
        None => Ok(()),
    };
    match (config_result, enrollment_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(enrollment_error)) => Err(error.context(format!(
            "restoring the managed runner enrollment also failed: {enrollment_error:#}"
        ))),
    }
}

async fn rollback_runner_service_install(
    install_root: &Path,
    config_path: &Path,
    previous_config: Option<&RunnerConfig>,
    migrated_config: Option<(&Path, &RunnerConfig)>,
    previous_system: Option<(&str, bool)>,
    previous_legacy: Option<(&Path, &str, &str, &str, bool)>,
    enrollment: Option<(&ManagedRunnerEnrollmentSnapshot, &str)>,
) -> anyhow::Result<()> {
    let mut failures = Vec::new();
    let system_service = format!("system/{RUNNER_SERVICE_LABEL}");
    if let Err(error) = stop_system_launchd_service(&system_service) {
        failures.push(format!("failed to stop replacement runner: {error:#}"));
    }
    let plist = system_runner_plist();
    let restore_system = if let Some((contents, _)) = previous_system {
        write_system_runner_plist(contents).map(|_| ())
    } else {
        sudo_tool_checked(
            "/bin/rm",
            &[OsStr::new("-f"), plist.as_os_str()],
            "removing the failed system runner plist",
        )
    };
    if let Err(error) = restore_system {
        failures.push(format!(
            "failed to restore system runner definition: {error:#}"
        ));
    }
    if let Err(error) = restore_runner_config_file(config_path, previous_config) {
        failures.push(format!("failed to restore runner config: {error:#}"));
    }
    if let Some((path, config)) = migrated_config
        && let Err(error) = write_runner_config(path, config)
    {
        failures.push(format!(
            "failed to restore migrated runner config: {error:#}"
        ));
    }
    if let Err(error) = restore_runner_release_marker(install_root) {
        failures.push(format!(
            "failed to restore runner release marker: {error:#}"
        ));
    }
    let enrollment_restored = if let Some((snapshot, runner_id)) = enrollment {
        match snapshot.restore(runner_id).await {
            Ok(()) => true,
            Err(error) => {
                failures.push(format!(
                    "failed to restore managed runner enrollment: {error:#}"
                ));
                false
            }
        }
    } else {
        true
    };
    if enrollment_restored
        && previous_system.is_some_and(|(_, was_loaded)| was_loaded)
        && let Err(error) = start_system_runner_service(&plist, &system_service, false)
    {
        failures.push(format!(
            "failed to restart previous system runner: {error:#}"
        ));
    }
    if let Some((plist, contents, domain, service, was_loaded)) = previous_legacy {
        let restored = fs::write(plist, contents)
            .with_context(|| format!("failed to restore legacy runner plist {}", plist.display()));
        if let Err(error) = restored {
            failures.push(format!(
                "failed to restore legacy runner definition: {error:#}"
            ));
        } else if enrollment_restored
            && was_loaded
            && let Err(error) = restore_legacy_runner_service(plist, domain, service)
        {
            failures.push(format!("failed to restart legacy runner: {error:#}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(failures.join("; "))
    }
}

async fn handle_runner_install_service(args: RunnerServiceArgs, quiet: bool) -> anyhow::Result<()> {
    handle_runner_install_service_inner(
        args,
        false,
        None,
        ManagedRunnerCredentialAction::Reuse,
        quiet,
    )
    .await
}

fn refuse_profile_runner_service_command(install_root: &Path, command: &str) -> anyhow::Result<()> {
    let manifest_path = install_root.join("install-manifest.json");
    match fs::symlink_metadata(&manifest_path) {
        Ok(_) => {
            install_manifest::InstallManifest::load(&manifest_path)?;
            anyhow::bail!(
                "`oore runner {command}` is not available for a profile-managed installation; run `oore setup` to repair the selected device role or `oore uninstall` to remove it"
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to inspect {}", manifest_path.display()))
        }
    }
}

async fn handle_runner_install_service_direct(args: RunnerServiceArgs) -> anyhow::Result<()> {
    let install_root = resolve_install_root()?;
    let _lifecycle_lock = install_lock::InstallLock::acquire(&install_root)?;
    refuse_profile_runner_service_command(&install_root, "install-service")?;
    handle_runner_install_service(args, false).await
}

async fn handle_runner_rotate_token() -> anyhow::Result<()> {
    if !cfg!(target_os = "macos") {
        anyhow::bail!("managed runner token rotation is supported on macOS only");
    }

    let install_root = resolve_install_root()?;
    let _lifecycle_lock = install_lock::InstallLock::acquire(&install_root)?;
    let manifest =
        install_manifest::InstallManifest::load(&install_root.join("install-manifest.json"))?;
    anyhow::ensure!(
        manifest.profile == install_manifest::InstallProfile::Complete
            && manifest.lifecycle.state == install_manifest::InstallState::Ready,
        "managed runner token rotation requires a Ready Complete installation; run `oore setup` first"
    );
    anyhow::ensure!(
        managed_services::service_is_owned(
            &install_root,
            install_manifest::InstallService::Runner,
        )?,
        "the managed runner service does not belong to this installation; run `oore setup` to repair it"
    );

    let state_file = manifest
        .lifecycle
        .state_file
        .context("the installation manifest has no control-plane database path")?;
    let backend_url = manifest
        .lifecycle
        .backend_url
        .context("the installation manifest has no control-plane URL")?;
    let config_path = managed_local_runner_config_path()?;
    let current = read_runner_config(&config_path)?.with_context(|| {
        format!(
            "managed runner config is missing: {}; run `oore setup` to repair it",
            config_path.display()
        )
    })?;
    anyhow::ensure!(
        setup_journey::same_http_endpoint(&current.daemon_url, &backend_url)?,
        "the managed runner does not target this installation's control plane; run `oore setup` to repair it"
    );

    handle_runner_install_service_inner(
        RunnerServiceArgs {
            config: None,
            managed_local: true,
            daemon_url: Some(current.daemon_url.clone()),
            state_file: Some(state_file),
            name: Some(current.name.clone()),
        },
        false,
        None,
        ManagedRunnerCredentialAction::Rotate {
            expected_runner_id: current.runner_id.clone(),
        },
        true,
    )
    .await?;

    println!("Rotated the managed local runner token.");
    println!("Runner: {} ({})", current.name, current.runner_id);
    Ok(())
}

async fn handle_runner_install_service_inner(
    args: RunnerServiceArgs,
    transition_barrier_held: bool,
    update_ack: Option<(&Path, &str, &str)>,
    credential_action: ManagedRunnerCredentialAction,
    quiet: bool,
) -> anyhow::Result<()> {
    if !cfg!(target_os = "macos") {
        anyhow::bail!("managed runner service installation is supported on macOS only");
    }

    let user = current_user_name()?;
    let working_dir = fs::canonicalize(resolve_install_root()?)
        .context("failed to resolve the Oore install root")?;
    let executable = fs::canonicalize(working_dir.join("bin/oore"))
        .context("failed to resolve the installed oore executable")?;
    if args.managed_local && args.config.is_some() {
        anyhow::bail!("--managed-local cannot be combined with --config");
    }
    let previous_system_plist = system_runner_plist();
    let legacy_plist = launch_agent_plist(RUNNER_SERVICE_LABEL)?;
    let (domain, service) = current_user_launchd_domain()?;
    let legacy_was_loaded = user_launchd_service_loaded(&service);
    let mut installed_service_configs = Vec::new();
    for existing_plist in [&previous_system_plist, &legacy_plist] {
        if !existing_plist.is_file() {
            continue;
        }
        validate_runner_service_definition(
            existing_plist,
            &executable,
            &user,
            existing_plist == &previous_system_plist,
        )?;
        let program_arguments = plist_program_arguments(existing_plist)?;
        let existing_config = runner_config_from_program_arguments(&program_arguments)
            .with_context(|| {
                format!(
                    "existing runner service has no config path: {}",
                    existing_plist.display()
                )
            })?;
        installed_service_configs.push(existing_config);
    }
    if let Some(first) = installed_service_configs.first()
        && installed_service_configs
            .iter()
            .any(|path| !paths_refer_to_same_file(first, path))
    {
        anyhow::bail!(
            "system and login-session runner services use different configs; remove the stale service before reinstalling"
        );
    }

    let existing_managed_config = args
        .managed_local
        .then(|| installed_service_configs.first().cloned())
        .flatten();
    let requested_config = if args.managed_local {
        managed_local_runner_config_path()?
    } else {
        let requested_config = runner_config_path(args.config.clone())?;
        for existing_config in &installed_service_configs {
            if !paths_refer_to_same_file(&requested_config, existing_config) {
                anyhow::bail!(
                    "existing runner service uses {}; uninstall it before switching to {}",
                    existing_config.display(),
                    requested_config.display()
                );
            }
        }
        requested_config
    };
    let _registration_lock = RunnerRegistrationLock::acquire(&requested_config)?;
    if !args.managed_local && !transition_barrier_held {
        let system_service = format!("system/{RUNNER_SERVICE_LABEL}");
        let system_output = system_launchd_service_output(&system_service)?;
        let system_is_loaded = system_output.status.success();
        if system_is_loaded || legacy_was_loaded {
            if system_is_loaded && previous_system_plist.is_file() && !legacy_plist.exists() {
                match managed_services::verify_service(
                    &working_dir,
                    install_manifest::InstallService::Runner,
                )
                .await
                {
                    Ok(()) => return Ok(()),
                    Err(error) => anyhow::bail!(
                        "the active remote runner needs repair, but this release cannot drain it safely; the service was left running: {error:#}"
                    ),
                }
            }
            anyhow::bail!(
                "the active remote runner needs a coordinated drain before its service can change; the service was left running"
            );
        }
    }
    let previous_config_snapshot = RunnerRegistrationConfigSnapshot::capture(&requested_config)?;
    let previous_config =
        parse_private_runner_snapshot(&requested_config, &previous_config_snapshot)?;
    let previous_migrated_config = existing_managed_config
        .as_deref()
        .filter(|path| !paths_refer_to_same_file(path, &requested_config))
        .map(read_runner_config)
        .transpose()?
        .flatten();
    if let Some(existing_config) = existing_managed_config.as_deref() {
        migrate_managed_runner_service_config(
            existing_config,
            &requested_config,
            args.state_file.as_deref(),
        )
        .await?;
    }
    let home = dirs::home_dir().context("could not determine home directory")?;
    let logs = working_dir.join("logs");
    let log_path = logs.join("oore-runner.log");
    let path = RUNNER_SERVICE_PATH;
    ensure_runner_log_path(&logs, &log_path)?;
    let acknowledgement = ensure_runner_acknowledgement_path(&working_dir)?;

    let system_service = format!("system/{RUNNER_SERVICE_LABEL}");
    let previous_system_contents = previous_system_plist
        .is_file()
        .then(|| {
            fs::read_to_string(&previous_system_plist).with_context(|| {
                format!(
                    "failed to preserve existing runner service {}",
                    previous_system_plist.display()
                )
            })
        })
        .transpose()?;
    let previous_legacy_contents = legacy_plist
        .is_file()
        .then(|| {
            fs::read_to_string(&legacy_plist).with_context(|| {
                format!(
                    "failed to preserve legacy runner service {}",
                    legacy_plist.display()
                )
            })
        })
        .transpose()?;

    authorize_system_service_restart()?;
    let system_was_loaded = system_launchd_service_loaded(&system_service)?;
    if system_was_loaded && previous_system_contents.is_none() {
        anyhow::bail!(
            "the existing system runner is loaded but its plist is missing; refusing to replace a service that cannot be restored"
        );
    }

    let claim_barrier = if args.managed_local
        && (legacy_was_loaded || system_was_loaded)
        && !transition_barrier_held
    {
        let (active_plist, active_service_kind) = if system_was_loaded {
            (&previous_system_plist, "system")
        } else {
            (&legacy_plist, "login-session")
        };
        let active_arguments = plist_program_arguments(active_plist)?;
        let active_config =
            runner_config_from_program_arguments(&active_arguments).with_context(|| {
                format!("active {active_service_kind} runner service has no config path")
            })?;
        let active_runner = read_runner_config(&active_config)?.with_context(|| {
            format!(
                "installed managed runner config is missing: {}",
                active_config.display()
            )
        })?;
        let barrier =
            acquire_runner_claim_barrier(&active_runner.daemon_url, args.state_file.as_deref())
                .await?;
        if let Err(error) = barrier.wait_for_runner(&active_runner.runner_id).await {
            barrier
                .release()
                .await
                .context("runner drain failed and releasing the claim barrier also failed")?;
            return Err(error);
        }
        Some(barrier)
    } else {
        None
    };
    if let Some(barrier) = claim_barrier.as_ref() {
        barrier.ensure_healthy().await?;
    }

    let install_result = async {
        let (config, runner, enrolled, enrollment_snapshot) =
            ensure_runner_service_config(
                &args,
                requested_config.clone(),
                existing_managed_config.is_some()
                    || (args.managed_local && previous_config.is_some()),
                &credential_action,
            )
            .await?;
        let enrollment = enrollment_snapshot
            .as_ref()
            .map(|snapshot| (snapshot, runner.runner_id.as_str()));
        let config = match fs::canonicalize(&config)
            .with_context(|| format!("failed to resolve runner config {}", config.display()))
        {
            Ok(config) => config,
            Err(error) => {
                rollback_runner_configuration(
                    &requested_config,
                    previous_config.as_ref(),
                    enrollment,
                )
                .await?;
                return Err(error);
            }
        };
        let rendered = render_runner_launch_daemon(
            &executable,
            &config,
            &acknowledgement,
            &home,
            &working_dir,
            &log_path,
            path,
            &user,
            &current_effective_uid().to_string(),
        );
        let plist = match write_system_runner_plist(&rendered) {
            Ok(plist) => plist,
            Err(error) => {
                rollback_runner_configuration(
                    &requested_config,
                    previous_config.as_ref(),
                    enrollment,
                )
                .await?;
                return Err(error);
            }
        };

        let transition = publish_runner_release_marker(&working_dir)
            .and_then(|()| stop_user_launchd_service(&service))
            .and_then(|()| stop_system_launchd_service(&system_service))
            .and_then(|()| start_system_runner_service(&plist, &system_service, true))
            .and_then(|()| {
                if legacy_plist.exists() {
                    fs::remove_file(&legacy_plist).with_context(|| {
                        format!(
                            "failed to remove legacy runner service {}",
                            legacy_plist.display()
                        )
                    })?;
                }
                Ok(())
            })
            .and_then(|()| {
                if let Some(existing_config) = existing_managed_config.as_deref() {
                    remove_migrated_managed_runner_config(existing_config, &requested_config)?;
                }
                Ok(())
            });
        if let Err(error) = transition {
            let rollback = rollback_runner_service_install(
                &working_dir,
                &requested_config,
                previous_config.as_ref(),
                existing_managed_config
                    .as_deref()
                    .zip(previous_migrated_config.as_ref()),
                previous_system_contents
                    .as_deref()
                    .map(|contents| (contents, system_was_loaded)),
                previous_legacy_contents.as_deref().map(|contents| {
                    (
                        legacy_plist.as_path(),
                        contents,
                        domain.as_str(),
                        service.as_str(),
                        legacy_was_loaded,
                    )
                }),
                enrollment,
            )
            .await;
            if let Err(rollback_error) = rollback {
                anyhow::bail!(
                    "{error}; restoring the previous runner service also failed: {rollback_error}"
                );
            }
            return Err(error);
        }
        if let Some((_database, installed_version, restored_version)) = update_ack {
            let acknowledgement = async {
                let ack = prepare_runner_update_ack(
                    &runner.daemon_url,
                    args.state_file.as_deref(),
                    installed_version,
                    restored_version,
                )
                .await?;
                ack.wait_for(ReleaseActivation::Installed).await
            }
            .await;
            if let Err(error) = acknowledgement {
                let rollback = rollback_runner_service_install(
                    &working_dir,
                    &requested_config,
                    previous_config.as_ref(),
                    existing_managed_config
                        .as_deref()
                        .zip(previous_migrated_config.as_ref()),
                    previous_system_contents
                        .as_deref()
                        .map(|contents| (contents, system_was_loaded)),
                    previous_legacy_contents.as_deref().map(|contents| {
                        (
                            legacy_plist.as_path(),
                            contents,
                            domain.as_str(),
                            service.as_str(),
                            legacy_was_loaded,
                        )
                    }),
                    enrollment,
                )
                .await;
                if let Err(rollback_error) = rollback {
                    anyhow::bail!(
                        "{error}; restoring the previous runner service also failed: {rollback_error}"
                    );
                }
                return Err(error);
            }
        }
        Ok((runner, enrolled, plist))
    }
    .await;
    let barrier_release = match claim_barrier {
        Some(barrier) => barrier.release().await,
        None => Ok(()),
    };
    let (runner, enrolled, plist) = match (install_result, barrier_release) {
        (Ok(result), Ok(())) => result,
        (Err(error), Ok(())) => return Err(error),
        (Ok(result), Err(error)) => {
            eprintln!(
                "Warning: runner service is active, but releasing the transition barrier failed: {error:#}"
            );
            result
        }
        (Err(error), Err(release_error)) => {
            return Err(error.context(format!(
                "releasing the runner claim barrier also failed: {release_error}"
            )));
        }
    };
    if !quiet {
        println!("Installed and started boot-time runner service: {RUNNER_SERVICE_LABEL}");
        println!("Runner: {} ({})", runner.name, runner.runner_id);
        if enrolled {
            println!("This local Oore installation was enrolled automatically.");
        }
        println!("Plist: {}", plist.display());
        println!("Logs:  {}", log_path.display());
    }
    Ok(())
}

fn handle_runner_uninstall_service() -> anyhow::Result<()> {
    if !cfg!(target_os = "macos") {
        anyhow::bail!("managed runner service removal is supported on macOS only");
    }
    let _ = current_user_name()?;
    let system_plist = system_runner_plist();
    let system_acknowledgement = system_plist
        .is_file()
        .then(|| managed_runner_service_spec(&system_plist))
        .transpose()?
        .flatten()
        .map(|spec| spec.acknowledgement);
    let system_service = format!("system/{RUNNER_SERVICE_LABEL}");
    let system_service_present =
        system_plist.exists() || user_launchd_service_loaded(&system_service);
    if system_service_present {
        authorize_system_service_restart()?;
        stop_system_launchd_service(&system_service)?;
        if let Some(path) = &system_acknowledgement {
            clear_runner_service_acknowledgement(path)?;
        }
        sudo_tool_checked(
            "/bin/rm",
            &[OsStr::new("-f"), system_plist.as_os_str()],
            "removing the system runner plist",
        )?;
    }

    let plist = launch_agent_plist(RUNNER_SERVICE_LABEL)?;
    let legacy_acknowledgement = plist
        .is_file()
        .then(|| managed_runner_service_spec(&plist))
        .transpose()?
        .flatten()
        .map(|spec| spec.acknowledgement);
    let (_, service) = current_user_launchd_domain()?;
    stop_user_launchd_service(&service)?;
    if let Some(path) = &legacy_acknowledgement {
        clear_runner_service_acknowledgement(path)?;
    }
    if plist.exists() {
        fs::remove_file(&plist)?;
    }
    println!("Removed runner service: {RUNNER_SERVICE_LABEL}");
    println!("Runner registration and logs were left untouched.");
    Ok(())
}

fn handle_runner_uninstall_service_direct() -> anyhow::Result<()> {
    let install_root = resolve_install_root()?;
    let _lifecycle_lock = install_lock::InstallLock::acquire(&install_root)?;
    refuse_profile_runner_service_command(&install_root, "uninstall-service")?;
    handle_runner_uninstall_service()
}

fn launch_agent_plist(label: &str) -> anyhow::Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("could not determine home directory")?
        .join("Library/LaunchAgents")
        .join(format!("{label}.plist")))
}

fn managed_service_plist(label: &str) -> anyhow::Result<(PathBuf, bool)> {
    let system_plist = PathBuf::from("/Library/LaunchDaemons").join(format!("{label}.plist"));
    if system_plist.is_file() {
        return Ok((system_plist, true));
    }
    Ok((launch_agent_plist(label)?, false))
}

fn url_from_socket_address(address: SocketAddr) -> String {
    let ip = match address.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    format!("http://{}", SocketAddr::new(ip, address.port()))
}

fn url_from_listen_address(listen: &str) -> anyhow::Result<String> {
    let listen = listen.trim();
    if listen.is_empty() {
        anyhow::bail!("service listen address is empty");
    }
    if let Ok(address) = listen.parse::<SocketAddr>() {
        return Ok(url_from_socket_address(address));
    }

    let url = url::Url::parse(listen)
        .or_else(|_| url::Url::parse(&format!("http://{listen}")))
        .with_context(|| format!("invalid service listen address: {listen}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("unsupported service URL scheme: {}", url.scheme());
    }
    let host = url
        .host_str()
        .context("service listen URL is missing a host")?;
    let port = url.port().unwrap_or(80);
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(url_from_socket_address(SocketAddr::new(ip, port)));
    }
    Ok(format!("http://{host}:{port}"))
}

fn daemon_url_from_listen_address(listen: &str) -> anyhow::Result<String> {
    let address = listen
        .trim()
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid daemon listen address: {listen}"))?;
    Ok(format!("http://127.0.0.1:{}", address.port()))
}

fn plutil_output(plist: &Path, args: &[&str]) -> anyhow::Result<std::process::Output> {
    let output = std::process::Command::new("/usr/bin/plutil")
        .args(args)
        .arg(plist)
        .output()
        .with_context(|| format!("failed to read launchd plist {}", plist.display()))?;
    if output.status.success() {
        return Ok(output);
    }
    std::process::Command::new("/usr/bin/sudo")
        .args(["-n", "/usr/bin/plutil"])
        .args(args)
        .arg(plist)
        .output()
        .with_context(|| format!("failed to read protected launchd plist {}", plist.display()))
}

fn plist_program_arguments(plist: &Path) -> anyhow::Result<Vec<String>> {
    let output = plutil_output(plist, &["-extract", "ProgramArguments", "json", "-o", "-"])?;
    if !output.status.success() {
        anyhow::bail!("failed to read ProgramArguments from {}", plist.display());
    }
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("invalid ProgramArguments in {}", plist.display()))
}

fn plist_json(plist: &Path) -> anyhow::Result<serde_json::Value> {
    let output = plutil_output(plist, &["-convert", "json", "-o", "-"])?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to convert launchd plist {} to JSON",
            plist.display()
        );
    }
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("invalid launchd plist {}", plist.display()))
}

fn validate_managed_service_executable(
    document: &serde_json::Value,
    expected: &Path,
    label: &str,
) -> anyhow::Result<()> {
    let configured = document
        .get("ProgramArguments")
        .and_then(serde_json::Value::as_array)
        .and_then(|arguments| arguments.first())
        .and_then(serde_json::Value::as_str)
        .context("managed service plist has no executable argument")?;
    let configured = fs::canonicalize(configured)
        .with_context(|| format!("failed to resolve {label} executable {configured}"))?;
    let expected = fs::canonicalize(expected)
        .with_context(|| format!("failed to resolve expected {label} executable"))?;
    anyhow::ensure!(
        configured == expected,
        "managed service {label} runs {}, not the selected install at {}",
        configured.display(),
        expected.display()
    );
    Ok(())
}

fn daemon_data_paths_from_plist(
    document: &serde_json::Value,
    fallback_data_dir: &Path,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let program_arguments = document
        .get("ProgramArguments")
        .and_then(serde_json::Value::as_array)
        .context("managed daemon plist has no ProgramArguments")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .context("managed daemon ProgramArguments must be strings")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let environment = document
        .get("EnvironmentVariables")
        .and_then(serde_json::Value::as_object);
    let env_value = |key: &str| {
        environment
            .and_then(|values| values.get(key))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
    };
    let working_directory = document
        .get("WorkingDirectory")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from);
    let absolute = |raw: &str| {
        let path = PathBuf::from(raw);
        if path.is_absolute() {
            path
        } else {
            working_directory
                .as_deref()
                .unwrap_or_else(|| Path::new("."))
                .join(path)
        }
    };
    let data_dir = env_value("OORED_DATA_DIR")
        .or_else(|| env_value("OORE_DATA_DIR"))
        .map(absolute)
        .or_else(|| {
            env_value("HOME").map(|home| {
                absolute(home)
                    .join("Library")
                    .join("Application Support")
                    .join("oore")
            })
        })
        .unwrap_or_else(|| fallback_data_dir.to_path_buf());
    let database = value_from_program_arguments(&program_arguments, "--state-file")
        .as_deref()
        .or_else(|| env_value("OORE_SETUP_STATE_FILE"))
        .map(absolute)
        .unwrap_or_else(|| data_dir.join(BACKUP_DATABASE_FILE));
    // oored has no key path override: the key always lives in its data root.
    let key = oore_contract::data_paths::encryption_key_path(&data_dir);
    Ok((database, key))
}

fn listen_address_from_program_arguments(program_args: &[String]) -> anyhow::Result<String> {
    for (index, arg) in program_args.iter().enumerate() {
        if arg == "--listen" {
            return program_args
                .get(index + 1)
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .context("--listen has no value");
        }
        if let Some(value) = arg.strip_prefix("--listen=")
            && !value.trim().is_empty()
        {
            return Ok(value.to_string());
        }
    }
    anyhow::bail!("service ProgramArguments do not include --listen")
}

fn managed_service_listen_address(label: &str) -> anyhow::Result<String> {
    let (plist, _) = managed_service_plist(label)?;
    if !plist.is_file() {
        anyhow::bail!("managed service plist is missing: {}", plist.display());
    }
    let program_args = plist_program_arguments(&plist)?;
    listen_address_from_program_arguments(&program_args)
        .with_context(|| format!("failed to find listen address in {}", plist.display()))
}

fn launchd_service_loaded(label: &str) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let Ok((_, system)) = managed_service_plist(label) else {
        return false;
    };
    let service = if system {
        format!("system/{label}")
    } else {
        let uid = current_effective_uid();
        format!("gui/{uid}/{label}")
    };
    std::process::Command::new("/bin/launchctl")
        .args(["print", &service])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn restart_launchd_service(label: &str) -> anyhow::Result<()> {
    if !cfg!(target_os = "macos") {
        anyhow::bail!("managed service restart is supported on macOS only");
    }
    let (plist, system) = managed_service_plist(label)?;
    if !plist.is_file() {
        anyhow::bail!("managed service plist is missing: {}", plist.display());
    }
    let (service, domain) = if system {
        (format!("system/{label}"), "system".to_string())
    } else {
        let uid = current_effective_uid();
        (format!("gui/{uid}/{label}"), format!("gui/{uid}"))
    };
    let command = if system {
        "/usr/bin/sudo"
    } else {
        "/bin/launchctl"
    };
    let prefix: &[&str] = if system {
        &["-n", "/bin/launchctl"]
    } else {
        &[]
    };
    if launchd_service_loaded(label) {
        let mut bootout = std::process::Command::new(command);
        bootout.args(prefix).args(["bootout", &service]);
        let status =
            run_command_with_timeout(&mut bootout, "launchd bootout", Duration::from_secs(15))?;
        if !status.success() && launchd_service_loaded(label) {
            anyhow::bail!("failed to stop managed service {label} before restart");
        }
    }

    let mut bootstrap = std::process::Command::new(command);
    bootstrap
        .args(prefix)
        .args(["bootstrap", &domain, &plist.display().to_string()]);
    let status =
        run_command_with_timeout(&mut bootstrap, "launchd bootstrap", Duration::from_secs(15))?;
    if !status.success() {
        anyhow::bail!("failed to bootstrap managed service {label}");
    }
    let mut kickstart = std::process::Command::new(command);
    kickstart.args(prefix).args(["kickstart", "-k", &service]);
    let _ = run_command_with_timeout(&mut kickstart, "launchd kickstart", Duration::from_secs(15))?;
    Ok(())
}

fn stop_launchd_service(label: &str) -> anyhow::Result<()> {
    if !cfg!(target_os = "macos") {
        anyhow::bail!("managed service stop is supported on macOS only");
    }
    let (_, system) = managed_service_plist(label)?;
    let service = if system {
        format!("system/{label}")
    } else {
        let uid = std::process::Command::new("/usr/bin/id")
            .arg("-u")
            .output()
            .context("failed to determine current user id")?;
        let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();
        format!("gui/{uid}/{label}")
    };
    let mut command = if system {
        let mut command = std::process::Command::new("/usr/bin/sudo");
        command.args(["-n", "/bin/launchctl"]);
        command
    } else {
        std::process::Command::new("/bin/launchctl")
    };
    let status = run_command_with_timeout(
        command.args(["bootout", &service]),
        "launchd bootout",
        Duration::from_secs(15),
    )?;
    if !status.success() && launchd_service_loaded(label) {
        anyhow::bail!("failed to stop managed service {label}");
    }
    Ok(())
}

fn run_command_with_timeout(
    command: &mut std::process::Command,
    description: &str,
    timeout: Duration,
) -> anyhow::Result<std::process::ExitStatus> {
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start {description}"))?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("failed to wait for {description}"))?
        {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "{description} timed out after {} seconds",
                timeout.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn authorize_system_service_restart() -> anyhow::Result<()> {
    use std::io::IsTerminal;

    let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    if !interactive {
        let output = sudo_tool_output("/usr/bin/true", &[])?;
        anyhow::ensure!(
            output.status.success(),
            "administrator access is required; run `sudo -v` in an interactive terminal, then retry"
        );
        return Ok(());
    }

    eprintln!("\nAdministrator access is required to manage Oore's macOS system services.");
    eprintln!("Your password is requested by sudo and is not stored by Oore.");
    let mut command = std::process::Command::new("/usr/bin/sudo");
    command.arg("-v");
    let status = run_command_with_timeout(
        &mut command,
        "requesting administrator access",
        Duration::from_secs(60),
    )?;
    if !status.success() {
        anyhow::bail!("administrator access was not granted; no service changes were made");
    }
    Ok(())
}

async fn endpoint_is_healthy(client: &reqwest::Client, url: &str) -> bool {
    client
        .get(url)
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

async fn wait_for_endpoint(client: &reqwest::Client, url: &str, name: &str) -> anyhow::Result<()> {
    for _ in 0..15 {
        if endpoint_is_healthy(client, url).await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    anyhow::bail!("{name} failed its readiness check: {url}")
}

async fn wait_for_daemon_release(
    client: &reqwest::Client,
    ready_url: &str,
    expected_package_version: &str,
    name: &str,
) -> anyhow::Result<()> {
    let mut health_url = url::Url::parse(ready_url)
        .with_context(|| format!("invalid {name} readiness URL: {ready_url}"))?;
    health_url.set_path("/healthz");
    health_url.set_query(None);
    health_url.set_fragment(None);
    for _ in 0..30 {
        let ready = endpoint_is_healthy(client, ready_url).await;
        let version_matches = if ready {
            match client
                .get(health_url.clone())
                .timeout(Duration::from_secs(3))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    response
                        .json::<serde_json::Value>()
                        .await
                        .ok()
                        .and_then(|value| {
                            value
                                .get("package_version")
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string)
                        })
                        .as_deref()
                        == Some(expected_package_version)
                }
                _ => false,
            }
        } else {
            false
        };
        if version_matches {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    anyhow::bail!(
        "{name} did not become ready as package version {expected_package_version}: {ready_url}"
    )
}

async fn daemon_package_version(
    client: &reqwest::Client,
    daemon_url: &str,
) -> anyhow::Result<String> {
    let response = client
        .get(endpoint_url(daemon_url, "/healthz"))
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .context("failed to read the running daemon version")?
        .error_for_status()
        .context("running daemon health check failed")?;
    let payload: serde_json::Value = response
        .json()
        .await
        .context("running daemon health response was invalid")?;
    payload
        .get("package_version")
        .and_then(serde_json::Value::as_str)
        .filter(|version| !version.is_empty())
        .map(str::to_string)
        .context("running daemon health response omitted package_version")
}

fn copy_release_snapshot(install_root: &Path, snapshot: &Path) -> anyhow::Result<()> {
    for relative in [
        "bin/oore",
        "bin/oored",
        "bin/oore-web",
        "VERSION",
        "WEB_VERSION",
        "CHANNEL",
        "WEB_CHANNEL",
        "GITHUB_REPO",
        "WEB_GITHUB_REPO",
        "BOOTSTRAP_ARCHIVE",
        "BOOTSTRAP_SHA256",
        "BOOTSTRAP_MANIFEST_SHA256",
        "install-manifest.json",
        "LICENSE",
    ] {
        let source = install_root.join(relative);
        if source.is_file() {
            let destination = snapshot.join(relative);
            fs::create_dir_all(destination.parent().expect("snapshot file parent"))?;
            fs::copy(&source, &destination)?;
        }
    }
    let web = install_root.join("web-dist");
    if web.is_dir() {
        copy_dir_recursive(&web, &snapshot.join("web-dist"))?;
    }
    Ok(())
}

fn atomic_replace_file(source: &Path, destination: &Path, executable: bool) -> anyhow::Result<()> {
    let parent = destination
        .parent()
        .context("release destination has no parent")?;
    fs::create_dir_all(parent)?;
    let stem = destination
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let staged = tempfile::Builder::new()
        .prefix(&format!(".{stem}.update-"))
        .tempfile_in(parent)
        .with_context(|| format!("failed to stage {}", destination.display()))?;
    fs::copy(source, staged.path())
        .with_context(|| format!("failed to stage {}", destination.display()))?;
    if executable {
        set_executable(staged.path())?;
    }
    staged
        .as_file()
        .sync_all()
        .with_context(|| format!("failed to sync staged file for {}", destination.display()))?;
    staged
        .persist(destination)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to atomically replace {}", destination.display()))?;
    Ok(())
}

fn runner_release_marker_path(install_root: &Path) -> PathBuf {
    install_root.join(oore_runner::RUNNER_RELEASE_MARKER_FILE)
}

fn write_runner_release_marker(install_root: &Path, contents: &[u8]) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(install_root)?;
    let path = runner_release_marker_path(install_root);
    let mut staged = tempfile::Builder::new()
        .prefix(".runner-release-")
        .tempfile_in(install_root)
        .context("failed to stage the runner release marker")?;
    staged
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o644))?;
    staged.as_file_mut().write_all(contents)?;
    staged.as_file_mut().sync_all()?;
    staged
        .persist(&path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to publish runner release marker {}", path.display()))?;
    Ok(())
}

fn publish_runner_release_marker(install_root: &Path) -> anyhow::Result<()> {
    let marker = oore_runner::runner_release_marker(&install_root.join("bin/oore"))?;
    write_runner_release_marker(install_root, format!("{marker}\n").as_bytes())
}

fn ensure_runner_release_marker(install_root: &Path) -> anyhow::Result<()> {
    publish_runner_release_marker(install_root)
}

fn restore_runner_release_marker(install_root: &Path) -> anyhow::Result<()> {
    publish_runner_release_marker(install_root)
}

fn atomic_replace_directory(source: &Path, destination: &Path) -> anyhow::Result<()> {
    let parent = destination
        .parent()
        .context("release destination has no parent")?;
    fs::create_dir_all(parent)?;
    let stem = destination
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let staged = tempfile::Builder::new()
        .prefix(&format!(".{stem}.update-"))
        .tempdir_in(parent)
        .with_context(|| format!("failed to stage directory {}", destination.display()))?;
    let next = staged.path().join("next");
    copy_dir_recursive(source, &next)?;

    let previous = if destination.exists() {
        let holder = tempfile::Builder::new()
            .prefix(&format!(".{stem}.previous-"))
            .tempdir_in(parent)
            .with_context(|| format!("failed to preserve directory {}", destination.display()))?;
        let path = holder.path().join("previous");
        fs::rename(destination, &path).with_context(|| {
            format!(
                "failed to preserve existing directory {}",
                destination.display()
            )
        })?;
        Some((holder, path))
    } else {
        None
    };

    if let Err(error) = fs::rename(&next, destination) {
        if let Some((holder, path)) = previous {
            if let Err(restore_error) = fs::rename(&path, destination) {
                let preserved = holder.keep().join("previous");
                anyhow::bail!(
                    "failed to atomically replace {}: {error}; restoring the previous directory also failed: {restore_error}; previous files remain at {}",
                    destination.display(),
                    preserved.display()
                );
            }
            let _ = holder.close();
        }
        return Err(error)
            .with_context(|| format!("failed to atomically replace {}", destination.display()));
    }

    if let Some((holder, _)) = previous {
        let _ = holder.close();
    }
    Ok(())
}

fn restore_release_snapshot(install_root: &Path, snapshot: &Path) -> anyhow::Result<()> {
    for relative in [
        "bin/oore",
        "bin/oored",
        "bin/oore-web",
        "VERSION",
        "WEB_VERSION",
        "CHANNEL",
        "WEB_CHANNEL",
        "GITHUB_REPO",
        "WEB_GITHUB_REPO",
        "BOOTSTRAP_ARCHIVE",
        "BOOTSTRAP_SHA256",
        "BOOTSTRAP_MANIFEST_SHA256",
        "install-manifest.json",
        "LICENSE",
    ] {
        let source = snapshot.join(relative);
        let destination = install_root.join(relative);
        if source.is_file() {
            atomic_replace_file(&source, &destination, relative.starts_with("bin/"))?;
        } else if destination.exists() {
            fs::remove_file(destination)?;
        }
    }
    let source = snapshot.join("web-dist");
    let destination = install_root.join("web-dist");
    if source.is_dir() {
        atomic_replace_directory(&source, &destination)?;
    } else if destination.exists() {
        fs::remove_dir_all(destination)?;
    }
    Ok(())
}

fn install_staged_release(
    stage: &Path,
    install_root: &Path,
    channel: ReleaseChannel,
    repo: &str,
    payload: Option<UpdatePayloadSelection>,
) -> anyhow::Result<()> {
    for (relative, executable) in [
        ("bin/oore", true),
        ("bin/oored", true),
        ("bin/oore-web", true),
        ("VERSION", false),
    ] {
        let selected = match relative {
            "bin/oored" => payload.is_none_or(|payload| payload.control_plane),
            "bin/oore-web" => payload.is_none_or(|payload| payload.web),
            _ => true,
        };
        if !selected {
            continue;
        }
        let source = stage.join(relative);
        if source.is_file() {
            atomic_replace_file(&source, &install_root.join(relative), executable)?;
        }
    }
    let web = stage.join("web-dist");
    if payload.is_none_or(|payload| payload.web) && web.is_dir() {
        let version = stage.join("VERSION");
        if version.is_file() {
            atomic_replace_file(&version, &install_root.join("WEB_VERSION"), false)?;
        }
        atomic_replace_directory(&web, &install_root.join("web-dist"))?;
        fs::write(install_root.join("WEB_CHANNEL"), channel.as_str())?;
        fs::write(install_root.join("WEB_GITHUB_REPO"), repo)?;
    }
    let license = stage.join("LICENSE");
    if license.is_file() {
        atomic_replace_file(&license, &install_root.join("LICENSE"), false)?;
    }
    fs::write(install_root.join("CHANNEL"), channel.as_str())?;
    fs::write(install_root.join("GITHUB_REPO"), repo)?;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReleaseActivation {
    Installed,
    Restored,
}

struct UpdateServicePlan {
    profile_update: Option<ProfileUpdatePlan>,
    defer_daemon_restart: bool,
    daemon_is_managed: bool,
    unmanaged_daemon_listen: Option<String>,
    daemon_should_be_running: bool,
    daemon_ready_url: String,
    installed_version: String,
    restored_version: String,
    runner_service: Option<&'static str>,
    runner_previous_pid: Option<u32>,
    runner_ack: Option<RunnerUpdateAck>,
    runner_installed_version: String,
    runner_restored_version: String,
    install_managed_runner: bool,
    managed_runner_service_existed: bool,
    managed_runner_config_existed: bool,
    managed_runner_database: Option<PathBuf>,
    web_is_managed: bool,
    web_was_running: bool,
    web_ready_url: Option<String>,
}

struct UpdateRollbackState {
    backup: PathBuf,
    database: PathBuf,
    key: PathBuf,
}

struct ManagedDaemonStoppedIntentGuard {
    restore_stopped: bool,
}

impl ManagedDaemonStoppedIntentGuard {
    fn disarm(&mut self) {
        self.restore_stopped = false;
    }
}

impl Drop for ManagedDaemonStoppedIntentGuard {
    fn drop(&mut self) {
        if self.restore_stopped {
            let _ = stop_launchd_service(DAEMON_SERVICE_LABEL);
        }
    }
}

struct ManagedDaemonServiceMigration {
    source_plist: PathBuf,
    source_was_system: bool,
    source_was_loaded: bool,
    source_service: String,
    transformed_plist: PathBuf,
    source_snapshot: PathBuf,
    target_plist: PathBuf,
}

impl ManagedDaemonServiceMigration {
    fn apply(&self) -> anyhow::Result<()> {
        sudo_tool_checked(
            "/usr/bin/install",
            &[
                OsStr::new("-o"),
                OsStr::new("root"),
                OsStr::new("-g"),
                OsStr::new("wheel"),
                OsStr::new("-m"),
                OsStr::new("0600"),
                self.transformed_plist.as_os_str(),
                self.target_plist.as_os_str(),
            ],
            "installing the migrated oored service",
        )?;
        if !self.source_was_system && self.source_was_loaded {
            let status = std::process::Command::new("/bin/launchctl")
                .args(["bootout", &self.source_service])
                .status()
                .context("failed to stop the legacy oored login service")?;
            if !status.success() {
                anyhow::bail!("failed to stop the legacy oored login service");
            }
        }
        Ok(())
    }

    fn rollback(&self) -> anyhow::Result<()> {
        let system_service = format!("system/{DAEMON_SERVICE_LABEL}");
        stop_system_launchd_service(&system_service)?;
        if self.source_was_system {
            sudo_tool_checked(
                "/usr/bin/install",
                &[
                    OsStr::new("-o"),
                    OsStr::new("root"),
                    OsStr::new("-g"),
                    OsStr::new("wheel"),
                    OsStr::new("-m"),
                    OsStr::new("0600"),
                    self.source_snapshot.as_os_str(),
                    self.target_plist.as_os_str(),
                ],
                "restoring the previous oored service",
            )?;
        } else {
            sudo_tool_checked(
                "/bin/rm",
                &[OsStr::new("-f"), self.target_plist.as_os_str()],
                "removing the migrated oored system service",
            )?;
            if let Some(parent) = self.source_plist.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&self.source_snapshot, &self.source_plist).with_context(|| {
                format!(
                    "failed to restore legacy oored service {}",
                    self.source_plist.display()
                )
            })?;
        }
        Ok(())
    }

    fn commit(&self) -> anyhow::Result<()> {
        if !self.source_was_system && self.source_plist.is_file() {
            fs::rename(&self.source_plist, &self.source_snapshot).with_context(|| {
                format!(
                    "failed to retire legacy oored service {}",
                    self.source_plist.display()
                )
            })?;
        }
        Ok(())
    }
}

struct ManagedRunnerServiceSnapshot {
    system_plist: PathBuf,
    system_contents: Option<Vec<u8>>,
    system_was_loaded: bool,
    legacy_plist: PathBuf,
    legacy_contents: Option<Vec<u8>>,
    legacy_domain: String,
    legacy_service: String,
    legacy_was_loaded: bool,
    configs: Vec<(PathBuf, Option<Vec<u8>>)>,
}

trait ManagedRunnerServiceRollback {
    fn stop_and_restore_files(&self) -> anyhow::Result<()>;
    fn start(&self) -> anyhow::Result<()>;
}

impl ManagedRunnerServiceSnapshot {
    fn capture() -> anyhow::Result<Self> {
        let system_plist = system_runner_plist();
        let legacy_plist = launch_agent_plist(RUNNER_SERVICE_LABEL)?;
        let (legacy_domain, legacy_service) = current_user_launchd_domain()?;
        let system_service = format!("system/{RUNNER_SERVICE_LABEL}");
        let mut config_paths: Vec<PathBuf> = Vec::new();
        for plist in [&system_plist, &legacy_plist] {
            if plist.is_file() {
                let arguments = plist_program_arguments(plist)?;
                if let Some(path) = runner_config_from_program_arguments(&arguments)
                    && !config_paths
                        .iter()
                        .any(|existing| paths_refer_to_same_file(existing, &path))
                {
                    config_paths.push(path);
                }
            }
        }
        let managed_config = managed_local_runner_config_path()?;
        if !config_paths
            .iter()
            .any(|existing| paths_refer_to_same_file(existing, &managed_config))
        {
            config_paths.push(managed_config);
        }
        let configs = config_paths
            .into_iter()
            .map(|path| {
                let contents = match fs::read(&path) {
                    Ok(contents) => Some(contents),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error.into()),
                };
                Ok((path, contents))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self {
            system_contents: system_plist
                .is_file()
                .then(|| read_service_plist_bytes(&system_plist, true))
                .transpose()?,
            system_was_loaded: system_launchd_service_loaded(&system_service)?,
            legacy_contents: legacy_plist
                .is_file()
                .then(|| fs::read(&legacy_plist))
                .transpose()?,
            legacy_was_loaded: user_launchd_service_loaded(&legacy_service),
            system_plist,
            legacy_plist,
            legacy_domain,
            legacy_service,
            configs,
        })
    }
}

impl ManagedRunnerServiceRollback for ManagedRunnerServiceSnapshot {
    fn stop_and_restore_files(&self) -> anyhow::Result<()> {
        let active_acknowledgement = self
            .system_plist
            .is_file()
            .then(|| managed_runner_service_spec(&self.system_plist))
            .transpose()?
            .flatten()
            .map(|spec| spec.acknowledgement);
        let system_service = format!("system/{RUNNER_SERVICE_LABEL}");
        stop_system_launchd_service(&system_service)?;
        if let Some(path) = &active_acknowledgement {
            clear_runner_service_acknowledgement(path)?;
        }
        stop_user_launchd_service(&self.legacy_service)?;
        match self.system_contents.as_deref() {
            Some(contents) => {
                let temporary = tempfile::NamedTempFile::new()
                    .context("failed to stage the previous runner service")?;
                fs::write(temporary.path(), contents)?;
                sudo_tool_checked(
                    "/usr/bin/install",
                    &[
                        OsStr::new("-o"),
                        OsStr::new("root"),
                        OsStr::new("-g"),
                        OsStr::new("wheel"),
                        OsStr::new("-m"),
                        OsStr::new("0644"),
                        temporary.path().as_os_str(),
                        self.system_plist.as_os_str(),
                    ],
                    "restoring the previous runner service",
                )?;
            }
            None => sudo_tool_checked(
                "/bin/rm",
                &[OsStr::new("-f"), self.system_plist.as_os_str()],
                "removing the new runner service",
            )?,
        }
        match self.legacy_contents.as_deref() {
            Some(contents) => {
                if let Some(parent) = self.legacy_plist.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&self.legacy_plist, contents)?;
            }
            None => {
                if self.legacy_plist.is_file() {
                    fs::remove_file(&self.legacy_plist)?;
                }
            }
        }
        for (path, contents) in &self.configs {
            match contents {
                Some(contents) => {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(path, contents)?;
                    set_private_permissions(path)?;
                }
                None => {
                    if path.is_file() {
                        fs::remove_file(path)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn start(&self) -> anyhow::Result<()> {
        if self.system_was_loaded {
            start_system_runner_service(
                &self.system_plist,
                &format!("system/{RUNNER_SERVICE_LABEL}"),
                false,
            )?;
        }
        if self.legacy_was_loaded {
            restore_legacy_runner_service(
                &self.legacy_plist,
                &self.legacy_domain,
                &self.legacy_service,
            )?;
        }
        Ok(())
    }
}

fn read_service_plist_bytes(path: &Path, system: bool) -> anyhow::Result<Vec<u8>> {
    if !system {
        return fs::read(path)
            .with_context(|| format!("failed to preserve service plist {}", path.display()));
    }
    let output = sudo_tool_output("/bin/cat", &[path.as_os_str()])?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to preserve protected service plist {}",
            path.display()
        );
    }
    Ok(output.stdout)
}

fn prepare_managed_daemon_service_migration(
    install_root: &Path,
    source_plist: &Path,
    source_was_system: bool,
    source_was_loaded: bool,
    document: &serde_json::Value,
    transaction_dir: &Path,
) -> anyhow::Result<ManagedDaemonServiceMigration> {
    use std::os::unix::fs::PermissionsExt;

    let mut transformed = document.clone();
    let object = transformed
        .as_object_mut()
        .context("managed daemon plist must contain a dictionary")?;
    let arguments = object
        .get_mut("ProgramArguments")
        .and_then(serde_json::Value::as_array_mut)
        .context("managed daemon plist has no ProgramArguments")?;
    let executable = arguments
        .first_mut()
        .context("managed daemon plist has no executable argument")?;
    *executable = serde_json::Value::String(
        fs::canonicalize(install_root.join("bin/oored"))?
            .display()
            .to_string(),
    );
    if let Some(environment) = object
        .get_mut("EnvironmentVariables")
        .and_then(serde_json::Value::as_object_mut)
    {
        environment.remove("OORED_RUNNER_MODE");
    }
    object.insert(
        "UserName".to_string(),
        serde_json::Value::String(current_user_name()?),
    );

    let source_snapshot = transaction_dir.join("oored-service.previous.plist");
    let mut snapshot = create_private_file(&source_snapshot)?;
    snapshot.write_all(&read_service_plist_bytes(source_plist, source_was_system)?)?;
    snapshot.sync_all()?;

    let transformed_plist = transaction_dir.join("oored-service.next.plist");
    let mut staged = create_private_file(&transformed_plist)?;
    serde_json::to_writer_pretty(&mut staged, &transformed)?;
    staged.write_all(b"\n")?;
    staged.sync_all()?;
    let status = std::process::Command::new("/usr/bin/plutil")
        .args(["-convert", "xml1"])
        .arg(&transformed_plist)
        .status()
        .context("failed to encode the migrated oored service plist")?;
    if !status.success() {
        anyhow::bail!("failed to encode the migrated oored service plist");
    }
    fs::set_permissions(&transformed_plist, fs::Permissions::from_mode(0o600))?;

    let source_service = if source_was_system {
        format!("system/{DAEMON_SERVICE_LABEL}")
    } else {
        let output = std::process::Command::new("/usr/bin/id")
            .arg("-u")
            .output()
            .context("failed to determine current user id")?;
        anyhow::ensure!(
            output.status.success(),
            "failed to determine current user id"
        );
        let uid = String::from_utf8(output.stdout)?.trim().to_string();
        format!("gui/{uid}/{DAEMON_SERVICE_LABEL}")
    };
    Ok(ManagedDaemonServiceMigration {
        source_plist: source_plist.to_path_buf(),
        source_was_system,
        source_was_loaded,
        source_service,
        transformed_plist,
        source_snapshot,
        target_plist: PathBuf::from("/Library/LaunchDaemons")
            .join(format!("{DAEMON_SERVICE_LABEL}.plist")),
    })
}

struct RunnerUpdateAck {
    daemon_url: String,
    state_file: Option<String>,
    runner_id: String,
    baseline_heartbeat: Option<i64>,
    installed_version: String,
    restored_version: String,
}

impl RunnerUpdateAck {
    async fn wait_for(&self, activation: ReleaseActivation) -> anyhow::Result<()> {
        let expected_version = match activation {
            ReleaseActivation::Installed => &self.installed_version,
            ReleaseActivation::Restored => &self.restored_version,
        };
        for _ in 0..60 {
            let response = call_operator(
                &self.daemon_url,
                self.state_file.as_deref(),
                OperatorRequest::RunnerStatus {
                    runner_id: self.runner_id.clone(),
                },
            )
            .await?;
            if let OperatorResponse::RunnerStatus {
                status,
                last_heartbeat_at: heartbeat,
                capabilities,
                ..
            } = response
            {
                let version = serde_json::from_str::<serde_json::Value>(&capabilities)
                    .ok()
                    .and_then(|value| value.get("version")?.as_str().map(str::to_string));
                let fresh = heartbeat.is_some_and(|heartbeat| {
                    self.baseline_heartbeat
                        .is_none_or(|baseline| heartbeat > baseline)
                });
                if status == "online"
                    && fresh
                    && version.as_deref() == Some(expected_version.as_str())
                {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        anyhow::bail!(
            "runner {} did not acknowledge version {} after handoff",
            self.runner_id,
            expected_version
        )
    }
}

async fn prepare_runner_update_ack(
    daemon_url: &str,
    state_file: Option<&str>,
    installed_version: &str,
    restored_version: &str,
) -> anyhow::Result<RunnerUpdateAck> {
    let (plist, _) = managed_service_plist(RUNNER_SERVICE_LABEL)?;
    let arguments = plist_program_arguments(&plist)?;
    let config_path = runner_config_from_program_arguments(&arguments)
        .context("managed runner service does not specify --config")?;
    let config = read_runner_config(&config_path)?.with_context(|| {
        format!(
            "managed runner config is missing: {}",
            config_path.display()
        )
    })?;
    let response = call_operator(
        daemon_url,
        state_file,
        OperatorRequest::RunnerStatus {
            runner_id: config.runner_id.clone(),
        },
    )
    .await?;
    let baseline_heartbeat = match response {
        OperatorResponse::RunnerStatus {
            last_heartbeat_at, ..
        } => last_heartbeat_at,
        _ => anyhow::bail!("oored returned an unexpected runner status response"),
    };
    Ok(RunnerUpdateAck {
        daemon_url: daemon_url.to_string(),
        state_file: state_file.map(str::to_string),
        runner_id: config.runner_id,
        baseline_heartbeat,
        installed_version: installed_version.to_string(),
        restored_version: restored_version.to_string(),
    })
}

trait UpdateServiceControl {
    fn restart_managed_service(&mut self, label: &str) -> anyhow::Result<()>;

    async fn activate_runner_service(
        &mut self,
        label: &str,
        _previous_pid: Option<u32>,
        _ack: Option<&RunnerUpdateAck>,
        _activation: ReleaseActivation,
    ) -> anyhow::Result<()> {
        self.restart_managed_service(label)
    }

    fn stop_managed_service(&mut self, label: &str) -> anyhow::Result<()>;

    async fn restart_unmanaged_daemon(&mut self, listen: &str) -> anyhow::Result<()>;

    async fn stop_unmanaged_daemon(&mut self, listen: &str) -> anyhow::Result<()>;

    async fn install_managed_runner(
        &mut self,
        _database: &Path,
        _installed_version: &str,
        _restored_version: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!("managed runner installation is unavailable")
    }

    fn remove_managed_runner(&mut self, _remove_config: bool) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait_for_endpoint(&mut self, url: &str, name: &str) -> anyhow::Result<()>;

    async fn wait_for_daemon_release(
        &mut self,
        ready_url: &str,
        expected_package_version: &str,
        name: &str,
    ) -> anyhow::Result<()> {
        let _ = expected_package_version;
        self.wait_for_endpoint(ready_url, name).await
    }
}

struct LiveUpdateServiceControl<'a> {
    install_root: &'a Path,
    daemon_url: &'a str,
    client: &'a reqwest::Client,
}

impl UpdateServiceControl for LiveUpdateServiceControl<'_> {
    fn restart_managed_service(&mut self, label: &str) -> anyhow::Result<()> {
        restart_launchd_service(label)
    }

    async fn activate_runner_service(
        &mut self,
        label: &str,
        previous_pid: Option<u32>,
        ack: Option<&RunnerUpdateAck>,
        activation: ReleaseActivation,
    ) -> anyhow::Result<()> {
        if label == RUNNER_SERVICE_LABEL {
            let (plist, system) = managed_service_plist(label)?;
            if system {
                let service = format!("system/{RUNNER_SERVICE_LABEL}");
                ensure_system_runner_service_active(&plist, &service, previous_pid)?;
                if let Some(ack) = ack {
                    ack.wait_for(activation).await?;
                }
                return Ok(());
            }
            let program_arguments = plist_program_arguments(&plist)?;
            let config = runner_config_from_program_arguments(&program_arguments)
                .context("legacy runner service does not specify --config")?;
            let daemon_plist = managed_service_plist(DAEMON_SERVICE_LABEL)?.0;
            let state_file = daemon_plist
                .is_file()
                .then(|| plist_program_arguments(&daemon_plist))
                .transpose()?
                .and_then(|arguments| value_from_program_arguments(&arguments, "--state-file"));
            let config = config
                .to_str()
                .context("legacy runner config path is not valid UTF-8")?
                .to_string();
            handle_runner_install_service_inner(
                RunnerServiceArgs {
                    config: Some(config),
                    managed_local: false,
                    daemon_url: Some(self.daemon_url.to_string()),
                    state_file,
                    name: None,
                },
                true,
                None,
                ManagedRunnerCredentialAction::Reuse,
                true,
            )
            .await
            .context("failed to migrate the legacy runner service")?;
            if let Some(ack) = ack {
                ack.wait_for(activation).await?;
            }
            return Ok(());
        }
        restart_launchd_service(label)
    }

    fn stop_managed_service(&mut self, label: &str) -> anyhow::Result<()> {
        stop_launchd_service(label)
    }

    async fn restart_unmanaged_daemon(&mut self, listen: &str) -> anyhow::Result<()> {
        restart_daemon(self.install_root, listen, self.daemon_url, self.client).await
    }

    async fn stop_unmanaged_daemon(&mut self, listen: &str) -> anyhow::Result<()> {
        stop_daemon(self.install_root, listen)
    }

    async fn install_managed_runner(
        &mut self,
        database: &Path,
        installed_version: &str,
        restored_version: &str,
    ) -> anyhow::Result<()> {
        handle_runner_install_service_inner(
            RunnerServiceArgs {
                config: None,
                managed_local: true,
                daemon_url: Some(self.daemon_url.to_string()),
                state_file: Some(database.display().to_string()),
                name: None,
            },
            true,
            Some((database, installed_version, restored_version)),
            ManagedRunnerCredentialAction::Reuse,
            true,
        )
        .await
    }

    fn remove_managed_runner(&mut self, remove_config: bool) -> anyhow::Result<()> {
        handle_runner_uninstall_service()?;
        if remove_config {
            let config = managed_local_runner_config_path()?;
            if config.is_file() {
                fs::remove_file(&config).with_context(|| {
                    format!("failed to remove new runner config {}", config.display())
                })?;
            }
        }
        Ok(())
    }

    async fn wait_for_endpoint(&mut self, url: &str, name: &str) -> anyhow::Result<()> {
        wait_for_endpoint(self.client, url, name).await
    }

    async fn wait_for_daemon_release(
        &mut self,
        ready_url: &str,
        expected_package_version: &str,
        name: &str,
    ) -> anyhow::Result<()> {
        wait_for_daemon_release(self.client, ready_url, expected_package_version, name).await
    }
}

struct DeferredUpdateServiceControl<'a> {
    install_root: &'a Path,
    transaction_dir: &'a Path,
    client: &'a reqwest::Client,
    /// The candidate daemon moved out of `bin/` by `stop_managed_service`. It
    /// lives as long as the control so that a rollback which fails before it
    /// restores the previous release, or unwinds, still leaves a daemon to run.
    quiesced_candidate: Option<update_supervisor::QuiescedCandidateDaemon>,
}

impl UpdateServiceControl for DeferredUpdateServiceControl<'_> {
    fn restart_managed_service(&mut self, label: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            label == DAEMON_SERVICE_LABEL,
            "deferred backend update cannot restart unrelated service {label}"
        );
        // The boot-time daemon is KeepAlive. Publishing bin/oored is the only
        // restart action needed, and avoids an interactive sudo dependency.
        // After a failed rollback the only executable left may be the
        // quiesced candidate, which is still better than no daemon at all.
        if let Some(candidate) = self.quiesced_candidate.as_ref()
            && candidate.restore()?
        {
            eprintln!(
                "Restarting the daemon on the candidate release because the rollback did not restore the previous one."
            );
        }
        Ok(())
    }

    async fn activate_runner_service(
        &mut self,
        label: &str,
        previous_pid: Option<u32>,
        ack: Option<&RunnerUpdateAck>,
        activation: ReleaseActivation,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            label == RUNNER_SERVICE_LABEL,
            "deferred backend update cannot restart unrelated service {label}"
        );
        let (plist, system) = managed_service_plist(label)?;
        anyhow::ensure!(
            system,
            "deferred updates require the boot-time runner service"
        );
        let spec = managed_runner_service_spec(&plist)?;
        if let Some(spec) = &spec {
            clear_runner_service_acknowledgement(&spec.acknowledgement)?;
        }
        let started_at = now_epoch_secs();
        let mut stable = 0;
        let mut readiness_error = None;
        for _ in 0..RUNNER_AUTHENTICATED_START_POLLS {
            let pid = update_supervisor::launchd_pid(&format!("system/{RUNNER_SERVICE_LABEL}"))?;
            let handed_off = pid.is_some() && previous_pid.is_none_or(|old| pid != Some(old));
            let authenticated = match (spec.as_ref(), pid) {
                (Some(spec), Some(pid)) if handed_off => {
                    match verify_managed_runner_service(spec, pid, Some(started_at)) {
                        Ok(()) => true,
                        Err(error) => {
                            readiness_error = Some(error.to_string());
                            false
                        }
                    }
                }
                (None, _) => handed_off,
                _ => false,
            };
            if authenticated {
                stable += 1;
                if stable >= RUNNER_STABLE_RUNNING_POLLS {
                    break;
                }
            } else {
                stable = 0;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        if stable < RUNNER_STABLE_RUNNING_POLLS {
            if let Some(error) = readiness_error {
                anyhow::bail!(
                    "managed runner handed off but did not authenticate from the launchd process: {error}"
                );
            }
            anyhow::bail!("managed runner did not hand off to a new process");
        }
        if let Some(ack) = ack {
            ack.wait_for(activation).await?;
        }
        Ok(())
    }

    fn stop_managed_service(&mut self, label: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            label == DAEMON_SERVICE_LABEL,
            "deferred backend update cannot stop unrelated service {label}"
        );
        let candidate =
            update_supervisor::quiesce_candidate_daemon(self.install_root, self.transaction_dir)?;
        self.quiesced_candidate = Some(candidate);
        Ok(())
    }

    async fn restart_unmanaged_daemon(&mut self, _listen: &str) -> anyhow::Result<()> {
        anyhow::bail!("deferred backend updates require the managed daemon service")
    }

    async fn stop_unmanaged_daemon(&mut self, _listen: &str) -> anyhow::Result<()> {
        anyhow::bail!("deferred backend updates require the managed daemon service")
    }

    async fn wait_for_endpoint(&mut self, url: &str, name: &str) -> anyhow::Result<()> {
        wait_for_endpoint(self.client, url, name).await
    }

    async fn wait_for_daemon_release(
        &mut self,
        ready_url: &str,
        expected_package_version: &str,
        name: &str,
    ) -> anyhow::Result<()> {
        wait_for_daemon_release(self.client, ready_url, expected_package_version, name).await
    }
}

impl UpdateServicePlan {
    async fn activate_dependencies<C: UpdateServiceControl>(
        &self,
        control: &mut C,
        activation: ReleaseActivation,
    ) -> anyhow::Result<()> {
        if self.daemon_is_managed && !self.defer_daemon_restart {
            println!("Restarting the backend and verifying its release...");
            control.restart_managed_service(DAEMON_SERVICE_LABEL)?;
        } else if let Some(listen) = &self.unmanaged_daemon_listen {
            control.restart_unmanaged_daemon(listen).await?;
        }
        if (self.daemon_is_managed || self.daemon_should_be_running) && !self.defer_daemon_restart {
            let name = if activation == ReleaseActivation::Restored {
                "rollback oored"
            } else {
                "oored"
            };
            let expected_version = if activation == ReleaseActivation::Restored {
                &self.restored_version
            } else {
                &self.installed_version
            };
            control
                .wait_for_daemon_release(&self.daemon_ready_url, expected_version, name)
                .await?;
        }
        if self.web_is_managed {
            control.restart_managed_service(WEB_SERVICE_LABEL)?;
        } else if self.web_was_running && activation == ReleaseActivation::Installed {
            anyhow::bail!(
                "refusing to replace an unmanaged local web process; install it as the launchd service first"
            );
        }
        if let Some(url) = &self.web_ready_url {
            let name = if activation == ReleaseActivation::Restored {
                "rollback oore-web"
            } else {
                "oore-web"
            };
            control.wait_for_endpoint(url, name).await?;
        }
        Ok(())
    }

    fn restore_daemon_intent<C: UpdateServiceControl>(
        &self,
        control: &mut C,
    ) -> anyhow::Result<()> {
        if self.daemon_is_managed
            && !self.daemon_should_be_running
            && !self.defer_daemon_restart
            && !self.install_managed_runner
            && self.runner_service.is_none()
        {
            control.stop_managed_service(DAEMON_SERVICE_LABEL)?;
        }
        Ok(())
    }

    async fn activate_runner<C: UpdateServiceControl>(
        &self,
        control: &mut C,
        activation: ReleaseActivation,
    ) -> anyhow::Result<()> {
        if self.install_managed_runner && activation == ReleaseActivation::Installed {
            println!("Migrating the runner to its boot-time service...");
            let database = self
                .managed_runner_database
                .as_deref()
                .context("managed runner migration is missing the database path")?;
            control
                .install_managed_runner(
                    database,
                    &self.runner_installed_version,
                    &self.runner_restored_version,
                )
                .await?;
            return Ok(());
        }
        if let Some(label) = self.runner_service {
            let previous_pid = (activation == ReleaseActivation::Installed)
                .then_some(self.runner_previous_pid)
                .flatten();
            control
                .activate_runner_service(label, previous_pid, self.runner_ack.as_ref(), activation)
                .await?;
        }
        Ok(())
    }

    fn remove_new_managed_runner<C: UpdateServiceControl>(
        &self,
        control: &mut C,
    ) -> anyhow::Result<()> {
        if self.install_managed_runner && !self.managed_runner_service_existed {
            control.remove_managed_runner(!self.managed_runner_config_existed)?;
        }
        Ok(())
    }

    async fn stop_daemon_for_database_restore<C: UpdateServiceControl>(
        &self,
        control: &mut C,
    ) -> anyhow::Result<()> {
        if self.daemon_is_managed && !self.defer_daemon_restart {
            control.stop_managed_service(DAEMON_SERVICE_LABEL)?;
        } else if let Some(listen) = &self.unmanaged_daemon_listen {
            control.stop_unmanaged_daemon(listen).await?;
        }
        Ok(())
    }
}

async fn resume_daemon_for_planned_barrier<C: UpdateServiceControl>(
    install_root: &Path,
    barrier: &RunnerClaimBarrier,
    services: &UpdateServicePlan,
    control: &mut C,
) -> anyhow::Result<()> {
    if !barrier.planned_daemon_outage_active() {
        return Ok(());
    }
    anyhow::ensure!(
        services.daemon_is_managed && !services.defer_daemon_restart,
        "the runner claim barrier cannot resume without the managed daemon"
    );
    control.restart_managed_service(DAEMON_SERVICE_LABEL)?;
    let expected_version = compiled_daemon_package_version(&install_root.join("bin/oored"))?;
    control
        .wait_for_daemon_release(
            &services.daemon_ready_url,
            &expected_version,
            "recovered oored",
        )
        .await?;
    barrier.end_planned_daemon_outage().await
}

#[allow(clippy::too_many_arguments)]
async fn install_release_with_rollback<C: UpdateServiceControl>(
    staged_release: &Path,
    install_root: &Path,
    previous_release: &Path,
    channel: ReleaseChannel,
    repo: &str,
    daemon_migration: Option<&ManagedDaemonServiceMigration>,
    runner_migration: Option<&dyn ManagedRunnerServiceRollback>,
    claim_barrier: Option<&RunnerClaimBarrier>,
    services: &UpdateServicePlan,
    rollback_state: Option<&UpdateRollbackState>,
    control: &mut C,
) -> anyhow::Result<()> {
    if let Some(barrier) = claim_barrier {
        barrier.ensure_healthy().await?;
    }
    ensure_runner_release_marker(install_root)?;
    let update_result: anyhow::Result<()> = async {
        install_staged_release(
            staged_release,
            install_root,
            channel,
            repo,
            services
                .profile_update
                .as_ref()
                .map(|profile| profile.payload),
        )?;
        if let Some(profile) = services.profile_update.as_ref() {
            publish_profile_update(install_root, profile)?;
        }
        if let Some(migration) = daemon_migration {
            migration.apply()?;
        }
        services
            .activate_dependencies(control, ReleaseActivation::Installed)
            .await?;
        if let Some(barrier) = claim_barrier
            && barrier.planned_daemon_outage_active()
        {
            barrier.end_planned_daemon_outage().await?;
        }
        publish_runner_release_marker(install_root)?;
        services
            .activate_runner(control, ReleaseActivation::Installed)
            .await?;
        if let Some(migration) = daemon_migration {
            migration.commit()?;
        }
        Ok(())
    }
    .await;
    if let Err(error) = update_result {
        eprintln!("Update failed: {error:#}");
        eprintln!("Restoring the previous release...");
        let rollback: anyhow::Result<()> = async {
            if let Some(snapshot) = runner_migration {
                snapshot.stop_and_restore_files()?;
            } else {
                services.remove_new_managed_runner(control)?;
            }
            if (services.daemon_is_managed || services.daemon_should_be_running)
                && !services.defer_daemon_restart
            {
                if let Some(barrier) = claim_barrier {
                    barrier.begin_planned_daemon_outage().await?;
                }
                services.stop_daemon_for_database_restore(control).await?;
                if let Some(state) = rollback_state {
                    restore_bounded_update_backup(&state.backup, &state.database, &state.key)
                        .await?;
                }
            }
            if let Some(migration) = daemon_migration {
                migration.rollback()?;
            }
            restore_release_snapshot(install_root, previous_release)?;
            restore_runner_release_marker(install_root)?;
            services
                .activate_dependencies(control, ReleaseActivation::Restored)
                .await?;
            if let Some(barrier) = claim_barrier
                && barrier.planned_daemon_outage_active()
            {
                barrier.end_planned_daemon_outage().await?;
            }
            if let Some(snapshot) = runner_migration {
                snapshot.start()?;
                if let Some(ack) = services.runner_ack.as_ref() {
                    ack.wait_for(ReleaseActivation::Restored).await?;
                }
            } else {
                services
                    .activate_runner(control, ReleaseActivation::Restored)
                    .await?;
            }
            if claim_barrier.is_none() {
                services.restore_daemon_intent(control)?;
            }
            Ok(())
        }
        .await;
        if let Err(rollback_error) = rollback {
            let recovery = match claim_barrier {
                Some(barrier) => {
                    resume_daemon_for_planned_barrier(install_root, barrier, services, control)
                        .await
                }
                None => Ok(()),
            };
            return match recovery {
                Ok(()) => Err(error.context(format!("rollback also failed: {rollback_error}"))),
                Err(recovery_error) => Err(error.context(format!(
                    "rollback also failed: {rollback_error}; restarting the daemon for the runner claim barrier also failed: {recovery_error}"
                ))),
            };
        }
        return Err(error);
    }
    let barrier_release = match claim_barrier {
        Some(barrier) => barrier.release().await,
        None => Ok(()),
    };
    let daemon_intent = if claim_barrier.is_none() {
        services.restore_daemon_intent(control)
    } else {
        Ok(())
    };
    match (barrier_release, daemon_intent) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(error)) => Err(error),
        (Err(error), Ok(())) => Err(error.context(
            "the update remains installed, but the runner claim barrier release state is uncertain",
        )),
        (Err(error), Err(intent_error)) => Err(error.context(format!(
            "the update remains installed, but the runner claim barrier release state is uncertain; restoring the prior daemon state also failed: {intent_error:#}"
        ))),
    }
}

struct PreparedUpdateRelease {
    path: Option<PathBuf>,
    version: semver::Version,
    package_version: Option<String>,
    label: String,
    provenance: Option<PreparedUpdateProvenance>,
    _temporary: Option<tempfile::TempDir>,
}

#[derive(Clone)]
struct PreparedUpdateProvenance {
    archive: String,
    archive_sha256: String,
    manifest_sha256: String,
}

fn require_unsigned_staged_update_acceptance(version: &str) -> anyhow::Result<()> {
    let accepted = version.ends_with("-dev")
        && std::env::var("OORE_ALLOW_UNSIGNED_LOCAL_RELEASE").as_deref() == Ok("true")
        && std::env::var("OORE_ALLOW_DEV_RELEASE_TAG_FOR_LOCAL_ACCEPTANCE").as_deref()
            == Ok("true")
        && std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true");
    anyhow::ensure!(
        accepted,
        "--staged-release is an unsigned local acceptance path; it requires a -dev release, OORE_ALLOW_UNSIGNED_LOCAL_RELEASE=true, and OORE_ALLOW_DEV_RELEASE_TAG_FOR_LOCAL_ACCEPTANCE=true, and it is disabled in GitHub Actions"
    );
    Ok(())
}

fn compiled_daemon_package_version(executable: &Path) -> anyhow::Result<String> {
    let package_output = std::process::Command::new(executable)
        .arg("package-version")
        .output()
        .with_context(|| {
            format!(
                "failed to inspect candidate daemon {}",
                executable.display()
            )
        })?;
    let output = if package_output.status.success() {
        package_output
    } else {
        let empty_install_root =
            tempfile::tempdir().context("failed to prepare legacy daemon package version probe")?;
        let legacy_output = std::process::Command::new(executable)
            .arg("version")
            .env("OORE_INSTALL_ROOT", empty_install_root.path())
            .output()
            .with_context(|| format!("failed to inspect legacy daemon {}", executable.display()))?;
        anyhow::ensure!(
            legacy_output.status.success(),
            "daemon did not report its compiled package version"
        );
        legacy_output
    };
    let version = String::from_utf8(output.stdout)?.trim().to_string();
    anyhow::ensure!(
        !version.is_empty(),
        "candidate daemon package version is empty"
    );
    Ok(version)
}

fn validate_prepared_release(path: &Path) -> anyhow::Result<()> {
    for relative in ["bin/oore", "bin/oored", "VERSION"] {
        anyhow::ensure!(
            path.join(relative).is_file(),
            "prepared release is missing {relative}"
        );
    }
    Ok(())
}

async fn prepare_update_release(
    client: &reqwest::Client,
    repo: &str,
    channel: ReleaseChannel,
    staged_release: Option<&Path>,
    check_only: bool,
) -> anyhow::Result<PreparedUpdateRelease> {
    if let Some(path) = staged_release {
        anyhow::ensure!(!check_only, "--check cannot use --staged-release");
        let path = fs::canonicalize(path)
            .with_context(|| format!("failed to resolve staged release {}", path.display()))?;
        validate_prepared_release(&path)?;
        let raw =
            read_trimmed_file(&path.join("VERSION")).context("staged release VERSION is empty")?;
        let version = parse_semver_loose(&raw).context("invalid staged release VERSION")?;
        require_unsigned_staged_update_acceptance(&raw)?;
        let package_version = compiled_daemon_package_version(&path.join("bin/oored"))?;
        return Ok(PreparedUpdateRelease {
            path: Some(path),
            package_version: Some(package_version),
            label: format!("staged {version}"),
            version,
            provenance: None,
            _temporary: None,
        });
    }

    let release = fetch_latest_release(client, repo, channel).await?;
    let version = parse_semver_loose(&release.version)
        .with_context(|| format!("invalid version in release tag: {}", release.tag))?;
    let arch = release_arch()?;
    let archive_filename = format!("oore_{}_darwin_{arch}.tar.gz", release.version);
    let checksums_filename = format!("oore_{}_checksums.txt", release.version);
    let archive_url = find_asset_url(&release, &archive_filename);
    let checksums_url = find_asset_url(&release, &checksums_filename);
    let checksums_bytes = download_verified_release_metadata(
        client,
        &checksums_url,
        RELEASE_MANIFEST_NAMESPACE,
        "release checksum manifest",
    )
    .await?;
    let checksums = std::str::from_utf8(&checksums_bytes)
        .context("release checksum manifest is not valid UTF-8")?;
    let expected_hash = parse_checksum(checksums, &archive_filename)?;
    let manifest_sha256 = hex::encode(Sha256::digest(&checksums_bytes));
    anyhow::ensure!(
        expected_hash.len() == 64
            && expected_hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "release archive checksum must contain 64 lowercase hexadecimal characters"
    );
    if check_only {
        return Ok(PreparedUpdateRelease {
            path: None,
            version,
            package_version: None,
            label: release.tag,
            provenance: Some(PreparedUpdateProvenance {
                archive: archive_filename,
                archive_sha256: expected_hash,
                manifest_sha256,
            }),
            _temporary: None,
        });
    }

    println!("Downloading {archive_filename}...");
    let archive_bytes = download_release_bytes(
        client,
        &archive_url,
        "release archive",
        MAX_RELEASE_ARCHIVE_BYTES,
    )
    .await?;
    let actual_hash = hex::encode(Sha256::digest(&archive_bytes));
    anyhow::ensure!(
        actual_hash == expected_hash,
        "Checksum mismatch!\n  Expected: {expected_hash}\n  Actual:   {actual_hash}"
    );
    println!("Checksum verified (SHA-256).");

    let temporary = tempfile::tempdir().context("failed to create temporary directory")?;
    let decoder = flate2::read::GzDecoder::new(&archive_bytes[..]);
    tar::Archive::new(decoder)
        .unpack(temporary.path())
        .context("failed to extract archive")?;
    validate_prepared_release(temporary.path())?;
    let package_version = compiled_daemon_package_version(&temporary.path().join("bin/oored"))?;
    Ok(PreparedUpdateRelease {
        path: Some(temporary.path().to_path_buf()),
        version,
        package_version: Some(package_version),
        label: release.tag,
        provenance: Some(PreparedUpdateProvenance {
            archive: archive_filename,
            archive_sha256: expected_hash,
            manifest_sha256,
        }),
        _temporary: Some(temporary),
    })
}

fn require_deferred_path(value: Option<&PathBuf>, name: &str) -> anyhow::Result<PathBuf> {
    let path = value
        .cloned()
        .with_context(|| format!("deferred update is missing --{name}"))?;
    anyhow::ensure!(path.is_absolute(), "--{name} must be an absolute path");
    Ok(path)
}

fn require_loopback_daemon_url(raw: Option<&str>) -> anyhow::Result<String> {
    let raw = raw.context("deferred update is missing --deferred-daemon-url")?;
    let url = url::Url::parse(raw).context("invalid deferred daemon URL")?;
    anyhow::ensure!(url.scheme() == "http", "deferred daemon URL must use http");
    let host = url.host_str().context("deferred daemon URL has no host")?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    anyhow::ensure!(loopback, "deferred daemon URL must use a loopback host");
    anyhow::ensure!(
        url.path() == "/" && url.query().is_none() && url.fragment().is_none(),
        "deferred daemon URL must not contain a path, query, or fragment"
    );
    Ok(raw.trim_end_matches('/').to_string())
}

fn install_path_exists(path: &Path) -> anyhow::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("failed to inspect {}", path.display())),
    }
}

fn installed_profile_name(profile: install_manifest::InstallProfile) -> &'static str {
    match profile {
        install_manifest::InstallProfile::Complete => "complete",
        install_manifest::InstallProfile::ControlPlane => "control-plane",
        install_manifest::InstallProfile::Runner => "runner",
        install_manifest::InstallProfile::WebNode => "web-node",
        install_manifest::InstallProfile::CliOnly => "cli-only",
    }
}

fn shell_word(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn load_backend_update_manifest(
    install_root: &Path,
) -> anyhow::Result<Option<install_manifest::InstallManifest>> {
    let manifest_path = install_root.join("install-manifest.json");
    let manifest_exists = install_path_exists(&manifest_path)?;
    let bootstrap_marker_exists = install_path_exists(&install_root.join("BOOTSTRAP_ARCHIVE"))?
        || install_path_exists(&install_root.join("BOOTSTRAP_SHA256"))?
        || install_path_exists(&install_root.join("BOOTSTRAP_MANIFEST_SHA256"))?;
    if !manifest_exists && !bootstrap_marker_exists {
        return Ok(None);
    }

    let Some(manifest) = manifest_exists
        .then(|| install_manifest::InstallManifest::load(&manifest_path))
        .transpose()
        .with_context(|| {
            format!(
                "cannot identify the current device role from {}; repair this manifest before updating",
                manifest_path.display()
            )
        })?
    else {
        anyhow::bail!(
            "this bootstrap-only installation has no backend profile; finish `oore install` before using backend updates"
        );
    };
    anyhow::ensure!(
        matches!(
            manifest.profile,
            install_manifest::InstallProfile::Complete
                | install_manifest::InstallProfile::ControlPlane
        ),
        "the {} profile has no managed backend to update with `oore update`",
        installed_profile_name(manifest.profile)
    );
    anyhow::ensure!(
        manifest.lifecycle.state == install_manifest::InstallState::Ready,
        "finish `oore setup` before updating this backend profile"
    );
    let bootstrap = installer::read_bootstrap_release(install_root)
        .context("cannot validate the installed bootstrap release")?;
    anyhow::ensure!(
        release_lineage_matches(&manifest.release, &bootstrap),
        "the installation profile and bootstrap metadata record different releases"
    );
    Ok(Some(manifest))
}

#[derive(Clone, Copy)]
struct UpdatePayloadSelection {
    control_plane: bool,
    web: bool,
}

struct ProfileUpdatePlan {
    manifest: install_manifest::InstallManifest,
    release: install_manifest::InstallRelease,
    manifest_sha256: String,
    payload: UpdatePayloadSelection,
}

fn prepare_profile_update_plan(
    manifest: Option<install_manifest::InstallManifest>,
    prepared: &PreparedUpdateRelease,
    channel: ReleaseChannel,
    repository: &str,
    include_web: bool,
    switch_channel: bool,
) -> anyhow::Result<Option<ProfileUpdatePlan>> {
    let Some(manifest) = manifest else {
        return Ok(None);
    };
    let provenance = prepared.provenance.as_ref().context(
        "profile updates require a signed release archive; --staged-release is unavailable for profile installations",
    )?;
    let installed_version = parse_semver_loose(&manifest.release.version)
        .context("the installation profile records an invalid release version")?;
    anyhow::ensure!(
        prepared.version >= installed_version,
        "profile updates cannot downgrade from {} to {}",
        installed_version,
        prepared.version
    );
    anyhow::ensure!(
        manifest.release.repository == repository,
        "profile updates must stay on the recorded {} repository",
        manifest.release.repository
    );
    let prerelease_to_stable = matches!(manifest.release.channel.as_str(), "alpha" | "beta")
        && channel == ReleaseChannel::Stable
        && !installed_version.pre.is_empty()
        && prepared.version.pre.is_empty()
        && installed_version.major == prepared.version.major
        && installed_version.minor == prepared.version.minor
        && installed_version.patch == prepared.version.patch;
    anyhow::ensure!(
        manifest.release.channel == channel.as_str() || prerelease_to_stable || switch_channel,
        "profile updates stay on the recorded {} channel; pass --switch-channel to move this installation to the {} channel",
        manifest.release.channel,
        channel.as_str()
    );
    let release = install_manifest::InstallRelease::new(
        prepared.version.to_string(),
        channel.as_str().to_string(),
        repository.to_string(),
        provenance.archive.clone(),
        provenance.archive_sha256.clone(),
    )?;
    let payload = UpdatePayloadSelection {
        control_plane: true,
        web: include_web && manifest.profile == install_manifest::InstallProfile::Complete,
    };
    Ok(Some(ProfileUpdatePlan {
        manifest,
        release,
        manifest_sha256: provenance.manifest_sha256.clone(),
        payload,
    }))
}

fn publish_profile_update(install_root: &Path, plan: &ProfileUpdatePlan) -> anyhow::Result<()> {
    installer::write_metadata_atomic(
        &install_root.join("BOOTSTRAP_ARCHIVE"),
        &plan.release.archive,
    )?;
    installer::write_metadata_atomic(&install_root.join("BOOTSTRAP_SHA256"), &plan.release.sha256)?;
    installer::write_metadata_atomic(
        &install_root.join("BOOTSTRAP_MANIFEST_SHA256"),
        &plan.manifest_sha256,
    )?;
    let mut manifest = plan.manifest.clone();
    manifest.release = plan.release.clone();
    manifest.write_atomic(&install_root.join("install-manifest.json"))
}

async fn run_deferred_update(args: &UpdateArgs, status_path: &Path) -> anyhow::Result<()> {
    let install_root = resolve_install_root()?;
    let profile_manifest = load_backend_update_manifest(&install_root)?;
    let expected_status = install_root.join(".runtime-update-status.json");
    anyhow::ensure!(
        status_path == expected_status,
        "deferred status file must be {}",
        expected_status.display()
    );
    anyhow::ensure!(
        !args.ensure_managed_runner,
        "deferred updates cannot perform first-time service migration"
    );
    let parent_pid = args
        .deferred_parent_pid
        .context("deferred update is missing --deferred-parent-pid")?;
    anyhow::ensure!(parent_pid > 1, "invalid deferred parent pid");
    let database = require_deferred_path(args.deferred_state_file.as_ref(), "deferred-state-file")?;
    let key = require_deferred_path(args.deferred_key_file.as_ref(), "deferred-key-file")?;
    anyhow::ensure!(database.is_file(), "deferred state database does not exist");
    anyhow::ensure!(key.is_file(), "deferred encryption key does not exist");
    let daemon_url = require_loopback_daemon_url(args.deferred_daemon_url.as_deref())?;
    let daemon_ready_url = endpoint_url(&daemon_url, "/readyz");

    let current_raw = read_trimmed_file(&install_root.join("VERSION"))
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let current = parse_semver_loose(&current_raw).context("failed to parse current version")?;
    let repo = args
        .repo
        .clone()
        .map(|repo| normalize_github_repo(&repo))
        .or_else(|| read_installed_repo(&install_root))
        .unwrap_or_else(|| DEFAULT_GITHUB_REPO.to_string());
    let channel = if let Some(raw) = &args.channel {
        ReleaseChannel::parse(raw)?
    } else if let Some(channel) = read_installed_channel(&install_root) {
        channel
    } else {
        infer_channel_from_version(&current)
    };
    let client = http_client()?;
    let prepared = prepare_update_release(
        &client,
        &repo,
        channel,
        args.staged_release.as_deref(),
        false,
    )
    .await?;
    if current >= prepared.version && !args.force {
        return Ok(());
    }
    anyhow::ensure!(!args.check, "deferred update cannot use --check");
    let profile_update = prepare_profile_update_plan(
        profile_manifest,
        &prepared,
        channel,
        &repo,
        false,
        args.switch_channel,
    )?;

    let managed_runner_service = managed_runner_update_service(&install_root)?;
    if profile_update.as_ref().is_some_and(|profile| {
        profile.manifest.profile == install_manifest::InstallProfile::Complete
    }) {
        anyhow::ensure!(
            managed_runner_service.is_some(),
            "this Complete installation needs a one-time managed runner repair; run `oore setup` from Terminal before updating from the web UI"
        );
    }
    if let Some(service) = managed_runner_service {
        let (runner_plist, runner_is_system) = managed_service_plist(service)?;
        anyhow::ensure!(
            runner_is_system && runner_plist.is_file(),
            "web updates require the boot-time managed runner service"
        );
        anyhow::ensure!(
            !managed_runner_service_requires_repair(&runner_plist)?,
            "this runner service needs a one-time repair; run `oore setup` from Terminal before updating from the web UI"
        );
    }
    let restored_package_version = daemon_package_version(&client, &daemon_url).await?;
    wait_for_daemon_release(
        &client,
        &daemon_ready_url,
        &restored_package_version,
        "existing oored",
    )
    .await?;
    let runner_previous_pid = managed_runner_service
        .map(|_| managed_runner_process_pid())
        .transpose()?
        .flatten();
    let runner_ack = match managed_runner_service {
        Some(_) => Some(
            prepare_runner_update_ack(
                &daemon_url,
                Some(
                    database
                        .to_str()
                        .context("deferred state database path is not valid UTF-8")?,
                ),
                &prepared.version.to_string(),
                &current.to_string(),
            )
            .await?,
        ),
        None => None,
    };

    fs::create_dir_all(&install_root)?;
    let update_stage = tempfile::Builder::new()
        .prefix(".supervised-update-")
        .tempdir_in(&install_root)
        .context("failed to create supervised update transaction")?;
    let staged_release = update_stage.path().join("release");
    copy_dir_recursive(
        prepared
            .path
            .as_deref()
            .context("verified update release was not prepared")?,
        &staged_release,
    )?;
    let previous_release = update_stage.path().join("previous");
    copy_release_snapshot(&install_root, &previous_release)?;

    let barrier = acquire_runner_claim_barrier(
        &daemon_url,
        Some(
            database
                .to_str()
                .context("deferred state database path is not valid UTF-8")?,
        ),
    )
    .await?;
    if let Err(error) = barrier.wait_for_all_work().await {
        barrier.release().await?;
        return Err(error);
    }
    update_supervisor::write_status(
        status_path,
        oore_contract::RuntimeUpdatePhase::Restarting,
        None,
    )?;
    barrier.begin_planned_daemon_outage().await?;
    let mut held = match update_supervisor::HeldDaemonExecutable::hold_parent(
        &install_root,
        update_stage.path(),
        parent_pid,
    ) {
        Ok(held) => held,
        Err(error) => {
            restart_launchd_service(DAEMON_SERVICE_LABEL)?;
            wait_for_daemon_release(
                &client,
                &daemon_ready_url,
                &restored_package_version,
                "existing oored",
            )
            .await?;
            barrier.end_planned_daemon_outage_and_release().await?;
            return Err(error);
        }
    };

    let backup_dir = install_root.join("backups");
    let backup_path = backup_dir.join(format!(
        "pre-update-{}-{}.tar.gz",
        current,
        now_epoch_secs()
    ));
    if let Err(error) = create_bounded_update_backup(&database, &key, &backup_path).await {
        held.restore()?;
        wait_for_daemon_release(
            &client,
            &daemon_ready_url,
            &restored_package_version,
            "existing oored",
        )
        .await?;
        barrier.end_planned_daemon_outage_and_release().await?;
        return Err(error);
    }
    held.release_to_transaction();

    let rollback = UpdateRollbackState {
        backup: backup_path,
        database: database.clone(),
        key,
    };
    let services = UpdateServicePlan {
        profile_update,
        defer_daemon_restart: false,
        daemon_is_managed: true,
        unmanaged_daemon_listen: None,
        daemon_should_be_running: true,
        daemon_ready_url,
        installed_version: prepared
            .package_version
            .context("verified update daemon version was not prepared")?,
        restored_version: restored_package_version,
        runner_service: managed_runner_service,
        runner_previous_pid,
        runner_ack,
        runner_installed_version: prepared.version.to_string(),
        runner_restored_version: current.to_string(),
        install_managed_runner: false,
        managed_runner_service_existed: managed_runner_service.is_some(),
        managed_runner_config_existed: managed_runner_service.is_some(),
        managed_runner_database: None,
        web_is_managed: false,
        web_was_running: false,
        web_ready_url: None,
    };
    let mut control = DeferredUpdateServiceControl {
        install_root: &install_root,
        transaction_dir: update_stage.path(),
        client: &client,
        quiesced_candidate: None,
    };
    let result = install_release_with_rollback(
        &staged_release,
        &install_root,
        &previous_release,
        channel,
        &repo,
        None,
        None,
        Some(&barrier),
        &services,
        Some(&rollback),
        &mut control,
    )
    .await;
    if let Err(error) = result {
        return match barrier.release().await {
            Ok(()) => Err(error),
            Err(release_error) => Err(error.context(format!(
                "releasing the maintenance barrier also failed: {release_error}"
            ))),
        };
    }
    Ok(())
}

async fn handle_deferred_update(args: UpdateArgs) -> anyhow::Result<()> {
    let install_root = resolve_install_root()?;
    let status_path =
        require_deferred_path(args.deferred_status_file.as_ref(), "deferred-status-file")?;
    update_supervisor::record_owned_result(&status_path, async {
        let _lifecycle_lock = install_lock::InstallLock::acquire(&install_root)?;
        run_deferred_update(&args, &status_path).await
    })
    .await
}

async fn handle_update_supervisor(args: UpdateSupervisorArgs) -> anyhow::Result<()> {
    supervise_update_request(&resolve_install_root()?, &args.request_file).await
}

async fn supervise_update_request(install_root: &Path, request_file: &Path) -> anyhow::Result<()> {
    let status_path = install_root.join(".runtime-update-status.json");
    let expected = install_root
        .join("run/runtime-update-queue")
        .join("request.json");
    // A launch with nothing queued has no attempt to report on.
    anyhow::ensure!(
        install_path_exists(&expected)?,
        "no runtime update request is queued at {}",
        expected.display()
    );
    let claimed = claim_update_request(install_root, request_file, &expected).and_then(|active| {
        match read_claimed_update_request(&active, &status_path) {
            Ok(request) => Ok((active, request)),
            Err(error) => {
                let _ = fs::remove_file(&active);
                Err(error)
            }
        }
    });
    let (active, request) = match claimed {
        Ok(claimed) => claimed,
        Err(error) => {
            // Drop the unusable request so the next click can queue a fresh attempt.
            let _ = fs::remove_file(&expected);
            update_supervisor::record_failure(&status_path, &error)?;
            return Err(error);
        }
    };
    let result = handle_deferred_update(UpdateArgs {
        check: false,
        force: false,
        channel: None,
        switch_channel: false,
        repo: None,
        staged_release: None,
        ensure_managed_runner: false,
        deferred_parent_pid: Some(request.parent_pid),
        deferred_state_file: Some(request.database),
        deferred_key_file: Some(request.key),
        deferred_daemon_url: Some(request.daemon_url),
        deferred_status_file: Some(request.status),
    })
    .await;
    let cleanup = fs::remove_file(&active).context("failed to clear runtime update request");
    match (result, cleanup) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn claim_update_request(
    install_root: &Path,
    request_file: &Path,
    expected: &Path,
) -> anyhow::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    anyhow::ensure!(
        request_file == expected,
        "runtime update request must be {}",
        expected.display()
    );
    let metadata = fs::symlink_metadata(request_file)
        .with_context(|| format!("failed to inspect {}", request_file.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file(),
        "runtime update request is not a file"
    );
    anyhow::ensure!(
        metadata.uid() == current_effective_uid(),
        "runtime update request is owned by another user"
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o077 == 0,
        "runtime update request permissions are too broad"
    );

    let active = install_root
        .join("run")
        .join(format!("runtime-update-active-{}.json", std::process::id()));
    fs::rename(request_file, &active).context("failed to claim runtime update request")?;
    Ok(active)
}

fn read_claimed_update_request(
    active: &Path,
    status_path: &Path,
) -> anyhow::Result<DeferredRuntimeUpdateRequest> {
    let request: DeferredRuntimeUpdateRequest =
        serde_json::from_slice(&fs::read(active).context("failed to read runtime update request")?)
            .context("invalid runtime update request")?;
    anyhow::ensure!(
        request.status == status_path,
        "runtime update status file must be {}",
        status_path.display()
    );
    Ok(request)
}

async fn handle_update(args: UpdateArgs) -> anyhow::Result<()> {
    let install_root = resolve_install_root()?;
    let defer_daemon_restart = std::env::var_os("OORE_UPDATE_DEFER_DAEMON_RESTART").is_some();
    if defer_daemon_restart {
        return handle_deferred_update(args).await;
    }
    let _lifecycle_lock = if args.check {
        None
    } else {
        Some(install_lock::InstallLock::acquire(&install_root)?)
    };
    let profile_manifest = if args.check {
        None
    } else {
        load_backend_update_manifest(&install_root)?
    };
    let current_str = read_trimmed_file(&install_root.join("VERSION"))
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let current = parse_semver_loose(&current_str).context("failed to parse current version")?;

    let repo = args
        .repo
        .clone()
        .map(|repo| normalize_github_repo(&repo))
        .or_else(|| read_installed_repo(&install_root))
        .unwrap_or_else(|| DEFAULT_GITHUB_REPO.to_string());

    let installed_channel = read_installed_channel(&install_root)
        .unwrap_or_else(|| infer_channel_from_version(&current));
    let channel = match &args.channel {
        Some(raw) => ReleaseChannel::parse(raw)?,
        None => installed_channel,
    };

    let client = http_client()?;
    let prepared = prepare_update_release(
        &client,
        &repo,
        channel,
        args.staged_release.as_deref(),
        args.check,
    )
    .await?;
    let latest = &prepared.version;
    println!("Channel:         {}", channel.as_str());
    println!("GitHub repo:     {repo}");
    println!("Current version: {current}");
    println!("Latest version:  {latest} ({})", prepared.label);
    if channel != installed_channel {
        anyhow::ensure!(
            current <= *latest,
            "cannot switch from the {} channel to the {} channel yet: its latest release {latest} is older than the installed {current}, and updates never downgrade; wait for a newer {} release",
            installed_channel.as_str(),
            channel.as_str(),
            channel.as_str()
        );
        println!(
            "Switching channel: {} -> {}",
            installed_channel.as_str(),
            channel.as_str()
        );
    }

    if current >= *latest && !args.force {
        println!("Already up to date.");
        return Ok(());
    }

    if current < *latest {
        println!("Update available: {current} -> {latest}");
    } else {
        println!("Reinstalling version {latest} (--force).");
    }
    if args.check {
        return Ok(());
    }
    let profile_update = prepare_profile_update_plan(
        profile_manifest,
        &prepared,
        channel,
        &repo,
        true,
        args.switch_channel,
    )?;
    fs::create_dir_all(&install_root)
        .with_context(|| format!("failed to create install root {}", install_root.display()))?;
    let update_stage = tempfile::Builder::new()
        .prefix(".update-")
        .tempdir_in(&install_root)
        .context("failed to create update staging directory")?;
    let staged_release = update_stage.path().join("release");
    copy_dir_recursive(
        prepared
            .path
            .as_deref()
            .context("verified update release was not prepared")?,
        &staged_release,
    )?;
    let previous_release = update_stage.path().join("previous");
    copy_release_snapshot(&install_root, &previous_release)?;

    let (daemon_service_plist, daemon_service_is_system) =
        managed_service_plist(DAEMON_SERVICE_LABEL)?;
    let daemon_service_is_installed = daemon_service_plist.is_file();
    let daemon_was_loaded = launchd_service_loaded(DAEMON_SERVICE_LABEL);
    let (runner_plist, runner_is_system) = managed_service_plist(RUNNER_SERVICE_LABEL)?;
    let runner_service_requires_repair = runner_plist.is_file()
        && runner_is_system
        && managed_runner_service_requires_repair(&runner_plist)?;
    if runner_service_requires_repair && !args.ensure_managed_runner {
        anyhow::bail!(
            "this runner service needs a one-time repair; rerun the verified installer for the installed channel before using ordinary updates"
        );
    }
    let (web_plist, web_is_system) = managed_service_plist(WEB_SERVICE_LABEL)?;
    let web_is_managed = launchd_service_loaded(WEB_SERVICE_LABEL);
    if let Some(profile) = profile_update.as_ref() {
        anyhow::ensure!(
            daemon_service_is_installed && daemon_service_is_system,
            "this backend service needs a one-time repair; run `oore setup` from Terminal before using ordinary updates"
        );
        if profile.manifest.profile == install_manifest::InstallProfile::Complete {
            anyhow::ensure!(
                runner_plist.is_file() && runner_is_system,
                "this runner service needs a one-time repair; run `oore setup` from Terminal before using ordinary updates"
            );
            anyhow::ensure!(
                web_plist.is_file() && web_is_system,
                "this web service needs a one-time repair; run `oore setup` from Terminal before using ordinary updates"
            );
        }
    }
    if (daemon_service_is_installed && daemon_service_is_system)
        || (runner_plist.is_file() && runner_is_system)
        || (web_is_managed && web_is_system)
        || args.ensure_managed_runner
    {
        authorize_system_service_restart()?;
    }

    let daemon_document = daemon_service_is_installed
        .then(|| plist_json(&daemon_service_plist))
        .transpose()?;
    if let Some(document) = daemon_document.as_ref() {
        validate_managed_service_executable(
            document,
            &install_root.join("bin/oored"),
            DAEMON_SERVICE_LABEL,
        )?;
    }
    let managed_daemon_arguments = daemon_document
        .as_ref()
        .map(|document| {
            document
                .get("ProgramArguments")
                .and_then(serde_json::Value::as_array)
                .context("managed daemon plist has no ProgramArguments")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .context("managed daemon ProgramArguments must be strings")
                })
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .transpose()?;
    let managed_daemon_listen = managed_daemon_arguments
        .as_deref()
        .map(listen_address_from_program_arguments)
        .transpose()?;
    let daemon_url = managed_daemon_listen
        .as_deref()
        .map(daemon_url_from_listen_address)
        .transpose()?
        .unwrap_or(resolve_daemon_url(None)?);
    let unmanaged_daemon_running =
        !daemon_service_is_installed && check_daemon_running(&client, &daemon_url).await;
    if unmanaged_daemon_running {
        anyhow::bail!(
            "refusing to update a running unmanaged oored process because its exact state paths and restart environment cannot be preserved; install the managed service first"
        );
    }
    let (database, key) = if let Some(document) = daemon_document.as_ref() {
        daemon_data_paths_from_plist(document, &resolve_data_dir()?)?
    } else {
        (resolve_db_path(None)?, resolve_key_path()?)
    };

    let legacy_runner_mode = daemon_document
        .as_ref()
        .and_then(|document| document.get("EnvironmentVariables"))
        .and_then(serde_json::Value::as_object)
        .and_then(|environment| environment.get("OORED_RUNNER_MODE"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|mode| matches!(mode, "embedded" | "hybrid"));
    let daemon_migration_required =
        daemon_service_is_installed && (!daemon_service_is_system || legacy_runner_mode);
    if daemon_migration_required && !args.ensure_managed_runner {
        anyhow::bail!(
            "this legacy managed backend needs the transactional service migration; rerun the verified installer to upgrade it safely"
        );
    }
    let daemon_migration = daemon_migration_required
        .then(|| {
            prepare_managed_daemon_service_migration(
                &install_root,
                &daemon_service_plist,
                daemon_service_is_system,
                daemon_was_loaded,
                daemon_document
                    .as_ref()
                    .context("managed daemon migration is missing its plist")?,
                update_stage.path(),
            )
        })
        .transpose()?;

    let managed_runner_service = managed_runner_update_service(&install_root)?;
    let install_managed_runner = args.ensure_managed_runner;
    let managed_runner_service_existed = managed_runner_service.is_some();
    let runner_migration = install_managed_runner
        .then(ManagedRunnerServiceSnapshot::capture)
        .transpose()?;
    let runner_update_service = (!install_managed_runner)
        .then_some(managed_runner_service)
        .flatten();
    if args.ensure_managed_runner && !daemon_service_is_installed {
        anyhow::bail!("--ensure-managed-runner requires an existing managed backend service");
    }
    let managed_runner_config_existed = managed_local_runner_config_path()?.is_file();

    let daemon_ready_url = endpoint_url(&daemon_url, "/readyz");
    let mut stopped_intent_guard = ManagedDaemonStoppedIntentGuard {
        restore_stopped: daemon_service_is_installed && !daemon_was_loaded,
    };
    if daemon_service_is_installed && !daemon_was_loaded {
        println!("The managed backend is stopped; starting it temporarily for a safe drain...");
        restart_launchd_service(DAEMON_SERVICE_LABEL)?;
        wait_for_endpoint(&client, &daemon_ready_url, "existing oored").await?;
    }
    let daemon_was_running =
        daemon_service_is_installed && check_daemon_running(&client, &daemon_url).await;
    let restored_package_version = if daemon_was_running {
        daemon_package_version(&client, &daemon_url).await?
    } else {
        compiled_daemon_package_version(&install_root.join("bin/oored"))?
    };

    if web_is_managed {
        let web_document = plist_json(&web_plist)?;
        validate_managed_service_executable(
            &web_document,
            &install_root.join("bin/oore-web"),
            WEB_SERVICE_LABEL,
        )?;
    }
    let web_ready_url = web_is_managed
        .then(|| -> anyhow::Result<String> {
            let listen = managed_service_listen_address(WEB_SERVICE_LABEL)?;
            Ok(endpoint_url(
                &url_from_listen_address(&listen)?,
                "/__oore_web_healthz",
            ))
        })
        .transpose()?;
    let web_was_running = match web_ready_url.as_deref() {
        Some(url) => endpoint_is_healthy(&client, url).await,
        None => false,
    };

    let runner_previous_pid = runner_update_service
        .map(|_| managed_runner_process_pid())
        .transpose()?
        .flatten();
    let runner_ack = match managed_runner_service {
        Some(_) => Some(
            prepare_runner_update_ack(
                &daemon_url,
                Some(
                    database
                        .to_str()
                        .context("managed state database path is not valid UTF-8")?,
                ),
                &prepared.version.to_string(),
                &current.to_string(),
            )
            .await?,
        ),
        None => None,
    };

    let barrier = acquire_runner_claim_barrier(
        &daemon_url,
        Some(
            database
                .to_str()
                .context("managed state database path is not valid UTF-8")?,
        ),
    )
    .await?;
    if daemon_service_is_installed && !daemon_was_loaded {
        stopped_intent_guard.disarm();
        println!(
            "The backend was stopped before this update. It will remain running because stopping it after the maintenance barrier opens can interrupt a remote runner claim."
        );
    }
    if let Err(error) = barrier.wait_for_all_work().await {
        barrier.release().await?;
        return Err(error);
    }

    if daemon_service_is_installed {
        barrier.begin_planned_daemon_outage().await?;
        if let Err(error) = stop_launchd_service(DAEMON_SERVICE_LABEL) {
            let recovery = async {
                restart_launchd_service(DAEMON_SERVICE_LABEL)?;
                wait_for_daemon_release(
                    &client,
                    &daemon_ready_url,
                    &restored_package_version,
                    "existing oored",
                )
                .await?;
                barrier.end_planned_daemon_outage_and_release().await
            }
            .await;
            return match recovery {
                Ok(()) => Err(error),
                Err(recovery_error) => Err(error.context(format!(
                    "stopping oored failed and restoring its runner claim barrier also failed: {recovery_error}"
                ))),
            };
        }
    }
    let backup_dir = install_root.join("backups");
    let backup_path = backup_dir.join(format!(
        "pre-update-{}-{}.tar.gz",
        current,
        now_epoch_secs()
    ));
    let backup_result = create_bounded_update_backup(&database, &key, &backup_path).await;
    if let Err(error) = backup_result {
        if daemon_service_is_installed {
            restart_launchd_service(DAEMON_SERVICE_LABEL)?;
            wait_for_daemon_release(
                &client,
                &daemon_ready_url,
                &restored_package_version,
                "existing oored",
            )
            .await?;
            barrier.end_planned_daemon_outage_and_release().await?;
        } else {
            barrier.release().await?;
        }
        return Err(error);
    }
    println!("Created pre-update backup: {}", backup_path.display());
    let rollback_state = UpdateRollbackState {
        backup: backup_path,
        database: database.clone(),
        key: key.clone(),
    };
    let services = UpdateServicePlan {
        profile_update,
        defer_daemon_restart,
        daemon_is_managed: daemon_service_is_installed,
        unmanaged_daemon_listen: None,
        daemon_should_be_running: daemon_was_loaded,
        daemon_ready_url,
        installed_version: prepared
            .package_version
            .clone()
            .context("verified update daemon version was not prepared")?,
        restored_version: restored_package_version,
        runner_service: runner_update_service,
        runner_previous_pid,
        runner_ack,
        runner_installed_version: prepared.version.to_string(),
        runner_restored_version: current.to_string(),
        install_managed_runner,
        managed_runner_service_existed,
        managed_runner_config_existed,
        managed_runner_database: install_managed_runner.then_some(database),
        web_is_managed,
        web_was_running,
        web_ready_url,
    };
    let mut control = LiveUpdateServiceControl {
        install_root: &install_root,
        daemon_url: &daemon_url,
        client: &client,
    };
    let update_result = install_release_with_rollback(
        &staged_release,
        &install_root,
        &previous_release,
        channel,
        &repo,
        daemon_migration.as_ref(),
        runner_migration
            .as_ref()
            .map(|snapshot| snapshot as &dyn ManagedRunnerServiceRollback),
        Some(&barrier),
        &services,
        Some(&rollback_state),
        &mut control,
    )
    .await;
    if let Err(error) = update_result {
        return match barrier.release().await {
            Ok(()) => Err(error),
            Err(release_error) => Err(error.context(format!(
                "releasing the maintenance barrier also failed: {release_error}"
            ))),
        };
    }
    println!("Updated to version {latest}.");

    // 12. Note about current process
    if current != *latest {
        println!(
            "\nNote: This process is still running version {current}. \
             The new version ({latest}) will be used on next invocation."
        );
    }

    Ok(())
}

fn handle_bootstrap_guard(args: BootstrapGuardArgs) -> anyhow::Result<()> {
    anyhow::ensure!(
        args.target_version == args.target_version.trim(),
        "target version contains surrounding whitespace"
    );
    semver::Version::parse(&args.target_version)
        .with_context(|| format!("target version is not supported: {}", args.target_version))?;
    anyhow::ensure!(
        matches!(args.target_channel.as_str(), "stable" | "beta" | "alpha"),
        "target channel must be stable, beta, or alpha"
    );
    install_manifest::validate_repository(&args.target_repository)?;

    let install_root = resolve_install_root()?;
    let manifest_path = install_root.join("install-manifest.json");
    let manifest = match fs::symlink_metadata(&manifest_path) {
        Ok(_) => install_manifest::InstallManifest::load(&manifest_path).with_context(|| {
            format!(
                "cannot validate the installed device profile at {}",
                manifest_path.display()
            )
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let root_exists = match fs::symlink_metadata(&install_root) {
                Ok(_) => true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to inspect install root {}", install_root.display())
                    });
                }
            };
            let legacy_marker_exists = root_exists
                && match fs::symlink_metadata(install_root.join("INSTALL_MODE")) {
                    Ok(_) => true,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                    Err(error) => return Err(error).context("failed to inspect INSTALL_MODE"),
                };
            if legacy_marker_exists && uninstaller::detect_legacy_v0141(&install_root)? {
                println!("legacy-v0.1.41");
            } else {
                let mut live_components = Vec::new();
                for (component, label) in [
                    (
                        install_manifest::InstallComponent::ControlPlane,
                        "control plane",
                    ),
                    (install_manifest::InstallComponent::Runner, "runner"),
                    (install_manifest::InstallComponent::Web, "web"),
                ] {
                    if legacy_live_component_artifact_exists(&install_root, component)? {
                        live_components.push(label);
                    }
                }
                anyhow::ensure!(
                    live_components.is_empty(),
                    "cannot bootstrap safely because live {} components or services exist without a valid profile manifest or exact v0.1.41 layout",
                    live_components.join(", ")
                );
                println!("update");
            }
            return Ok(());
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {}", manifest_path.display()));
        }
    };
    let current = installer::read_bootstrap_release(&install_root)
        .context("cannot validate the installed bootstrap release")?;
    if manifest.profile != install_manifest::InstallProfile::CliOnly {
        anyhow::ensure!(
            manifest.release.version == current.version
                && manifest.release.channel == current.channel
                && manifest.release.repository == current.repository,
            "the installation profile and bootstrap metadata record different releases"
        );
    }

    let release_changed = current.version != args.target_version
        || current.channel != args.target_channel
        || current.repository != args.target_repository;
    if release_changed && manifest.profile != install_manifest::InstallProfile::CliOnly {
        let cli = shell_word(&install_root.join("bin/oore").display().to_string());
        anyhow::bail!(
            "This profile records release {} ({}, {}), but the bootstrap selected {} ({}, {}).\n\
             Run {cli} uninstall without --purge.\n\
             This preserves Oore data.\n\
             Then rerun the CLI bootstrap and install the same profile.",
            current.version,
            current.channel,
            current.repository,
            args.target_version,
            args.target_channel,
            args.target_repository
        );
    }
    if manifest.profile != install_manifest::InstallProfile::CliOnly {
        println!(
            "profile-preserve={}",
            installed_profile_name(manifest.profile)
        );
    } else {
        println!("update");
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let terminal = Terminal::new(cli.color.into());

    match cli.command {
        Commands::Install(args) => {
            let runtime =
                tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
            runtime.block_on(handle_install(args, terminal))?;
        }
        Commands::BootstrapGuard(args) => handle_bootstrap_guard(args)?,
        Commands::Uninstall(args) => uninstaller::handle(args, terminal)?,
        Commands::Setup(mut setup) => match setup.command.take() {
            Some(SetupSubcommand::Init(args)) => {
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_setup_init_direct(args))?;
            }
            Some(SetupSubcommand::Token(args) | SetupSubcommand::Open(args)) => {
                let daemon_url = resolve_daemon_url(setup.daemon_url.as_deref())?;
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_setup_token(args, &daemon_url))?;
            }
            None => {
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(setup_journey::handle(*setup, terminal))?;
            }
        },
        Commands::Frontend(frontend) => match frontend.command {
            FrontendSubcommand::Invite(args) => {
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_frontend_invite(args))?;
            }
        },
        Commands::Login(args) => {
            let runtime =
                tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
            runtime.block_on(handle_login(args, false))?;
        }
        Commands::Recovery(args) => {
            let runtime =
                tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
            runtime.block_on(handle_recovery(args))?;
        }
        Commands::Status(args) => {
            let runtime =
                tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
            runtime.block_on(handle_status(args))?;
        }
        Commands::Runner(runner) => match runner.command {
            RunnerSubcommand::Register(args) => {
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_runner_register(args, false))?;
            }
            RunnerSubcommand::Start(args) => {
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_runner_start(args))?;
            }
            RunnerSubcommand::InstallService(args) => {
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_runner_install_service_direct(args))?;
            }
            RunnerSubcommand::RotateToken => {
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_runner_rotate_token())?;
            }
            RunnerSubcommand::UninstallService => handle_runner_uninstall_service_direct()?,
        },
        Commands::Config(config) => match config.command {
            ConfigSubcommand::Set(args) => {
                if !config_key_supported(&args.key) {
                    eprintln!(
                        "Unsupported config key '{}'. Supported keys: {}, {}",
                        args.key, CONFIG_KEY_DAEMON_URL, CONFIG_KEY_SESSION_TOKEN
                    );
                    std::process::exit(2);
                }
                handle_config_set(args)?;
            }
            ConfigSubcommand::Get(args) => {
                if !config_key_supported(&args.key) {
                    eprintln!(
                        "Unsupported config key '{}'. Supported keys: {}, {}",
                        args.key, CONFIG_KEY_DAEMON_URL, CONFIG_KEY_SESSION_TOKEN
                    );
                    std::process::exit(2);
                }
                handle_config_get(args)?;
            }
        },
        Commands::Doctor(args) => {
            run_doctor_checks(args)?;
        }
        Commands::Pipeline(pipeline) => match pipeline.command {
            PipelineSubcommand::Validate(args) => handle_pipeline_validate(args)?,
        },
        Commands::Version => {
            let install_root = resolve_install_root()?;
            if let Some(v) = read_trimmed_file(&install_root.join("VERSION")) {
                println!("{v}");
            } else {
                println!("{}", env!("CARGO_PKG_VERSION"));
            }
        }
        Commands::Update(args) => {
            let runtime =
                tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
            runtime.block_on(handle_update(args))?;
        }
        Commands::UpdateSupervisor(args) => {
            let runtime =
                tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
            runtime.block_on(handle_update_supervisor(args))?;
        }
        Commands::Backup(args) => match args.command {
            BackupSubcommand::Create(args) => backup_create(args)?,
            BackupSubcommand::Verify(args) => backup_verify(args)?,
            BackupSubcommand::Restore(args) => backup_restore(args)?,
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn key_check_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            "CREATE TABLE trusted_proxy_settings (id INTEGER PRIMARY KEY, encrypted_shared_secret TEXT);
             CREATE TABLE encryption_key_check (id INTEGER PRIMARY KEY, check_ciphertext TEXT NOT NULL, created_at INTEGER NOT NULL);",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn setup_key_is_generated_only_for_a_database_without_ciphertext() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("encryption.key");
        let pool = key_check_pool().await;

        let key = load_or_generate_encryption_key_at(&pool, &path)
            .await
            .unwrap();
        assert_eq!(fs::read(&path).unwrap(), key);

        fs::remove_file(&path).unwrap();
        sqlx::query("INSERT INTO trusted_proxy_settings VALUES (1, ?1)")
            .bind(encrypt_secret("secret", &key).unwrap())
            .execute(&pool)
            .await
            .unwrap();
        let error = load_or_generate_encryption_key_at(&pool, &path)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("is missing"));
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn setup_key_must_match_the_daemon_key_check() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("encryption.key");
        let pool = key_check_pool().await;
        let daemon_key = [1u8; 32];
        sqlx::query("INSERT INTO encryption_key_check VALUES (1, ?1, 0)")
            .bind(
                encrypt_secret(
                    oore_contract::data_paths::ENCRYPTION_KEY_CHECK_PLAINTEXT,
                    &daemon_key,
                )
                .unwrap(),
            )
            .execute(&pool)
            .await
            .unwrap();

        assert!(
            load_or_generate_encryption_key_at(&pool, &path)
                .await
                .is_err()
        );
        assert!(!path.exists());

        fs::write(&path, [2u8; 32]).unwrap();
        let error = load_or_generate_encryption_key_at(&pool, &path)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("does not match this database"));

        fs::write(&path, daemon_key).unwrap();
        assert_eq!(
            load_or_generate_encryption_key_at(&pool, &path)
                .await
                .unwrap(),
            daemon_key
        );
    }

    fn queue_update_request(
        install_root: &Path,
        request: &serde_json::Value,
        mode: u32,
    ) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let request_file = install_root.join("run/runtime-update-queue/request.json");
        fs::create_dir_all(request_file.parent().unwrap()).unwrap();
        fs::write(&request_file, serde_json::to_vec(request).unwrap()).unwrap();
        fs::set_permissions(&request_file, fs::Permissions::from_mode(mode)).unwrap();
        request_file
    }

    fn recorded_update_status(install_root: &Path) -> oore_contract::RuntimeUpdateStatus {
        serde_json::from_slice(&fs::read(install_root.join(".runtime-update-status.json")).unwrap())
            .unwrap()
    }

    #[tokio::test]
    async fn update_supervisor_records_failed_pre_checks_and_clears_the_request() {
        let install_root = tempfile::tempdir().unwrap();
        let status = install_root.path().join(".runtime-update-status.json");
        let request = serde_json::json!({
            "parent_pid": 4242,
            "database": "/tmp/oore.db",
            "key": "/tmp/oore.key",
            "daemon_url": "http://127.0.0.1:8787",
            "status": status,
        });
        let request_file = queue_update_request(install_root.path(), &request, 0o644);

        let error = supervise_update_request(install_root.path(), &request_file)
            .await
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "runtime update request permissions are too broad"
        );
        let recorded = recorded_update_status(install_root.path());
        assert_eq!(recorded.phase, oore_contract::RuntimeUpdatePhase::Failed);
        assert_eq!(
            recorded.error.as_deref(),
            Some("runtime update request permissions are too broad")
        );
        assert!(!request_file.exists());
    }

    #[tokio::test]
    async fn update_supervisor_without_a_request_keeps_the_last_status() {
        let install_root = tempfile::tempdir().unwrap();
        let request_file = install_root
            .path()
            .join("run/runtime-update-queue/request.json");

        supervise_update_request(install_root.path(), &request_file)
            .await
            .unwrap_err();

        assert!(
            !install_root
                .path()
                .join(".runtime-update-status.json")
                .exists()
        );
    }

    #[tokio::test]
    async fn update_supervisor_records_invalid_requests_and_clears_the_claim() {
        let install_root = tempfile::tempdir().unwrap();
        let request = serde_json::json!({ "parent_pid": 4242 });
        let request_file = queue_update_request(install_root.path(), &request, 0o600);

        supervise_update_request(install_root.path(), &request_file)
            .await
            .unwrap_err();

        let recorded = recorded_update_status(install_root.path());
        assert_eq!(recorded.phase, oore_contract::RuntimeUpdatePhase::Failed);
        assert!(
            recorded
                .error
                .as_deref()
                .is_some_and(|error| error.starts_with("invalid runtime update request"))
        );
        assert!(!request_file.exists());
        assert_eq!(
            fs::read_dir(install_root.path().join("run"))
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("runtime-update-active-"))
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn setup_states_keep_their_database_encoding() {
        let directory = tempfile::tempdir().unwrap();
        let pool = connect_db(&directory.path().join("setup.db"))
            .await
            .unwrap();
        let mut state = new_setup_state(123);
        for (value, text) in [
            (SetupState::Uninitialized, "uninitialized"),
            (SetupState::BootstrapPending, "bootstrap_pending"),
            (SetupState::IdpConfigured, "idp_configured"),
            (SetupState::OwnerCreated, "owner_created"),
            (SetupState::Ready, "ready"),
        ] {
            state.setup_state = value;
            save_state(&pool, &state).await.unwrap();
            let stored: String =
                sqlx::query_scalar("SELECT setup_state FROM setup_state WHERE id = 1")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(stored, text);
            assert_eq!(load_state(&pool).await.unwrap().unwrap().setup_state, value);
        }
        sqlx::query("UPDATE setup_state SET setup_state = 'invalid'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            load_state(&pool).await.unwrap_err().to_string(),
            "unknown setup state: invalid"
        );
        pool.close().await;
    }

    fn profile_manifest_on(version: &str, channel: &str) -> install_manifest::InstallManifest {
        let release = install_manifest::InstallRelease::new(
            version.to_string(),
            channel.to_string(),
            DEFAULT_GITHUB_REPO.to_string(),
            format!("oore_{version}_darwin_arm64.tar.gz"),
            "a".repeat(64),
        )
        .unwrap();
        install_manifest::InstallManifest::new(
            install_manifest::InstallProfile::ControlPlane,
            release,
            Vec::new(),
        )
        .unwrap()
    }

    fn signed_update_release(version: &str) -> PreparedUpdateRelease {
        PreparedUpdateRelease {
            path: None,
            version: semver::Version::parse(version).unwrap(),
            package_version: None,
            label: format!("v{version}"),
            provenance: Some(PreparedUpdateProvenance {
                archive: format!("oore_{version}_darwin_arm64.tar.gz"),
                archive_sha256: "b".repeat(64),
                manifest_sha256: "c".repeat(64),
            }),
            _temporary: None,
        }
    }

    fn plan_profile_update(
        installed: (&str, &str),
        target: (&str, ReleaseChannel),
        switch_channel: bool,
    ) -> anyhow::Result<ProfileUpdatePlan> {
        prepare_profile_update_plan(
            Some(profile_manifest_on(installed.0, installed.1)),
            &signed_update_release(target.0),
            target.1,
            DEFAULT_GITHUB_REPO,
            false,
            switch_channel,
        )
        .map(Option::unwrap)
    }

    #[test]
    fn profile_updates_switch_channel_only_when_asked() {
        let error = plan_profile_update(
            ("0.1.47", "stable"),
            ("0.1.48-alpha.1", ReleaseChannel::Alpha),
            false,
        )
        .err()
        .unwrap();
        assert_eq!(
            error.to_string(),
            "profile updates stay on the recorded stable channel; pass --switch-channel to move this installation to the alpha channel"
        );

        let plan = plan_profile_update(
            ("0.1.47", "stable"),
            ("0.1.48-alpha.1", ReleaseChannel::Alpha),
            true,
        )
        .unwrap();
        assert_eq!(plan.release.version, "0.1.48-alpha.1");
        assert_eq!(plan.release.channel, "alpha");

        let plan = plan_profile_update(
            ("0.1.48-alpha.2", "alpha"),
            ("0.1.48-beta.1", ReleaseChannel::Beta),
            true,
        )
        .unwrap();
        assert_eq!(plan.release.channel, "beta");
    }

    #[test]
    fn profile_channel_switches_never_downgrade() {
        let error = plan_profile_update(
            ("0.1.47", "stable"),
            ("0.1.47-alpha.3", ReleaseChannel::Alpha),
            true,
        )
        .err()
        .unwrap();
        assert_eq!(
            error.to_string(),
            "profile updates cannot downgrade from 0.1.47 to 0.1.47-alpha.3"
        );
        let error = plan_profile_update(
            ("0.1.48-alpha.1", "alpha"),
            ("0.1.47", ReleaseChannel::Stable),
            true,
        )
        .err()
        .unwrap();
        assert_eq!(
            error.to_string(),
            "profile updates cannot downgrade from 0.1.48-alpha.1 to 0.1.47"
        );
    }

    #[test]
    fn profile_updates_promote_a_prerelease_to_its_stable_release_without_a_switch() {
        let plan = plan_profile_update(
            ("0.1.48-alpha.3", "alpha"),
            ("0.1.48", ReleaseChannel::Stable),
            false,
        )
        .unwrap();
        assert_eq!(plan.release.channel, "stable");
        assert!(
            plan_profile_update(
                ("0.1.48-alpha.3", "alpha"),
                ("0.1.49", ReleaseChannel::Stable),
                false,
            )
            .is_err()
        );
    }

    #[derive(Default)]
    struct ScriptedServiceControl {
        calls: Vec<String>,
        failures: BTreeSet<usize>,
    }

    impl ScriptedServiceControl {
        fn record(&mut self, call: String) -> anyhow::Result<()> {
            let index = self.calls.len();
            self.calls.push(call.clone());
            anyhow::ensure!(!self.failures.contains(&index), "{call} failed");
            Ok(())
        }
    }

    impl UpdateServiceControl for ScriptedServiceControl {
        fn restart_managed_service(&mut self, label: &str) -> anyhow::Result<()> {
            self.record(format!("restart {label}"))
        }

        async fn activate_runner_service(
            &mut self,
            label: &str,
            _previous_pid: Option<u32>,
            _ack: Option<&RunnerUpdateAck>,
            activation: ReleaseActivation,
        ) -> anyhow::Result<()> {
            let activation = match activation {
                ReleaseActivation::Installed => "installed",
                ReleaseActivation::Restored => "restored",
            };
            self.record(format!("activate {label} {activation}"))
        }

        fn stop_managed_service(&mut self, label: &str) -> anyhow::Result<()> {
            self.record(format!("stop {label}"))
        }

        async fn restart_unmanaged_daemon(&mut self, listen: &str) -> anyhow::Result<()> {
            self.record(format!("restart unmanaged {listen}"))
        }

        async fn stop_unmanaged_daemon(&mut self, listen: &str) -> anyhow::Result<()> {
            self.record(format!("stop unmanaged {listen}"))
        }

        async fn wait_for_endpoint(&mut self, url: &str, _name: &str) -> anyhow::Result<()> {
            self.record(format!("wait {url}"))
        }

        async fn wait_for_daemon_release(
            &mut self,
            ready_url: &str,
            expected_package_version: &str,
            _name: &str,
        ) -> anyhow::Result<()> {
            self.record(format!("wait {ready_url} {expected_package_version}"))
        }
    }

    const INSTALLED_CALLS: [&str; 5] = [
        "restart build.oore.oored",
        "wait http://127.0.0.1:8787/readyz 0.1.49",
        "restart build.oore.oore-web",
        "wait http://127.0.0.1:3000/",
        "activate build.oore.oore-runner installed",
    ];

    const ROLLBACK_CALLS: [&str; 6] = [
        "stop build.oore.oored",
        "restart build.oore.oored",
        "wait http://127.0.0.1:8787/readyz 0.1.48",
        "restart build.oore.oore-web",
        "wait http://127.0.0.1:3000/",
        "activate build.oore.oore-runner restored",
    ];

    struct UpdateFixture {
        _temporary: tempfile::TempDir,
        install_root: PathBuf,
        previous_release: PathBuf,
        stage: PathBuf,
    }

    impl UpdateFixture {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let install_root = temporary.path().join("oore");
            let previous_release = temporary.path().join("previous");
            let stage = temporary.path().join("stage");
            for (root, version) in [
                (&install_root, "0.1.48"),
                (&previous_release, "0.1.48"),
                (&stage, "0.1.49"),
            ] {
                for binary in ["oore", "oored", "oore-web"] {
                    let path = root.join("bin").join(binary);
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(&path, format!("{binary} {version}")).unwrap();
                    set_executable(&path).unwrap();
                }
                fs::write(root.join("VERSION"), version).unwrap();
            }
            for root in [&install_root, &previous_release] {
                fs::write(root.join("CHANNEL"), "stable").unwrap();
                fs::write(root.join("GITHUB_REPO"), "oore-ci/oore.build").unwrap();
            }
            fs::create_dir(stage.join("web-dist")).unwrap();
            fs::write(stage.join("web-dist/index.html"), "0.1.49").unwrap();
            Self {
                _temporary: temporary,
                install_root,
                previous_release,
                stage,
            }
        }

        fn services() -> UpdateServicePlan {
            UpdateServicePlan {
                profile_update: None,
                defer_daemon_restart: false,
                daemon_is_managed: true,
                unmanaged_daemon_listen: None,
                daemon_should_be_running: true,
                daemon_ready_url: "http://127.0.0.1:8787/readyz".to_string(),
                installed_version: "0.1.49".to_string(),
                restored_version: "0.1.48".to_string(),
                runner_service: Some(RUNNER_SERVICE_LABEL),
                runner_previous_pid: None,
                runner_ack: None,
                runner_installed_version: "0.1.49".to_string(),
                runner_restored_version: "0.1.48".to_string(),
                install_managed_runner: false,
                managed_runner_service_existed: false,
                managed_runner_config_existed: false,
                managed_runner_database: None,
                web_is_managed: true,
                web_was_running: true,
                web_ready_url: Some("http://127.0.0.1:3000/".to_string()),
            }
        }

        async fn install(&self, control: &mut ScriptedServiceControl) -> anyhow::Result<()> {
            install_release_with_rollback(
                &self.stage,
                &self.install_root,
                &self.previous_release,
                ReleaseChannel::Beta,
                "oore-ci/oore.build",
                None,
                None,
                None,
                &Self::services(),
                None,
                control,
            )
            .await
        }

        fn read(&self, relative: &str) -> Option<String> {
            fs::read_to_string(self.install_root.join(relative)).ok()
        }

        fn assert_release(&self, version: &str, channel: &str) {
            for binary in ["oore", "oored", "oore-web"] {
                assert_eq!(
                    self.read(&format!("bin/{binary}")).as_deref(),
                    Some(format!("{binary} {version}").as_str())
                );
            }
            assert_eq!(self.read("VERSION").as_deref(), Some(version));
            assert_eq!(self.read("CHANNEL").as_deref(), Some(channel));
            // Markers are `v2:<nonce>:<executable identity>`; the nonce differs per write.
            let identity =
                |marker: &str| marker.trim_end().splitn(3, ':').nth(2).map(str::to_owned);
            let expected =
                oore_runner::runner_release_marker(&self.install_root.join("bin/oore")).unwrap();
            let published = self.read(oore_runner::RUNNER_RELEASE_MARKER_FILE).unwrap();
            assert_eq!(identity(&published), identity(&expected));
            assert!(identity(&expected).is_some());
        }
    }

    #[tokio::test]
    async fn updates_install_the_staged_release_and_activate_every_service() {
        let fixture = UpdateFixture::new();
        let mut control = ScriptedServiceControl::default();

        fixture.install(&mut control).await.unwrap();

        assert_eq!(control.calls, INSTALLED_CALLS);
        fixture.assert_release("0.1.49", "beta");
        assert_eq!(fixture.read("WEB_VERSION").as_deref(), Some("0.1.49"));
        assert_eq!(
            fixture.read("web-dist/index.html").as_deref(),
            Some("0.1.49")
        );
    }

    #[tokio::test]
    async fn updates_restore_the_previous_release_when_any_activation_step_fails() {
        for failed_step in 0..INSTALLED_CALLS.len() {
            let fixture = UpdateFixture::new();
            let mut control = ScriptedServiceControl {
                failures: BTreeSet::from([failed_step]),
                ..Default::default()
            };

            let error = fixture.install(&mut control).await.unwrap_err();

            assert_eq!(
                error.to_string(),
                format!("{} failed", INSTALLED_CALLS[failed_step]),
            );
            let expected = INSTALLED_CALLS[..=failed_step]
                .iter()
                .chain(ROLLBACK_CALLS.iter())
                .collect::<Vec<_>>();
            assert_eq!(control.calls.iter().collect::<Vec<_>>(), expected);
            fixture.assert_release("0.1.48", "stable");
            assert_eq!(fixture.read("WEB_VERSION"), None);
            assert!(!fixture.install_root.join("web-dist").exists());
        }
    }

    #[tokio::test]
    async fn updates_report_both_errors_when_the_rollback_also_fails() {
        let fixture = UpdateFixture::new();
        let rollback_restart = 1 + ROLLBACK_CALLS
            .iter()
            .position(|call| *call == "restart build.oore.oored")
            .unwrap();
        let mut control = ScriptedServiceControl {
            failures: BTreeSet::from([0, rollback_restart]),
            ..Default::default()
        };

        let error = fixture.install(&mut control).await.unwrap_err();

        assert_eq!(
            format!("{error:#}"),
            "rollback also failed: restart build.oore.oored failed: restart build.oore.oored failed"
        );
        assert_eq!(control.calls.len(), rollback_restart + 1);
        // The files were restored before the services failed to restart.
        fixture.assert_release("0.1.48", "stable");
    }

    #[tokio::test]
    async fn updates_refuse_to_start_without_an_installed_cli() {
        let fixture = UpdateFixture::new();
        fs::remove_file(fixture.install_root.join("bin/oore")).unwrap();
        let mut control = ScriptedServiceControl::default();

        assert!(fixture.install(&mut control).await.is_err());
        assert!(control.calls.is_empty());
        assert_eq!(fixture.read("VERSION").as_deref(), Some("0.1.48"));
    }
}
