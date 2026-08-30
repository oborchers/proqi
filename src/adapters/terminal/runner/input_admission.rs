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
    attachment_deadline: &mut Option<Instant>,
    sequence: u64,
    event: UiInput,
) -> Result<(), TerminalError> {
    if !app.accept_update_input(sequence) {
        return Ok(());
    }
    if matches!(event, UiInput::Resize { .. }) {
        *agent_deadline = Some(Instant::now() + Duration::from_millis(250));
    }
    if matches!(event, UiInput::HostFocusGained) {
        *invocation_deadline = Some(Instant::now() + Duration::from_millis(180));
    }
    schedule_attachment_focus_refresh(&event, Instant::now(), attachment_deadline);
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
