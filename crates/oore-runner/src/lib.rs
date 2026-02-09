use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oore_contract::{
    BuildPlatform, BuildStatus, ClaimJobResponse, ClaimedJob, JobStatusResponse,
    PipelineCommandStages, PipelineEnvVar, PipelineExecutionConfig, PlatformBuildArgs,
    PlatformBuildCommands, StepResult,
};
use sha2::{Digest, Sha256};
use tokio::io::AsyncBufReadExt;

const AUTO_CONFIG_PATHS: [&str; 2] = [".oore.yaml", ".oore.yml"];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunnerConfig {
    pub runner_id: String,
    pub runner_token: String,
    pub daemon_url: String,
    pub name: String,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub async fn detect_capabilities() -> serde_json::Value {
    let os_version = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let xcode_version = std::process::Command::new("xcodebuild")
        .arg("-version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .unwrap_or_default();

    let arch = std::env::consts::ARCH.to_string();

    serde_json::json!({
        "os": "macos",
        "os_version": os_version,
        "arch": arch,
        "xcode_version": xcode_version,
    })
}

pub fn get_hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub async fn run_runner_forever(
    config: RunnerConfig,
    daemon_url_override: Option<String>,
) -> anyhow::Result<()> {
    let daemon_url = daemon_url_override.unwrap_or(config.daemon_url.clone());
    let client = reqwest::Client::new();

    println!("Starting runner '{}' ({})", config.name, config.runner_id);
    println!("Connecting to: {}", daemon_url);

    let capabilities = detect_capabilities().await;

    // Send one heartbeat immediately so runner status appears online
    // without waiting for the first interval tick.
    let _ = client
        .post(format!(
            "{}/v1/runners/{}/heartbeat",
            daemon_url, config.runner_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&serde_json::json!({ "status": "online", "capabilities": capabilities }))
        .send()
        .await;

    let hb_client = client.clone();
    let hb_url = daemon_url.clone();
    let hb_token = config.runner_token.clone();
    let hb_runner_id = config.runner_id.clone();
    let hb_capabilities = capabilities.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let _ = hb_client
                .post(format!("{}/v1/runners/{}/heartbeat", hb_url, hb_runner_id))
                .bearer_auth(&hb_token)
                .json(&serde_json::json!({ "status": "online", "capabilities": hb_capabilities }))
                .send()
                .await;
        }
    });

    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        match claim_and_execute(&client, &daemon_url, &config).await {
            Ok(_executed) => {}
            Err(e) => {
                eprintln!("Error during claim/execute: {}", e);
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }
    }
}

async fn claim_and_execute(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
) -> anyhow::Result<bool> {
    let resp = client
        .post(format!(
            "{}/v1/runners/{}/claim",
            daemon_url, config.runner_id
        ))
        .bearer_auth(&config.runner_token)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Claim request failed: {}", resp.status());
    }

    let claim: ClaimJobResponse = resp.json().await?;
    let job = match claim.job {
        Some(j) => j,
        None => return Ok(false),
    };

    println!(
        "Claimed build {} (#{}) for project {}",
        job.build_id, job.build_number, job.project_id
    );

    report_status(
        client,
        daemon_url,
        config,
        &job.build_id,
        "running",
        None,
        None,
        &[],
    )
    .await?;

    let (steps, result) = execute_build(&job, client, daemon_url, config).await;

    match result {
        Ok(()) => {
            report_status(
                client,
                daemon_url,
                config,
                &job.build_id,
                "succeeded",
                Some(0),
                None,
                &steps,
            )
            .await?;
            println!("Build {} succeeded", job.build_id);
        }
        Err(e) => {
            if e.downcast_ref::<BuildTerminated>().is_some() {
                println!(
                    "Build {} was externally terminated, skipping status report",
                    job.build_id
                );
            } else {
                report_status(
                    client,
                    daemon_url,
                    config,
                    &job.build_id,
                    "failed",
                    Some(1),
                    Some(&e.to_string()),
                    &steps,
                )
                .await?;
                eprintln!("Build {} failed: {}", job.build_id, e);
            }
        }
    }

    Ok(true)
}

struct WorkspaceCleanup {
    path: PathBuf,
}

impl Drop for WorkspaceCleanup {
    fn drop(&mut self) {
        if self.path.exists() {
            if let Err(e) = fs::remove_dir_all(&self.path) {
                eprintln!(
                    "Warning: failed to clean up workspace {}: {}",
                    self.path.display(),
                    e
                );
            }
        }
    }
}

#[derive(Debug)]
struct BuildTerminated {
    status: String,
}

impl std::fmt::Display for BuildTerminated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "build was externally terminated (status: {})",
            self.status
        )
    }
}

