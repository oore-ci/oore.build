use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::net::SocketAddr;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};
use std::{ffi::OsStr, fs};

use anyhow::Context;
use clap::{Parser, Subcommand};
use oored::build_router_with_recovery;
use oored::crypto;
use oored::local_recovery::{ManagementSocket, RecoveryCapabilityStore, management_socket_path};
use oored::observability;
use oored::store::SetupStore;
use tracing::{error, info};

// ── CLI ──────────────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(name = "oored")]
#[command(about = "oore daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Run(RunArgs),
    InstallService(InstallServiceArgs),
    UninstallService(UninstallServiceArgs),
    Version,
    #[command(hide = true)]
    PackageVersion,
}

#[derive(Debug, clap::Args)]
struct RunArgs {
    #[arg(long, env = "OORED_LISTEN_ADDR", default_value = "127.0.0.1:8787")]
    listen: String,

    /// Path to the setup database file (overrides OORE_SETUP_STATE_FILE and default).
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,
}

#[derive(Debug, clap::Args)]
struct InstallServiceArgs {
    #[arg(long, env = "OORED_LISTEN_ADDR", default_value = "127.0.0.1:8787")]
    listen: String,

    /// Path to the setup database file used by the launchd service.
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,

    /// launchd label to install.
    #[arg(long, default_value = "build.oore.oored")]
    label: String,

    /// Add or override environment variables in the launchd plist.
    ///
    /// Use KEY=VALUE. Repeat for multiple values.
    #[arg(long = "env", value_name = "KEY=VALUE")]
    env: Vec<String>,

    /// Write the launchd plist without starting the service.
    #[arg(long)]
    no_start: bool,

    /// Install a boot-time LaunchDaemon instead of a GUI-session LaunchAgent.
    #[arg(long)]
    system: bool,

    /// User account that runs a system LaunchDaemon.
    #[arg(long, requires = "system")]
    user: Option<String>,
}

#[derive(Debug, clap::Args)]
struct UninstallServiceArgs {
    /// launchd label to remove.
    #[arg(long, default_value = "build.oore.oored")]
    label: String,

    /// Remove the boot-time LaunchDaemon instead of the user LaunchAgent.
    #[arg(long)]
    system: bool,
}

// ── Server bootstrap ─────────────────────────────────────────────

