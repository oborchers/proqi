//! Debian package assembly from the already verified GNU/Linux release archive.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Output},
};

use flate2::read::GzDecoder;
use serde_json::json;
use tar::Archive;

const NFPM_VERSION: &str = "2.47.0";
const MAINTAINER: &str = "Oliver Borchers <oliver-borchers@gmx.net>";
pub(super) const INSTALLED_PATHS: [(&str, &str, u32); 6] = [
    ("proqi", "/usr/bin/proqi", 0o755),
    (
        "proqi.bash",
        "/usr/share/bash-completion/completions/proqi",
        0o644,
    ),
    ("_proqi", "/usr/share/zsh/vendor-completions/_proqi", 0o644),
    (
        "proqi.fish",
        "/usr/share/fish/vendor_completions.d/proqi.fish",
        0o644,
    ),
    ("copyright", "/usr/share/doc/proqi/copyright", 0o644),
    (
        "THIRD-PARTY-NOTICES.md",
        "/usr/share/doc/proqi/THIRD-PARTY-NOTICES.md",
        0o644,
    ),
];

pub(super) fn package(
    root: &Path,
    archive: &Path,
    output: &Path,
    triple: &str,
) -> Result<(), String> {
    super::timing::phase("debian.package.preflight", require_linux)?;
    let target = super::release_targets::find(triple)?;
    let package = target
        .debian
        .ok_or_else(|| format!("target `{triple}` has no Debian artifact"))?;
    let archive = absolute_from(root, archive);
    let output = absolute_from(root, output);
    super::timing::phase("debian.package.nfpm", || {
        require_tool_version(root, "nfpm", NFPM_VERSION)
    })?;
    let inspected_digest = super::timing::phase("debian.package.archive_inspect", || {
        super::linux_compat::inspect_archive(&archive, target)
    })?;
    let temporary = tempfile::Builder::new()
        .prefix("proqi-debian-")
        .tempdir()
        .map_err(|error| format!("create Debian package root: {error}"))?;
    let stage = temporary.path().join("stage");
    fs::create_dir_all(&stage).map_err(|error| format!("create Debian stage: {error}"))?;
    super::timing::phase("debian.package.stage", || {
        extract_release_archive(&archive, temporary.path())?;
        stage_contents(root, temporary.path(), &stage, target)
    })?;
    let binary = stage.join("proqi");
    let (derived, dependencies) = super::timing::phase("debian.package.dependencies", || {
        let derived = derive_dependencies(temporary.path(), &binary, package.architecture)?;
        let dependencies = enforce_support_floor(&derived)?;
        Ok((derived, dependencies))
    })?;
    let version = super::release::workspace_version(root)?;
    let config = temporary.path().join("nfpm.yaml");
    fs::write(
        &config,
        render_config(&version.to_string(), &dependencies, package.architecture)?,
    )
    .map_err(|error| format!("write nFPM config: {error}"))?;
    fs::create_dir_all(&output).map_err(|error| format!("create Debian output: {error}"))?;
    let destination = output.join(package.filename);
    super::timing::phase("debian.package.construct", || {
        run_status(
            temporary.path(),
            "nfpm",
            ["package", "--packager", "deb", "--config"],
            Some(&config),
            ["--target"],
            Some(&destination),
        )
    })?;
    let archive_binary = extracted_binary(temporary.path(), target);
    if super::release::checksum(&archive_binary)? != inspected_digest {
        return Err("Linux archive changed between inspection and Debian staging".to_owned());
    }
    super::timing::phase("debian.package.static_verify", || {
        super::debian_verify::package(
            root,
            &destination,
            &archive_binary,
            &version.to_string(),
            &dependencies,
            package,
        )
    })?;
    super::timing::phase("debian.package.evidence", || {
        persist_evidence(
            &output,
            &destination,
            &archive,
            &archive_binary,
            &derived,
            &dependencies,
            target,
        )
    })?;
    println!("packaged {}", destination.display());
    Ok(())
}

