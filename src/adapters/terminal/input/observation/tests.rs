//! Deterministic stage and timing evidence independent of scheduling and content.

use std::time::{Duration, Instant};

use super::{InputReaderStage, InputStallEvidence, LeaseDecision, ReaderProgress, SourceLease};

#[test]
fn read_stall_keeps_last_completed_poll_separate_from_current_stage_and_lease() {
    let start = Instant::now();
    let reader = ReaderProgress::new(start);
    let mut lease = SourceLease::new(start);
    reader.enter(InputReaderStage::Poll, start);
    reader.complete(start + Duration::from_millis(40));
    reader.enter(InputReaderStage::Read, start + Duration::from_millis(41));
    assert_eq!(
        lease.observe(start + Duration::from_millis(450), false),
        LeaseDecision::Continue
    );
    let evidence = lease.evidence(start + Duration::from_millis(500), &reader);
    assert_eq!(
        evidence,
        InputStallEvidence {
            reader_stage: Some(InputReaderStage::Read),
            reader_stage_elapsed_ms: Some(459),
            last_completed_stage: Some(InputReaderStage::Poll),
            last_completed_gap_ms: Some(460),
            lease_gap_ms: 500,
            observer_gap_ms: 50,
        }
    );
}

#[test]
fn lease_reset_does_not_invent_completed_reader_progress() {
    let start = Instant::now();
    let reader = ReaderProgress::new(start);
    reader.enter(InputReaderStage::Poll, start);
    let mut lease = SourceLease::new(start);
    let resumed = start + Duration::from_secs(2);
    assert!(matches!(
        lease.observe(resumed, false),
        LeaseDecision::ResetAfterSupervisorGap { .. }
    ));
    let evidence = lease.evidence(resumed + Duration::from_millis(50), &reader);
    assert_eq!(evidence.last_completed_gap_ms, None);
    assert_eq!(evidence.last_completed_stage, None);
    assert_eq!(evidence.reader_stage_elapsed_ms, Some(2050));
    assert_eq!(evidence.lease_gap_ms, 50);
    assert_eq!(evidence.observer_gap_ms, 50);
}

#[test]
fn delivery_and_stopped_stages_preserve_the_actual_last_completion() {
    let start = Instant::now();
    let reader = ReaderProgress::new(start);
    let lease = SourceLease::new(start);
    reader.enter(InputReaderStage::Read, start);
    reader.complete(start + Duration::from_millis(1));
    reader.enter(InputReaderStage::Delivery, start + Duration::from_millis(2));
    let evidence = lease.evidence(start + Duration::from_millis(100), &reader);
    assert_eq!(evidence.reader_stage, Some(InputReaderStage::Delivery));
    assert_eq!(evidence.last_completed_stage, Some(InputReaderStage::Read));
    assert_eq!(evidence.last_completed_gap_ms, Some(99));
    reader.complete(start + Duration::from_millis(101));
    reader.enter(
        InputReaderStage::Stopped,
        start + Duration::from_millis(102),
    );
    let evidence = lease.evidence(start + Duration::from_millis(103), &reader);
    assert_eq!(evidence.reader_stage, Some(InputReaderStage::Stopped));
    assert_eq!(
        evidence.last_completed_stage,
        Some(InputReaderStage::Delivery)
    );
    assert_eq!(evidence.last_completed_gap_ms, Some(2));
}

#[test]
fn unavailable_snapshot_reports_unknown_without_blocking_or_guessing() {
    let start = Instant::now();
    let reader = ReaderProgress::new(start);
    let _held = reader.0.lock().expect("hold instrumentation lock");
    let evidence = SourceLease::new(start).evidence(start + Duration::from_millis(500), &reader);
    assert_eq!(
        evidence,
        InputStallEvidence {
            reader_stage: None,
            reader_stage_elapsed_ms: None,
            last_completed_stage: None,
            last_completed_gap_ms: None,
            lease_gap_ms: 500,
            observer_gap_ms: 500,
        }
    );
}
