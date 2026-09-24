//! Companion toggle policy and orchestration over fake host, state, and sessions.

mod fakes;

use std::path::PathBuf;

use crate::ports::companion::{CompanionRecord, CompanionSessionState, PaneProcess};

use super::{
    COMPANION_PANE_LABEL, CompanionToggleError, CompanionToggleOutcome, companion_session_name,
    toggle_companion,
};
use fakes::{FakeHost, FakeRecords, FakeSessions, agent, companion, context, session, shell};

const OWN: &str = "ses_06g30t7dv5qv55n1ppn3clis3k";
const OTHER: &str = "ses_06g30t8fudrq55fdkjqr6mpe44";

fn record(pane: &str, session_id: &str) -> CompanionRecord {
    CompanionRecord {
        tab_id: "w1:t1".to_owned(),
        pane_id: pane.to_owned(),
        session_id: session(session_id),
    }
}

#[test]
fn session_name_uses_a_meaningful_tab_label_and_qualifies_numeric_defaults() {
    let mut named = context("w1:p1");
    named.tab_label = Some("  herdr-plugin ".to_owned());
    assert_eq!(companion_session_name(&named), "herdr-plugin");
    let mut numeric = context("w1:p1");
    numeric.tab_label = Some("2".to_owned());
    numeric.workspace_label = Some("api".to_owned());
    assert_eq!(companion_session_name(&numeric), "api-2");
    let mut unlabeled = context("w1:p1");
    unlabeled.tab_label = None;
    unlabeled.workspace_label = Some(" ".to_owned());
    assert_eq!(companion_session_name(&unlabeled), "w1-w1:t1");
}

#[test]
fn first_toggle_ensures_the_tab_session_and_opens_beside_the_focused_agent() {
    let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true)]);
    let mut records = FakeRecords::default();
    let mut sessions = FakeSessions::with_named("agent-tab", OWN);
    let outcome = toggle_companion(&mut host, &mut records, &mut sessions).expect("toggle");
    assert_eq!(
        outcome,
        CompanionToggleOutcome::Opened {
            tab_id: "w1:t1".to_owned(),
            pane_id: "w1:p10".to_owned(),
            session_id: session(OWN),
            replaced_pane_id: None,
        }
    );
    assert_eq!(
        sessions.ensured,
        vec![("agent-tab".to_owned(), PathBuf::from("/work"))]
    );
    assert_eq!(host.calls, vec![format!("open w1:p1 /work {OWN}")]);
    assert_eq!(records.0, vec![record("w1:p10", OWN)]);
}

#[test]
fn toggle_focuses_an_unfocused_live_companion_and_closes_a_focused_one_after_flushing() {
    let panes = vec![agent("w1:p1", true), companion("w1:p9", false)];
    let mut host = FakeHost::new("w1:p1", panes.clone()).with_process("w1:p9", own_proqi());
    let mut records = FakeRecords(vec![record("w1:p9", OWN)]);
    let mut sessions = FakeSessions::default();
    let focused = toggle_companion(&mut host, &mut records, &mut sessions).expect("focus");
    assert_eq!(
        focused,
        CompanionToggleOutcome::Focused {
            pane_id: "w1:p9".to_owned()
        }
    );

    let mut host = FakeHost::new("w1:p9", panes).with_process("w1:p9", own_proqi());
    let closed = toggle_companion(&mut host, &mut records, &mut sessions).expect("close");
    assert_eq!(
        closed,
        CompanionToggleOutcome::Closed {
            pane_id: "w1:p9".to_owned(),
            session_id: session(OWN)
        }
    );
    assert_eq!(sessions.flushed, vec![session(OWN)]);
    assert_eq!(host.calls, vec!["close w1:p9".to_owned()]);
    assert!(records.0.is_empty());
}

#[test]
fn failed_flush_keeps_the_companion_open_and_reports_it() {
    let panes = vec![agent("w1:p1", false), companion("w1:p9", true)];
    let mut host = FakeHost::new("w1:p9", panes).with_process("w1:p9", own_proqi());
    let mut records = FakeRecords(vec![record("w1:p9", OWN)]);
    let mut sessions = FakeSessions::default();
    sessions.fail_flush = true;
    let error = toggle_companion(&mut host, &mut records, &mut sessions).expect_err("flush");
    assert!(matches!(error, CompanionToggleError::Session(_)));
    assert!(host.calls.is_empty());
    assert_eq!(records.0.len(), 1);
    assert_eq!(
        host.notifications,
        vec!["owner did not confirm the flush".to_owned()]
    );
}

