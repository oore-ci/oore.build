use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::Context;
use oore_cli_ui::{PromptResult, SelectChoice, Terminal};
use oore_contract::{
    FrontendPairRequest, FrontendPairResponse, RemoteAuthMode, RuntimeMode, SetupState, SetupStatus,
};
use rand::RngCore;
use serde::Deserialize;
use tokio::process::Command;
use tokio::time::{Instant, sleep, timeout};
use url::Url;

use super::install_lock::InstallLock;
use super::install_manifest::{InstallManifest, InstallProfile, InstallService, InstallState};
use super::{
    LoginArgs, RunnerRegisterArgs, RunnerServiceArgs, SetupAccess, SetupArgs, SetupInitArgs,
    SetupInitMode, SetupInterface,
};

const DEFAULT_DAEMON_LISTEN: &str = "127.0.0.1:8787";
const DEFAULT_WEB_LISTEN: &str = "127.0.0.1:4173";
const SETUP_WAIT_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const SUDO_INTERACTIVE_TIMEOUT: Duration = Duration::from_secs(60);
const SUDO_NONINTERACTIVE_TIMEOUT: Duration = Duration::from_secs(10);
const CONFIG_FILE_LIMIT: u64 = 1024 * 1024;
const WEB_PROOF_FILE: &str = "web-backend-proof";
const WEB_IDENTITY_HEADER_FILE: &str = "web-user-email-header";
const WEB_BACKEND_URL_FILE: &str = "web-backend-url";
const WEB_UPSTREAM_PROOF_FILE: &str = "web-upstream-proof";
const CONTROL_PLANE_PROXY_PROOF_FILE: &str = "control-plane-proxy-proof";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetupFile {
    interface: Option<SetupInterface>,
    access: Option<SetupAccess>,
    owner_email: Option<String>,
    state_file: Option<String>,
    daemon_url: Option<String>,
    daemon_listen: Option<String>,
    web_listen: Option<String>,
    backend_url: Option<String>,
    runner_name: Option<String>,
    runner_token_file: Option<PathBuf>,
    session_token_file: Option<PathBuf>,
    pairing_code: Option<String>,
    browser_transport_protected: Option<bool>,
    backend_transport_protected: Option<bool>,
    trusted_proxy: Option<TrustedProxyFile>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustedProxyFile {
    user_email_header: Option<String>,
    trusted_proxy_cidrs: Option<Vec<String>>,
    shared_secret_file: Option<PathBuf>,
    upstream_shared_secret_file: Option<PathBuf>,
}

#[derive(Debug)]
struct JourneyOptions {
    interface: SetupInterface,
    access: Option<SetupAccess>,
    owner_email: Option<String>,
    state_file: Option<String>,
    daemon_url: Option<String>,
    daemon_listen: String,
    web_listen: String,
    backend_url: Option<String>,
    runner_token: Option<String>,
    session_token: Option<String>,
    runner_name: Option<String>,
    pairing_code: Option<String>,
    browser_transport_protected: bool,
    backend_transport_protected: bool,
    trusted_proxy: TrustedProxyOptions,
    plan_only: bool,
    json: bool,
}

#[derive(Debug, Default)]
struct TrustedProxyOptions {
    user_email_header: Option<String>,
    trusted_proxy_cidrs: Option<Vec<String>>,
    shared_secret_file: Option<PathBuf>,
    upstream_shared_secret_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WebPairingState {
    Absent,
    Bound,
    DifferentBackend,
}

struct WebSecretsSnapshot {
    directory: PathBuf,
    values: Vec<(PathBuf, Option<Vec<u8>>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlPlaneProxyProofSource {
    Supplied,
    Stored,
    Generated,
}

struct ControlPlaneProxyProof {
    path: PathBuf,
    source: ControlPlaneProxyProofSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanAccessResolution {
    NotApplicable,
    Selected(SetupAccess),
    ChooseInBrowser,
    ResumePersisted,
}

pub(super) async fn handle(args: SetupArgs, terminal: Terminal) -> anyhow::Result<()> {
    terminal.intro("Setup")?;

    let install_root = super::resolve_install_root()?;
    let manifest_path = install_root.join("install-manifest.json");
    let initial_manifest = load_manifest(&manifest_path)?;
    let setup_file = load_setup_file(args.config_file.as_deref())?;
    let mut options = JourneyOptions::merge(args, setup_file, &initial_manifest)?;
    apply_existing_daemon_defaults(&install_root, &initial_manifest, &mut options)?;
    options.validate(initial_manifest.profile)?;
    options.interface = resolve_interface(
        options.interface,
        options.access,
        initial_manifest.profile,
        terminal,
    );
    validate_profile_interface(initial_manifest.profile, options.interface)?;
    validate_access_interface(options.access, options.interface)?;
    prepare_profile_backend(initial_manifest.profile, &mut options, terminal)?;
    preflight_profile_backend(initial_manifest.profile, &options).await?;
    let Some(plan_access) = resolve_plan_access(&initial_manifest, &mut options, terminal).await?
    else {
        terminal.outro("No changes were made.")?;
        return Ok(());
    };

    terminal.note(
        "Setup plan",
        render_plan(initial_manifest.profile, &options, plan_access),
    )?;
    if options.plan_only {
        terminal.outro("No changes were made.")?;
        return Ok(());
    }

    let _lifecycle_lock = InstallLock::acquire(&install_root)?;
    let mut manifest = load_manifest(&manifest_path)?;
    anyhow::ensure!(
        manifest == initial_manifest,
        "the installation changed while the setup plan was reviewed; run `oore setup` again"
    );
    options.apply_manifest_defaults(&manifest);
    validate_profile_interface(manifest.profile, options.interface)?;
    validate_access_interface(options.access, options.interface)?;
    options.validate(manifest.profile)?;
    prepare_profile_backend(manifest.profile, &mut options, terminal)?;
    preflight_profile_backend(manifest.profile, &options).await?;
    authorize_service_changes(manifest.profile, terminal).await?;
    options.validate(manifest.profile)?;
    revalidate_local_access(manifest.profile, &options).await?;
    publish_configuring(&mut manifest, &manifest_path, &options)?;

    let result = match manifest.profile {
        InstallProfile::Complete => setup_complete(&install_root, &options, terminal).await,
        InstallProfile::ControlPlane => {
            setup_control_plane(&install_root, &options, terminal).await
        }
        InstallProfile::Runner => setup_runner(&install_root, &options, terminal).await,
        InstallProfile::WebNode => setup_web_node(&install_root, &options, terminal).await,
        InstallProfile::CliOnly => setup_cli_only(&options, terminal).await,
    };

    let backend_url = match result {
        Ok(backend_url) => backend_url,
        Err(error) => {
            let guidance = "setup stopped before final verification; fix the reported problem and run `oore setup` again";
            return Err(error.context(guidance));
        }
    };
    let Some(backend_url) = backend_url else {
        terminal.outro("Setup is paused. Run `oore setup` to continue.")?;
        return Ok(());
    };
    manifest.lifecycle.backend_url = Some(backend_url);
    manifest.write_atomic(&manifest_path)?;

    verify_profile_services(&install_root, manifest.profile).await?;
    let backend_url = manifest
        .lifecycle
        .backend_url
        .as_deref()
        .context("setup verification did not produce a control-plane URL")?;
    super::save_cli_daemon_url(backend_url)?;
    manifest.mark_ready()?;
    manifest.write_atomic(&manifest_path)?;
    print_ready(&manifest, &manifest_path, options.json, terminal).await
}

fn validate_profile_interface(
    profile: InstallProfile,
    interface: SetupInterface,
) -> anyhow::Result<()> {
    if interface == SetupInterface::Browser && profile != InstallProfile::Complete {
        anyhow::bail!(
            "browser setup is available for the Complete profile; use `--interface terminal` for this device role"
        );
    }
    Ok(())
}

fn validate_access_interface(
    access: Option<SetupAccess>,
    interface: SetupInterface,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        access != Some(SetupAccess::TrustedProxy) || interface != SetupInterface::Browser,
        "trusted access proxy setup uses the terminal in this release; rerun with `oore setup --interface terminal --access trusted-proxy`"
    );
    Ok(())
}

impl JourneyOptions {
    fn merge(args: SetupArgs, file: SetupFile, manifest: &InstallManifest) -> anyhow::Result<Self> {
        let interface = if args.interface == SetupInterface::Auto {
            file.interface.unwrap_or(args.interface)
        } else {
            args.interface
        };
        let daemon_listen = prefer_config_default(
            args.daemon_listen,
            DEFAULT_DAEMON_LISTEN,
            file.daemon_listen,
        );
        let web_listen =
            prefer_config_default(args.web_listen, DEFAULT_WEB_LISTEN, file.web_listen);
        let explicit_token = args
            .runner_token
            .map(|token| nonempty(token, "Oore API or session token"))
            .transpose()?;
        let runner_token = match explicit_token.clone() {
            Some(token) => Some(token),
            None => read_optional_secret(
                file.runner_token_file.as_deref(),
                "Oore API or session token",
            )?,
        };
        let session_token = match explicit_token {
            Some(token) => Some(token),
            None => read_optional_secret(
                file.session_token_file.as_deref(),
                "Oore API or session token",
            )?,
        };
        let trusted_proxy_file = file.trusted_proxy.unwrap_or_default();
        let mut options = Self {
            interface,
            access: args.access.or(file.access),
            owner_email: args.owner_email.or(file.owner_email),
            state_file: args.state_file.or(file.state_file),
            daemon_url: args.daemon_url.or(file.daemon_url),
            daemon_listen,
            web_listen,
            backend_url: args.backend_url.or(file.backend_url),
            runner_token,
            session_token,
            runner_name: args.runner_name.or(file.runner_name),
            pairing_code: args.pairing_code.or(file.pairing_code),
            browser_transport_protected: args.browser_transport_protected
                || file.browser_transport_protected.unwrap_or(false),
            backend_transport_protected: args.backend_transport_protected
                || file.backend_transport_protected.unwrap_or(false),
            trusted_proxy: TrustedProxyOptions {
                user_email_header: trusted_proxy_file.user_email_header,
                trusted_proxy_cidrs: trusted_proxy_file.trusted_proxy_cidrs,
                shared_secret_file: trusted_proxy_file.shared_secret_file,
                upstream_shared_secret_file: trusted_proxy_file.upstream_shared_secret_file,
            },
            plan_only: args.plan,
            json: args.json,
        };
        options.apply_manifest_defaults(manifest);
        options.validate(manifest.profile)?;
        Ok(options)
    }

    fn apply_manifest_defaults(&mut self, manifest: &InstallManifest) {
        if self.daemon_listen == DEFAULT_DAEMON_LISTEN
            && let Some(value) = manifest.lifecycle.daemon_listen.as_deref()
        {
            self.daemon_listen = value.to_string();
        }
        if self.web_listen == DEFAULT_WEB_LISTEN
            && let Some(value) = manifest.lifecycle.web_listen.as_deref()
        {
            self.web_listen = value.to_string();
        }
        if self.backend_url.is_none() {
            self.backend_url = manifest.lifecycle.backend_url.clone();
        }
        if self.state_file.is_none() {
            self.state_file = manifest.lifecycle.state_file.clone();
        }
        self.browser_transport_protected |= manifest.lifecycle.browser_transport_protected;
        self.backend_transport_protected |= manifest.lifecycle.backend_transport_protected;
    }

    fn validate(&self, profile: InstallProfile) -> anyhow::Result<()> {
        let daemon_listen = parse_listen(&self.daemon_listen, "daemon listen address")?;
        validate_managed_daemon_listen(daemon_listen, self.backend_transport_protected)?;
        let web_listen = parse_listen(&self.web_listen, "web listen address")?;
        anyhow::ensure!(
            profile != InstallProfile::Complete || web_listen.ip().is_loopback(),
            "the Complete profile web service must listen on loopback in this release; use a Web node behind protected ingress for a non-loopback listener"
        );
        if let Some(state_file) = self.state_file.as_deref() {
            anyhow::ensure!(
                Path::new(state_file).is_absolute(),
                "the control-plane database path must be absolute"
            );
        }
        if let Some(url) = self.daemon_url.as_deref() {
            parse_http_url(url, "daemon URL")?;
        }
        if let Some(url) = self.backend_url.as_deref() {
            validate_backend_transport(url, self.backend_transport_protected)?;
        }
        Ok(())
    }
}

fn apply_existing_daemon_defaults(
    install_root: &Path,
    manifest: &InstallManifest,
    options: &mut JourneyOptions,
) -> anyhow::Result<()> {
    if !matches!(
        manifest.profile,
        InstallProfile::Complete | InstallProfile::ControlPlane
    ) {
        return Ok(());
    }
    let Some((listen, state_file)) = super::managed_services::daemon_configuration(install_root)?
    else {
        return Ok(());
    };
    if manifest.lifecycle.daemon_listen.is_none() && options.daemon_listen == DEFAULT_DAEMON_LISTEN
    {
        options.daemon_listen = listen;
    }
    if options.state_file.is_none() {
        options.state_file = Some(state_file.display().to_string());
    }
    Ok(())
}

async fn authorize_service_changes(
    profile: InstallProfile,
    terminal: Terminal,
) -> anyhow::Result<()> {
    if profile.services().is_empty() {
        return Ok(());
    }

    terminal.note(
        "Administrator access",
        "Oore needs permission to install or repair this device's managed services.",
    )?;

    let mut command = Command::new("/usr/bin/sudo");
    if terminal.is_interactive() {
        command.arg("-v").stdin(Stdio::inherit());
    } else {
        command.args(["-n", "-v"]).stdin(Stdio::null());
    }
    command.stdout(Stdio::inherit()).stderr(Stdio::inherit());

    let mut child = command
        .spawn()
        .context("failed to request administrator access")?;
    let wait_limit = if terminal.is_interactive() {
        SUDO_INTERACTIVE_TIMEOUT
    } else {
        SUDO_NONINTERACTIVE_TIMEOUT
    };
    let status = match timeout(wait_limit, child.wait()).await {
        Ok(result) => result.context("failed while requesting administrator access")?,
        Err(_) => {
            child
                .start_kill()
                .context("administrator access timed out and sudo could not be stopped")?;
            let _ = child.wait().await;
            anyhow::bail!("administrator access timed out before setup changed the installation")
        }
    };
    if status.success() {
        return Ok(());
    }
    if terminal.is_interactive() {
        anyhow::bail!("administrator access was not granted; no setup changes were made")
    }
    anyhow::bail!("administrator access is not active; run `sudo -v`, then rerun `oore setup`")
}

fn prepare_profile_backend(
    profile: InstallProfile,
    options: &mut JourneyOptions,
    terminal: Terminal,
) -> anyhow::Result<()> {
    let candidate = match profile {
        InstallProfile::Complete | InstallProfile::ControlPlane => return Ok(()),
        InstallProfile::Runner => {
            let existing = super::read_runner_config(&super::runner_config_path(None)?)?;
            options
                .backend_url
                .clone()
                .or_else(|| existing.map(|config| config.daemon_url))
        }
        InstallProfile::WebNode => options.backend_url.clone(),
        InstallProfile::CliOnly => options
            .backend_url
            .clone()
            .or_else(|| options.daemon_url.clone()),
    };
    let cancellation = match profile {
        InstallProfile::Runner => "runner setup was cancelled",
        InstallProfile::WebNode => "web-node setup was cancelled",
        InstallProfile::CliOnly => "CLI setup was cancelled",
        InstallProfile::Complete | InstallProfile::ControlPlane => unreachable!(),
    };
    let backend_url = required_value(
        candidate,
        terminal,
        "Control-plane URL",
        None,
        "set --backend-url or add backend_url to the setup config",
    )?
    .context(cancellation)?;
    validate_backend_transport(&backend_url, options.backend_transport_protected)?;
    options.backend_url = Some(canonical_http_url(&backend_url, "control-plane URL")?);
    Ok(())
}

async fn preflight_profile_backend(
    profile: InstallProfile,
    options: &JourneyOptions,
) -> anyhow::Result<()> {
    if matches!(
        profile,
        InstallProfile::Complete | InstallProfile::ControlPlane
    ) {
        return Ok(());
    }
    let backend_url = options
        .backend_url
        .as_deref()
        .context("remote profile setup is missing its control-plane URL")?;
    let status = require_ready_backend(backend_url).await?;
    if profile == InstallProfile::WebNode {
        anyhow::ensure!(
            status.runtime_mode == RuntimeMode::Remote,
            "a Web node cannot sign in to a Local Only control plane; use Complete on that device, or configure Identity provider or Trusted Proxy access on the control plane first"
        );
    }
    Ok(())
}

async fn resolve_plan_access(
    manifest: &InstallManifest,
    options: &mut JourneyOptions,
    terminal: Terminal,
) -> anyhow::Result<Option<PlanAccessResolution>> {
    if !matches!(
        manifest.profile,
        InstallProfile::Complete | InstallProfile::ControlPlane
    ) {
        return Ok(Some(PlanAccessResolution::NotApplicable));
    }
    if options.interface == SetupInterface::Browser {
        return Ok(Some(match options.access {
            Some(access) => PlanAccessResolution::Selected(access),
            None => PlanAccessResolution::ChooseInBrowser,
        }));
    }

    let daemon_url = local_daemon_url(&options.daemon_listen)?;
    let status = verify_daemon_instance(&daemon_url).await.ok();
    if let Some(status) = status
        .as_ref()
        .filter(|status| setup_status_has_persisted_access(status))
    {
        if let Some(requested) = options.access {
            if matches!(status.state, SetupState::OwnerCreated | SetupState::Ready) {
                ensure_persisted_access_matches(Some(requested), status)?;
            }
            return Ok(Some(PlanAccessResolution::Selected(requested)));
        }

        let persisted = setup_access_from_status(status);
        options.access = Some(persisted);
        return Ok(Some(PlanAccessResolution::Selected(persisted)));
    }

    if let Some(access) = options.access {
        return Ok(Some(PlanAccessResolution::Selected(access)));
    }
    if status.is_none() && persisted_local_setup_exists(manifest, options)? {
        return Ok(Some(PlanAccessResolution::ResumePersisted));
    }

    let Some(access) = choose_access(None, terminal)? else {
        return Ok(None);
    };
    options.access = Some(access);
    Ok(Some(PlanAccessResolution::Selected(access)))
}

fn setup_status_has_persisted_access(status: &SetupStatus) -> bool {
    matches!(
        status.state,
        SetupState::IdpConfigured | SetupState::OwnerCreated | SetupState::Ready
    )
}

fn persisted_local_setup_exists(
    manifest: &InstallManifest,
    options: &JourneyOptions,
) -> anyhow::Result<bool> {
    let state_file = super::resolve_db_path(options.state_file.as_deref())?;
    let exists = match fs::symlink_metadata(&state_file) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to inspect setup state {}", state_file.display())
            });
        }
    };
    anyhow::ensure!(
        exists || manifest.lifecycle.state != InstallState::Ready,
        "the installation is marked ready, but its saved setup state is missing at {}; restore a backup or run `oore uninstall --purge` before a fresh install",
        state_file.display()
    );
    Ok(exists)
}

