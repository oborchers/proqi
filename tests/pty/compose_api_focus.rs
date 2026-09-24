//! First active-owner API addition transfers empty Compose to visible Board focus.

use super::support::{
    expect_command, json_command, json_input_command, wait_for_control_owner, wait_for_path,
};

const WORKFLOW: &str = r#"
    log_user 0
    set timeout 15
    set stty_init "rows 12 columns 48"
    proc capture {path} {
        set output [open $path "w"]
        fconfigure $output -translation binary -encoding binary
        set timeout 1
        expect {
            -re {.+} {
                puts -nonewline $output $expect_out(buffer)
                exp_continue
            }
            timeout {}
        }
        close $output
    }
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
    fconfigure $spawn_id -translation binary -encoding binary
    expect -exact "\x1b\[?1049h"
    capture $env(PROQI_TEST_INITIAL)
    close [open $env(PROQI_TEST_READY) w]
    while {![file exists $env(PROQI_TEST_ADDED)]} { after 20 }
    after 150
    capture $env(PROQI_TEST_FIRST)
    close [open $env(PROQI_TEST_FIRST_CAPTURED) w]
    while {![file exists $env(PROQI_TEST_SECOND_READY)]} { after 20 }
    after 150
    capture $env(PROQI_TEST_SECOND)
    send -- "j"
    after 150
    capture $env(PROQI_TEST_DOWN)
    send -- "k"
    after 150
    capture $env(PROQI_TEST_UP)
    send -- "\x1b\[B\x1b\[A"
    after 150
    capture $env(PROQI_TEST_ARROWS)
    send -- "q"
    expect eof
    catch wait result
    exit [lindex $result 3]
"#;

#[test]
fn api_first_add_renders_board_focus_and_navigation_without_host_click() {
    let state = tempfile::tempdir().expect("isolated state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let path = |name: &str| state.path().join(name);
    let ready = path("ready");
    let added = path("added");
    let second_ready = path("second-ready");
    let first_captured = path("first-captured");
    let stages = ["initial", "first", "second", "down", "up", "arrows"]
        .map(|name| path(&format!("{name}.terminal")));
    let mut owner = expect_command()
        .args(["-c", WORKFLOW])
        .env("TERM", "xterm-256color")
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_READY", &ready)
        .env("PROQI_TEST_ADDED", &added)
        .env("PROQI_TEST_SECOND_READY", &second_ready)
        .env("PROQI_TEST_INITIAL", &stages[0])
        .env("PROQI_TEST_FIRST", &stages[1])
        .env("PROQI_TEST_FIRST_CAPTURED", &first_captured)
        .env("PROQI_TEST_SECOND", &stages[2])
        .env("PROQI_TEST_DOWN", &stages[3])
        .env("PROQI_TEST_UP", &stages[4])
        .env("PROQI_TEST_ARROWS", &stages[5])
        .spawn()
        .expect("spawn isolated active owner");
    wait_for_path(&ready);
    wait_for_control_owner(state.path(), session);
    let first = json_input_command(
        binary,
        state.path(),
        &["thoughts", "add", session],
        "first API focus",
    );
    assert_eq!(first["data"]["receipt"]["idempotent_replay"], false);
    std::fs::write(&added, b"added").expect("advance owner");
    wait_for_path(&first_captured);
    let second = json_input_command(
        binary,
        state.path(),
        &["thoughts", "add", session],
        "second API item",
    );
    assert_eq!(second["data"]["receipt"]["idempotent_replay"], false);
    std::fs::write(&second_ready, b"second").expect("advance owner");
    let status = owner.wait().expect("owner exits");
    assert!(status.success(), "owner workflow failed: {status}");

    assert_focus_frames(&stages);
    let listed = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        listed["data"]["thoughts"]
            .as_array()
            .expect("thoughts")
            .len(),
        2
    );
}

fn assert_focus_frames(stages: &[std::path::PathBuf; 6]) {
    let mut parser = vt100::Parser::new(12, 48, 0);
    parser.process(b"\x1b[?1049h");
    let expected = [
        None,
        Some("first API focus"),
        Some("first API focus"),
        Some("second API item"),
        Some("first API focus"),
        Some("first API focus"),
    ];
    for (index, stage) in stages.iter().enumerate() {
        parser.process(&std::fs::read(stage).expect("terminal stage"));
        let screen = parser.screen();
        let contents = screen.contents();
        let rows = contents.lines().collect::<Vec<_>>();
        if let Some(content) = expected[index] {
            let row = rows
                .iter()
                .position(|line| line.contains(content))
                .expect("item is rendered");
            assert!(
                rows[row].starts_with('⋮'),
                "missing focus gutter at stage {index}: {contents}"
            );
        } else {
            assert!(screen.contents().contains("Start typing"));
        }
    }
}
