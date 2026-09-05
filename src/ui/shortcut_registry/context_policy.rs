//! Context-owned modifier normalization and effective Board overrides.

use std::collections::BTreeMap;

use crate::{ports::editor::CursorMovement, ui::settings::KeyBindings};

use super::{intentions::opposite_ascii_case, model::ShortcutActionId as Action};

pub(super) fn board_navigation_action(
    movement: CursorMovement,
    extend: bool,
    reorder: bool,
) -> Option<Action> {
    let previous = matches!(
        movement,
        CursorMovement::VisualUp | CursorMovement::VisualJumpUp | CursorMovement::DocumentStart
    );
    let next = matches!(
        movement,
        CursorMovement::VisualDown | CursorMovement::VisualJumpDown | CursorMovement::DocumentEnd
    );
    if !previous && !next {
        return None;
    }
    Some(match (previous, extend, reorder) {
        (true, _, true) => Action::MoveUp,
        (false, _, true) => Action::MoveDown,
        (true, true, false) => Action::ExtendPrevious,
        (false, true, false) => Action::ExtendNext,
        (true, false, false) => Action::FocusPrevious,
        (false, false, false) => Action::FocusNext,
    })
}

pub(super) fn effective_board_bindings(keys: &KeyBindings) -> BTreeMap<char, Action> {
    let mut bindings = BTreeMap::new();
    for (key, action) in [
        (keys.new, Action::New),
        (keys.edit, Action::Edit),
        (keys.delete, Action::Delete),
        (keys.copy, Action::Copy),
        (keys.cut, Action::Cut),
        (keys.submit_remove, Action::SubmitRemove),
        (keys.submit_keep, Action::SubmitKeep),
        (keys.undo, Action::Undo),
        (keys.focus_up, Action::FocusPrevious),
        (keys.focus_down, Action::FocusNext),
        (keys.range_up, Action::ExtendPrevious),
        (keys.range_down, Action::ExtendNext),
        (keys.collapse, Action::Collapse),
        (keys.select, Action::Select),
        (keys.select_all, Action::SelectAll),
        (keys.range_select, Action::RangeSelect),
        (keys.search, Action::OpenSearch),
        (keys.commands, Action::OpenCommands),
        (keys.help, Action::Help),
        (keys.quit, Action::Quit),
        (keys.screenshot_inbox, Action::ScreenshotInbox),
    ] {
        bindings.entry(key).or_insert(action);
    }
    bindings
        .entry(keys.transform)
        .or_insert(Action::ContextualTransform);
    bindings.entry(keys.paste).or_insert(Action::PasteExact);
    if let Some(reflow) = opposite_ascii_case(keys.paste) {
        bindings.entry(reflow).or_insert(Action::PasteReflow);
    }
    bindings
}
