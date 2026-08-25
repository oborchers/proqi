//! Real terminal coverage for the keypress diagnostic.

use super::expect_command;

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
