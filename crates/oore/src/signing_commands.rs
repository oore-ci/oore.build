//! `oore signing list|add|remove`: an app's signing library. Keystores,
//! certificates and profiles come from files; passwords come from a
//! variable or standard input, never from an argument.

use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

use anyhow::Context;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use clap::{Args, Subcommand};
use oore_contract::{ListSigningIdentitiesResponse, SigningIdentity, SigningIdentityKind};
use serde_json::{Value, json};

use crate::admin_api::{self, api_url, call, confirm, decode, format_time, get, print_json};
use crate::build_commands::{Api, ApiConnectionArgs};

const KEYSTORE_PASSWORD_ENV: &str = "OORE_KEYSTORE_PASSWORD";
const KEY_PASSWORD_ENV: &str = "OORE_KEY_PASSWORD";
const P12_PASSWORD_ENV: &str = "OORE_P12_PASSWORD";

#[derive(Debug, Args)]
pub(crate) struct SigningArgs {
    #[command(subcommand)]
    pub(crate) command: SigningSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum SigningSubcommand {
    /// List an app's signing identities. Key material is never shown.
    List(SigningListArgs),
    /// Add an Android keystore (--keystore) or an iOS distribution
    /// certificate with its profiles (--p12 and --profile). Passwords come
    /// from OORE_KEYSTORE_PASSWORD and OORE_KEY_PASSWORD, or OORE_P12_PASSWORD,
    /// else a prompt, else one per line on standard input.
    Add(SigningAddArgs),
    /// Remove a signing identity from an app.
    Remove(SigningRemoveArgs),
}

#[derive(Debug, Args)]
pub(crate) struct SigningListArgs {
    /// App whose library to list.
    #[arg(long, value_name = "PROJECT_ID")]
    project: String,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct SigningAddArgs {
    /// App to add the identity to.
    #[arg(long, value_name = "PROJECT_ID")]
    project: String,

    /// Name in the app, as a v2 file's `signing` names it.
    #[arg(long)]
    name: String,

    /// Android: the keystore file (.jks or .keystore).
    #[arg(long, value_name = "PATH", conflicts_with_all = ["p12", "profiles"], required_unless_present = "p12")]
    keystore: Option<PathBuf>,

    /// Android: the key alias in the keystore.
    #[arg(long, requires = "keystore")]
    alias: Option<String>,

    /// iOS: the distribution certificate (.p12).
    #[arg(long, value_name = "PATH", requires = "profiles")]
    p12: Option<PathBuf>,

    /// iOS: a provisioning profile (.mobileprovision). Repeat for each
    /// bundle id.
    #[arg(long = "profile", value_name = "PATH")]
    profiles: Vec<PathBuf>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the saved identity as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct SigningRemoveArgs {
    /// ID of the identity, from `oore signing list`.
    identity_id: String,

    /// App the identity belongs to.
    #[arg(long, value_name = "PROJECT_ID")]
    project: String,

    /// Remove without asking. Required when no terminal is attached.
    #[arg(long, short = 'y', default_value = "false")]
    yes: bool,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

pub(crate) async fn handle(args: SigningArgs) -> anyhow::Result<()> {
    match args.command {
        SigningSubcommand::List(args) => list(args).await,
        SigningSubcommand::Add(args) => add(args).await,
        SigningSubcommand::Remove(args) => remove(args).await,
    }
}

fn kind_label(kind: SigningIdentityKind) -> &'static str {
    match kind {
        SigningIdentityKind::AndroidKeystore => "android",
        SigningIdentityKind::IosDistribution => "ios",
    }
}

fn identity_detail(identity: &SigningIdentity) -> String {
    match identity.kind {
        SigningIdentityKind::AndroidKeystore => {
            format!("alias {}", identity.key_alias.as_deref().unwrap_or("-"))
        }
        SigningIdentityKind::IosDistribution => format!(
            "team {}, {} profile(s), expires {}",
            identity.team_id.as_deref().unwrap_or("-"),
            identity.profiles.len(),
            format_time(identity.expires_at)
        ),
    }
}

async fn list(args: SigningListArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let response = get(
        &api,
        api_url(&api, &["projects", &args.project, "signing-identities"])?,
        "signing list",
    )
    .await?;
    if args.json {
        return print_json(&response);
    }
    let list: ListSigningIdentitiesResponse = decode(&response, "signing list")?;
    if list.identities.is_empty() {
        println!("No signing identities. Add one with `oore signing add`.");
        return Ok(());
    }
    let rows = list
        .identities
        .iter()
        .map(|identity| {
            let used = identity.used_by.len() + identity.used_by_pipelines.len();
            vec![
                identity.id.clone(),
                identity.name.clone(),
                kind_label(identity.kind).to_string(),
                identity_detail(identity),
                format!("{used}"),
            ]
        })
        .collect::<Vec<_>>();
    admin_api::print_table(&["ID", "NAME", "KIND", "DETAIL", "USED BY"], &rows);
    Ok(())
}

/// Passwords piped on standard input, one per line, read once.
struct PipedPasswords(Option<Vec<String>>);

impl PipedPasswords {
    fn line(&mut self, index: usize) -> anyhow::Result<Option<String>> {
        if self.0.is_none() {
            let mut raw = String::new();
            std::io::stdin()
                .read_to_string(&mut raw)
                .context("failed to read passwords from standard input")?;
            self.0 = Some(raw.lines().map(str::to_string).collect());
        }
        Ok(self
            .0
            .as_ref()
            .and_then(|lines| lines.get(index))
            .filter(|line| !line.is_empty())
            .cloned())
    }
}

/// A password from its variable, a prompt, or line `index` of standard
/// input; `fallback` when standard input has no such line.
fn password(
    env: &str,
    prompt: &str,
    piped: &mut PipedPasswords,
    index: usize,
    fallback: Option<&str>,
) -> anyhow::Result<String> {
    if let Ok(value) = std::env::var(env)
        && !value.is_empty()
    {
        return Ok(value);
    }
    if std::io::stdin().is_terminal() {
        return admin_api::read_secret(admin_api::SecretSource::Stdin, prompt);
    }
    match (piped.line(index)?, fallback) {
        (Some(value), _) => Ok(value),
        (None, Some(fallback)) => Ok(fallback.to_string()),
        (None, None) => anyhow::bail!("{prompt}: set {env} or pipe it on standard input"),
    }
}

fn file_input(path: &Path) -> anyhow::Result<(String, String)> {
    let bytes =
        std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    anyhow::ensure!(!bytes.is_empty(), "{} is empty", path.display());
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_string();
    Ok((name, BASE64.encode(bytes)))
}

fn add_request(args: &SigningAddArgs) -> anyhow::Result<Value> {
    let name = args.name.trim();
    anyhow::ensure!(!name.is_empty(), "--name must not be empty");
    let mut piped = PipedPasswords(None);
    if let Some(keystore) = &args.keystore {
        let alias = args
            .alias
            .as_deref()
            .map(str::trim)
            .filter(|alias| !alias.is_empty())
            .context("--alias is required with --keystore")?;
        let (filename, encoded) = file_input(keystore)?;
        let store_password = password(
            KEYSTORE_PASSWORD_ENV,
            "Keystore password",
            &mut piped,
            0,
            None,
        )?;
        let key_password = password(
            KEY_PASSWORD_ENV,
            "Key password",
            &mut piped,
            1,
            Some(&store_password),
        )?;
        return Ok(json!({
            "name": name,
            "kind": "android_keystore",
            "android": {
                "keystore_filename": filename,
                "keystore_base64": encoded,
                "store_password": store_password,
                "key_alias": alias,
                "key_password": key_password,
            }
        }));
    }
    let p12 = args
        .p12
        .as_ref()
        .context("pass --keystore for Android, or --p12 and --profile for iOS")?;
    anyhow::ensure!(
        !args.profiles.is_empty(),
        "--p12 needs at least one --profile"
    );
    let (p12_filename, p12_base64) = file_input(p12)?;
    let profiles = args
        .profiles
        .iter()
        .map(|path| {
            file_input(path).map(|(filename, encoded)| {
                json!({ "profile_filename": filename, "profile_base64": encoded })
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let p12_password = password(
        P12_PASSWORD_ENV,
        "Certificate password",
        &mut piped,
        0,
        None,
    )?;
    Ok(json!({
        "name": name,
        "kind": "ios_distribution",
        "ios": {
            "p12_filename": p12_filename,
            "p12_base64": p12_base64,
            "p12_password": p12_password,
            "profiles": profiles,
        }
    }))
}

async fn add(args: SigningAddArgs) -> anyhow::Result<()> {
    let body = add_request(&args)?;
    let api = Api::connect(&args.connection)?;
    let saved = call(
        &api,
        reqwest::Method::POST,
        api_url(&api, &["projects", &args.project, "signing-identities"])?,
        Some(&body),
        "signing add",
    )
    .await?;
    if args.json {
        return print_json(&saved);
    }
    let identity: SigningIdentity = decode(&saved["identity"], "signing add")?;
    println!(
        "Added {} signing identity {} ({}): {}.",
        kind_label(identity.kind),
        identity.name,
        identity.id,
        identity_detail(&identity)
    );
    Ok(())
}

async fn remove(args: SigningRemoveArgs) -> anyhow::Result<()> {
    let id = args.identity_id.trim();
    confirm(
        args.yes,
        &format!(
            "Remove signing identity {id}? Builds that sign with it fail until another is picked."
        ),
    )?;
    let api = Api::connect(&args.connection)?;
    let response = call(
        &api,
        reqwest::Method::DELETE,
        api_url(&api, &["projects", &args.project, "signing-identities", id])?,
        None,
        "signing remove",
    )
    .await?;
    if args.json {
        return print_json(&response);
    }
    println!("Removed signing identity {id}.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_android_identity_reads_its_keystore_and_passwords_from_variables() {
        let dir = tempfile::tempdir().unwrap();
        let keystore = dir.path().join("upload.jks");
        std::fs::write(&keystore, b"keystore-bytes").unwrap();
        // SAFETY: tests in this module are the only readers of these names.
        unsafe {
            std::env::set_var(KEYSTORE_PASSWORD_ENV, "store-pw");
            std::env::set_var(KEY_PASSWORD_ENV, "key-pw");
        }
        let args = SigningAddArgs {
            project: "p1".into(),
            name: "upload".into(),
            keystore: Some(keystore),
            alias: Some("upload".into()),
            p12: None,
            profiles: Vec::new(),
            connection: ApiConnectionArgs::default(),
            json: false,
        };
        let body = add_request(&args).unwrap();
        assert_eq!(body["kind"], "android_keystore");
        assert_eq!(body["android"]["keystore_filename"], "upload.jks");
        assert_eq!(
            body["android"]["keystore_base64"],
            BASE64.encode(b"keystore-bytes")
        );
        assert_eq!(body["android"]["store_password"], "store-pw");
        assert_eq!(body["android"]["key_password"], "key-pw");
        // SAFETY: as above.
        unsafe {
            std::env::remove_var(KEYSTORE_PASSWORD_ENV);
            std::env::remove_var(KEY_PASSWORD_ENV);
        }
    }
}