async fn revalidate_local_access(
    profile: InstallProfile,
    options: &JourneyOptions,
) -> anyhow::Result<()> {
    if !matches!(
        profile,
        InstallProfile::Complete | InstallProfile::ControlPlane
    ) {
        return Ok(());
    }
    let Some(requested) = options.access else {
        return Ok(());
    };

    let daemon_url = local_daemon_url(&options.daemon_listen)?;
    let Ok(status) = verify_daemon_instance(&daemon_url).await else {
        return Ok(());
    };
    if matches!(status.state, SetupState::OwnerCreated | SetupState::Ready) {
        ensure_persisted_access_matches(Some(requested), &status)?;
    }
    Ok(())
}

async fn setup_complete(
    install_root: &Path,
    options: &JourneyOptions,
    terminal: Terminal,
) -> anyhow::Result<Option<String>> {
    let daemon_url = ensure_daemon(install_root, options, terminal).await?;

    let web_already_started = options.interface == SetupInterface::Browser;
    if web_already_started {
        ensure_web(install_root, options, &daemon_url, terminal).await?;
        if !complete_access_in_browser(options, &daemon_url, terminal).await? {
            return Ok(None);
        }
    } else if !complete_access_in_terminal(
        install_root,
        InstallProfile::Complete,
        options,
        &daemon_url,
        terminal,
    )
    .await?
    {
        return Ok(None);
    }

    verify_setup_ready(&daemon_url).await?;
    reconcile_complete_web(
        install_root,
        options,
        &daemon_url,
        terminal,
        web_already_started,
    )
    .await?;
    ensure_managed_local_runner(install_root, options, &daemon_url, terminal).await?;
    verify_setup_ready(&daemon_url).await?;
    Ok(Some(daemon_url))
}

