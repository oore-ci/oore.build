use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use oore_contract::KeyStorageMode;
use ring::aead::{self, AES_256_GCM, Aad, BoundKey, NONCE_LEN, Nonce, NonceSequence, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
#[cfg(target_os = "macos")]
use tracing::warn;

use crate::util::resolve_oored_data_dir;

/// Length of the AES-256 key in bytes.
const KEY_LEN: usize = 32;

/// A nonce sequence that uses a single pre-generated random nonce.
/// This is used for one-shot encryption operations where each call
/// generates a fresh random nonce.
struct SingleNonce(Option<[u8; NONCE_LEN]>);

impl NonceSequence for SingleNonce {
    fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
        let bytes = self.0.take().ok_or(ring::error::Unspecified)?;
        Ok(Nonce::assume_unique_for_key(bytes))
    }
}

/// Resolve the encryption key file path from the platform data directory.
///
/// The key is stored at `{data_root}/encryption.key`, the same path the
/// `oore` CLI uses.
pub fn resolve_key_path() -> anyhow::Result<PathBuf> {
    Ok(oore_contract::data_paths::encryption_key_path(
        &resolve_oored_data_dir()?,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    Keychain,
    KeychainMigratedFromFile,
    KeychainGenerated,
    LegacyFile,
    LegacyFileFallback,
}

impl KeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keychain => "keychain",
            Self::KeychainMigratedFromFile => "keychain_migrated_from_file",
            Self::KeychainGenerated => "keychain_generated",
            Self::LegacyFile => "legacy_file",
            Self::LegacyFileFallback => "legacy_file_fallback",
        }
    }
}

#[derive(Debug)]
pub struct RuntimeKey {
    pub key: Vec<u8>,
    pub source: KeySource,
    pub legacy_file_path: PathBuf,
}

/// Load the AES-256 encryption key, generating one only when allowed.
///
/// If the key file exists at `path`, reads and returns it. Otherwise, when
/// `allow_generate` is set, generates a new random 256-bit key, writes it to
/// `path` with restrictive permissions (0o600), and returns it. Callers pass
/// `allow_generate = false` when the database already holds ciphertext, since
/// a new key would make all of it unreadable.
pub fn load_or_generate_key(path: &Path, allow_generate: bool) -> anyhow::Result<Vec<u8>> {
    if path.exists() {
        let key = fs::read(path)
            .with_context(|| format!("failed to read encryption key: {}", path.display()))?;
        if key.len() != KEY_LEN {
            bail!(
                "encryption key at {} has invalid length: expected {} bytes, got {}",
                path.display(),
                KEY_LEN,
                key.len()
            );
        }
        return Ok(key);
    }

    if !allow_generate {
        bail!(
            "encryption key {} is missing, but the database already holds data encrypted with it. \
             Refusing to generate a new key, because every stored credential would become unreadable. \
             Restore the original key with `oore backup restore`, or set OORED_DATA_DIR to the \
             directory that holds it.",
            path.display()
        );
    }

    let key = generate_random_key()?;
    write_key_file(path, &key)?;
    Ok(key)
}

fn write_key_file(path: &Path, key: &[u8]) -> anyhow::Result<()> {
    if key.len() != KEY_LEN {
        bail!(
            "encryption key at {} has invalid length: expected {} bytes, got {}",
            path.display(),
            KEY_LEN,
            key.len()
        );
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    // Write key with restrictive permissions
    fs::write(path, key)
        .with_context(|| format!("failed to write encryption key: {}", path.display()))?;

    // Set file permissions to 0o600 (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to set permissions on: {}", path.display()))?;
    }

    Ok(())
}

fn generate_random_key() -> anyhow::Result<Vec<u8>> {
    let rng = SystemRandom::new();
    let mut key = vec![0u8; KEY_LEN];
    rng.fill(&mut key)
        .map_err(|_| anyhow::anyhow!("failed to generate random encryption key"))?;
    Ok(key)
}

/// Load the runtime key for `mode` from `legacy_file_path`. See
/// [`load_or_generate_key`] for `allow_generate`.
pub fn load_runtime_key_with_mode(
    mode: KeyStorageMode,
    legacy_file_path: PathBuf,
    allow_generate: bool,
) -> anyhow::Result<RuntimeKey> {
    #[cfg(target_os = "macos")]
    {
        if mode == KeyStorageMode::Keychain {
            warn!("keychain key storage is temporarily disabled; using file mode");
        }

        let key = load_or_generate_key(&legacy_file_path, allow_generate)?;
        Ok(RuntimeKey {
            key,
            source: KeySource::LegacyFile,
            legacy_file_path,
        })
    }

    #[cfg(not(target_os = "macos"))]
    {
        if mode == KeyStorageMode::Keychain {
            bail!("keychain mode is not supported on this platform");
        }
        let key = load_or_generate_key(&legacy_file_path, allow_generate)?;
        Ok(RuntimeKey {
            key,
            source: KeySource::LegacyFile,
            legacy_file_path,
        })
    }
}

pub fn persist_current_key_for_mode(key: &[u8], mode: KeyStorageMode) -> anyhow::Result<KeySource> {
    let legacy_file_path = resolve_key_path()?;

    match mode {
        KeyStorageMode::File => {
            write_key_file(&legacy_file_path, key)?;
            Ok(KeySource::LegacyFile)
        }
        KeyStorageMode::Keychain => bail!("keychain mode is disabled in this release"),
    }
}

/// Encrypt plaintext using AES-256-GCM.
///
/// Returns a base64-encoded string of `nonce || ciphertext || tag`.
pub fn encrypt(plaintext: &str, key: &[u8]) -> anyhow::Result<String> {
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| anyhow::anyhow!("failed to generate random nonce"))?;

    let unbound_key = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| anyhow::anyhow!("invalid encryption key"))?;
    let mut sealing_key = aead::SealingKey::new(unbound_key, SingleNonce(Some(nonce_bytes)));

    let mut in_out = plaintext.as_bytes().to_vec();
    sealing_key
        .seal_in_place_append_tag(Aad::empty(), &mut in_out)
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;

    // Prepend nonce to ciphertext+tag
    let mut result = Vec::with_capacity(NONCE_LEN + in_out.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&in_out);

    Ok(BASE64.encode(&result))
}

/// Decrypt a base64-encoded `nonce || ciphertext || tag` string using AES-256-GCM.
///
/// Returns the plaintext string.
pub fn decrypt(ciphertext_b64: &str, key: &[u8]) -> anyhow::Result<String> {
    let data = BASE64
        .decode(ciphertext_b64)
        .context("failed to decode base64 ciphertext")?;

    if data.len() < NONCE_LEN + AES_256_GCM.tag_len() {
        bail!("ciphertext too short to contain nonce and tag");
    }

    let (nonce_bytes, ciphertext_and_tag) = data.split_at(NONCE_LEN);
    let mut nonce_arr = [0u8; NONCE_LEN];
    nonce_arr.copy_from_slice(nonce_bytes);

    let unbound_key = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| anyhow::anyhow!("invalid encryption key"))?;
    let mut opening_key = aead::OpeningKey::new(unbound_key, SingleNonce(Some(nonce_arr)));

    let mut in_out = ciphertext_and_tag.to_vec();
    let plaintext = opening_key
        .open_in_place(Aad::empty(), &mut in_out)
        .map_err(|_| anyhow::anyhow!("decryption failed: invalid key or corrupted data"))?;

    String::from_utf8(plaintext.to_vec()).context("decrypted data is not valid UTF-8")
}
