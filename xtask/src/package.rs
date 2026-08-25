//! Host archive staging and installed-product smoke orchestration.

use std::{
    ffi::OsStr,
    fs::{self, File},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use tar::{Archive, Builder};

const INSTALL_MARKER: &[u8] =
    br#"{"schema_version":1,"product":"proqi","kind":"standalone_archive"}"#;

pub(super) fn run(root: &Path) -> Result<(), String> {
    generate_notices(root)?;
    super::run(
        root,
        "cargo",
        [
            "build",
            "--locked",
            "--workspace",
            "--all-features",
            "--release",
        ],
    )?;
    let temporary = tempfile::Builder::new()
        .prefix("proqi-package-")
        .tempdir()
        .map_err(|error| format!("create package root: {error}"))?;
    let installed = install_binary(root, temporary.path())?;
    let host = host_triple(root)?;
    let archive = stage_archive(root, temporary.path(), &installed, &host)?;
    verify_archive(&archive, &host)?;
    prepare_isolated_state(temporary.path())?;
    run_installed_contract(root, temporary.path(), &installed, &archive)?;
    persist_archive(root, &archive)
}

fn install_binary(root: &Path, temporary: &Path) -> Result<PathBuf, String> {
    let executable = executable_name();
    let source = root.join("target/release").join(executable);
    let bin = temporary.join("install/bin");
    fs::create_dir_all(&bin).map_err(|error| format!("create install prefix: {error}"))?;
    let installed = bin.join(executable);
    fs::copy(&source, &installed)
        .map_err(|error| format!("install {}: {error}", source.display()))?;
    fs::write(bin.join("proqi-installation.json"), INSTALL_MARKER)
        .map_err(|error| format!("write installation marker: {error}"))?;
    Ok(installed)
}

fn host_triple(root: &Path) -> Result<String, String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .current_dir(root)
        .output()
        .map_err(|error| format!("start rustc -vV: {error}"))?;
    if !output.status.success() {
        return Err(format!("rustc -vV exited with {}", output.status));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("rustc -vV returned non-UTF-8 output: {error}"))?;
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .ok_or_else(|| "rustc -vV did not report a host triple".to_owned())
}

fn stage_archive(
    root: &Path,
    temporary: &Path,
    installed: &Path,
    host: &str,
) -> Result<PathBuf, String> {
    let package = package_name(host);
    let stage = temporary.join("stage").join(&package);
    let completions = stage.join("completions");
    fs::create_dir_all(&completions).map_err(|error| format!("create stage: {error}"))?;
    fs::copy(installed, stage.join(executable_name()))
        .map_err(|error| format!("stage executable: {error}"))?;
    fs::copy(root.join("LICENSE"), stage.join("LICENSE"))
        .map_err(|error| format!("stage license: {error}"))?;
    fs::copy(
        root.join("target/package/THIRD-PARTY-NOTICES.md"),
        stage.join("THIRD-PARTY-NOTICES.md"),
    )
    .map_err(|error| format!("stage third-party notices: {error}"))?;
    fs::write(stage.join("proqi-installation.json"), INSTALL_MARKER)
        .map_err(|error| format!("stage installation marker: {error}"))?;
    for (shell, filename) in [
        ("bash", "proqi.bash"),
        ("zsh", "_proqi"),
        ("fish", "proqi.fish"),
    ] {
        let script = command_output(installed, ["completions", shell])?;
        fs::write(completions.join(filename), script)
            .map_err(|error| format!("stage {shell} completions: {error}"))?;
    }
    let archive = temporary.join(format!("{package}.tar.gz"));
    let file = File::create(&archive).map_err(|error| format!("create archive: {error}"))?;
    let encoder = GzEncoder::new(file, Compression::best());
    let mut builder = Builder::new(encoder);
    append_archive_members(&mut builder, &stage, &package)?;
    let encoder = builder
        .into_inner()
        .map_err(|error| format!("finish tar archive: {error}"))?;
    encoder
        .finish()
        .map_err(|error| format!("finish compressed archive: {error}"))?;
    Ok(archive)
}

fn append_archive_members(
    builder: &mut Builder<GzEncoder<File>>,
    stage: &Path,
    package: &str,
) -> Result<(), String> {
    for relative in [
        executable_name(),
        "LICENSE",
        "THIRD-PARTY-NOTICES.md",
        "proqi-installation.json",
        "completions/proqi.bash",
        "completions/_proqi",
        "completions/proqi.fish",
    ] {
        let source = stage.join(relative);
        let member = Path::new(package).join(relative);
        builder
            .append_path_with_name(&source, &member)
            .map_err(|error| format!("archive {}: {error}", source.display()))?;
    }
    Ok(())
}