async fn setup_control_plane(
    install_root: &Path,
    options: &JourneyOptions,
    terminal: Terminal,
) -> anyhow::Result<Option<String>> {
    let daemon_url = ensure_daemon(install_root, options, terminal).await?;
    if options.interface == SetupInterface::Browser {
        anyhow::bail!(
            "browser setup needs the Complete profile's local web service; rerun with `--interface terminal`"
        );
    }
    if !complete_access_in_terminal(
        install_root,
        InstallProfile::ControlPlane,
        options,
        &daemon_url,
        terminal,
    )
    .await?
    {
        return Ok(None);
    }
    verify_setup_ready(&daemon_url).await?;
    Ok(Some(daemon_url))
}

async fn setup_runner(
    install_root: &Path,
    options: &JourneyOptions,
    terminal: Terminal,
) -> anyhow::Result<Option<String>> {
    let config_path = super::runner_config_path(None)?;
    let existing = super::read_runner_config(&config_path)?;
    let backend_url = options
        .backend_url
        .clone()
        .context("runner setup is missing its control-plane URL")?;
    require_ready_backend(&backend_url).await?;

    let needs_registration = match existing.as_ref() {
        Some(config) if same_http_endpoint(&config.daemon_url, &backend_url)? => {
            !runner_registration_is_valid(config).await?
        }
        _ => true,
    };
    if needs_registration {
        let saved_token = if options.runner_token.is_none() {
            validated_saved_cli_token_for_backend(&backend_url)
                .await?
                .filter(|(_, role)| matches!(role.as_str(), "owner" | "admin"))
                .map(|(token, _)| token)
        } else {
            None
        };
        if options.runner_token.is_none() && saved_token.is_none() && terminal.is_interactive() {
            terminal.note(
                "Create a Runner token",
                "Use a browser-connected Oore UI. Sign in as Owner or Admin, then open Settings > API tokens. A Control plane profile needs a Web node or another configured web client first.",
            )?;
        }
        let token = required_secret(
            options.runner_token.clone().or(saved_token),
            terminal,
            "Owner or Admin Oore API or session token",
            "create an Owner or Admin token under Settings > API tokens, then set OORE_SESSION_TOKEN or runner_token_file",
        )?
        .context("runner setup was cancelled")?;
        let name = optional_value(
            options.runner_name.clone(),
            terminal,
            "Runner name",
            Some(&oore_runner::get_hostname()),
        )?
        .context("runner setup was cancelled")?;
        super::handle_runner_register_with_lifecycle_lock(
            RunnerRegisterArgs {
                daemon_url: backend_url.clone(),
                name: nonempty_optional(name),
                labels: Vec::new(),
                token,
            },
            true,
        )
        .await?;
    }

    let operation = terminal.operation("Starting the runner service");
    let install_result = super::handle_runner_install_service(
        RunnerServiceArgs {
            config: None,
            managed_local: false,
            daemon_url: None,
            state_file: None,
            name: None,
        },
        true,
    )
    .await;
    finish_operation(operation, install_result, "Runner service is ready")?;
    super::managed_services::verify_service(install_root, InstallService::Runner).await?;
    Ok(Some(backend_url))
}

