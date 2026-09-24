//! Board-only footer toggling over real macOS terminal bytes.

use super::support::{expect_command, json_command, json_input_command};

#[test]
fn macos_plain_h_hides_optional_footer_without_persisting_or_breaking_restoration() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"]
        .as_str()
        .expect("session ID")
        .to_owned();
    let content = "footer PTY seed";
    let _added = json_input_command(
        binary,
        state.path(),
        &["thoughts", "add", &session],
        content,
    );
    let transcript = state.path().join("footer.transcript");
    let script = r#"
        log_user 0
        set timeout 10
        set stty_init "rows 12 columns 42"
        set capture [open $env(PROQI_TEST_TRANSCRIPT) "w"]
        fconfigure $capture -translation binary -encoding binary
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        puts -nonewline $capture "\x1b\[?1049h"
        after 400
        set timeout 1
        expect {
            -re {.+} {
                puts -nonewline $capture $expect_out(buffer)
                exp_continue
            }
            timeout {}
        }
        set timeout 10
        send -- "h"
        after 250
        set timeout 1
        expect {
            -re {.+} {
                puts -nonewline $capture $expect_out(buffer)
                exp_continue
            }
            timeout {}
        }
        close $capture
        set timeout 10
        send "q"
        expect -exact "\x1b\[?1049l"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", &session)
        .env("PROQI_TEST_TRANSCRIPT", &transcript)
        .status()
        .expect("run footer-visibility PTY workflow");
    assert!(
        status.success(),
        "footer-visibility PTY exited with {status}"
    );

    let bytes = std::fs::read(transcript).expect("read footer transcript");
    let wire = String::from_utf8_lossy(&bytes);
    assert!(
        wire.contains("1 thought"),
        "visible footer was not rendered"
    );
    let mut parser = vt100::Parser::new(12, 42, 0);
    parser.process(&bytes);
    assert!(
        !parser.screen().contents().contains("1 thought"),
        "hidden footer remained on the final terminal screen: {:?}",
        parser.screen().contents()
    );
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", &session]);
    assert_eq!(thoughts["data"]["items"][0]["content"], content);
}