impl std::error::Error for BuildTerminated {}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoPipelineConfig {
    version: u32,
    platforms: Vec<BuildPlatform>,
    #[serde(default)]
    flutter_version: Option<String>,
    #[serde(default)]
    commands: RepoCommandStages,
    #[serde(default)]
    platform_build_args: RepoPlatformBuildArgs,
    #[serde(default)]
    platform_commands: RepoPlatformBuildCommands,
    #[serde(default)]
    env: Vec<RepoEnvVar>,
    #[serde(default)]
    artifacts: RepoArtifacts,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoCommandStages {
    #[serde(default)]
    pre_build: Vec<String>,
    #[serde(default)]
    build: Vec<String>,
    #[serde(default)]
    post_build: Vec<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoArtifacts {
    #[serde(default)]
    patterns: Vec<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoPlatformBuildArgs {
    #[serde(default)]
    android: Vec<String>,
    #[serde(default)]
    ios: Vec<String>,
    #[serde(default)]
    macos: Vec<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoPlatformBuildCommands {
    #[serde(default)]
    android: Option<String>,
    #[serde(default)]
    ios: Option<String>,
    #[serde(default)]
    macos: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoEnvVar {
    key: String,
    value: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FvmRcConfig {
    flutter: String,
}

#[derive(Debug, Clone)]
struct ResolvedExecutionPlan {
    stage_commands: PipelineCommandStages,
    artifact_patterns: Vec<String>,
    env: Vec<PipelineEnvVar>,
    source: String,
}

fn validate_command_list(stage: &str, commands: &[String]) -> anyhow::Result<Vec<String>> {
    let mut cleaned = Vec::with_capacity(commands.len());
    for (idx, command) in commands.iter().enumerate() {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            anyhow::bail!("commands.{stage}[{idx}] must not be empty");
        }
        cleaned.push(trimmed.to_string());
    }
    Ok(cleaned)
}

fn normalize_flutter_version(
    value: Option<&str>,
    field_path: &str,
) -> anyhow::Result<Option<String>> {
    match value {
        None => Ok(None),
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                anyhow::bail!("{field_path} must not be empty");
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

fn read_fvmrc_version(workspace: &Path) -> anyhow::Result<Option<String>> {
    let fvmrc_path = workspace.join(".fvmrc");
    if !fvmrc_path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&fvmrc_path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", fvmrc_path.display()))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("{} is empty", fvmrc_path.display());
    }

    if trimmed.starts_with('{') {
        let parsed: FvmRcConfig = serde_json::from_str(trimmed).map_err(|e| {
            anyhow::anyhow!(
                "failed to parse {} as JSON object with 'flutter': {e}",
                fvmrc_path.display()
            )
        })?;
        return normalize_flutter_version(Some(parsed.flutter.as_str()), ".fvmrc.flutter");
    }

    if trimmed.starts_with('"') {
        let parsed: String = serde_json::from_str(trimmed).map_err(|e| {
            anyhow::anyhow!(
                "failed to parse {} as JSON string: {e}",
                fvmrc_path.display()
            )
        })?;
        return normalize_flutter_version(Some(parsed.as_str()), ".fvmrc");
    }

    normalize_flutter_version(Some(trimmed), ".fvmrc")
}

fn maybe_wrap_with_fvm(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed == "flutter" {
        return "fvm flutter".to_string();
    }
    if trimmed == "dart" {
        return "fvm dart".to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("flutter ") {
        return format!("fvm flutter {rest}");
    }
    if let Some(rest) = trimmed.strip_prefix("dart ") {
        return format!("fvm dart {rest}");
    }
    trimmed.to_string()
}

fn apply_fvm_wrappers(stage_commands: PipelineCommandStages) -> PipelineCommandStages {
    PipelineCommandStages {
        pre_build: stage_commands
            .pre_build
            .into_iter()
            .map(|cmd| maybe_wrap_with_fvm(&cmd))
            .collect(),
        build: stage_commands
            .build
            .into_iter()
            .map(|cmd| maybe_wrap_with_fvm(&cmd))
            .collect(),
        post_build: stage_commands
            .post_build
            .into_iter()
            .map(|cmd| maybe_wrap_with_fvm(&cmd))
            .collect(),
    }
}

fn validate_artifact_patterns(patterns: &[String]) -> anyhow::Result<Vec<String>> {
    let mut cleaned = Vec::with_capacity(patterns.len());
    for (idx, pattern) in patterns.iter().enumerate() {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            anyhow::bail!("artifacts.patterns[{idx}] must not be empty");
        }
        if !trimmed.starts_with("*.") || trimmed.chars().any(char::is_whitespace) {
            anyhow::bail!("artifacts.patterns[{idx}] must be extension globs like '*.apk'");
        }
        cleaned.push(trimmed.to_string());
    }
    Ok(cleaned)
}

fn validate_platform_args(args: &[String], path: &str) -> anyhow::Result<Vec<String>> {
    let mut cleaned = Vec::with_capacity(args.len());
    for (idx, arg) in args.iter().enumerate() {
        let trimmed = arg.trim();
        if trimmed.is_empty() {
            anyhow::bail!("{path}[{idx}] must not be empty");
        }
        cleaned.push(trimmed.to_string());
    }
    Ok(cleaned)
}

fn validate_platform_command(
    command: &Option<String>,
    path: &str,
) -> anyhow::Result<Option<String>> {
    match command {
        None => Ok(None),
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                anyhow::bail!("{path} must not be empty when provided");
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first != '_' && !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn validate_env_vars(env: &[PipelineEnvVar]) -> anyhow::Result<Vec<PipelineEnvVar>> {
    let mut validated = Vec::with_capacity(env.len());
    let mut seen = std::collections::HashSet::new();
    for (idx, pair) in env.iter().enumerate() {
        let key = pair.key.trim();
        let value = pair.value.trim();
        if key.is_empty() {
            anyhow::bail!("env[{idx}].key must not be empty");
        }
        if !is_valid_env_key(key) {
            anyhow::bail!("env[{idx}].key must match [A-Za-z_][A-Za-z0-9_]*");
        }
        if !seen.insert(key.to_string()) {
            anyhow::bail!("env contains duplicate key '{key}'");
        }
        validated.push(PipelineEnvVar {
            key: key.to_string(),
            value: value.to_string(),
        });
    }
    Ok(validated)
}

fn normalize_execution_config(
    config: PipelineExecutionConfig,
) -> anyhow::Result<PipelineExecutionConfig> {
    if config.platforms.is_empty() {
        anyhow::bail!("platforms must include at least one target");
    }

    let commands = PipelineCommandStages {
        pre_build: validate_command_list("pre_build", &config.commands.pre_build)?,
        build: validate_command_list("build", &config.commands.build)?,
        post_build: validate_command_list("post_build", &config.commands.post_build)?,
    };
    let platform_build_args = PlatformBuildArgs {
        android: validate_platform_args(
            &config.platform_build_args.android,
            "platform_build_args.android",
        )?,
        ios: validate_platform_args(&config.platform_build_args.ios, "platform_build_args.ios")?,
        macos: validate_platform_args(
            &config.platform_build_args.macos,
            "platform_build_args.macos",
        )?,
    };
    let platform_commands = PlatformBuildCommands {
        android: validate_platform_command(
            &config.platform_commands.android,
            "platform_commands.android",
        )?,
        ios: validate_platform_command(&config.platform_commands.ios, "platform_commands.ios")?,
        macos: validate_platform_command(
            &config.platform_commands.macos,
            "platform_commands.macos",
        )?,
    };
    let env = validate_env_vars(&config.env)?;
    let artifact_patterns = validate_artifact_patterns(&config.artifact_patterns)?;
    let flutter_version =
        normalize_flutter_version(config.flutter_version.as_deref(), "flutter_version")?;

    Ok(PipelineExecutionConfig {
        platforms: config.platforms,
        flutter_version,
        commands,
        platform_build_args,
        platform_commands,
        env,
        artifact_patterns,
    })
}

fn parse_repo_config_file(raw: &str) -> anyhow::Result<PipelineExecutionConfig> {
    let parsed: RepoPipelineConfig =
        serde_yaml::from_str(raw).map_err(|e| anyhow::anyhow!("YAML parse error: {e}"))?;

    if parsed.version != 1 {
        anyhow::bail!("unsupported config version {}, expected 1", parsed.version);
    }

    normalize_execution_config(PipelineExecutionConfig {
        platforms: parsed.platforms,
        flutter_version: parsed.flutter_version,
        commands: PipelineCommandStages {
            pre_build: parsed.commands.pre_build,
            build: parsed.commands.build,
            post_build: parsed.commands.post_build,
        },
        platform_build_args: PlatformBuildArgs {
            android: parsed.platform_build_args.android,
            ios: parsed.platform_build_args.ios,
            macos: parsed.platform_build_args.macos,
        },
        platform_commands: PlatformBuildCommands {
            android: parsed.platform_commands.android,
            ios: parsed.platform_commands.ios,
            macos: parsed.platform_commands.macos,
        },
        env: parsed
            .env
            .into_iter()
            .map(|entry| PipelineEnvVar {
                key: entry.key,
                value: entry.value,
            })
            .collect(),
        artifact_patterns: parsed.artifacts.patterns,
    })
}

fn build_default_command_with_args(base: &str, args: &[String]) -> String {
    if args.is_empty() {
        return base.to_string();
    }
    format!("{base} {}", args.join(" "))
}

fn default_platform_command(
    platform: &BuildPlatform,
    overrides: &PlatformBuildCommands,
    args: &PlatformBuildArgs,
) -> String {
    match platform {
        BuildPlatform::Android => overrides.android.clone().unwrap_or_else(|| {
            build_default_command_with_args("flutter build apk --release", &args.android)
        }),
        BuildPlatform::Ios => overrides.ios.clone().unwrap_or_else(|| {
            build_default_command_with_args("flutter build ios --release --no-codesign", &args.ios)
        }),
        BuildPlatform::Macos => overrides.macos.clone().unwrap_or_else(|| {
            build_default_command_with_args("flutter build macos --release", &args.macos)
        }),
    }
}

fn materialize_stage_commands(config: &PipelineExecutionConfig) -> PipelineCommandStages {
    let mut pre_build = Vec::new();
    if !config.platforms.is_empty() {
        pre_build.push("flutter pub get".to_string());
    }
    pre_build.extend(config.commands.pre_build.clone());

    let mut build = config
        .platforms
        .iter()
        .map(|platform| {
            default_platform_command(
                platform,
                &config.platform_commands,
                &config.platform_build_args,
            )
        })
        .collect::<Vec<_>>();
    build.extend(config.commands.build.clone());

    PipelineCommandStages {
        pre_build,
        build,
        post_build: config.commands.post_build.clone(),
    }
}

fn load_ui_execution_config(
    snapshot: &serde_json::Value,
) -> anyhow::Result<PipelineExecutionConfig> {
    let raw = snapshot.get("ui_execution_config");
    if raw.is_none() {
        return Ok(PipelineExecutionConfig::default());
    }
    let parsed: PipelineExecutionConfig = serde_json::from_value(raw.cloned().unwrap_or_default())
        .map_err(|e| anyhow::anyhow!("Invalid ui_execution_config in snapshot: {e}"))?;
    normalize_execution_config(parsed)
}

fn resolve_execution_plan(
    workspace: &Path,
    snapshot: &serde_json::Value,
) -> anyhow::Result<ResolvedExecutionPlan> {
    let config_path_explicit = snapshot
        .get("config_path_explicit")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let snapshot_config_path = snapshot
        .get("config_path")
        .and_then(|v| v.as_str())
        .unwrap_or(".oore.yaml")
        .trim()
        .to_string();

    let candidate_paths: Vec<String> = if config_path_explicit {
        vec![snapshot_config_path]
    } else {
        AUTO_CONFIG_PATHS.iter().map(|p| p.to_string()).collect()
    };

    let fvmrc_version = read_fvmrc_version(workspace)?;

    for rel_path in &candidate_paths {
        let full_path = workspace.join(rel_path);
        if !full_path.exists() {
            continue;
        }

        let content = fs::read_to_string(&full_path).map_err(|e| {
            anyhow::anyhow!("Failed to read config file {}: {e}", full_path.display())
        })?;
        let file_config = parse_repo_config_file(&content).map_err(|e| {
            anyhow::anyhow!("Invalid pipeline config in {}: {}", full_path.display(), e)
        })?;
        let resolved_flutter_version = fvmrc_version
            .clone()
            .or_else(|| file_config.flutter_version.clone());
        let mut stage_commands = materialize_stage_commands(&file_config);
        if let Some(version) = resolved_flutter_version {
            stage_commands = apply_fvm_wrappers(stage_commands);
            stage_commands
                .pre_build
                .insert(0, format!("fvm use {version} --force"));
        }

        return Ok(ResolvedExecutionPlan {
            stage_commands,
            artifact_patterns: file_config.artifact_patterns,
            env: file_config.env,
            source: format!("file:{}", full_path.display()),
        });
    }

    let fallback = load_ui_execution_config(snapshot)?;
    let resolved_flutter_version = fvmrc_version.or_else(|| fallback.flutter_version.clone());
    let mut stage_commands = materialize_stage_commands(&fallback);
    if let Some(version) = resolved_flutter_version {
        stage_commands = apply_fvm_wrappers(stage_commands);
        stage_commands
            .pre_build
            .insert(0, format!("fvm use {version} --force"));
    }
    Ok(ResolvedExecutionPlan {
        stage_commands,
        artifact_patterns: fallback.artifact_patterns,
        env: fallback.env,
        source: "ui_fallback".to_string(),
    })
}

async fn check_build_active(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!(
            "{}/v1/runners/{}/jobs/{}",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let status_resp: JobStatusResponse = r.json().await?;
            let status: BuildStatus = status_resp
                .status
                .parse()
                .map_err(|e: String| anyhow::anyhow!(e))?;

            if status.is_terminal() {
                return Err(BuildTerminated {
                    status: status_resp.status,
                }
                .into());
            }
            Ok(())
        }
        Ok(_) | Err(_) => Ok(()),
    }
}

async fn poll_cancellation(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        if check_build_active(client, daemon_url, config, build_id)
            .await
            .is_err()
        {
            return;
        }
    }
}

async fn execute_build(
    job: &ClaimedJob,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
) -> (Vec<StepResult>, anyhow::Result<()>) {
    let workspace = PathBuf::from(format!("/tmp/oore-builds/{}", job.build_id));
    if let Err(e) = fs::create_dir_all(&workspace) {
        return (vec![], Err(e.into()));
    }

    let _cleanup = WorkspaceCleanup {
        path: workspace.clone(),
    };

    let snapshot = &job.config_snapshot;
    let mut steps = Vec::new();
    let mut log_seq: i64 = 0;

    if let Err(e) = check_build_active(client, daemon_url, config, &job.build_id).await {
        return (steps, Err(e));
    }

    let repo_url = snapshot
        .get("repo_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if repo_url.is_empty() {
        return (
            steps,
            Err(anyhow::anyhow!(
                "Build config snapshot has no repo_url — cannot checkout source"
            )),
        );
    }

    if job.commit_sha.is_none() && job.branch.is_none() {
        return (
            steps,
            Err(anyhow::anyhow!(
                "Build has neither commit_sha nor branch — cannot checkout source"
            )),
        );
    }

    let start = now_unix();

    let checkout_command = if let Some(sha) = &job.commit_sha {
        format!("git fetch --depth 1 <repo> {sha} && git checkout FETCH_HEAD")
    } else if let Some(branch) = &job.branch {
        format!("git clone --depth 1 --branch {branch} <repo>")
    } else {
        unreachable!()
    };

    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        &mut log_seq,
        "stdout",
        &step_start_marker("checkout", &checkout_command),
    )
    .await;
    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        &mut log_seq,
        "stdout",
        &format!("$ {checkout_command}"),
    )
    .await;

    let child = if let Some(sha) = &job.commit_sha {
        match tokio::process::Command::new("sh")
            .arg("-c")
            .arg("git init && git fetch --depth 1 \"$OORE_REPO\" \"$OORE_SHA\" && git checkout FETCH_HEAD")
            .env("OORE_REPO", repo_url)
            .env("OORE_SHA", sha)
            .current_dir(&workspace)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return (steps, Err(e.into())),
        }
    } else if let Some(branch) = &job.branch {
        match tokio::process::Command::new("git")
            .arg("clone")
            .arg("--depth")
            .arg("1")
            .arg("--branch")
            .arg(branch)
            .arg(repo_url)
            .arg(".")
            .current_dir(&workspace)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return (steps, Err(e.into())),
        }
    } else {
        unreachable!()
    };

    let clone_status = run_and_stream(
        child,
        client,
        daemon_url,
        config,
        &job.build_id,
        &mut log_seq,
        poll_cancellation(client, daemon_url, config, &job.build_id),
    )
    .await;

    let finished = now_unix();
    match clone_status {
        None => {
            steps.push(StepResult {
                name: "checkout".to_string(),
                status: "failed".to_string(),
                exit_code: None,
                started_at: start,
                finished_at: finished,
                duration_ms: (finished - start) * 1000,
            });
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                &mut log_seq,
                "stdout",
                &step_end_marker("checkout", "canceled", None),
            )
            .await;
            return (
                steps,
                Err(BuildTerminated {
                    status: "canceled".to_string(),
                }
                .into()),
            );
        }
        Some(status) => {
            let exit_code = status.code();
            let success = exit_code == Some(0);
            steps.push(StepResult {
                name: "checkout".to_string(),
                status: if success { "succeeded" } else { "failed" }.to_string(),
                exit_code,
                started_at: start,
                finished_at: finished,
                duration_ms: (finished - start) * 1000,
            });
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                &mut log_seq,
                "stdout",
                &step_end_marker(
                    "checkout",
                    if success { "succeeded" } else { "failed" },
                    exit_code,
                ),
            )
            .await;
            if !success {
                return (steps, Err(anyhow::anyhow!("Git checkout failed")));
            }
        }
    }

