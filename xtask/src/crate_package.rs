//! Credential-free crates.io package assembly and installed-product verification.

use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read as _,
    path::{Component, Path, PathBuf},
    process::{Command, Output},
};

use flate2::read::GzDecoder;
use serde_json::{Value, json};
use tar::Archive;

const REGISTRY: &str = "crates-io";
const PRIVATE_MARKERS: [&str; 4] = ["/Users/", "Code.nosync", "TemporaryItems", "/home/runner"];

pub(super) fn run(root: &Path) -> Result<(), String> {
    verify(root, true)
}

pub(super) fn evidence(root: &Path) -> Result<(), String> {
    verify(root, false)
}

fn verify(root: &Path, dry_run: bool) -> Result<(), String> {
    super::release_highlights::validate(root, None)?;
    super::run(root, "cargo", ["package", "--locked"])?;
    if dry_run {
        super::run(root, "cargo", ["publish", "--dry-run", "--locked"])?;
    }
    let version = super::release::workspace_version(root)?;
    let crate_path = root
        .join("target/package")
        .join(format!("proqi-{version}.crate"));
    let digest = super::release::checksum(&crate_path)?;
    let temporary = tempfile::Builder::new()
        .prefix("proqi-crate-contract-")
        .tempdir()
        .map_err(|error| format!("create crate verification root: {error}"))?;
    let package_root = extract_and_verify(root, &crate_path, temporary.path(), &version)?;
    verify_packaged_install(root, temporary.path(), &package_root, &version)?;
    persist_evidence(root, &crate_path, &digest, &version)?;
    println!(
        "verified crates.io package: {} sha256 {digest}",
        crate_path.display()
    );
    Ok(())
}

fn extract_and_verify(
    root: &Path,
    crate_path: &Path,
    output: &Path,
    version: &semver::Version,
) -> Result<PathBuf, String> {
    let expected_root = format!("proqi-{version}");
    let file = File::open(crate_path)
        .map_err(|error| format!("open {}: {error}", crate_path.display()))?;
    let mut archive = Archive::new(GzDecoder::new(file));
    let mut actual = BTreeSet::new();
    for entry in archive
        .entries()
        .map_err(|error| format!("read crate archive: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("read crate member: {error}"))?;
        let path = entry
            .path()
            .map_err(|error| format!("read crate member path: {error}"))?
            .into_owned();
        validate_member(&path, &expected_root)?;
        let relative = path
            .strip_prefix(&expected_root)
            .map_err(|error| format!("strip crate package root: {error}"))?
            .to_path_buf();
        actual.insert(relative);
        if !entry
            .unpack_in(output)
            .map_err(|error| format!("extract crate member: {error}"))?
        {
            return Err("crate archive contains an escaping member".to_owned());
        }
    }
    let expected = expected_members(root)?;
    if actual != expected {
        return Err(format!(
            "crate members differ\nfound: {actual:#?}\nexpected: {expected:#?}"
        ));
    }
    let package_root = output.join(expected_root);
    verify_manifest(&package_root.join("Cargo.toml"), version)?;
    verify_vcs(root, &package_root.join(".cargo_vcs_info.json"))?;
    reject_private_markers(&package_root)?;
    Ok(package_root)
}

fn validate_member(path: &Path, expected_root: &str) -> Result<(), String> {
    let safe = !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && path.starts_with(expected_root);
    safe.then_some(())
        .ok_or_else(|| format!("unsafe crate member: {}", path.display()))
}

fn expected_members(root: &Path) -> Result<BTreeSet<PathBuf>, String> {
    let mut expected = [
        ".cargo_vcs_info.json",
        "Cargo.lock",
        "Cargo.toml",
        "Cargo.toml.orig",
        "LICENSE",
        "README.md",
        "release-highlights.json",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect::<BTreeSet<_>>();
    collect_rust_sources(root, &root.join("src"), &mut expected)?;
    Ok(expected)
}

fn collect_rust_sources(
    root: &Path,
    directory: &Path,
    expected: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("read source directory {}: {error}", directory.display()))?
    {
        let path = entry
            .map_err(|error| format!("read source entry: {error}"))?
            .path();
        if path.is_dir() {
            collect_rust_sources(root, &path, expected)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            expected.insert(
                path.strip_prefix(root)
                    .map_err(|error| format!("make source path relative: {error}"))?
                    .to_path_buf(),
            );
        }
    }
    Ok(())
}

fn verify_manifest(path: &Path, version: &semver::Version) -> Result<(), String> {
    let contents =
        fs::read_to_string(path).map_err(|error| format!("read normalized manifest: {error}"))?;
    let manifest: toml::Value =
        toml::from_str(&contents).map_err(|error| format!("parse normalized manifest: {error}"))?;
    let package = manifest
        .get("package")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| "normalized manifest has no package table".to_owned())?;
    require_string(package, "name", "proqi")?;
    require_string(package, "version", &version.to_string())?;
    let publish = package
        .get("publish")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| "normalized manifest has no registry allowlist".to_owned())?;
    if publish.as_slice() != [toml::Value::String(REGISTRY.to_owned())] {
        return Err(format!(
            "unexpected publish registry allowlist: {publish:?}"
        ));
    }
    for forbidden in ["workspace", "patch", "replace"] {
        if manifest.get(forbidden).is_some() {
            return Err(format!("normalized manifest retained `{forbidden}`"));
        }
    }
    let dependencies = manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| "normalized manifest has no dependencies".to_owned())?;
    for (name, dependency) in dependencies {
        if dependency
            .as_table()
            .is_some_and(|table| table.contains_key("git") || table.contains_key("path"))
        {
            return Err(format!("dependency `{name}` is not registry-resolvable"));
        }
    }
    Ok(())
}

