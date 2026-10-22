use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::Context;
use oore_cli_ui::Operation;

use super::{
    InstallProfile, atomic_replace_directory, atomic_replace_file, copy_dir_recursive,
    parse_checksum, release_arch, sha256_file,
};

const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CHECKSUM_BYTES: u64 = 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 50_000;
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const PROBE_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactKind {
    Full,
    Web,
    Cli,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReleaseProvenance {
    pub(crate) version: String,
    pub(crate) channel: String,
    pub(crate) repository: String,
    pub(crate) archive: String,
    pub(crate) sha256: String,
}

pub(crate) struct PreparedInstall {
    provenance: ReleaseProvenance,
    release: Option<PreparedRelease>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PayloadSelection {
    control_plane: bool,
    web: bool,
}

struct PreparedRelease {
    stage: tempfile::TempDir,
}

impl PreparedInstall {
    pub(crate) fn provenance(&self) -> &ReleaseProvenance {
        &self.provenance
    }
}

impl PayloadSelection {
    pub(crate) const fn new(control_plane: bool, web: bool) -> Self {
        Self { control_plane, web }
    }
}

impl ArtifactKind {
    fn for_profile(profile: InstallProfile) -> Option<Self> {
        match profile {
            InstallProfile::Complete | InstallProfile::ControlPlane => Some(Self::Full),
            InstallProfile::WebNode => Some(Self::Web),
            InstallProfile::Runner | InstallProfile::CliOnly => None,
        }
    }

    fn archive_name(self, version: &str, arch: &str) -> String {
        match self {
            Self::Full => format!("oore_{version}_darwin_{arch}.tar.gz"),
            Self::Web => format!("oore-web_{version}_darwin_{arch}.tar.gz"),
            Self::Cli => format!("oore-cli_{version}_darwin_{arch}.tar.gz"),
        }
    }

    fn allows_path(self, path: &Path) -> bool {
        if path == Path::new("bin") {
            return true;
        }
        if matches!(path.to_str(), Some("VERSION" | "LICENSE")) {
            return true;
        }
        match self {
            Self::Full => {
                matches!(
                    path.to_str(),
                    Some("bin/oore" | "bin/oored" | "bin/oore-web")
                ) || path.starts_with("web-dist")
            }
            Self::Web => path == Path::new("bin/oore-web") || path.starts_with("web-dist"),
            Self::Cli => path == Path::new("bin/oore"),
        }
    }
}

pub(crate) async fn prepare(
    profile: InstallProfile,
    install_root: &Path,
    staged_archive: Option<&Path>,
    operation: &Operation,
) -> anyhow::Result<PreparedInstall> {
    let bootstrap = read_bootstrap_release(install_root)?;
    let version = bootstrap.version;
    let channel = bootstrap.channel;
    let repository = bootstrap.repository;
    let arch = release_arch()?;
    if staged_archive.is_some() {
        require_unsigned_staged_archive_acceptance(&version)?;
    }
    let manifest_sha256 = read_bootstrap_manifest_sha256(install_root)?;

    if let Some(kind) = ArtifactKind::for_profile(profile) {
        let archive_name = kind.archive_name(&version, arch);
        operation.update(format!("Preparing {archive_name}"));
        let (archive, sha256, temporary_download) = match staged_archive {
            Some(path) => {
                validate_staged_archive(path, &archive_name)?;
                (path.to_path_buf(), sha256_file(path)?, None)
            }
            None => {
                let download = download_exact_release(
                    &repository,
                    &version,
                    &archive_name,
                    &manifest_sha256,
                    operation,
                )
                .await?;
                let archive = download.path().join(&archive_name);
                let sha256 = sha256_file(&archive)?;
                (archive, sha256, Some(download))
            }
        };

        operation.update("Checking the release archive");
        let stage = extract_archive(&archive, kind).await?;
        drop(temporary_download);
        validate_stage(stage.path(), kind, &version)?;
        if matches!(kind, ArtifactKind::Full | ArtifactKind::Cli) {
            require_release_cli_matches_bootstrap(stage.path(), install_root)?;
        }
        Ok(PreparedInstall {
            provenance: ReleaseProvenance {
                version,
                channel,
                repository,
                archive: archive_name,
                sha256,
            },
            release: Some(PreparedRelease { stage }),
        })
    } else if let Some(path) = staged_archive {
        let kind = ArtifactKind::Cli;
        let archive_name = kind.archive_name(&version, arch);
        validate_staged_archive(path, &archive_name)?;
        let sha256 = sha256_file(path)?;
        let stage = extract_archive(path, kind).await?;
        validate_stage(stage.path(), kind, &version)?;
        require_release_cli_matches_bootstrap(stage.path(), install_root)?;
        Ok(PreparedInstall {
            provenance: ReleaseProvenance {
                version,
                channel,
                repository,
                archive: archive_name,
                sha256,
            },
            release: Some(PreparedRelease { stage }),
        })
    } else {
        Ok(PreparedInstall {
            provenance: ReleaseProvenance {
                version,
                channel,
                repository,
                archive: bootstrap.archive,
                sha256: bootstrap.sha256,
            },
            release: None,
        })
    }
}

fn read_bootstrap_manifest_sha256(install_root: &Path) -> anyhow::Result<String> {
    let sha256 = read_owned_metadata(install_root, "BOOTSTRAP_MANIFEST_SHA256").context(
        "signed release manifest metadata is missing; reinstall the Oore CLI before selecting this profile",
    )?;
    validate_sha256(&sha256)?;
    Ok(sha256)
}

fn require_unsigned_staged_archive_acceptance(version: &str) -> anyhow::Result<()> {
    let accepted = version.ends_with("-dev")
        && std::env::var("OORE_ALLOW_UNSIGNED_LOCAL_RELEASE").as_deref() == Ok("true")
        && std::env::var("OORE_ALLOW_DEV_RELEASE_TAG_FOR_LOCAL_ACCEPTANCE").as_deref()
            == Ok("true")
        && std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true");
    anyhow::ensure!(
        accepted,
        "--staged-archive is an unsigned local acceptance path; it requires a -dev bootstrap, OORE_ALLOW_UNSIGNED_LOCAL_RELEASE=true, and OORE_ALLOW_DEV_RELEASE_TAG_FOR_LOCAL_ACCEPTANCE=true, and it is disabled in GitHub Actions"
    );
    Ok(())
}

pub(crate) fn read_bootstrap_release(install_root: &Path) -> anyhow::Result<ReleaseProvenance> {
    validate_install_root(install_root)?;
    let version = read_owned_metadata(install_root, "VERSION")?;
    semver::Version::parse(&version)
        .with_context(|| format!("installed VERSION is not a supported release: {version}"))?;
    let channel = read_owned_metadata(install_root, "CHANNEL")?;
    anyhow::ensure!(
        matches!(channel.as_str(), "stable" | "beta" | "alpha"),
        "installed CHANNEL must be stable, beta, or alpha"
    );
    let repository = read_owned_metadata(install_root, "GITHUB_REPO")?;
    validate_repository(&repository)?;
    let arch = release_arch()?;
    let archive = read_owned_metadata(install_root, "BOOTSTRAP_ARCHIVE").context(
        "bootstrap release metadata is missing; reinstall the Oore CLI before selecting this profile",
    )?;
    let expected_cli_archive = ArtifactKind::Cli.archive_name(&version, arch);
    let expected_full_archive = ArtifactKind::Full.archive_name(&version, arch);
    anyhow::ensure!(
        archive == expected_cli_archive || archive == expected_full_archive,
        "bootstrap archive does not match the installed release: {archive}; expected {expected_cli_archive} or {expected_full_archive}"
    );
    let sha256 = read_owned_metadata(install_root, "BOOTSTRAP_SHA256").context(
        "bootstrap checksum metadata is missing; reinstall the Oore CLI before selecting this profile",
    )?;
    validate_sha256(&sha256)?;
    Ok(ReleaseProvenance {
        version,
        channel,
        repository,
        archive,
        sha256,
    })
}

pub(crate) fn apply<F>(
    prepared: &PreparedInstall,
    profile: InstallProfile,
    payload: PayloadSelection,
    install_root: &Path,
    publish_manifest: F,
) -> anyhow::Result<ReleaseProvenance>
where
    F: FnOnce(&ReleaseProvenance) -> anyhow::Result<()>,
{
    validate_install_root(install_root)?;
    let affected = affected_paths(payload);
    let snapshot = snapshot_paths(install_root, &affected)?;
    let result: anyhow::Result<ReleaseProvenance> = (|| {
        if let Some(release) = &prepared.release {
            install_selected_payload(release.stage.path(), install_root, payload)?;
        }
        verify_installed_profile(install_root, profile)?;
        publish_manifest(&prepared.provenance)?;
        Ok(prepared.provenance.clone())
    })();

    if let Err(error) = result {
        if let Err(rollback_error) = restore_snapshot(install_root, snapshot.path(), &affected) {
            return Err(error.context(format!(
                "restoring the previous installation also failed: {rollback_error:#}"
            )));
        }
        return Err(error);
    }
    Ok(prepared.provenance.clone())
}

pub(crate) fn shell_path_files(install_root: &Path) -> anyhow::Result<Vec<String>> {
    let path = install_root.join("SHELL_PATH_FILE");
    match fs::symlink_metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    }
    let value = read_owned_metadata(install_root, "SHELL_PATH_FILE")?;
    if value.is_empty() {
        return Ok(Vec::new());
    }
    anyhow::ensure!(
        matches!(value.as_str(), ".zshrc" | ".bashrc" | ".bash_profile"),
        "unsupported shell PATH file in bootstrap metadata: {value}"
    );
    Ok(vec![value])
}

fn affected_paths(payload: PayloadSelection) -> Vec<&'static str> {
    let mut paths = BTreeSet::from(["install-manifest.json"]);
    if payload.control_plane {
        paths.extend(["bin/oored", "LICENSE"]);
    }
    if payload.web {
        paths.extend([
            "bin/oore-web",
            "web-dist",
            "WEB_VERSION",
            "WEB_CHANNEL",
            "WEB_GITHUB_REPO",
            "LICENSE",
        ]);
    }
    paths.into_iter().collect()
}