async fn run_server(args: RunArgs) -> anyhow::Result<()> {
    // Initialise tracing (+ optional OTel layer when OTEL_EXPORTER_OTLP_ENDPOINT is set)
    let otel_provider = observability::init_tracing();

    // Install the Prometheus metrics recorder
    let metrics_handle = observability::init_metrics();

    let addr: SocketAddr = args
        .listen
        .parse()
        .with_context(|| format!("invalid listen address: {}", args.listen))?;

    // Resolve database path and connect store
    let db_path = SetupStore::resolve_path(args.state_file.as_deref())
        .context("failed to resolve database path")?;
    info!(path = %db_path.display(), "using database");

    let store = SetupStore::connect(db_path.clone())
        .await
        .context("failed to connect to database")?;
    let initial = store
        .init_if_missing()
        .await
        .context("failed to initialise database")?;

    info!(
        instance_id = %initial.instance_id,
        state = ?initial.setup_state,
        "database ready"
    );

    // Backfill owner user if upgrading from before migration 002
    store
        .ensure_owner_user()
        .await
        .context("failed to ensure owner user")?;

    // Load or generate the AES-256 encryption key for secrets at rest.
    // Key storage mode is persisted in instance preferences and applied on restart.
    let key_storage_mode = oored::instance_settings::load_key_storage_mode(store.pool())
        .await
        .context("failed to load key storage mode preference")?;
    let runtime_key = crypto::load_runtime_key_with_mode(key_storage_mode)
        .context("failed to load runtime encryption key")?;
    info!(
        mode = %key_storage_mode,
        source = runtime_key.source.as_str(),
        legacy_file_path = %runtime_key.legacy_file_path.display(),
        "encryption key ready"
    );

    let recovery_pool = store.pool().clone();
    let recovery_capabilities = RecoveryCapabilityStore::default();
    let app = build_router_with_recovery(
        store,
        runtime_key.key,
        metrics_handle,
        recovery_capabilities.clone(),
    )
    .await
    .context("failed to build daemon router")?;
    let management_socket = ManagementSocket::bind(
        management_socket_path(&db_path)?,
        recovery_pool,
        recovery_capabilities.clone(),
    )
    .await
    .context("failed to start local recovery management socket")?;
    info!(
        path = %management_socket.path().display(),
        "local recovery management socket ready"
    );
    let management_task = tokio::spawn(async move {
        if let Err(error) = management_socket.serve().await {
            error!(%error, "local recovery management socket stopped");
        }
    });

    info!(listen = %addr, "starting oored daemon");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    if let Some(loopback_addr) = loopback_companion_addr(addr) {
        let loopback_listener = tokio::net::TcpListener::bind(loopback_addr)
            .await
            .with_context(|| {
                format!("failed to bind loopback companion address: {loopback_addr}")
            })?;
        let loopback_app = app.clone();

        info!(listen = %loopback_addr, "also listening on loopback for local runner and CLI access");
        tokio::spawn(async move {
            if let Err(error) = axum::serve(
                loopback_listener,
                loopback_app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            {
                error!(%error, "loopback oored listener stopped");
            }
        });
    }

    validate_runner_mode(std::env::var("OORED_RUNNER_MODE"))
        .context("failed to initialize embedded runner")?;
    info!("embedded runner disabled; use an external Direct macOS runner");

    let server_result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await;

    management_task.abort();
    recovery_capabilities.clear().await;

    // Best-effort flush of OTel spans on shutdown
    observability::shutdown_tracing(otel_provider);
    server_result.context("oored server failed")?;

    Ok(())
}

fn loopback_companion_addr(addr: SocketAddr) -> Option<SocketAddr> {
    let ip = addr.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then(|| oored::loopback_maintenance_addr(addr))
}

fn validate_runner_mode(value: Result<String, std::env::VarError>) -> anyhow::Result<()> {
    let raw = match value {
        Ok(raw) => Some(raw),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => return Err(error.into()),
    };
    match raw
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        None | Some("external") => Ok(()),
        Some("embedded" | "hybrid") => anyhow::bail!(
            "embedded runner execution is disabled; use an external Direct macOS runner"
        ),
        Some(raw) => anyhow::bail!("invalid OORED_RUNNER_MODE: {raw:?}"),
    }
}

fn read_trimmed_file(path: &std::path::Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn resolve_install_root() -> anyhow::Result<PathBuf> {
    if let Ok(val) = std::env::var("OORE_INSTALL_ROOT") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(bin_dir) = exe.parent()
        && bin_dir.file_name() == Some(OsStr::new("bin"))
        && let Some(root) = bin_dir.parent()
    {
        return Ok(root.to_path_buf());
    }

    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".oore"))
}

fn refuse_manifest_managed_service_command(
    command: &str,
    supported_command: &str,
) -> anyhow::Result<()> {
    let executable = std::env::current_exe().context("failed to resolve current executable")?;
    let Some(bin_dir) = executable.parent() else {
        return Ok(());
    };
    if bin_dir.file_name() != Some(OsStr::new("bin")) {
        return Ok(());
    }
    let Some(install_root) = bin_dir.parent() else {
        return Ok(());
    };
    let manifest = install_root.join("install-manifest.json");
    match fs::symlink_metadata(&manifest) {
        Ok(_) => anyhow::bail!(
            "`oored {command}` is not available for a profile-managed installation. Run `{supported_command}` instead."
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to inspect installation manifest {}",
                manifest.display()
            )
        }),
    }
}

fn validate_launchd_label(label: &str) -> anyhow::Result<()> {
    let valid = !label.is_empty()
        && label
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'));
    if valid {
        Ok(())
    } else {
        anyhow::bail!(
            "invalid launchd label: {label}. Use letters, numbers, dots, dashes, or underscores."
        )
    }
}

fn launch_agent_dir() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join("Library").join("LaunchAgents"))
}

fn launch_agent_plist_path(label: &str) -> anyhow::Result<PathBuf> {
    validate_launchd_label(label)?;
    Ok(launch_agent_dir()?.join(format!("{label}.plist")))
}

