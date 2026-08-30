use std::{
    fs,
    os::unix::fs::PermissionsExt as _,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

use proqi::ports::recovery::RecoveryDocument;

use super::{consume_first_run, expect_command, json_command};

#[test]
fn exported_save_failure_accepts_the_raw_board_quit_key() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    create_empty_session(binary, state.path());
    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let database = state.path().join("data/proqi.sqlite3");
    let ready = state.path().join("immutable-ready");
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set session $env(PROQI_TEST_SESSION)
        spawn $binary --state-dir $state -r $session
        while {![file exists "$state/immutable-ready"]} {
            expect -timeout 0 {
                -re ".+" { exp_continue }
                timeout {}
                eof { exit 123 }
            }
            after 25
        }
        send -- "\x1b\[200~recovery-quit-sentinel\x1b\[201~"
        expect -re "storage I/O failed"
        send "w"
        expect -re "exporting recovery file"
        expect -re "recovery exported"
        send "q"
        expect {
            eof {}
            timeout {
                exit 124
            }
        }
        catch wait result
        exit [lindex $result 3]
    "#;
    let mut child = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", session)
        .env_remove("HERDR_ENV")
        .spawn()
        .expect("run recovery PTY workflow");
    wait_for_path(&mut child, &database);
    let immutable = ImmutableGuard::set(database);
    fs::write(ready, b"ready").expect("signal immutable database");
    let status = child.wait().expect("wait for recovery PTY workflow");
    drop(immutable);
    assert!(status.success(), "recovery PTY failed: {status}");

    let recovery = state.path().join("data/recovery");
    let paths = fs::read_dir(recovery)
        .expect("recovery directory")
        .map(|entry| entry.expect("recovery entry").path())
        .collect::<Vec<_>>();
    let [path] = paths.as_slice() else {
        panic!("expected one recovery export, found {}", paths.len());
    };
    let document: RecoveryDocument =
        serde_json::from_slice(&fs::read(path).expect("read recovery export"))
            .expect("decode recovery export");
    assert!(
        document
            .thoughts
            .iter()
            .any(|thought| thought.content == "recovery-quit-sentinel")
    );
    assert_eq!(
        fs::metadata(path)
            .expect("recovery metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

struct ImmutableGuard(std::path::PathBuf);

impl ImmutableGuard {
    fn set(path: std::path::PathBuf) -> Self {
        let status = Command::new("chflags")
            .args(["uchg", path.to_str().expect("UTF-8 database path")])
            .status()
            .expect("set immutable database");
        assert!(status.success());
        Self(path)
    }
}

impl Drop for ImmutableGuard {
    fn drop(&mut self) {
        let _status = Command::new("chflags").arg("nouchg").arg(&self.0).status();
    }
}

fn wait_for_path(child: &mut Child, path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() && Instant::now() < deadline {
        assert_eq!(child.try_wait().expect("inspect PTY child"), None);
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        path.exists(),
        "database was not created before the deadline"
    );
    thread::sleep(Duration::from_millis(300));
}

fn create_empty_session(binary: &str, state: &std::path::Path) {
    let script = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env_remove("HERDR_ENV")
        .status()
        .expect("create empty session");
    assert!(status.success());
}