fn install_selected_payload(
    stage: &Path,
    install_root: &Path,
    payload: PayloadSelection,
) -> anyhow::Result<()> {
    if payload.control_plane {
        atomic_replace_file(
            &stage.join("bin/oored"),
            &install_root.join("bin/oored"),
            true,
        )?;
    }
    if payload.web {
        atomic_replace_file(
            &stage.join("bin/oore-web"),
            &install_root.join("bin/oore-web"),
            true,
        )?;
        atomic_replace_directory(&stage.join("web-dist"), &install_root.join("web-dist"))?;
        atomic_replace_file(
            &stage.join("VERSION"),
            &install_root.join("WEB_VERSION"),
            false,
        )?;
        write_metadata_atomic(
            &install_root.join("WEB_CHANNEL"),
            &read_owned_metadata(install_root, "CHANNEL")?,
        )?;
        write_metadata_atomic(
            &install_root.join("WEB_GITHUB_REPO"),
            &read_owned_metadata(install_root, "GITHUB_REPO")?,
        )?;
    }
    if payload.control_plane || payload.web {
        atomic_replace_file(&stage.join("LICENSE"), &install_root.join("LICENSE"), false)?;
    }
    Ok(())
}

fn verify_installed_profile(install_root: &Path, profile: InstallProfile) -> anyhow::Result<()> {
    let cli = install_root.join("bin/oore");
    require_owned_regular_file(&cli, true)?;
    run_probe(&cli, &["--help"], "installed Oore CLI")?;
    if matches!(
        profile,
        InstallProfile::Complete | InstallProfile::ControlPlane
    ) {
        let daemon = install_root.join("bin/oored");
        require_owned_regular_file(&daemon, true)?;
        run_probe(&daemon, &["package-version"], "installed control plane")?;
    }
    if matches!(profile, InstallProfile::Complete | InstallProfile::WebNode) {
        let web = install_root.join("bin/oore-web");
        require_owned_regular_file(&web, true)?;
        run_probe(&web, &["--help"], "installed web server")?;
        validate_owned_tree(&install_root.join("web-dist"))?;
        anyhow::ensure!(
            install_root.join("web-dist/index.html").is_file(),
            "installed web assets are missing index.html"
        );
    }
    Ok(())
}

