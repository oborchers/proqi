//! Bounded real-PTY transport for the historical recovery lifecycle.

use std::{fs, path::Path, time::Duration};

use crate::{support::expect_command, watchdog::Workflow};

const DRIVER: &str = r#"
if {[catch {
    log_user 0
    set timeout 15
    proc mark {name value} {
        set file [open "$::env(PROQI_TEST_DRIVER)/$name" w]
        puts $file $value
        close $file
    }
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
    set owned [open $env(PROQI_TEST_PIDS) a]
    puts $owned [exp_pid]
    close $owned
    expect {
        -exact "\x1b\[?1049h" { mark started [exp_pid] }
        eof { exit 80 }
        timeout { exit 81 }
    }
    set timeout 0
    while {![file exists "$env(PROQI_TEST_DRIVER)/quit"]} {
        if {[file exists "$env(PROQI_TEST_DRIVER)/input"]} {
            set file [open "$env(PROQI_TEST_DRIVER)/input" rb]
            set input [read $file]
            close $file
            file delete "$env(PROQI_TEST_DRIVER)/input"
            send -- $input
        }
        expect {
            -re ".+" {}
            eof { exit 82 }
            timeout {}
        }
        after 10
    }
    set timeout 15
    set request [open "$env(PROQI_TEST_DRIVER)/quit" r]
    set expected [read $request]
    close $request
    if {$expected eq "quit"} {
        send "\x1b"
        after 100
        send "q"
    }
    expect {
        -exact "\x1b\[?1049l" {}
        eof { exit 83 }
        timeout { exit 84 }
    }
    if {$expected ne "quit"} {
        expect {
            -exact $expected {}
            timeout { exit 91 }
            eof { exit 92 }
        }
    }
    set guidance [expr {$expected eq "quit" ? "Resume later:" : "exact resume command:"}]
    expect {
        -exact $guidance {}
        timeout { exit 85 }
        eof { exit 86 }
    }
    expect {
        -exact $env(PROQI_TEST_SESSION) {}
        timeout { exit 87 }
        eof { exit 88 }
    }
    expect {
        eof {}
        timeout { exit 89 }
    }
    catch wait result
    mark restored [lindex $result 3]
    set expected_status [expr {$expected eq "quit" ? 0 : 1}]
    if {[lindex $result 3] != $expected_status} { exit 93 }
    exit 0
} problem]} {
    puts stderr "recovery PTY driver error: $problem"
    exit 90
}
"#;

pub(super) struct Owner {
    workflow: Workflow,
    directory: std::path::PathBuf,
}

impl Owner {
    pub(super) fn spawn(binary: &Path, state: &Path, session: &str, label: &str) -> Self {
        let directory = state.join(label);
        fs::create_dir(&directory).expect("owned driver directory");
        let pids = directory.join("pids");
        let mut command = expect_command();
        command
            .args(["-c", DRIVER])
            .env("PROQI_TEST_BINARY", binary)
            .env("PROQI_TEST_STATE", state)
            .env("PROQI_TEST_SESSION", session)
            .env("PROQI_TEST_DRIVER", &directory)
            .env("PROQI_TEST_PIDS", &pids)
            .env("PROQI_TEST_INPUT_STALL", "1")
            .env("PROQI_TEST_INPUT_ACCEPTANCE", "1")
            .env_remove("HERDR_ENV");
        Self {
            workflow: Workflow::spawn(
                command,
                Duration::from_secs(60),
                pids,
                "historical recovery lifecycle",
            ),
            directory,
        }
    }

    pub(super) fn send(&self, bytes: &[u8]) {
        assert!(
            !self.directory.join("input").exists(),
            "previous input not consumed"
        );
        let temporary = self.directory.join("input.tmp");
        fs::write(&temporary, bytes).expect("PTY input bytes");
        fs::rename(temporary, self.directory.join("input")).expect("publish PTY input");
    }

    pub(super) fn refuse_next_stall(&mut self, state: &Path) {
        self.request_exit(b"circuit_open");
        fs::write(state.join("runtime/input-stall-trigger"), b"stall").expect("third rapid stall");
        self.finish("1");
    }

    pub(super) fn stop(&mut self) {
        self.request_exit(b"quit");
        self.finish("0");
    }

    fn request_exit(&self, expectation: &[u8]) {
        let temporary = self.directory.join("quit.tmp");
        fs::write(&temporary, expectation).expect("complete exit expectation");
        fs::rename(temporary, self.directory.join("quit")).expect("publish exit expectation");
    }

    fn finish(&mut self, expected: &str) {
        let status = self.workflow.finish();
        assert!(status.success(), "PTY restoration/exit failed: {status}");
        assert_eq!(
            fs::read_to_string(self.directory.join("restored"))
                .expect("terminal restoration receipt")
                .trim(),
            expected
        );
    }
}
