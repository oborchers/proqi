//! Container installation contract for the generated Debian artifact.

use std::{path::Path, process::Command};

const IMAGES: [&str; 3] = [
    "ubuntu:22.04@sha256:2edbbc5dc405e9612ba3584ce95480277e3eb374407b5505fe26f17df77c7dbc",
    "ubuntu:24.04@sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517",
    "debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171",
];

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
test "$(dpkg-deb --field /work/proqi_amd64.deb Package)" = "proqi"
test "$(dpkg-deb --field /work/proqi_amd64.deb Version)" = "{version}-1"
test "$(dpkg-deb --field /work/proqi_amd64.deb Architecture)" = "amd64"
dpkg-deb --field /work/proqi_amd64.deb Depends | grep -q 'libc6 (>= 2.35)'
dpkg-deb --field /work/proqi_amd64.deb Depends | grep -q 'libgcc-s1'
dpkg-deb --contents /work/proqi_amd64.deb | grep -q './usr/bin/proqi'
mkdir /tmp/wrong-architecture
dpkg-deb --raw-extract /work/proqi_amd64.deb /tmp/wrong-architecture
sed -i 's/^Architecture: amd64$/Architecture: arm64/' /tmp/wrong-architecture/DEBIAN/control
dpkg-deb --build /tmp/wrong-architecture /tmp/proqi-wrong-architecture.deb
if apt-get install -y /tmp/proqi-wrong-architecture.deb; then exit 1; fi
mkdir /tmp/missing-dependency
dpkg-deb --raw-extract /work/proqi_amd64.deb /tmp/missing-dependency
sed -i 's/^Depends:.*$/Depends: proqi-deliberately-missing-dependency/' /tmp/missing-dependency/DEBIAN/control
dpkg-deb --build /tmp/missing-dependency /tmp/proqi-missing-dependency.deb
if apt-get install -y /tmp/proqi-missing-dependency.deb; then exit 1; fi
if dpkg-query -W proqi >/dev/null 2>&1; then exit 1; fi
apt-get install -y /work/proqi_amd64.deb
test "$(proqi --version)" = "proqi {version}"
test "$(sha256sum /usr/bin/proqi | cut -d' ' -f1)" = "{digest}"
test -x /usr/bin/proqi
test -r /usr/share/bash-completion/completions/proqi
test -r /usr/share/zsh/vendor-completions/_proqi
test -r /usr/share/fish/vendor_completions.d/proqi.fish
test -r /usr/share/doc/proqi/copyright
install -d -m 700 /tmp/proqi-state
proqi --state-dir /tmp/proqi-state --json capabilities > /tmp/capabilities.json
grep -q '"ok":true' /tmp/capabilities.json
proqi --state-dir /tmp/proqi-state --json doctor > /tmp/doctor.json
grep -q '"ok":true' /tmp/doctor.json
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
        assert!(script.contains("proqi-wrong-architecture.deb"));
        assert!(script.contains("proqi-missing-dependency.deb"));
        assert!(script.contains("install -d -m 700 /tmp/proqi-state"));
        assert!(script.contains("--json doctor"));
        assert!(script.contains("abc123"));
    }
}