    let execution_plan = match resolve_execution_plan(&workspace, snapshot) {
        Ok(plan) => plan,
        Err(e) => return (steps, Err(e)),
    };

    println!("Using pipeline config source: {}", execution_plan.source);
    let mut step_env: Vec<(String, String)> = vec![
        ("PROJECT_ID".to_string(), job.project_id.clone()),
        ("PIPELINE_ID".to_string(), job.pipeline_id.clone()),
        ("BUILD_ID".to_string(), job.build_id.clone()),
        (
            "PROJECT_BUILD_NUMBER".to_string(),
            job.build_number.to_string(),
        ),
        ("BUILD_NUMBER".to_string(), job.build_number.to_string()),
    ];
    if let Some(branch) = &job.branch {
        step_env.push(("BRANCH".to_string(), branch.clone()));
    }
    if let Some(commit_sha) = &job.commit_sha {
        step_env.push(("COMMIT_SHA".to_string(), commit_sha.clone()));
    }
    for pair in &execution_plan.env {
        step_env.push((pair.key.clone(), pair.value.clone()));
    }

    // Flutter expects dart-defines to be materialized in a file. When pipeline
    // env vars are provided, create a temporary .env and pass it to flutter build.
    let dart_define_file = if execution_plan.env.is_empty() {
        None
    } else {
        let mut content = String::new();
        for pair in &execution_plan.env {
            content.push_str(&pair.key);
            content.push('=');
            content.push_str(&pair.value);
            content.push('\n');
        }
        let define_file_path = workspace.join(".env");
        if let Err(e) = fs::write(&define_file_path, content) {
            return (
                steps,
                Err(anyhow::anyhow!(
                    "Failed to write dart define file {}: {e}",
                    define_file_path.display()
                )),
            );
        }
        Some(".env".to_string())
    };

