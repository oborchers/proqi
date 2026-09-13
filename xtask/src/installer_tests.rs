//! Contract tests for the generated release installer.

use std::{
    fs,
    io::Write as _,
    os::unix::fs::{PermissionsExt as _, symlink},
    path::{Path, PathBuf},
    process::{Command, Output},
};

use flate2::{Compression, write::GzEncoder};
use proqi::{
    adapters::update::SystemInstallDetector, domain::InstallationKind,
    ports::update::InstallDetector as _,
};
use tempfile::TempDir;

use super::render;

const VERSION: &str = "v1.2.3";
const TARGET: &str = "x86_64-unknown-linux-gnu";

struct Fixture {
    temporary: TempDir,
    script: PathBuf,
    assets: PathBuf,
    home: PathBuf,
    destination: PathBuf,
    path: String,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary fixture");
        let script = temporary.path().join("installer.sh");
        let assets = temporary.path().join("assets");
        let home = temporary.path().join("home");
        let destination = home.join("bin");
        let commands = temporary.path().join("commands");
        fs::create_dir_all(&assets).expect("assets");
        fs::create_dir_all(&home).expect("home");
        fs::create_dir_all(&commands).expect("commands");
        fs::write(&script, render(VERSION).expect("render installer")).expect("installer");
        write_command(&commands, "uname", UNAME);
        write_command(&commands, "getconf", GETCONF);
        write_command(&commands, "ldd", LDD);
        write_command(&commands, "curl", CURL);
        write_command(&commands, "tar", TAR);
        write_command(&commands, "mv", MV);
        write_command(&commands, "sha256sum", SHA256SUM);
        let original_path = std::env::var("PATH").expect("PATH");
        let path = format!("{}:{original_path}", commands.display());
        let fixture = Self {
            temporary,
            script,
            assets,
            home,
            destination,
            path,
        };
        fixture.write_archive(TARGET);
        fixture
    }

    fn command(&self) -> Command {
        let mut command = Command::new("sh");
        command
            .env("PATH", &self.path)
            .env("HOME", &self.home)
            .env("PROQI_INSTALL_DIR", &self.destination)
            .env("PROQI_TEST_ASSETS", &self.assets)
            .env("PROQI_TEST_OS", "Linux")
            .env("PROQI_TEST_CPU", "x86_64")
            .env("PROQI_TEST_LIBC", "gnu")
            .env("PROQI_TEST_LATEST", VERSION)
            .env("PROQI_TEST_DESTINATION", &self.destination);
        command
    }

    fn run(&self, arguments: &[&str], environment: &[(&str, &str)]) -> Output {
        let mut command = self.command();
        command.arg(&self.script).args(arguments);
        for (name, value) in environment {
            command.env(name, value);
        }
        command.output().expect("run installer")
    }

    fn write_archive(&self, target: &str) {
        let name = format!("proqi-{target}.tar.gz");
        let archive = self.assets.join(&name);
        let output = fs::File::create(&archive).expect("archive");
        let encoder = GzEncoder::new(output, Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let root = format!("proqi-{target}");
        append(
            &mut builder,
            &format!("{root}/proqi"),
            b"#!/bin/sh\nprintf 'proqi 1.2.3\\n'\n",
            0o755,
        );
        append(&mut builder, &format!("{root}/LICENSE"), b"MIT\n", 0o644);
        append(
            &mut builder,
            &format!("{root}/THIRD-PARTY-NOTICES.md"),
            b"notices\n",
            0o644,
        );
        append(
            &mut builder,
            &format!("{root}/proqi-installation.json"),
            crate::package::INSTALL_MARKER,
            0o644,
        );
        append(
            &mut builder,
            &format!("{root}/completions/proqi.bash"),
            b"bash\n",
            0o644,
        );
        append(
            &mut builder,
            &format!("{root}/completions/_proqi"),
            b"zsh\n",
            0o644,
        );
        append(
            &mut builder,
            &format!("{root}/completions/proqi.fish"),
            b"fish\n",
            0o644,
        );
        builder.finish().expect("finish archive");
        builder
            .into_inner()
            .expect("archive encoder")
            .finish()
            .expect("finish gzip");
        let digest = crate::release::checksum(&archive).expect("checksum");
        fs::write(
            self.assets.join(format!("{name}.sha256")),
            format!("{digest}  {name}\n"),
        )
        .expect("checksum file");
    }

    fn write_installer_assets(&self) {
        let installer = self.assets.join("proqi-installer.sh");
        fs::copy(&self.script, &installer).expect("installer asset");
        let digest = crate::release::checksum(&installer).expect("installer checksum");
        fs::write(
            self.assets.join("proqi-installer.sh.sha256"),
            format!("{digest}  proqi-installer.sh\n"),
        )
        .expect("installer checksum file");
    }

    fn assert_failure(&self, arguments: &[&str], environment: &[(&str, &str)], expected: &str) {
        let output = self.run(arguments, environment);
        assert!(!output.status.success(), "installer unexpectedly succeeded");
        let stderr = String::from_utf8(output.stderr).expect("UTF-8 stderr");
        assert!(
            stderr.contains(expected),
            "expected `{expected}` in `{stderr}`"
        );
    }
}