fn launch_daemon_plist_path(label: &str) -> anyhow::Result<PathBuf> {
    validate_launchd_label(label)?;
    Ok(PathBuf::from("/Library/LaunchDaemons").join(format!("{label}.plist")))
}

fn xml_escape(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn parse_env_assignment(raw: &str) -> anyhow::Result<(String, String)> {
    let (key, value) = raw
        .split_once('=')
        .with_context(|| format!("invalid --env value: {raw}. Expected KEY=VALUE."))?;
    let valid_key = !key.is_empty()
        && key.chars().enumerate().all(|(index, ch)| {
            ch == '_' || ch.is_ascii_alphabetic() || (index > 0 && ch.is_ascii_digit())
        });
    if !valid_key {
        anyhow::bail!("invalid environment variable name in --env: {key}");
    }
    Ok((key.to_string(), value.to_string()))
}

fn service_environment(overrides: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();
    let path = std::env::var("PATH").unwrap_or_else(|_| {
        "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".into()
    });
    env.insert("PATH".to_string(), path);
    env.insert(
        "RUST_LOG".to_string(),
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
    );

    for key in [
        "HOME",
        "SHELL",
        "OORED_RUNNER_MODE",
        "OORED_EMBEDDED_RUNNER_NAME",
        "OORED_DATA_DIR",
        "OORE_DATA_DIR",
        "OORE_SETUP_STATE_FILE",
        "OORE_PUBLIC_URL",
        "OORE_WARPGATE_TICKET",
        "OORE_ARTIFACT_DELIVERY_URL",
        "OORE_CORS_ORIGINS",
        "OORE_CORS_ORIGIN",
        "OORE_COOKIE_SECURE",
        "OORE_APP_STORE_CONNECT_API_BASE_URL",
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "OTEL_SERVICE_NAME",
        "OTEL_RESOURCE_ATTRIBUTES",
    ] {
        if let Ok(value) = std::env::var(key)
            && !value.is_empty()
        {
            env.insert(key.to_string(), value);
        }
    }

    for raw in overrides {
        let (key, value) = parse_env_assignment(raw)?;
        env.insert(key, value);
    }

    Ok(env)
}

fn render_launchd_plist(
    label: &str,
    program_args: &[String],
    env: &BTreeMap<String, String>,
    log_path: &Path,
    working_dir: &Path,
    user: Option<&str>,
) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
"#,
    );
    out.push_str(&format!(
        "    <key>Label</key>\n    <string>{}</string>\n",
        xml_escape(label)
    ));
    if let Some(user) = user {
        out.push_str(&format!(
            "    <key>UserName</key>\n    <string>{}</string>\n",
            xml_escape(user)
        ));
    }
    out.push_str("    <key>ProgramArguments</key>\n    <array>\n");
    for arg in program_args {
        out.push_str(&format!("      <string>{}</string>\n", xml_escape(arg)));
    }
    out.push_str("    </array>\n");
    out.push_str(&format!(
        "    <key>WorkingDirectory</key>\n    <string>{}</string>\n",
        xml_escape(&working_dir.display().to_string())
    ));
    out.push_str("    <key>EnvironmentVariables</key>\n    <dict>\n");
    for (key, value) in env {
        out.push_str(&format!(
            "      <key>{}</key>\n      <string>{}</string>\n",
            xml_escape(key),
            xml_escape(value)
        ));
    }
    out.push_str("    </dict>\n");
    out.push_str("    <key>Umask</key>\n    <integer>63</integer>\n");
    out.push_str("    <key>RunAtLoad</key>\n    <true/>\n");
    out.push_str("    <key>KeepAlive</key>\n    <true/>\n");
    out.push_str(&format!(
        "    <key>StandardOutPath</key>\n    <string>{}</string>\n",
        xml_escape(&log_path.display().to_string())
    ));
    out.push_str(&format!(
        "    <key>StandardErrorPath</key>\n    <string>{}</string>\n",
        xml_escape(&log_path.display().to_string())
    ));
    out.push_str("  </dict>\n</plist>\n");
    out
}

fn verify_regular_file(path: &Path, expected_uid: u32, expected_mode: u32) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file(),
        "plist is not a regular file"
    );
    anyhow::ensure!(metadata.uid() == expected_uid, "plist has unexpected owner");
    anyhow::ensure!(
        metadata.permissions().mode() & 0o777 == expected_mode,
        "plist permissions are not {expected_mode:o}"
    );
    Ok(())
}

