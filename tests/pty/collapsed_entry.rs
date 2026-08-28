use std::{
    io::Write,
    process::{Command, Stdio},
};

use super::{expect_command, json_command};

const COLLAPSED_ENTRY_WORKFLOW: &str = r#"
    log_user 0
    set timeout 15
    set binary $env(PROQI_TEST_BINARY)
    set state $env(PROQI_TEST_STATE)
    set session $env(PROQI_TEST_SESSION)
    spawn $binary --state-dir $state -r $session
    expect -exact "\x1b\[?1049h"
    stty rows 18 columns 50
    after 400
    send "j"
    after 200
    for {set i 0} {$i < 40} {incr i} {
        send -- "\x1b\[<64;8;6M"
    }
    send -- "\x1b\[<0;3;5M\x1b\[<0;3;5m"
    after 200
    send "\x1b"
    after 150
    send "\r"
    after 150
    send "!"
    for {set i 0} {$i < 20} {incr i} {
        send -- "\x1b\[<65;8;8M"
    }
    for {set i 0} {$i < 20} {incr i} {
        send -- "\x1b\[<64;8;8M"
    }
    stty rows 10 columns 32
    after 100
    stty rows 26 columns 80
    after 100
    stty rows 12 columns 44
    after 300
    send "\x1b"
    after 200
    send "c"
    after 150
    for {set i 0} {$i < 40} {incr i} {
        send -- "\x1b\[<64;8;6M"
    }
    send -- "\x1b\[<0;1;5M\x1b\[<0;1;5m"
    after 150
    send "c"
    after 150
    for {set i 0} {$i < 40} {incr i} {
        send -- "\x1b\[<64;8;6M"
    }
    send -- "\x1b\[<0;20;6M\x1b\[<0;20;6m"
    after 150
    send "c"
    after 100
    send "c"
    for {set i 0} {$i < 80} {incr i} {
        send -- "\x1b\[<65;8;6M"
    }
    for {set i 0} {$i < 80} {incr i} {
        send -- "\x1b\[<64;8;6M"
    }
    after 500
    send "q"
    expect eof
    catch wait result
    exit [lindex $result 3]
"#;

#[test]
fn collapsed_long_thought_mouse_entry_survives_cycles_scroll_and_resize() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"]
        .as_str()
        .expect("session ID")
        .to_owned();
    let long = (0..=10)
        .map(|line| {
            if line == 0 {
                "A界B long thought start".to_owned()
            } else {
                format!("line {line} wraps with Grüße 界, emoji 🧪, and enough ordinary words")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    add_thought(binary, state.path(), &session, "ordinary before");
    add_thought(binary, state.path(), &session, &long);
    add_thought(binary, state.path(), &session, "ordinary after");
    let status = expect_command()
        .args(["-c", COLLAPSED_ENTRY_WORKFLOW])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", &session)
        .status()
        .expect("run collapsed-thought mouse PTY workflow");
    assert!(status.success());

    let thoughts = json_command(binary, state.path(), &["thoughts", "list", &session]);
    let thoughts = thoughts["data"]["thoughts"].as_array().expect("thoughts");
    assert_eq!(thoughts.len(), 3, "{thoughts:#?}");
    assert_eq!(thoughts[0]["content"], "ordinary before");
    assert_eq!(thoughts[1]["content"], format!("{long}!"));
    assert_eq!(thoughts[2]["content"], "ordinary after");
}

fn add_thought(binary: &str, state: &std::path::Path, session: &str, content: &str) {
    let mut child = Command::new(binary)
        .arg("--state-dir")
        .arg(state)
        .arg("--json")
        .args(["thoughts", "add", session])
        .env_remove("HERDR_ENV")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn thought seed command");
    child
        .stdin
        .take()
        .expect("thought seed stdin")
        .write_all(content.as_bytes())
        .expect("write thought seed");
    assert!(child.wait().expect("wait for thought seed").success());
}
