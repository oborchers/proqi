//! Static verification of Debian metadata, members, permissions, and binary identity.

use std::{
    collections::BTreeMap,
    io::Cursor,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use tar::Archive;

use super::debian::{INSTALLED_PATHS, MAINTAINER};
use super::release_targets::DebianArtifact;

pub(super) fn package(
    root: &Path,
    package: &Path,
    archive_binary: &Path,
    version: &str,
    dependencies: &[String],
    metadata: DebianArtifact,
) -> Result<(), String> {
    verify_control(root, package, version, dependencies, metadata)?;
    verify_members(root, package)?;
    verify_no_hooks(root, package)?;
    let extracted = tempfile::Builder::new()
        .prefix("proqi-deb-verify-")
        .tempdir()
        .map_err(|error| format!("create Debian verification root: {error}"))?;
    run_extract(root, package, extracted.path())?;
    verify_permissions(extracted.path())?;
    let installed = extracted.path().join("usr/bin/proqi");
    if super::release::checksum(&installed)? != super::release::checksum(archive_binary)? {
        return Err("Debian executable differs from the verified archive binary".to_owned());
    }
    Ok(())
}

fn verify_control(
    root: &Path,
    package: &Path,
    version: &str,
    dependencies: &[String],
    metadata: DebianArtifact,
) -> Result<(), String> {
    let output = Command::new("dpkg-deb")
        .arg("--field")
        .arg(package)
        .args([
            "Package",
            "Version",
            "Architecture",
            "Depends",
            "Maintainer",
        ])
        .current_dir(root)
        .output()
        .map_err(|error| format!("read Debian control: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "dpkg-deb control inspection exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let fields = String::from_utf8(output.stdout)
        .map_err(|error| format!("Debian control output is not UTF-8: {error}"))?;
    validate_control(&fields, version, dependencies, metadata.architecture)
}

fn validate_control(
    fields: &str,
    version: &str,
    dependencies: &[String],
    architecture: &str,
) -> Result<(), String> {
    let expected = [
        "Package: proqi".to_owned(),
        format!("Maintainer: {MAINTAINER}"),
        format!("Version: {version}-1"),
        format!("Architecture: {architecture}"),
        format!("Depends: {}", dependencies.join(", ")),
    ];
    for field in expected {
        if !fields.lines().any(|line| line == field) {
            return Err(format!("Debian control omitted `{field}`: {fields}"));
        }
    }
    Ok(())
}

fn verify_members(root: &Path, package: &Path) -> Result<(), String> {
    let output = dpkg_output(root, package, ["--fsys-tarfile"])?;
    let mut archive = Archive::new(Cursor::new(output.stdout));
    let mut actual = BTreeMap::new();
    for entry in archive
        .entries()
        .map_err(|error| format!("read Debian data archive: {error}"))?
    {
        let entry = entry.map_err(|error| format!("read Debian data member: {error}"))?;
        if entry.header().entry_type().is_file() {
            let path = normalized_member(
                &entry
                    .path()
                    .map_err(|error| format!("read Debian data member path: {error}"))?,
            );
            let mode = entry
                .header()
                .mode()
                .map_err(|error| format!("read Debian data member mode: {error}"))?;
            actual.insert(path, mode);
        }
    }
    let expected = INSTALLED_PATHS
        .map(|(_, destination, mode)| (PathBuf::from(destination.trim_start_matches('/')), mode))
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    if actual != expected {
        return Err(format!(
            "Debian data members differ\nfound: {actual:#?}\nexpected: {expected:#?}"
        ));
    }
    Ok(())
}

fn normalized_member(path: &Path) -> PathBuf {
    path.strip_prefix(".")
        .map_or_else(|_| path.to_path_buf(), Path::to_path_buf)
}

fn verify_no_hooks(root: &Path, package: &Path) -> Result<(), String> {
    let output = dpkg_output(root, package, ["--ctrl-tarfile"])?;
    let mut archive = Archive::new(Cursor::new(output.stdout));
    let forbidden = ["preinst", "postinst", "prerm", "postrm", "triggers"];
    for entry in archive
        .entries()
        .map_err(|error| format!("read Debian control archive: {error}"))?
    {
        let entry = entry.map_err(|error| format!("read Debian control member: {error}"))?;
        let path = entry
            .path()
            .map_err(|error| format!("read Debian control path: {error}"))?;
        if forbidden.iter().any(|name| path.ends_with(name)) {
            return Err(format!("Debian package contains hook {}", path.display()));
        }
    }
    Ok(())
}

fn run_extract(root: &Path, package: &Path, output: &Path) -> Result<(), String> {
    let status = Command::new("dpkg-deb")
        .arg("--extract")
        .arg(package)
        .arg(output)
        .current_dir(root)
        .status()
        .map_err(|error| format!("start dpkg-deb extraction: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("dpkg-deb extraction exited with {status}"))
}

fn verify_permissions(root: &Path) -> Result<(), String> {
    for (_, destination, expected) in INSTALLED_PATHS {
        let path = root.join(destination.trim_start_matches('/'));
        let mode = path
            .metadata()
            .map_err(|error| format!("inspect installed {}: {error}", path.display()))?
            .permissions()
            .mode()
            & 0o777;
        if mode != expected {
            return Err(format!(
                "installed {} mode is {mode:04o}, expected {expected:04o}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn dpkg_output<I, S>(root: &Path, package: &Path, arguments: I) -> Result<Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new("dpkg-deb")
        .args(arguments)
        .arg(package)
        .current_dir(root)
        .output()
        .map_err(|error| format!("start dpkg-deb: {error}"))?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "dpkg-deb exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{MAINTAINER, validate_control};

    #[test]
    fn control_requires_the_exact_approved_maintainer() {
        let fields = "Package: proqi\nVersion: 0.1.2-1\nArchitecture: amd64\nDepends: libc6\n";
        let dependencies = ["libc6".to_owned()];
        let valid = format!("{fields}Maintainer: {MAINTAINER}\n");
        assert!(validate_control(&valid, "0.1.2", &dependencies, "amd64").is_ok());
        for maintainer in [
            "",
            "Maintainer: Example <person@example.test>\n",
            &format!("Maintainer: {MAINTAINER} unexpected\n"),
        ] {
            let invalid = format!("{fields}{maintainer}");
            assert!(validate_control(&invalid, "0.1.2", &dependencies, "amd64").is_err());
        }
    }
}
