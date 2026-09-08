//! Typed failure status retention around automatic Screenshot Inbox pauses.

use super::behavior::app_with_thought;
use crate::{
    ports::editor::CursorMovement,
    ui::{UiInput, UiKey},
};

#[test]
fn ordinary_errors_remain_transient_while_typed_failures_are_retained() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.set_error("ordinary transient error");
    app.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert_eq!(app.status_text(), None);

    app.set_storage_failure("critical storage failure");
    app.set_success("unrelated completion");
    app.set_warning("unrelated warning");
    assert_eq!(app.status_text(), Some("critical storage failure"));
}
