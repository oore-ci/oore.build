use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use oore_contract::KeyStorageMode;
use ring::aead::{self, AES_256_GCM, Aad, BoundKey, NONCE_LEN, Nonce, NonceSequence, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
#[cfg(target_os = "macos")]
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};
#[cfg(target_os = "macos")]
use tracing::warn;

/// Length of the AES-256 key in bytes.
const KEY_LEN: usize = 32;
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "build.oore.oored";
#[cfg(target_os = "macos")]
const KEYCHAIN_ACCOUNT: &str = "encryption-key-v1";
#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
#[cfg(target_os = "macos")]
const ERR_SEC_DUPLICATE_ITEM: i32 = -25299;

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
/// The key is stored at `{data_dir}/oore/encryption.key`.
pub fn resolve_key_path() -> anyhow::Result<PathBuf> {
    let data_dir =
        dirs::data_dir().context("could not determine platform data directory (dirs::data_dir)")?;
    Ok(data_dir.join("oore").join("encryption.key"))
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

/// Load or generate the AES-256 encryption key.
///
/// If the key file exists at `path`, reads and returns it.
/// Otherwise, generates a new random 256-bit key, writes it to `path`
/// with restrictive permissions (0o600), and returns it.
pub fn load_or_generate_key(path: &Path) -> anyhow::Result<Vec<u8>> {
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
    fs::write(path, &key)
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

#[cfg(target_os = "macos")]
fn load_key_from_keychain() -> anyhow::Result<Option<Vec<u8>>> {
    match get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        Ok(key) => {
            if key.len() != KEY_LEN {
                bail!(
                    "keychain encryption key has invalid length: expected {} bytes, got {}",
                    KEY_LEN,
                    key.len()
                );
            }
            Ok(Some(key))
        }
        Err(err) if err.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
        Err(err) => Err(anyhow::anyhow!(
            "failed to read encryption key from macOS Keychain: {err}"
        )),
    }
}

#[cfg(target_os = "macos")]
fn save_key_to_keychain(key: &[u8]) -> anyhow::Result<()> {
    if key.len() != KEY_LEN {
        bail!(
            "cannot store encryption key in keychain: invalid key length {}",
            key.len()
        );
    }

    match set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, key) {
        Ok(_) => Ok(()),
        Err(err) if err.code() == ERR_SEC_DUPLICATE_ITEM => {
            delete_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
                .context("failed to remove existing keychain encryption key item")?;
            set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, key)
                .context("failed to update keychain encryption key item")
        }
        Err(err) => Err(anyhow::anyhow!(
            "failed to write encryption key to macOS Keychain: {err}"
        )),
    }
}

pub fn load_runtime_key() -> anyhow::Result<RuntimeKey> {
    load_runtime_key_with_mode(default_key_storage_mode())
}

