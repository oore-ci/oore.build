use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

pub(crate) const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum InstallProfile {
    Complete,
    ControlPlane,
    Runner,
    WebNode,
    CliOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum InstallComponent {
    Cli,
    ControlPlane,
    Runner,
    Web,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum InstallState {
    #[default]
    ComponentsReady,
    Configuring,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum InstallService {
    Daemon,
    Updater,
    Runner,
    Web,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManagedService {
    pub(crate) service: InstallService,
    pub(crate) label: String,
    pub(crate) definition: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstallLifecycle {
    pub(crate) state: InstallState,
    pub(crate) services: Vec<ManagedService>,
    pub(crate) daemon_listen: Option<String>,
    #[serde(default)]
    pub(crate) state_file: Option<String>,
    pub(crate) web_listen: Option<String>,
    pub(crate) backend_url: Option<String>,
    #[serde(default)]
    pub(crate) browser_transport_protected: bool,
    #[serde(default)]
    pub(crate) backend_transport_protected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstallRelease {
    pub(crate) version: String,
    pub(crate) channel: String,
    pub(crate) repository: String,
    pub(crate) archive: String,
    pub(crate) sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstallManifest {
    pub(crate) schema_version: u32,
    pub(crate) profile: InstallProfile,
    pub(crate) components: Vec<InstallComponent>,
    pub(crate) release: InstallRelease,
    pub(crate) shell_path_files: Vec<String>,
    #[serde(default)]
    pub(crate) lifecycle: InstallLifecycle,
}

impl InstallProfile {
    pub(crate) const fn components(self) -> &'static [InstallComponent] {
        match self {
            Self::Complete => &[
                InstallComponent::Cli,
                InstallComponent::ControlPlane,
                InstallComponent::Runner,
                InstallComponent::Web,
            ],
            Self::ControlPlane => &[InstallComponent::Cli, InstallComponent::ControlPlane],
            Self::Runner => &[InstallComponent::Cli, InstallComponent::Runner],
            Self::WebNode => &[InstallComponent::Cli, InstallComponent::Web],
            Self::CliOnly => &[InstallComponent::Cli],
        }
    }

    pub(crate) const fn services(self) -> &'static [InstallService] {
        match self {
            Self::Complete => &[
                InstallService::Daemon,
                InstallService::Runner,
                InstallService::Web,
            ],
            Self::ControlPlane => &[InstallService::Daemon],
            Self::Runner => &[InstallService::Runner],
            Self::WebNode => &[InstallService::Web],
            Self::CliOnly => &[],
        }
    }
}

impl InstallService {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Daemon => "build.oore.oored",
            Self::Updater => "build.oore.oore-updater",
            Self::Runner => "build.oore.oore-runner",
            Self::Web => "build.oore.oore-web",
        }
    }

    pub(crate) fn definition(self) -> PathBuf {
        Path::new("/Library/LaunchDaemons").join(format!("{}.plist", self.label()))
    }
}

impl ManagedService {
    pub(crate) fn new(service: InstallService) -> Self {
        Self {
            service,
            label: service.label().to_string(),
            definition: service.definition().display().to_string(),
        }
    }

    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.label == self.service.label(),
            "managed service label does not match its service"
        );
        anyhow::ensure!(
            Path::new(&self.definition) == self.service.definition(),
            "managed service definition does not match its service"
        );
        Ok(())
    }
}

impl InstallRelease {
    pub(crate) fn new(
        version: String,
        channel: String,
        repository: String,
        archive: String,
        sha256: String,
    ) -> anyhow::Result<Self> {
        let release = Self {
            version,
            channel,
            repository,
            archive,
            sha256,
        };
        release.validate()?;
        Ok(release)
    }

    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.version == self.version.trim(),
            "release version contains surrounding whitespace"
        );
        semver::Version::parse(&self.version)
            .with_context(|| format!("unsupported release version {}", self.version))?;
        anyhow::ensure!(
            matches!(self.channel.as_str(), "stable" | "beta" | "alpha"),
            "unsupported release channel {}",
            self.channel
        );
        validate_repository(&self.repository)?;
        validate_archive_name(&self.archive)?;
        anyhow::ensure!(
            self.sha256.len() == 64
                && self
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "release SHA-256 must contain 64 lowercase hexadecimal characters"
        );
        Ok(())
    }
}

