//! Live terminal hover attributes and passive durability through real SGR input.

use std::hash::{Hash, Hasher as _};

use super::support::{expect_command, json_command, json_input_command, wait_for_path};

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
    spawn -noecho sh -c {before=$(stty -g); "$PROQI_TEST_BINARY" --state-dir "$PROQI_TEST_STATE" -r "$PROQI_TEST_SESSION"; result=$?; after=$(stty -g); [ "$before" = "$after" ] || exit 90; exit "$result"}
    expect -exact "\x1b\[?1049h"
    after 300
    capture_pending $env(PROQI_TEST_INITIAL)
    close [open $env(PROQI_TEST_BEFORE_READY) "w"]
    while {![file exists $env(PROQI_TEST_BEFORE_ACK)]} { after 10 }
    send -- "\x1b\[<35;11;12M"
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
    after 200
    send -- "\x1b\[<35;2;3M"
    after 300
    capture_pending $env(PROQI_TEST_HEADING)
    send -- "\x1b\[<35;2;4M"
    after 300
    capture_pending $env(PROQI_TEST_ACTION)
    close [open $env(PROQI_TEST_AFTER_READY) "w"]
    while {![file exists $env(PROQI_TEST_AFTER_ACK)]} { after 10 }
    send -- "\x1b"
    after 100
    stty rows 6 columns 22
    after 200
    stty rows 18 columns 72
    after 200
    send -- "q"
    expect {
        eof {}
        timeout { exit 92 }
    }
    catch wait result
    exit [lindex $result 3]
"#;

fn content_hash(path: &std::path::Path) -> u64 {
    let bytes = std::fs::read(path).expect("read durable database");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
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
    assert!(
        !parser
            .screen()
            .cell(11, 10)
            .expect("Commands key")
            .underline()
    );
    parser.process(&footer);
    assert!(
        parser
            .screen()
            .cell(11, 10)
            .expect("hovered Commands key")
            .underline()
    );
    parser.process(&overlay);
    parser.process(&heading);
    assert!(
        !parser
            .screen()
            .cell(2, 1)
            .expect("passive Commands heading")
            .underline()
    );
    parser.process(&action);
    assert!(
        parser
            .screen()
            .cell(3, 1)
            .expect("hovered Commands action")
            .underline()
    );
}

#[test]
fn browser_footer_hover_emits_fresh_attributes_without_mutating_durable_state() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let _created = json_command(binary, state.path(), &[]);
    let database = state.path().join("data/proqi.sqlite3");
    let durable_before = content_hash(&database);
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
    let underlined_columns = (0..12)
        .filter(|column| {
            hover_screen
                .cell(23, *column)
                .is_some_and(vt100::Cell::underline)
        })
        .collect::<Vec<_>>();
    assert_eq!(initial_cell.contents(), "F");
    assert_eq!(hover_cell.contents(), "F");
    assert!(
        !initial_cell.underline(),
        "resting footer must not be underlined"
    );
    assert!(
        hover_cell.underline(),
        "hover must emit a fresh underline attribute; hover_bytes={} underlined_columns={underlined_columns:?}",
        hover_bytes.len()
    );

    let durable_after = content_hash(&database);
    assert_eq!(
        durable_after, durable_before,
        "passive hover changed durable state"
    );
    println!(
        "LIVE_BROWSER_HOVER_OK durable_hash={durable_after:016x} baseline_underlined={} hover_underlined={} click_result=rename terminal_restored=true",
        initial_cell.underline(),
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
        .env("PROQI_TEST_AFTER_ACK", &after_ack);
    let mut child = command.spawn().expect("spawn Board hover PTY workflow");
    let database = state.path().join("data/proqi.sqlite3");
    wait_for_path(&before_ready);
    let durable_before = content_hash(&database);
    std::fs::write(&before_ack, []).expect("release before-hover checkpoint");
    wait_for_path(&after_ready);
    let durable_after = content_hash(&database);
    std::fs::write(&after_ack, []).expect("release after-hover checkpoint");
    let status = child.wait().expect("wait for Board hover PTY workflow");
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