pub fn load_runtime_key_with_mode(mode: KeyStorageMode) -> anyhow::Result<RuntimeKey> {
    let legacy_file_path = resolve_key_path()?;

    #[cfg(target_os = "macos")]
    {
        match mode {
            KeyStorageMode::File => {
                let key = load_or_generate_key(&legacy_file_path)?;
                return Ok(RuntimeKey {
                    key,
                    source: KeySource::LegacyFile,
                    legacy_file_path,
                });
            }
            KeyStorageMode::Keychain => {
                if let Some(key) = load_key_from_keychain()? {
                    return Ok(RuntimeKey {
                        key,
                        source: KeySource::Keychain,
                        legacy_file_path,
                    });
                }

                if legacy_file_path.exists() {
                    let key = load_or_generate_key(&legacy_file_path)?;
                    match save_key_to_keychain(&key) {
                        Ok(_) => {
                            return Ok(RuntimeKey {
                                key,
                                source: KeySource::KeychainMigratedFromFile,
                                legacy_file_path,
                            });
                        }
                        Err(err) => {
                            warn!(
                                error = %err,
                                "failed to migrate encryption key to keychain; continuing with legacy file key"
                            );
                            return Ok(RuntimeKey {
                                key,
                                source: KeySource::LegacyFileFallback,
                                legacy_file_path,
                            });
                        }
                    }
                }

                let generated = generate_random_key()?;
                match save_key_to_keychain(&generated) {
                    Ok(_) => Ok(RuntimeKey {
                        key: generated,
                        source: KeySource::KeychainGenerated,
                        legacy_file_path,
                    }),
                    Err(err) => {
                        warn!(
                            error = %err,
                            "failed to persist generated encryption key to keychain; falling back to legacy file storage"
                        );
                        let key = load_or_generate_key(&legacy_file_path)?;
                        Ok(RuntimeKey {
                            key,
                            source: KeySource::LegacyFileFallback,
                            legacy_file_path,
                        })
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if mode == KeyStorageMode::Keychain {
            bail!("keychain mode is not supported on this platform");
        }
        let key = load_or_generate_key(&legacy_file_path)?;
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
        KeyStorageMode::Keychain => {
            #[cfg(target_os = "macos")]
            {
                save_key_to_keychain(key)?;
                Ok(KeySource::Keychain)
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = key;
                bail!("keychain mode is not supported on this platform")
            }
        }
    }
}

pub fn default_key_storage_mode() -> KeyStorageMode {
    #[cfg(target_os = "macos")]
    {
        KeyStorageMode::Keychain
    }

    #[cfg(not(target_os = "macos"))]
    {
        KeyStorageMode::File
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [0x42u8; KEY_LEN];
        let plaintext = "my-super-secret-client-secret-123";

        let encrypted = encrypt(plaintext, &key).unwrap();
        assert_ne!(encrypted, plaintext);

        let decrypted = decrypt(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext_each_time() {
        let key = [0x42u8; KEY_LEN];
        let plaintext = "same-plaintext";

        let a = encrypt(plaintext, &key).unwrap();
        let b = encrypt(plaintext, &key).unwrap();
        // Different nonces should produce different ciphertext
        assert_ne!(a, b);

        // But both should decrypt to the same value
        assert_eq!(decrypt(&a, &key).unwrap(), plaintext);
        assert_eq!(decrypt(&b, &key).unwrap(), plaintext);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let key1 = [0x42u8; KEY_LEN];
        let key2 = [0x43u8; KEY_LEN];
        let plaintext = "secret";

        let encrypted = encrypt(plaintext, &key1).unwrap();
        assert!(decrypt(&encrypted, &key2).is_err());
    }

    #[test]
    fn test_decrypt_corrupted_data_fails() {
        let key = [0x42u8; KEY_LEN];
        let plaintext = "secret";

        let encrypted = encrypt(plaintext, &key).unwrap();
        let mut data = BASE64.decode(&encrypted).unwrap();
        // Corrupt a byte
        if let Some(byte) = data.last_mut() {
            *byte ^= 0xFF;
        }
        let corrupted = BASE64.encode(&data);
        assert!(decrypt(&corrupted, &key).is_err());
    }

    #[test]
    fn test_load_or_generate_key_creates_new_key() {
        let tmp = tempfile::TempDir::new().unwrap();
        let key_path = tmp.path().join("test.key");

        let key = load_or_generate_key(&key_path).unwrap();
        assert_eq!(key.len(), KEY_LEN);
        assert!(key_path.exists());

        // Loading again should return the same key
        let key2 = load_or_generate_key(&key_path).unwrap();
        assert_eq!(key, key2);
    }

    #[test]
    fn test_load_or_generate_key_rejects_bad_length() {
        let tmp = tempfile::TempDir::new().unwrap();
        let key_path = tmp.path().join("bad.key");
        fs::write(&key_path, &[0u8; 16]).unwrap(); // 16 bytes instead of 32

        assert!(load_or_generate_key(&key_path).is_err());
    }
}
