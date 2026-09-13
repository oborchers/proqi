//! Exact platform defaults that intentionally differ from the shared modifier ladder.

use crate::ui::{LogicalKey, LogicalModifiers};

use super::super::{Action, Context};
use crate::ui::shortcut_registry::model::ShortcutBindingPresentation;

const OPTION_SHIFT: LogicalModifiers = LogicalModifiers::ALT.union(LogicalModifiers::SHIFT);
const CONTROL_SHIFT: LogicalModifiers = LogicalModifiers::CONTROL.union(LogicalModifiers::SHIFT);
const ALT_SHIFT: LogicalModifiers = LogicalModifiers::ALT.union(LogicalModifiers::SHIFT);

const MACOS_BOARD_DEFAULTS: &[(LogicalKey, Action, ShortcutBindingPresentation)] = &[
    (
        LogicalKey::Up,
        Action::MoveUp,
        ShortcutBindingPresentation::Explicit,
    ),
    (
        LogicalKey::Character('k'),
        Action::MoveUp,
        ShortcutBindingPresentation::DispatchOnly,
    ),
    (
        LogicalKey::Character('K'),
        Action::MoveUp,
        ShortcutBindingPresentation::DispatchOnly,
    ),
    (
        LogicalKey::Down,
        Action::MoveDown,
        ShortcutBindingPresentation::Explicit,
    ),
    (
        LogicalKey::Character('j'),
        Action::MoveDown,
        ShortcutBindingPresentation::DispatchOnly,
    ),
    (
        LogicalKey::Character('J'),
        Action::MoveDown,
        ShortcutBindingPresentation::DispatchOnly,
    ),
];

pub(super) fn binding(
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
    macos: bool,
) -> Option<(Action, ShortcutBindingPresentation)> {
    if let Some(binding) = macos_history(context, key, modifiers, macos) {
        return Some(binding);
    }
    if macos
        && key == LogicalKey::Enter
        && matches!(modifiers, LogicalModifiers::CONTROL | CONTROL_SHIFT)
    {
        return super::primary_action(context, key, modifiers.contains(LogicalModifiers::SHIFT))
            .map(|action| (action, ShortcutBindingPresentation::Explicit));
    }
    if let Some(action) = editor_boundary(context, key, modifiers, macos) {
        return Some((action, ShortcutBindingPresentation::Explicit));
    }
    if let Some(binding) = board_boundary(context, key, modifiers, macos) {
        return Some(binding);
    }
    (matches!(context, Context::Board | Context::InsertionBoundary)
        && macos_reorder_modifiers(macos, modifiers))
    .then(|| {
        MACOS_BOARD_DEFAULTS
            .iter()
            .find_map(|(candidate, action, presentation)| {
                (*candidate == key).then_some((*action, *presentation))
            })
    })
    .flatten()
}

fn macos_history(
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
    macos: bool,
) -> Option<(Action, ShortcutBindingPresentation)> {
    if !macos {
        return None;
    }
    let (action, presentation) = match (key, modifiers) {
        (LogicalKey::Character('z'), LogicalModifiers::CONTROL) => {
            (Action::Undo, ShortcutBindingPresentation::Explicit)
        }
        (LogicalKey::Character('z' | 'Z'), CONTROL_SHIFT) => {
            (Action::Redo, ShortcutBindingPresentation::Explicit)
        }
        // Some terminal paths encode Shift in the uppercase codepoint without
        // retaining a distinct modifier bit.
        (LogicalKey::Character('Z' | 'y' | 'Y'), LogicalModifiers::CONTROL) => {
            (Action::Redo, ShortcutBindingPresentation::DispatchOnly)
        }
        _ => return None,
    };
    // Every active keyboard context must absorb history at its own owner,
    // including blocking surfaces whose answer is explicitly unavailable.
    let _ownership = context.undo_contract();
    Some((action, presentation))
}

