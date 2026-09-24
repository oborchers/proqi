//! Close safety: which panes the toggle may close or replace, and how it
//! behaves when a pane changes, cannot be classified, or cannot be recorded.

use crate::ports::companion::PaneProcess;

use super::fakes::{
    FakeHost, FakeRecords, FakeSessions, UnwritableRecords, agent, companion, record, session,
};
use super::{
    CompanionToggleError, CompanionToggleOutcome, OTHER, OWN, own_proqi, toggle_companion,
};

fn dead(pane: &str) -> crate::ports::companion::PaneObservation {
    let mut pane = companion(pane, false);
    pane.presence = crate::ports::companion::ProqiPresence::Absent;
    pane
}

fn closed_panes(host: &FakeHost) -> Vec<&String> {
    host.calls
        .iter()
        .filter(|call| call.starts_with("close"))
        .collect()
}

#[test]
fn a_dead_companion_after_restart_is_replaced_with_the_same_session() {
    let panes = vec![agent("w1:p1", true), dead("w1:p9")];
    let mut host = FakeHost::new("w1:p1", panes).with_process("w1:p9", PaneProcess::IdleShell);
    let mut records = FakeRecords(vec![record(Some("w1:p9"), OWN)]);
    let mut sessions = FakeSessions::default();
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
        vec![
            format!("open w1:p1 /work/sub {OWN}"),
            "close w1:p9".to_owned()
        ]
    );
    assert_eq!(records.0, vec![record(Some("w1:p10"), OWN)]);
}

#[test]
fn focusing_the_dead_pane_itself_splits_beside_the_agent_before_closing_it() {
    let panes = vec![agent("w1:p1", false), dead("w1:p9")];
    let mut host = FakeHost::new("w1:p9", panes).with_process("w1:p9", PaneProcess::IdleShell);
    let mut records = FakeRecords(vec![record(Some("w1:p9"), OWN)]);
    toggle_companion(&mut host, &mut records, &mut FakeSessions::default()).expect("replace");
    assert_eq!(
        host.calls,
        vec![
            format!("open w1:p1 /work/sub {OWN}"),
            "close w1:p9".to_owned()
        ]
    );
}

#[test]
fn a_dead_shell_that_started_work_while_the_replacement_opened_is_kept() {
    for changed in [PaneProcess::Other, PaneProcess::Unknown] {
        let panes = vec![agent("w1:p1", true), dead("w1:p9")];
        let mut host = FakeHost::new("w1:p1", panes)
            .with_processes("w1:p9", vec![PaneProcess::IdleShell, changed.clone()]);
        let mut records = FakeRecords(vec![record(Some("w1:p9"), OWN)]);
        let outcome =
            toggle_companion(&mut host, &mut records, &mut FakeSessions::default()).expect("open");
        assert!(
            matches!(
                outcome,
                CompanionToggleOutcome::Opened {
                    replaced_pane_id: None,
                    ..
                }
            ),
            "{changed:?}"
        );
        assert!(closed_panes(&host).is_empty(), "{changed:?}");
        assert_eq!(records.0, vec![record(Some("w1:p10"), OWN)]);
    }
}

#[test]
fn a_recorded_pane_that_now_runs_anything_else_is_never_closed() {
    for process in [
        PaneProcess::Other,
        PaneProcess::Proqi {
            session_id: Some(session(OTHER)),
        },
        PaneProcess::Proqi { session_id: None },
    ] {
        let panes = vec![agent("w1:p1", true), dead("w1:p9")];
        let mut host = FakeHost::new("w1:p1", panes).with_process("w1:p9", process);
        let mut records = FakeRecords(vec![record(Some("w1:p9"), OWN)]);
        toggle_companion(&mut host, &mut records, &mut FakeSessions::default()).expect("open");
        assert_eq!(host.calls, vec![format!("open w1:p1 /work/sub {OWN}")]);
        assert_eq!(records.0, vec![record(Some("w1:p10"), OWN)]);
    }
}

#[test]
fn a_focused_recorded_pane_running_an_unidentified_proqi_is_never_closed() {
    let panes = vec![agent("w1:p1", false), companion("w1:p9", true)];
    let mut host = FakeHost::new("w1:p9", panes)
        .with_process("w1:p9", PaneProcess::Proqi { session_id: None });
    let mut records = FakeRecords(vec![record(Some("w1:p9"), OWN)]);
    let mut sessions = FakeSessions::default();
    let outcome = toggle_companion(&mut host, &mut records, &mut sessions).expect("return");
    assert_eq!(
        outcome,
        CompanionToggleOutcome::Returned {
            pane_id: "w1:p1".to_owned()
        }
    );
    assert!(sessions.flushed.is_empty());
    assert!(closed_panes(&host).is_empty());
}