#[test]
fn a_starting_launcher_counts_as_the_live_companion() {
    let panes = vec![agent("w1:p1", true), companion_without_presence("w1:p9")];
    let mut host = FakeHost::new("w1:p1", panes).with_process("w1:p9", PaneProcess::Launcher);
    let mut records = FakeRecords(vec![record("w1:p9", OWN)]);
    let outcome =
        toggle_companion(&mut host, &mut records, &mut FakeSessions::default()).expect("focus");
    assert_eq!(
        outcome,
        CompanionToggleOutcome::Focused {
            pane_id: "w1:p9".to_owned()
        }
    );
}

#[test]
fn a_dead_companion_after_restart_is_replaced_with_the_same_session_without_duplicates() {
    let panes = vec![agent("w1:p1", true), companion_without_presence("w1:p9")];
    let mut host = FakeHost::new("w1:p1", panes).with_process("w1:p9", PaneProcess::IdleShell);
    let mut records = FakeRecords(vec![record("w1:p9", OWN)]);
    let mut sessions = FakeSessions::default().with_state(OWN, CompanionSessionState::Resumable);
    let outcome = toggle_companion(&mut host, &mut records, &mut sessions).expect("replace");
    assert_eq!(
        outcome,
        CompanionToggleOutcome::Opened {
            tab_id: "w1:t1".to_owned(),
            pane_id: "w1:p10".to_owned(),
            session_id: session(OWN),
            replaced_pane_id: Some("w1:p9".to_owned()),
        }
    );
    assert!(
        sessions.ensured.is_empty(),
        "the recorded session is reopened"
    );
    assert_eq!(
        host.calls,
        vec![format!("open w1:p1 /work {OWN}"), "close w1:p9".to_owned()]
    );
    assert_eq!(records.0, vec![record("w1:p10", OWN)]);
}

#[test]
fn focusing_the_dead_pane_itself_splits_beside_the_agent_before_closing_it() {
    let panes = vec![agent("w1:p1", false), companion_without_presence("w1:p9")];
    let mut host = FakeHost::new("w1:p9", panes).with_process("w1:p9", PaneProcess::IdleShell);
    let mut records = FakeRecords(vec![record("w1:p9", OWN)]);
    let mut sessions = FakeSessions::default().with_state(OWN, CompanionSessionState::Resumable);
    toggle_companion(&mut host, &mut records, &mut sessions).expect("replace");
    assert_eq!(
        host.calls,
        vec![format!("open w1:p1 /work {OWN}"), "close w1:p9".to_owned()]
    );
}

#[test]
fn a_recorded_pane_that_now_runs_anything_else_is_never_closed() {
    for process in [
        PaneProcess::Other,
        PaneProcess::Proqi {
            session_id: Some(session(OTHER)),
        },
    ] {
        let panes = vec![agent("w1:p1", true), companion_without_presence("w1:p9")];
        let mut host = FakeHost::new("w1:p1", panes).with_process("w1:p9", process);
        let mut records = FakeRecords(vec![record("w1:p9", OWN)]);
        let mut sessions = FakeSessions::with_named("agent-tab", OWN);
        toggle_companion(&mut host, &mut records, &mut sessions).expect("open");
        assert_eq!(host.calls, vec![format!("open w1:p1 /work {OWN}")]);
        assert_eq!(records.0, vec![record("w1:p10", OWN)]);
    }
}

#[test]
fn an_idle_shell_that_lost_the_companion_label_is_left_alone() {
    let mut renamed = companion_without_presence("w1:p9");
    renamed.label = Some("scratch".to_owned());
    let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true), renamed])
        .with_process("w1:p9", PaneProcess::IdleShell);
    let mut records = FakeRecords(vec![record("w1:p9", OWN)]);
    toggle_companion(
        &mut host,
        &mut records,
        &mut FakeSessions::with_named("agent-tab", OWN),
    )
    .expect("open");
    assert!(!host.calls.iter().any(|call| call.starts_with("close")));
}

