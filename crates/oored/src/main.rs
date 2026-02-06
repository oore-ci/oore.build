use std::net::SocketAddr;

use anyhow::Context;
use clap::{Parser, Subcommand};
use oored::build_router;
use oored::crypto;
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
    tracing_subscriber::fmt().with_env_filter("info").init();

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

    // Load or generate the AES-256 encryption key for secrets at rest
    let key_path = crypto::resolve_key_path()
        .context("failed to resolve encryption key path")?;
    let encryption_key = crypto::load_or_generate_key(&key_path)
        .context("failed to load or generate encryption key")?;
    info!(path = %key_path.display(), "encryption key ready");

    let app = build_router(store, encryption_key);

    info!(listen = %addr, "starting oored daemon");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .await
        .context("oored server failed")?;

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
