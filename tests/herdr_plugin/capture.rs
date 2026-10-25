//! The real `proqi herdr capture` binary against a scripted Herdr and a
//! clipboard fixture: selection precedence, clipboard fallback, the shared tab
//! session, notifications, and failures that store and create nothing.

use std::{fs, path::Path, process::Output};

use serde_json::{Value, json};

use super::fake_herdr::{FakeHerdr, data};

fn agent() -> Value {
    json!({ "pane_id": "w1:p1", "tab_id": "w1:t1", "agent": "claude", "cwd": "/work" })
}

fn clipboard(herdr: &FakeHerdr, fixture: &Value) -> std::path::PathBuf {
    let path = herdr.sandbox.path().join("clipboard.json");
    fs::write(&path, fixture.to_string()).expect("clipboard fixture");
    path
}

/// Capture with an optional selection; the clipboard is always a fixture.
fn capture(herdr: &FakeHerdr, selection: Option<&str>, fixture: &Path) -> Output {
    herdr.invoke("capture", "w1:p1", true, |context, command| {
        if let Some(selection) = selection {
            context["selected_text"] = json!(selection);
        }
        command.env("PROQI_TEST_CLIPBOARD_FIXTURE", fixture);
    })
}

fn proqi(herdr: &FakeHerdr, arguments: &[&str]) -> Value {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_proqi"))
        .arg("--json")
        .arg("--state-dir")
        .arg(herdr.state_dir())
        .args(arguments)
        .env_remove("HERDR_ENV")
        .output()
        .expect("run proqi");
    data(&output)
}

fn sessions(herdr: &FakeHerdr) -> Vec<Value> {
    proqi(herdr, &["sessions", "list"])["sessions"]
        .as_array()
        .expect("sessions")
        .clone()
}

fn contents(herdr: &FakeHerdr, session: &str) -> Vec<String> {
    proqi(herdr, &["thoughts", "list", session])["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["content"].as_str().expect("content").to_owned())
        .collect()
}

fn notifications(herdr: &FakeHerdr) -> Vec<String> {
    herdr
        .sandbox
        .calls()
        .into_iter()
        .filter_map(|call| {
            call.strip_prefix("herdr notification show Proqi --body ")
                .map(str::to_owned)
        })
        .collect()
}

#[test]
fn a_selection_is_stored_exactly_in_the_tab_session_and_the_toggle_opens_it() {
    let herdr = FakeHerdr::new();
    herdr.panes(&[agent()]);
    let unreadable = clipboard(&herdr, &json!({ "kind": "unavailable" }));
    let selection = "  fn main() {\r\n\tprintln!(\"Grüße 👩‍💻\");\u{7}\n}\n";
    let captured = data(&capture(&herdr, Some(selection), &unreadable));
    assert_eq!(captured["action"], "captured");
    assert_eq!(captured["source"], "selection");
    assert_eq!(captured["bytes"], selection.len());
    let session = captured["session_id"].as_str().expect("session").to_owned();
    assert_eq!(contents(&herdr, &session), vec![selection]);
    assert_eq!(sessions(&herdr)[0]["name"], "demo-w1-t1");
    assert_eq!(
        notifications(&herdr),
        vec![format!(
            "Captured to Proqi: \"fn main() {{ println!(\"Grüße 👩‍💻\"); }}\" ({} characters)",
            captured["characters"]
        )]
    );
    let calls = herdr.sandbox.calls();
    assert!(
        calls
            .iter()
            .all(|call| !call.contains("pane open") && !call.contains("focus")),
        "capture opens and focuses nothing: {calls:?}"
    );

    herdr.opens("w1:p2");
    let opened = data(&herdr.toggle("w1:p1", true));
    assert_eq!(opened["session_id"], session.as_str());
}

#[test]
fn without_a_selection_the_clipboard_text_is_stored() {
    let herdr = FakeHerdr::new();
    herdr.panes(&[agent()]);
    let fixture = clipboard(&herdr, &json!({ "kind": "text", "text": "copied\n" }));
    let captured = data(&capture(&herdr, None, &fixture));
    assert_eq!(captured["source"], "clipboard");
    let session = captured["session_id"].as_str().expect("session");
    assert_eq!(contents(&herdr, session), vec!["copied\n"]);
    assert_eq!(
        notifications(&herdr),
        vec!["Captured to Proqi: \"copied\" (7 characters)".to_owned()]
    );
}

#[test]
fn repeated_captures_append_to_one_session_in_order() {
    let herdr = FakeHerdr::new();
    herdr.panes(&[agent()]);
    let fixture = clipboard(&herdr, &json!({ "kind": "text", "text": "same" }));
    let mut session = String::new();
    for selection in [None, Some("second"), None] {
        let captured = data(&capture(&herdr, selection, &fixture));
        session = captured["session_id"].as_str().expect("session").to_owned();
    }
    assert_eq!(sessions(&herdr).len(), 1);
    assert_eq!(contents(&herdr, &session), vec!["same", "second", "same"]);
}

#[test]
fn nothing_is_created_or_stored_when_there_is_no_text_to_capture() {
    let cases = [
        (
            Some(" \n\t"),
            json!({ "kind": "text", "text": "unused" }),
            "Nothing captured: the selection is empty",
            "capture_empty",
        ),
        (
            None,
            json!({ "kind": "text", "text": "" }),
            "Nothing captured: the clipboard is empty",
            "capture_empty",
        ),
        (
            None,
            json!({ "kind": "image" }),
            "Nothing captured: the clipboard holds no text (for example an image)",
            "capture_no_text",
        ),
    ];
    for (selection, fixture, message, code) in cases {
        let herdr = FakeHerdr::new();
        herdr.panes(&[agent()]);
        let path = clipboard(&herdr, &fixture);
        let output = capture(&herdr, selection, &path);
        assert_eq!(output.status.code(), Some(7));
        let envelope: Value = serde_json::from_slice(&output.stdout).expect("JSON error");
        assert_eq!(envelope["error"]["code"], code);
        assert_eq!(envelope["error"]["message"], message);
        assert_eq!(notifications(&herdr), vec![message.to_owned()]);
        assert!(sessions(&herdr).is_empty(), "no session is created");
        assert!(
            !herdr
                .sandbox
                .path()
                .join("plugin-state/companions.json")
                .exists()
        );
    }
}

#[test]
fn a_failed_agent_query_stores_nothing() {
    let herdr = FakeHerdr::new();
    herdr.panes(&[agent()]);
    fs::write(herdr.sandbox.path().join("herdr").join("agents-fail"), "").expect("marker");
    let fixture = clipboard(&herdr, &json!({ "kind": "text", "text": "text" }));
    let output = capture(&herdr, None, &fixture);
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("JSON error");
    assert_eq!(envelope["error"]["code"], "herdr_failed");
    assert!(sessions(&herdr).is_empty());
    let shown = notifications(&herdr);
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(
        shown[0].starts_with("Nothing captured: Herdr rejected"),
        "{shown:?}"
    );
}

#[test]
fn capture_outside_a_herdr_plugin_action_is_unsupported() {
    let herdr = FakeHerdr::new();
    let fixture = clipboard(&herdr, &json!({ "kind": "text", "text": "text" }));
    let output = herdr.invoke("capture", "w1:p1", false, |_, command| {
        command.env("PROQI_TEST_CLIPBOARD_FIXTURE", &fixture);
    });
    assert_eq!(output.status.code(), Some(6));
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("JSON error");
    assert_eq!(envelope["error"]["code"], "unsupported");
    assert!(herdr.sandbox.calls().is_empty());
}
