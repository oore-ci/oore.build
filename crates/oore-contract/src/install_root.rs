//! Install root discovery shared by the `oore` CLI and the `oored` daemon.

use std::path::{Path, PathBuf};

/// Files that only an Oore install root contains. A `bin` directory without
/// one of them belongs to something else, such as `/usr/local` or `~/.cargo`.
const INSTALL_ROOT_MARKERS: [&str; 2] = ["VERSION", "install-manifest.json"];

/// Returns the install root that holds `executable` as `<root>/bin/<name>`.
///
/// The executable path is canonicalised first, so a symbolic link such as
/// `/usr/local/bin/oore -> ~/.oore/bin/oore` resolves to `~/.oore`. The
/// candidate counts only when it holds an install marker file.
#[must_use]
pub fn install_root_for_executable(executable: &Path) -> Option<PathBuf> {
    let executable = std::fs::canonicalize(executable).ok()?;
    let bin = executable.parent()?;
    if bin.file_name()? != "bin" {
        return None;
    }
    let root = bin.parent()?;
    INSTALL_ROOT_MARKERS
        .iter()
        .any(|marker| {
            std::fs::symlink_metadata(root.join(marker)).is_ok_and(|metadata| metadata.is_file())
        })
        .then(|| root.to_path_buf())
}

/// Returns the install root of the running executable, if it is installed.
#[must_use]
pub fn install_root_for_current_executable() -> Option<PathBuf> {
    install_root_for_executable(&std::env::current_exe().ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install(root: &Path, marker: Option<&str>) -> PathBuf {
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let executable = root.join("bin/oore");
        std::fs::write(&executable, b"").unwrap();
        if let Some(marker) = marker {
            std::fs::write(root.join(marker), b"0.1.0\n").unwrap();
        }
        executable
    }

    #[test]
    fn finds_a_root_with_a_marker() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap().join(".oore");
        for marker in INSTALL_ROOT_MARKERS {
            let _ = std::fs::remove_dir_all(&root);
            let executable = install(&root, Some(marker));
            assert_eq!(install_root_for_executable(&executable), Some(root.clone()));
        }
    }

    #[test]
    fn ignores_a_bin_directory_without_a_marker() {
        let temp = tempfile::tempdir().unwrap();
        let executable = install(&temp.path().join(".cargo"), None);
        assert_eq!(install_root_for_executable(&executable), None);
    }

    #[test]
    fn ignores_a_marker_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("usr");
        let executable = install(&root, None);
        std::fs::create_dir(root.join("VERSION")).unwrap();
        assert_eq!(install_root_for_executable(&executable), None);
    }

    #[cfg(unix)]
    #[test]
    fn follows_a_symlinked_executable_to_its_install_root() {
        let temp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(temp.path()).unwrap();
        let root = base.join(".oore");
        let executable = install(&root, Some("VERSION"));
        let usr_local_bin = base.join("usr/local/bin");
        std::fs::create_dir_all(&usr_local_bin).unwrap();
        let link = usr_local_bin.join("oore");
        std::os::unix::fs::symlink(&executable, &link).unwrap();
        assert_eq!(install_root_for_executable(&link), Some(root));
    }
}
