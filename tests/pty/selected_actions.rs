//! Real modified input, resize, durable order, and terminal restoration for selected runs.

use super::support::{consume_first_run, expect_command, json_command};

const WORKFLOW: &str = r#"
    log_user 0
    set timeout 12
    set binary $env(PROQI_TEST_BINARY)
    set state $env(PROQI_TEST_STATE)
    set stty_init "rows 8 columns 36"
    spawn $binary --state-dir $state
    expect -exact "\x1b\[?1049h"
    after 200
    foreach thought {first second third fourth fifth} {
        send -- "\x1b\[200~$thought\x1b\[201~"
        after 100
        send "\x1b"
        after 60
    }
    send " "
    send "kk"
    send " "
    stty rows 4 columns 20
    after 100
    send -- $env(PROQI_TEST_REORDER_BYTES)
    after 300
    send "q"
    expect -exact "\x1b\[?1049l"
    expect eof
    catch wait result
    exit [lindex $result 3]
"#;

fn selected_order(reorder_bytes: &str) -> Vec<String> {
    let state = tempfile::tempdir().expect("isolated state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let status = expect_command()
        .args(["-c", WORKFLOW])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_REORDER_BYTES", reorder_bytes)
        .env_remove("HERDR_ENV")
        .status()
        .expect("selected reorder PTY");
    assert!(status.success(), "PTY exited with {status}");
    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session");
    json_command(binary, state.path(), &["thoughts", "list", session])["data"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter(|item| item["kind"] == "thought")
        .map(|thought| thought["content"].as_str().expect("content").to_owned())
        .collect()
}

#[test]
fn disjoint_selected_reorder_arrow_and_k_bytes_share_durable_order() {
    let expected = ["first", "third", "second", "fifth", "fourth"];
    assert_eq!(selected_order("\x1b[1;10A"), expected);
    assert_eq!(selected_order("\x1b[107;10u"), expected);
}
