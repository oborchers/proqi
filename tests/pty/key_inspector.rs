//! Real terminal coverage for the keypress diagnostic.

use super::expect_command;

fn inspect_sequence(sequence: &str, raw_code: &str, action: &str) {
    let script = format!(
        r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        spawn $binary diagnostics keypress
        expect -exact "Press one key to inspect its terminal event and Proqi action."
        send -- "{sequence}"
        expect -exact "Raw event: KeyEvent {{ code: {raw_code}"
        expect -exact "Matched action: {action}"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#
    );
    let status = expect_command()
        .args(["-c", &script])
        .env("PROQI_TEST_BINARY", env!("CARGO_BIN_EXE_proqi"))
        .status()
        .expect("run modified keypress diagnostic in PTY");
    assert!(status.success(), "keypress PTY exited with {status}");
}

#[test]
fn keypress_inspector_reports_raw_and_normalized_input_then_restores() {
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        spawn $binary diagnostics keypress
        expect -exact "Press one key to inspect its terminal event and Proqi action."
        send -- "a"
        expect -exact "Raw event: KeyEvent { code: Char('a')"
        expect -exact "Matched action: Character('a')"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", env!("CARGO_BIN_EXE_proqi"))
        .status()
        .expect("run keypress diagnostic in PTY");
    assert!(status.success(), "keypress PTY exited with {status}");
}

#[test]
fn alt_arrow_is_forwarded_through_the_real_pty_and_crossterm_parser() {
    inspect_sequence(
        r"\x1b\[1;3A",
        "Up, modifiers: KeyModifiers(ALT)",
        "EditNavigation { editor_movement: VisualJumpUp, board_movement: VisualUp }",
    );
}

#[test]
fn platform_primary_arrow_is_forwarded_through_the_real_pty_and_restores() {
    if cfg!(target_os = "macos") {
        inspect_sequence(
            r"\x1b\[1;9B",
            "Down, modifiers: KeyModifiers(SUPER)",
            "EditNavigation { editor_movement: DocumentEnd, board_movement: DocumentEnd }",
        );
    } else {
        inspect_sequence(
            r"\x1b\[1;5B",
            "Down, modifiers: KeyModifiers(CONTROL)",
            "EditNavigation { editor_movement: DocumentEnd, board_movement: VisualDown }",
        );
    }
}