async fn runner_registration_is_valid(config: &oore_runner::RunnerConfig) -> anyhow::Result<bool> {
    let client = super::endpoint_http_client_builder(&config.daemon_url)
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to prepare the runner registration check")?;
    let response = client
        .post(format!(
            "{}/v1/runners/{}/heartbeat",
            config.daemon_url.trim_end_matches('/'),
            config.runner_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&serde_json::json!({ "status": "offline", "capabilities": {} }))
        .send()
        .await
        .context("failed to verify the stored runner registration")?;
    if response.status().is_success() {
        return Ok(true);
    }
    if matches!(
        response.status(),
        reqwest::StatusCode::UNAUTHORIZED
            | reqwest::StatusCode::FORBIDDEN
            | reqwest::StatusCode::NOT_FOUND
    ) {
        return Ok(false);
    }
    anyhow::bail!(
        "the control plane could not verify the stored runner registration (HTTP {})",
        response.status().as_u16()
    )
}

async fn setup_web_node(
    install_root: &Path,
    options: &JourneyOptions,
    terminal: Terminal,
) -> anyhow::Result<Option<String>> {
    let backend_url = options
        .backend_url
        .clone()
        .context("web-node setup is missing its control-plane URL")?;
    let backend_status = require_ready_backend(&backend_url).await?;
    anyhow::ensure!(
        backend_status.runtime_mode == RuntimeMode::Remote,
        "a Web node cannot sign in to a Local Only control plane; use Complete on that device, or configure Identity provider or Trusted Proxy access on the control plane first"
    );
    let snapshot = capture_web_secrets(install_root)?;
    let result = setup_web_node_inner(
        install_root,
        options,
        terminal,
        &backend_url,
        &backend_status,
    )
    .await;
    match result {
        Ok(()) => Ok(Some(backend_url)),
        Err(error) => match restore_web_secrets(&snapshot) {
            Ok(()) => Err(error.context("the previous web credentials were restored")),
            Err(rollback) => anyhow::bail!(
                "{error:#}; restoring the previous web credentials also failed: {rollback:#}"
            ),
        },
    }
}

async fn setup_web_node_inner(
    install_root: &Path,
    options: &JourneyOptions,
    terminal: Terminal,
    backend_url: &str,
    backend_status: &SetupStatus,
) -> anyhow::Result<()> {
    let pairing_state = preflight_private_pair(
        install_root,
        WEB_PROOF_FILE,
        WEB_IDENTITY_HEADER_FILE,
        backend_url,
    )?;
    let uses_trusted_proxy = backend_status.runtime_mode == RuntimeMode::Remote
        && backend_status.remote_auth_mode == RemoteAuthMode::TrustedProxy;
    let pairing_code = if uses_trusted_proxy && pairing_state != WebPairingState::Bound {
        Some(
            required_secret(
                options.pairing_code.clone(),
                terminal,
                "Single-use frontend pairing code",
                "set --pairing-code or add pairing_code to the setup config",
            )?
            .context("web-node pairing was cancelled")?,
        )
    } else {
        options.pairing_code.clone().and_then(nonempty_optional)
    };

    if let Some(code) = pairing_code {
        let paired = pair_frontend(backend_url, &code).await?;
        write_bound_web_pairing(install_root, backend_url, &paired)?;
    } else if !uses_trusted_proxy && pairing_state != WebPairingState::Absent {
        clear_web_proxy_credentials(install_root)?;
    }

    if uses_trusted_proxy {
        ensure_upstream_proxy_proof(install_root, options, terminal)?;
    }

    ensure_web(install_root, options, backend_url, terminal).await
}

async fn setup_cli_only(
    options: &JourneyOptions,
    terminal: Terminal,
) -> anyhow::Result<Option<String>> {
    let daemon_url = options
        .backend_url
        .clone()
        .context("CLI setup is missing its control-plane URL")?;

    let client = super::endpoint_http_client_builder(&daemon_url)
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to prepare the control-plane connection")?;
    let status = super::fetch_setup_status(&client, &daemon_url).await?;
    anyhow::ensure!(
        status.state == SetupState::Ready,
        "the selected control plane is not ready yet"
    );

    let explicit_token = options
        .session_token
        .clone()
        .or_else(|| options.runner_token.clone());
    let saved_token = if explicit_token.is_none() {
        validated_saved_cli_token_for_backend(&daemon_url)
            .await?
            .map(|(token, _)| token)
    } else {
        None
    };
    let supplied_token = explicit_token.or(saved_token);
    let token = if status.runtime_mode == RuntimeMode::Local && is_loopback_url(&daemon_url)? {
        supplied_token
    } else {
        Some(
            required_secret(
                supplied_token,
                terminal,
                "Oore API or session token",
                "set OORE_SESSION_TOKEN or add session_token_file to the setup config",
            )?
            .context("CLI setup was cancelled")?,
        )
    };

    super::handle_login(
        LoginArgs {
            daemon_url: Some(daemon_url.clone()),
            token,
            email: None,
            json: false,
        },
        true,
    )
    .await?;

    let saved = super::load_cli_config()?
        .session_token
        .context("login finished without saving a CLI session")?;
    super::fetch_user_profile(&client, &daemon_url, &saved).await?;
    Ok(Some(daemon_url))
}

async fn validated_saved_cli_token_for_backend(
    backend_url: &str,
) -> anyhow::Result<Option<(String, String)>> {
    let saved = super::load_cli_config()?;
    let token = match (saved.daemon_url.as_deref(), saved.session_token) {
        (Some(saved_url), Some(token)) if same_http_endpoint(saved_url, backend_url)? => {
            Some(token)
        }
        _ => None,
    };
    let Some(token) = token else {
        return Ok(None);
    };

    let client = super::endpoint_http_client_builder(backend_url)
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to prepare the saved token check")?;
    match super::fetch_user_profile(&client, backend_url, &token).await {
        Ok(profile) => Ok(Some((token, profile.user.role))),
        Err(_) => Ok(None),
    }
}

async fn ensure_daemon(
    install_root: &Path,
    options: &JourneyOptions,
    terminal: Terminal,
) -> anyhow::Result<String> {
    let daemon_url = local_daemon_url(&options.daemon_listen)?;
    let operation = terminal.operation("Starting the control plane");
    let state_file = super::resolve_db_path(options.state_file.as_deref())?;
    let result =
        super::managed_services::install_daemon(install_root, &options.daemon_listen, &state_file)
            .await;
    finish_operation(operation, result, "Control plane is running")?;
    super::managed_services::verify_service(install_root, InstallService::Daemon).await?;
    let operation = terminal.operation("Preparing one-click updates");
    let result = super::managed_services::install_update_supervisor(install_root).await;
    finish_operation(operation, result, "One-click updates are ready")?;
    super::managed_services::verify_service(install_root, InstallService::Updater).await?;
    Ok(daemon_url)
}

async fn ensure_web(
    install_root: &Path,
    options: &JourneyOptions,
    backend_url: &str,
    terminal: Terminal,
) -> anyhow::Result<()> {
    let operation = terminal.operation("Starting the web service");
    let result = super::managed_services::install_web(
        install_root,
        &options.web_listen,
        backend_url,
        options.browser_transport_protected,
        options.backend_transport_protected,
    )
    .await;
    finish_operation(operation, result, "Web service is running")?;
    super::managed_services::verify_service(install_root, InstallService::Web).await
}

async fn reconcile_complete_web(
    install_root: &Path,
    options: &JourneyOptions,
    daemon_url: &str,
    terminal: Terminal,
    web_already_started: bool,
) -> anyhow::Result<()> {
    let snapshot = capture_web_secrets(install_root)?;
    let result: anyhow::Result<()> = async {
        let changed =
            configure_complete_web_credentials(install_root, options, daemon_url, terminal).await?;
        if !web_already_started || changed {
            ensure_web(install_root, options, daemon_url, terminal).await?;
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(error) => match restore_web_secrets(&snapshot) {
            Ok(()) => Err(error.context("the previous web credentials were restored")),
            Err(rollback) => anyhow::bail!(
                "{error:#}; restoring the previous web credentials also failed: {rollback:#}"
            ),
        },
    }
}

async fn configure_complete_web_credentials(
    install_root: &Path,
    options: &JourneyOptions,
    daemon_url: &str,
    terminal: Terminal,
) -> anyhow::Result<bool> {
    let status = verify_daemon_instance(daemon_url).await?;
    let pairing_state = preflight_private_pair(
        install_root,
        WEB_PROOF_FILE,
        WEB_IDENTITY_HEADER_FILE,
        daemon_url,
    )?;
    if status.runtime_mode != RuntimeMode::Remote
        || status.remote_auth_mode != RemoteAuthMode::TrustedProxy
    {
        if pairing_state != WebPairingState::Absent {
            clear_web_proxy_credentials(install_root)?;
            return Ok(true);
        }
        return Ok(false);
    }

    if pairing_state != WebPairingState::Bound {
        let (code, _) = super::create_frontend_pairing_invite(
            daemon_url,
            options.state_file.as_deref(),
            Duration::from_secs(10 * 60),
        )
        .await?;
        let paired = pair_frontend(daemon_url, &code).await?;
        write_bound_web_pairing(install_root, daemon_url, &paired)?;
    }

    ensure_upstream_proxy_proof(install_root, options, terminal)?;
    Ok(true)
}

fn ensure_upstream_proxy_proof(
    install_root: &Path,
    options: &JourneyOptions,
    terminal: Terminal,
) -> anyhow::Result<()> {
    let secrets = prepare_secrets_directory(install_root)?;
    let destination = secrets.join(WEB_UPSTREAM_PROOF_FILE);
    let identity_header_path = secrets.join(WEB_IDENTITY_HEADER_FILE);
    let identity_header = read_existing_private_value(&identity_header_path)?
        .context("the paired web service is missing its identity header")?;
    let identity_header = nonempty(
        String::from_utf8(identity_header)
            .context("the paired web identity header is not UTF-8")?
            .trim()
            .to_string(),
        "paired web identity header",
    )?;
    let existing = read_existing_private_value(&destination)?;
    let configured = read_optional_secret(
        options.trusted_proxy.upstream_shared_secret_file.as_deref(),
        "upstream trusted proxy proof",
    )?;
    let (value, generated) = match (configured, existing) {
        (Some(value), _) => (value, false),
        (None, Some(value)) => (
            nonempty(
                String::from_utf8(value)
                    .context("the stored upstream trusted proxy proof is not UTF-8")?
                    .trim()
                    .to_string(),
                "upstream trusted proxy proof",
            )?,
            false,
        ),
        (None, None) => (generate_proxy_proof(), true),
    };
    write_private_value(&secrets, &destination, WEB_UPSTREAM_PROOF_FILE, &value)?;
    let generated_note = if generated {
        "Oore generated this proof for you."
    } else {
        "Oore kept your configured proof."
    };
    terminal.note(
        "Connect your access proxy",
        format!(
            "{generated_note}\n\nConfigure the proxy to send these request headers:\n  Proof: x-oore-web-trusted-proxy-secret\n  Email: {identity_header}\n\nRead the proof value when you configure the proxy:\n  cat {}\n\nKeep the proof file private.",
            super::shell_word(&destination.display().to_string()),
        ),
    )?;
    Ok(())
}

async fn ensure_managed_local_runner(
    install_root: &Path,
    options: &JourneyOptions,
    daemon_url: &str,
    terminal: Terminal,
) -> anyhow::Result<()> {
    let operation = terminal.operation("Starting the local runner");
    let result = super::handle_runner_install_service(
        RunnerServiceArgs {
            config: None,
            managed_local: true,
            daemon_url: Some(daemon_url.to_string()),
            state_file: options.state_file.clone(),
            name: options.runner_name.clone(),
        },
        true,
    )
    .await;
    finish_operation(operation, result, "Local runner is ready")?;
    super::managed_services::verify_service(install_root, InstallService::Runner).await
}

async fn complete_access_in_terminal(
    install_root: &Path,
    profile: InstallProfile,
    options: &JourneyOptions,
    daemon_url: &str,
    terminal: Terminal,
) -> anyhow::Result<bool> {
    let client = super::endpoint_http_client_builder(daemon_url)
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to prepare the setup connection")?;
    let setup_status = super::fetch_setup_status(&client, daemon_url).await?;
    if setup_status.state == SetupState::Ready {
        ensure_persisted_access_matches(options.access, &setup_status)?;
        return Ok(true);
    }
    if setup_status.state == SetupState::OwnerCreated {
        ensure_persisted_access_matches(options.access, &setup_status)?;
        terminal.note(
            "Owner account",
            "The owner already exists. Oore will finish the saved access setup without changing it.",
        )?;
        let operation = terminal.operation("Finishing access setup");
        let completed = finish_operation(
            operation,
            super::resume_setup_completion_via_daemon(
                &client,
                daemon_url,
                options.state_file.as_deref(),
            )
            .await,
            "Access setup is complete",
        )?;
        terminal.note(
            "Setup complete",
            format!("Instance: {}", completed.instance_id),
        )?;
        return Ok(true);
    }
    let recover_idp_configured = setup_status.state == SetupState::IdpConfigured;
    let selected_access = match (options.access, recover_idp_configured) {
        (Some(explicit), _) => Some(explicit),
        (None, true) => Some(setup_access_from_status(&setup_status)),
        (None, false) => None,
    };

    let Some(access) = choose_access(selected_access, terminal)? else {
        return Ok(false);
    };
    match access {
        SetupAccess::ThisDevice => {
            if options.owner_email.is_none() && terminal.is_interactive() {
                terminal.note(
                    "Local owner account",
                    "This email is your local account label. Oore does not send email or ask for a password. Passwordless sign-in works from this device or through an SSH tunnel.",
                )?;
            }
            let owner_email = required_value(
                options.owner_email.clone(),
                terminal,
                "Local owner email",
                Some("owner@local"),
                "set --owner-email or add owner_email to the setup config",
            )?
            .context("setup was cancelled")?;
            super::handle_setup_init_via_daemon(
                SetupInitArgs {
                    mode: SetupInitMode::Local,
                    owner_email,
                    user_email_header: "x-oore-user-email".to_string(),
                    trusted_proxy_cidrs: Vec::new(),
                    shared_secret: None,
                    shared_secret_file: None,
                    state_file: options.state_file.clone(),
                    force: recover_idp_configured,
                    json: false,
                },
                daemon_url,
                true,
            )
            .await?;
            Ok(true)
        }
        SetupAccess::IdentityProvider => {
            anyhow::ensure!(
                terminal.is_interactive(),
                "identity-provider setup needs an interactive terminal or `--interface browser`"
            );
            super::handle_setup_oidc_interactive(
                daemon_url,
                options.state_file.as_deref(),
                terminal,
            )
            .await
        }
        SetupAccess::TrustedProxy => {
            setup_trusted_proxy(
                install_root,
                profile,
                options,
                daemon_url,
                terminal,
                recover_idp_configured,
            )
            .await?;
            Ok(true)
        }
    }
}

async fn setup_trusted_proxy(
    install_root: &Path,
    profile: InstallProfile,
    options: &JourneyOptions,
    daemon_url: &str,
    terminal: Terminal,
    recover_idp_configured: bool,
) -> anyhow::Result<()> {
    let access_explanation = match profile {
        InstallProfile::Complete => {
            "Your proxy signs people in before they reach Oore. The first owner email must match the email that the proxy sends. Oore manages the private proof between its web service and control plane."
        }
        InstallProfile::ControlPlane => {
            "Your proxy signs people in before they reach Oore. The first owner email must match the email that the proxy sends. Oore stores the direct proxy proof privately on this device."
        }
        _ => anyhow::bail!("trusted access proxy setup needs a Complete or Control plane profile"),
    };
    terminal.note("Trusted access proxy", access_explanation)?;
    let owner_email = required_value(
        options.owner_email.clone(),
        terminal,
        "Initial owner email from the proxy",
        None,
        "set --owner-email or add owner_email to the setup config",
    )?
    .context("setup was cancelled")?;
    let header = required_value(
        options.trusted_proxy.user_email_header.clone(),
        terminal,
        "Header that contains the signed-in user email",
        Some("x-oore-user-email"),
        "add trusted_proxy.user_email_header to the setup config",
    )?
    .context("setup was cancelled")?;
    let cidrs = match options.trusted_proxy.trusted_proxy_cidrs.clone() {
        Some(values) => values,
        None if terminal.is_interactive() => {
            let raw = required_value(
                None,
                terminal,
                "Networks that can connect directly to the control plane",
                Some("127.0.0.1/32, ::1/128"),
                "add trusted_proxy.trusted_proxy_cidrs to the setup config",
            )?
            .context("setup was cancelled")?;
            raw.split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect()
        }
        None => vec!["127.0.0.1/32".to_string(), "::1/128".to_string()],
    };
    let control_plane_proof = if profile == InstallProfile::ControlPlane {
        Some(prepare_control_plane_proxy_proof(
            install_root,
            options.trusted_proxy.shared_secret_file.as_deref(),
        )?)
    } else {
        None
    };
    let (shared_secret, shared_secret_file) = if let Some(proof) = &control_plane_proof {
        (None, Some(proof.path.display().to_string()))
    } else if let Some(path) = options.trusted_proxy.shared_secret_file.as_deref() {
        validate_secret_file(path, "Oore web-to-control-plane proof")?;
        (None, Some(path.display().to_string()))
    } else {
        (Some(generate_proxy_proof()), None)
    };

    super::handle_setup_init_via_daemon(
        SetupInitArgs {
            mode: SetupInitMode::TrustedProxy,
            owner_email,
            user_email_header: header.clone(),
            trusted_proxy_cidrs: cidrs,
            shared_secret,
            shared_secret_file,
            state_file: options.state_file.clone(),
            force: recover_idp_configured,
            json: false,
        },
        daemon_url,
        true,
    )
    .await?;

    if let Some(proof) = &control_plane_proof {
        print_control_plane_proxy_instructions(proof, &header, terminal)?;
    }
    Ok(())
}

fn prepare_control_plane_proxy_proof(
    install_root: &Path,
    configured: Option<&Path>,
) -> anyhow::Result<ControlPlaneProxyProof> {
    if let Some(path) = configured {
        validate_secret_file(path, "direct trusted proxy proof")?;
        return Ok(ControlPlaneProxyProof {
            path: path.to_path_buf(),
            source: ControlPlaneProxyProofSource::Supplied,
        });
    }

    let secrets = prepare_secrets_directory(install_root)?;
    let destination = secrets.join(CONTROL_PLANE_PROXY_PROOF_FILE);
    if let Some(value) = read_existing_private_value(&destination)? {
        let value = String::from_utf8(value)
            .context("the stored control-plane proxy proof is not UTF-8")?;
        nonempty(value.trim().to_string(), "stored control-plane proxy proof")?;
        return Ok(ControlPlaneProxyProof {
            path: destination,
            source: ControlPlaneProxyProofSource::Stored,
        });
    }

    write_private_value(
        &secrets,
        &destination,
        CONTROL_PLANE_PROXY_PROOF_FILE,
        &generate_proxy_proof(),
    )?;
    Ok(ControlPlaneProxyProof {
        path: destination,
        source: ControlPlaneProxyProofSource::Generated,
    })
}

fn print_control_plane_proxy_instructions(
    proof: &ControlPlaneProxyProof,
    identity_header: &str,
    terminal: Terminal,
) -> anyhow::Result<()> {
    let proof_note = match proof.source {
        ControlPlaneProxyProofSource::Supplied => {
            "Oore used the private proof file from your setup configuration."
        }
        ControlPlaneProxyProofSource::Stored => {
            "Oore reused the private proof stored by an earlier setup attempt."
        }
        ControlPlaneProxyProofSource::Generated => "Oore generated and stored this proof for you.",
    };
    terminal.note(
        "Connect your access proxy",
        format!(
            "{proof_note}\n\nConfigure the proxy that connects directly to this control plane to send these request headers:\n  Proof: x-oore-trusted-proxy-secret\n  Email: {identity_header}\n\nRead the proof value when you configure the proxy:\n  cat {}\n\nKeep the proof file private.",
            super::shell_word(&proof.path.display().to_string()),
        ),
    )?;
    Ok(())
}

async fn complete_access_in_browser(
    options: &JourneyOptions,
    daemon_url: &str,
    terminal: Terminal,
) -> anyhow::Result<bool> {
    let status = verify_daemon_instance(daemon_url).await?;
    if status.state == SetupState::Ready {
        ensure_persisted_access_matches(options.access, &status)?;
        return Ok(true);
    }
    if status.state == SetupState::OwnerCreated {
        ensure_persisted_access_matches(options.access, &status)?;
    }
    if needs_terminal_trusted_proxy_setup(&status) {
        terminal.note(
            "Continue in the terminal",
            "Trusted access proxy setup uses the terminal in this release. Run:\n\n  oore setup --interface terminal --access trusted-proxy",
        )?;
        return Ok(false);
    }

    let token = create_setup_token(daemon_url, options.state_file.as_deref()).await?;
    let web_url = setup_browser_url(&options.web_listen, &token)?;
    terminal.note(
        "Continue in your browser",
        format!("Open\n  {web_url}\n\nThis single-use link expires in 30 minutes."),
    )?;

    if terminal.is_remote_session() {
        let tunnel = ssh_tunnel_command(&options.web_listen, &options.daemon_listen)?;
        terminal.note(
            "Run this on your computer",
            format!("{tunnel}\n\nThen open\n  {web_url}"),
        )?;
    } else if !super::open_browser(web_url.as_str()) {
        terminal.note("Browser did not open", format!("Open\n  {web_url}"))?;
    }

    wait_until_setup_ready(daemon_url, options.access, terminal).await
}

async fn create_setup_token(daemon_url: &str, state_file: Option<&str>) -> anyhow::Result<String> {
    let response = super::call_operator(
        daemon_url,
        state_file,
        oore_contract::OperatorRequest::MintBootstrapToken {
            ttl_secs: SETUP_WAIT_TIMEOUT.as_secs(),
        },
    )
    .await?;
    match response {
        oore_contract::OperatorResponse::BootstrapToken { token, .. } => Ok(token),
        _ => anyhow::bail!("oored returned an unexpected bootstrap token response"),
    }
}

async fn wait_until_setup_ready(
    daemon_url: &str,
    requested_access: Option<SetupAccess>,
    terminal: Terminal,
) -> anyhow::Result<bool> {
    let client = super::endpoint_http_client_builder(daemon_url)
        .timeout(Duration::from_secs(5))
        .build()
        .context("failed to prepare the setup status check")?;
    let deadline = Instant::now() + SETUP_WAIT_TIMEOUT;
    let operation = terminal.operation("Waiting for browser setup to finish");
    loop {
        if let Ok(status) = super::fetch_setup_status(&client, daemon_url).await {
            if status.state == SetupState::Ready {
                if let Err(error) = ensure_persisted_access_matches(requested_access, &status) {
                    operation.failed("Browser setup selected different access");
                    return Err(error);
                }
                operation.done("Browser setup is complete");
                return Ok(true);
            }
            if needs_terminal_trusted_proxy_setup(&status) {
                operation.done("Browser setup is paused");
                terminal.note(
                    "Continue in the terminal",
                    "Trusted access proxy setup uses the terminal in this release. Run:\n\n  oore setup --interface terminal --access trusted-proxy",
                )?;
                return Ok(false);
            }
        }
        if Instant::now() >= deadline {
            operation.failed("Browser setup is still incomplete");
            anyhow::bail!("browser setup did not finish within 30 minutes")
        }
        sleep(Duration::from_secs(2)).await;
    }
}

fn needs_terminal_trusted_proxy_setup(status: &SetupStatus) -> bool {
    status.state != SetupState::Ready
        && status.runtime_mode == RuntimeMode::Remote
        && status.remote_auth_mode == RemoteAuthMode::TrustedProxy
}

fn setup_access_from_status(status: &SetupStatus) -> SetupAccess {
    match (status.runtime_mode, status.remote_auth_mode) {
        (RuntimeMode::Local, _) => SetupAccess::ThisDevice,
        (RuntimeMode::Remote, RemoteAuthMode::Oidc) => SetupAccess::IdentityProvider,
        (RuntimeMode::Remote, RemoteAuthMode::TrustedProxy) => SetupAccess::TrustedProxy,
    }
}

fn ensure_persisted_access_matches(
    requested: Option<SetupAccess>,
    status: &SetupStatus,
) -> anyhow::Result<()> {
    let Some(requested) = requested else {
        return Ok(());
    };
    if requested == setup_access_from_status(status) {
        return Ok(());
    }

    let requested_label = match requested {
        SetupAccess::ThisDevice => "this device (passwordless loopback)",
        SetupAccess::IdentityProvider => "identity provider (OIDC)",
        SetupAccess::TrustedProxy => "trusted access proxy",
    };
    let current_label = match (status.runtime_mode, status.remote_auth_mode) {
        (RuntimeMode::Local, _) => "this device (passwordless loopback)",
        (RuntimeMode::Remote, RemoteAuthMode::Oidc) => "identity provider (OIDC)",
        (RuntimeMode::Remote, RemoteAuthMode::TrustedProxy) => "trusted access proxy",
    };
    if status.state == SetupState::OwnerCreated {
        anyhow::bail!(
            "requested access is {requested_label}, but this instance already created its owner for {current_label}. Run `oore setup` without changing access to finish"
        );
    }
    anyhow::bail!(
        "requested access is {requested_label}, but this instance is already ready for {current_label}. Open Oore Settings to change access"
    )
}

async fn pair_frontend(backend_url: &str, code: &str) -> anyhow::Result<FrontendPairResponse> {
    let client = super::endpoint_http_client_builder(backend_url)
        .timeout(Duration::from_secs(15))
        .build()
        .context("failed to prepare frontend pairing")?;
    let response = client
        .post(format!(
            "{}/v1/frontend/pair",
            backend_url.trim_end_matches('/')
        ))
        .json(&FrontendPairRequest {
            code: code.to_string(),
        })
        .send()
        .await
        .context("failed to reach the control plane for frontend pairing")?;
    if response.status().is_success() {
        return response
            .json()
            .await
            .context("failed to read the frontend pairing response");
    }
    let status = response.status();
    let message = super::extract_error_message(response).await;
    anyhow::bail!(
        "frontend pairing failed (HTTP {}): {}",
        status.as_u16(),
        message
    )
}

fn publish_configuring(
    manifest: &mut InstallManifest,
    manifest_path: &Path,
    options: &JourneyOptions,
) -> anyhow::Result<()> {
    manifest.lifecycle.state = InstallState::Configuring;
    match manifest.profile {
        InstallProfile::Complete => {
            manifest.lifecycle.daemon_listen = Some(options.daemon_listen.clone());
            manifest.lifecycle.state_file = Some(
                super::resolve_db_path(options.state_file.as_deref())?
                    .display()
                    .to_string(),
            );
            manifest.lifecycle.web_listen = Some(options.web_listen.clone());
            manifest.lifecycle.backend_url = Some(local_daemon_url(&options.daemon_listen)?);
            manifest.lifecycle.browser_transport_protected = options.browser_transport_protected;
            manifest.lifecycle.backend_transport_protected = options.backend_transport_protected;
        }
        InstallProfile::ControlPlane => {
            manifest.lifecycle.daemon_listen = Some(options.daemon_listen.clone());
            manifest.lifecycle.state_file = Some(
                super::resolve_db_path(options.state_file.as_deref())?
                    .display()
                    .to_string(),
            );
            manifest.lifecycle.backend_url = Some(local_daemon_url(&options.daemon_listen)?);
            manifest.lifecycle.backend_transport_protected = options.backend_transport_protected;
        }
        InstallProfile::Runner | InstallProfile::WebNode | InstallProfile::CliOnly => {
            manifest.lifecycle.backend_url = options
                .backend_url
                .clone()
                .or_else(|| options.daemon_url.clone());
            if manifest.profile == InstallProfile::WebNode {
                manifest.lifecycle.web_listen = Some(options.web_listen.clone());
                manifest.lifecycle.browser_transport_protected =
                    options.browser_transport_protected;
            }
            manifest.lifecycle.backend_transport_protected = options.backend_transport_protected;
        }
    }
    for service in manifest.profile.services() {
        manifest.record_service(*service)?;
    }
    manifest.write_atomic(manifest_path)
}

async fn verify_profile_services(
    install_root: &Path,
    profile: InstallProfile,
) -> anyhow::Result<()> {
    for service in profile.services() {
        anyhow::ensure!(
            super::managed_services::service_is_owned(install_root, *service)?,
            "{} is not managed by this Oore installation",
            service.label()
        );
        super::managed_services::verify_service(install_root, *service).await?;
    }
    if matches!(
        profile,
        InstallProfile::Complete | InstallProfile::ControlPlane
    ) {
        anyhow::ensure!(
            super::managed_services::service_is_owned(install_root, InstallService::Updater)?,
            "{} is not managed by this Oore installation",
            InstallService::Updater.label()
        );
        super::managed_services::verify_service(install_root, InstallService::Updater).await?;
    }
    Ok(())
}

fn render_plan(
    profile: InstallProfile,
    options: &JourneyOptions,
    access: PlanAccessResolution,
) -> String {
    let interface = match options.interface {
        SetupInterface::Auto => "automatic",
        SetupInterface::Terminal => "terminal",
        SetupInterface::Browser => "browser",
    };
    let mut lines = vec![
        format!("Device role   {}", profile_label(profile)),
        format!("Interface     {interface}"),
    ];
    match access {
        PlanAccessResolution::NotApplicable => {}
        PlanAccessResolution::Selected(access) => {
            let label = match access {
                SetupAccess::ThisDevice => "Only this device (passwordless loopback)",
                SetupAccess::IdentityProvider => "Identity provider (OIDC)",
                SetupAccess::TrustedProxy => "Existing trusted access proxy",
            };
            lines.push(format!("Access        {label}"));
        }
        PlanAccessResolution::ChooseInBrowser => {
            lines.push(
                "Access        Choose in browser after local setup services start".to_string(),
            );
        }
        PlanAccessResolution::ResumePersisted => {
            lines.push(
                "Access        Resume saved setup after the local control plane starts".to_string(),
            );
        }
    }
    if matches!(
        profile,
        InstallProfile::Runner | InstallProfile::WebNode | InstallProfile::CliOnly
    ) && let Some(backend_url) = options.backend_url.as_deref()
    {
        lines.push(format!("Control plane {backend_url}"));
    }
    if matches!(
        profile,
        InstallProfile::Complete | InstallProfile::ControlPlane
    ) && parse_listen(&options.daemon_listen, "daemon listen address")
        .is_ok_and(|address| !address.ip().is_loopback())
    {
        lines.push("Transport     Protected private network confirmed".to_string());
    }
    lines.extend([String::new(), "Required work".to_string()]);
    match profile {
        InstallProfile::Complete => {
            lines.push(format!(
                "  Start the control plane on {}",
                options.daemon_listen
            ));
            lines.push("  Configure access and create the owner".to_string());
            lines.push(format!("  Start the web UI on {}", options.web_listen));
            lines.push("  Enroll and start the local runner".to_string());
        }
        InstallProfile::ControlPlane => {
            lines.push(format!(
                "  Start the control plane on {}",
                options.daemon_listen
            ));
            lines.push("  Configure access and create the owner".to_string());
        }
        InstallProfile::Runner => {
            lines.push("  Register this device with a control plane".to_string());
            lines.push("  Start the runner at boot".to_string());
        }
        InstallProfile::WebNode => {
            lines.push("  Connect this web node to a control plane".to_string());
            lines.push(format!("  Start the web UI on {}", options.web_listen));
        }
        InstallProfile::CliOnly => {
            lines.push("  Connect the CLI to a ready control plane".to_string());
            lines.push("  Validate the signed-in account".to_string());
        }
    }
    lines.push("  Verify the final result".to_string());
    lines.join("\n")
}

fn resolve_interface(
    requested: SetupInterface,
    access: Option<SetupAccess>,
    profile: InstallProfile,
    terminal: Terminal,
) -> SetupInterface {
    match requested {
        SetupInterface::Auto if terminal.is_remote_session() => SetupInterface::Terminal,
        SetupInterface::Auto if access == Some(SetupAccess::TrustedProxy) => {
            SetupInterface::Terminal
        }
        SetupInterface::Auto
            if profile == InstallProfile::Complete && terminal.is_interactive() =>
        {
            SetupInterface::Browser
        }
        SetupInterface::Auto => SetupInterface::Terminal,
        value => value,
    }
}

fn choose_access(
    selected: Option<SetupAccess>,
    terminal: Terminal,
) -> anyhow::Result<Option<SetupAccess>> {
    if let Some(selected) = selected {
        return Ok(Some(selected));
    }
    if !terminal.is_interactive() {
        return Ok(Some(SetupAccess::ThisDevice));
    }
    let choices = [
        SelectChoice::new(
            SetupAccess::ThisDevice,
            "Only on this device",
            "Best for a personal Mac or SSH-only build machine",
        ),
        SelectChoice::new(
            SetupAccess::IdentityProvider,
            "Through an identity provider",
            "For a team after you publish Oore through HTTPS",
        ),
        SelectChoice::new(
            SetupAccess::TrustedProxy,
            "Through an existing access proxy",
            "Your HTTPS proxy already authenticates people, such as Cloudflare Access",
        ),
    ];
    match terminal
        .select(
            "How should people access Oore?",
            choices,
            SetupAccess::ThisDevice,
        )
        .context("failed to read the access choice")?
    {
        PromptResult::Submitted(value) => Ok(Some(value)),
        PromptResult::Cancelled => Ok(None),
    }
}

fn required_value(
    value: Option<String>,
    terminal: Terminal,
    prompt: &str,
    default: Option<&str>,
    noninteractive_help: &str,
) -> anyhow::Result<Option<String>> {
    if let Some(value) = value {
        return Ok(Some(nonempty(value, prompt)?));
    }
    if !terminal.is_interactive() {
        if let Some(default) = default {
            return Ok(Some(default.to_string()));
        }
        anyhow::bail!("{prompt} is required; {noninteractive_help}");
    }
    match terminal
        .input(prompt, default, true)
        .with_context(|| format!("failed to read {prompt}"))?
    {
        PromptResult::Submitted(value) => Ok(Some(nonempty(value, prompt)?)),
        PromptResult::Cancelled => Ok(None),
    }
}

fn optional_value(
    value: Option<String>,
    terminal: Terminal,
    prompt: &str,
    default: Option<&str>,
) -> anyhow::Result<Option<String>> {
    if value.is_some() || !terminal.is_interactive() {
        return Ok(value.or_else(|| default.map(ToString::to_string)));
    }
    match terminal
        .input(prompt, default, false)
        .with_context(|| format!("failed to read {prompt}"))?
    {
        PromptResult::Submitted(value) => Ok(Some(value)),
        PromptResult::Cancelled => Ok(None),
    }
}

fn required_secret(
    value: Option<String>,
    terminal: Terminal,
    prompt: &str,
    noninteractive_help: &str,
) -> anyhow::Result<Option<String>> {
    if let Some(value) = value {
        return Ok(Some(nonempty(value, prompt)?));
    }
    if !terminal.is_interactive() {
        anyhow::bail!("{prompt} is required; {noninteractive_help}");
    }
    match terminal
        .password(prompt, false)
        .with_context(|| format!("failed to read {prompt}"))?
    {
        PromptResult::Submitted(value) => Ok(Some(nonempty(value, prompt)?)),
        PromptResult::Cancelled => Ok(None),
    }
}

fn finish_operation<T>(
    operation: oore_cli_ui::Operation,
    result: anyhow::Result<T>,
    success: &str,
) -> anyhow::Result<T> {
    match result {
        Ok(value) => {
            operation.done(success);
            Ok(value)
        }
        Err(error) => {
            operation.failed("Setup step failed");
            Err(error)
        }
    }
}

fn load_manifest(path: &Path) -> anyhow::Result<InstallManifest> {
    InstallManifest::load(path).with_context(|| {
        format!(
            "could not load the device role from {}; run `oore install` first or repair this manifest",
            path.display()
        )
    })
}

fn load_setup_file(path: Option<&Path>) -> anyhow::Result<SetupFile> {
    let Some(path) = path else {
        return Ok(SetupFile::default());
    };
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect setup config {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && metadata.uid() == super::current_effective_uid()
            && metadata.mode() & 0o077 == 0,
        "setup config must be a private regular file owned by the current user"
    );
    anyhow::ensure!(
        metadata.len() <= CONFIG_FILE_LIMIT,
        "setup config is larger than 1 MiB"
    );
    let contents = fs::read(path)
        .with_context(|| format!("failed to read setup config {}", path.display()))?;
    let mut config = match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => serde_json::from_slice(&contents)
            .with_context(|| format!("invalid JSON setup config {}", path.display()))?,
        Some("yaml" | "yml") => serde_yaml::from_slice(&contents)
            .with_context(|| format!("invalid YAML setup config {}", path.display()))?,
        _ => parse_unknown_config_format(&contents, path)?,
    };
    resolve_config_paths(&mut config, path)?;
    Ok(config)
}

fn parse_unknown_config_format(contents: &[u8], path: &Path) -> anyhow::Result<SetupFile> {
    match serde_json::from_slice(contents) {
        Ok(config) => Ok(config),
        Err(json_error) => match serde_yaml::from_slice(contents) {
            Ok(config) => Ok(config),
            Err(yaml_error) => anyhow::bail!(
                "setup config {} is neither valid JSON ({json_error}) nor valid YAML ({yaml_error})",
                path.display()
            ),
        },
    }
}

fn resolve_config_paths(config: &mut SetupFile, source: &Path) -> anyhow::Result<()> {
    let base = source
        .parent()
        .context("setup config path has no parent directory")?;
    resolve_relative_config_path(config.runner_token_file.as_mut(), base);
    resolve_relative_config_path(config.session_token_file.as_mut(), base);
    if let Some(proxy) = config.trusted_proxy.as_mut() {
        resolve_relative_config_path(proxy.shared_secret_file.as_mut(), base);
        resolve_relative_config_path(proxy.upstream_shared_secret_file.as_mut(), base);
    }
    if let Some(state_file) = config.state_file.as_mut() {
        let path = Path::new(state_file);
        if path.is_relative() {
            *state_file = base.join(path).display().to_string();
        }
    }
    Ok(())
}

fn resolve_relative_config_path(path: Option<&mut PathBuf>, base: &Path) {
    if let Some(path) = path
        && path.is_relative()
    {
        *path = base.join(&*path);
    }
}

fn read_optional_secret(path: Option<&Path>, label: &str) -> anyhow::Result<Option<String>> {
    path.map(|path| read_secret(path, label)).transpose()
}

fn read_secret(path: &Path, label: &str) -> anyhow::Result<String> {
    validate_secret_file(path, label)?;
    let value = fs::read_to_string(path)
        .with_context(|| format!("failed to read {label} file {}", path.display()))?;
    nonempty(value.trim().to_string(), label)
}

fn validate_secret_file(path: &Path, label: &str) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {label} file {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && metadata.uid() == super::current_effective_uid()
            && metadata.mode() & 0o077 == 0,
        "{label} file {} must be a regular file owned by the current user with mode 0600 or stricter",
        path.display()
    );
    Ok(())
}

