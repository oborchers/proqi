//! Live terminal hover attributes and passive durability through real SGR input.

use std::{
    hash::{Hash, Hasher as _},
    process::{Command, ExitStatus},
    thread,
    time::Duration,
};

use super::{
    support::{expect_command, json_command, json_input_command, wait_for_path},
    watchdog,
};

const WORKFLOW_LIMIT: Duration = Duration::from_secs(30);

const BROWSER_WORKFLOW: &str = r#"
    log_user 0
    set timeout 10
    set stty_init "rows 24 columns 80"
    set initial [open $env(PROQI_TEST_INITIAL) "w"]
    set hover [open $env(PROQI_TEST_HOVER) "w"]
    fconfigure $initial -translation binary -encoding binary
    fconfigure $hover -translation binary -encoding binary
    spawn -noecho sh -c {before=$(stty -g); "$PROQI_TEST_BINARY" --state-dir "$PROQI_TEST_STATE" -r; result=$?; after=$(stty -g); [ "$before" = "$after" ] || exit 90; exit "$result"}
    expect -exact "\x1b\[?1049h"
    # Allow the initial coalesced terminal frame to finish before draining it.
    after 300
    set timeout 1
    expect {
        -re {.+} {
            puts -nonewline $initial $expect_out(buffer)
            exp_continue
        }
        timeout {}
    }
    send -- "\x1b\[<35;2;24M"
    # Allow the motion-triggered redraw to finish before capturing its attributes.
    after 300
    set timeout 1
    expect {
        -re {.+} {
            puts -nonewline $hover $expect_out(buffer)
            exp_continue
        }
        timeout {}
    }
    close $initial
    close $hover
    set timeout 10
    send -- "\x1b\[<0;2;24M\x1b\[<0;2;24m"
    expect {
        -exact "Save" {}
        timeout { exit 91 }
    }
    send -- "\x1b"
    # Let the rename cancellation render before the final quit input.
    after 100
    send -- "\x1b"
    expect {
        eof {}
        timeout { exit 92 }
    }
    catch wait result
    exit [lindex $result 3]
"#;

const BOARD_WORKFLOW: &str = r#"
    log_user 0
    set timeout 10
    set stty_init "rows 12 columns 42"
    proc capture_pending {path} {
        set capture [open $path "w"]
        fconfigure $capture -translation binary -encoding binary
        set prior $::timeout
        set ::timeout 1
        expect {
            -re {.+} {
                puts -nonewline $capture $expect_out(buffer)
                exp_continue
            }
            timeout {}
        }
        set ::timeout $prior
        close $capture
    }
    proc register_watchdog_pid {pid} {
        global env
        set owned [open $env(PROQI_TEST_PIDS) a]
        puts $owned $pid
        close $owned
    }
    spawn -noecho sh -c {before=$(stty -g); "$PROQI_TEST_BINARY" --state-dir "$PROQI_TEST_STATE" -r "$PROQI_TEST_SESSION"; result=$?; after=$(stty -g); [ "$before" = "$after" ] || exit 90; exit "$result"}
    register_watchdog_pid [exp_pid]
    expect -exact "\x1b\[?1049h"
    # Allow the initial coalesced terminal frame to finish before draining it.
    after 300
    capture_pending $env(PROQI_TEST_INITIAL)
    close [open $env(PROQI_TEST_BEFORE_READY) "w"]
    set waits 0
    while {![file exists $env(PROQI_TEST_BEFORE_ACK)] && $waits < 1000} {
        after 10
        incr waits
    }
    if {![file exists $env(PROQI_TEST_BEFORE_ACK)]} { exit 93 }
    send -- "\x1b\[<35;11;12M"
    # Allow the footer hover redraw to finish before capturing its attributes.
    after 300
    capture_pending $env(PROQI_TEST_FOOTER)
    send -- "\x1b\[<0;11;12M\x1b\[<0;11;12m"
    expect {
        -exact "Relevant now" {
            set capture [open $env(PROQI_TEST_OVERLAY) "w"]
            fconfigure $capture -translation binary -encoding binary
            puts -nonewline $capture $expect_out(buffer)
            close $capture
        }
        timeout { exit 91 }
    }
    # Let Commands finish opening before targeting its current rows.
    after 200
    send -- "\x1b\[<35;2;3M"
    # Capture the passive heading only after the pointer redraw has settled.
    after 300
    capture_pending $env(PROQI_TEST_HEADING)
    send -- "\x1b\[<35;2;4M"
    # Capture the actionable row only after the pointer redraw has settled.
    after 300
    capture_pending $env(PROQI_TEST_ACTION)
    close [open $env(PROQI_TEST_AFTER_READY) "w"]
    set waits 0
    while {![file exists $env(PROQI_TEST_AFTER_ACK)] && $waits < 1000} {
        after 10
        incr waits
    }
    if {![file exists $env(PROQI_TEST_AFTER_ACK)]} { exit 94 }
    send -- "\x1b"
    # Let overlay cancellation render before exercising resize restoration.
    after 100
    stty rows 6 columns 22
    # Give each terminal resize one bounded redraw window.
    after 200
    stty rows 18 columns 72
    # Give the restored viewport the same bounded redraw window before quit.
    after 200
    send -- "q"
    expect {
        eof {}
        timeout { exit 92 }
    }
    catch wait result
    exit [lindex $result 3]
