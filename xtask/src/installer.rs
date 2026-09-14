//! Versioned release installer generation and static contract checks.

use std::{fs, os::unix::fs::PermissionsExt as _, path::Path};

pub(super) const INSTALLER_NAME: &str = "proqi-installer.sh";
pub(super) const INSTALLER_CHECKSUM: &str = "proqi-installer.sh.sha256";
pub(super) const INSTALLER_SBOM: &str = "proqi-installer.sh.spdx.json";

pub(super) fn package(root: &Path, output: &Path) -> Result<(), String> {
    let output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        root.join(output)
    };
    fs::create_dir_all(&output).map_err(|error| format!("create installer output: {error}"))?;
    let version = super::release::workspace_version(root)?;
    let script = render(&format!("v{version}"))?;
    let path = output.join(INSTALLER_NAME);
    fs::write(&path, script).map_err(|error| format!("write installer: {error}"))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("set installer permissions: {error}"))?;
    let digest = super::release::checksum(&path)?;
    fs::write(
        output.join(INSTALLER_CHECKSUM),
        format!("{digest}  {INSTALLER_NAME}\n"),
    )
    .map_err(|error| format!("write installer checksum: {error}"))?;
    validate(root, &path)
}

fn render(version: &str) -> Result<String, String> {
    validate_version(version)?;
    let template = include_str!("../../install/proqi-installer.sh");
    let cases = super::release_targets::installer_cases();
    Ok(template
        .replace("@@VERSION@@", version)
        .replace("@@GLIBC_FLOOR@@", super::release_targets::GLIBC_FLOOR)
        .replace("@@TARGET_CASES@@", &cases))
}

fn validate_version(version: &str) -> Result<(), String> {
    let parsed = version
        .strip_prefix('v')
        .and_then(|value| semver::Version::parse(value).ok());
    parsed
        .filter(|value| value.pre.is_empty() && value.build.is_empty())
        .filter(|value| version == format!("v{}.{}.{}", value.major, value.minor, value.patch))
        .map(|_| ())
        .ok_or_else(|| format!("installer version `{version}` is not a canonical stable tag"))
}

fn validate(root: &Path, path: &Path) -> Result<(), String> {
    let status = std::process::Command::new("sh")
        .args(["-n"])
        .arg(path)
        .current_dir(root)
        .status()
        .map_err(|error| format!("start shell syntax check: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "installer shell syntax check failed".to_owned())
}

#[cfg(test)]
#[path = "installer_tests.rs"]
mod tests;
