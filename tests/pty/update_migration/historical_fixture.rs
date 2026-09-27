//! Pinned released sources and private Homebrew-shaped installation fixtures.

use std::{
    fmt::Write as _,
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use proqi::{domain::InstallationIdentity, ports::update::InstallDetector as _};

use super::old_fixture::{BUILD_TIMEOUT, run_bounded, shared_fixture_target};

struct Release {
    tag: &'static str,
    commit: &'static str,
    version: &'static str,
    binary: &'static str,
}
const V0_9: Release = Release {
    tag: "v0.9.0",
    commit: "dd05c49bf3c1e8c1aa2cd707ed3f3b40ca2bc2b9",
    version: "0.9.0",
    binary: "proqi_v0_9_fixture",
};
const V0_10_2: Release = Release {
    tag: "v0.10.2",
    commit: "9ddc01f1cb2b55dc4f82c587124844a7209aceba",
    version: "0.10.2",
    binary: "proqi_v0_10_2_fixture",
};
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct HistoricalFixture {
    _root: tempfile::TempDir,
    binary: PathBuf,
    version: &'static str,
}

pub(super) struct HistoricalInstallation {
    pub(super) old_binary: PathBuf,
    pub(super) active_binary: PathBuf,
    pub(super) current_binary: PathBuf,
    pub(super) identity: InstallationIdentity,
}

impl HistoricalFixture {
    pub(super) fn build() -> Self {
        Self::build_release(&V0_9)
    }

    pub(super) fn build_recovery_release() -> Self {
        Self::build_release(&V0_10_2)
    }

    fn build_release(release: &Release) -> Self {
        let root = tempfile::Builder::new()
            .prefix("proqi-historical-source")
            .tempdir_in("/private/tmp")
            .expect("historical source root");
        verify_tag(release);
        let archive = root.path().join("historical.tar");
        let source = root.path().join("source");
        fs::create_dir(&source).expect("historical source directory");
        let mut export = Command::new("git");
        export
            .args(["archive", "--format=tar", "--output"])
            .arg(&archive)
            .arg(release.commit)
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
        if release.version == V0_9.version {
            use_cached_rustls(&source);
        }
        use_distinct_binary_name(&source, release.binary);
        let target = shared_fixture_target();
        let mut build = Command::new("cargo");
        build
            .args([
                "build",
                "--offline",
                "--locked",
                "--package",
                "proqi",
                "--bin",
                release.binary,
            ])
            .current_dir(&source)
            .env("CARGO_TARGET_DIR", &target);
        let output = run_bounded(&mut build, BUILD_TIMEOUT, "historical release build");
        assert_success(&output, "historical release build");
        let binary = target.join("debug").join(release.binary);
        assert_ne!(
            fs::read(&binary).expect("historical executable bytes"),
            fs::read(env!("CARGO_BIN_EXE_proqi")).expect("current executable bytes")
        );
        Self {
            _root: root,
            binary,
            version: release.version,
        }
    }

    pub(super) fn install(&self, state: &Path) -> HistoricalInstallation {
        let prefix = state.join("prefix");
        let old_binary = prefix
            .join("Cellar/proqi")
            .join(self.version)
            .join("bin/proqi");
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

fn use_distinct_binary_name(source: &Path, binary: &str) {
    const PACKAGE: &str = "[package]\nname = \"proqi\"";
    let manifest = source.join("Cargo.toml");
    let contents = fs::read_to_string(&manifest).expect("historical manifest");
    assert_eq!(contents.matches(PACKAGE).count(), 1);
    let mut contents =
        contents.replacen(PACKAGE, "[package]\nname = \"proqi\"\nautobins = false", 1);
    write!(
        contents,
        "\n[[bin]]\nname = \"{binary}\"\npath = \"src/bin/proqi.rs\"\n"
    )
    .expect("append fixture binary target");
    fs::write(manifest, contents).expect("write historical manifest");
}

impl HistoricalInstallation {
    pub(super) fn replace_externally(&self) {
        fs::remove_file(&self.active_binary).expect("remove historical active link");
        symlink(&self.current_binary, &self.active_binary).expect("activate current keg");
        fs::remove_file(&self.old_binary).expect("remove historical Cellar executable");
        assert!(!self.old_binary.exists());
    }
}

fn verify_tag(release: &Release) {
    let mut command = Command::new("git");
    command
        .args(["rev-list", "-n", "1", release.tag])
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    let output = run_bounded(&mut command, COMMAND_TIMEOUT, "historical tag verification");
    assert_success(&output, "historical tag verification");
    assert_eq!(
        String::from_utf8(output.stdout)
            .expect("historical commit UTF-8")
            .trim(),
        release.commit
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