impl InstallManifest {
    pub(crate) fn new(
        profile: InstallProfile,
        release: InstallRelease,
        shell_path_files: Vec<String>,
    ) -> anyhow::Result<Self> {
        let manifest = Self {
            schema_version: SCHEMA_VERSION,
            profile,
            components: profile.components().to_vec(),
            release,
            shell_path_files,
            lifecycle: InstallLifecycle::default(),
        };
        manifest.validate()?;
        Ok(manifest)
    }

    pub(crate) fn load(path: &Path) -> anyhow::Result<Self> {
        Self::load_for_uid(path, current_effective_uid())
    }

    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema_version == SCHEMA_VERSION,
            "unsupported installation manifest schema version {}",
            self.schema_version
        );
        anyhow::ensure!(
            self.components == self.profile.components(),
            "installation manifest components do not match the selected profile"
        );
        self.release.validate()?;
        for path_file in &self.shell_path_files {
            anyhow::ensure!(
                matches!(path_file.as_str(), ".zshrc" | ".bashrc" | ".bash_profile"),
                "unsupported shell PATH file {path_file}"
            );
        }
        anyhow::ensure!(
            all_unique(&self.shell_path_files),
            "installation manifest contains duplicate shell PATH files"
        );
        self.lifecycle.validate(self.profile)?;
        Ok(())
    }

    pub(crate) fn record_service(&mut self, service: InstallService) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.profile.services().contains(&service),
            "the selected profile does not include {}",
            service.label()
        );
        if !self
            .lifecycle
            .services
            .iter()
            .any(|managed| managed.service == service)
        {
            self.lifecycle.services.push(ManagedService::new(service));
        }
        self.lifecycle.state = InstallState::Configuring;
        self.validate()
    }

    pub(crate) fn mark_ready(&mut self) -> anyhow::Result<()> {
        self.lifecycle.state = InstallState::Ready;
        self.validate()
    }

    pub(crate) fn write_atomic(&self, path: &Path) -> anyhow::Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .context("installation manifest path has no parent directory")?;
        validate_parent_directory(parent)?;

        let mut contents = serde_json::to_vec_pretty(self)
            .context("failed to serialize the installation manifest")?;
        contents.push(b'\n');

        let mut staged = tempfile::NamedTempFile::new_in(parent).with_context(|| {
            format!(
                "failed to stage the installation manifest in {}",
                parent.display()
            )
        })?;
        staged
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .context("failed to set installation manifest permissions")?;
        staged
            .write_all(&contents)
            .context("failed to write the installation manifest")?;
        staged
            .as_file_mut()
            .sync_all()
            .context("failed to sync the installation manifest")?;

        let persisted = staged
            .persist(path)
            .map_err(|error| error.error)
            .with_context(|| {
                format!("failed to publish installation manifest {}", path.display())
            })?;
        persisted
            .sync_all()
            .context("failed to sync the published installation manifest")?;
        validate_manifest_metadata(&persisted.metadata()?, current_effective_uid())?;
        File::open(parent)
            .with_context(|| format!("failed to open manifest directory {}", parent.display()))?
            .sync_all()
            .with_context(|| format!("failed to sync manifest directory {}", parent.display()))?;
        Ok(())
    }

    fn load_for_uid(path: &Path, expected_uid: u32) -> anyhow::Result<Self> {
        let path_metadata = fs::symlink_metadata(path).with_context(|| {
            format!("failed to inspect installation manifest {}", path.display())
        })?;
        validate_manifest_metadata(&path_metadata, expected_uid)?;

        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .with_context(|| format!("failed to open installation manifest {}", path.display()))?;
        let opened_metadata = file
            .metadata()
            .with_context(|| format!("failed to inspect open manifest {}", path.display()))?;
        validate_manifest_metadata(&opened_metadata, expected_uid)?;
        anyhow::ensure!(
            path_metadata.dev() == opened_metadata.dev()
                && path_metadata.ino() == opened_metadata.ino(),
            "installation manifest changed while it was opened"
        );

        serde_json::from_reader(BufReader::new(file))
            .with_context(|| format!("failed to parse installation manifest {}", path.display()))
            .and_then(|manifest: Self| manifest.validate().map(|()| manifest))
            .with_context(|| {
                format!(
                    "installation manifest {} is invalid. To recover, run `oore uninstall --ignore-invalid-manifest`, which keeps Oore data, then rerun the CLI bootstrap and `oore install` with the same role",
                    path.display()
                )
            })
    }
}