#[test]
fn installer_is_idempotent_and_selects_the_exact_native_target() {
    let fixture = Fixture::new();
    for _ in 0..2 {
        let output = fixture.run(&[], &[]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        fs::read_to_string(fixture.destination.join("proqi")).expect("installed"),
        "#!/bin/sh\nprintf 'proqi 1.2.3\\n'\n"
    );
    assert_updatable_installation(&fixture);
}

#[test]
fn exact_readme_command_installs_an_updatable_standalone_binary() {
    let fixture = Fixture::new();
    fixture.write_installer_assets();
    let command = readme_install_command();
    assert!(command.contains("--proto-redir \"=https\""));
    let output = fixture
        .command()
        .arg("-c")
        .arg(command)
        .output()
        .expect("run README installer command");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_updatable_installation(&fixture);
}

fn assert_updatable_installation(fixture: &Fixture) {
    let marker = fixture.destination.join("proqi-installation.json");
    assert_eq!(
        fs::read(marker).expect("installed marker"),
        crate::package::INSTALL_MARKER
    );
    let executable = fixture.destination.join("proqi");
    let installation = SystemInstallDetector::for_executable(executable.clone())
        .detect()
        .expect("detect installer-created installation");
    let executable = fs::canonicalize(executable).expect("canonical installed executable");
    assert_eq!(installation.kind, InstallationKind::StandaloneArchive);
    assert_eq!(installation.executable, executable);
    assert_eq!(installation.restart_executable, Some(executable));
}

fn readme_install_command() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let readme = fs::read_to_string(root.join("README.md")).expect("README");
    let install = readme
        .split_once("## Install\n")
        .expect("install section")
        .1;
    let code = install.split_once("```shell\n").expect("shell fence").1;
    code.split_once("\n```")
        .expect("closing fence")
        .0
        .to_owned()
}

#[test]
fn platform_and_libc_detection_fail_closed() {
    let fixture = Fixture::new();
    fixture.assert_failure(
        &[],
        &[("PROQI_TEST_OS", "FreeBSD")],
        "unsupported operating system",
    );
    fixture.assert_failure(
        &[],
        &[("PROQI_TEST_CPU", "riscv64")],
        "unsupported CPU architecture",
    );
    fixture.assert_failure(
        &[],
        &[("PROQI_TEST_LIBC", "ambiguous")],
        "could not determine libc",
    );
    fixture.assert_failure(
        &[],
        &[("PROQI_TEST_CPU", "aarch64")],
        "aarch64-unknown-linux-gnu",
    );
    fixture.assert_failure(
        &[],
        &[("PROQI_TEST_LIBC", "old")],
        "x86_64-unknown-linux-musl",
    );
}

#[test]
fn download_checksum_and_archive_failures_preserve_an_existing_installation() {
    let fixture = Fixture::new();
    fs::create_dir_all(&fixture.destination).expect("destination");
    fs::write(fixture.destination.join("proqi"), "existing\n").expect("existing binary");
    fixture.assert_failure(
        &[],
        &[("PROQI_TEST_NETWORK_FAIL", "1")],
        "could not download checksum",
    );
    assert_existing(&fixture);

    let checksum = fixture.assets.join(format!("proqi-{TARGET}.tar.gz.sha256"));
    fs::remove_file(&checksum).expect("remove checksum");
    fixture.assert_failure(&[], &[], "could not download checksum");
    fixture.write_archive(TARGET);
    fs::write(&checksum, vec![b'a'; 513]).expect("oversized checksum");
    fixture.assert_failure(&[], &[], "could not download checksum");
    fixture.write_archive(TARGET);
    let archive = fixture.assets.join(format!("proqi-{TARGET}.tar.gz"));
    fs::OpenOptions::new()
        .append(true)
        .open(archive)
        .expect("archive")
        .write_all(b"tamper")
        .expect("tamper");
    fixture.assert_failure(&[], &[], "archive checksum verification failed");
    assert_existing(&fixture);
}

