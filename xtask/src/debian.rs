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

use super::release_targets::LINUX_X86_64;

pub(super) const PACKAGE_NAME: &str = "proqi_amd64.deb";
const NFPM_VERSION: &str = "2.47.0";
const MINIMUM_LIBC: &str = "2.35";
const MAINTAINER: &str = "Oliver Borchers <oliver-borchers@gmx.net>";
pub(super) const INSTALLED_PATHS: [(&str, &str, u32); 7] = [
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
    (
        "proqi-installation.json",
        "/usr/lib/proqi/proqi-installation.json",
        0o644,
    ),
];

pub(super) fn package(root: &Path, archive: &Path, output: &Path) -> Result<(), String> {
    require_linux()?;
    let archive = absolute_from(root, archive);
    let output = absolute_from(root, output);
    require_tool_version(root, "nfpm", NFPM_VERSION)?;
    let inspected_digest = super::linux_compat::inspect_archive(&archive)?;
    let temporary = tempfile::Builder::new()
        .prefix("proqi-debian-")
        .tempdir()
        .map_err(|error| format!("create Debian package root: {error}"))?;
    let stage = temporary.path().join("stage");
    fs::create_dir_all(&stage).map_err(|error| format!("create Debian stage: {error}"))?;
    extract_release_archive(&archive, temporary.path())?;
    stage_contents(root, temporary.path(), &stage)?;
    let binary = stage.join("proqi");
    let derived = derive_dependencies(temporary.path(), &binary)?;
    let dependencies = enforce_support_floor(&derived)?;
    let version = super::release::workspace_version(root)?;
    let config = temporary.path().join("nfpm.yaml");
    fs::write(&config, render_config(&version.to_string(), &dependencies)?)
        .map_err(|error| format!("write nFPM config: {error}"))?;
    fs::create_dir_all(&output).map_err(|error| format!("create Debian output: {error}"))?;
    let destination = output.join(PACKAGE_NAME);
    run_status(
        temporary.path(),
        "nfpm",
        ["package", "--packager", "deb", "--config"],
        Some(&config),
        ["--target"],
        Some(&destination),
    )?;
    let archive_binary = extracted_binary(temporary.path());
    if super::release::checksum(&archive_binary)? != inspected_digest {
        return Err("Linux archive changed between inspection and Debian staging".to_owned());
    }
    super::debian_verify::package(
        root,
        &destination,
        &archive_binary,
        &version.to_string(),
        &dependencies,
    )?;
    persist_evidence(
        &output,
        &destination,
        &archive,
        &archive_binary,
        &derived,
        &dependencies,
    )?;
    println!("packaged {}", destination.display());
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

fn extracted_root(root: &Path) -> PathBuf {
    root.join(format!("proqi-{LINUX_X86_64}"))
}

fn extracted_binary(root: &Path) -> PathBuf {
    extracted_root(root).join("proqi")
}

fn stage_contents(root: &Path, extracted: &Path, stage: &Path) -> Result<(), String> {
    let source = extracted_root(extracted);
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
        (
            source.join("proqi-installation.json"),
            stage.join("proqi-installation.json"),
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

fn derive_dependencies(root: &Path, binary: &Path) -> Result<Vec<String>, String> {
    let debian = root.join("debian");
    fs::create_dir_all(&debian).map_err(|error| format!("create Debian metadata root: {error}"))?;
    fs::write(
        debian.join("control"),
        "Source: proqi\nSection: utils\nPriority: optional\nMaintainer: Oliver Borchers <oliver-borchers@gmx.net>\nStandards-Version: 4.7.2\n\nPackage: proqi\nArchitecture: amd64\nDescription: thoughtpad for humans working with agents\n",
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
    by_name.insert("libc6".to_owned(), format!("libc6 (>= {MINIMUM_LIBC})"));
    Ok(by_name.into_values().collect())
}

fn render_config(version: &str, dependencies: &[String]) -> Result<String, String> {
    let mut config = format!(
        "name: proqi\narch: amd64\nplatform: linux\nversion: {version}\nrelease: 1\nsection: utils\npriority: optional\nmaintainer: \"{MAINTAINER}\"\ndescription: \"A thoughtpad for humans working with agents\"\nhomepage: https://github.com/oborchers/proqi\nlicense: MIT\n"
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
) -> Result<(), String> {
    let digest = super::release::checksum(package)?;
    fs::write(
        output.join(format!("{PACKAGE_NAME}.sha256")),
        format!("{digest}  {PACKAGE_NAME}\n"),
    )
    .map_err(|error| format!("write Debian checksum: {error}"))?;
    let evidence = json!({
        "schema_version": 1,
        "package": PACKAGE_NAME,
        "sha256": digest,
        "source_archive": archive.file_name().and_then(|name| name.to_str()),
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