pub(crate) fn verify_profile(install_root: &Path, profile: InstallProfile) -> anyhow::Result<()> {
    validate_install_root(install_root)?;
    verify_installed_profile(install_root, profile)
}

fn require_release_cli_matches_bootstrap(stage: &Path, install_root: &Path) -> anyhow::Result<()> {
    let staged_cli = stage.join("bin/oore");
    let installed_cli = install_root.join("bin/oore");
    let staged_sha256 = sha256_file(&staged_cli)?;
    let installed_sha256 = sha256_file(&installed_cli)?;
    anyhow::ensure!(
        staged_sha256 == installed_sha256,
        "release bin/oore does not match the installed bootstrap CLI"
    );
    Ok(())
}

fn run_probe(executable: &Path, args: &[&str], name: &str) -> anyhow::Result<()> {
    let mut child = Command::new(executable)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to start {name}"))?;
    let deadline = Instant::now() + PROBE_TIMEOUT;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                anyhow::ensure!(status.success(), "{name} did not start successfully");
                return Ok(());
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(PROBE_POLL_INTERVAL);
            }
            Ok(None) => {
                if let Err(kill_error) = child.kill() {
                    match child.try_wait() {
                        Ok(Some(_)) => {}
                        Ok(None) => {
                            return Err(kill_error).with_context(|| {
                                format!("failed to stop {name} after its startup probe timed out")
                            });
                        }
                        Err(wait_error) => {
                            return Err(kill_error).context(format!(
                                "failed to stop {name} after its startup probe timed out; inspecting the child also failed: {wait_error}"
                            ));
                        }
                    }
                }
                child.wait().with_context(|| {
                    format!("failed to reap {name} after its startup probe timed out")
                })?;
                anyhow::bail!(
                    "{name} did not finish its startup probe within {} seconds",
                    PROBE_TIMEOUT.as_secs()
                );
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error).with_context(|| format!("failed to inspect {name}"));
            }
        }
    }
}

