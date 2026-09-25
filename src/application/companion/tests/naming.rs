//! Session naming for a tab's first companion: the tab's only named agent,
//! else a meaningful tab label, else the stable tab identity.

use std::path::PathBuf;

use super::super::{CompanionToggleError, companion_session_name, toggle_companion};
use super::OWN;
use super::fakes::{FakeHost, FakeRecords, FakeSessions, agent, context, record};

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn a_meaningful_tab_label_names_the_session() {
    let mut named = context("w1:p1");
    named.tab_label = Some("  herdr-plugin ".to_owned());
    assert_eq!(companion_session_name(&named, &[]), "herdr-plugin");
}

#[test]
fn the_tabs_only_named_agent_names_the_session_over_any_label() {
    let mut labeled = context("w1:p1");
    labeled.tab_label = Some("review".to_owned());
    assert_eq!(
        companion_session_name(&labeled, &names(&[" api-claude "])),
        "api-claude"
    );
    let mut numeric = labeled.clone();
    numeric.tab_label = Some("3".to_owned());
    assert_eq!(
        companion_session_name(&numeric, &names(&["api-claude"])),
        "api-claude"
    );
}

#[test]
fn several_named_agents_or_blank_names_fall_back_to_the_label() {
    let mut labeled = context("w1:p1");
    labeled.tab_label = Some("review".to_owned());
    assert_eq!(
        companion_session_name(&labeled, &names(&["api-claude", "api-codex"])),
        "review"
    );
    assert_eq!(companion_session_name(&labeled, &names(&["  "])), "review");
    assert_eq!(
        companion_session_name(&labeled, &names(&["", "api-codex"])),
        "api-codex",
        "a blank name does not count as a second named agent"
    );
}

#[test]
fn default_numeric_labels_use_the_stable_tab_identity_not_the_position() {
    // Herdr labels unnamed tabs by position, so tab w1:t4 shown second and tab
    // w1:t2 shown second at another time must still get different names.
    let mut fourth = context("w1:p1");
    fourth.tab_id = "w1:t4".to_owned();
    fourth.tab_label = Some("2".to_owned());
    let mut second = fourth.clone();
    second.tab_id = "w1:t2".to_owned();
    assert_eq!(companion_session_name(&fourth, &[]), "demo-w1-t4");
    assert_eq!(companion_session_name(&second, &[]), "demo-w1-t2");
    // Closing or reordering tabs changes the label but not the name.
    let mut moved = fourth.clone();
    moved.tab_label = Some("1".to_owned());
    assert_eq!(
        companion_session_name(&moved, &[]),
        companion_session_name(&fourth, &[])
    );
}

#[test]
fn duplicate_workspace_labels_still_yield_distinct_default_names() {
    let mut first = context("w1:p1");
    first.tab_label = None;
    let mut second = first.clone();
    second.workspace_id = "w2".to_owned();
    second.tab_id = "w2:t1".to_owned();
    assert_ne!(
        companion_session_name(&first, &[]),
        companion_session_name(&second, &[])
    );
    let mut blank = first.clone();
    blank.workspace_label = Some(" ".to_owned());
    assert_eq!(companion_session_name(&blank, &[]), "w1-w1-t1");
}

#[test]
fn a_first_open_ensures_the_session_named_after_the_tabs_agent() {
    let mut host =
        FakeHost::new("w1:p1", vec![agent("w1:p1", true)]).with_agent_names(&["api-claude"]);
    let mut records = FakeRecords::default();
    let mut sessions = FakeSessions::with_named("api-claude", OWN);
    toggle_companion(&mut host, &mut records, &mut sessions).expect("toggle");
    assert_eq!(
        sessions.ensured,
        vec![("api-claude".to_owned(), PathBuf::from("/work"))]
    );
    assert_eq!(host.agent_queries, 1);
}

#[test]
fn a_recorded_session_reopens_without_asking_for_agent_names() {
    let mut host =
        FakeHost::new("w1:p1", vec![agent("w1:p1", true)]).with_agent_names(&["api-claude"]);
    let mut records = FakeRecords(vec![record(None, OWN)]);
    let mut sessions = FakeSessions::default();
    toggle_companion(&mut host, &mut records, &mut sessions).expect("toggle");
    assert!(sessions.ensured.is_empty());
    assert_eq!(host.agent_queries, 0);
}

#[test]
fn a_failed_agent_query_opens_nothing_and_records_nothing() {
    // Falling back to the label would record a different session for good.
    let mut host = FakeHost::new("w1:p1", vec![agent("w1:p1", true)]);
    host.agent_names = None;
    let mut records = FakeRecords::default();
    let mut sessions = FakeSessions::with_named("agent-tab", OWN);
    let error = toggle_companion(&mut host, &mut records, &mut sessions).expect_err("fails");
    assert!(matches!(error, CompanionToggleError::Host(_)), "{error}");
    assert!(sessions.ensured.is_empty());
    assert!(host.calls.is_empty(), "nothing opened: {:?}", host.calls);
    assert!(records.0.is_empty());
    assert_eq!(host.notifications.len(), 1, "the failure is reported");
}
