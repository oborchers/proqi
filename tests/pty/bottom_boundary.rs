//! Bottom-boundary keyboard creation, durable order, history, resize, and restart.

use super::support::{expect_command, json_command, json_input_command};

const TOP_SENTINEL: &str = "top boundary sentinel";
const BOTTOM_SENTINEL: &str = "bottom boundary Grüße 界";
const BOTTOM_WORKFLOW: &str = r#"
    log_user 0
    set timeout 15
    set binary $env(PROQI_TEST_BINARY)
    set state $env(PROQI_TEST_STATE)
    set session $env(PROQI_TEST_SESSION)
    set stty_init "rows 10 columns 34"
    spawn $binary --state-dir $state -r $session
    expect -exact "\x1b\[?1049h"
    after 300
    send "c"
    send -- "\x1b\[A\x1b\[A"
    after 300
    send -- "top boundary sentinel"
    send "\x1b"
    after 200
    stty rows 7 columns 24
    after 100
    stty rows 18 columns 72
    after 100
    stty rows 9 columns 30
    after 200
    for {set i 0} {$i < 11} {incr i} {
        if {$i % 2 == 0} {
            send -- "\x1b\[B"
        } else {
            send "j"
        }
    }
    send -- "\x1b\[B"
    send "j"
    after 300
    send -- "bottom boundary Grüße 界"
    after 400
    send "\x1b"
    after 150
    send "u"
    after 300
    send -- "\x1b\[122:90;10u"
    after 500
    send "\x1b"
    after 150
    send "q"
    expect eof
    catch wait result
    exit [lindex $result 3]
"#;
const RESTART_WORKFLOW: &str = r#"
    log_user 0
    set timeout 10
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
    expect -exact "\x1b\[?1049h"
    after 300
    send "q"
    expect eof
    catch wait result
    exit [lindex $result 3]
"#;

#[test]
fn scrolled_bottom_confirmation_appends_after_prior_top_insertion() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"]
        .as_str()
        .expect("session ID")
        .to_owned();
    seed_long_board(binary, state.path(), &session);
    run_workflow(binary, state.path(), &session, BOTTOM_WORKFLOW);
    assert_durable_order(binary, state.path(), &session);
    run_workflow(binary, state.path(), &session, RESTART_WORKFLOW);
    assert_durable_order(binary, state.path(), &session);
}

fn seed_long_board(binary: &str, state: &std::path::Path, session: &str) {
    for index in 0..10 {
        let content = format!(
            "seed {index}: Grüße 界 👩‍💻\tcontrol\u{7}\nsecond line that wraps in the narrow PTY"
        );
        let _added = json_input_command(binary, state, &["thoughts", "add", session], &content);
    }
}

fn run_workflow(binary: &str, state: &std::path::Path, session: &str, script: &str) {
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env_remove("HERDR_ENV")
        .status()
        .expect("run bottom-boundary PTY process");
    assert!(status.success(), "bottom-boundary PTY failed: {status}");
}

fn assert_durable_order(binary: &str, state: &std::path::Path, session: &str) {
    let listed = json_command(binary, state, &["thoughts", "list", session]);
    let thoughts = listed["data"]["thoughts"].as_array().expect("thoughts");
    assert_eq!(thoughts.len(), 12);
    assert_eq!(thoughts.first().expect("first")["content"], TOP_SENTINEL);
    assert_eq!(thoughts.last().expect("last")["content"], BOTTOM_SENTINEL);
    for (position, thought) in thoughts.iter().enumerate() {
        assert_eq!(thought["position"], position);
    }
}
