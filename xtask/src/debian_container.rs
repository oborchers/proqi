//! Container installation contract for the generated Debian artifact.

use std::{path::Path, process::Command};

const IMAGES: [ImageProfile; 3] = [
    ImageProfile {
        name: "ubuntu-22.04",
        image: "ubuntu:22.04@sha256:829f6df217bcbae2b371026e81711d1a787c61b2967ad09d015063663ebafbf7",
    },
    ImageProfile {
        name: "ubuntu-24.04",
        image: "ubuntu:24.04@sha256:224a1869083a311ef3f13648a154ba79832fbef6364d31493642ca03082da254",
    },
    ImageProfile {
        name: "debian-bookworm",
        image: "debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171",
    },
];

#[derive(Clone, Copy)]
struct ImageProfile {
    name: &'static str,
    image: &'static str,
}

pub(super) fn verify_target(
    root: &Path,
    archive: &Path,
    package: &Path,
    triple: &str,
) -> Result<(), String> {
    require_file(archive, "Linux archive")?;
    require_file(package, "Debian package")?;
    let target = super::release_targets::find(triple)?;
    let expected_binary = archive_binary_digest(root, archive, target)?;
    for profile in IMAGES {
        verify_profile(root, package, profile, &expected_binary, target)?;
    }
    println!(
        "verified Debian installation contract in {}",
        IMAGES.map(|profile| profile.name).join(", ")
    );
    Ok(())
}

pub(super) fn verify_one(
    root: &Path,
    profile: &str,
    archive: &Path,
    package: &Path,
    evidence_directory: &Path,
    triple: &str,
) -> Result<(), String> {
    require_file(archive, "Linux archive")?;
    require_file(package, "Debian package")?;
    let image = image_profile(profile)?;
    let target = super::release_targets::find(triple)?;
    let expected_binary = super::timing::phase("debian.evidence", || {
        super::debian::verify_evidence(root, archive, package, evidence_directory, triple)
    })?;
    verify_profile(root, package, image, &expected_binary, target)
}

fn image_profile(name: &str) -> Result<ImageProfile, String> {
    IMAGES
        .into_iter()
        .find(|profile| profile.name == name)
        .ok_or_else(|| {
            format!(
                "unknown Debian image profile `{name}`; expected {}",
                IMAGES.map(|profile| profile.name).join(", ")
            )
        })
}

fn require_file(path: &Path, label: &str) -> Result<(), String> {
    path.is_file()
        .then_some(())
        .ok_or_else(|| format!("{label} does not exist: {}", path.display()))
}

fn archive_binary_digest(
    root: &Path,
    archive: &Path,
    target: super::release_targets::ReleaseTarget,
) -> Result<String, String> {
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
    super::release::checksum(
        &temporary
            .path()
            .join(format!("proqi-{}/proqi", target.triple)),
    )
}

fn verify_profile(
    root: &Path,
    package: &Path,
    profile: ImageProfile,
    digest: &str,
    target: super::release_targets::ReleaseTarget,
) -> Result<(), String> {
    super::timing::phase(&format!("debian.verify.{}", profile.name), || {
        verify_image(root, package, profile.image, digest, target)
    })
}

fn verify_image(
    root: &Path,
    package: &Path,
    image: &str,
    digest: &str,
    target: super::release_targets::ReleaseTarget,
) -> Result<(), String> {
    println!("+ Debian contract {image}");
    let package = package
        .canonicalize()
        .map_err(|error| format!("canonicalize Debian package: {error}"))?;
    let metadata = target
        .debian
        .ok_or_else(|| "Debian target metadata is missing".to_owned())?;
    let mounted = format!("/work/{}", metadata.filename);
    let mount = format!("{}:{mounted}:ro", package.display());
    let script = container_script(digest, metadata);
    let platform = target
        .docker_platform
        .ok_or_else(|| "Debian target has no Docker platform".to_owned())?;
    let status = Command::new("docker")
        .args(["run", "--rm", "--platform", platform, "-v"])
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

fn container_script(digest: &str, metadata: super::release_targets::DebianArtifact) -> String {
    let package = format!("/work/{}", metadata.filename);
    format!(
        r#"export DEBIAN_FRONTEND=noninteractive
apt-get update
test "$(dpkg-deb --field {package} Package)" = "proqi"
test "$(dpkg-deb --field {package} Version)" = "{version}-1"
test "$(dpkg-deb --field {package} Architecture)" = "{architecture}"
dpkg-deb --field {package} Depends | grep -q 'libc6 (>= {glibc})'
dpkg-deb --field {package} Depends | grep -q 'libgcc-s1'
dpkg-deb --contents {package} | grep -q './usr/bin/proqi'
mkdir /tmp/wrong-architecture
dpkg-deb --raw-extract {package} /tmp/wrong-architecture
wrong_architecture=amd64
if [ "{architecture}" = amd64 ]; then wrong_architecture=arm64; fi
sed -i "s/^Architecture: .*$/Architecture: $wrong_architecture/" /tmp/wrong-architecture/DEBIAN/control
dpkg-deb --build /tmp/wrong-architecture /tmp/proqi-wrong-architecture.deb
if apt-get install -y /tmp/proqi-wrong-architecture.deb; then exit 1; fi
mkdir /tmp/missing-dependency
dpkg-deb --raw-extract {package} /tmp/missing-dependency
sed -i 's/^Depends:.*$/Depends: proqi-deliberately-missing-dependency/' /tmp/missing-dependency/DEBIAN/control
dpkg-deb --build /tmp/missing-dependency /tmp/proqi-missing-dependency.deb
if apt-get install -y /tmp/proqi-missing-dependency.deb; then exit 1; fi
if dpkg-query -W proqi >/dev/null 2>&1; then exit 1; fi
apt-get install -y {package}
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
apt-get install -y {package}
proqi --state-dir /tmp/proqi-state --json sessions > /tmp/sessions.json
grep -q '"sessions"' /tmp/sessions.json
grep -q '"id":"ses_' /tmp/sessions.json
dpkg-query -W -f='${{Status}}' proqi | grep -q 'install ok installed'
"#,
        version = env!("CARGO_PKG_VERSION"),
        package = package,
        architecture = metadata.architecture,
        glibc = super::release_targets::GLIBC_FLOOR,
    )
}

#[cfg(test)]
mod tests {
    use super::{container_script, image_profile};
    use crate::release_targets::DebianArtifact;

    #[test]
    fn install_contract_preserves_state_across_remove_and_reinstall() {
        let script = container_script(
            "abc123",
            DebianArtifact {
                architecture: "amd64",
                filename: "proqi_amd64.deb",
            },
        );
        assert!(script.contains("apt-get remove -y proqi"));
        assert!(script.contains("test -f /tmp/proqi-state/data/proqi.sqlite3"));
        assert!(script.contains("apt-get install -y /work/proqi_amd64.deb"));
        assert!(script.contains("proqi-wrong-architecture.deb"));
        assert!(script.contains("proqi-missing-dependency.deb"));
        assert!(script.contains("install -d -m 700 /tmp/proqi-state"));
        assert!(script.contains("--json doctor"));
        assert!(script.contains("'\"id\":\"ses_'"));
        assert!(script.contains("abc123"));
    }

    #[test]
    fn image_profiles_are_closed_and_immutable() {
        assert!(
            image_profile("ubuntu-22.04")
                .expect("profile")
                .image
                .contains("@sha256:")
        );
        assert!(image_profile("ubuntu:latest").is_err());
        assert!(image_profile("unknown").is_err());
    }
}
