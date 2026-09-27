//! Plugin state records: round trip, per-tab replacement, bounds, corruption,
//! and the lock that serializes toggles.

use std::time::Duration;

use crate::ports::companion::{CompanionError, CompanionRecord, CompanionRecords};

use super::super::{FileCompanionRecords, TOGGLE_WORST_CASE};
use super::SESSION;

fn record(tab: &str, pane: Option<&str>) -> CompanionRecord {
    CompanionRecord {
        tab_id: tab.to_owned(),
        pane_id: pane.map(str::to_owned),
        session_id: SESSION.parse().expect("session"),
    }
}

#[test]
fn records_round_trip_replace_per_tab_and_survive_reopening() {
    let directory = tempfile::tempdir().expect("state");
    {
        let mut records = FileCompanionRecords::acquire(directory.path()).expect("lock");
        assert!(records.all().expect("empty").is_empty());
        records.save(&record("w1:t1", Some("w1:p2"))).expect("save");
        records.save(&record("w2:t1", Some("w2:p2"))).expect("save");
        records
            .save(&record("w1:t1", Some("w1:p9")))
            .expect("replace");
        records
            .save(&record("w2:t1", None))
            .expect("close keeps the session");
    }
    let mut records = FileCompanionRecords::acquire(directory.path()).expect("relock");
    assert_eq!(
        records.all().expect("records"),
        vec![record("w1:t1", Some("w1:p9")), record("w2:t1", None)]
    );
    assert_eq!(
        records.load("w1:t1").expect("load"),
        Some(record("w1:t1", Some("w1:p9")))
    );
}

#[test]
fn unreadable_or_foreign_state_is_treated_as_no_record() {
    for contents in [
        "{",
        "{\"version\":2,\"companions\":[]}",
        "{\"version\":1,\"companions\":[],\"x\":1}",
    ] {
        let directory = tempfile::tempdir().expect("state");
        std::fs::write(directory.path().join("companions.json"), contents).expect("seed");
        let mut records = FileCompanionRecords::acquire(directory.path()).expect("lock");
        assert!(records.all().expect("records").is_empty(), "{contents}");
        records
            .save(&record("w1:t1", Some("w1:p2")))
            .expect("overwrite");
        assert_eq!(records.all().expect("records").len(), 1);
    }
}

#[test]
fn a_second_toggle_waits_for_the_lock_and_then_reports_contention() {
    let directory = tempfile::tempdir().expect("state");
    let _held = FileCompanionRecords::acquire(directory.path()).expect("first");
    let contended = FileCompanionRecords::acquire_within(
        directory.path(),
        std::time::Duration::from_millis(60),
    );
    assert!(matches!(contended, Err(CompanionError::State(_))));
}

#[test]
fn records_are_bounded_by_forgetting_the_oldest_tabs() {
    let directory = tempfile::tempdir().expect("state");
    let mut records = FileCompanionRecords::acquire(directory.path()).expect("lock");
    for index in 0..520 {
        records
            .save(&record(&format!("w1:t{index}"), None))
            .expect("save");
    }
    let all = records.all().expect("records");
    assert_eq!(all.len(), 512);
    assert_eq!(all[0].tab_id, "w1:t8");
    assert_eq!(all[511].tab_id, "w1:t519");
}

#[test]
fn the_lock_wait_outlasts_the_slowest_complete_toggle() {
    assert!(super::super::state::LOCK_TIMEOUT > TOGGLE_WORST_CASE);
    assert!(TOGGLE_WORST_CASE >= Duration::from_secs(40));
}