pub(super) fn verify_evidence(
    root: &Path,
    archive: &Path,
    package: &Path,
    evidence_directory: &Path,
    triple: &str,
) -> Result<String, String> {
    let target = super::release_targets::find(triple)?;
    let package_metadata = target
        .debian
        .ok_or_else(|| format!("target `{triple}` has no Debian artifact"))?;
    let archive = absolute_from(root, archive);
    let package = absolute_from(root, package);
    let evidence_directory = absolute_from(root, evidence_directory);
    let package_digest = super::release::checksum(&package)?;
    let archive_digest = super::release::checksum(&archive)?;
    let temporary = tempfile::Builder::new()
        .prefix("proqi-debian-evidence-")
        .tempdir()
        .map_err(|error| format!("create Debian evidence root: {error}"))?;
    extract_release_archive(&archive, temporary.path())?;
    let binary_digest = super::release::checksum(&extracted_binary(temporary.path(), target))?;
    let checksum = fs::read_to_string(
        evidence_directory.join(format!("{}.sha256", package_metadata.filename)),
    )
    .map_err(|error| format!("read Debian checksum evidence: {error}"))?;
    let expected_checksum = format!("{package_digest}  {}\n", package_metadata.filename);
    if checksum != expected_checksum {
        return Err("Debian checksum evidence does not match the package".to_owned());
    }
    let evidence_path = evidence_directory.join("debian-evidence.json");
    let evidence: serde_json::Value = serde_json::from_slice(
        &fs::read(&evidence_path)
            .map_err(|error| format!("read {}: {error}", evidence_path.display()))?,
    )
    .map_err(|error| format!("parse Debian evidence: {error}"))?;
    validate_evidence(
        &evidence,
        archive.file_name().and_then(|name| name.to_str()),
        &archive_digest,
        &package_digest,
        &binary_digest,
        package_metadata.filename,
    )?;
    println!("verified downloaded Debian package and source archive evidence");
    Ok(binary_digest)
}

fn validate_evidence(
    evidence: &serde_json::Value,
    archive_name: Option<&str>,
    archive_digest: &str,
    package_digest: &str,
    binary_digest: &str,
    package_name: &str,
) -> Result<(), String> {
    let expected = [
        ("package", Some(package_name)),
        ("source_archive", archive_name),
        ("source_archive_sha256", Some(archive_digest)),
        ("sha256", Some(package_digest)),
        ("source_binary_sha256", Some(binary_digest)),
    ];
    if evidence
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(2)
    {
        return Err("Debian evidence requires schema_version 2".to_owned());
    }
    for (field, expected) in expected {
        if evidence.get(field).and_then(serde_json::Value::as_str) != expected {
            return Err(format!("Debian evidence field `{field}` does not match"));
        }
    }
    Ok(())
}

