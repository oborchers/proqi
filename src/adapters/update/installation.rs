//! Verified installation-context detection and stable local identity.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use crate::{
    domain::{Installation, InstallationIdentity, InstallationKind},
    ports::update::{InstallDetector, UpdateError},
};

const MAX_MARKER_BYTES: u64 = 4 * 1024;

/// Detects the running executable's installation mechanism.
#[derive(Clone, Debug)]
pub struct SystemInstallDetector {
    executable: Option<PathBuf>,
}

impl SystemInstallDetector {
    /// Detect the current process executable.
    #[must_use]
    pub const fn current() -> Self {
        Self { executable: None }
    }

    /// Use an explicit executable path for deterministic tests and package smoke checks.
    #[must_use]
    pub fn for_executable(executable: PathBuf) -> Self {
        Self {
            executable: Some(executable),
        }
    }
}

impl Default for SystemInstallDetector {
    fn default() -> Self {
        Self::current()
    }
}

impl InstallDetector for SystemInstallDetector {
    fn detect(&self) -> Result<Installation, UpdateError> {
        let executable = self
            .executable
            .clone()
            .map_or_else(std::env::current_exe, Ok)
            .map_err(|error| UpdateError::Installation(error.to_string()))?;
        let executable = fs::canonicalize(&executable)
            .map_err(|error| UpdateError::Installation(error.to_string()))?;
        let (kind, identity_path, restart_executable) = homebrew_context(&executable)?
            .map(|(root, active)| (InstallationKind::HomebrewFormula, root, Some(active)))
            .or_else(|| {
                standalone_root(&executable).map(|root| {
                    (
                        InstallationKind::StandaloneArchive,
                        root,
                        Some(executable.clone()),
                    )
                })
            })
            .unwrap_or_else(|| (InstallationKind::SourceOrUnknown, executable.clone(), None));
        let identity = identity(kind, &identity_path);
        Ok(Installation {
            identity,
            kind,
            executable,
            restart_executable,
        })
    }
}

fn homebrew_context(executable: &Path) -> Result<Option<(PathBuf, PathBuf)>, UpdateError> {
    let Some(bin) = executable.parent() else {
        return Ok(None);
    };
    let Some(keg) = bin.parent() else {
        return Ok(None);
    };
    let Some(formula) = keg.parent() else {
        return Ok(None);
    };
    let Some(cellar) = formula.parent() else {
        return Ok(None);
    };
    let valid_shape = executable.file_name().is_some_and(|name| name == "proqi")
        && bin.file_name().is_some_and(|name| name == "bin")
        && formula.file_name().is_some_and(|name| name == "proqi")
        && cellar.file_name().is_some_and(|name| name == "Cellar");
    if !valid_shape || !regular_bounded_file(&keg.join("INSTALL_RECEIPT.json")) {
        return Ok(None);
    }
    let Some(prefix) = cellar.parent() else {
        return Ok(None);
    };
    let active = prefix.join("opt/proqi/bin/proqi");
    let active_executable = fs::canonicalize(&active).map_err(|error| {
        UpdateError::Installation(format!(
            "active Homebrew executable is unavailable: {error}"
        ))
    })?;
    if active_executable != executable {
        return Err(UpdateError::Installation(
            "running executable does not match the active Homebrew installation".to_owned(),
        ));
    }
    Ok(Some((formula.to_path_buf(), active)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StandaloneMarker {
    schema_version: u32,
    product: String,
    kind: String,
}

fn standalone_root(executable: &Path) -> Option<PathBuf> {
    let root = executable.parent()?;
    let marker = root.join("proqi-installation.json");
    if !regular_bounded_file(&marker) {
        return None;
    }
    let bytes = fs::read(marker).ok()?;
    let parsed: StandaloneMarker = serde_json::from_slice(&bytes).ok()?;
    (parsed.schema_version == 1 && parsed.product == "proqi" && parsed.kind == "standalone_archive")
        .then(|| root.to_path_buf())
}

fn regular_bounded_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= MAX_MARKER_BYTES
    })
}

fn identity(kind: InstallationKind, path: &Path) -> InstallationIdentity {
    let mut hash = Sha256::new();
    hash.update(match kind {
        InstallationKind::HomebrewFormula => b"homebrew-formula\0".as_slice(),
        InstallationKind::StandaloneArchive => b"standalone-archive\0".as_slice(),
        InstallationKind::SourceOrUnknown => b"source-or-unknown\0".as_slice(),
    });
    hash.update(identity_path_bytes(path));
    InstallationIdentity::from_digest(hash.finalize().into())
}

fn identity_path_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt as _;

    path.as_os_str().as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink};

    use crate::{domain::InstallationKind, ports::update::InstallDetector as _};

    use super::SystemInstallDetector;

    #[test]
    fn homebrew_versions_share_identity_only_when_each_is_active() {
        let temporary = tempfile::tempdir().expect("installation root");
        let formula = temporary.path().join("Cellar/proqi");
        let first = formula.join("0.1.0/bin/proqi");
        let second = formula.join("0.2.0/bin/proqi");
        for binary in [&first, &second] {
            fs::create_dir_all(binary.parent().expect("bin parent")).expect("create keg");
            fs::write(binary, b"binary").expect("write binary");
            fs::write(
                binary
                    .parent()
                    .expect("bin")
                    .parent()
                    .expect("keg")
                    .join("INSTALL_RECEIPT.json"),
                b"{}",
            )
            .expect("write receipt");
        }
        let active = fs::canonicalize(temporary.path())
            .expect("canonical root")
            .join("opt/proqi/bin/proqi");
        fs::create_dir_all(active.parent().expect("active parent")).expect("create active root");
        symlink(&first, &active).expect("activate first keg");
        let first_installation = SystemInstallDetector::for_executable(first.clone())
            .detect()
            .expect("first install");
        fs::remove_file(&active).expect("remove first link");
        symlink(&second, &active).expect("activate second keg");
        let second_installation = SystemInstallDetector::for_executable(second.clone())
            .detect()
            .expect("second install");
        assert_eq!(first_installation.kind, InstallationKind::HomebrewFormula);
        assert_eq!(first_installation.identity, second_installation.identity);
        assert_eq!(first_installation.restart_executable, Some(active.clone()));
        assert!(
            SystemInstallDetector::for_executable(first)
                .detect()
                .is_err(),
            "an inactive keg cannot claim authority for the active installation"
        );
        assert!(
            SystemInstallDetector::for_executable(second)
                .detect()
                .is_ok()
        );
    }

    #[test]
    fn standalone_marker_is_strict_and_unknown_is_non_destructive() {
        let temporary = tempfile::tempdir().expect("installation root");
        let binary = temporary.path().join("proqi");
        fs::write(&binary, b"binary").expect("write binary");
        let unknown = SystemInstallDetector::for_executable(binary.clone())
            .detect()
            .expect("unknown install");
        assert_eq!(unknown.kind, InstallationKind::SourceOrUnknown);

        fs::write(
            temporary.path().join("proqi-installation.json"),
            br#"{"schema_version":1,"product":"proqi","kind":"standalone_archive"}"#,
        )
        .expect("write marker");
        let archive = SystemInstallDetector::for_executable(binary)
            .detect()
            .expect("archive install");
        assert_eq!(archive.kind, InstallationKind::StandaloneArchive);
        assert_eq!(archive.restart_executable, Some(archive.executable.clone()));
    }
}