"#;

struct WatchedWorkflow {
    watcher: Option<thread::JoinHandle<ExitStatus>>,
    acknowledgements: [std::path::PathBuf; 2],
}

impl WatchedWorkflow {
    fn spawn(
        mut command: Command,
        watchdog_pids: std::path::PathBuf,
        acknowledgements: [std::path::PathBuf; 2],
    ) -> Self {
        let watcher = thread::spawn(move || {
            watchdog::status_before(
                &mut command,
                WORKFLOW_LIMIT,
                &watchdog_pids,
                "Board hover PTY workflow",
            )
        });
        Self {
            watcher: Some(watcher),
            acknowledgements,
        }
    }

    fn finish(mut self) -> ExitStatus {
        self.watcher
            .take()
            .expect("active Board hover watchdog")
            .join()
            .expect("Board hover watchdog thread")
    }

    fn release(&self, index: usize) {
        std::fs::write(&self.acknowledgements[index], []).expect("release Board hover checkpoint");
    }
}

impl Drop for WatchedWorkflow {
    fn drop(&mut self) {
        for acknowledgement in &self.acknowledgements {
            let _released = std::fs::write(acknowledgement, []);
        }
        if let Some(watcher) = self.watcher.take() {
            let _settled = watcher.join();
        }
    }
}

