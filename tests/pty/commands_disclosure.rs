//! Commands disclosure, complete search, execution, and restoration in a real PTY.

use super::{
    support::{expect_command, json_command, json_input_command},
    watchdog,
};
use std::time::Duration;

#[test]
fn commands_search_bypasses_disclosure_and_expansion_restores_the_terminal() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let _added = json_input_command(
        binary,
        state.path(),
        &["thoughts", "add", session],
        "alpha  Grüße\t界",
    );
    let cleanup_pids = state.path().join("commands-disclosure-watchdog-pids");
    let workflow = r#"
        log_user 0
        set timeout 15
        set stty_init "rows 14 columns 60"
        spawn /bin/sh -c {before=$(stty -g); "$PROQI_TEST_BINARY" --state-dir "$PROQI_TEST_STATE" -r "$PROQI_TEST_SESSION"; result=$?; after=$(stty -g); [ "$before" = "$after" ] || exit 90; exit "$result"}
        set output [open $env(PROQI_TEST_PIDS) a]
        puts $output [exp_pid]
        close $output
        expect {
            -exact "\x1b\[?1049h" {}
            timeout { exit 91 }
        }
        after 300
        send "\x1b"
        send ":"
        expect {
            -exact "Relevant now" {}
            timeout { exit 92 }
        }
        send -- "clean up spacing"
        after 200
        send "\r"
        after 500
        send ":"
        expect {
            -exact "More commands..." {}
            timeout { exit 93 }
        }
        for {set index 0} {$index < 12} {incr index} {
            send -- "\x1b\[B"
        }
        send "\r"
        expect {
            -exact "Thought" {}
            timeout { exit 94 }
        }
        send "\x1b\x1b"
        send "q"
        expect {
            eof {}
            timeout { exit 95 }
        }
        catch wait result
        exit [lindex $result 3]
    "#;
    let mut command = expect_command();
    command
        .args(["-c", workflow])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_PIDS", &cleanup_pids)
        .env_remove("HERDR_ENV");
    let status = watchdog::status_before(
        &mut command,
        Duration::from_secs(30),
        &cleanup_pids,
        "Commands disclosure PTY workflow",
    );
    assert!(status.success(), "Commands PTY exited with {status}");

    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let thoughts = thoughts["data"]["thoughts"].as_array().expect("thoughts");
    assert_eq!(thoughts.len(), 1);
    assert_eq!(thoughts[0]["content"], "alpha Grüße 界");
}