fn editor_boundary(
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
    macos: bool,
) -> Option<Action> {
    if context == Context::Invocation && matches!(key, LogicalKey::Up | LogicalKey::Down) {
        return None;
    }
    if !matches!(
        context,
        Context::Compose | Context::Edit | Context::Invocation
    ) {
        return None;
    }
    match (key, modifiers) {
        (LogicalKey::Up, LogicalModifiers::CONTROL) => Some(Action::MoveDocumentStart),
        (LogicalKey::Down, LogicalModifiers::CONTROL) => Some(Action::MoveDocumentEnd),
        (LogicalKey::Up, CONTROL_SHIFT) => Some(Action::ExtendDocumentStart),
        (LogicalKey::Down, CONTROL_SHIFT) => Some(Action::ExtendDocumentEnd),
        (LogicalKey::Left, LogicalModifiers::CONTROL) if macos => Some(Action::MoveLineStart),
        (LogicalKey::Right, LogicalModifiers::CONTROL) if macos => Some(Action::MoveLineEnd),
        (LogicalKey::Left, CONTROL_SHIFT) if macos => Some(Action::ExtendLineStart),
        (LogicalKey::Right, CONTROL_SHIFT) if macos => Some(Action::ExtendLineEnd),
        (LogicalKey::Left, LogicalModifiers::ALT) if !macos => Some(Action::MoveLineStart),
        (LogicalKey::Right, LogicalModifiers::ALT) if !macos => Some(Action::MoveLineEnd),
        (LogicalKey::Left, ALT_SHIFT) if !macos => Some(Action::ExtendLineStart),
        (LogicalKey::Right, ALT_SHIFT) if !macos => Some(Action::ExtendLineEnd),
        _ => None,
    }
}

fn board_boundary(
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
    macos: bool,
) -> Option<(Action, ShortcutBindingPresentation)> {
    if !matches!(context, Context::Board | Context::InsertionBoundary) {
        return None;
    }
    let (action, presentation) = match (key, modifiers, macos) {
        (LogicalKey::Up, LogicalModifiers::CONTROL, _) => {
            (Action::FocusFirst, ShortcutBindingPresentation::Explicit)
        }
        (LogicalKey::Down, LogicalModifiers::CONTROL, _) => {
            (Action::FocusLast, ShortcutBindingPresentation::Explicit)
        }
        (LogicalKey::Up, CONTROL_SHIFT, true) => {
            (Action::ExtendFirst, ShortcutBindingPresentation::Explicit)
        }
        (LogicalKey::Down, CONTROL_SHIFT, true) => {
            (Action::ExtendLast, ShortcutBindingPresentation::Explicit)
        }
        (LogicalKey::Character('n'), LogicalModifiers::CONTROL, true)
        | (LogicalKey::Down, LogicalModifiers::ALT, false) => {
            (Action::InsertBelow, ShortcutBindingPresentation::Explicit)
        }
        (LogicalKey::Character('n'), CONTROL_SHIFT, true)
        | (LogicalKey::Up, LogicalModifiers::ALT, false) => {
            (Action::InsertAbove, ShortcutBindingPresentation::Explicit)
        }
        (LogicalKey::Character('N'), LogicalModifiers::CONTROL | CONTROL_SHIFT, true) => (
            Action::InsertAbove,
            ShortcutBindingPresentation::DispatchOnly,
        ),
        _ => return None,
    };
    Some((action, presentation))
}

pub(super) fn macos_reorder_modifiers(macos: bool, modifiers: LogicalModifiers) -> bool {
    macos && modifiers == OPTION_SHIFT
}

pub(super) fn configured_board_boundary(
    previous: bool,
    base_focus: bool,
    modifiers: LogicalModifiers,
    macos: bool,
) -> Option<Action> {
    let action = match (previous, base_focus, modifiers, macos) {
        (true, true, LogicalModifiers::CONTROL, _) => Action::FocusFirst,
        (false, true, LogicalModifiers::CONTROL, _) => Action::FocusLast,
        (true, false, value, true)
            if value.difference(LogicalModifiers::SHIFT) == LogicalModifiers::CONTROL =>
        {
            Action::ExtendFirst
        }
        (false, false, value, true)
            if value.difference(LogicalModifiers::SHIFT) == LogicalModifiers::CONTROL =>
        {
            Action::ExtendLast
        }
        (true, true, LogicalModifiers::ALT, false) => Action::InsertAbove,
        (false, true, LogicalModifiers::ALT, false) => Action::InsertBelow,
        _ => return None,
    };
    Some(action)
}