#[test]
fn unsafe_members_and_extraction_failures_are_rejected_before_replacement() {
    let fixture = Fixture::new();
    fs::create_dir_all(&fixture.destination).expect("destination");
    fs::write(fixture.destination.join("proqi"), "existing\n").expect("existing binary");
    fixture.assert_failure(
        &[],
        &[("PROQI_TEST_TAR", "unsafe")],
        "unsafe or unexpected archive member",
    );
    fixture.assert_failure(
        &[],
        &[("PROQI_TEST_TAR", "extract-fail")],
        "could not extract verified release executable",
    );
    assert_existing(&fixture);
}

#[test]
fn failed_final_replacement_restores_the_existing_installation() {
    let fixture = Fixture::new();
    fs::create_dir_all(&fixture.destination).expect("destination");
    fs::write(fixture.destination.join("proqi"), "existing\n").expect("existing binary");
    fs::write(
        fixture.destination.join("proqi-installation.json"),
        "existing marker\n",
    )
    .expect("existing marker");
    fixture.assert_failure(
        &[],
        &[("PROQI_TEST_FINAL_MOVE_FAIL", "1")],
        "could not install executable",
    );
    assert_existing(&fixture);
    assert_eq!(
        fs::read_to_string(fixture.destination.join("proqi-installation.json"))
            .expect("existing marker"),
        "existing marker\n"
    );
}

#[test]
fn unsafe_destination_paths_and_existing_non_files_fail_before_replacement() {
    let fixture = Fixture::new();
    fixture.assert_failure(
        &[],
        &[("HOME", "/")],
        "HOME must not resolve to the filesystem root",
    );
    fs::create_dir_all(&fixture.destination).expect("destination");
    fs::create_dir(fixture.destination.join("proqi")).expect("directory collision");
    fixture.assert_failure(&[], &[], "existing Proqi executable is not a regular file");

    fs::remove_dir(fixture.destination.join("proqi")).expect("remove collision");
    let outside = fixture.temporary.path().join("outside");
    fs::create_dir(&outside).expect("outside directory");
    let escape = fixture.home.join("escape");
    symlink(&outside, &escape).expect("escape symlink");
    let escaped_destination = escape.join("new-bin");
    let output = fixture.run(
        &[
            "--prefix",
            escaped_destination.to_str().expect("UTF-8 path"),
        ],
        &[],
    );
    assert!(!output.status.success(), "escaped destination succeeded");
    assert!(
        String::from_utf8(output.stderr)
            .expect("UTF-8 stderr")
            .contains("installation destination must remain below HOME")
    );
    assert!(
        !escaped_destination.exists(),
        "installer created escaped path"
    );
}

#[test]
fn stale_latest_selection_is_bounded_and_rejected() {
    let fixture = Fixture::new();
    fixture.assert_failure(
        &["--version", "latest"],
        &[("PROQI_TEST_LATEST", "v1.2.4")],
        "stale installer",
    );
    fixture.assert_failure(
        &["--version", "v1.2.4"],
        &[],
        "requested version must exactly equal",
    );
}

#[test]
fn installer_is_version_bound_and_never_streams_an_archive_to_execution() {
    let script = render(VERSION).expect("installer");
    assert!(script.contains("sha256sum \"$temporary/$archive\""));
    assert!(!script.contains("sha256sum --check --strict"));
    assert!(script.contains("release archive member set is incomplete or duplicated"));
    assert!(script.contains("installation destination must remain below HOME"));
    assert!(script.contains("--proto-redir '=https'"));
    assert!(script.contains("--max-filesize \"$MAX_CHECKSUM_BYTES\""));
    assert!(script.contains("--max-filesize \"$MAX_ARCHIVE_BYTES\""));
    assert!(!script.contains("PROQI_RELEASE_BASE_URL"));
    assert!(!script.contains("curl | sh"));
    assert!(!script.contains("sudo"));
    assert!(!script.contains("apt "));
}

