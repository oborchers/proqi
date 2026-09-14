//! One bounded pass across every asynchronous worker result lane.

use crate::{
    adapters::runtime::{FileSchemaLease, SystemClock, SystemIdGenerator},
    ui::BoardApp,
};

use super::{
    CaptureRuntime, PaneHeartbeat, PendingWork, TerminalError, WorkerLanes, accessibility_results,
    durability::drain_persistence, external_results, owner_control, screenshot_results,
    update_results,
};

pub(super) struct DrainContext<'a> {
    ids: &'a mut SystemIdGenerator,
    clock: SystemClock,
    pane_heartbeat: &'a mut Option<PaneHeartbeat>,
    schema_lease: &'a mut Option<FileSchemaLease>,
}

impl<'a> DrainContext<'a> {
    pub(super) fn new(
        ids: &'a mut SystemIdGenerator,
        clock: SystemClock,
        pane_heartbeat: &'a mut Option<PaneHeartbeat>,
        schema_lease: &'a mut Option<FileSchemaLease>,
    ) -> Self {
        Self {
            ids,
            clock,
            pane_heartbeat,
            schema_lease,
        }
    }
}

pub(super) fn drain(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    capture: &mut CaptureRuntime,
    context: &mut DrainContext<'_>,
) -> Result<(bool, bool), TerminalError> {
    let persistence = drain_persistence(app, lanes, pending, context.ids, &context.clock)?;
    let accessibility = accessibility_results::drain(app, lanes, pending)?;
    let external = external_results::drain(
        app,
        lanes,
        pending,
        context.ids,
        context.clock,
        context.pane_heartbeat,
    )?;
    let control = owner_control::drain(
        app,
        lanes,
        pending,
        capture,
        context.ids,
        context.clock,
        context.schema_lease,
    )?;
    let update = update_results::drain(app, lanes, pending)?;
    let screenshot =
        screenshot_results::drain(app, lanes, pending, capture, lanes.monotonic.now())?;
    let changed = persistence.changed
        || accessibility.changed
        || external.changed
        || control.changed
        || update.changed
        || screenshot.changed;
    let backlog = persistence.budget_exhausted
        || accessibility.budget_exhausted
        || external.budget_exhausted
        || control.budget_exhausted
        || update.budget_exhausted
        || screenshot.budget_exhausted;
    Ok((changed, backlog))
}
