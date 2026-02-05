use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use oore_contract::{BootstrapTokenRecord, SetupState, SetupStateFile};
use rand::RngCore;
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(name = "oore")]
#[command(about = "oore operator CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Setup(SetupArgs),
    Login,
    Status,
    Runner(RunnerArgs),
    Config(ConfigArgs),
    Doctor,
}

#[derive(Debug, Args)]
struct SetupArgs {
    #[command(subcommand)]
    command: Option<SetupSubcommand>,
}

#[derive(Debug, Subcommand)]
enum SetupSubcommand {
    Open(SetupOpenArgs),
}

#[derive(Debug, Args)]
struct SetupOpenArgs {
    #[arg(long, default_value = "15m")]
    ttl: String,

    #[arg(long, default_value = "false")]
    json: bool,

    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,
}

#[derive(Debug, Args)]
struct RunnerArgs {
    #[command(subcommand)]
    command: RunnerSubcommand,
}

#[derive(Debug, Subcommand)]
enum RunnerSubcommand {
    Register,
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

fn parse_ttl(raw: &str) -> anyhow::Result<Duration> {
    humantime::parse_duration(raw).with_context(|| format!("invalid ttl value: {raw}"))
}

fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn resolve_state_file_path(override_path: Option<&str>) -> PathBuf {
    if let Some(p) = override_path {
        return PathBuf::from(p);
    }

    match dirs::data_dir() {
        Some(data_dir) => data_dir.join("oore").join("setup-state.json"),
        None => PathBuf::from("./.oore/setup-state.json"),
    }
}

fn load_or_create_state_file(path: &PathBuf) -> anyhow::Result<SetupStateFile> {
    if path.exists() {
        let contents =
            fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        let state_file: SetupStateFile = serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse state file at {}", path.display()))?;
        Ok(state_file)
    } else {
        let now = now_epoch_secs();
        Ok(SetupStateFile {
            schema_version: SetupStateFile::CURRENT_SCHEMA_VERSION,
            instance_id: uuid::Uuid::new_v4().to_string(),
            setup_state: SetupState::BootstrapPending,
            bootstrap_token: None,
            setup_session: None,
            oidc_config: None,
            owner: None,
            created_at: now,
            updated_at: now,
        })
    }
}

fn write_state_file_atomic(path: &PathBuf, state: &SetupStateFile) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let contents = serde_json::to_string_pretty(state)
        .context("failed to serialize state file")?;

    fs::write(&tmp_path, &contents)
        .with_context(|| format!("failed to write temp file {}", tmp_path.display()))?;

    fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "failed to rename {} to {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    Ok(())
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

fn handle_setup_open(args: SetupOpenArgs) -> anyhow::Result<()> {
    let ttl = parse_ttl(&args.ttl)?;

    // 1. Resolve state file path
    let state_path = resolve_state_file_path(args.state_file.as_deref());

    // 2. Load or create state file
    let mut state = load_or_create_state_file(&state_path)?;

    // 3. Validate state — if Ready, error out
    if state.setup_state == SetupState::Ready {
        eprintln!("Setup is already complete. Instance is in 'ready' state.");
        std::process::exit(1);
    }

    // 4. Generate bootstrap token
    let mut token_bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut token_bytes);
    let plaintext_token = hex::encode(token_bytes);

    // Hash the token for storage
    let token_hash = hex::encode(Sha256::digest(plaintext_token.as_bytes()));

    // 5. Calculate expiry
    let expires_at = now_epoch_secs() + ttl.as_secs() as i64;

    // 6. Update state file
    state.bootstrap_token = Some(BootstrapTokenRecord {
        hash: token_hash,
        expires_at,
        consumed_at: None,
    });
    state.updated_at = now_epoch_secs();

    write_state_file_atomic(&state_path, &state)?;

    // 7. Output
    let state_display = match state.setup_state {
        SetupState::Uninitialized => "uninitialized",
        SetupState::BootstrapPending => "bootstrap_pending",
        SetupState::IdpConfigured => "idp_configured",
        SetupState::OwnerCreated => "owner_created",
        SetupState::Ready => unreachable!("ready state is rejected above"),
    };
    let state_file_display = state_path.display();

    if args.json {
        let output = serde_json::json!({
            "token": plaintext_token,
            "expires_at": expires_at,
            "state": state_display,
            "state_file": state_file_display.to_string(),
            "instance_id": state.instance_id,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let ttl_display = format_ttl_human(&ttl);
        println!("Bootstrap token generated.");
        println!();
        println!("Token:   {}", plaintext_token);
        println!("Expires: {} ({} from now)", expires_at, ttl_display);
        println!("State:   {}", state_display);
        println!("File:    {}", state_file_display);
        println!();
        println!(
            "Use this token with POST /v1/setup/bootstrap-token/verify to start setup."
        );
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Setup(setup) => match setup.command {
            Some(SetupSubcommand::Open(args)) => {
                handle_setup_open(args)?;
            }
            None => {
                println!("starting interactive setup flow (not implemented yet)");
            }
        },
        Commands::Login => {
            println!("login flow placeholder");
        }
        Commands::Status => {
            println!("status command placeholder");
        }
        Commands::Runner(runner) => match runner.command {
            RunnerSubcommand::Register => {
                println!("runner registration placeholder");
            }
        },
        Commands::Config(config) => match config.command {
            ConfigSubcommand::Set(args) => {
                println!("config set placeholder: {}={}", args.key, args.value);
            }
            ConfigSubcommand::Get(args) => {
                println!("config get placeholder: {}", args.key);
            }
        },
        Commands::Doctor => {
            println!("doctor checks placeholder");
        }
    }

    Ok(())
}
