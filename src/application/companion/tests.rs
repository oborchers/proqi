//! Companion session resolution and open/focus/close policy over fake host,
//! state, and sessions. Naming lives in `naming`, close safety in `safety`.

mod fakes;
mod naming;
mod safety;

use std::path::PathBuf;

use crate::ports::companion::{CompanionRecord, CompanionSessionState, PaneProcess, ProqiPresence};

use super::{
    CompanionToggleError, CompanionToggleOutcome, companion_session_cwd, toggle_companion,
};
use fakes::{
    FakeHost, FakeRecords, FakeSessions, agent, companion, context, record, session, shell,
};

const OWN: &str = "ses_06g30t7dv5qv55n1ppn3clis3k";
const OTHER: &str = "ses_06g30t8fudrq55fdkjqr6mpe44";

fn own_proqi() -> PaneProcess {
    PaneProcess::Proqi {
        session_id: Some(session(OWN)),
    }
}

#[test]
fn the_session_origin_is_the_session_root_not_the_focused_split() {
    let context = context("w1:p1");
    assert_eq!(companion_session_cwd(&context), PathBuf::from("/work"));
    let mut without_root = context;
    without_root.session_root = None;
    assert_eq!(
        companion_session_cwd(&without_root),
        PathBuf::from("/work/sub")
    );
}

#[test]
fn first_toggle_ensures_the_tab_session_and_opens_beside_the_focused_pane() {
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
    assert_eq!(host.calls, vec![format!("open w1:p1 /work/sub {OWN}")]);
    assert_eq!(records.0, vec![record(Some("w1:p10"), OWN)]);
}

#[test]
fn toggle_focuses_a_live_companion_and_closes_it_after_a_confirmed_flush() {
    let panes = vec![agent("w1:p1", true), companion("w1:p9", false)];
    let mut records = FakeRecords(vec![record(Some("w1:p9"), OWN)]);
    let mut sessions = FakeSessions::default();
    let mut host = FakeHost::new("w1:p1", panes.clone()).with_process("w1:p9", own_proqi());
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
    assert_eq!(
        records.0,
        vec![record(None, OWN)],
        "the tab keeps its session"
    );
}

#[test]
fn a_closed_companion_reopens_its_session_from_any_pane_in_the_tab() {
    // The focused split sits in a subdirectory where a name lookup could
    // conflict; the recorded session is reused without consulting the name.
    let panes = vec![agent("w1:p1", false), shell("w1:p5")];
    let mut host = FakeHost::new("w1:p5", panes);
    let mut records = FakeRecords(vec![record(None, OWN)]);
    let mut sessions = FakeSessions::default();
    let outcome = toggle_companion(&mut host, &mut records, &mut sessions).expect("reopen");
    assert!(matches!(
        outcome,
        CompanionToggleOutcome::Opened { session_id, .. } if session_id == session(OWN)
    ));
    assert!(sessions.ensured.is_empty());
    assert_eq!(host.calls, vec![format!("open w1:p5 /work/sub {OWN}")]);
}

#[test]
fn failed_flush_keeps_the_companion_open_and_reports_it() {
    let panes = vec![agent("w1:p1", false), companion("w1:p9", true)];
    let mut host = FakeHost::new("w1:p9", panes).with_process("w1:p9", own_proqi());
    let mut records = FakeRecords(vec![record(Some("w1:p9"), OWN)]);
    let mut sessions = FakeSessions::default();
    sessions.fail_flush = true;
    let error = toggle_companion(&mut host, &mut records, &mut sessions).expect_err("flush");
    assert!(matches!(error, CompanionToggleError::Session(_)));
    assert!(host.calls.is_empty());
    assert_eq!(records.0, vec![record(Some("w1:p9"), OWN)]);
    assert_eq!(
        host.notifications,
        vec!["owner did not confirm the flush".to_owned()]
    );
}

#[test]
fn a_starting_launcher_counts_as_the_live_companion() {
    let mut launching = companion("w1:p9", false);
    launching.presence = ProqiPresence::Absent;
    let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true), launching])
        .with_process("w1:p9", PaneProcess::Launcher);
    let mut records = FakeRecords(vec![record(Some("w1:p9"), OWN)]);
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
fn a_trashed_recorded_session_falls_back_to_the_named_session() {
    let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true)]);
    let mut records = FakeRecords(vec![record(None, OTHER)]);
    let mut sessions = FakeSessions::with_named("agent-tab", OWN)
        .with_state(OTHER, CompanionSessionState::Unavailable);
    let outcome = toggle_companion(&mut host, &mut records, &mut sessions).expect("open");
    assert!(matches!(
        outcome,
        CompanionToggleOutcome::Opened { session_id, .. } if session_id == session(OWN)
    ));
    assert_eq!(records.0, vec![record(Some("w1:p10"), OWN)]);
}