fn prepare_secrets_directory(install_root: &Path) -> anyhow::Result<PathBuf> {
    let secrets = install_root.join("secrets");
    match fs::symlink_metadata(&secrets) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&secrets)
                .with_context(|| format!("failed to create {}", secrets.display()))?;
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", secrets.display()));
        }
    }
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(&secrets)
        .with_context(|| format!("failed to open {} safely", secrets.display()))?;
    let metadata = directory
        .metadata()
        .with_context(|| format!("failed to inspect {}", secrets.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_dir() && metadata.uid() == super::current_effective_uid(),
        "the Oore secrets path must be a real directory owned by the current user"
    );
    directory
        .set_permissions(fs::Permissions::from_mode(0o700))
        .with_context(|| format!("failed to secure {}", secrets.display()))?;
    Ok(secrets)
}

fn stage_private_value(
    secrets: &Path,
    name: &str,
    value: &str,
) -> anyhow::Result<tempfile::NamedTempFile> {
    let value = nonempty(value.to_string(), name)?;
    let mut staged = tempfile::NamedTempFile::new_in(secrets)
        .with_context(|| format!("failed to stage {name}"))?;
    staged
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    staged.write_all(value.as_bytes())?;
    staged.write_all(b"\n")?;
    staged.as_file_mut().sync_all()?;
    Ok(staged)
}