fn durable_content_hash(path: &std::path::Path) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    for durable_path in [path.to_path_buf(), std::path::PathBuf::from(wal)] {
        let present = durable_path.exists();
        present.hash(&mut hasher);
        if present {
            std::fs::read(&durable_path)
                .expect("read durable database content")
                .hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn assert_board_hover_attributes(paths: [&std::path::Path; 5]) {
    let [initial, footer, overlay, heading, action] = paths;
    let initial = std::fs::read(initial).expect("read Board initial frame");
    let footer = std::fs::read(footer).expect("read Board footer frame");
    let overlay = std::fs::read(overlay).expect("read Commands overlay frame");
    let heading = std::fs::read(heading).expect("read Commands heading frame");
    let action = std::fs::read(action).expect("read Commands action frame");
    let mut parser = vt100::Parser::new(12, 42, 0);
    parser.process(&initial);
    assert!(!parser.screen().cell(11, 10).expect("Commands key").bold());
    parser.process(&footer);
    let footer_cell = parser.screen().cell(11, 10).expect("hovered Commands key");
    assert!(footer_cell.bold());
    assert!(!footer_cell.underline());
    parser.process(&overlay);
    parser.process(&heading);
    let heading_cell = parser
        .screen()
        .cell(2, 1)
        .expect("passive Commands heading");
    assert!(!heading_cell.italic());
    assert!(!heading_cell.underline());
    let action_before = parser
        .screen()
        .cell(3, 1)
        .expect("resting Commands action")
        .clone();
    parser.process(&action);
    let action_cell = parser.screen().cell(3, 1).expect("hovered Commands action");
    assert!(action_cell.bold());
    assert!(!action_cell.underline());
    assert_ne!(action_cell, &action_before);
}

#[test]
fn browser_footer_hover_emits_fresh_attributes_without_mutating_durable_state() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let _created = json_command(binary, state.path(), &[]);
    let database = state.path().join("data/proqi.sqlite3");
    let durable_before = durable_content_hash(&database);
    let initial = state.path().join("browser-initial.transcript");
    let hover = state.path().join("browser-hover.transcript");

    let status = expect_command()
        .args(["-c", BROWSER_WORKFLOW])
        .env_remove("NO_COLOR")
        .env_remove("COLORTERM")
        .env("TERM", "xterm-256color")
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_INITIAL", &initial)
        .env("PROQI_TEST_HOVER", &hover)
        .status()
        .expect("run browser hover PTY workflow");
    assert!(status.success(), "browser hover PTY exited with {status}");

    let initial_bytes = std::fs::read(initial).expect("read initial terminal frame");
    let hover_bytes = std::fs::read(hover).expect("read hover terminal frame");
    let mut initial_parser = vt100::Parser::new(24, 80, 0);
    initial_parser.process(&initial_bytes);
    let initial_screen = initial_parser.screen().clone();
    let mut hover_parser = vt100::Parser::new(24, 80, 0);
    hover_parser.process(&initial_bytes);
    hover_parser.process(&hover_bytes);
    let hover_screen = hover_parser.screen();
    let initial_cell = initial_screen.cell(23, 1).expect("initial Rename key cell");
    let hover_cell = hover_screen.cell(23, 1).expect("hovered Rename key cell");
    let bold_columns = (0..12)
        .filter(|column| {
            hover_screen
                .cell(23, *column)
                .is_some_and(vt100::Cell::bold)
        })
        .collect::<Vec<_>>();
    assert_eq!(initial_cell.contents(), "F");
    assert_eq!(hover_cell.contents(), "F");
    assert!(
        !initial_cell.underline(),
        "resting footer must not be underlined"
    );
    assert!(!initial_cell.bold(), "resting footer must not be bold");
    assert!(
        hover_cell.bold(),
        "hover must emit a fresh bold attribute; hover_bytes={} bold_columns={bold_columns:?}",
        hover_bytes.len()
    );
    assert!(!hover_cell.underline(), "hover must not emit an underline");

    let durable_after = durable_content_hash(&database);
    assert_eq!(
        durable_after, durable_before,
        "passive hover changed durable state"
    );
    println!(
        "LIVE_BROWSER_HOVER_OK durable_hash={durable_after:016x} baseline_bold={} hover_bold={} hover_underlined={} click_result=rename terminal_restored=true",
        initial_cell.bold(),
        hover_cell.bold(),
        hover_cell.underline()
    );
}

#[test]
fn board_footer_and_commands_rows_emit_live_hover_attributes() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let content = format!(
        "Grüße 界 e\u{301}\tcontrol\u{7} {}",
        "large hover content ".repeat(40)
    );
    let _added = json_input_command(
        binary,
        state.path(),
        &["thoughts", "add", session],
        &content,
    );
    let initial = state.path().join("board-initial.transcript");
    let footer = state.path().join("board-footer.transcript");
    let overlay = state.path().join("commands-overlay.transcript");
    let heading = state.path().join("commands-heading.transcript");
    let action = state.path().join("commands-action.transcript");
    let before_ready = state.path().join("before-hover-ready");
    let before_ack = state.path().join("before-hover-ack");
    let after_ready = state.path().join("after-hover-ready");
    let after_ack = state.path().join("after-hover-ack");
    let watchdog_pids = state.path().join("hover-watchdog-pids");

    let mut command = expect_command();
    command
        .args(["-c", BOARD_WORKFLOW])
        .env_remove("NO_COLOR")
        .env_remove("COLORTERM")
        .env("TERM", "xterm-256color")
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_INITIAL", &initial)
        .env("PROQI_TEST_FOOTER", &footer)
        .env("PROQI_TEST_OVERLAY", &overlay)
        .env("PROQI_TEST_HEADING", &heading)
        .env("PROQI_TEST_ACTION", &action)
        .env("PROQI_TEST_BEFORE_READY", &before_ready)
        .env("PROQI_TEST_BEFORE_ACK", &before_ack)
        .env("PROQI_TEST_AFTER_READY", &after_ready)
        .env("PROQI_TEST_AFTER_ACK", &after_ack)
        .env("PROQI_TEST_PIDS", &watchdog_pids);
    let workflow = WatchedWorkflow::spawn(
        command,
        watchdog_pids,
        [before_ack.clone(), after_ack.clone()],
    );
    let database = state.path().join("data/proqi.sqlite3");
    wait_for_path(&before_ready);
    let durable_before = durable_content_hash(&database);
    workflow.release(0);
    wait_for_path(&after_ready);
    let durable_after = durable_content_hash(&database);
    workflow.release(1);
    let status = workflow.finish();
    assert!(status.success(), "Board hover PTY exited with {status}");

    assert_board_hover_attributes([&initial, &footer, &overlay, &heading, &action]);
    assert_eq!(
        durable_after, durable_before,
        "hover workflow changed durable state"
    );
    println!(
        "LIVE_BOARD_HOVER_OK durable_hash={durable_after:016x} footer=true heading=false action=true unicode_control_large=true resize=22x6_to_72x18 cancel=true terminal_restored=true"
    );
}