async fn download_exact_release(
    repository: &str,
    version: &str,
    archive_name: &str,
    manifest_sha256: &str,
    operation: &Operation,
) -> anyhow::Result<tempfile::TempDir> {
    let base = std::env::var("OORE_RELEASE_BASE_URL")
        .unwrap_or_else(|_| format!("https://github.com/{repository}/releases/download"));
    validate_release_base_url(&base)?;
    let base = format!("{}/v{version}", base.trim_end_matches('/'));
    let checksums_name = format!("oore_{version}_checksums.txt");
    let temporary = tempfile::tempdir().context("failed to prepare the release download")?;
    let archive_path = temporary.path().join(archive_name);
    let checksums_path = temporary.path().join(&checksums_name);
    let client = reqwest::Client::builder()
        .user_agent(format!("oore/{}/install", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .build()
        .context("failed to create the release client")?;

    operation.update("Checking the signed release manifest");
    download_file_limited(
        &client,
        &format!("{base}/{checksums_name}"),
        &checksums_path,
        MAX_CHECKSUM_BYTES,
    )
    .await?;

    let checksums =
        fs::read_to_string(&checksums_path).context("release checksum file is not valid UTF-8")?;
    let actual_manifest_sha256 = sha256_file(&checksums_path)?;
    anyhow::ensure!(
        actual_manifest_sha256 == manifest_sha256,
        "release manifest does not match the manifest verified by the bootstrap installer"
    );
    let expected = parse_checksum(&checksums, archive_name)?;
    validate_sha256(&expected)?;

    operation.update(format!("Downloading {archive_name}"));
    download_file_limited(
        &client,
        &format!("{base}/{archive_name}"),
        &archive_path,
        MAX_ARCHIVE_BYTES,
    )
    .await?;
    let actual = sha256_file(&archive_path)?;
    anyhow::ensure!(
        actual == expected,
        "SHA-256 verification failed for {archive_name}"
    );
    Ok(temporary)
}

async fn download_file_limited(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
    limit: u64,
) -> anyhow::Result<()> {
    let mut response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to download {url}"))?
        .error_for_status()
        .with_context(|| format!("release download failed: {url}"))?;
    if let Some(length) = response.content_length() {
        anyhow::ensure!(length <= limit, "release download exceeds the size limit");
    }
    let mut file = File::create(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let mut written = 0_u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("failed to read {url}"))?
    {
        written = written
            .checked_add(chunk.len() as u64)
            .context("release download size overflow")?;
        anyhow::ensure!(written <= limit, "release download exceeds the size limit");
        file.write_all(&chunk)
            .with_context(|| format!("failed to write {}", destination.display()))?;
    }
    file.sync_all()
        .with_context(|| format!("failed to sync {}", destination.display()))?;
    Ok(())
}

async fn extract_archive(archive: &Path, kind: ArtifactKind) -> anyhow::Result<tempfile::TempDir> {
    let stage = tempfile::tempdir().context("failed to prepare release extraction")?;
    let archive = archive.to_path_buf();
    let destination = stage.path().to_path_buf();
    tokio::task::spawn_blocking(move || unpack_archive(&archive, &destination, kind))
        .await
        .context("release extraction task failed")??;
    Ok(stage)
}

fn unpack_archive(archive: &Path, destination: &Path, kind: ArtifactKind) -> anyhow::Result<()> {
    let file = File::open(archive)
        .with_context(|| format!("failed to open release archive {}", archive.display()))?;
    unpack_archive_from(file, destination, kind)
}

/// Unpacks a verified full release archive with the installer's entry, path
/// and size checks, then confirms its layout and that its `VERSION` is
/// `version`. `oore update` uses this for the archive it downloaded.
pub(crate) fn unpack_full_release(
    archive: impl std::io::Read,
    destination: &Path,
    version: &str,
) -> anyhow::Result<()> {
    unpack_archive_from(archive, destination, ArtifactKind::Full)?;
    validate_stage(destination, ArtifactKind::Full, version)
}

fn unpack_archive_from(
    archive: impl std::io::Read,
    destination: &Path,
    kind: ArtifactKind,
) -> anyhow::Result<()> {
    let decoder = flate2::read::GzDecoder::new(archive);
    let mut archive = tar::Archive::new(decoder);
    archive.set_preserve_permissions(false);
    archive.set_preserve_ownerships(false);
    let mut extracted_bytes = 0_u64;
    let mut entries_seen = 0_usize;
    let mut paths_seen = BTreeSet::new();
    for entry in archive
        .entries()
        .context("failed to read release archive")?
    {
        let mut entry = entry.context("failed to read release archive entry")?;
        entries_seen += 1;
        anyhow::ensure!(
            entries_seen <= MAX_ARCHIVE_ENTRIES,
            "release archive contains too many entries"
        );
        let path = normalized_archive_path(&entry.path().context("invalid archive path")?)?;
        if path.as_os_str().is_empty() {
            continue;
        }
        anyhow::ensure!(
            paths_seen.insert(path.clone()),
            "release archive contains a duplicate path: {}",
            path.display()
        );
        anyhow::ensure!(
            kind.allows_path(&path),
            "unexpected release archive path: {}",
            path.display()
        );
        let entry_type = entry.header().entry_type();
        anyhow::ensure!(
            entry_type.is_file() || entry_type.is_dir(),
            "unsupported release archive entry type: {}",
            path.display()
        );
        if entry_type.is_file() {
            extracted_bytes = extracted_bytes
                .checked_add(entry.size())
                .context("release archive size overflow")?;
            anyhow::ensure!(
                extracted_bytes <= MAX_EXTRACTED_BYTES,
                "release archive exceeds the extracted size limit"
            );
        }
        anyhow::ensure!(
            entry.unpack_in(destination)?,
            "unsafe release archive path: {}",
            path.display()
        );
    }
    Ok(())
}

fn normalized_archive_path(path: &Path) -> anyhow::Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!(
                    "release archive contains an unsafe path: {}",
                    path.display()
                )
            }
        }
    }
    Ok(normalized)
}

