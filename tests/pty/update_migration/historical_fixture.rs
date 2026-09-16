//! Exact v0.9.0 source and Homebrew-shaped installation fixture.

use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use proqi::{domain::InstallationIdentity, ports::update::InstallDetector as _};

use super::old_fixture::{BUILD_TIMEOUT, run_bounded};

const HISTORICAL_TAG: &str = "v0.9.0";
const HISTORICAL_COMMIT: &str = "dd05c49bf3c1e8c1aa2cd707ed3f3b40ca2bc2b9";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct HistoricalFixture {
    _root: tempfile::TempDir,
    binary: PathBuf,
}

pub(super) struct HistoricalInstallation {
    pub(super) old_binary: PathBuf,
    pub(super) active_binary: PathBuf,
    pub(super) current_binary: PathBuf,
    pub(super) identity: InstallationIdentity,
}

impl HistoricalFixture {
    pub(super) fn build() -> Self {
        let root = tempfile::Builder::new()
            .prefix("proqi-v0.9.0-source")
            .tempdir_in("/private/tmp")
            .expect("historical source root");
        verify_tag();
        let archive = root.path().join("v0.9.0.tar");
        let source = root.path().join("source");
        fs::create_dir(&source).expect("historical source directory");
        let mut export = Command::new("git");
        export
            .args(["archive", "--format=tar", "--output"])
            .arg(&archive)
            .arg(HISTORICAL_TAG)
            .current_dir(env!("CARGO_MANIFEST_DIR"));
        let output = run_bounded(&mut export, COMMAND_TIMEOUT, "historical source export");
        assert_success(&output, "historical source export");
        let mut extract = Command::new("/usr/bin/tar");
        extract.args(["-xf"]).arg(&archive).arg("-C").arg(&source);
        let output = run_bounded(
            &mut extract,
            COMMAND_TIMEOUT,
            "historical source extraction",
        );
        assert_success(&output, "historical source extraction");
        use_cached_rustls(&source);
        let target = root.path().join("target");
        let mut build = Command::new("cargo");
        build
            .args([
                "build",
                "--offline",
                "--locked",
                "--package",
                "proqi",
                "--bin",
                "proqi",
            ])
            .current_dir(&source)
            .env("CARGO_TARGET_DIR", &target);
        let output = run_bounded(&mut build, BUILD_TIMEOUT, "historical v0.9.0 build");
        assert_success(&output, "historical v0.9.0 build");
        let binary = target.join("debug/proqi");
        assert_ne!(
            fs::read(&binary).expect("historical executable bytes"),
            fs::read(env!("CARGO_BIN_EXE_proqi")).expect("current executable bytes")
        );
        Self {
            _root: root,
            binary,
        }
    }

    pub(super) fn install(&self, state: &Path) -> HistoricalInstallation {
        let prefix = state.join("prefix");
        let old_binary = prefix.join("Cellar/proqi/0.9.0/bin/proqi");
        let current_binary = prefix
            .join("Cellar/proqi")
            .join(env!("CARGO_PKG_VERSION"))
            .join("bin/proqi");
        install_keg(&self.binary, &old_binary);
        install_keg(Path::new(env!("CARGO_BIN_EXE_proqi")), &current_binary);
        let active_binary = prefix.join("opt/proqi/bin/proqi");
        fs::create_dir_all(active_binary.parent().expect("active binary parent"))
            .expect("active installation directory");
        symlink(&old_binary, &active_binary).expect("activate historical keg");
        let identity =
            proqi::adapters::update::SystemInstallDetector::for_executable(old_binary.clone())
                .detect()
                .expect("historical installation identity")
                .identity;
        HistoricalInstallation {
            old_binary,
            active_binary,
            current_binary,
            identity,
        }
    }
}

fn use_cached_rustls(source: &Path) {
    // The release lock's sole third-party drift is this semver-compatible patch. The current
    // workspace build has fetched it, which keeps the historical source build offline in CI.
    const HISTORICAL: &str = "name = \"rustls\"\nversion = \"0.23.43\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"0283386ce02abc0151e1761d08802dfe86c173b0b494af5cbc086574e453da06\"";
    const CURRENT: &str = "name = \"rustls\"\nversion = \"0.23.45\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"0d41d731c7d2f962d1ccc364cec258de3c0e93b38c2fb3ba97ac74513048d634\"";
    let lock = source.join("Cargo.lock");
    let contents = fs::read_to_string(&lock).expect("historical lockfile");
    assert_eq!(contents.matches(HISTORICAL).count(), 1);
    fs::write(lock, contents.replace(HISTORICAL, CURRENT)).expect("cached historical lockfile");
}

impl HistoricalInstallation {
    pub(super) fn replace_externally(&self) {
        fs::remove_file(&self.active_binary).expect("remove historical active link");
        symlink(&self.current_binary, &self.active_binary).expect("activate current keg");
        fs::remove_file(&self.old_binary).expect("remove historical Cellar executable");
        assert!(!self.old_binary.exists());
    }
}

fn verify_tag() {
    let mut command = Command::new("git");
    command
        .args(["rev-list", "-n", "1", HISTORICAL_TAG])
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    let output = run_bounded(&mut command, COMMAND_TIMEOUT, "historical tag verification");
    assert_success(&output, "historical tag verification");
    assert_eq!(
        String::from_utf8(output.stdout)
            .expect("historical commit UTF-8")
            .trim(),
        HISTORICAL_COMMIT
    );
}

fn install_keg(source: &Path, binary: &Path) {
    fs::create_dir_all(binary.parent().expect("keg binary parent")).expect("keg directory");
    fs::copy(source, binary).expect("install keg executable");
    fs::write(
        binary
            .parent()
            .and_then(Path::parent)
            .expect("keg root")
            .join("INSTALL_RECEIPT.json"),
        b"{}",
    )
    .expect("installation receipt");
}

fn assert_success(output: &std::process::Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
