#[cfg(target_os = "macos")]
use super::{consume_first_run, expect_command, json_command};

#[cfg(target_os = "macos")]
#[test]
fn board_key_and_forwarded_primary_a_select_every_thought_in_a_real_pty() {
    for selection_input in ["a", "\x01"] {
        let state = tempfile::tempdir().expect("temporary state");
        let binary = env!("CARGO_BIN_EXE_proqi");
        consume_first_run(binary, state.path());
        let interact = format!(
            r#"
                log_user 0
                set timeout 10
                set binary $env(PROQI_TEST_BINARY)
                set state $env(PROQI_TEST_STATE)
                spawn $binary --state-dir $state
                expect -exact "\x1b\[?1049h"
                after 300
                send -- "\x1b\[200~first\x1b\[201~"
                send "\x1b"
                send -- "\x1b\[200~Grüße 👩‍💻\x1b\[201~"
                send "\x1b"
                send -- "\x1b\[200~第三\x1b\[201~"
                send "\x1b"
                after 200
                send -- "{selection_input}"
                send "d"
                after 500
                send "q"
                expect eof
                catch wait result
                exit [lindex $result 3]
            "#
        );
        let status = expect_command()
            .args(["-c", &interact])
            .env("PROQI_TEST_BINARY", binary)
            .env("PROQI_TEST_STATE", state.path())
            .status()
            .expect("run PTY select-all workflow");
        assert!(status.success());

        let sessions = json_command(binary, state.path(), &["sessions", "list"]);
        let session = sessions["data"]["sessions"][0]["id"]
            .as_str()
            .expect("session ID");
        let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
        assert_eq!(
            thoughts["data"]["thoughts"].as_array().map(Vec::len),
            Some(0)
        );
    }
}