fn write_system_plist(path: &Path, contents: &str, expected_uid: u32) -> anyhow::Result<()> {
    write_system_plist_with_mode(path, contents, expected_uid, 0o600)
}

fn write_system_plist_with_mode(
    path: &Path,
    contents: &str,
    expected_uid: u32,
    mode: u32,
) -> anyhow::Result<()> {
    anyhow::ensure!(matches!(mode, 0o600 | 0o644), "unsupported plist mode");
    let file_name = path.file_name().context("plist path has no file name")?;
    let temporary = path.with_file_name(format!(
        ".{}.{}.tmp",
        file_name.to_string_lossy(),
        uuid::Uuid::new_v4()
    ));

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&temporary)
            .with_context(|| format!("failed to create {}", temporary.display()))?;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        verify_regular_file(&temporary, expected_uid, mode)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        drop(file);

        fs::rename(&temporary, path)
            .with_context(|| format!("failed to replace {}", path.display()))?;
        verify_regular_file(path, expected_uid, mode)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn set_path_owner(path: &Path, expected_uid: u32) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if metadata.uid() == expected_uid {
        return Ok(());
    }
    anyhow::ensure!(
        current_uid() == 0 && metadata.uid() == 0,
        "refusing to take ownership of {} from another account",
        path.display()
    );
    let mut command = Command::new("/usr/sbin/chown");
    command.arg(expected_uid.to_string()).arg(path);
    let output = command_output_with_timeout(command, "assigning daemon log ownership")?;
    anyhow::ensure!(
        output.status.success(),
        "failed to assign daemon log ownership: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

fn prepare_service_log(log_dir: &Path, log_path: &Path, owner_uid: u32) -> anyhow::Result<()> {
    fs::create_dir_all(log_dir).context("failed to create oored log directory")?;
    let directory_metadata = fs::symlink_metadata(log_dir)
        .with_context(|| format!("failed to inspect {}", log_dir.display()))?;
    anyhow::ensure!(
        directory_metadata.file_type().is_dir() && !directory_metadata.file_type().is_symlink(),
        "oored log path is not a regular directory"
    );
    set_path_owner(log_dir, owner_uid)?;
    fs::set_permissions(log_dir, fs::Permissions::from_mode(0o700))
        .context("failed to secure oored log directory")?;

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(log_path)
        .with_context(|| format!("failed to prepare oored log file {}", log_path.display()))?;
    let file_metadata = file.metadata()?;
    anyhow::ensure!(
        file_metadata.file_type().is_file(),
        "oored log path is not a regular file"
    );
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    drop(file);
    set_path_owner(log_path, owner_uid)?;
    let metadata = fs::symlink_metadata(log_path)?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == owner_uid
            && metadata.permissions().mode() & 0o777 == 0o600,
        "oored log file has unsafe ownership or permissions"
    );
    Ok(())
}

fn current_uid() -> u32 {
    // SAFETY: `geteuid` has no arguments, pointer requirements, or failure state.
    unsafe { libc::geteuid() }
}

fn user_uid(user: &str) -> anyhow::Result<u32> {
    let mut command = Command::new("/usr/bin/id");
    command.args(["-u", user]);
    let output = command_output_with_timeout(command, "resolving the launchd service account")?;
    anyhow::ensure!(
        output.status.success(),
        "launchd service account does not exist: {user}"
    );
    let uid = String::from_utf8(output.stdout)
        .context("launchd service account id was not valid UTF-8")?
        .trim()
        .parse::<u32>()
        .context("launchd service account id was invalid")?;
    anyhow::ensure!(uid != 0, "the managed daemon must not run as root");
    Ok(uid)
}

fn user_home(user: &str) -> anyhow::Result<String> {
    let mut command = Command::new("/usr/bin/dscl");
    command.args([".", "-read", &format!("/Users/{user}"), "NFSHomeDirectory"]);
    let output = command_output_with_timeout(command, "resolving the launchd service home")?;
    anyhow::ensure!(
        output.status.success(),
        "failed to resolve the home directory for {user}"
    );
    let output =
        String::from_utf8(output.stdout).context("launchd service home was not valid UTF-8")?;
    let home = output
        .split_once(':')
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
        .context("launchd service home was missing")?;
    anyhow::ensure!(
        Path::new(home).is_absolute(),
        "launchd service home is not absolute"
    );
    Ok(home.to_string())
}

