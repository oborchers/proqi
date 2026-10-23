//! The real `proqi herdr toggle` binary against a scripted Herdr: open, repeated
//! focus, close, and dead-pane replacement after a simulated host restart.

use std::{fs, path::Path};

use serde_json::{Value, json};

use super::{
    fake_herdr::{FakeHerdr, data},
    support::stderr,
};

fn agent() -> Value {
    json!({ "pane_id": "w1:p1", "tab_id": "w1:t1", "agent": "claude", "cwd": "/work" })
}

fn companion(pane: &str, live: bool) -> Value {
    let mut value = json!({ "pane_id": pane, "tab_id": "w1:t1", "label": "Proqi" });
    if live {
        value["display_agent"] = json!("proqi");
    }
    value
}

#[test]
fn toggle_outside_a_herdr_plugin_action_is_unsupported() {
    let herdr = FakeHerdr::new();
    let output = herdr.toggle("w1:p1", false);
    assert_eq!(output.status.code(), Some(6));
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("JSON error");
    assert_eq!(envelope["error"]["code"], "unsupported");
    assert!(herdr.sandbox.calls().is_empty());
}

#[test]
fn toggle_opens_once_focuses_repeatedly_closes_and_replaces_a_dead_pane() {
    let herdr = FakeHerdr::new();
    herdr.panes(&[agent()]);
    herdr.opens("w1:p2");
    let opened = data(&herdr.toggle("w1:p1", true));
    assert_eq!(opened["action"], "opened");
    assert_eq!(opened["pane_id"], "w1:p2");
    let session = opened["session_id"].as_str().expect("session").to_owned();
    let open = herdr
        .sandbox
        .calls()
        .into_iter()
        .find(|call| call.contains("pane open"))
        .expect("open");
    assert!(
        open.contains(&format!("--env PROQI_HERDR_SESSION={session} --focus")),
        "{open}"
    );
    assert!(
        open.contains("--target-pane w1:p1 --direction right"),
        "{open}"
    );
    assert_named_session(&herdr, &session, "demo-w1-t1");

    herdr.panes(&[agent(), companion("w1:p2", true)]);
    herdr.process("w1:p2", &["/opt/bin/proqi", "--resume", &session], false);
    for _ in 0..3 {
        assert_eq!(data(&herdr.toggle("w1:p1", true))["action"], "focused");
    }
    assert_eq!(
        herdr.opened_calls(),
        1,
        "repeated toggles never open a second pane"
    );

    // No Proqi owns the session in this scripted Herdr, so nothing can confirm
    // that pending edits are durable. The toggle must refuse to close.
    let refused = herdr.toggle("w1:p2", true);
    assert_eq!(refused.status.code(), Some(5), "{}", stderr(&refused));
    let envelope: Value = serde_json::from_slice(&refused.stdout).expect("JSON error");
    assert_eq!(envelope["error"]["code"], "session_busy");
    assert!(
        !herdr
            .sandbox
            .calls()
            .iter()
            .any(|call| call.starts_with("herdr pane close"))
    );

    herdr.panes(&[agent()]);
    herdr.opens("w1:p3");
    let reopened = data(&herdr.toggle("w1:p1", true));
    assert_eq!(
        reopened["session_id"],
        session.as_str(),
        "the tab keeps one session"
    );

    // A host cold restart leaves the companion pane as an idle login shell.
    herdr.panes(&[agent(), companion("w1:p3", false)]);
    herdr.process("w1:p3", &["-zsh"], false);
    herdr.opens("w1:p4");
    let replaced = data(&herdr.toggle("w1:p1", true));
    assert_eq!(replaced["action"], "opened");
    assert_eq!(replaced["replaced_pane_id"], "w1:p3");
    assert_eq!(replaced["session_id"], session.as_str());
    let calls = herdr.sandbox.calls();
    let open_p4 = calls
        .iter()
        .rposition(|call| call.contains("pane open"))
        .expect("open");
    let close_p3 = calls
        .iter()
        .position(|call| call == "herdr pane close w1:p3")
        .expect("close");
    assert!(
        open_p4 < close_p3,
        "the replacement opens before the dead pane closes"
    );
    assert_eq!(herdr.opened_calls(), 3);
}

