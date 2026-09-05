//! Real terminal coverage for the keypress diagnostic.

use super::support::expect_command;

fn inspect_sequence(sequence: &str, raw_code: &str, action: &str) {
    let script = format!(
        r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        spawn $binary diagnostics keypress
        expect {{
            -exact "\x1b\[>5u" {{}}
            -exact "\x1b\[>7u" {{}}
        }}
        expect {{
            -exact "Press one key to inspect its terminal event and Proqi action." {{}}
            timeout {{exit 91}}
            eof {{exit 92}}
        }}
        send -- "{sequence}"
        expect {{
            -exact "\x1b\[<u" {{}}
            timeout {{exit 93}}
            eof {{exit 94}}
        }}
        expect {{
            -exact "Raw event: KeyEvent \{{ code: {raw_code}" {{}}
            timeout {{exit 95}}
            eof {{exit 96}}
        }}
        expect {{
            -exact "Matched action: {action}" {{}}
            timeout {{exit 97}}
            eof {{exit 98}}
        }}
        catch {{expect eof}}
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
        expect {{
            -exact "\x1b\[>5u" {{}}
            -exact "\x1b\[>7u" {{}}
        }}
        expect {
            -exact "Press one key to inspect its terminal event and Proqi action." {}
            timeout {exit 91}
            eof {exit 92}
        }
        send -- "a"
        expect {
            -exact "\x1b\[<u" {}
            timeout {exit 93}
            eof {exit 94}
        }
        expect {
            -exact "Raw event: KeyEvent \{ code: Char('a')" {}
            timeout {exit 95}
            eof {exit 96}
        }
        expect {
            -exact "Matched action: selection.select_all" {}
            timeout {exit 97}
            eof {exit 98}
        }
        catch {expect eof}
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
        "list.previous",
    );
}

#[test]
fn page_keys_keep_their_fast_navigation_identity_in_the_inspector_board_context() {
    inspect_sequence(r"\x1b\[5~", "PageUp", "navigation.fast_previous");
    inspect_sequence(r"\x1b\[6~", "PageDown", "navigation.fast_next");
}

#[test]
fn platform_primary_arrow_is_forwarded_through_the_real_pty_and_restores() {
    if cfg!(target_os = "macos") {
        inspect_sequence(
            r"\x1b\[1;9B",
            "Down, modifiers: KeyModifiers(SUPER)",
            "list.next",
        );
    } else {
        inspect_sequence(
            r"\x1b\[1;5B",
            "Down, modifiers: KeyModifiers(CONTROL)",
            "list.next",
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
        "submission.submit_remove",
    );
    inspect_sequence(
        keep,
        &format!("Enter, modifiers: KeyModifiers(SHIFT | {modifier})"),
        "submission.submit_keep",
    );
}

#[test]
fn macos_super_meta_and_raw_control_remain_distinct_in_the_real_pty() {
    if cfg!(target_os = "macos") {
        inspect_sequence(
            r"\x1b\[97;9u",
            "Char('a'), modifiers: KeyModifiers(SUPER)",
            "selection.select_all",
        );
        inspect_sequence(
            r"\x1b\[97;33u",
            "Char('a'), modifiers: KeyModifiers(META)",
            "selection.select_all",
        );
        inspect_sequence(
            r"\x1b\[97;5u",
            "Char('a'), modifiers: KeyModifiers(CONTROL)",
            "text.input_or_unbound",
        );
    }
}

#[test]
fn kitty_repeat_event_is_dispatched_but_release_is_not() {
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        spawn $binary diagnostics keypress
        expect {
            -exact "\x1b\[>5u" {}
            -exact "\x1b\[>7u" {}
        }
        expect {
            -exact "Press one key to inspect its terminal event and Proqi action." {}
            timeout {exit 91}
            eof {exit 92}
        }
        send -- "\x1b\[106;1:3u"
        after 50
        send -- "\x1b\[106;1:2u"
        expect {
            -exact "\x1b\[<u" {}
            timeout {exit 93}
            eof {exit 94}
        }
        expect {
            -exact "Raw event: KeyEvent \{ code: Char('j')" {}
            timeout {exit 95}
            eof {exit 96}
        }
        expect {
            -exact "kind: Repeat" {}
            timeout {exit 97}
            eof {exit 98}
        }
        expect {
            -exact "Matched action: list.next" {}
            timeout {exit 99}
            eof {exit 100}
        }
        catch {expect eof}
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", env!("CARGO_BIN_EXE_proqi"))
        .status()
        .expect("run repeat and release keypress diagnostic in PTY");
    assert!(status.success(), "keypress repeat PTY exited with {status}");
}

#[test]
fn macos_primary_shift_horizontal_arrows_have_exact_distinct_pty_encodings() {
    if cfg!(target_os = "macos") {
        inspect_sequence(
            r"\x1b\[1;9D",
            "Left, modifiers: KeyModifiers(SUPER)",
            "text.input_or_unbound",
        );
        inspect_sequence(
            r"\x1b\[1;9C",
            "Right, modifiers: KeyModifiers(SUPER)",
            "text.input_or_unbound",
        );
        inspect_sequence(
            r"\x1b\[1;10D",
            "Left, modifiers: KeyModifiers(SHIFT | SUPER)",
            "text.input_or_unbound",
        );
        inspect_sequence(
            r"\x1b\[1;10C",
            "Right, modifiers: KeyModifiers(SHIFT | SUPER)",
            "text.input_or_unbound",
        );
    }
}

#[test]
fn control_shift_horizontal_arrow_uses_platform_word_or_base_selection() {
    inspect_sequence(
        r"\x1b\[1;6D",
        "Left, modifiers: KeyModifiers(SHIFT | CONTROL)",
        "text.input_or_unbound",
    );
}

#[test]
fn macos_raw_control_v_is_not_a_second_primary_paste_chord() {
    if cfg!(target_os = "macos") {
        inspect_sequence(
            r"\x16",
            "Char('v'), modifiers: KeyModifiers(CONTROL)",
            "text.input_or_unbound",
        );
        inspect_sequence(
            r"\x1b\[118;5u",
            "Char('v'), modifiers: KeyModifiers(CONTROL)",
            "text.input_or_unbound",
        );
        inspect_sequence(
            r"\x1b\[118;6u",
            "Char('v'), modifiers: KeyModifiers(SHIFT | CONTROL)",
            "text.input_or_unbound",
        );
    }
}

#[test]
fn distinctly_shifted_primary_v_is_reflow_in_the_real_pty() {
    let (sequence, modifier) = if cfg!(target_os = "macos") {
        (r"\x1b\[118;10u", "SUPER")
    } else {
        (r"\x1b\[118;6u", "CONTROL")
    };
    inspect_sequence(
        sequence,
        &format!("Char('v'), modifiers: KeyModifiers(SHIFT | {modifier})"),
        "clipboard.paste_reflow",
    );
}

#[test]
fn macos_cmd_shift_z_encoding_is_redo_in_the_real_pty() {
    inspect_sequence(
        r"\x1b\[90;10u",
        "Char('Z'), modifiers: KeyModifiers(SHIFT | SUPER)",
        "history.redo",
    );
}

#[test]
fn delete_and_backspace_are_distinct_in_the_real_pty() {
    inspect_sequence(r"\x1b\[3~", "Delete", "thought.delete");
    inspect_sequence(
        r"\x1b\[3;2~",
        "Delete, modifiers: KeyModifiers(SHIFT)",
        "text.input_or_unbound",
    );
    inspect_sequence(r"\x7f", "Backspace", "text.input_or_unbound");
}