    for (stage_name, commands) in [
        (
            "pre_build",
            execution_plan.stage_commands.pre_build.as_slice(),
        ),
        ("build", execution_plan.stage_commands.build.as_slice()),
        (
            "post_build",
            execution_plan.stage_commands.post_build.as_slice(),
        ),
    ] {
        for (index, command) in commands.iter().enumerate() {
            if let Err(e) = check_build_active(client, daemon_url, config, &job.build_id).await {
                return (steps, Err(e));
            }

            let step_name = format!("{stage_name}-{}", index + 1);
            let start = now_unix();
            let mut normalized_command = normalize_legacy_env_syntax(command);
            if stage_name == "build"
                && let Some(define_file) = dart_define_file.as_deref()
                && is_flutter_build_command(&normalized_command)
            {
                normalized_command = with_dart_define_file(&normalized_command, define_file);
            }
            let command_preview = render_command_preview(&normalized_command, &step_env);

            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                &mut log_seq,
                "stdout",
                &step_start_marker(&step_name, &normalized_command),
            )
            .await;
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                &mut log_seq,
                "stdout",
                &format!("$ {command_preview}"),
            )
            .await;
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                &mut log_seq,
                "stdout",
                &render_step_env_preview(&step_env),
            )
            .await;

            let mut step_cmd = tokio::process::Command::new("sh");
            step_cmd
                .arg("-c")
                .arg(&normalized_command)
                .current_dir(&workspace)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);
            for (key, value) in &step_env {
                step_cmd.env(key, value);
            }
            let child = match step_cmd.spawn() {
                Ok(c) => c,
                Err(e) => return (steps, Err(e.into())),
            };

            let step_status = run_and_stream(
                child,
                client,
                daemon_url,
                config,
                &job.build_id,
                &mut log_seq,
                poll_cancellation(client, daemon_url, config, &job.build_id),
            )
            .await;

            let finished = now_unix();
            match step_status {
                None => {
                    steps.push(StepResult {
                        name: step_name,
                        status: "failed".to_string(),
                        exit_code: None,
                        started_at: start,
                        finished_at: finished,
                        duration_ms: (finished - start) * 1000,
                    });
                    let _ = append_runner_log_line(
                        client,
                        daemon_url,
                        config,
                        &job.build_id,
                        &mut log_seq,
                        "stdout",
                        &step_end_marker(&format!("{stage_name}-{}", index + 1), "canceled", None),
                    )
                    .await;
                    return (
                        steps,
                        Err(BuildTerminated {
                            status: "canceled".to_string(),
                        }
                        .into()),
                    );
                }
                Some(status) => {
                    let exit_code = status.code().unwrap_or(-1);
                    steps.push(StepResult {
                        name: step_name,
                        status: if exit_code == 0 {
                            "succeeded"
                        } else {
                            "failed"
                        }
                        .to_string(),
                        exit_code: Some(exit_code),
                        started_at: start,
                        finished_at: finished,
                        duration_ms: (finished - start) * 1000,
                    });
                    let step_name = format!("{stage_name}-{}", index + 1);
                    let _ = append_runner_log_line(
                        client,
                        daemon_url,
                        config,
                        &job.build_id,
                        &mut log_seq,
                        "stdout",
                        &step_end_marker(
                            &step_name,
                            if exit_code == 0 { "succeeded" } else { "failed" },
                            Some(exit_code),
                        ),
                    )
                    .await;
                    if exit_code != 0 {
                        return (
                            steps,
                            Err(anyhow::anyhow!("Step failed with exit code {}", exit_code)),
                        );
                    }
                }
            }
        }
    }

    scan_and_upload_artifacts(
        workspace.as_path(),
        client,
        daemon_url,
        config,
        &job.build_id,
        &execution_plan.artifact_patterns,
    )
    .await;

    (steps, Ok(()))
}