#[test]
fn an_unclassifiable_recorded_pane_is_focused_never_closed_or_duplicated() {
    let panes = vec![agent("w1:p1", true), dead("w1:p9")];
    let mut host =
        FakeHost::new("w1:p1", panes.clone()).with_process("w1:p9", PaneProcess::Unknown);
    let mut records = FakeRecords(vec![record(Some("w1:p9"), OWN)]);
    let outcome =
        toggle_companion(&mut host, &mut records, &mut FakeSessions::default()).expect("focus");
    assert_eq!(
        outcome,
        CompanionToggleOutcome::Focused {
            pane_id: "w1:p9".to_owned()
        }
    );
    let mut host = FakeHost::new("w1:p9", panes).with_process("w1:p9", PaneProcess::Unknown);
    let outcome =
        toggle_companion(&mut host, &mut records, &mut FakeSessions::default()).expect("return");
    assert_eq!(
        outcome,
        CompanionToggleOutcome::Returned {
            pane_id: "w1:p1".to_owned()
        }
    );
    assert!(closed_panes(&host).is_empty());
}

#[test]
fn an_idle_shell_that_lost_the_companion_label_is_left_alone() {
    let mut renamed = dead("w1:p9");
    renamed.label = Some("scratch".to_owned());
    let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true), renamed])
        .with_process("w1:p9", PaneProcess::IdleShell);
    let mut records = FakeRecords(vec![record(Some("w1:p9"), OWN)]);
    toggle_companion(&mut host, &mut records, &mut FakeSessions::default()).expect("open");
    assert!(closed_panes(&host).is_empty());
}

#[test]
fn a_display_lease_on_a_recorded_idle_shell_keeps_it_from_being_closed() {
    let panes = vec![agent("w1:p1", true), companion("w1:p9", false)];
    let mut host = FakeHost::new("w1:p1", panes).with_process("w1:p9", PaneProcess::IdleShell);
    let mut records = FakeRecords(vec![record(Some("w1:p9"), OWN)]);
    let outcome =
        toggle_companion(&mut host, &mut records, &mut FakeSessions::default()).expect("focus");
    assert_eq!(
        outcome,
        CompanionToggleOutcome::Focused {
            pane_id: "w1:p9".to_owned()
        }
    );
    assert!(closed_panes(&host).is_empty());
}

#[test]
fn a_failed_open_keeps_the_dead_pane_and_its_record() {
    let panes = vec![agent("w1:p1", true), dead("w1:p9")];
    let mut host = FakeHost::new("w1:p1", panes).with_process("w1:p9", PaneProcess::IdleShell);
    host.fail_open = true;
    let mut records = FakeRecords(vec![record(Some("w1:p9"), OWN)]);
    let error = toggle_companion(&mut host, &mut records, &mut FakeSessions::default())
        .expect_err("open fails");
    assert!(matches!(error, CompanionToggleError::Host(_)));
    assert_eq!(host.calls, vec![format!("open w1:p1 /work/sub {OWN}")]);
    assert_eq!(records.0, vec![record(Some("w1:p9"), OWN)]);
}

#[test]
fn an_unrecordable_open_keeps_the_new_pane_and_the_dead_pane() {
    let panes = vec![agent("w1:p1", true), dead("w1:p9")];
    let mut host = FakeHost::new("w1:p1", panes).with_process("w1:p9", PaneProcess::IdleShell);
    let mut records = UnwritableRecords(vec![record(Some("w1:p9"), OWN)]);
    let error = toggle_companion(&mut host, &mut records, &mut FakeSessions::default())
        .expect_err("save fails");
    assert!(matches!(error, CompanionToggleError::Host(_)));
    assert_eq!(host.calls, vec![format!("open w1:p1 /work/sub {OWN}")]);
    assert_eq!(host.notifications.len(), 1);
}

#[test]
fn after_a_failed_save_the_unrecorded_proqi_is_focused_and_nothing_is_closed() {
    let panes = vec![
        agent("w1:p1", true),
        dead("w1:p9"),
        companion("w1:p10", false),
    ];
    let mut host = FakeHost::new("w1:p1", panes).with_process("w1:p9", PaneProcess::IdleShell);
    let mut records = FakeRecords(vec![record(Some("w1:p9"), OWN)]);
    let outcome =
        toggle_companion(&mut host, &mut records, &mut FakeSessions::default()).expect("focus");
    assert_eq!(
        outcome,
        CompanionToggleOutcome::Focused {
            pane_id: "w1:p10".to_owned()
        }
    );
    assert_eq!(host.calls, vec!["focus w1:p10".to_owned()]);
}

#[test]
fn the_companion_label_matches_the_manifest_title() {
    assert_eq!(super::super::COMPANION_PANE_LABEL, "Proqi");
    let _ = own_proqi();
}
