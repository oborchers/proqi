//! Credential-free release planning and host rehearsal.

use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    process::Command,
};

use semver::Version;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const DIST_VERSION: &str = "0.32.0";
use super::release_targets::ALL as TARGETS;

pub(super) fn plan(root: &Path, requested_tag: Option<&str>) -> Result<(), String> {
    let version = workspace_version(root)?;
    let tag = requested_tag.map_or_else(|| format!("v{version}"), str::to_owned);
    super::release_readiness::validate_preparation(root, &tag)
}

pub(super) fn rehearse(root: &Path) -> Result<(), String> {
    let version = workspace_version(root)?;
    let tag = format!("v{version}");
    let plan = plan_output(root, Some(&tag))?;
    let manifest: Value =
        serde_json::from_slice(&plan).map_err(|error| format!("parse cargo-dist plan: {error}"))?;
    validate_planned_targets(&manifest)?;
    super::package::run(root, None)?;

    let output = root.join("target/release-rehearsal");
    recreate_output(root, &output)?;
    fs::write(output.join("dist-plan.json"), &plan)
        .map_err(|error| format!("write dist plan: {error}"))?;
    let archive = super::package::host_archive_path(root)?;
    let digest = checksum(&archive)?;
    let archive_name = filename(&archive)?;
    fs::write(
        output.join("SHA256SUMS"),
        format!("{digest}  {archive_name}\n"),
    )
    .map_err(|error| format!("write checksums: {error}"))?;
    fs::copy(&archive, output.join(&archive_name))
        .map_err(|error| format!("copy host archive: {error}"))?;
    fs::copy(
        root.join("target/package/THIRD-PARTY-NOTICES.md"),
        output.join("THIRD-PARTY-NOTICES.md"),
    )
    .map_err(|error| format!("copy notices: {error}"))?;
    let sbom = output.join(format!("{archive_name}.spdx.json"));
    generate_sbom(root, &sbom)?;
    validate_sbom(&sbom)?;
    super::homebrew::write_rehearsal(&output, &version, &archive_name, &digest)?;
    write_summary(&output, &version, &tag, &archive_name, &digest)?;
    println!("release rehearsal complete: {}", output.display());
    Ok(())
}

pub(super) fn print_checksum(root: &Path, path: &Path) -> Result<(), String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    println!("{}  {}", checksum(&path)?, filename(&path)?);
    Ok(())
}