#[test]
fn a_recorded_pane_now_running_other_work_is_left_untouched() {
    let herdr = FakeHerdr::new();
    herdr.panes(&[agent()]);
    herdr.opens("w1:p2");
    data(&herdr.toggle("w1:p1", true));
    herdr.panes(&[agent(), companion("w1:p2", false)]);
    herdr.process("w1:p2", &["vim", "notes.md"], true);
    herdr.opens("w1:p3");
    let opened = data(&herdr.toggle("w1:p1", true));
    assert_eq!(opened["replaced_pane_id"], Value::Null);
    assert!(
        !herdr
            .sandbox
            .calls()
            .iter()
            .any(|call| call.starts_with("herdr pane close"))
    );
}

fn assert_named_session(herdr: &FakeHerdr, session: &str, name: &str) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_proqi"))
        .args(["--json", "--state-dir"])
        .arg(herdr.sandbox.path().join("proqi-state"))
        .args(["sessions", "list"])
        .env("HOME", herdr.sandbox.home())
        .output()
        .expect("list sessions");
    let listed: Value = serde_json::from_slice(&output.stdout).expect("sessions");
    let sessions = listed["data"]["sessions"]
        .as_array()
        .expect("sessions array");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["id"], session);
    assert_eq!(sessions[0]["name"], name);
    let origin = sessions[0]["origin_cwd"].as_str().expect("origin");
    assert_eq!(
        Path::new(origin),
        fs::canonicalize(&herdr.work).expect("canonical work")
    );
}

#[test]
fn the_first_open_names_the_session_after_the_tabs_only_named_agent() {
    let herdr = FakeHerdr::new();
    herdr.panes(&[agent()]);
    herdr.agents(&[
        json!({ "pane_id": "w1:p1", "tab_id": "w1:t1", "agent": "claude", "name": "api-claude" }),
        json!({ "pane_id": "w1:p8", "tab_id": "w1:t2", "agent": "codex", "name": "elsewhere" }),
    ]);
    herdr.opens("w1:p2");
    let opened = data(&herdr.toggle("w1:p1", true));
    let session = opened["session_id"].as_str().expect("session");
    assert_named_session(&herdr, session, "api-claude");
}

#[test]
fn a_failed_agent_query_reports_herdr_failed_and_opens_nothing() {
    let herdr = FakeHerdr::new();
    herdr.panes(&[agent()]);
    fs::write(herdr.sandbox.path().join("herdr").join("agents-fail"), "").expect("marker");
    herdr.opens("w1:p2");
    let output = herdr.toggle("w1:p1", true);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("JSON error");
    assert_eq!(envelope["error"]["code"], "herdr_failed");
    assert_eq!(herdr.opened_calls(), 0);
    assert!(
        !herdr
            .sandbox
            .path()
            .join("plugin-state")
            .join("companions.json")
            .exists(),
        "no record is written"
    );
}

#[test]
fn toggling_from_a_proqi_the_plugin_did_not_open_returns_to_the_agent_without_closing_it() {
    let herdr = FakeHerdr::new();
    herdr.panes(&[agent(), companion("w1:p5", true)]);
    herdr.process("w1:p5", &["proqi", "--resume", "manual"], true);
    let returned = data(&herdr.toggle("w1:p5", true));
    assert_eq!(returned["action"], "returned");
    assert_eq!(returned["pane_id"], "w1:p1");
    let calls = herdr.sandbox.calls();
    assert!(
        !calls
            .iter()
            .any(|call| call.starts_with("herdr pane close"))
    );
    assert!(!calls.iter().any(|call| call.contains("pane open")));
}
