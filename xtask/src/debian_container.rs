//! Container installation contract for the generated Debian artifact.

use std::{path::Path, process::Command};

const IMAGES: [&str; 3] = ["ubuntu:22.04", "ubuntu:24.04", "debian:bookworm-slim"];

pub(super) fn verify(root: &Path, archive: &Path, package: &Path) -> Result<(), String> {
    require_file(archive, "Linux archive")?;
    require_file(package, "Debian package")?;
    let expected_binary = archive_binary_digest(root, archive)?;
    for image in IMAGES {
        verify_image(root, package, image, &expected_binary)?;
    }
    println!(
        "verified Debian installation contract in {}",
        IMAGES.join(", ")
    );
    Ok(())
}

fn require_file(path: &Path, label: &str) -> Result<(), String> {
    path.is_file()
        .then_some(())
        .ok_or_else(|| format!("{label} does not exist: {}", path.display()))
}

fn archive_binary_digest(root: &Path, archive: &Path) -> Result<String, String> {
    let temporary = tempfile::Builder::new()
        .prefix("proqi-deb-source-")
        .tempdir()
        .map_err(|error| format!("create Debian source root: {error}"))?;
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(archive)
        .arg("-C")
        .arg(temporary.path())
        .current_dir(root)
        .status()
        .map_err(|error| format!("extract Linux archive: {error}"))?;
    if !status.success() {
        return Err(format!("Linux archive extraction exited with {status}"));
    }
    super::release::checksum(&temporary.path().join(format!(
        "proqi-{}/proqi",
        super::release_targets::LINUX_X86_64
    )))
}

fn verify_image(root: &Path, package: &Path, image: &str, digest: &str) -> Result<(), String> {
    println!("+ Debian contract {image}");
    let package = package
        .canonicalize()
        .map_err(|error| format!("canonicalize Debian package: {error}"))?;
    let mount = format!("{}:/work/proqi_amd64.deb:ro", package.display());
    let script = container_script(digest);
    let status = Command::new("docker")
        .args(["run", "--rm", "--platform", "linux/amd64", "-v"])
        .arg(mount)
        .args([image, "sh", "-euxc", &script])
        .current_dir(root)
        .status()
        .map_err(|error| format!("start Debian test container {image}: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("Debian test container {image} exited with {status}"))
}

fn container_script(digest: &str) -> String {
    format!(
        r#"export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y /work/proqi_amd64.deb
test "$(proqi --version)" = "proqi {version}"
test "$(sha256sum /usr/bin/proqi | cut -d' ' -f1)" = "{digest}"
test -x /usr/bin/proqi
test -r /usr/share/bash-completion/completions/proqi
test -r /usr/share/zsh/vendor-completions/_proqi
test -r /usr/share/fish/vendor_completions.d/proqi.fish
test -r /usr/share/doc/proqi/copyright
test -r /usr/lib/proqi/proqi-installation.json
mkdir -p /tmp/proqi-state
proqi --state-dir /tmp/proqi-state --json capabilities > /tmp/capabilities.json
grep -q '"ok":true' /tmp/capabilities.json
proqi --state-dir /tmp/proqi-state --json > /tmp/session.json
grep -q '"session_id"' /tmp/session.json
test -f /tmp/proqi-state/data/proqi.sqlite3
apt-get remove -y proqi
test ! -e /usr/bin/proqi
test -f /tmp/proqi-state/data/proqi.sqlite3
apt-get install -y /work/proqi_amd64.deb
proqi --state-dir /tmp/proqi-state --json sessions > /tmp/sessions.json
grep -q '"session_id"' /tmp/sessions.json
dpkg-query -W -f='${{Status}}' proqi | grep -q 'install ok installed'
"#,
        version = env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::container_script;

    #[test]
    fn install_contract_preserves_state_across_remove_and_reinstall() {
        let script = container_script("abc123");
        assert!(script.contains("apt-get remove -y proqi"));
        assert!(script.contains("test -f /tmp/proqi-state/data/proqi.sqlite3"));
        assert!(script.contains("apt-get install -y /work/proqi_amd64.deb"));
        assert!(script.contains("abc123"));
    }
}