fn require_string(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
    expected: &str,
) -> Result<(), String> {
    let actual = table.get(key).and_then(toml::Value::as_str);
    (actual == Some(expected))
        .then_some(())
        .ok_or_else(|| format!("normalized `{key}` is {actual:?}, expected `{expected}`"))
}

fn verify_vcs(root: &Path, path: &Path) -> Result<(), String> {
    let value: Value = serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("read crate VCS metadata: {error}"))?,
    )
    .map_err(|error| format!("parse crate VCS metadata: {error}"))?;
    let expected = command_text(root, "git", ["rev-parse", "HEAD"])?;
    let dirty = value
        .pointer("/git/dirty")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if value.pointer("/git/sha1").and_then(Value::as_str) != Some(expected.trim()) || dirty {
        return Err(format!(
            "crate VCS metadata does not bind clean HEAD: {value}"
        ));
    }
    Ok(())
}

fn reject_private_markers(root: &Path) -> Result<(), String> {
    let mut files = BTreeSet::new();
    collect_files(root, root, &mut files)?;
    for path in files {
        let mut bytes = Vec::new();
        File::open(&path)
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .map_err(|error| format!("read packaged {}: {error}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        if let Some(marker) = PRIVATE_MARKERS
            .iter()
            .find(|marker| text.contains(**marker))
        {
            return Err(format!(
                "packaged file {} contains private marker `{marker}`",
                path.display()
            ));
        }
    }
    Ok(())
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("read packaged directory {}: {error}", directory.display()))?
    {
        let path = entry
            .map_err(|error| format!("read packaged entry: {error}"))?
            .path();
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else if path.is_file() && path != root.join("Cargo.lock") {
            files.insert(path);
        }
    }
    Ok(())
}

fn verify_packaged_install(
    root: &Path,
    temporary: &Path,
    package_root: &Path,
    version: &semver::Version,
) -> Result<(), String> {
    let cargo_home = temporary.join("cargo-home");
    let install_root = temporary.join("install");
    fs::create_dir_all(&cargo_home)
        .map_err(|error| format!("create isolated Cargo home: {error}"))?;
    let status = Command::new("cargo")
        .args(["install", "--locked", "--path"])
        .arg(package_root)
        .args(["--root"])
        .arg(&install_root)
        .env("CARGO_HOME", &cargo_home)
        .current_dir(root)
        .status()
        .map_err(|error| format!("start packaged-source install: {error}"))?;
    if !status.success() {
        return Err(format!("packaged-source install exited with {status}"));
    }
    let binary = install_root.join("bin/proqi");
    let expected_version = format!("proqi {version}");
    let actual_version = command_text(root, &binary, ["--version"])?;
    if actual_version.trim() != expected_version {
        return Err(format!(
            "packaged binary reported `{}`, expected `{expected_version}`",
            actual_version.trim()
        ));
    }
    let state = temporary.join("state");
    let capabilities = isolated_json(root, &binary, &state, ["capabilities"])?;
    if capabilities
        .pointer("/data/commands")
        .and_then(Value::as_array)
        .is_none()
    {
        return Err("packaged capabilities omitted commands".to_owned());
    }
    let created = isolated_json(root, &binary, &state, std::iter::empty::<&str>())?;
    if created
        .pointer("/data/session_id")
        .and_then(Value::as_str)
        .is_none()
    {
        return Err("packaged binary did not create isolated state".to_owned());
    }
    Ok(())
}

fn isolated_json<I, S>(
    root: &Path,
    binary: &Path,
    state: &Path,
    arguments: I,
) -> Result<Value, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new(binary)
        .arg("--state-dir")
        .arg(state)
        .arg("--json")
        .args(arguments)
        .env("PROQI_DISABLE_HERDR", "1")
        .current_dir(root)
        .output()
        .map_err(|error| format!("start packaged JSON command: {error}"))?;
    parse_success(&output)
}

