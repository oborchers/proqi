//! Bounded real-PTY fixtures for release-highlight restart behavior.

use std::{
    path::Path,
    process::{Child, ExitStatus},
};

use super::super::support::expect_command;

pub(super) fn spawn_crash_owner(binary: &Path, state: &Path, session: &str, ready: &Path) -> Child {
    let script = r#"
        log_user 0
        set timeout 20
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        stty rows 18 columns 84
        close [open $env(PROQI_TEST_READY) w]
        expect -exact "\x1b\[?1049l"
        expect -exact "\x1b\[?1049h"
        stty rows 19 columns 85
        after 500
        exec kill -KILL [exp_pid]
        expect {
            eof {}
            timeout { exit 95 }
        }
        catch wait result
        exit 0
    "#;
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_READY", ready)
        .spawn()
        .expect("spawn highlight crash owner")
}

pub(super) fn run_dismissal(binary: &Path, state: &Path, session: &str) -> ExitStatus {
    let script = r#"
        log_user 0
        set timeout 20
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        stty rows 18 columns 84
        after 300
        send -- "\x1b"
        after 500
        send -- "\x11"
        expect {
            eof {}
            timeout { exit 95 }
        }
        catch wait result
        exit [lindex $result 3]
    "#;
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .status()
        .expect("run highlight dismissal")
}

pub(super) fn run_quiet_resume(binary: &Path, state: &Path, session: &str) -> ExitStatus {
    let script = r#"
        log_user 0
        set timeout 20
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        stty rows 18 columns 84
        after 300
        send -- "\x11"
        expect {
            eof {}
            timeout { exit 95 }
        }
        catch wait result
        exit [lindex $result 3]
    "#;
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .status()
        .expect("run quiet highlight resume")
}

pub(super) fn run_manual_reopen(binary: &Path, state: &Path, session: &str) -> ExitStatus {
    let script = r#"
        log_user 0
        set timeout 20
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        stty rows 18 columns 84
        after 300
        send -- "\x1b"
        after 100
        send -- ":What's new\r"
        after 300
        send -- "q"
        set timeout 1
        expect {
            eof { exit 94 }
            timeout {}
        }
        set timeout 20
        send -- "\x1b"
        after 200
        send -- "\x11"
        expect {
            eof {}
            timeout { exit 95 }
        }
        catch wait result
        exit [lindex $result 3]
    "#;
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .status()
        .expect("run manual highlight reopen")
}