#[test]
fn every_runtime_selection_is_derived_from_target_metadata() {
    let script = render(VERSION).expect("installer");
    for target in crate::release_targets::ALL {
        assert!(script.contains(target.triple), "missing {}", target.triple);
    }
    assert!(!script.contains("@@TARGET_CASES@@"));
}

#[test]
fn unstable_or_ambiguous_versions_are_rejected() {
    for version in ["1.2.3", "v1.2", "v1.2.3-rc.1", "latest", "v01.2.3"] {
        assert!(render(version).is_err(), "accepted {version}");
    }
}

fn append(builder: &mut tar::Builder<GzEncoder<fs::File>>, path: &str, bytes: &[u8], mode: u32) {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(mode);
    header.set_cksum();
    builder
        .append_data(&mut header, path, bytes)
        .expect("archive member");
}

fn assert_existing(fixture: &Fixture) {
    assert_eq!(
        fs::read_to_string(fixture.destination.join("proqi")).expect("existing"),
        "existing\n"
    );
}

fn write_command(directory: &Path, name: &str, contents: &str) {
    let path = directory.join(name);
    fs::write(&path, contents).expect("fake command");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("fake permissions");
}

const UNAME: &str = "#!/bin/sh\ncase $1 in -s) printf '%s\\n' \"$PROQI_TEST_OS\" ;; -m) printf '%s\\n' \"$PROQI_TEST_CPU\" ;; *) exit 2 ;; esac\n";
const GETCONF: &str = "#!/bin/sh\ncase ${PROQI_TEST_LIBC:-gnu} in gnu) printf 'glibc 2.35\\n' ;; old) printf 'glibc 2.34\\n' ;; *) exit 2 ;; esac\n";
const LDD: &str = "#!/bin/sh\ncase ${PROQI_TEST_LIBC:-gnu} in musl|old) printf 'musl libc\\n' ;; *) printf 'unknown libc\\n' ;; esac\n";
const CURL: &str = r#"#!/bin/sh
output=
head=0
url=
maximum=
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output) output=$2; shift 2 ;;
    --head) head=1; shift ;;
    --max-filesize) maximum=$2; shift 2 ;;
    --write-out|--proto|--proto-redir|--connect-timeout|--max-time|--max-redirs|--retry) shift 2 ;;
    --fail|--silent|--show-error|--location|--tlsv1.2) shift ;;
    *) url=$1; shift ;;
  esac
done
[ "${PROQI_TEST_NETWORK_FAIL:-0}" = 0 ] || exit 22
if [ "$head" = 1 ]; then
  printf 'https://github.com/oborchers/proqi/releases/tag/%s' "$PROQI_TEST_LATEST"
  exit 0
fi
name=${url##*/}
source="$PROQI_TEST_ASSETS/$name"
[ "$(wc -c < "$source")" -le "$maximum" ] || exit 63
cp "$source" "$output"
"#;
const TAR: &str = r#"#!/bin/sh
case "${PROQI_TEST_TAR:-}" in
  unsafe) if [ "$1" = -tzf ]; then printf '../escape\n'; exit 0; fi ;;
  extract-fail) if [ "$1" = -xOzf ]; then exit 2; fi ;;
  wrong-architecture|wrong-libc) if [ "$1" = -xOzf ] && [ "${3##*/}" = proqi ]; then printf '#!/bin/sh\nexit 126\n'; exit 0; fi ;;
esac
exec /usr/bin/tar "$@"
"#;
const MV: &str = r#"#!/bin/sh
last=
for argument in "$@"; do last=$argument; done
if [ "${PROQI_TEST_FINAL_MOVE_FAIL:-0}" = 1 ] && [ "$last" = "$PROQI_TEST_DESTINATION/proqi" ]; then
  exit 1
fi
exec /bin/mv "$@"
"#;
const SHA256SUM: &str = r#"#!/bin/sh
case "$1" in -*) exit 64 ;; esac
exec /usr/bin/shasum -a 256 "$1"
"#;

#[path = "installer_update_tests.rs"]
mod update_contract;