fn step_start_marker(name: &str, command: &str) -> String {
    format!(
        "[oore-step] {}",
        serde_json::json!({
            "event": "start",
            "name": name,
            "command": command,
        })
    )
}

fn is_sensitive_env_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    upper.contains("SECRET")
        || upper.contains("TOKEN")
        || upper.contains("PASSWORD")
        || upper.contains("CREDENTIAL")
        || upper.contains("PRIVATE")
        || upper.contains("AUTH")
}

fn preview_env_value(key: &str, value: &str) -> String {
    if is_sensitive_env_key(key) {
        if value.is_empty() {
            String::new()
        } else {
            "***".to_string()
        }
    } else {
        value.to_string()
    }
}

fn render_step_env_preview(env: &[(String, String)]) -> String {
    if env.is_empty() {
        return "# env: (none)".to_string();
    }

    let mut parts = Vec::new();
    for (idx, (key, value)) in env.iter().enumerate() {
        if idx >= 20 {
            parts.push(format!("...(+{} more)", env.len() - 20));
            break;
        }
        let preview = preview_env_value(key, value);
        if preview.is_empty() {
            parts.push(format!("{key}="));
        } else {
            parts.push(format!("{key}={preview}"));
        }
    }

    format!("# env: {}", parts.join(" "))
}

fn normalize_legacy_env_syntax(command: &str) -> String {
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0usize;
    let mut out = String::with_capacity(command.len());

    while i < chars.len() {
        if chars[i] == '$' && i + 3 < chars.len() && chars[i + 1] == '(' && chars[i + 2] == '$' {
            let mut j = i + 3;
            if j < chars.len() && (chars[j] == '_' || chars[j].is_ascii_alphabetic()) {
                j += 1;
                while j < chars.len() && (chars[j] == '_' || chars[j].is_ascii_alphanumeric()) {
                    j += 1;
                }
                if j < chars.len() && chars[j] == ')' {
                    let var_name: String = chars[(i + 3)..j].iter().collect();
                    out.push('$');
                    out.push_str(&var_name);
                    i = j + 1;
                    continue;
                }
            }
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

fn lookup_env_value<'a>(env: &'a [(String, String)], key: &str) -> Option<&'a str> {
    env.iter()
        .find(|(k, _)| k == key)
        .map(|(_, value)| value.as_str())
}

fn render_command_preview(command: &str, env: &[(String, String)]) -> String {
    let normalized = normalize_legacy_env_syntax(command);
    let chars: Vec<char> = normalized.chars().collect();
    let mut i = 0usize;
    let mut out = String::with_capacity(normalized.len());

    while i < chars.len() {
        if chars[i] == '$' {
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                let mut j = i + 2;
                while j < chars.len() && chars[j] != '}' {
                    j += 1;
                }
                if j < chars.len() && chars[j] == '}' {
                    let key: String = chars[(i + 2)..j].iter().collect();
                    if !key.is_empty() {
                        if let Some(value) = lookup_env_value(env, &key) {
                            out.push_str(&preview_env_value(&key, value));
                        } else {
                            out.push('$');
                            out.push('{');
                            out.push_str(&key);
                            out.push('}');
                        }
                        i = j + 1;
                        continue;
                    }
                }
            } else if i + 1 < chars.len()
                && (chars[i + 1] == '_' || chars[i + 1].is_ascii_alphabetic())
            {
                let mut j = i + 2;
                while j < chars.len() && (chars[j] == '_' || chars[j].is_ascii_alphanumeric()) {
                    j += 1;
                }
                let key: String = chars[(i + 1)..j].iter().collect();
                if let Some(value) = lookup_env_value(env, &key) {
                    out.push_str(&preview_env_value(&key, value));
                } else {
                    out.push('$');
                    out.push_str(&key);
                }
                i = j;
                continue;
            }
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

fn is_flutter_build_command(command: &str) -> bool {
    let trimmed = command.trim_start();
    trimmed.starts_with("flutter build ") || trimmed.starts_with("fvm flutter build ")
}

fn with_dart_define_file(command: &str, define_file: &str) -> String {
    if command.contains("--dart-define-from-file=") {
        return command.to_string();
    }
    format!("{command} --dart-define-from-file={define_file}")
}

fn step_end_marker(name: &str, status: &str, exit_code: Option<i32>) -> String {
    format!(
        "[oore-step] {}",
        serde_json::json!({
            "event": "end",
            "name": name,
            "status": status,
            "exit_code": exit_code,
        })
    )
}

async fn append_runner_log_line(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    seq: &mut i64,
    stream: &str,
    content: &str,
) -> anyhow::Result<()> {
    let body = serde_json::json!({
        "chunks": [{
            "sequence": *seq,
            "content": content,
            "stream": stream,
        }],
    });

    let resp = client
        .post(format!(
            "{}/v1/runners/{}/jobs/{}/logs",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("log append failed: {}", resp.status());
    }

    *seq += 1;
    Ok(())
}

fn artifact_type_for_extension(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        "apk" => Some("apk"),
        "ipa" => Some("ipa"),
        _ => None,
    }
}

fn walk_dir_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    fn walk(dir: &std::path::Path, result: &mut Vec<PathBuf>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') {
                        continue;
                    }
                }
                walk(&path, result);
            } else if path.is_file() {
                result.push(path);
            }
        }
    }
    walk(dir, &mut result);
    result
}