fn read_existing_private_value(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    };
    anyhow::ensure!(
        metadata.file_type().is_file()
            && metadata.uid() == super::current_effective_uid()
            && metadata.mode() & 0o077 == 0,
        "{} must be a private regular file owned by the current user",
        path.display()
    );
    fs::read(path)
        .map(Some)
        .with_context(|| format!("failed to read {}", path.display()))
}

fn preflight_private_pair(
    install_root: &Path,
    first_name: &str,
    second_name: &str,
    backend_url: &str,
) -> anyhow::Result<WebPairingState> {
    let secrets = prepare_secrets_directory(install_root)?;
    let first_exists = read_existing_private_value(&secrets.join(first_name))?.is_some();
    let second_exists = read_existing_private_value(&secrets.join(second_name))?.is_some();
    let binding = read_existing_private_value(&secrets.join(WEB_BACKEND_URL_FILE))?;
    anyhow::ensure!(
        first_exists == second_exists,
        "frontend pairing is incomplete; remove both pairing files or provide a new pairing code"
    );
    if !first_exists {
        anyhow::ensure!(
            binding.is_none(),
            "frontend pairing has a backend binding without its proof files"
        );
        return Ok(WebPairingState::Absent);
    }
    let Some(binding) = binding else {
        return Ok(WebPairingState::DifferentBackend);
    };
    let binding =
        String::from_utf8(binding).context("the stored frontend backend binding is not UTF-8")?;
    let bound_url = parse_http_url(binding.trim(), "stored frontend backend URL")?;
    let requested_url = parse_http_url(backend_url, "control-plane URL")?;
    if bound_url == requested_url {
        Ok(WebPairingState::Bound)
    } else {
        Ok(WebPairingState::DifferentBackend)
    }
}