fn validate_stage(stage: &Path, kind: ArtifactKind, version: &str) -> anyhow::Result<()> {
    require_owned_regular_file(&stage.join("VERSION"), false)?;
    let archived = fs::read_to_string(stage.join("VERSION"))?;
    anyhow::ensure!(
        archived.trim() == version,
        "release archive VERSION {} does not match the expected release {version}",
        archived.trim()
    );
    require_owned_regular_file(&stage.join("LICENSE"), false)?;
    match kind {
        ArtifactKind::Full => {
            for binary in ["oore", "oored", "oore-web"] {
                require_owned_regular_file(&stage.join("bin").join(binary), false)?;
            }
            validate_owned_tree(&stage.join("web-dist"))?;
            require_owned_regular_file(&stage.join("web-dist/index.html"), false)?;
        }
        ArtifactKind::Web => {
            require_owned_regular_file(&stage.join("bin/oore-web"), false)?;
            validate_owned_tree(&stage.join("web-dist"))?;
            require_owned_regular_file(&stage.join("web-dist/index.html"), false)?;
        }
        ArtifactKind::Cli => require_owned_regular_file(&stage.join("bin/oore"), false)?,
    }
    Ok(())
}

fn snapshot_paths(install_root: &Path, paths: &[&str]) -> anyhow::Result<tempfile::TempDir> {
    let snapshot = tempfile::Builder::new()
        .prefix(".install-snapshot-")
        .tempdir_in(install_root)
        .context("failed to create the installation snapshot")?;
    for relative in paths {
        let source = install_root.join(relative);
        let destination = snapshot.path().join(relative);
        let metadata = match fs::symlink_metadata(&source) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", source.display()));
            }
        };
        anyhow::ensure!(
            metadata.uid() == current_effective_uid(),
            "existing install path has an unexpected owner: {}",
            source.display()
        );
        if metadata.file_type().is_file() {
            fs::create_dir_all(
                destination
                    .parent()
                    .context("snapshot path has no parent")?,
            )?;
            fs::copy(&source, &destination)?;
        } else if metadata.file_type().is_dir() {
            validate_owned_tree(&source)?;
            copy_dir_recursive(&source, &destination)?;
        } else {
            anyhow::bail!("existing install path is unsafe: {}", source.display());
        }
    }
    Ok(snapshot)
}

