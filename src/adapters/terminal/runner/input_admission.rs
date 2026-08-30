//! Lossless UI input admission while durable screenshot work is serialized.

use std::time::{Duration, Instant};

use crate::{
    adapters::{
        runtime::{SystemClock, SystemIdGenerator},
        terminal::TerminalError,
    },
    ui::{BoardApp, UiInput},
};

use super::{PendingWork, WorkerLanes, durability::enqueue_effects};

#[expect(
    clippy::too_many_arguments,
    reason = "one input admission helper keeps the lossless held-input path identical"
)]
pub(super) fn apply(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
    pending: &mut PendingWork,
    agent_deadline: &mut Option<Instant>,
    invocation_deadline: &mut Option<Instant>,
    sequence: u64,
    event: UiInput,
) -> Result<(), TerminalError> {
    if !app.accept_update_input(sequence) || !app.accept_release_highlights_input(sequence) {
        return Ok(());
    }
    if matches!(event, UiInput::Resize { .. }) {
        *agent_deadline = Some(Instant::now() + Duration::from_millis(250));
    }
    if matches!(event, UiInput::HostFocusGained) {
        *invocation_deadline = Some(Instant::now() + Duration::from_millis(180));
    }
    app.note_screenshot_activity(&event, lanes.monotonic.now());
    let effects = app.handle(event, ids, &clock);
    enqueue_effects(app, lanes, effects, pending)
}
