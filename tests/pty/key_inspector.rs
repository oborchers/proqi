//! Real terminal coverage for the keypress diagnostic.

use super::support::expect_command;

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
        "FastNavigation { direction: Previous, extend_selection: false }",
    );
}

#[test]
fn page_keys_are_forwarded_as_the_same_fast_intention_in_the_real_pty() {
    inspect_sequence(
        r"\x1b\[5~",
        "PageUp",
        "FastNavigation { direction: Previous, extend_selection: false }",
    );
    inspect_sequence(
        r"\x1b\[6~",
        "PageDown",
        "FastNavigation { direction: Next, extend_selection: false }",
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

#[test]
fn primary_enter_variants_are_distinct_in_the_real_pty() {
    let (submit, keep, modifier) = if cfg!(target_os = "macos") {
        (r"\x1b\[13;9u", r"\x1b\[13;10u", "SUPER")
    } else {
        (r"\x1b\[13;5u", r"\x1b\[13;6u", "CONTROL")
    };
    inspect_sequence(
        submit,
        &format!("Enter, modifiers: KeyModifiers({modifier})"),
        "Submit",
    );
    inspect_sequence(
        keep,
        &format!("Enter, modifiers: KeyModifiers(SHIFT | {modifier})"),
        "SubmitKeep",
    );
}

#[test]
fn macos_primary_shift_horizontal_arrows_have_exact_distinct_pty_encodings() {
    if cfg!(target_os = "macos") {
        inspect_sequence(
            r"\x1b\[1;9D",
            "Left, modifiers: KeyModifiers(SUPER)",
            "MoveVisualRow { edge: Start }",
        );
        inspect_sequence(
            r"\x1b\[1;9C",
            "Right, modifiers: KeyModifiers(SUPER)",
            "MoveVisualRow { edge: End }",
        );
        inspect_sequence(
            r"\x1b\[1;10D",
            "Left, modifiers: KeyModifiers(SHIFT | SUPER)",
            "ExtendVisualRow { edge: Start }",
        );
        inspect_sequence(
            r"\x1b\[1;10C",
            "Right, modifiers: KeyModifiers(SHIFT | SUPER)",
            "ExtendVisualRow { edge: End }",
        );
    }
}

#[test]
fn control_shift_horizontal_arrow_keeps_word_selection_in_the_real_pty() {
    inspect_sequence(
        r"\x1b\[1;6D",
        "Left, modifiers: KeyModifiers(SHIFT | CONTROL)",
        "Move { movement: WordBack, extend_selection: true }",
    );
}

#[test]
fn distinctly_shifted_primary_v_stays_available_in_the_real_pty() {
    let (sequence, modifier) = if cfg!(target_os = "macos") {
        (r"\x1b\[118;10u", "SUPER")
    } else {
        (r"\x1b\[118;6u", "CONTROL")
    };
    inspect_sequence(
        sequence,
        &format!("Char('v'), modifiers: KeyModifiers(SHIFT | {modifier})"),
        "PrimaryShiftCharacter('v')",
    );
}

#[test]
fn macos_cmd_shift_z_encoding_is_redo_in_the_real_pty() {
    inspect_sequence(
        r"\x1b\[90;10u",
        "Char('Z'), modifiers: KeyModifiers(SHIFT | SUPER)",
        "Redo",
    );
}

#[test]
fn delete_and_backspace_are_distinct_in_the_real_pty() {
    inspect_sequence(r"\x1b\[3~", "Delete", "Delete");
    inspect_sequence(
        r"\x1b\[3;2~",
        "Delete, modifiers: KeyModifiers(SHIFT)",
        "ModifiedDelete",
    );
    inspect_sequence(r"\x7f", "Backspace", "Backspace");
}
