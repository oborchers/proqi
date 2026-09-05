//! Reader heartbeat lease and bounded stalled-source supervision.

use std::{
    collections::VecDeque,
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crossterm::event::Event;

use crate::adapters::terminal::supervisor::ShutdownDeadline;

use super::super::{
    EventSource, InputFailure, InputLane, InputMessage, LeaseDecision, MONITOR_INTERVAL,
    SOURCE_STALL_LIMIT, SourceLease,
};

struct FakeSource {
    polls: VecDeque<io::Result<bool>>,
    reads: VecDeque<io::Result<Event>>,
    delay: Duration,
    entered: Option<Arc<AtomicBool>>,
}

impl EventSource for FakeSource {
    fn poll(&mut self, _timeout: Duration) -> io::Result<bool> {
        if let Some(entered) = &self.entered {
            entered.store(true, Ordering::Release);
        }
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        self.polls.pop_front().unwrap_or(Ok(false))
    }

    fn read(&mut self) -> io::Result<Event> {
        self.reads
            .pop_front()
            .unwrap_or_else(|| Err(io::Error::other("missing fake event")))
    }
}

fn source_with_poll(result: io::Result<bool>) -> Box<dyn EventSource> {
    Box::new(FakeSource {
        polls: VecDeque::from([result]),
        reads: VecDeque::new(),
        delay: Duration::ZERO,
        entered: None,
    })
}

#[test]
fn eof_and_revoked_terminal_errors_remain_typed() {
    for (error, expected) in [
        (
            io::Error::new(io::ErrorKind::UnexpectedEof, "closed"),
            InputFailure::EndOfFile,
        ),
        (
            io::Error::from_raw_os_error(5),
            InputFailure::TerminalRevoked,
        ),
    ] {
        let lane = InputLane::spawn_with_source(source_with_poll(Err(error)));
        let InputMessage::Failed(actual) = lane
            .receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("typed input failure")
        else {
            panic!("expected a failure message");
        };
        assert_eq!(actual, expected);
        lane.stop(ShutdownDeadline::after(Duration::from_secs(1)))
            .expect("failed input lane stops");
    }
}

#[test]
fn nonresponsive_source_cannot_make_stop_wait_without_bound() {
    let entered = Arc::new(AtomicBool::new(false));
    let lane = InputLane::spawn_with_source(Box::new(FakeSource {
        polls: VecDeque::new(),
        reads: VecDeque::new(),
        delay: Duration::from_millis(200),
        entered: Some(Arc::clone(&entered)),
    }));
    while !entered.load(Ordering::Acquire) {
        std::thread::yield_now();
    }
    let started = Instant::now();
    let result = lane.stop(ShutdownDeadline::after(Duration::from_millis(10)));
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_millis(100));
}

#[test]
fn stalled_registry_event_source_becomes_a_typed_failure() {
    let lane = InputLane::spawn_with_source(Box::new(FakeSource {
        polls: VecDeque::new(),
        reads: VecDeque::new(),
        delay: Duration::from_secs(2),
        entered: None,
    }));
    let InputMessage::Failed(failure) = lane
        .receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("stalled source failure")
    else {
        panic!("expected a failure message");
    };
    assert_eq!(failure, InputFailure::Unresponsive);
    lane.stop(ShutdownDeadline::after(Duration::from_secs(1)))
        .expect("input supervisor stops without the stalled reader");
}

#[test]
fn process_pause_resets_the_reader_lease() {
    let start = Instant::now();
    let mut lease = SourceLease::new(start);
    let decision = lease.observe(start + SOURCE_STALL_LIMIT, false);
    assert_eq!(
        decision,
        LeaseDecision::ResetAfterSupervisorGap {
            gap: SOURCE_STALL_LIMIT
        }
    );
}

#[test]
fn supervisor_gap_above_the_stall_limit_resets_the_reader_lease() {
    let start = Instant::now();
    let mut lease = SourceLease::new(start);
    let gap = SOURCE_STALL_LIMIT + Duration::from_nanos(1);
    assert_eq!(
        lease.observe(start + gap, false),
        LeaseDecision::ResetAfterSupervisorGap { gap }
    );
}

