//! Exact CSI-u decoding, contextual diagnostics and terminal restoration in a real PTY.

use super::support::expect_command;
use serde_json::{Value, json};

fn capture(sequence: &str, context: &str, config: Option<&str>, defaults: bool) -> Value {
    let state = tempfile::tempdir().expect("isolated state");
    if let Some(config) = config {
        std::fs::create_dir(state.path().join("config")).unwrap();
        std::fs::write(state.path().join("config/config.toml"), config).unwrap();
    }
    let script = r#"
        log_user 0
        set timeout 10
        spawn /bin/sh -c {before=$(stty -g); "$PROQI_TEST_BINARY" --json --state-dir "$PROQI_TEST_STATE" diagnostics keypress --context "$PROQI_TEST_CONTEXT" --timeout-ms "$PROQI_TEST_TIMEOUT" $PROQI_TEST_DEFAULTS; result=$?; after=$(stty -g); [ "$before" = "$after" ] || exit 90; exit "$result"}
        expect {
            -exact "Press one key to inspect" {}
            timeout {exit 91}
            eof {exit 92}
        }
        if {$env(PROQI_TEST_SEQUENCE) eq "__SIGTERM__"} {
            set children [exec /usr/bin/pgrep -P [exp_pid]]
            if {[llength $children] != 1} {exit 95}
            exec /bin/kill -TERM [lindex $children 0]
        } elseif {$env(PROQI_TEST_SEQUENCE) ne ""} {send -- $env(PROQI_TEST_SEQUENCE)}
        expect {
            eof {puts $expect_out(buffer)}
            timeout {exit 93}
        }
        catch wait result
        exit [lindex $result 3]
    "#;
    let output = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", env!("CARGO_BIN_EXE_proqi"))
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_CONTEXT", context)
        .env("PROQI_TEST_SEQUENCE", sequence)
        .env(
            "PROQI_TEST_TIMEOUT",
            if sequence.is_empty() { "100" } else { "3000" },
        )
        .env(
            "PROQI_TEST_DEFAULTS",
            if defaults { "--defaults" } else { "" },
        )
        .output()
        .expect("bounded PTY diagnostic");
    assert!(
        output.status.success(),
        "PTY diagnostic/restoration failed: {} {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let transcript = String::from_utf8(output.stdout).expect("UTF-8 diagnostic");
    assert!(
        transcript.contains("\x1b[<u"),
        "keyboard reporting restored"
    );
    let start = transcript.find("{\"").expect("JSON envelope");
    let line = transcript[start..].lines().next().unwrap();
    let envelope: Value = serde_json::from_str(line).expect("valid JSON diagnostic");
    envelope["data"].clone()
}

fn inspect(sequence: &str, key: &str, modifiers: &[&str], action: Option<&str>) {
    let data = capture(sequence, "board,edit", None, true);
    assert_eq!(data["status"], "event_received");
    let event = &data["event"];
    assert_eq!(event["keystroke"]["key"], key);
    assert_eq!(event["keystroke"]["modifiers"], json!(modifiers));
    assert_eq!(event["keystroke"]["phase"], "press");
    assert_eq!(event["action"].as_str(), action);
    assert_eq!(event["active_context"], "edit");
    assert_eq!(event["context_stack"], json!(["board", "edit"]));
}

#[test]
fn csi_u_super_meta_raw_control_shift_and_smart_paste_are_exact() {
    for (bytes, key, modifiers, action) in [
        (
            "\x1b[97;9u",
            "U+0061",
            vec!["Super"],
            Some("selection.select_all"),
        ),
        (
            "\x1b[97;33u",
            "U+0061",
            vec!["Meta"],
            Some("selection.select_all"),
        ),
        ("\x1b[97;5u", "U+0061", vec!["Control"], None),
        ("\x16", "U+0076", vec!["Control"], None),
        ("\x1b[118;5u", "U+0076", vec!["Control"], None),
        ("\x1b[118;6u", "U+0076", vec!["Control", "Shift"], None),
        (
            "\x1b[118;10u",
            "U+0076",
            vec!["Shift", "Super"],
            Some("clipboard.paste_reflow"),
        ),
        (
            "\x1b[90;10u",
            "U+005A",
            vec!["Shift", "Super"],
            Some("history.redo"),
        ),
        (
            "\x1b[13;9u",
            "Enter",
            vec!["Super"],
            Some("submission.submit_remove"),
        ),
        (
            "\x1b[13;10u",
            "Enter",
            vec!["Shift", "Super"],
            Some("submission.submit_keep"),
        ),
    ] {
        inspect(bytes, key, &modifiers, action);
    }
}

#[test]
fn named_keys_and_navigation_keep_exact_kitty_or_legacy_identity() {
    for (bytes, key, modifiers, action) in [
        ("\x1b[1;3A", "Up", vec!["Alt"], "navigation.fast_previous"),
        ("\x1b[5~", "PageUp", vec![], "navigation.fast_previous"),
        ("\x1b[6~", "PageDown", vec![], "navigation.fast_next"),
        (
            "\x1b[5;2~",
            "PageUp",
            vec!["Shift"],
            "navigation.fast_extend_previous",
        ),
        (
            "\x1b[6;2~",
            "PageDown",
            vec!["Shift"],
            "navigation.fast_extend_next",
        ),
        ("\x1b[1;9B", "Down", vec!["Super"], "editor.visual_down"),
        (
            "\x1b[1;9D",
            "Left",
            vec!["Super"],
            "editor.move_visual_row_start",
        ),
        (
            "\x1b[1;9C",
            "Right",
            vec!["Super"],
            "editor.move_visual_row_end",
        ),
        (
            "\x1b[1;10D",
            "Left",
            vec!["Shift", "Super"],
            "editor.extend_visual_row_start",
        ),
        (
            "\x1b[1;10C",
            "Right",
            vec!["Shift", "Super"],
            "editor.extend_visual_row_end",
        ),
        (
            "\x1b[1;6D",
            "Left",
            vec!["Control", "Shift"],
            "editor.extend_line_start",
        ),
        ("\x1b[3~", "Delete", vec![], "text.delete_forward"),
        ("\x1b[3;2~", "Delete", vec!["Shift"], "text.delete_forward"),
        ("\x7f", "Backspace", vec![], "text.backspace"),
    ] {
        inspect(bytes, key, &modifiers, Some(action));
    }
}

#[test]
fn terminal_safe_boundary_family_reports_exact_logical_events() {
    for (bytes, key, modifiers, action) in [
        ("\x1b[1;5A", "Up", vec!["Control"], "editor.document_start"),
        ("\x1b[1;5B", "Down", vec!["Control"], "editor.document_end"),
        (
            "\x1b[1;6A",
            "Up",
            vec!["Control", "Shift"],
            "editor.extend_document_start",
        ),
        (
            "\x1b[1;6B",
            "Down",
            vec!["Control", "Shift"],
            "editor.extend_document_end",
        ),
        ("\x1b[1;5D", "Left", vec!["Control"], "editor.line_start"),
        ("\x1b[1;5C", "Right", vec!["Control"], "editor.line_end"),
        ("\x1b[H", "Home", vec![], "editor.line_start"),
        ("\x1b[F", "End", vec![], "editor.line_end"),
        ("\x1b[1;9A", "Up", vec!["Super"], "editor.visual_up"),
        ("\x1b[1;33B", "Down", vec!["Meta"], "editor.visual_down"),
    ] {
        inspect(bytes, key, &modifiers, Some(action));
    }

    for (bytes, key, modifiers, action) in [
        ("\x1b[1;5A", "Up", vec!["Control"], "board.first_thought"),
        ("\x1b[1;5:1A", "Up", vec!["Control"], "board.first_thought"),
        ("\x1b[1;5B", "Down", vec!["Control"], "board.last_thought"),
        (
            "\x1b[1;6A",
            "Up",
            vec!["Control", "Shift"],
            "board.range_first_thought",
        ),
        (
            "\x1b[1;6B",
            "Down",
            vec!["Control", "Shift"],
            "board.range_last_thought",
        ),
        ("\x1b[1;3A", "Up", vec!["Alt"], "thought.insert_above"),
        ("\x1b[1;3B", "Down", vec!["Alt"], "thought.insert_below"),
        (
            "\x1b[107;5u",
            "U+006B",
            vec!["Control"],
            "board.first_thought",
        ),
        (
            "\x1b[106;5u",
            "U+006A",
            vec!["Control"],
            "board.last_thought",
        ),
        (
            "\x1b[75;5u",
            "U+004B",
            vec!["Control"],
            "board.range_first_thought",
        ),
        (
            "\x1b[74;5u",
            "U+004A",
            vec!["Control"],
            "board.range_last_thought",
        ),
        ("\x1b[107;3u", "U+006B", vec!["Alt"], "thought.insert_above"),
        ("\x1b[106;3u", "U+006A", vec!["Alt"], "thought.insert_below"),
    ] {
        let data = capture(bytes, "board", None, true);
        assert_eq!(data["event"]["keystroke"]["key"], key);
        assert_eq!(data["event"]["keystroke"]["modifiers"], json!(modifiers));
        assert_eq!(data["event"]["action"], action);
        assert_eq!(data["event"]["classification"], "resolved");
    }
}

#[test]
fn macos_option_shift_reorder_diagnostic_reports_exact_received_event_and_action() {
    let data = capture("\x1b[1;4A", "board", None, true);
    let event = &data["event"];
    assert_eq!(event["keystroke"]["key"], "Up");
    assert_eq!(event["keystroke"]["modifiers"], json!(["Alt", "Shift"]));
    assert_eq!(event["action"], "thought.move_up");
    assert_eq!(event["classification"], "resolved");
}

#[test]
fn repeat_is_resolved_release_is_reported_without_dispatch() {
    for (sequence, phase, classification, action) in [
        ("\x1b[106;1:2u", "repeat", "resolved", Some("list.next")),
        ("\x1b[106;1:3u", "release", "release_ignored", None),
        (
            "\x1b[1;5:2B",
            "repeat",
            "resolved",
            Some("board.last_thought"),
        ),
        ("\x1b[1;5:3B", "release", "release_ignored", None),
        (
            "\x1b[1;3:2A",
            "repeat",
            "resolved",
            Some("thought.insert_above"),
        ),
        ("\x1b[1;3:3A", "release", "release_ignored", None),
    ] {
        let data = capture(sequence, "board", None, true);
        assert_eq!(data["event"]["keystroke"]["phase"], phase);
        assert_eq!(data["event"]["classification"], classification);
        assert_eq!(data["event"]["action"].as_str(), action);
    }
}

#[test]
fn altgr_and_layout_text_are_reserved_without_echoing_user_content() {
    let data = capture("\x1b[8364;7u", "board,compose", None, true);
    assert_eq!(data["event"]["keystroke"]["key"], "U+20AC");
    assert_eq!(
        data["event"]["keystroke"]["modifiers"],
        json!(["Control", "Alt"])
    );
    assert_eq!(data["event"]["classification"], "reserved_literal");
    assert!(!data.to_string().contains('€'));
}

#[test]
fn custom_aliases_resolve_but_removed_bindings_are_unbound() {
    let config = "[keymap]\nschema_version=1\n[keymap.bindings.board]\n\"submission.submit_remove\"=[{key='F5'},{key='F6',modifiers=['Hyper']}]";
    for sequence in ["\x1b[15~", "\x1b[17;17~"] {
        let data = capture(sequence, "board", Some(config), false);
        assert_eq!(data["event"]["action"], "submission.submit_remove");
    }
    let data = capture("\x1b[13;9u", "board", Some(config), false);
    assert_eq!(data["event"]["classification"], "unbound");
}

#[test]
fn timeout_escape_and_invalid_configuration_bypass_restore_terminal_state() {
    let invalid = "[keymap]\nschema_version=99";
    let data = capture("", "board", Some(invalid), true);
    assert_eq!(data["status"], "no_event_received");
    assert!(data["event"].is_null());
    assert!(data["explanation"].as_str().unwrap().contains(
        "Proqi cannot know whether Ghostty, the OS, Karabiner, Herdr, or another layer consumed it"
    ));
    let data = capture("\x1b", "help", Some(invalid), true);
    assert_eq!(data["status"], "cancelled");
    assert_eq!(data["event"]["action"], "context.close");
}

#[test]
fn enhanced_keypad_lock_state_and_alternate_layout_fields_are_truthful() {
    let data = capture("\x1b[57427;193u", "board", None, true);
    assert_eq!(data["event"]["keystroke"]["key"], "KeypadBegin");
    assert_eq!(
        data["event"]["keystroke"]["state"],
        json!(["Keypad", "CapsLock", "NumLock"])
    );
    let data = capture("\x1b[97:65;2u", "compose", None, true);
    assert_eq!(data["event"]["keystroke"]["key"], "U+0041");
    assert_eq!(data["event"]["keystroke"]["modifiers"], json!([]));
    assert_eq!(data["event"]["classification"], "reserved_literal");
}

#[test]
fn termination_request_cancels_capture_and_restores_the_same_terminal() {
    let data = capture("__SIGTERM__", "board", None, true);
    assert_eq!(data["status"], "cancelled");
    assert!(data["event"].is_null());
    assert_eq!(
        data["explanation"],
        "Capture cancelled by a termination request."
    );
}

#[test]
fn current_capture_json_contract_matches_reviewed_fixtures() {
    let received: Value =
        serde_json::from_str(include_str!("../fixtures/cli/v1/key_capture.received.json")).unwrap();
    assert_eq!(capture("\x1b[118;10u", "board,edit", None, true), received);
    let timeout: Value =
        serde_json::from_str(include_str!("../fixtures/cli/v1/key_capture.no_event.json")).unwrap();
    assert_eq!(capture("", "board", None, true), timeout);
}

#[test]
fn cleanup_control_shift_chords_and_plain_editor_text_are_distinct() {
    inspect(
        "\x1b[102;6u",
        "U+0066",
        &["Control", "Shift"],
        Some("thought.reflow"),
    );
    inspect("\x1b[70;5u", "U+0046", &["Control"], Some("thought.reflow"));
    inspect("\x1b[102;9u", "U+0066", &["Super"], None);
    inspect("\x1b[102;5u", "U+0066", &["Control"], None);
    inspect("f", "U+0066", &[], None);
}