fn restore_snapshot(install_root: &Path, snapshot: &Path, paths: &[&str]) -> anyhow::Result<()> {
    for relative in paths {
        let source = snapshot.join(relative);
        let destination = install_root.join(relative);
        if source.is_file() {
            atomic_replace_file(&source, &destination, relative.starts_with("bin/"))?;
        } else if source.is_dir() {
            atomic_replace_directory(&source, &destination)?;
        } else {
            match fs::symlink_metadata(&destination) {
                Ok(metadata) if metadata.file_type().is_file() => {
                    fs::remove_file(&destination)?;
                }
                Ok(metadata) if metadata.file_type().is_dir() => {
                    fs::remove_dir_all(&destination)?;
                }
                Ok(_) => {
                    anyhow::bail!(
                        "cannot restore unsafe install path: {}",
                        destination.display()
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to inspect {}", destination.display()));
                }
            }
        }
    }
    Ok(())
}

fn validate_install_root(install_root: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        install_root.is_absolute(),
        "the Oore install root must be absolute"
    );
    let metadata = fs::symlink_metadata(install_root)
        .with_context(|| format!("failed to inspect install root {}", install_root.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_dir(),
        "the Oore install root is not a regular directory: {}",
        install_root.display()
    );
    anyhow::ensure!(
        metadata.uid() == current_effective_uid(),
        "the Oore install root has an unexpected owner: {}",
        install_root.display()
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o022 == 0,
        "the Oore install root is writable by another user: {}",
        install_root.display()
    );
    require_owned_directory(&install_root.join("bin"))?;
    require_owned_regular_file(&install_root.join("bin/oore"), true)
}

fn require_owned_directory(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("required directory is missing: {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_dir(),
        "path is not a regular directory: {}",
        path.display()
    );
    anyhow::ensure!(
        metadata.uid() == current_effective_uid(),
        "path has an unexpected owner: {}",
        path.display()
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o022 == 0,
        "directory is writable by another user: {}",
        path.display()
    );
    Ok(())
}

fn validate_staged_archive(path: &Path, expected_name: &str) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect staged archive {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file(),
        "staged archive is not a regular file"
    );
    anyhow::ensure!(
        metadata.uid() == current_effective_uid() && metadata.permissions().mode() & 0o022 == 0,
        "staged archive has unsafe ownership or permissions"
    );
    anyhow::ensure!(
        path.file_name() == Some(OsStr::new(expected_name)),
        "staged archive must be named {expected_name}"
    );
    anyhow::ensure!(
        metadata.len() <= MAX_ARCHIVE_BYTES,
        "staged archive exceeds the size limit"
    );
    Ok(())
}