fn compute_file_sha256(path: &std::path::Path) -> anyhow::Result<String> {
    use std::io::Read;
    let file = fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

async fn scan_and_upload_artifacts(
    workspace: &std::path::Path,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    artifact_patterns: &[String],
) {
    let all_files = walk_dir_files(workspace);

    let custom_extensions: Vec<String> = artifact_patterns
        .iter()
        .filter_map(|pat| pat.strip_prefix("*."))
        .map(|ext| ext.to_lowercase())
        .collect();

    let mut artifacts: Vec<(PathBuf, String)> = Vec::new();

    for path in &all_files {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if let Some(art_type) = artifact_type_for_extension(ext) {
                artifacts.push((path.clone(), art_type.to_string()));
            } else if custom_extensions.contains(&ext.to_lowercase()) {
                artifacts.push((path.clone(), "generic".to_string()));
            }
        }
    }

    if artifacts.is_empty() {
        return;
    }

    println!("Found {} artifact(s) to upload", artifacts.len());

    for (path, artifact_type) in &artifacts {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let file_size = fs::metadata(path).map(|m| m.len() as i64).ok();

        let checksum = match compute_file_sha256(path) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("Warning: failed to compute checksum for {}: {}", name, e);
                None
            }
        };

        let body = serde_json::json!({
            "name": name,
            "artifact_type": artifact_type,
            "file_size": file_size,
            "checksum": checksum,
            "metadata": {},
        });

        let resp = match client
            .post(format!(
                "{}/v1/runners/{}/jobs/{}/artifacts",
                daemon_url, config.runner_id, build_id
            ))
            .bearer_auth(&config.runner_token)
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Warning: failed to register artifact {}: {}", name, e);
                continue;
            }
        };

        if !resp.status().is_success() {
            eprintln!(
                "Warning: artifact registration failed for {} (HTTP {})",
                name,
                resp.status()
            );
            continue;
        }

        let create_resp: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "Warning: failed to parse artifact response for {}: {}",
                    name, e
                );
                continue;
            }
        };

        let upload_url = create_resp
            .get("upload_url")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if upload_url.is_empty() {
            println!(
                "  Registered artifact {} (no S3 upload URL — storage not configured)",
                name
            );
            continue;
        }

        match tokio::fs::read(path).await {
            Ok(bytes) => match client.put(upload_url).body(bytes).send().await {
                Ok(r) if r.status().is_success() => {
                    println!("  Uploaded artifact {}", name);
                }
                Ok(r) => {
                    eprintln!(
                        "Warning: S3 upload failed for {} (HTTP {})",
                        name,
                        r.status()
                    );
                }
                Err(e) => {
                    eprintln!("Warning: S3 upload failed for {}: {}", name, e);
                }
            },
            Err(e) => {
                eprintln!("Warning: failed to read artifact file {}: {}", name, e);
            }
        }
    }
}

