use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
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
    observability::init_tracing();

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

    let runner_pool = store.pool().clone();
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
        management_socket_path(&db_path),
        runner_pool.clone(),
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

    // The managed local runner and operator CLI reach the daemon through
    // loopback. Keep a loopback listener alongside a private (for example,
    // NetBird) bind.
    let daemon_url = embedded_runner_url(addr);
    let _embedded_runner = oored::embedded_runner::start_if_enabled(runner_pool, daemon_url)
        .await
        .context("failed to initialize embedded runner")?;

    let server_result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await;

    management_task.abort();
    recovery_capabilities.clear().await;
    server_result.context("oored server failed")?;

    // Best-effort flush of OTel spans on shutdown
    observability::shutdown_tracing();

    Ok(())
}

fn loopback_companion_addr(addr: SocketAddr) -> Option<SocketAddr> {
    match addr.ip() {
        ip if ip.is_loopback() || ip.is_unspecified() => None,
        IpAddr::V4(_) => Some(SocketAddr::from(([127, 0, 0, 1], addr.port()))),
        IpAddr::V6(_) => Some(SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], addr.port()))),
    }
}

fn embedded_runner_url(addr: SocketAddr) -> String {
    let loopback = match addr.ip() {
        IpAddr::V4(_) => IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        IpAddr::V6(_) => IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
    };
    format!("http://{}", SocketAddr::new(loopback, addr.port()))
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

fn verify_private_regular_file(path: &Path, expected_uid: u32) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file(),
        "plist is not a regular file"
    );
    anyhow::ensure!(metadata.uid() == expected_uid, "plist has unexpected owner");
    anyhow::ensure!(
        metadata.permissions().mode() & 0o777 == 0o600,
        "plist permissions are not 0600"
    );
    Ok(())
}