impl InstallLifecycle {
    fn validate(&self, profile: InstallProfile) -> anyhow::Result<()> {
        for service in &self.services {
            service.validate()?;
            anyhow::ensure!(
                profile.services().contains(&service.service),
                "installation manifest records a service outside its profile"
            );
        }
        anyhow::ensure!(
            self.services.iter().enumerate().all(|(index, service)| {
                !self.services[..index]
                    .iter()
                    .any(|candidate| candidate.service == service.service)
            }),
            "installation manifest contains duplicate managed services"
        );
        if self.state == InstallState::Ready {
            anyhow::ensure!(
                profile.services().iter().all(|expected| {
                    self.services
                        .iter()
                        .any(|managed| managed.service == *expected)
                }),
                "ready installation manifest is missing a required service"
            );
        }
        for value in [
            self.daemon_listen.as_deref(),
            self.state_file.as_deref(),
            self.web_listen.as_deref(),
            self.backend_url.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            anyhow::ensure!(
                !value.trim().is_empty() && value == value.trim(),
                "installation lifecycle value is empty or contains surrounding whitespace"
            );
        }
        if let Some(state_file) = self.state_file.as_deref() {
            anyhow::ensure!(
                Path::new(state_file).is_absolute(),
                "installation lifecycle state file must be absolute"
            );
        }
        Ok(())
    }
}

pub(crate) fn validate_repository(repository: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        repository == repository.trim(),
        "release repository contains surrounding whitespace"
    );
    let mut segments = repository.split('/');
    let owner = segments.next().unwrap_or_default();
    let name = segments.next().unwrap_or_default();
    anyhow::ensure!(
        !owner.is_empty() && !name.is_empty() && segments.next().is_none(),
        "release repository must use owner/name format"
    );
    anyhow::ensure!(
        [owner, name].into_iter().all(|segment| {
            segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        }),
        "release repository contains unsupported characters"
    );
    Ok(())
}

fn validate_archive_name(archive: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !archive.is_empty() && archive == archive.trim(),
        "release archive name is empty or contains surrounding whitespace"
    );
    anyhow::ensure!(
        Path::new(archive).file_name() == Some(OsStr::new(archive))
            && !matches!(archive, "." | ".."),
        "release archive must be a file name"
    );
    Ok(())
}