fn read_owned_metadata(install_root: &Path, name: &str) -> anyhow::Result<String> {
    let path = install_root.join(name);
    require_owned_regular_file(&path, false)?;
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    anyhow::ensure!(
        metadata.len() <= 4096,
        "installed release metadata is too large: {}",
        path.display()
    );
    let value =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let value = value.strip_suffix('\n').unwrap_or(&value);
    anyhow::ensure!(
        !value.is_empty() && !value.contains(['\n', '\r']) && value == value.trim(),
        "installed release metadata must contain one non-empty line: {}",
        path.display()
    );
    Ok(value.to_string())
}

pub(crate) fn write_metadata_atomic(path: &Path, value: &str) -> anyhow::Result<()> {
    let parent = path.parent().context("metadata path has no parent")?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(value.as_bytes())?;
    staged.write_all(b"\n")?;
    staged.as_file_mut().sync_all()?;
    staged.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn require_owned_regular_file(path: &Path, executable: bool) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("required file is missing: {}", path.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file(),
        "path is not a regular file: {}",
        path.display()
    );
    anyhow::ensure!(
        metadata.uid() == current_effective_uid(),
        "path has an unexpected owner: {}",
        path.display()
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o022 == 0,
        "file is writable by another user: {}",
        path.display()
    );
    if executable {
        anyhow::ensure!(
            metadata.permissions().mode() & 0o111 != 0,
            "file is not executable: {}",
            path.display()
        );
    }
    Ok(())
}

fn validate_owned_tree(root: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(root)
        .with_context(|| format!("required directory is missing: {}", root.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_dir(),
        "path is not a regular directory: {}",
        root.display()
    );
    anyhow::ensure!(
        metadata.uid() == current_effective_uid(),
        "path has an unexpected owner: {}",
        root.display()
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o022 == 0,
        "directory is writable by another user: {}",
        root.display()
    );
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        anyhow::ensure!(
            metadata.uid() == current_effective_uid(),
            "path has an unexpected owner: {}",
            path.display()
        );
        anyhow::ensure!(
            metadata.permissions().mode() & 0o022 == 0,
            "path is writable by another user: {}",
            path.display()
        );
        if metadata.file_type().is_dir() {
            validate_owned_tree(&path)?;
        } else if !metadata.file_type().is_file() {
            anyhow::bail!("directory contains an unsafe entry: {}", path.display());
        }
    }
    Ok(())
}

