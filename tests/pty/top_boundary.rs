//! Exact top-boundary key translation and durable insertion through a real PTY.

use super::support::{consume_first_run, expect_command, json_command};

#[test]
fn arrow_previous_and_mixed_top_creation_persist_exactly_once() {
    for keys in ["\x1b[A\x1b[A", "kk", "\x1b[Ak", "k\x1b[A"] {
        let state = tempfile::tempdir().expect("temporary state");
        let binary = env!("CARGO_BIN_EXE_proqi");
        consume_first_run(binary, state.path());
        let script = r#"
            log_user 0
            set timeout 10
            set binary $env(PROQI_TEST_BINARY)
            set state $env(PROQI_TEST_STATE)
            set keys $env(PROQI_TEST_KEYS)
            spawn $binary --state-dir $state
            expect -exact "\x1b\[?1049h"
            send -- "\x1b\[200~former first\x1b\[201~"
            after 500
            send "\x1b"
            after 100
            send -- $keys
            after 500
            send "\x1b"
            after 100
            send "q"
            expect eof
            catch wait result
            exit [lindex $result 3]
        "#;
        let status = expect_command()
            .args(["-c", script])
            .env("PROQI_TEST_BINARY", binary)
            .env("PROQI_TEST_STATE", state.path())
            .env("PROQI_TEST_KEYS", keys)
            .status()
            .expect("run top-boundary PTY workflow");
        assert!(status.success());

        let sessions = json_command(binary, state.path(), &["sessions", "list"]);
        let session = sessions["data"]["sessions"][0]["id"]
            .as_str()
            .expect("session ID");
        let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
        let thoughts = thoughts["data"]["thoughts"].as_array().expect("thoughts");
        assert_eq!(thoughts.len(), 2);
        assert_eq!(thoughts[0]["content"], "");
        assert_eq!(thoughts[1]["content"], "former first");
    }
}
