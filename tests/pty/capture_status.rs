//! A live owner appends forwarded clipboard captures quietly: the active
//! editor keeps its thought and caret while the footer counts new captures.

use super::support::{
    expect_command, json_command, json_input_command, wait_for_control_owner, wait_for_path,
};

const WORKFLOW: &str = r#"
    log_user 0
    set timeout 15
    set stty_init "rows 14 columns 60"
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
    after 300
    send -- "e"
    after 200
    send -- "X"
    after 200
    capture $env(PROQI_TEST_INITIAL)
    close [open $env(PROQI_TEST_READY) w]
    while {![file exists $env(PROQI_TEST_FIRST_ADDED)]} { after 20 }
    after 200
    capture $env(PROQI_TEST_FIRST)
    close [open $env(PROQI_TEST_FIRST_SHOWN) w]
    while {![file exists $env(PROQI_TEST_SECOND_ADDED)]} { after 20 }
    after 200
    capture $env(PROQI_TEST_SECOND)
    send -- "Y"
    after 200
    send -- "\x1b"
    after 300
    send -- "q"
    expect eof
    catch wait result
    exit [lindex $result 3]
"#;

#[test]
fn forwarded_captures_append_quietly_and_count_new_captures() {
    let state = tempfile::tempdir().expect("isolated state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    json_input_command(binary, state.path(), &["thoughts", "add", session], "draft");
    let path = |name: &str| state.path().join(name);
    let fixture = path("clipboard.json");
    let stages = ["initial", "first", "second"].map(|name| path(&format!("{name}.terminal")));
    let mut owner = expect_command()
        .args(["-c", WORKFLOW])
        .env("TERM", "xterm-256color")
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_READY", path("ready"))
        .env("PROQI_TEST_FIRST_ADDED", path("first-added"))
        .env("PROQI_TEST_FIRST_SHOWN", path("first-shown"))
        .env("PROQI_TEST_SECOND_ADDED", path("second-added"))
        .env("PROQI_TEST_INITIAL", &stages[0])
        .env("PROQI_TEST_FIRST", &stages[1])
        .env("PROQI_TEST_SECOND", &stages[2])
        .spawn()
        .expect("spawn isolated active owner");
    wait_for_path(&path("ready"));
    wait_for_control_owner(state.path(), session);

    for (text, added, shown) in [
        ("  captured one\r\n", "first-added", Some("first-shown")),
        ("captured two", "second-added", None),
    ] {
        std::fs::write(
            &fixture,
            serde_json::json!({ "kind": "text", "text": text }).to_string(),
        )
        .expect("clipboard fixture");
        let output = std::process::Command::new(binary)
            .arg("--state-dir")
            .arg(state.path())
            .arg("--json")
            .args(["thoughts", "capture", session, "--from", "clipboard"])
            .env("PROQI_TEST_CLIPBOARD_FIXTURE", &fixture)
            .output()
            .expect("run capture");
        assert!(
            output.status.success(),
            "capture failed: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        std::fs::write(path(added), b"added").expect("advance owner");
        if let Some(shown) = shown {
            wait_for_path(&path(shown));
        }
    }
    let status = owner.wait().expect("owner exits");
    assert!(status.success(), "owner workflow failed: {status}");

    let screens = rendered(&stages);
    assert!(!screens[0].contains("new capture"), "{}", screens[0]);
    assert!(screens[1].contains("1 new capture"), "{}", screens[1]);
    assert!(screens[2].contains("2 new captures"), "{}", screens[2]);

    let listed = json_command(binary, state.path(), &["thoughts", "list", session]);
    let contents = listed["data"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["content"].as_str().expect("content").to_owned())
        .collect::<Vec<_>>();
    // Typing before and after the captures reached the same editor and caret,
    // and both captures were appended after it with their exact bytes.
    assert_eq!(
        contents,
        vec!["draftXY", "  captured one\r\n", "captured two"],
        "{contents:?}"
    );
}

fn rendered(stages: &[std::path::PathBuf; 3]) -> Vec<String> {
    let mut parser = vt100::Parser::new(14, 60, 0);
    parser.process(b"\x1b[?1049h");
    stages
        .iter()
        .map(|stage| {
            parser.process(&std::fs::read(stage).expect("terminal stage"));
            parser.screen().contents()
        })
        .collect()
}
