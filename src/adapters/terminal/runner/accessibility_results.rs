//! Bounded application of attachment accessibility worker results.

use std::{sync::mpsc::TryRecvError, time::Instant};

use crate::{adapters::terminal::TerminalError, ui::BoardApp};

use super::{
    PendingWork, WorkerLanes,
    fairness::{DrainOutcome, drain_bounded},
};

pub(super) fn start(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
) -> Result<(), TerminalError> {
    let effects = app.start_attachment_checks(lanes.monotonic.now());
    super::durability::enqueue_effects(app, lanes, effects, pending)
}

pub(super) fn refresh_if_due(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    deadline: &mut Option<Instant>,
) -> Result<(), TerminalError> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        let effects = app.refresh_attachments(false);
        super::durability::enqueue_effects(app, lanes, effects, pending)?;
        *deadline = None;
    }
    Ok(())
}

pub(super) fn drain(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
) -> Result<DrainOutcome, TerminalError> {
    drain_bounded(
        || match lanes.accessibility.receiver.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) if lanes.accessibility.stopped_cleanly() => Ok(None),
            Err(TryRecvError::Disconnected) => {
                Err(lanes
                    .accessibility
                    .worker_failure()
                    .unwrap_or(TerminalError::Worker(
                        "accessibility result lane disconnected",
                    )))
            }
        },
        |result| {
            pending.accessibility = pending.accessibility.saturating_sub(1);
            let effects = app.complete_attachment_checks(result);
            super::durability::enqueue_effects(app, lanes, effects, pending)?;
            Ok(true)
        },
    )
}