#[test]
fn supervisor_gap_below_the_stall_limit_cannot_reset_expired_silence() {
    let start = Instant::now();
    let mut lease = SourceLease::new(start);
    let below = SOURCE_STALL_LIMIT
        .checked_sub(Duration::from_nanos(1))
        .expect("stall limit exceeds one nanosecond");
    assert_eq!(lease.observe(start + below, false), LeaseDecision::Continue);
    assert_eq!(
        lease.observe(start + below + below, false),
        LeaseDecision::Unresponsive
    );
}

#[test]
fn reader_only_stall_remains_unresponsive_at_the_existing_boundary() {
    let start = Instant::now();
    let mut lease = SourceLease::new(start);
    let mut elapsed = MONITOR_INTERVAL;
    while elapsed < SOURCE_STALL_LIMIT {
        assert_eq!(
            lease.observe(start + elapsed, false),
            LeaseDecision::Continue
        );
        elapsed += MONITOR_INTERVAL;
    }
    assert_eq!(
        lease.observe(start + SOURCE_STALL_LIMIT, false),
        LeaseDecision::Unresponsive
    );
}

#[test]
fn post_resume_reader_gets_exactly_one_fresh_bounded_lease() {
    let start = Instant::now();
    let mut lease = SourceLease::new(start);
    let resumed = start + SOURCE_STALL_LIMIT + MONITOR_INTERVAL;
    assert!(matches!(
        lease.observe(resumed, false),
        LeaseDecision::ResetAfterSupervisorGap { .. }
    ));
    let mut elapsed = MONITOR_INTERVAL;
    while elapsed < SOURCE_STALL_LIMIT {
        assert_eq!(
            lease.observe(resumed + elapsed, false),
            LeaseDecision::Continue
        );
        elapsed += MONITOR_INTERVAL;
    }
    assert_eq!(
        lease.observe(resumed + SOURCE_STALL_LIMIT, false),
        LeaseDecision::Unresponsive
    );
}

#[test]
fn reader_response_after_a_pause_is_fresh_liveness_evidence() {
    let start = Instant::now();
    let mut lease = SourceLease::new(start);
    let resumed = start + SOURCE_STALL_LIMIT + MONITOR_INTERVAL;
    assert_eq!(lease.observe(resumed, true), LeaseDecision::Continue);
    let just_before_expiry = resumed
        + SOURCE_STALL_LIMIT
            .checked_sub(Duration::from_nanos(1))
            .expect("stall limit exceeds one nanosecond");
    assert_eq!(
        lease.observe(just_before_expiry, false),
        LeaseDecision::Continue
    );
    assert_eq!(
        lease.observe(resumed + SOURCE_STALL_LIMIT, false),
        LeaseDecision::Unresponsive
    );
}

#[test]
fn repeated_process_pauses_each_replace_stale_silence() {
    let start = Instant::now();
    let mut lease = SourceLease::new(start);
    let first = start + SOURCE_STALL_LIMIT;
    assert!(matches!(
        lease.observe(first, false),
        LeaseDecision::ResetAfterSupervisorGap { .. }
    ));
    assert_eq!(
        lease.observe(first + MONITOR_INTERVAL, false),
        LeaseDecision::Continue
    );
    let second = first + MONITOR_INTERVAL + SOURCE_STALL_LIMIT;
    assert!(matches!(
        lease.observe(second, false),
        LeaseDecision::ResetAfterSupervisorGap { .. }
    ));
    assert_eq!(
        lease.observe(second + MONITOR_INTERVAL, false),
        LeaseDecision::Continue
    );
}

#[test]
fn monitor_and_stall_threshold_boundaries_are_deterministic() {
    let start = Instant::now();
    let mut lease = SourceLease::new(start);
    let just_below_monitor = MONITOR_INTERVAL
        .checked_sub(Duration::from_nanos(1))
        .expect("monitor interval exceeds one nanosecond");
    let just_below_stall = SOURCE_STALL_LIMIT
        .checked_sub(Duration::from_nanos(1))
        .expect("stall limit exceeds one nanosecond");
    assert_eq!(
        lease.observe(start + just_below_monitor, false),
        LeaseDecision::Continue
    );
    assert_eq!(
        lease.observe(start + MONITOR_INTERVAL, false),
        LeaseDecision::Continue
    );
    assert_eq!(
        lease.observe(start + just_below_stall, false),
        LeaseDecision::Continue
    );
    assert_eq!(
        lease.observe(start + SOURCE_STALL_LIMIT, false),
        LeaseDecision::Unresponsive
    );
}
