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
        let (kind, identity_path, restart_executable) = homebrew_context(&executable)
            .map(|(root, active)| (InstallationKind::HomebrewFormula, root, Some(active)))
            .or_else(|| {
                standalone_root(&executable)
                    .map(|root| (InstallationKind::StandaloneArchive, root, None))
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

fn homebrew_context(executable: &Path) -> Option<(PathBuf, PathBuf)> {
    let bin = executable.parent()?;
    let keg = bin.parent()?;
    let formula = keg.parent()?;
    let cellar = formula.parent()?;
    let valid_shape = executable.file_name()? == "proqi"
        && bin.file_name()? == "bin"
        && formula.file_name()? == "proqi"
        && cellar.file_name()? == "Cellar";
    if !valid_shape || !regular_bounded_file(&keg.join("INSTALL_RECEIPT.json")) {
        return None;
    }
    let prefix = cellar.parent()?;
    let active = prefix.join("opt/proqi/bin/proqi");
    Some((formula.to_path_buf(), active))
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
    use std::fs;

    use crate::{domain::InstallationKind, ports::update::InstallDetector as _};

    use super::SystemInstallDetector;

    #[test]
    fn homebrew_versions_share_identity_only_with_a_receipt() {
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
        let first = SystemInstallDetector::for_executable(first)
            .detect()
            .expect("first install");
        let second = SystemInstallDetector::for_executable(second)
            .detect()
            .expect("second install");
        assert_eq!(first.kind, InstallationKind::HomebrewFormula);
        assert_eq!(first.identity, second.identity);
        assert_eq!(
            first.restart_executable,
            Some(
                fs::canonicalize(temporary.path())
                    .expect("canonical root")
                    .join("opt/proqi/bin/proqi")
            )
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
    }
}