fn verify_archive(archive: &Path, host: &str) -> Result<(), String> {
    let file = File::open(archive).map_err(|error| format!("open archive: {error}"))?;
    let mut entries = Archive::new(GzDecoder::new(file))
        .entries()
        .map_err(|error| format!("read archive entries: {error}"))?
        .map(|entry| {
            let entry = entry.map_err(|error| format!("read archive member: {error}"))?;
            let path = entry
                .path()
                .map_err(|error| format!("read archive path: {error}"))?
                .into_owned();
            validate_member_path(&path)?;
            Ok(path)
        })
        .collect::<Result<Vec<_>, String>>()?;
    entries.sort();
    let package = package_name(host);
    let mut expected = [
        executable_name(),
        "LICENSE",
        "THIRD-PARTY-NOTICES.md",
        "proqi-installation.json",
        "completions/proqi.bash",
        "completions/_proqi",
        "completions/proqi.fish",
    ]
    .map(|relative| Path::new(&package).join(relative))
    .to_vec();
    expected.sort();
    (entries == expected)
        .then_some(())
        .ok_or_else(|| format!("archive members differ: found {entries:?}, expected {expected:?}"))
}

fn validate_member_path(path: &Path) -> Result<(), String> {
    let valid = !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    valid
        .then_some(())
        .ok_or_else(|| format!("archive contains unsafe member path: {}", path.display()))
}

fn prepare_isolated_state(temporary: &Path) -> Result<(), String> {
    for path in [
        temporary.join("state/config"),
        temporary.join("state/data"),
        temporary.join("state/cache"),
        temporary.join("state/runtime"),
        temporary.join("working"),
    ] {
        fs::create_dir_all(&path)
            .map_err(|error| format!("create isolated {}: {error}", path.display()))?;
    }
    fs::write(
        temporary.join("state/config/config.toml"),
        b"check_for_updates = false\n",
    )
    .map_err(|error| format!("disable package-smoke update check: {error}"))
}

fn run_installed_contract(
    root: &Path,
    temporary: &Path,
    installed: &Path,
    archive: &Path,
) -> Result<(), String> {
    println!("+ installed-product contract {}", installed.display());
    let status = Command::new("cargo")
        .args([
            "test",
            "--locked",
            "--test",
            "package_contract",
            "--",
            "--ignored",
            "--exact",
            "installed_product_contract",
        ])
        .env("PROQI_PACKAGE_BINARY", installed)
        .env("PROQI_PACKAGE_ARCHIVE", archive)
        .env("PROQI_PACKAGE_STATE", temporary.join("state"))
        .env("PROQI_PACKAGE_WORKING", temporary.join("working"))
        .env("PROQI_DISABLE_HERDR", "1")
        .current_dir(root)
        .status()
        .map_err(|error| format!("start installed-product contract: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("installed-product contract exited with {status}"))
}

fn persist_archive(root: &Path, archive: &Path) -> Result<(), String> {
    let output = root.join("target/package");
    fs::create_dir_all(&output).map_err(|error| format!("create package output: {error}"))?;
    let filename = archive
        .file_name()
        .ok_or_else(|| "archive has no filename".to_owned())?;
    let destination = output.join(filename);
    fs::copy(archive, &destination)
        .map_err(|error| format!("persist {}: {error}", destination.display()))?;
    println!("packaged {}", destination.display());
    Ok(())
}

pub(super) fn host_archive_path(root: &Path) -> Result<PathBuf, String> {
    Ok(root
        .join("target/package")
        .join(format!("{}.tar.gz", package_name(&host_triple(root)?))))
}

fn generate_notices(root: &Path) -> Result<(), String> {
    let output = root.join("target/package/THIRD-PARTY-NOTICES.md");
    fs::create_dir_all(
        output
            .parent()
            .ok_or_else(|| "notice output has no parent".to_owned())?,
    )
    .map_err(|error| format!("create notice output directory: {error}"))?;
    super::run(
        root,
        "cargo",
        [
            "about",
            "generate",
            "about.hbs",
            "--output-file",
            output
                .to_str()
                .ok_or_else(|| "notice path is not UTF-8".to_owned())?,
        ],
    )
}

fn command_output<I, S>(program: &Path, arguments: I) -> Result<Vec<u8>, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("start {}: {error}", program.display()))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "{} exited with {}: {}",
            program.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn package_name(host: &str) -> String {
    format!("proqi-{host}")
}

const fn executable_name() -> &'static str {
    if cfg!(windows) { "proqi.exe" } else { "proqi" }
}

#[cfg(test)]
mod tests {
    use super::validate_member_path;
    use std::path::Path;

    #[test]
    fn archive_paths_are_relative_and_cannot_escape() {
        assert!(validate_member_path(Path::new("proqi-aarch64-apple-darwin/proqi")).is_ok());
        assert!(validate_member_path(Path::new("../proqi")).is_err());
        assert!(validate_member_path(Path::new("/tmp/proqi")).is_err());
    }
}