fn plan_output(root: &Path, requested_tag: Option<&str>) -> Result<Vec<u8>, String> {
    verify_dist(root)?;
    let version = workspace_version(root)?;
    let tag = requested_tag.map_or_else(|| format!("v{version}"), str::to_owned);
    validate_tag(&tag, &version)?;
    super::release_readiness::validate_release_content(root, &tag)?;
    let output = Command::new("dist")
        .args(["plan", "--tag", &tag, "--output-format", "json"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("start cargo-dist plan: {error}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "cargo-dist plan exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn verify_dist(root: &Path) -> Result<(), String> {
    let output = Command::new("dist")
        .arg("--version")
        .current_dir(root)
        .output()
        .map_err(|error| format!("start cargo-dist: {error}"))?;
    let version = String::from_utf8(output.stdout)
        .map_err(|error| format!("cargo-dist version is not UTF-8: {error}"))?;
    let expected = format!("cargo-dist {DIST_VERSION}");
    (output.status.success() && version.trim() == expected)
        .then_some(())
        .ok_or_else(|| format!("expected `{expected}`, found `{}`", version.trim()))
}

pub(super) fn workspace_version(root: &Path) -> Result<Version, String> {
    let contents = fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|error| format!("read Cargo.toml: {error}"))?;
    let manifest: toml::Value =
        toml::from_str(&contents).map_err(|error| format!("parse Cargo.toml: {error}"))?;
    let raw = manifest
        .get("workspace")
        .and_then(|value| value.get("package"))
        .and_then(|value| value.get("version"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| "Cargo.toml has no workspace package version".to_owned())?;
    Version::parse(raw).map_err(|error| format!("parse workspace version: {error}"))
}

fn validate_tag(tag: &str, version: &Version) -> Result<(), String> {
    let raw = tag
        .strip_prefix('v')
        .ok_or_else(|| format!("release tag `{tag}` must begin with `v`"))?;
    let parsed =
        Version::parse(raw).map_err(|error| format!("invalid release tag `{tag}`: {error}"))?;
    let canonical = format!("v{}.{}.{}", parsed.major, parsed.minor, parsed.patch);
    if parsed.pre.is_empty() && parsed.build.is_empty() && tag == canonical && &parsed == version {
        Ok(())
    } else {
        Err(format!(
            "release tag `{tag}` must exactly match stable Cargo version `v{version}`"
        ))
    }
}

fn validate_planned_targets(manifest: &Value) -> Result<(), String> {
    let artifacts = manifest
        .get("artifacts")
        .and_then(Value::as_object)
        .ok_or_else(|| "cargo-dist plan has no artifacts map".to_owned())?;
    let actual = artifacts
        .values()
        .filter_map(|artifact| artifact.get("target_triples"))
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let expected = TARGETS
        .iter()
        .map(|target| (*target).to_owned())
        .collect::<BTreeSet<_>>();
    (actual == expected).then_some(()).ok_or_else(|| {
        format!("cargo-dist targets differ: found {actual:?}, expected {expected:?}")
    })
}

fn recreate_output(root: &Path, output: &Path) -> Result<(), String> {
    let expected = root.join("target/release-rehearsal");
    if output != expected {
        return Err(format!(
            "refuse unexpected rehearsal path: {}",
            output.display()
        ));
    }
    if output.exists() {
        fs::remove_dir_all(output).map_err(|error| format!("clear rehearsal output: {error}"))?;
    }
    fs::create_dir_all(output).map_err(|error| format!("create rehearsal output: {error}"))
}

pub(super) fn checksum(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    Ok(hex_digest(digest.as_ref()))
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn generate_sbom(root: &Path, output: &Path) -> Result<(), String> {
    let destination = format!("spdx-json={}", output.display());
    let status = Command::new("syft")
        .args(["scan", "dir:.", "--exclude", "./target", "--output"])
        .arg(destination)
        .current_dir(root)
        .status()
        .map_err(|error| format!("start syft: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("syft exited with {status}"))
}

fn validate_sbom(path: &Path) -> Result<(), String> {
    let contents = fs::read(path).map_err(|error| format!("read SBOM: {error}"))?;
    let value: Value =
        serde_json::from_slice(&contents).map_err(|error| format!("parse SPDX JSON: {error}"))?;
    (value.get("spdxVersion").and_then(Value::as_str) == Some("SPDX-2.3"))
        .then_some(())
        .ok_or_else(|| "SBOM is not SPDX 2.3 JSON".to_owned())
}

fn write_summary(
    output: &Path,
    version: &Version,
    tag: &str,
    archive: &str,
    digest: &str,
) -> Result<(), String> {
    let summary = json!({
        "schema_version": 1,
        "version": version.to_string(),
        "tag": tag,
        "cargo_dist_version": DIST_VERSION,
        "release_targets": TARGETS,
        "host_archive": archive,
        "host_sha256": digest,
        "reproducibility": "Source, lockfile, tool versions, and artifact layout are pinned; byte-for-byte reproduction is not claimed across operating systems or toolchain builds.",
        "ci_only": TARGETS.iter().filter(|target| !archive.contains(**target)).collect::<Vec<_>>(),
    });
    let mut file = File::create(output.join("rehearsal.json"))
        .map_err(|error| format!("create rehearsal summary: {error}"))?;
    serde_json::to_writer_pretty(&mut file, &summary)
        .map_err(|error| format!("write rehearsal summary: {error}"))?;
    file.write_all(b"\n")
        .map_err(|error| format!("finish rehearsal summary: {error}"))
}

fn filename(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| format!("path has no UTF-8 filename: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{hex_digest, validate_tag, workspace_version};
    use semver::Version;
    use std::path::Path;

    #[test]
    fn release_tag_is_canonical_stable_and_matches_cargo() {
        let version = Version::new(0, 1, 0);
        assert!(validate_tag("v0.1.0", &version).is_ok());
        assert!(validate_tag("0.1.0", &version).is_err());
        assert!(validate_tag("v0.1.0-alpha.1", &version).is_err());
        assert!(validate_tag("v0.1.00", &version).is_err());
        assert!(validate_tag("v0.2.0", &version).is_err());
    }

    #[test]
    fn workspace_version_comes_from_the_single_cargo_source() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask manifest has a workspace parent");
        let expected = Version::parse(env!("CARGO_PKG_VERSION")).expect("workspace version");
        assert_eq!(workspace_version(root), Ok(expected.clone()));
        assert!(
            super::super::release_highlights::validate(root, Some(&format!("v{expected}"))).is_ok()
        );
        assert!(super::super::release_highlights::validate(root, Some("v9.9.9")).is_err());
        let tag = format!("v{expected}");
        assert!(
            root.join(format!(".github/release-notes/{tag}.md"))
                .is_file()
        );
    }

    #[test]
    fn digest_hex_encoding_is_lowercase_and_zero_padded() {
        assert_eq!(hex_digest(&[0x00, 0x09, 0xaf, 0xff]), "0009afff");
    }
}