fn write_system_plist(path: &Path, contents: &str, expected_uid: u32) -> anyhow::Result<()> {
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
            .mode(0o600)
            .open(&temporary)
            .with_context(|| format!("failed to create {}", temporary.display()))?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        verify_private_regular_file(&temporary, expected_uid)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        drop(file);

        fs::rename(&temporary, path)
            .with_context(|| format!("failed to replace {}", path.display()))?;
        verify_private_regular_file(path, expected_uid)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn current_uid() -> u32 {
    // SAFETY: `geteuid` has no arguments, pointer requirements, or failure state.
    unsafe { libc::geteuid() }
}

fn launchctl(args: &[&str]) -> anyhow::Result<std::process::Output> {
    Command::new("launchctl")
        .args(args)
        .output()
        .with_context(|| format!("failed to run launchctl {}", args.join(" ")))
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

fn install_service(args: InstallServiceArgs) -> anyhow::Result<()> {
    if !cfg!(target_os = "macos") {
        anyhow::bail!("oored install-service is currently supported on macOS only");
    }

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

    fs::create_dir_all(&log_dir).context("failed to create oored log directory")?;
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

    let env = service_environment(&args.env)?;
    let service_user = args.user.as_deref();
    if args.system && service_user.is_none() {
        anyhow::bail!("--user is required with --system");
    }
    let plist = render_launchd_plist(
        &args.label,
        &program_args,
        &env,
        &log_path,
        &install_root,
        service_user,
    );
    if args.system {
        write_system_plist(&plist_path, &plist, 0)?;
    } else {
        fs::write(&plist_path, plist)
            .with_context(|| format!("failed to write {}", plist_path.display()))?;
    }

    let (service, domain) = if args.system {
        (format!("system/{}", args.label), "system".to_string())
    } else {
        let uid = current_uid();
        (format!("gui/{uid}/{}", args.label), format!("gui/{uid}"))
    };

    if args.no_start {
        println!("Installed launchd service plist: {}", plist_path.display());
        println!(
            "Start it with: launchctl bootstrap {domain} {}",
            plist_path.display()
        );
        return Ok(());
    }

    let _ = launchctl(&["bootout", &service]);
    let plist_str = plist_path.display().to_string();
    if launchctl_checked(&["bootstrap", &domain, &plist_str]).is_err() {
        std::thread::sleep(std::time::Duration::from_millis(250));
        launchctl_checked(&["bootstrap", &domain, &plist_str])?;
    }
    launchctl_checked(&["kickstart", "-k", &service])?;
    launchctl_checked(&["print", &service])?;

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

    let _ = launchctl(&["bootout", &service]);
    let _ = launchctl(&["remove", &args.label]);
    if plist_path.exists() {
        fs::remove_file(&plist_path)
            .with_context(|| format!("failed to remove {}", plist_path.display()))?;
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
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn launchctl_failure_reports_command_and_stderr() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(256),
            stdout: Vec::new(),
            stderr: b"Bootstrap failed: 5: Input/output error\n".to_vec(),
        };

        let error = check_launchctl_output(&["bootstrap", "system", "/tmp/oored.plist"], &output)
            .unwrap_err()
            .to_string();
        assert!(error.contains("launchctl bootstrap system /tmp/oored.plist failed"));
        assert!(error.contains("Bootstrap failed: 5: Input/output error"));
    }

    #[test]
    fn launchd_plist_escapes_program_args_and_env() {
        let args = vec![
            "/Users/me/.oore/bin/oored".to_string(),
            "run".to_string(),
            "--listen".to_string(),
            "127.0.0.1:8787".to_string(),
        ];
        let env = BTreeMap::from([(
            "OORE_CORS_ORIGINS".to_string(),
            "https://ci.example.com?a=1&b=2".to_string(),
        )]);
        let plist = render_launchd_plist(
            "build.oore.oored",
            &args,
            &env,
            Path::new("/tmp/oore<log>.log"),
            Path::new("/tmp/oore&root"),
            None,
        );

        assert!(plist.contains("<string>build.oore.oored</string>"));
        assert!(plist.contains("<string>/Users/me/.oore/bin/oored</string>"));
        assert!(plist.contains("https://ci.example.com?a=1&amp;b=2"));
        assert!(plist.contains("/tmp/oore&lt;log&gt;.log"));
        assert!(plist.contains("/tmp/oore&amp;root"));
        assert!(!plist.contains("<key>UserName</key>"));

        let system_plist = render_launchd_plist(
            "build.oore.oored",
            &args,
            &env,
            Path::new("/tmp/oore.log"),
            Path::new("/Users/appbuilder/.oore"),
            Some("appbuilder"),
        );
        assert!(system_plist.contains("<key>UserName</key>\n    <string>appbuilder</string>"));
    }

    #[test]
    fn rejects_unsafe_launchd_labels() {
        assert!(validate_launchd_label("build.oore.oored").is_ok());
        assert!(validate_launchd_label("../build.oore.oored").is_err());
        assert!(validate_launchd_label("build/oore/oored").is_err());
        assert!(validate_launchd_label("").is_err());
    }

    #[test]
    fn parses_env_assignment() {
        assert_eq!(
            parse_env_assignment("OORE_PUBLIC_URL=https://ci.example.com").unwrap(),
            (
                "OORE_PUBLIC_URL".to_string(),
                "https://ci.example.com".to_string()
            )
        );
        assert!(parse_env_assignment("1BAD=value").is_err());
        assert!(parse_env_assignment("MISSING_VALUE").is_err());
    }

    #[test]
    fn system_plist_replaces_broad_file_with_private_regular_file() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let tmp = tempfile::TempDir::new().expect("tempdir");
        let path = tmp.path().join("build.oore.oored.plist");
        fs::write(&path, "old").expect("seed plist");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("broaden plist");
        let expected_uid = fs::metadata(&path).expect("seed metadata").uid();

        write_system_plist(&path, "secret-bearing plist", expected_uid)
            .expect("secure plist write");

        let metadata = fs::symlink_metadata(&path).expect("final metadata");
        assert!(metadata.file_type().is_file());
        assert_eq!(metadata.uid(), expected_uid);
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::read_to_string(&path).unwrap(), "secret-bearing plist");
    }

    #[test]
    fn adds_loopback_companion_only_for_specific_non_loopback_binds() {
        let netbird: SocketAddr = "100.107.193.1:8787".parse().unwrap();
        let loopback: SocketAddr = "127.0.0.1:8787".parse().unwrap();
        let wildcard: SocketAddr = "0.0.0.0:8787".parse().unwrap();

        assert_eq!(
            loopback_companion_addr(netbird),
            Some("127.0.0.1:8787".parse().unwrap())
        );
        assert_eq!(loopback_companion_addr(loopback), None);
        assert_eq!(loopback_companion_addr(wildcard), None);
        assert_eq!(embedded_runner_url(netbird), "http://127.0.0.1:8787");
        assert_eq!(
            embedded_runner_url("[fd00::1]:8787".parse().unwrap()),
            "http://[::1]:8787"
        );
    }
}