fn validate_repository(repository: &str) -> anyhow::Result<()> {
    let mut segments = repository.split('/');
    let owner = segments.next().unwrap_or_default();
    let name = segments.next().unwrap_or_default();
    anyhow::ensure!(
        !owner.is_empty() && !name.is_empty() && segments.next().is_none(),
        "installed GITHUB_REPO must use owner/name format"
    );
    anyhow::ensure!(
        [owner, name].into_iter().all(|segment| segment
            .bytes()
            .all(|byte| { byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') })),
        "installed GITHUB_REPO contains unsupported characters"
    );
    Ok(())
}

fn validate_sha256(value: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "release SHA-256 must contain 64 lowercase hexadecimal characters"
    );
    Ok(())
}

fn validate_release_base_url(value: &str) -> anyhow::Result<()> {
    let url = url::Url::parse(value).context("OORE_RELEASE_BASE_URL is not a valid URL")?;
    anyhow::ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "OORE_RELEASE_BASE_URL must not contain credentials, a query, or a fragment"
    );
    let loopback = match url.host().context("OORE_RELEASE_BASE_URL has no host")? {
        url::Host::Ipv4(address) => address.is_loopback(),
        url::Host::Ipv6(address) => address.is_loopback(),
        url::Host::Domain(_) => false,
    };
    anyhow::ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && loopback),
        "OORE_RELEASE_BASE_URL must use HTTPS, except for a loopback test server"
    );
    Ok(())
}

fn current_effective_uid() -> u32 {
    // SAFETY: `geteuid` has no arguments, pointer requirements, or failure state.
    unsafe { libc::geteuid() }
}

#[cfg(test)]
mod tests {
    use super::unpack_full_release;

    fn release_archive(version: &str, extra: impl FnOnce(&mut tar::Builder<Vec<u8>>)) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        let mut add = |path: &str, contents: &[u8], mode: u32| {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(mode);
            header.set_cksum();
            builder.append_data(&mut header, path, contents).unwrap();
        };
        add("VERSION", format!("{version}\n").as_bytes(), 0o644);
        add("LICENSE", b"license", 0o644);
        for binary in ["bin/oore", "bin/oored", "bin/oore-web"] {
            add(binary, b"#!/bin/sh\n", 0o755);
        }
        add("web-dist/index.html", b"<!doctype html>", 0o644);
        extra(&mut builder);
        let tar = builder.into_inner().unwrap();
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut encoder, &tar).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn a_full_release_archive_unpacks_when_its_version_matches() {
        let stage = tempfile::tempdir().unwrap();
        unpack_full_release(&release_archive("1.2.3", |_| {})[..], stage.path(), "1.2.3").unwrap();
        assert!(stage.path().join("bin/oored").is_file());
    }

    #[test]
    fn a_full_release_archive_with_another_version_is_rejected() {
        let stage = tempfile::tempdir().unwrap();
        let error =
            unpack_full_release(&release_archive("1.2.2", |_| {})[..], stage.path(), "1.2.3")
                .unwrap_err();
        assert_eq!(
            error.to_string(),
            "release archive VERSION 1.2.2 does not match the expected release 1.2.3"
        );
    }

    #[test]
    fn a_full_release_archive_with_a_symlink_is_rejected() {
        let stage = tempfile::tempdir().unwrap();
        let archive = release_archive("1.2.3", |builder| {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_cksum();
            builder
                .append_link(&mut header, "web-dist/escape", "/etc")
                .unwrap();
        });
        let error = unpack_full_release(&archive[..], stage.path(), "1.2.3").unwrap_err();
        assert_eq!(
            error.to_string(),
            "unsupported release archive entry type: web-dist/escape"
        );
    }

    #[test]
    fn a_full_release_archive_with_an_unexpected_path_is_rejected() {
        let stage = tempfile::tempdir().unwrap();
        let archive = release_archive("1.2.3", |builder| {
            let mut header = tar::Header::new_gnu();
            header.set_size(1);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "bin/extra", &b"x"[..])
                .unwrap();
        });
        let error = unpack_full_release(&archive[..], stage.path(), "1.2.3").unwrap_err();
        assert_eq!(
            error.to_string(),
            "unexpected release archive path: bin/extra"
        );
    }
}
