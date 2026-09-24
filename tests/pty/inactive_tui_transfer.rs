//! Focused and selected Commands transfer into an inactive session through real terminal input.

use std::time::Duration;

use super::{
    support::{consume_first_run, expect_command, json_command, json_input_command},
    watchdog,
};

const WORKFLOW: &str = r#"
    log_user 0
    set timeout 15
    set stty_init "rows 14 columns 72"
    spawn /bin/sh -c {before=$(stty -g); "$PROQI_TEST_BINARY" --state-dir "$PROQI_TEST_STATE" -r "$PROQI_TEST_SOURCE"; result=$?; after=$(stty -g); [ "$before" = "$after" ] || exit 90; exit "$result"}
    set output [open $env(PROQI_TEST_PIDS) a]
    puts $output [exp_pid]
    close $output
    expect {
        -exact "\x1b\[?1049h" {}
        timeout { exit 91 }
    }
    if {$env(PROQI_TEST_SELECT_ALL) eq "1"} {
        expect {
            -re {2 thoughts.*board} {}
            timeout { exit 92 }
        }
        send -- $env(PROQI_TEST_PRIMARY_A)
    } else {
        after 200
    }
    send ":"
    expect {
        -exact "Relevant now" {}
        timeout { exit 92 }
    }
    send -- "\x1b\[200~and remove thought\x1b\[201~"
    after 200
    send "\r"
    expect {
        -exact "Transfer target" {}
        timeout { exit 93 }
    }
    send "\r"
    if {$env(PROQI_TEST_SELECT_ALL) eq "1"} {
        expect {
            -re {0 thoughts} {}
            timeout {
                puts stderr "source TUI after transfer: $expect_out(buffer)"
                exit 94
            }
        }
    } else {
        expect {
            -re {thought sent; removing the source} {}
            timeout { exit 94 }
        }
    }
    send -- $env(PROQI_TEST_PRIMARY_Q)
    expect {
        -exact "\x1b\[?1049l" {}
        timeout { exit 95 }
    }
    expect {
        eof {}
        timeout { exit 96 }
    }
    catch wait result
    exit [lindex $result 3]
"#;

#[test]
fn focused_commands_send_and_remove_into_inactive_session_is_durable() {
    run_transfer(&["Focused Grüße 界\nsource"], false);
}

#[test]
fn selected_commands_send_and_remove_into_inactive_session_is_atomic() {
    run_transfer(&["Beta Grüße", "Delta 界"], true);
}

fn run_transfer(contents: &[&str], select_all: bool) {
    let state = tempfile::tempdir().expect("isolated transfer state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let destination = json_command(binary, state.path(), &[])["data"]["session_id"]
        .as_str()
        .expect("destination ID")
        .to_owned();
    json_command(
        binary,
        state.path(),
        &["sessions", "rename", &destination, "Transfer target"],
    );
    let source = json_command(binary, state.path(), &[])["data"]["session_id"]
        .as_str()
        .expect("source ID")
        .to_owned();
    for content in contents {
        json_input_command(binary, state.path(), &["thoughts", "add", &source], content);
    }
    let cleanup_pids = state.path().join("inactive-tui-transfer-watchdog-pids");
    let mut command = expect_command();
    command
        .args(["-c", WORKFLOW])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SOURCE", &source)
        .env("PROQI_TEST_SELECT_ALL", if select_all { "1" } else { "0" })
        .env("PROQI_TEST_PIDS", &cleanup_pids)
        .env_remove("HERDR_ENV");
    let status = watchdog::status_before(
        &mut command,
        Duration::from_secs(50),
        &cleanup_pids,
        "inactive Commands transfer PTY workflow",
    );
    assert!(
        status.success(),
        "Commands transfer PTY exited with {status}"
    );
    assert_thoughts(binary, state.path(), &source, &destination, contents);
}

fn assert_thoughts(
    binary: &str,
    state: &std::path::Path,
    source: &str,
    destination: &str,
    contents: &[&str],
) {
    let source_thoughts = json_command(binary, state, &["thoughts", "list", source]);
    assert!(
        source_thoughts["data"]["thoughts"]
            .as_array()
            .expect("source thoughts")
            .is_empty()
    );
    let destination_thoughts = json_command(binary, state, &["thoughts", "list", destination]);
    let destination_thoughts = destination_thoughts["data"]["thoughts"]
        .as_array()
        .expect("destination thoughts");
    assert_eq!(
        destination_thoughts
            .iter()
            .map(|thought| thought["content"].as_str().expect("content"))
            .collect::<Vec<_>>(),
        contents
    );
}