fn launchctl(args: &[&str]) -> anyhow::Result<std::process::Output> {
    let mut command = Command::new("/bin/launchctl");
    command.args(args);
    command_output_with_timeout(command, &format!("running launchctl {}", args.join(" ")))
}

fn command_output_with_timeout(mut command: Command, action: &str) -> anyhow::Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
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

fn launchctl_checked(args: &[&str]) -> anyhow::Result<()> {
    let output = launchctl(args)?;
    check_launchctl_output(args, &output)
}

fn check_launchctl_output(args: &[&str], output: &std::process::Output) -> anyhow::Result<()> {
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
    anyhow::bail!("launchctl {} failed: {detail}", args.join(" "))
}

struct DaemonServiceSnapshot {
    definition: Option<(String, u32)>,
    was_loaded: bool,
}

fn launchd_job_state(service: &str, expected_executable: &Path) -> anyhow::Result<Option<bool>> {
    let output = launchctl(&["print", service])?;
    if !output.status.success() {
        let detail = format!(
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .to_ascii_lowercase();
        if detail.contains("could not find service")
            || detail.contains("service not found")
            || detail.contains("no such process")
        {
            return Ok(None);
        }
        anyhow::bail!("failed to inspect launchd service {service}: {detail}");
    }
    let output = String::from_utf8(output.stdout).context("launchd output was not valid UTF-8")?;
    let program = output
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("program = "))
        .map(PathBuf::from)
        .context("loaded launchd service does not report its executable")?;
    anyhow::ensure!(
        paths_refer_to_same_file(&program, expected_executable),
        "loaded launchd service uses foreign executable {}",
        program.display()
    );
    let running = output
        .lines()
        .map(str::trim)
        .any(|line| line == "state = running")
        && output
            .lines()
            .map(str::trim)
            .any(|line| line.strip_prefix("pid = ").is_some());
    Ok(Some(running))
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn read_service_document(path: &Path) -> anyhow::Result<serde_json::Value> {
    let mut command = Command::new("/usr/bin/plutil");
    command.args(["-convert", "json", "-o", "-"]).arg(path);
    let output = command_output_with_timeout(command, "reading the launchd service definition")?;
    anyhow::ensure!(
        output.status.success(),
        "failed to read launchd service definition: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    serde_json::from_slice(&output.stdout).context("launchd service definition is invalid")
}

fn validate_existing_service_definition(
    path: &Path,
    expected_uid: u32,
    label: &str,
    executable: &Path,
    service_user: Option<&str>,
) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == expected_uid
            && matches!(metadata.permissions().mode() & 0o777, 0o600 | 0o644),
        "service definition has unsafe ownership or permissions: {}",
        path.display()
    );
    let document = read_service_document(path)?;
    anyhow::ensure!(
        document.get("Label").and_then(serde_json::Value::as_str) == Some(label),
        "service definition has an unexpected label"
    );
    let arguments = document
        .get("ProgramArguments")
        .and_then(serde_json::Value::as_array)
        .context("service definition has no ProgramArguments")?;
    let configured_executable = arguments
        .first()
        .and_then(serde_json::Value::as_str)
        .map(Path::new)
        .context("service definition has no executable")?;
    anyhow::ensure!(
        paths_refer_to_same_file(configured_executable, executable)
            && arguments.get(1).and_then(serde_json::Value::as_str) == Some("run"),
        "service definition does not launch this oored executable"
    );
    if let Some(user) = service_user {
        anyhow::ensure!(
            document.get("UserName").and_then(serde_json::Value::as_str) == Some(user),
            "service definition runs as an unexpected account"
        );
    }
    Ok(())
}

