//! Mouse-capture opt-out: sequence presence, absence, and preserved command order.

use super::support::expect_command;

fn capture_transcript(config: Option<&str>) -> String {
    let state = tempfile::tempdir().expect("isolated state");
    if let Some(config) = config {
        std::fs::create_dir(state.path().join("config")).expect("config directory");
        std::fs::write(state.path().join("config/config.toml"), config).expect("config file");
    }
    let transcript = state.path().join("transcript.log");
    let script = r#"
        log_user 0
        log_file -a $env(PROQI_TEST_TRANSCRIPT)
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        after 300
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", env!("CARGO_BIN_EXE_proqi"))
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_TRANSCRIPT", &transcript)
        .status()
        .expect("run PTY mouse-capture workflow");
    assert!(status.success(), "PTY mouse-capture workflow failed");
    std::fs::read_to_string(&transcript).expect("mouse-capture transcript")
}

#[test]
fn default_mouse_capture_preserves_the_established_command_order() {
    let transcript = capture_transcript(None);
    let alternate_screen_on = transcript
        .find("\x1b[?1049h")
        .expect("enter alternate screen");
    let mouse_on = transcript
        .find("\x1b[?1000h")
        .expect("enable mouse capture");
    let bracketed_paste_on = transcript
        .find("\x1b[?2004h")
        .expect("enable bracketed paste");
    assert!(
        alternate_screen_on < mouse_on && mouse_on < bracketed_paste_on,
        "mouse capture must enable right after the alternate screen and before bracketed paste"
    );

    let bracketed_paste_off = transcript
        .rfind("\x1b[?2004l")
        .expect("disable bracketed paste");
    let mouse_off = transcript
        .rfind("\x1b[?1000l")
        .expect("disable mouse capture");
    let alternate_screen_off = transcript
        .rfind("\x1b[?1049l")
        .expect("leave alternate screen");
    assert!(
        bracketed_paste_off < mouse_off && mouse_off < alternate_screen_off,
        "mouse capture must disable after bracketed paste and before leaving the alternate screen"
    );
}

#[test]
fn mouse_capture_false_omits_every_mouse_sequence() {
    let transcript = capture_transcript(Some("mouse_capture = false\n"));
    assert!(
        !transcript.contains("\x1b[?1000h"),
        "an opted-out session must never enable mouse capture"
    );
    assert!(
        !transcript.contains("\x1b[?1000l"),
        "an opted-out session must never disable mouse capture it did not acquire"
    );
}
