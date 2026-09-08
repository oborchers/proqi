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
    if let Some(action) = editor_boundary(context, key, modifiers, macos) {
        return Some((action, ShortcutBindingPresentation::Explicit));
    }
    if let Some(action) = board_boundary(context, key, modifiers, macos) {
        return Some((action, ShortcutBindingPresentation::Explicit));
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
) -> Option<Action> {
    if !matches!(context, Context::Board | Context::InsertionBoundary) {
        return None;
    }
    match (key, modifiers) {
        (LogicalKey::Up, LogicalModifiers::CONTROL) => Some(Action::FocusFirst),
        (LogicalKey::Down, LogicalModifiers::CONTROL) => Some(Action::FocusLast),
        (LogicalKey::Up, CONTROL_SHIFT) if macos => Some(Action::ExtendFirst),
        (LogicalKey::Down, CONTROL_SHIFT) if macos => Some(Action::ExtendLast),
        (LogicalKey::Up, LogicalModifiers::ALT) => Some(Action::InsertAbove),
        (LogicalKey::Down, LogicalModifiers::ALT) => Some(Action::InsertBelow),
        _ => None,
    }
}

pub(super) fn macos_reorder_modifiers(macos: bool, modifiers: LogicalModifiers) -> bool {
    macos && modifiers == OPTION_SHIFT
}