async fn report_status(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    status: &str,
    exit_code: Option<i32>,
    error_message: Option<&str>,
    steps: &[StepResult],
) -> anyhow::Result<()> {
    let body = serde_json::json!({
        "status": status,
        "exit_code": exit_code,
        "error_message": error_message,
        "steps": steps,
    });

    let resp = client
        .post(format!(
            "{}/v1/runners/{}/jobs/{}/status",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Status update failed: {}", resp.status());
    }

    Ok(())
}

async fn run_and_stream(
    mut child: tokio::process::Child,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    seq: &mut i64,
    cancel_fut: impl std::future::Future<Output = ()>,
) -> Option<std::process::ExitStatus> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let client_out = client.clone();
    let daemon_out = daemon_url.to_string();
    let config_out_id = config.runner_id.clone();
    let config_out_token = config.runner_token.clone();
    let build_out = build_id.to_string();

    let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<(String, String)>(256);

    if let Some(stdout) = stdout {
        let tx = line_tx.clone();
        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send((line, "stdout".to_string())).await.is_err() {
                    break;
                }
            }
        });
    }

    if let Some(stderr) = stderr {
        let tx = line_tx.clone();
        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send((line, "stderr".to_string())).await.is_err() {
                    break;
                }
            }
        });
    }

    drop(line_tx);

    let upload_client = client_out;
    let upload_daemon = daemon_out;
    let upload_config_id = config_out_id;
    let upload_config_token = config_out_token;
    let upload_build = build_out;
    let seq_start = *seq;
    let upload_handle = tokio::spawn(async move {
        let mut local_seq = seq_start;
        let mut batch = Vec::new();
        let mut interval = tokio::time::interval(Duration::from_millis(500));

        loop {
            tokio::select! {
                line = line_rx.recv() => {
                    match line {
                        Some((content, stream)) => {
                            batch.push(serde_json::json!({
                                "sequence": local_seq,
                                "content": content,
                                "stream": stream,
                            }));
                            local_seq += 1;

                            if batch.len() >= 50 {
                                let body = serde_json::json!({ "chunks": batch });
                                let _ = upload_client
                                    .post(format!(
                                        "{}/v1/runners/{}/jobs/{}/logs",
                                        upload_daemon, upload_config_id, upload_build
                                    ))
                                    .bearer_auth(&upload_config_token)
                                    .json(&body)
                                    .send()
                                    .await;
                                batch = Vec::new();
                            }
                        }
                        None => {
                            if !batch.is_empty() {
                                let body = serde_json::json!({ "chunks": batch });
                                let _ = upload_client
                                    .post(format!(
                                        "{}/v1/runners/{}/jobs/{}/logs",
                                        upload_daemon, upload_config_id, upload_build
                                    ))
                                    .bearer_auth(&upload_config_token)
                                    .json(&body)
                                    .send()
                                    .await;
                            }
                            return local_seq;
                        }
                    }
                }
                _ = interval.tick() => {
                    if !batch.is_empty() {
                        let body = serde_json::json!({ "chunks": batch });
                        let _ = upload_client
                            .post(format!(
                                "{}/v1/runners/{}/jobs/{}/logs",
                                upload_daemon, upload_config_id, upload_build
                            ))
                            .bearer_auth(&upload_config_token)
                            .json(&body)
                            .send()
                            .await;
                        batch = Vec::new();
                    }
                }
            }
        }
    });

    let status = tokio::select! {
        result = child.wait() => {
            match result {
                Ok(s) => Some(s),
                Err(_) => None,
            }
        },
        _ = cancel_fut => {
            child.kill().await.ok();
            None
        }
    };

    if let Ok(final_seq) = upload_handle.await {
        *seq = final_seq;
    }

    status
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_workspace() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("oore-runner-test-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn cleanup_workspace(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn autodetect_prefers_dot_oore_yaml_over_yml() {
        let workspace = temp_workspace();
        fs::write(
            workspace.join(".oore.yaml"),
            "version: 1\nplatforms: [ios]\ncommands:\n  pre_build: []\n  build: []\n  post_build: []\nartifacts:\n  patterns: [\"*.ipa\"]\n",
        )
        .expect("write yaml");
        fs::write(
            workspace.join(".oore.yml"),
            "version: 1\nplatforms: [android]\ncommands:\n  pre_build: []\n  build: []\n  post_build: []\nartifacts:\n  patterns: [\"*.apk\"]\n",
        )
        .expect("write yml");

        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "config_path": ".oore.yml",
            "ui_execution_config": {
                "platforms": ["android"],
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": ["*.apk"]
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot).expect("resolve plan");
        assert_eq!(
            plan.stage_commands.build,
            vec!["flutter build ios --release --no-codesign".to_string()]
        );
        assert_eq!(plan.artifact_patterns, vec!["*.ipa".to_string()]);

        cleanup_workspace(&workspace);
    }

    #[test]
    fn explicit_path_wins_over_auto_detect() {
        let workspace = temp_workspace();
        fs::create_dir_all(workspace.join("ci")).expect("mkdir ci");
        fs::write(
            workspace.join("ci/pipeline.yml"),
            "version: 1\nplatforms: [macos]\ncommands:\n  pre_build: []\n  build: []\n  post_build: []\nartifacts:\n  patterns: [\"*.zip\"]\n",
        )
        .expect("write explicit");
        fs::write(
            workspace.join(".oore.yaml"),
            "version: 1\nplatforms: [android]\ncommands:\n  pre_build: []\n  build: []\n  post_build: []\nartifacts:\n  patterns: [\"*.apk\"]\n",
        )
        .expect("write auto");

        let snapshot = serde_json::json!({
            "config_path_explicit": true,
            "config_path": "ci/pipeline.yml",
            "ui_execution_config": {
                "platforms": ["android"],
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": ["*.apk"]
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot).expect("resolve plan");
        assert_eq!(
            plan.stage_commands.build,
            vec!["flutter build macos --release".to_string()]
        );
        assert_eq!(plan.artifact_patterns, vec!["*.zip".to_string()]);

        cleanup_workspace(&workspace);
    }

    #[test]
    fn invalid_yaml_fails_without_fallback() {
        let workspace = temp_workspace();
        fs::write(
            workspace.join(".oore.yaml"),
            "version: 1\nplatforms: [android]\ncommands:\n  bad_stage: [\"echo no\"]\nartifacts:\n  patterns: [\"*.apk\"]\n",
        )
        .expect("write invalid yaml");

        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "ui_execution_config": {
                "platforms": ["android"],
                "commands": { "pre_build": [], "build": ["echo fallback"], "post_build": [] },
                "artifact_patterns": ["*.apk"]
            }
        });

        let error = resolve_execution_plan(&workspace, &snapshot).expect_err("should fail");
        assert!(error.to_string().contains("Invalid pipeline config"));

        cleanup_workspace(&workspace);
    }

    #[test]
    fn missing_file_uses_ui_fallback_config() {
        let workspace = temp_workspace();

        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "ui_execution_config": {
                "platforms": ["android", "macos"],
                "commands": { "pre_build": ["echo pre"], "build": ["echo custom"], "post_build": ["echo post"] },
                "artifact_patterns": ["*.apk", "*.zip"]
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot).expect("resolve plan");
        assert_eq!(plan.source, "ui_fallback");
        assert_eq!(
            plan.stage_commands.pre_build,
            vec!["flutter pub get".to_string(), "echo pre".to_string()]
        );
        assert_eq!(
            plan.stage_commands.build,
            vec![
                "flutter build apk --release".to_string(),
                "flutter build macos --release".to_string(),
                "echo custom".to_string(),
            ]
        );
        assert_eq!(
            plan.stage_commands.post_build,
            vec!["echo post".to_string()]
        );

        cleanup_workspace(&workspace);
    }

    #[test]
    fn fvmrc_flutter_version_is_applied_to_flutter_commands() {
        let workspace = temp_workspace();
        fs::write(workspace.join(".fvmrc"), "{ \"flutter\": \"3.24.0\" }\n")
            .expect("write .fvmrc");

        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "ui_execution_config": {
                "platforms": ["android"],
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": ["*.apk"]
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot).expect("resolve plan");
        assert_eq!(
            plan.stage_commands.pre_build,
            vec![
                "fvm use 3.24.0 --force".to_string(),
                "fvm flutter pub get".to_string(),
            ]
        );
        assert_eq!(
            plan.stage_commands.build,
            vec!["fvm flutter build apk --release".to_string()]
        );

        cleanup_workspace(&workspace);
    }

    #[test]
    fn ui_flutter_version_used_when_fvmrc_absent() {
        let workspace = temp_workspace();

        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "ui_execution_config": {
                "platforms": ["android"],
                "flutter_version": "3.22.3",
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": ["*.apk"]
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot).expect("resolve plan");
        assert_eq!(
            plan.stage_commands.pre_build,
            vec![
                "fvm use 3.22.3 --force".to_string(),
                "fvm flutter pub get".to_string(),
            ]
        );
        assert_eq!(
            plan.stage_commands.build,
            vec!["fvm flutter build apk --release".to_string()]
        );

        cleanup_workspace(&workspace);
    }

    #[test]
    fn invalid_fvmrc_fails_resolution() {
        let workspace = temp_workspace();
        fs::write(workspace.join(".fvmrc"), "{ }\n").expect("write invalid .fvmrc");

        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "ui_execution_config": {
                "platforms": ["android"],
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": ["*.apk"]
            }
        });

        let error = resolve_execution_plan(&workspace, &snapshot).expect_err("should fail");
        assert!(error.to_string().contains(".fvmrc"));

        cleanup_workspace(&workspace);
    }

    #[test]
    fn multi_platform_default_command_order_is_sequential() {
        let config = PipelineExecutionConfig {
            platforms: vec![
                BuildPlatform::Android,
                BuildPlatform::Ios,
                BuildPlatform::Macos,
            ],
            flutter_version: None,
            commands: PipelineCommandStages::default(),
            platform_build_args: PlatformBuildArgs::default(),
            platform_commands: PlatformBuildCommands::default(),
            env: Vec::new(),
            artifact_patterns: vec!["*.apk".to_string()],
        };
        let commands = materialize_stage_commands(&config);
        assert_eq!(
            commands.build,
            vec![
                "flutter build apk --release".to_string(),
                "flutter build ios --release --no-codesign".to_string(),
                "flutter build macos --release".to_string(),
            ]
        );
    }

    #[test]
    fn defaults_run_before_custom_build_commands() {
        let config = PipelineExecutionConfig {
            platforms: vec![BuildPlatform::Android],
            flutter_version: None,
            commands: PipelineCommandStages {
                pre_build: Vec::new(),
                build: vec!["echo custom-build".to_string()],
                post_build: Vec::new(),
            },
            platform_build_args: PlatformBuildArgs::default(),
            platform_commands: PlatformBuildCommands::default(),
            env: Vec::new(),
            artifact_patterns: vec!["*.apk".to_string()],
        };

        let commands = materialize_stage_commands(&config);
        assert_eq!(
            commands.build,
            vec![
                "flutter build apk --release".to_string(),
                "echo custom-build".to_string(),
            ]
        );
    }

    #[test]
    fn platform_build_args_are_appended_to_default_commands() {
        let config = PipelineExecutionConfig {
            platforms: vec![BuildPlatform::Android],
            flutter_version: None,
            commands: PipelineCommandStages::default(),
            platform_build_args: PlatformBuildArgs {
                android: vec![
                    "--dart-define-from-file=config/dev.json".to_string(),
                    "--build-number=$PROJECT_BUILD_NUMBER".to_string(),
                ],
                ios: Vec::new(),
                macos: Vec::new(),
            },
            platform_commands: PlatformBuildCommands::default(),
            env: Vec::new(),
            artifact_patterns: vec!["*.apk".to_string()],
        };

        let commands = materialize_stage_commands(&config);
        assert_eq!(
            commands.build,
            vec![
                "flutter build apk --release --dart-define-from-file=config/dev.json --build-number=$PROJECT_BUILD_NUMBER".to_string()
            ]
        );
    }

    #[test]
    fn platform_command_override_takes_precedence_over_default_and_args() {
        let config = PipelineExecutionConfig {
            platforms: vec![BuildPlatform::Android],
            flutter_version: None,
            commands: PipelineCommandStages::default(),
            platform_build_args: PlatformBuildArgs {
                android: vec!["--build-number=42".to_string()],
                ios: Vec::new(),
                macos: Vec::new(),
            },
            platform_commands: PlatformBuildCommands {
                android: Some("flutter build appbundle --release".to_string()),
                ios: None,
                macos: None,
            },
            env: Vec::new(),
            artifact_patterns: vec!["*.apk".to_string()],
        };

        let commands = materialize_stage_commands(&config);
        assert_eq!(
            commands.build,
            vec!["flutter build appbundle --release".to_string()]
        );
    }

    #[test]
    fn env_keys_must_be_unique_and_valid() {
        let config = PipelineExecutionConfig {
            platforms: vec![BuildPlatform::Android],
            flutter_version: None,
            commands: PipelineCommandStages::default(),
            platform_build_args: PlatformBuildArgs::default(),
            platform_commands: PlatformBuildCommands::default(),
            env: vec![
                PipelineEnvVar {
                    key: "PROJECT_BUILD_NUMBER".to_string(),
                    value: "1".to_string(),
                },
                PipelineEnvVar {
                    key: "PROJECT_BUILD_NUMBER".to_string(),
                    value: "2".to_string(),
                },
            ],
            artifact_patterns: vec!["*.apk".to_string()],
        };

        let err = normalize_execution_config(config).expect_err("duplicate env keys should fail");
        assert!(err.to_string().contains("duplicate key"));
    }

    #[test]
    fn normalizes_legacy_command_substitution_syntax() {
        let normalized = normalize_legacy_env_syntax(
            "flutter build apk --build-number=$($PROJECT_BUILD_NUMBER)",
        );
        assert_eq!(
            normalized,
            "flutter build apk --build-number=$PROJECT_BUILD_NUMBER"
        );
    }

    #[test]
    fn command_preview_expands_env_and_masks_sensitive_values() {
        let env = vec![
            ("PROJECT_BUILD_NUMBER".to_string(), "42".to_string()),
            ("API_TOKEN".to_string(), "secret-123".to_string()),
        ];
        let preview = render_command_preview(
            "flutter build apk --build-number=$PROJECT_BUILD_NUMBER --dart-define=API_TOKEN=$API_TOKEN",
            &env,
        );
        assert!(preview.contains("--build-number=42"));
        assert!(preview.contains("API_TOKEN=***"));
        assert!(!preview.contains("secret-123"));
    }

    #[test]
    fn appends_dart_define_file_to_flutter_build_commands() {
        let command = "fvm flutter build apk --release";
        assert!(is_flutter_build_command(command));
        let updated = with_dart_define_file(command, ".env");
        assert_eq!(
            updated,
            "fvm flutter build apk --release --dart-define-from-file=.env"
        );
    }

    #[test]
    fn does_not_duplicate_existing_dart_define_file_arg() {
        let command = "flutter build ios --release --dart-define-from-file=.env";
        let updated = with_dart_define_file(command, ".env");
        assert_eq!(updated, command);
    }
}