fn parse_success(output: &Output) -> Result<Value, String> {
    if !output.status.success() {
        return Err(format!(
            "packaged command exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("parse packaged JSON: {error}"))?;
    (value.get("ok").and_then(Value::as_bool) == Some(true))
        .then_some(value)
        .ok_or_else(|| "packaged JSON command did not return success".to_owned())
}

fn persist_evidence(
    root: &Path,
    crate_path: &Path,
    digest: &str,
    version: &semver::Version,
) -> Result<(), String> {
    let output = root.join("target/crate-package");
    if output.exists() {
        fs::remove_dir_all(&output).map_err(|error| format!("clear crate evidence: {error}"))?;
    }
    fs::create_dir_all(&output).map_err(|error| format!("create crate evidence: {error}"))?;
    let name = format!("proqi-{version}.crate");
    fs::copy(crate_path, output.join(&name))
        .map_err(|error| format!("copy verified crate: {error}"))?;
    fs::write(
        output.join(format!("{name}.sha256")),
        format!("{digest}  {name}\n"),
    )
    .map_err(|error| format!("write crate checksum: {error}"))?;
    let evidence = json!({
        "schema_version": 1,
        "package": "proqi",
        "version": version.to_string(),
        "registry": REGISTRY,
        "crate": name,
        "sha256": digest,
    });
    fs::write(
        output.join("crate-evidence.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&evidence)
                .map_err(|error| format!("render crate evidence: {error}"))?
        ),
    )
    .map_err(|error| format!("write crate evidence: {error}"))
}

fn command_text<I, S>(
    root: &Path,
    program: impl AsRef<std::ffi::OsStr>,
    arguments: I,
) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("start command: {error}"))?;
    if !output.status.success() {
        return Err(format!("command exited with {}", output.status));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("command output is not UTF-8: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{validate_member, verify_manifest};
    use semver::Version;
    use std::{fs, path::Path};

    #[test]
    fn crate_members_stay_below_the_canonical_package_root() {
        assert!(validate_member(Path::new("proqi-0.1.2/src/lib.rs"), "proqi-0.1.2").is_ok());
        assert!(validate_member(Path::new("proqi-0.1.2/../secret"), "proqi-0.1.2").is_err());
        assert!(validate_member(Path::new("other/src/lib.rs"), "proqi-0.1.2").is_err());
    }

    #[test]
    fn normalized_manifest_rejects_non_registry_dependencies() {
        let root = tempfile::tempdir().expect("manifest root");
        let path = root.path().join("Cargo.toml");
        fs::write(
            &path,
            "[package]\nname='proqi'\nversion='0.1.2'\npublish=['crates-io']\n[dependencies]\nbad={git='https://example.test/repo'}\n",
        )
        .expect("manifest fixture");
        assert!(verify_manifest(&path, &Version::new(0, 1, 2)).is_err());
    }
}