#[test]
fn a_companion_opened_elsewhere_is_focused_and_toggling_from_it_returns_to_the_agent() {
    let panes = vec![agent("w1:p1", true), companion("w1:p5", false)];
    let mut host = FakeHost::new("w1:p1", panes.clone());
    let mut records = FakeRecords::default();
    let mut sessions = FakeSessions::default();
    let focused = toggle_companion(&mut host, &mut records, &mut sessions).expect("focus");
    assert_eq!(
        focused,
        CompanionToggleOutcome::Focused {
            pane_id: "w1:p5".to_owned()
        }
    );

    let mut host = FakeHost::new("w1:p5", panes);
    let returned = toggle_companion(&mut host, &mut records, &mut sessions).expect("return");
    assert_eq!(
        returned,
        CompanionToggleOutcome::Returned {
            pane_id: "w1:p1".to_owned()
        }
    );
    assert!(!host.calls.iter().any(|call| call.starts_with("close")));
    assert!(
        records.0.is_empty(),
        "a foreign companion is never recorded"
    );
}

#[test]
fn a_foreign_companion_without_one_agent_reports_instead_of_guessing() {
    let panes = vec![
        agent("w1:p1", false),
        agent("w1:p2", false),
        companion("w1:p5", true),
    ];
    let mut host = FakeHost::new("w1:p5", panes);
    let error = toggle_companion(
        &mut host,
        &mut FakeRecords::default(),
        &mut FakeSessions::default(),
    )
    .expect_err("ambiguous");
    assert!(matches!(error, CompanionToggleError::NoReturnTarget));
    assert_eq!(host.notifications.len(), 1);
}

#[test]
fn a_session_already_open_in_another_workspace_is_reported_and_never_launched() {
    let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true)]);
    let mut records = FakeRecords::default();
    let mut sessions =
        FakeSessions::with_named("agent-tab", OWN).with_state(OWN, CompanionSessionState::Active);
    let error = toggle_companion(&mut host, &mut records, &mut sessions).expect_err("active");
    assert!(matches!(error, CompanionToggleError::SessionActive { .. }));
    assert!(host.calls.is_empty());
    assert!(records.0.is_empty());
    assert_eq!(
        host.notifications,
        vec!["Proqi session agent-tab is already open in another pane".to_owned()]
    );
}

#[test]
fn a_companion_another_tab_is_still_launching_blocks_a_racing_open() {
    let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true)])
        .with_process("w2:p4", PaneProcess::Launcher);
    let mut records = FakeRecords(vec![CompanionRecord {
        tab_id: "w2:t3".to_owned(),
        pane_id: "w2:p4".to_owned(),
        session_id: session(OWN),
    }]);
    let mut sessions = FakeSessions::with_named("agent-tab", OWN);
    let error = toggle_companion(&mut host, &mut records, &mut sessions).expect_err("racing");
    assert!(matches!(error, CompanionToggleError::SessionActive { .. }));
    assert!(host.calls.is_empty());
}

#[test]
fn a_vanished_record_is_forgotten_and_the_named_session_reopens() {
    let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true)]);
    let mut records = FakeRecords(vec![record("w1:p9", OWN)]);
    let mut sessions = FakeSessions::with_named("agent-tab", OWN);
    toggle_companion(&mut host, &mut records, &mut sessions).expect("open");
    assert_eq!(records.0, vec![record("w1:p10", OWN)]);
    assert_eq!(sessions.ensured.len(), 1);
}

#[test]
fn a_trashed_recorded_session_falls_back_to_the_named_session() {
    let panes = vec![agent("w1:p1", true), companion_without_presence("w1:p9")];
    let mut host = FakeHost::new("w1:p1", panes).with_process("w1:p9", PaneProcess::IdleShell);
    let mut records = FakeRecords(vec![record("w1:p9", OTHER)]);
    let mut sessions = FakeSessions::with_named("agent-tab", OWN)
        .with_state(OTHER, CompanionSessionState::Unavailable);
    let outcome = toggle_companion(&mut host, &mut records, &mut sessions).expect("open");
    assert!(matches!(
        outcome,
        CompanionToggleOutcome::Opened { session_id, .. } if session_id == session(OWN)
    ));
}

#[test]
fn the_companion_label_matches_the_manifest_title() {
    assert_eq!(COMPANION_PANE_LABEL, "Proqi");
    assert_eq!(shell("w1:p2").label, None);
}

fn own_proqi() -> PaneProcess {
    PaneProcess::Proqi {
        session_id: Some(session(OWN)),
    }
}

fn companion_without_presence(pane: &str) -> crate::ports::companion::PaneObservation {
    let mut pane = companion(pane, false);
    pane.proqi_presence = false;
    pane
}