#[test]
fn a_companion_opened_elsewhere_is_focused_and_toggling_from_it_returns_to_the_agent() {
    let panes = vec![agent("w1:p1", true), companion("w1:p5", false)];
    let mut records = FakeRecords::default();
    let mut sessions = FakeSessions::default();
    let mut host = FakeHost::new("w1:p1", panes.clone());
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

fn other_tab(pane: Option<&str>) -> CompanionRecord {
    CompanionRecord {
        tab_id: "w2:t3".to_owned(),
        pane_id: pane.map(str::to_owned),
        session_id: session(OWN),
    }
}

#[test]
fn a_companion_another_tab_is_launching_or_running_blocks_the_open() {
    for process in [PaneProcess::Launcher, own_proqi()] {
        let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true)])
            .with_process("w2:p4", process.clone());
        let mut records = FakeRecords(vec![other_tab(Some("w2:p4"))]);
        let mut sessions = FakeSessions::with_named("agent-tab", OWN);
        let error = toggle_companion(&mut host, &mut records, &mut sessions)
            .expect_err("never two panes for one session");
        assert!(
            matches!(error, CompanionToggleError::SessionActive { .. }),
            "{process:?}"
        );
        assert!(host.calls.is_empty(), "{process:?}");
    }
}

#[test]
fn unidentified_or_vanished_panes_in_other_tabs_do_not_block_and_are_not_rewritten() {
    for process in [Some(PaneProcess::Proqi { session_id: None }), None] {
        let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true)]);
        if let Some(process) = process.clone() {
            host = host.with_process("w2:p4", process);
        }
        let mut records = FakeRecords(vec![other_tab(Some("w2:p4"))]);
        let mut sessions = FakeSessions::with_named("agent-tab", OWN);
        let outcome = toggle_companion(&mut host, &mut records, &mut sessions).expect("open");
        assert!(matches!(outcome, CompanionToggleOutcome::Opened { .. }));
        assert!(records.0.contains(&other_tab(Some("w2:p4"))), "{process:?}");
    }
}

#[test]
fn records_are_scoped_to_their_own_tab() {
    let mut other = context("w2:p1");
    other.tab_id = "w2:t3".to_owned();
    let mut host = FakeHost::new("w2:p1", vec![agent("w2:p1", true)]).with_context(other);
    let mut records = FakeRecords(vec![record(None, OWN)]);
    let mut sessions = FakeSessions::with_named("agent-tab", OTHER);
    toggle_companion(&mut host, &mut records, &mut sessions).expect("open");
    assert_eq!(
        sessions.ensured,
        vec![("agent-tab".to_owned(), PathBuf::from("/work"))]
    );
}

#[test]
fn an_unclassified_pane_in_another_tabs_record_blocks_and_names_that_pane() {
    let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true)])
        .with_process("w2:p4", PaneProcess::Unknown);
    let mut records = FakeRecords(vec![other_tab(Some("w2:p4"))]);
    let mut sessions = FakeSessions::with_named("agent-tab", OWN);
    let error = toggle_companion(&mut host, &mut records, &mut sessions).expect_err("blocked");
    assert!(matches!(
        &error,
        CompanionToggleError::Unclassified { pane_id } if pane_id == "w2:p4"
    ));
    assert!(host.calls.is_empty());
    assert!(host.notifications[0].contains("w2:p4"));

    // Once Herdr reports the pane again, the same toggle opens normally.
    let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true)])
        .with_process("w2:p4", PaneProcess::Other);
    toggle_companion(&mut host, &mut records, &mut sessions).expect("recovered");
}

#[test]
fn an_unclassified_pane_in_this_tab_blocks_opening_but_not_focusing() {
    let mut hidden = shell("w1:p7");
    hidden.presence = ProqiPresence::Unknown;
    let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true), hidden.clone()]);
    let error = toggle_companion(
        &mut host,
        &mut FakeRecords::default(),
        &mut FakeSessions::with_named("agent-tab", OWN),
    )
    .expect_err("may hide a Proqi");
    assert!(matches!(
        &error,
        CompanionToggleError::Unclassified { pane_id } if pane_id == "w1:p7"
    ));
    assert!(host.calls.is_empty());

    let panes = vec![agent("w1:p1", true), hidden, companion("w1:p9", false)];
    let mut host = FakeHost::new("w1:p1", panes);
    let focused = toggle_companion(
        &mut host,
        &mut FakeRecords::default(),
        &mut FakeSessions::default(),
    )
    .expect("focus still works");
    assert_eq!(
        focused,
        CompanionToggleOutcome::Focused {
            pane_id: "w1:p9".to_owned()
        }
    );
}
