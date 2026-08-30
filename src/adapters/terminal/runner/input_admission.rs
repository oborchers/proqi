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

const ATTACHMENT_FOCUS_DEBOUNCE: Duration = Duration::from_millis(180);

#[derive(Default)]
pub(super) struct RefreshDeadlines {
    pub(super) agent: Option<Instant>,
    pub(super) invocation: Option<Instant>,
    pub(super) attachment: Option<Instant>,
}

pub(super) fn refresh_if_due(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    deadlines: &mut RefreshDeadlines,
) -> Result<(), TerminalError> {
    let now = Instant::now();
    if deadlines.agent.is_some_and(|deadline| now >= deadline) {
        enqueue_effects(app, lanes, BoardApp::discover_agents(), pending)?;
        deadlines.agent = None;
    }
    if deadlines.invocation.is_some_and(|deadline| now >= deadline) {
        let effects = app.refresh_invocations();
        enqueue_effects(app, lanes, effects, pending)?;
        deadlines.invocation = None;
    }
    super::accessibility_results::refresh_if_due(app, lanes, pending, &mut deadlines.attachment)
}

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
    deadlines: &mut RefreshDeadlines,
    sequence: u64,
    event: UiInput,
) -> Result<(), TerminalError> {
    if !app.accept_update_input(sequence) {
        return Ok(());
    }
    if matches!(event, UiInput::Resize { .. }) {
        deadlines.agent = Some(Instant::now() + Duration::from_millis(250));
    }
    if matches!(event, UiInput::HostFocusGained) {
        deadlines.invocation = Some(Instant::now() + Duration::from_millis(180));
    }
    schedule_attachment_focus_refresh(&event, Instant::now(), &mut deadlines.attachment);
    app.note_screenshot_activity(&event, lanes.monotonic.now());
    if event.is_deliberate_interaction() {
        let effects = app.note_attachment_interaction(lanes.monotonic.now());
        enqueue_effects(app, lanes, effects, pending)?;
    }
    let effects = app.handle(event, ids, &clock);
    enqueue_effects(app, lanes, effects, pending)
}

fn schedule_attachment_focus_refresh(
    event: &UiInput,
    now: Instant,
    deadline: &mut Option<Instant>,
) {
    if matches!(event, UiInput::HostFocusGained) {
        *deadline = Some(now + ATTACHMENT_FOCUS_DEBOUNCE);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crate::ui::UiInput;

    use super::{ATTACHMENT_FOCUS_DEBOUNCE, schedule_attachment_focus_refresh};

    #[test]
    fn host_focus_is_debounced_and_passive_resize_does_not_change_the_deadline() {
        let start = Instant::now();
        let mut deadline = None;
        schedule_attachment_focus_refresh(&UiInput::HostFocusGained, start, &mut deadline);
        assert_eq!(deadline, Some(start + ATTACHMENT_FOCUS_DEBOUNCE));

        let repeated = start + Duration::from_millis(100);
        schedule_attachment_focus_refresh(&UiInput::HostFocusGained, repeated, &mut deadline);
        assert_eq!(deadline, Some(repeated + ATTACHMENT_FOCUS_DEBOUNCE));

        schedule_attachment_focus_refresh(
            &UiInput::Resize {
                width: 80,
                height: 24,
            },
            repeated + Duration::from_millis(1),
            &mut deadline,
        );
        assert_eq!(deadline, Some(repeated + ATTACHMENT_FOCUS_DEBOUNCE));
    }
}
