use std::net::SocketAddr;

use anyhow::Context;
use clap::{Parser, Subcommand};
use oored::build_router;
use oored::crypto;
use oored::observability;
use oored::store::SetupStore;
use tracing::info;

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
    InstallService,
    UninstallService,
    Version,
}

#[derive(Debug, clap::Args)]
struct RunArgs {
    #[arg(long, env = "OORED_LISTEN_ADDR", default_value = "127.0.0.1:8787")]
    listen: String,

    /// Path to the setup database file (overrides OORE_SETUP_STATE_FILE and default).
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,
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

    let store = SetupStore::connect(db_path)
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

    // Start embedded local runner in default mode so single-host installations
    // can execute queued builds without a separate `oore runner start` process.
    let daemon_url = format!("http://127.0.0.1:{}", addr.port());
    let _embedded_runner =
        oored::embedded_runner::start_if_enabled(store.pool().clone(), daemon_url)
            .await
            .context("failed to initialize embedded runner")?;

    let app = build_router(store, runtime_key.key, metrics_handle).await;

    info!(listen = %addr, "starting oored daemon");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("oored server failed")?;

    // Best-effort flush of OTel spans on shutdown
    observability::shutdown_tracing();

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run(args) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(run_server(args))?;
        }
        Commands::InstallService => {
            println!("install-service placeholder (launchd integration pending)");
        }
        Commands::UninstallService => {
            println!("uninstall-service placeholder (launchd integration pending)");
        }
        Commands::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
        }
    }

    Ok(())
}