fn web_secret_paths(secrets: &Path) -> [PathBuf; 4] {
    [
        secrets.join(WEB_PROOF_FILE),
        secrets.join(WEB_IDENTITY_HEADER_FILE),
        secrets.join(WEB_BACKEND_URL_FILE),
        secrets.join(WEB_UPSTREAM_PROOF_FILE),
    ]
}

fn capture_web_secrets(install_root: &Path) -> anyhow::Result<WebSecretsSnapshot> {
    let directory = prepare_secrets_directory(install_root)?;
    let values = web_secret_paths(&directory)
        .into_iter()
        .map(|path| {
            let value = read_existing_private_value(&path)?;
            Ok((path, value))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(WebSecretsSnapshot { directory, values })
}

fn restore_web_secrets(snapshot: &WebSecretsSnapshot) -> anyhow::Result<()> {
    let mut failures = Vec::new();
    for (path, value) in &snapshot.values {
        if let Err(error) = restore_private_value(&snapshot.directory, path, value.as_deref()) {
            failures.push(format!("{}: {error:#}", path.display()));
        }
    }
    anyhow::ensure!(
        failures.is_empty(),
        "failed to restore web credentials: {}",
        failures.join("; ")
    );
    Ok(())
}

fn clear_web_proxy_credentials(install_root: &Path) -> anyhow::Result<()> {
    let secrets = prepare_secrets_directory(install_root)?;
    for path in web_secret_paths(&secrets) {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("failed to remove {}", path.display()));
            }
        }
    }
    File::open(&secrets)?
        .sync_all()
        .context("failed to sync the cleared web credentials")
}

fn restore_private_value(
    secrets: &Path,
    destination: &Path,
    previous: Option<&[u8]>,
) -> anyhow::Result<()> {
    if let Some(previous) = previous {
        let mut staged = tempfile::NamedTempFile::new_in(secrets)
            .context("failed to stage the previous frontend proof")?;
        staged
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
        staged.write_all(previous)?;
        staged.as_file_mut().sync_all()?;
        staged
            .persist(destination)
            .map_err(|error| error.error)
            .with_context(|| format!("failed to restore {}", destination.display()))?;
    } else {
        match fs::remove_file(destination) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to remove {}", destination.display()));
            }
        }
    }
    File::open(secrets)?.sync_all()?;
    Ok(())
}

fn private_value_publish_error(
    error: anyhow::Error,
    secrets: &Path,
    destination: &Path,
    previous: Option<&[u8]>,
) -> anyhow::Error {
    match restore_private_value(secrets, destination, previous) {
        Ok(()) => error.context("the previous private value was restored"),
        Err(rollback) => anyhow::anyhow!(
            "{error:#}; restoring the previous private value also failed: {rollback:#}"
        ),
    }
}

fn write_private_value(
    secrets: &Path,
    destination: &Path,
    name: &str,
    value: &str,
) -> anyhow::Result<()> {
    let previous = read_existing_private_value(destination)?;
    let staged = stage_private_value(secrets, name, value)?;
    let published = match staged.persist(destination) {
        Ok(file) => file,
        Err(error) => {
            let error = anyhow::Error::new(error.error)
                .context(format!("failed to save {}", destination.display()));
            return Err(private_value_publish_error(
                error,
                secrets,
                destination,
                previous.as_deref(),
            ));
        }
    };
    if let Err(error) = published.sync_all() {
        return Err(private_value_publish_error(
            anyhow::Error::new(error).context(format!("failed to sync {}", destination.display())),
            secrets,
            destination,
            previous.as_deref(),
        ));
    }
    if let Err(error) = File::open(secrets).and_then(|directory| directory.sync_all()) {
        return Err(private_value_publish_error(
            anyhow::Error::new(error).context(format!("failed to sync {}", secrets.display())),
            secrets,
            destination,
            previous.as_deref(),
        ));
    }
    Ok(())
}

fn restore_private_pair(
    secrets: &Path,
    first_destination: &Path,
    previous_first: Option<&[u8]>,
    second_destination: &Path,
    previous_second: Option<&[u8]>,
) -> anyhow::Result<()> {
    let first = restore_private_value(secrets, first_destination, previous_first);
    let second = restore_private_value(secrets, second_destination, previous_second);
    match (first, second) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(first), Ok(())) => Err(first),
        (Ok(()), Err(second)) => Err(second),
        (Err(first), Err(second)) => {
            anyhow::bail!("first restore failed ({first:#}); second restore failed ({second:#})")
        }
    }
}

fn pair_publish_error(
    error: anyhow::Error,
    secrets: &Path,
    first_destination: &Path,
    previous_first: Option<&[u8]>,
    second_destination: &Path,
    previous_second: Option<&[u8]>,
) -> anyhow::Error {
    match restore_private_pair(
        secrets,
        first_destination,
        previous_first,
        second_destination,
        previous_second,
    ) {
        Ok(()) => error.context("the previous frontend pairing files were restored"),
        Err(rollback) => anyhow::anyhow!(
            "{error:#}; restoring the previous frontend pairing files also failed: {rollback:#}"
        ),
    }
}

fn write_private_pair(
    install_root: &Path,
    first: (&str, &str),
    second: (&str, &str),
) -> anyhow::Result<()> {
    let secrets = prepare_secrets_directory(install_root)?;
    let first_destination = secrets.join(first.0);
    let second_destination = secrets.join(second.0);
    let previous_first = read_existing_private_value(&first_destination)?;
    let previous_second = read_existing_private_value(&second_destination)?;

    let first_staged = stage_private_value(&secrets, first.0, first.1)?;
    let second_staged = stage_private_value(&secrets, second.0, second.1)?;
    let first_file = match first_staged.persist(&first_destination) {
        Ok(file) => file,
        Err(error) => {
            let error = anyhow::Error::new(error.error)
                .context(format!("failed to save {}", first_destination.display()));
            return Err(pair_publish_error(
                error,
                &secrets,
                &first_destination,
                previous_first.as_deref(),
                &second_destination,
                previous_second.as_deref(),
            ));
        }
    };
    if let Err(error) = first_file.sync_all() {
        return Err(pair_publish_error(
            anyhow::Error::new(error).context("failed to sync the frontend proof"),
            &secrets,
            &first_destination,
            previous_first.as_deref(),
            &second_destination,
            previous_second.as_deref(),
        ));
    }

    let second_file = match second_staged.persist(&second_destination) {
        Ok(file) => file,
        Err(error) => {
            let error = anyhow::Error::new(error.error)
                .context(format!("failed to save {}", second_destination.display()));
            return Err(pair_publish_error(
                error,
                &secrets,
                &first_destination,
                previous_first.as_deref(),
                &second_destination,
                previous_second.as_deref(),
            ));
        }
    };
    if let Err(error) = second_file.sync_all() {
        return Err(pair_publish_error(
            anyhow::Error::new(error).context("failed to sync the frontend identity header"),
            &secrets,
            &first_destination,
            previous_first.as_deref(),
            &second_destination,
            previous_second.as_deref(),
        ));
    }
    if let Err(error) = File::open(&secrets).and_then(|directory| directory.sync_all()) {
        return Err(pair_publish_error(
            anyhow::Error::new(error).context("failed to sync the frontend pairing directory"),
            &secrets,
            &first_destination,
            previous_first.as_deref(),
            &second_destination,
            previous_second.as_deref(),
        ));
    }
    Ok(())
}