fn capture_service_snapshot(
    path: &Path,
    service: &str,
    executable: &Path,
) -> anyhow::Result<DaemonServiceSnapshot> {
    let definition = match fs::read_to_string(path) {
        Ok(contents) => Some((
            contents,
            fs::symlink_metadata(path)?.permissions().mode() & 0o777,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let loaded = launchd_job_state(service, executable)?.is_some();
    anyhow::ensure!(
        definition.is_some() || !loaded,
        "launchd service is loaded without a restorable definition"
    );
    Ok(DaemonServiceSnapshot {
        definition,
        was_loaded: loaded,
    })
}

fn stop_daemon_service(service: &str, executable: &Path) -> anyhow::Result<()> {
    if launchd_job_state(service, executable)?.is_none() {
        return Ok(());
    }
    let output = launchctl(&["bootout", service])?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if launchd_job_state(service, executable)?.is_none() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!(
        "failed to stop launchd service {service}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

fn start_daemon_service(
    domain: &str,
    service: &str,
    plist_path: &Path,
    executable: &Path,
) -> anyhow::Result<()> {
    let plist = plist_path.display().to_string();
    if launchctl_checked(&["bootstrap", domain, &plist]).is_err() {
        std::thread::sleep(Duration::from_millis(250));
        launchctl_checked(&["bootstrap", domain, &plist])?;
    }
    launchctl_checked(&["kickstart", "-k", service])?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if launchd_job_state(service, executable)? == Some(true) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("launchd service {service} did not start")
}

fn write_service_definition(path: &Path, contents: &str, expected_uid: u32) -> anyhow::Result<()> {
    write_system_plist(path, contents, expected_uid)
}

fn remove_service_definition(path: &Path) -> anyhow::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn restore_service_snapshot(
    snapshot: &DaemonServiceSnapshot,
    path: &Path,
    expected_uid: u32,
    domain: &str,
    service: &str,
    executable: &Path,
) -> anyhow::Result<()> {
    let mut failures = Vec::new();
    if let Err(error) = stop_daemon_service(service, executable) {
        failures.push(format!("failed to stop replacement service: {error:#}"));
    }
    let definition = match snapshot.definition.as_ref() {
        Some((contents, mode)) => write_system_plist_with_mode(path, contents, expected_uid, *mode),
        None => remove_service_definition(path),
    };
    if let Err(error) = definition {
        failures.push(format!("failed to restore service definition: {error:#}"));
    } else if snapshot.was_loaded
        && let Err(error) = start_daemon_service(domain, service, path, executable)
    {
        failures.push(format!("failed to restart previous service: {error:#}"));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(failures.join("; "))
    }
}

async fn verify_daemon_health(listen: &str) -> anyhow::Result<()> {
    let address: SocketAddr = listen
        .parse()
        .with_context(|| format!("invalid daemon listen address: {listen}"))?;
    let base = format!("http://{}", oored::loopback_maintenance_addr(address));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .context("failed to create daemon health client")?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last_error = "daemon did not answer".to_string();
    while Instant::now() < deadline {
        let health = client.get(format!("{base}/healthz")).send().await;
        let ready = client.get(format!("{base}/readyz")).send().await;
        match (health, ready) {
            (Ok(health), Ok(ready))
                if health.status().is_success() && ready.status().is_success() =>
            {
                let health: serde_json::Value = health
                    .json()
                    .await
                    .context("daemon health response was invalid")?;
                let ready: serde_json::Value = ready
                    .json()
                    .await
                    .context("daemon readiness response was invalid")?;
                if health.get("ok").and_then(serde_json::Value::as_bool) == Some(true)
                    && health
                        .get("package_version")
                        .and_then(serde_json::Value::as_str)
                        == Some(env!("CARGO_PKG_VERSION"))
                    && ready.get("ok").and_then(serde_json::Value::as_bool) == Some(true)
                {
                    return Ok(());
                }
                last_error = "daemon returned an unhealthy response".to_string();
            }
            (health, ready) => {
                last_error = format!("health={health:?}, ready={ready:?}");
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    anyhow::bail!("daemon did not become ready within 30 seconds: {last_error}")
}

fn install_service(args: InstallServiceArgs) -> anyhow::Result<()> {
    if !cfg!(target_os = "macos") {
        anyhow::bail!("oored install-service is currently supported on macOS only");
    }

    refuse_manifest_managed_service_command("install-service", "oore setup")?;
    validate_launchd_label(&args.label)?;
    let bin = std::env::current_exe().context("failed to resolve current executable")?;
    let install_root = if args.system {
        if current_uid() != 0 {
            anyhow::bail!("system service installation requires root; rerun with sudo");
        }
        bin.parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .context("failed to derive install root from oored binary")?
    } else {
        resolve_install_root()?
    };
    let log_dir = install_root.join("logs");
    let log_path = log_dir.join("oored.log");
    let plist_path = if args.system {
        launch_daemon_plist_path(&args.label)?
    } else {
        launch_agent_plist_path(&args.label)?
    };

    let service_user = args.user.as_deref();
    if args.system && service_user.is_none() {
        anyhow::bail!("--user is required with --system");
    }
    let service_uid = service_user
        .map(user_uid)
        .transpose()?
        .unwrap_or(current_uid());
    prepare_service_log(&log_dir, &log_path, service_uid)?;
    if !args.system {
        fs::create_dir_all(launch_agent_dir()?)
            .context("failed to create LaunchAgents directory")?;
    }

    let mut program_args = vec![
        bin.display().to_string(),
        "run".to_string(),
        "--listen".to_string(),
        args.listen.clone(),
    ];
    if let Some(state_file) = args.state_file.as_deref() {
        program_args.push("--state-file".to_string());
        program_args.push(state_file.to_string());
    }

    let mut env = service_environment(&args.env)?;
    if let Some(user) = service_user {
        env.insert("HOME".to_string(), user_home(user)?);
    }
    let plist = render_launchd_plist(
        &args.label,
        &program_args,
        &env,
        &log_path,
        &install_root,
        service_user,
    );

    let (service, domain) = if args.system {
        (format!("system/{}", args.label), "system".to_string())
    } else {
        let uid = current_uid();
        (format!("gui/{uid}/{}", args.label), format!("gui/{uid}"))
    };

    let expected_uid = if args.system { 0 } else { current_uid() };
    if plist_path.exists() {
        validate_existing_service_definition(
            &plist_path,
            expected_uid,
            &args.label,
            &bin,
            service_user,
        )?;
    }
    let snapshot = capture_service_snapshot(&plist_path, &service, &bin)?;

    if args.no_start {
        anyhow::ensure!(
            !snapshot.was_loaded,
            "--no-start cannot replace a loaded service; stop it first"
        );
        if let Err(error) = write_service_definition(&plist_path, &plist, expected_uid) {
            if let Err(rollback_error) = restore_service_snapshot(
                &snapshot,
                &plist_path,
                expected_uid,
                &domain,
                &service,
                &bin,
            ) {
                anyhow::bail!(
                    "{error:#}; restoring the previous service definition also failed: {rollback_error:#}"
                );
            }
            return Err(error);
        }
        println!("Installed launchd service plist: {}", plist_path.display());
        println!(
            "Start it with: launchctl bootstrap {domain} {}",
            plist_path.display()
        );
        return Ok(());
    }

    let activation = (|| -> anyhow::Result<()> {
        write_service_definition(&plist_path, &plist, expected_uid)?;
        if snapshot.was_loaded {
            stop_daemon_service(&service, &bin)?;
        }
        start_daemon_service(&domain, &service, &plist_path, &bin)
    })();
    let activation = match activation {
        Ok(()) => tokio::runtime::Runtime::new()
            .context("failed to create daemon health runtime")
            .and_then(|runtime| runtime.block_on(verify_daemon_health(&args.listen))),
        Err(error) => Err(error),
    };
    if let Err(error) = activation {
        if let Err(rollback_error) = restore_service_snapshot(
            &snapshot,
            &plist_path,
            expected_uid,
            &domain,
            &service,
            &bin,
        ) {
            anyhow::bail!(
                "{error:#}; restoring the previous daemon service also failed: {rollback_error:#}"
            );
        }
        return Err(error);
    }

    println!("Installed and started launchd service: {}", args.label);
    println!("Plist: {}", plist_path.display());
    println!("Logs:  {}", log_path.display());
    println!("API:   http://{}", args.listen);
    Ok(())
}

fn uninstall_service(args: UninstallServiceArgs) -> anyhow::Result<()> {
    if !cfg!(target_os = "macos") {
        anyhow::bail!("oored uninstall-service is currently supported on macOS only");
    }

    refuse_manifest_managed_service_command("uninstall-service", "oore uninstall")?;
    validate_launchd_label(&args.label)?;
    if args.system && current_uid() != 0 {
        anyhow::bail!("system service removal requires root; rerun with sudo");
    }
    let plist_path = if args.system {
        launch_daemon_plist_path(&args.label)?
    } else {
        launch_agent_plist_path(&args.label)?
    };
    let service = if args.system {
        format!("system/{}", args.label)
    } else {
        let uid = current_uid();
        format!("gui/{uid}/{}", args.label)
    };

    let bin = std::env::current_exe().context("failed to resolve current executable")?;
    let expected_uid = if args.system { 0 } else { current_uid() };
    if plist_path.exists() {
        validate_existing_service_definition(&plist_path, expected_uid, &args.label, &bin, None)?;
    }
    let snapshot = capture_service_snapshot(&plist_path, &service, &bin)?;
    if snapshot.definition.is_none() {
        println!("Removed launchd service: {}", args.label);
        println!("Data and logs were left untouched.");
        return Ok(());
    }
    let domain = if args.system {
        "system".to_string()
    } else {
        format!("gui/{}", current_uid())
    };
    let removal =
        stop_daemon_service(&service, &bin).and_then(|()| remove_service_definition(&plist_path));
    if let Err(error) = removal {
        if let Err(rollback_error) = restore_service_snapshot(
            &snapshot,
            &plist_path,
            expected_uid,
            &domain,
            &service,
            &bin,
        ) {
            anyhow::bail!(
                "{error:#}; restoring the daemon service also failed: {rollback_error:#}"
            );
        }
        return Err(error);
    }

    println!("Removed launchd service: {}", args.label);
    println!("Data and logs were left untouched.");
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run(args) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(run_server(args))?;
        }
        Commands::InstallService(args) => {
            install_service(args)?;
        }
        Commands::UninstallService(args) => {
            uninstall_service(args)?;
        }
        Commands::Version => {
            let install_root = resolve_install_root()?;
            if let Some(v) = read_trimmed_file(&install_root.join("VERSION")) {
                println!("{v}");
            } else {
                println!("{}", env!("CARGO_PKG_VERSION"));
            }
        }
        Commands::PackageVersion => println!("{}", env!("CARGO_PKG_VERSION")),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{loopback_companion_addr, validate_runner_mode};
    use std::env::VarError;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn private_interface_listeners_get_a_loopback_companion() {
        let netbird = "100.92.14.7:8787".parse().unwrap();
        assert_eq!(
            loopback_companion_addr(netbird),
            Some("127.0.0.1:8787".parse().unwrap())
        );
        let ipv6 = "[fd7a:115c:a1e0::1]:8787".parse().unwrap();
        assert_eq!(
            loopback_companion_addr(ipv6),
            Some("[::1]:8787".parse().unwrap())
        );
        for listen in ["127.0.0.1:8787", "0.0.0.0:8787", "[::]:8787"] {
            assert_eq!(loopback_companion_addr(listen.parse().unwrap()), None);
        }
    }

    #[test]
    fn runner_mode_keeps_external_only_compatibility() {
        assert!(validate_runner_mode(Err(VarError::NotPresent)).is_ok());
        for value in ["external", " EXTERNAL ", "\texternal\n"] {
            assert!(validate_runner_mode(Ok(value.to_string())).is_ok());
        }
        for value in ["embedded", " HYBRID "] {
            assert_eq!(
                validate_runner_mode(Ok(value.to_string()))
                    .unwrap_err()
                    .to_string(),
                "embedded runner execution is disabled; use an external Direct macOS runner"
            );
        }
        for value in ["", "  ", "unknown"] {
            assert_eq!(
                validate_runner_mode(Ok(value.to_string()))
                    .unwrap_err()
                    .to_string(),
                format!("invalid OORED_RUNNER_MODE: {:?}", value.trim())
            );
        }
        let invalid = VarError::NotUnicode(OsString::from_vec(vec![0xff]));
        assert!(
            validate_runner_mode(Err(invalid))
                .unwrap_err()
                .is::<VarError>()
        );
    }
}