fn validate_parent_directory(parent: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(parent)
        .with_context(|| format!("failed to inspect manifest directory {}", parent.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_dir(),
        "manifest parent is not a regular directory: {}",
        parent.display()
    );
    anyhow::ensure!(
        metadata.uid() == current_effective_uid(),
        "manifest parent has an unexpected owner: {}",
        parent.display()
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o022 == 0,
        "manifest parent is writable by another user: {}",
        parent.display()
    );
    Ok(())
}

/// Checks that `path` is a private regular file owned by the current user,
/// without reading or validating its contents.
pub(crate) fn ensure_private_manifest_file(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect installation manifest {}", path.display()))?;
    validate_manifest_metadata(&metadata, current_effective_uid())
}

fn validate_manifest_metadata(metadata: &fs::Metadata, expected_uid: u32) -> anyhow::Result<()> {
    anyhow::ensure!(
        metadata.file_type().is_file(),
        "installation manifest is not a regular file"
    );
    anyhow::ensure!(
        metadata.uid() == expected_uid,
        "installation manifest has an unexpected owner"
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o077 == 0,
        "installation manifest permissions must not grant group or other access"
    );
    Ok(())
}

fn all_unique(values: &[String]) -> bool {
    values
        .iter()
        .enumerate()
        .all(|(index, value)| !values[..index].contains(value))
}

fn current_effective_uid() -> u32 {
    // SAFETY: `geteuid` has no arguments, pointer requirements, or failure state.
    unsafe { libc::geteuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_manifest_names_the_recovery_command() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("install-manifest.json");
        fs::write(&path, b"{ not json").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let error = InstallManifest::load(&path).unwrap_err();
        assert!(
            format!("{error:#}").contains("oore uninstall --ignore-invalid-manifest"),
            "{error:#}"
        );
        ensure_private_manifest_file(&path).unwrap();
    }

    #[test]
    fn a_shared_manifest_cannot_be_set_aside() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("install-manifest.json");
        fs::write(&path, b"{}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        assert!(ensure_private_manifest_file(&path).is_err());
    }

    const SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn release() -> InstallRelease {
        InstallRelease::new(
            "0.1.48".to_string(),
            "stable".to_string(),
            "oore-ci/oore.build".to_string(),
            "oore_0.1.48_darwin_arm64.tar.gz".to_string(),
            SHA256.to_string(),
        )
        .unwrap()
    }

    fn manifest(profile: InstallProfile) -> InstallManifest {
        InstallManifest::new(profile, release(), vec![".zshrc".to_string()]).unwrap()
    }

    fn private_root() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        root
    }

    fn write_private(path: &Path, contents: &[u8]) {
        fs::write(path, contents).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn load_error(contents: &[u8]) -> String {
        let root = private_root();
        let path = root.path().join("install-manifest.json");
        write_private(&path, contents);
        format!("{:#}", InstallManifest::load(&path).unwrap_err())
    }

    fn manifest_json(edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let mut value = serde_json::to_value(manifest(InstallProfile::Complete)).unwrap();
        edit(&mut value);
        serde_json::to_vec(&value).unwrap()
    }

    #[test]
    fn written_manifests_load_back_unchanged() {
        let root = private_root();
        let path = root.path().join("install-manifest.json");
        let mut written = manifest(InstallProfile::Complete);
        written.record_service(InstallService::Daemon).unwrap();
        written.write_atomic(&path).unwrap();

        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(InstallManifest::load(&path).unwrap(), written);
    }

    #[test]
    fn corrupt_manifests_fail_to_load() {
        assert!(load_error(b"{\"schema_version\": 1, \"profile\":").contains("failed to parse"));
        assert!(load_error(b"").contains("failed to parse"));
        assert!(
            load_error(&manifest_json(|value| value["unexpected"] = true.into()))
                .contains("unknown field")
        );
    }

    #[test]
    fn manifests_with_invalid_contents_fail_to_load() {
        assert!(
            load_error(&manifest_json(|value| value["schema_version"] = 2.into()))
                .contains("unsupported installation manifest schema version 2")
        );
        assert!(
            load_error(&manifest_json(|value| {
                value["components"] = serde_json::json!(["cli"]);
            }))
            .contains("components do not match")
        );
        assert!(
            load_error(&manifest_json(|value| {
                value["release"]["sha256"] = SHA256.to_uppercase().into();
            }))
            .contains("64 lowercase hexadecimal characters")
        );
        assert!(
            load_error(&manifest_json(|value| {
                value["release"]["channel"] = "nightly".into();
            }))
            .contains("unsupported release channel nightly")
        );
        assert!(
            load_error(&manifest_json(|value| {
                value["release"]["archive"] = "../oore.tar.gz".into();
            }))
            .contains("must be a file name")
        );
        assert!(
            load_error(&manifest_json(|value| {
                value["shell_path_files"] = serde_json::json!([".zshrc", ".zshrc"]);
            }))
            .contains("duplicate shell PATH files")
        );
        assert!(
            load_error(&manifest_json(|value| {
                value["shell_path_files"] = serde_json::json!([".profile"]);
            }))
            .contains("unsupported shell PATH file .profile")
        );
    }

    #[test]
    fn manifests_older_than_the_lifecycle_section_load_with_defaults() {
        let root = private_root();
        let path = root.path().join("install-manifest.json");
        write_private(
            &path,
            &manifest_json(|value| {
                value.as_object_mut().unwrap().remove("lifecycle");
            }),
        );
        let loaded = InstallManifest::load(&path).unwrap();
        assert_eq!(loaded.lifecycle, InstallLifecycle::default());
    }

    #[test]
    fn manifests_readable_by_others_or_behind_links_are_refused() {
        let root = private_root();
        let path = root.path().join("install-manifest.json");
        manifest(InstallProfile::Runner)
            .write_atomic(&path)
            .unwrap();

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            format!("{:#}", InstallManifest::load(&path).unwrap_err())
                .contains("must not grant group or other access")
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let link = root.path().join("linked-manifest.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(
            format!("{:#}", InstallManifest::load(&link).unwrap_err())
                .contains("not a regular file")
        );

        assert!(
            format!(
                "{:#}",
                InstallManifest::load_for_uid(&path, current_effective_uid() + 1).unwrap_err()
            )
            .contains("unexpected owner")
        );
        assert!(InstallManifest::load(&root.path().join("missing.json")).is_err());
    }

    #[test]
    fn manifests_are_not_written_into_shared_directories() {
        let root = private_root();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o777)).unwrap();
        let error = manifest(InstallProfile::CliOnly)
            .write_atomic(&root.path().join("install-manifest.json"))
            .unwrap_err();
        assert!(error.to_string().contains("writable by another user"));
        assert!(!root.path().join("install-manifest.json").exists());
    }

    #[test]
    fn lifecycle_changes_stay_within_the_profile() {
        let mut runner = manifest(InstallProfile::Runner);
        assert!(runner.record_service(InstallService::Daemon).is_err());
        assert!(runner.mark_ready().is_err());

        runner.record_service(InstallService::Runner).unwrap();
        runner.record_service(InstallService::Runner).unwrap();
        assert_eq!(runner.lifecycle.services.len(), 1);
        assert_eq!(runner.lifecycle.state, InstallState::Configuring);
        runner.mark_ready().unwrap();
        assert_eq!(runner.lifecycle.state, InstallState::Ready);

        let mut tampered = runner.clone();
        tampered.lifecycle.services[0].definition = "/tmp/build.oore.oore-runner.plist".into();
        assert!(tampered.validate().is_err());

        let mut relative_state = runner;
        relative_state.lifecycle.state_file = Some("oore.db".to_string());
        assert!(relative_state.validate().is_err());
    }

    #[test]
    fn release_details_are_validated() {
        let new = |version: &str, repository: &str| {
            InstallRelease::new(
                version.to_string(),
                "stable".to_string(),
                repository.to_string(),
                "oore.tar.gz".to_string(),
                SHA256.to_string(),
            )
        };
        assert!(new("0.1.48", "oore-ci/oore.build").is_ok());
        assert!(new(" 0.1.48", "oore-ci/oore.build").is_err());
        assert!(new("latest", "oore-ci/oore.build").is_err());
        assert!(new("0.1.48", "oore-ci").is_err());
        assert!(new("0.1.48", "oore-ci/oore.build/extra").is_err());
        assert!(new("0.1.48", "oore ci/oore.build").is_err());
    }
}