fn write_bound_web_pairing(
    install_root: &Path,
    backend_url: &str,
    pairing: &FrontendPairResponse,
) -> anyhow::Result<()> {
    write_private_pair(
        install_root,
        (WEB_PROOF_FILE, &pairing.backend_proof),
        (WEB_IDENTITY_HEADER_FILE, &pairing.user_email_header),
    )?;
    let secrets = prepare_secrets_directory(install_root)?;
    let destination = secrets.join(WEB_BACKEND_URL_FILE);
    let backend_url = parse_http_url(backend_url, "control-plane URL")?.to_string();
    write_private_value(&secrets, &destination, WEB_BACKEND_URL_FILE, &backend_url)
}

fn prefer_config_default(cli: String, default: &str, config: Option<String>) -> String {
    if cli == default {
        config.unwrap_or(cli)
    } else {
        cli
    }
}

fn parse_listen(value: &str, label: &str) -> anyhow::Result<SocketAddr> {
    value
        .parse()
        .with_context(|| format!("{label} must use the form IP:port"))
}

fn valid_managed_daemon_ip(ip: IpAddr) -> bool {
    !ip.is_unspecified()
        && !ip.is_multicast()
        && !matches!(ip, IpAddr::V4(address) if address.is_broadcast())
}

fn validate_managed_daemon_listen(address: SocketAddr, protected: bool) -> anyhow::Result<()> {
    anyhow::ensure!(
        valid_managed_daemon_ip(address.ip()),
        "the managed control plane must listen on a loopback or concrete unicast address"
    );
    anyhow::ensure!(
        address.ip().is_loopback() || protected,
        "a non-loopback managed control plane requires --backend-transport-protected and a separately protected private network"
    );
    Ok(())
}

fn local_daemon_url(listen: &str) -> anyhow::Result<String> {
    let address = parse_listen(listen, "daemon listen address")?;
    let ip = match address.ip() {
        IpAddr::V4(_) => IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        IpAddr::V6(_) => IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
    };
    Ok(format!("http://{}", SocketAddr::new(ip, address.port())))
}

fn local_url(listen: &str, label: &str) -> anyhow::Result<String> {
    let address = parse_listen(listen, label)?;
    let ip = match address.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    Ok(format!("http://{}", SocketAddr::new(ip, address.port())))
}

fn parse_http_url(value: &str, label: &str) -> anyhow::Result<Url> {
    let url = Url::parse(value).with_context(|| format!("{label} is invalid"))?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
        "{label} must use http or https and include a host"
    );
    anyhow::ensure!(
        url.username().is_empty() && url.password().is_none(),
        "{label} must not contain credentials"
    );
    anyhow::ensure!(
        url.query().is_none() && url.fragment().is_none(),
        "{label} must not contain a query or fragment"
    );
    anyhow::ensure!(url.path() == "/", "{label} must use the root path");
    Ok(url)
}

fn canonical_http_url(value: &str, label: &str) -> anyhow::Result<String> {
    Ok(parse_http_url(value, label)?
        .to_string()
        .trim_end_matches('/')
        .to_string())
}

pub(super) fn same_http_endpoint(left: &str, right: &str) -> anyhow::Result<bool> {
    Ok(parse_http_url(left, "stored control-plane URL")?
        == parse_http_url(right, "control-plane URL")?)
}

fn validate_backend_transport(value: &str, protected_http: bool) -> anyhow::Result<()> {
    let url = parse_http_url(value, "control-plane URL")?;
    if url.scheme() == "https" || is_loopback_host(&url) || protected_http {
        return Ok(());
    }
    anyhow::bail!(
        "remote control-plane URLs must use HTTPS; use --backend-transport-protected only for a separately protected private network"
    )
}

fn is_loopback_url(value: &str) -> anyhow::Result<bool> {
    Ok(is_loopback_host(&parse_http_url(
        value,
        "control-plane URL",
    )?))
}

fn is_loopback_host(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    })
}

fn setup_browser_url(web_listen: &str, bootstrap_token: &str) -> anyhow::Result<Url> {
    let web_base = local_url(web_listen, "web listen address")?;
    let mut url = Url::parse(&web_base)?;
    url.set_path("/setup");
    url.query_pairs_mut().append_pair("backend", &web_base);
    let fragment = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("bootstrap_token", bootstrap_token)
        .finish();
    url.set_fragment(Some(&fragment));
    Ok(url)
}

fn ssh_tunnel_command(web_listen: &str, daemon_listen: &str) -> anyhow::Result<String> {
    let (target, server_port) = super::ssh_session_target()?;
    let web_port = parse_listen(web_listen, "web listen address")?.port();
    let daemon_port = parse_listen(daemon_listen, "daemon listen address")?.port();
    Ok(format!(
        "ssh -o ExitOnForwardFailure=yes -p {server_port} -L {web_port}:127.0.0.1:{web_port} -L {daemon_port}:127.0.0.1:{daemon_port} {target}"
    ))
}

fn ssh_web_tunnel_command(web_listen: &str) -> anyhow::Result<String> {
    let (target, server_port) = super::ssh_session_target()?;
    let web_address = parse_listen(web_listen, "web listen address")?;
    let destination = match web_address.ip() {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    };
    let web_port = web_address.port();
    Ok(format!(
        "ssh -o ExitOnForwardFailure=yes -p {server_port} -L {web_port}:{destination}:{web_port} {target}"
    ))
}

async fn verify_daemon_instance(daemon_url: &str) -> anyhow::Result<SetupStatus> {
    let client = super::endpoint_http_client_builder(daemon_url)
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to prepare the setup status check")?;
    super::fetch_setup_status(&client, daemon_url).await
}

async fn require_ready_backend(backend_url: &str) -> anyhow::Result<SetupStatus> {
    let status = verify_daemon_instance(backend_url)
        .await
        .with_context(|| format!("could not read setup status from {backend_url}"))?;
    anyhow::ensure!(
        status.state == SetupState::Ready,
        "the selected control plane is not ready yet"
    );
    Ok(status)
}

async fn verify_setup_ready(daemon_url: &str) -> anyhow::Result<()> {
    let status = verify_daemon_instance(daemon_url).await?;
    anyhow::ensure!(
        status.state == SetupState::Ready,
        "instance setup is still {}; run `oore setup` again to continue",
        status.state
    );
    Ok(())
}

fn nonempty(value: String, label: &str) -> anyhow::Result<String> {
    let trimmed = value.trim();
    anyhow::ensure!(!trimmed.is_empty(), "{label} cannot be empty");
    Ok(trimmed.to_string())
}

fn nonempty_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn generate_proxy_proof() -> String {
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn profile_label(profile: InstallProfile) -> &'static str {
    match profile {
        InstallProfile::Complete => "Complete",
        InstallProfile::ControlPlane => "Control plane",
        InstallProfile::Runner => "Runner",
        InstallProfile::WebNode => "Web node",
        InstallProfile::CliOnly => "CLI only",
    }
}

async fn print_ready(
    manifest: &InstallManifest,
    manifest_path: &Path,
    json: bool,
    terminal: Terminal,
) -> anyhow::Result<()> {
    let backend_url = manifest
        .lifecycle
        .backend_url
        .as_deref()
        .context("the ready installation is missing its control-plane URL")?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "profile": manifest.profile,
                "state": manifest.lifecycle.state,
                "manifest": manifest_path,
                "services": manifest.lifecycle.services,
                "backend_url": backend_url,
            }))?
        );
        return Ok(());
    }
    if manifest.profile == InstallProfile::Complete {
        let status = verify_daemon_instance(backend_url).await?;
        if status.runtime_mode == RuntimeMode::Remote
            && status.remote_auth_mode == RemoteAuthMode::TrustedProxy
        {
            terminal.outro(
                "Complete setup is ready.\n\nFinish the HTTPS route through your trusted access proxy. Then open Oore at that proxy's HTTPS address.\n\nThe local web service remains on loopback and is not the user-facing address.\n\nRun `oore status` to inspect the instance.",
            )?;
            return Ok(());
        }

        let web_listen = manifest
            .lifecycle
            .web_listen
            .as_deref()
            .context("the Complete profile is missing its web listen address")?;
        let web_url = local_url(web_listen, "web listen address")?;
        let message = if terminal.is_remote_session() {
            let daemon_listen = manifest
                .lifecycle
                .daemon_listen
                .as_deref()
                .context("the Complete profile is missing its daemon listen address")?;
            let tunnel = ssh_tunnel_command(web_listen, daemon_listen)?;
            format!(
                "Complete setup is ready.\n\nRun this on your computer and keep it open:\n  {tunnel}\n\nThen open Oore:\n  {web_url}\n\nRun `oore status` to inspect the instance."
            )
        } else {
            format!(
                "Complete setup is ready.\n\nOpen Oore:\n  {web_url}\n\nRun `oore status` to inspect the instance."
            )
        };
        terminal.outro(message)?;
        return Ok(());
    }

    let message = match manifest.profile {
        InstallProfile::ControlPlane => format!(
            "Control plane setup is ready.\n\nControl plane:\n  {backend_url}\n\nNext: run `oore status` to inspect this instance."
        ),
        InstallProfile::Runner => format!(
            "Runner setup is ready.\n\nConnected control plane:\n  {backend_url}\n\nNext: run `oore status` to inspect the connected instance."
        ),
        InstallProfile::WebNode => {
            let web_listen = manifest
                .lifecycle
                .web_listen
                .as_deref()
                .context("the Web node profile is missing its web listen address")?;
            let web_url = local_url(web_listen, "web listen address")?;
            let web_address = parse_listen(web_listen, "web listen address")?;
            if !web_address.ip().is_loopback() {
                format!(
                    "Web node setup is ready.\n\nWeb service:\n  {web_url}\n\nConnected control plane:\n  {backend_url}\n\nNext: finish the protected HTTPS route, then open its HTTPS address."
                )
            } else if terminal.is_remote_session() {
                let tunnel = ssh_web_tunnel_command(web_listen)?;
                format!(
                    "Web node setup is ready.\n\nRun this on your computer and keep it open:\n  {tunnel}\n\nThen open Oore:\n  {web_url}\n\nConnected control plane:\n  {backend_url}"
                )
            } else {
                format!(
                    "Web node setup is ready.\n\nOpen Oore:\n  {web_url}\n\nConnected control plane:\n  {backend_url}"
                )
            }
        }
        InstallProfile::CliOnly => format!(
            "CLI-only setup is ready.\n\nConnected control plane:\n  {backend_url}\n\nNext: run `oore status`."
        ),
        InstallProfile::Complete => unreachable!(),
    };
    terminal.outro(message)?;
    Ok(())
}
