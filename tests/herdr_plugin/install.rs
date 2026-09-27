//! The plugin build step: existing installations win, and only a checksum-matched
//! standalone installer from the latest release may run.

use std::{fmt::Write as _, fs};

use sha2::{Digest as _, Sha256};

use super::support::{Sandbox, stderr, stdout, write_tool};

const SCRIPT: &str = "herdr-plugin/install.sh";

fn release(sandbox: &Sandbox, installer: &str, checksum: Option<String>) {
    let release = sandbox.path().join("release");
    fs::create_dir_all(&release).expect("release");
    fs::write(release.join("proqi-installer.sh"), installer).expect("installer");
    let digest = hex(&Sha256::digest(installer.as_bytes()));
    let record = checksum.unwrap_or_else(|| format!("{digest}  proqi-installer.sh\n"));
    fs::write(release.join("proqi-installer.sh.sha256"), record).expect("checksum");
    let log = sandbox.log();
    write_tool(
        &sandbox.bin(),
        "curl",
        &format!(
            "out=; url=\nwhile [ $# -gt 0 ]; do case \"$1\" in --output) out=$2; shift 2 ;; *) url=$1; shift ;; esac; done\nprintf 'curl %s\\n' \"$url\" >> '{}'\n[ -f '{}'/\"${{url##*/}}\" ] || exit 22\ncp '{}'/\"${{url##*/}}\" \"$out\"\n",
            log.display(),
            release.display(),
            release.display()
        ),
    );
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut hex, byte| {
        let _infallible = write!(hex, "{byte:02x}");
        hex
    })
}

fn installer(sandbox: &Sandbox) -> String {
    format!(
        "printf 'installer %s\\n' \"$*\" >> '{}'\nmkdir -p \"$HOME/.local/bin\"\nprintf '#!/bin/sh\\n' > \"$HOME/.local/bin/proqi\"\nchmod 755 \"$HOME/.local/bin/proqi\"\n",
        sandbox.log().display()
    )
}

#[test]
fn an_existing_proqi_on_path_or_in_the_standalone_directory_is_never_replaced() {
    let on_path = Sandbox::new();
    release(&on_path, &installer(&on_path), None);
    write_tool(&on_path.bin(), "proqi", "exit 0\n");
    let output = on_path.run_script(SCRIPT, &[], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("nothing was installed"));
    assert!(
        on_path.calls().is_empty(),
        "no download: {:?}",
        on_path.calls()
    );

    let standalone = Sandbox::new();
    release(&standalone, &installer(&standalone), None);
    let custom = standalone.path().join("custom-bin");
    write_tool(&custom, "proqi", "exit 0\n");
    let output = standalone.run_script(
        SCRIPT,
        &[],
        &[("PROQI_INSTALL_DIR", custom.to_str().expect("utf-8"))],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(standalone.calls().is_empty());
}

#[test]
fn a_missing_proqi_is_installed_by_the_verified_latest_standalone_installer() {
    let sandbox = Sandbox::new();
    release(&sandbox, &installer(&sandbox), None);
    let output = sandbox.run_script(SCRIPT, &[], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    let latest = "https://github.com/oborchers/proqi/releases/latest/download";
    assert_eq!(
        sandbox.calls(),
        vec![
            format!("curl {latest}/proqi-installer.sh.sha256"),
            format!("curl {latest}/proqi-installer.sh"),
            "installer --version latest".to_owned(),
        ]
    );
    assert!(sandbox.home().join(".local/bin/proqi").is_file());
    let leftovers = fs::read_dir(sandbox.path())
        .expect("sandbox")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("proqi-herdr-plugin.")
        })
        .count();
    assert_eq!(leftovers, 0, "temporary download directory is removed");
}

#[test]
fn a_mismatched_or_malformed_checksum_never_runs_the_installer() {
    let valid = "0".repeat(64);
    for record in [
        format!("{valid}  proqi-installer.sh\n"),
        format!("{valid}  other.sh\n"),
        format!("{valid}  proqi-installer.sh\n{valid}  proqi-installer.sh\n"),
        format!("{}  proqi-installer.sh\n", "A".repeat(64)),
        "abc  proqi-installer.sh\n".to_owned(),
    ] {
        let sandbox = Sandbox::new();
        release(&sandbox, &installer(&sandbox), Some(record.clone()));
        let output = sandbox.run_script(SCRIPT, &[], &[]);
        assert!(!output.status.success(), "{record}");
        assert!(
            stderr(&output).contains("checksum"),
            "{record}: {}",
            stderr(&output)
        );
        assert!(
            !sandbox
                .calls()
                .iter()
                .any(|call| call.starts_with("installer")),
            "{record}"
        );
        assert!(!sandbox.home().join(".local/bin/proqi").exists());
    }
}

#[test]
fn an_offline_install_fails_with_guidance_and_installs_nothing() {
    let sandbox = Sandbox::new();
    release(&sandbox, &installer(&sandbox), None);
    fs::remove_dir_all(sandbox.path().join("release")).expect("go offline");
    let output = sandbox.run_script(SCRIPT, &[], &[]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("check the network"),
        "{}",
        stderr(&output)
    );
    assert!(!sandbox.home().join(".local/bin/proqi").exists());
}

#[test]
fn an_unusable_standalone_entry_fails_with_repair_guidance_and_downloads_nothing() {
    let dangling = Sandbox::new();
    release(&dangling, &installer(&dangling), None);
    let bin = dangling.home().join(".local/bin");
    fs::create_dir_all(&bin).expect("standalone dir");
    std::os::unix::fs::symlink(dangling.path().join("missing"), bin.join("proqi")).expect("link");
    let output = dangling.run_script(SCRIPT, &[], &[]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("repair or remove it"),
        "{}",
        stderr(&output)
    );
    assert!(dangling.calls().is_empty());

    let plain = Sandbox::new();
    release(&plain, &installer(&plain), None);
    let bin = plain.home().join(".local/bin");
    fs::create_dir_all(&bin).expect("standalone dir");
    fs::write(bin.join("proqi"), "not executable").expect("plain file");
    let output = plain.run_script(SCRIPT, &[], &[]);
    assert!(!output.status.success());
    assert!(plain.calls().is_empty());
}

#[test]
fn an_interrupted_install_exits_and_removes_its_download_directory() {
    // The installer signals the build step, as a terminal interrupt signals the
    // whole process group, and then returns so the deferred trap can run.
    let sandbox = Sandbox::new();
    let installer = format!(
        "printf 'installer %s\\n' \"$*\" >> '{}'\nkill -TERM $PPID\nexit 0\n",
        sandbox.log().display()
    );
    release(&sandbox, &installer, None);
    let output = sandbox.run_script(SCRIPT, &[], &[]);
    assert_eq!(output.status.code(), Some(143), "{}", stderr(&output));
    let leftovers = fs::read_dir(sandbox.path())
        .expect("sandbox")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("proqi-herdr-plugin.")
        })
        .count();
    assert_eq!(
        leftovers, 0,
        "the EXIT trap still removes the download directory"
    );
}