fn absolute_from(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn require_linux() -> Result<(), String> {
    cfg!(target_os = "linux")
        .then_some(())
        .ok_or_else(|| "Debian assembly must run on GNU/Linux".to_owned())
}

fn require_tool_version(root: &Path, program: &str, expected: &str) -> Result<(), String> {
    let output = command_output(root, program, ["--version"])?;
    let version = String::from_utf8_lossy(&output.stdout);
    version
        .split_whitespace()
        .any(|component| component.trim_start_matches('v') == expected)
        .then_some(())
        .ok_or_else(|| {
            format!(
                "{program} version is `{}`, expected {expected}",
                version.trim()
            )
        })
}

fn extract_release_archive(archive: &Path, output: &Path) -> Result<(), String> {
    let file = File::open(archive)
        .map_err(|error| format!("open release archive {}: {error}", archive.display()))?;
    let mut archive = Archive::new(GzDecoder::new(file));
    for entry in archive
        .entries()
        .map_err(|error| format!("read release archive: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("read release member: {error}"))?;
        if !entry
            .unpack_in(output)
            .map_err(|error| format!("extract release member: {error}"))?
        {
            return Err("release archive contains an escaping member".to_owned());
        }
    }
    Ok(())
}

fn extracted_root(root: &Path, target: super::release_targets::ReleaseTarget) -> PathBuf {
    root.join(format!("proqi-{}", target.triple))
}

fn extracted_binary(root: &Path, target: super::release_targets::ReleaseTarget) -> PathBuf {
    extracted_root(root, target).join("proqi")
}

fn stage_contents(
    root: &Path,
    extracted: &Path,
    stage: &Path,
    target: super::release_targets::ReleaseTarget,
) -> Result<(), String> {
    let source = extracted_root(extracted, target);
    for (from, to) in [
        (source.join("proqi"), stage.join("proqi")),
        (
            source.join("completions/proqi.bash"),
            stage.join("proqi.bash"),
        ),
        (source.join("completions/_proqi"), stage.join("_proqi")),
        (
            source.join("completions/proqi.fish"),
            stage.join("proqi.fish"),
        ),
        (
            source.join("THIRD-PARTY-NOTICES.md"),
            stage.join("THIRD-PARTY-NOTICES.md"),
        ),
    ] {
        fs::copy(&from, &to)
            .map_err(|error| format!("stage {} as {}: {error}", from.display(), to.display()))?;
    }
    fs::write(stage.join("copyright"), debian_copyright(root)?)
        .map_err(|error| format!("stage Debian copyright: {error}"))
}

fn debian_copyright(root: &Path) -> Result<String, String> {
    let license = fs::read_to_string(root.join("LICENSE"))
        .map_err(|error| format!("read license: {error}"))?;
    let formatted = license
        .lines()
        .map(|line| {
            if line.is_empty() {
                " .".to_owned()
            } else {
                format!(" {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\nUpstream-Name: Proqi\nSource: https://github.com/oborchers/proqi\n\nFiles: *\nCopyright: 2026 Oliver Borchers\nLicense: MIT\n{formatted}\n"
    ))
}

fn derive_dependencies(
    root: &Path,
    binary: &Path,
    architecture: &str,
) -> Result<Vec<String>, String> {
    let debian = root.join("debian");
    fs::create_dir_all(&debian).map_err(|error| format!("create Debian metadata root: {error}"))?;
    fs::write(
        debian.join("control"),
        format!("Source: proqi\nSection: utils\nPriority: optional\nMaintainer: Oliver Borchers <oliver-borchers@gmx.net>\nStandards-Version: 4.7.2\n\nPackage: proqi\nArchitecture: {architecture}\nDescription: thoughtpad for humans working with agents\n"),
    )
    .map_err(|error| format!("write dependency derivation control file: {error}"))?;
    let output = Command::new("dpkg-shlibdeps")
        .args(["-O", "-e"])
        .arg(binary)
        .current_dir(root)
        .output()
        .map_err(|error| format!("start dpkg-shlibdeps: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "dpkg-shlibdeps exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    parse_dependencies(&String::from_utf8_lossy(&output.stdout))
}

fn parse_dependencies(output: &str) -> Result<Vec<String>, String> {
    let value = output
        .lines()
        .find_map(|line| line.strip_prefix("shlibs:Depends="))
        .ok_or_else(|| "dpkg-shlibdeps did not emit shlibs:Depends".to_owned())?;
    let dependencies = value
        .split(',')
        .map(str::trim)
        .filter(|dependency| !dependency.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    (!dependencies.is_empty())
        .then_some(dependencies)
        .ok_or_else(|| "dpkg-shlibdeps emitted no dependencies".to_owned())
}

fn enforce_support_floor(derived: &[String]) -> Result<Vec<String>, String> {
    let mut by_name = BTreeMap::new();
    for dependency in derived {
        let name = dependency
            .split_whitespace()
            .next()
            .ok_or_else(|| format!("invalid derived dependency `{dependency}`"))?;
        if !matches!(name, "libc6" | "libgcc-s1") {
            return Err(format!("unexpected runtime dependency `{dependency}`"));
        }
        by_name.insert(name.to_owned(), dependency.to_owned());
    }
    if !by_name.contains_key("libc6") || !by_name.contains_key("libgcc-s1") {
        return Err(format!("incomplete runtime dependencies: {derived:?}"));
    }
    by_name.insert(
        "libc6".to_owned(),
        format!("libc6 (>= {})", super::release_targets::GLIBC_FLOOR),
    );
    Ok(by_name.into_values().collect())
}

fn render_config(
    version: &str,
    dependencies: &[String],
    architecture: &str,
) -> Result<String, String> {
    let mut config = format!(
        "name: proqi\narch: {architecture}\nplatform: linux\nversion: {version}\nrelease: 1\nsection: utils\npriority: optional\nmaintainer: \"{MAINTAINER}\"\ndescription: \"A thoughtpad for humans working with agents\"\nhomepage: https://github.com/oborchers/proqi\nlicense: MIT\n"
    );
    config.push_str("depends:\n");
    for dependency in dependencies {
        writeln!(config, "  - \"{dependency}\"")
            .map_err(|error| format!("render nFPM dependency: {error}"))?;
    }
    config.push_str("contents:\n");
    for (source, destination, mode) in INSTALLED_PATHS {
        write!(
            config,
            "  - src: stage/{source}\n    dst: {destination}\n    file_info:\n      mode: {mode:04o}\n"
        )
        .map_err(|error| format!("render nFPM content: {error}"))?;
    }
    Ok(config)
}

fn persist_evidence(
    output: &Path,
    package: &Path,
    archive: &Path,
    archive_binary: &Path,
    derived: &[String],
    installed: &[String],
    target: super::release_targets::ReleaseTarget,
) -> Result<(), String> {
    let package_name = target
        .debian
        .ok_or_else(|| "Debian target metadata is missing".to_owned())?
        .filename;
    let digest = super::release::checksum(package)?;
    fs::write(
        output.join(format!("{package_name}.sha256")),
        format!("{digest}  {package_name}\n"),
    )
    .map_err(|error| format!("write Debian checksum: {error}"))?;
    let evidence = json!({
        "schema_version": 2,
        "package": package_name,
        "target": target.triple,
        "sha256": digest,
        "source_archive": archive.file_name().and_then(|name| name.to_str()),
        "source_archive_sha256": super::release::checksum(archive)?,
        "source_binary_sha256": super::release::checksum(archive_binary)?,
        "derived_dependencies": derived,
        "installed_dependencies": installed,
        "installed_paths": INSTALLED_PATHS.map(|(_, destination, _)| destination),
        "maintainer_scripts": [],
    });
    fs::write(
        output.join("debian-evidence.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&evidence)
                .map_err(|error| format!("render Debian evidence: {error}"))?
        ),
    )
    .map_err(|error| format!("write Debian evidence: {error}"))
}

fn command_output<I, S>(root: &Path, program: &str, arguments: I) -> Result<Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("start {program}: {error}"))?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "{program} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn run_status<A, B, C, D>(
    root: &Path,
    program: &str,
    before: A,
    path: Option<&Path>,
    after: C,
    final_path: Option<&Path>,
) -> Result<(), String>
where
    A: IntoIterator<Item = B>,
    B: AsRef<std::ffi::OsStr>,
    C: IntoIterator<Item = D>,
    D: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new(program);
    command.args(before);
    if let Some(path) = path {
        command.arg(path);
    }
    command.args(after);
    if let Some(path) = final_path {
        command.arg(path);
    }
    let status = command
        .current_dir(root)
        .status()
        .map_err(|error| format!("start {program}: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("{program} exited with {status}"))
}

#[cfg(test)]
#[path = "debian_tests.rs"]
mod tests;
