//! Terminal shutdown, restoration, and accepted-work durability.

use super::{expect_command, json_command};

#[test]
fn termination_signal_restores_and_releases_the_session() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let terminate = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        set child [exp_pid]
        expect -exact "\x1b\[?1049h"
        expect -exact "\x1b\[1 q"
        system /bin/kill -TERM $child
        expect -exact "\x1b\[0 q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", terminate])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY signal workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    assert_eq!(sessions["data"]["sessions"][0]["state"], "resumable");
}

#[test]
fn hangup_uses_the_operating_system_default_and_never_leaves_a_lease() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let hangup = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        set child [exp_pid]
        expect -exact "\x1b\[?1049h"
        system /bin/kill -HUP $child
        expect eof
        catch wait result
        exit 0
    "#;
    let status = expect_command()
        .args(["-c", hangup])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY hangup workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let resumed = json_command(binary, state.path(), &["-r", session]);
    assert_eq!(resumed["data"]["session_id"], session);
}

#[test]
fn one_hundred_terminal_cycles_restore_and_leave_no_processes() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let cycles = r#"
        log_user 0
        set timeout 10
        for {set cycle 0} {$cycle < 100} {incr cycle} {
            spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
            expect -exact "\x1b\[?1049h"
            send -- "q"
            expect -exact "\x1b\[0 q"
            expect eof
            catch wait result
            if {[lindex $result 3] != 0} { exit 91 }
        }
        exit 0
    "#;
    let status = expect_command()
        .args(["-c", cycles])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run repeated PTY shutdown workflow");
    assert!(status.success());

    let pattern = format!("{binary} --state-dir {}", state.path().display());
    let remaining = std::process::Command::new("/usr/bin/pgrep")
        .args(["-f", &pattern])
        .status()
        .expect("inspect repeated PTY descendants");
    assert!(
        !remaining.success(),
        "a repeated PTY process survived shutdown"
    );
}

#[test]
fn queued_quit_waits_for_the_preceding_paste_to_become_durable() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let quit = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~accepted pending work 界\x1b\[201~"
        send -- "\x11"
        expect -exact "\x1b\[0 q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", quit])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY pending-work quit");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["thoughts"][0]["content"],
        "accepted pending work 界"
    );
}

#[test]
fn acknowledged_paste_survives_forced_process_termination() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let crash = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        set child [exp_pid]
        after 500
        send -- "\x1b\[200~committed before crash 界\x1b\[201~"
        after 800
        system /bin/kill -KILL $child
        expect eof
        catch wait result
        exit 0
    "#;
    let status = expect_command()
        .args(["-c", crash])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY crash workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    assert_eq!(sessions["data"]["sessions"][0]["state"], "recovered");
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["thoughts"][0]["content"],
        "committed before crash 界"
    );
}
