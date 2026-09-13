//! Named-key and modal-character bindings in the effective shortcut graph.

use crate::ui::{LogicalKey, LogicalModifiers};

use crate::ui::shortcut_registry::model::{ShortcutActionId as Action, ShortcutContext as Context};

use super::super::ESCAPE_CONTEXTS;
use super::vocabulary::{
    command_modifiers, is_editor_context, is_query_cursor_context, is_text_context,
};

const MODAL_CHARACTER_BINDINGS: &[(Context, char, Action)] = &[
    (Context::Recovery, 'r', Action::RetryStorage),
    (Context::Recovery, 'w', Action::ExportRecovery),
];

pub(in crate::ui) fn fixed_character_binding(action: Action, context: Context) -> Option<char> {
    MODAL_CHARACTER_BINDINGS
        .iter()
        .find_map(|&(owner, character, candidate)| {
            (owner == context && candidate == action).then_some(character)
        })
}

pub(super) fn fixed_character_keys() -> impl Iterator<Item = LogicalKey> {
    MODAL_CHARACTER_BINDINGS
        .iter()
        .map(|&(_, character, _)| LogicalKey::Character(character))
}

pub(super) fn named_action(
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
) -> Option<Action> {
    if context == Context::Browser && modifiers.is_empty() {
        match key {
            LogicalKey::Function(2) => return Some(Action::RenameSession),
            LogicalKey::Function(8) => return Some(Action::BrowserTrash),
            _ => {}
        }
    }
    if let Some(action) = modal_character_action(context, key, modifiers) {
        return Some(action);
    }
    if key == LogicalKey::Escape && ESCAPE_CONTEXTS.contains(&context) {
        return Some(Action::Close);
    }
    text_named_action(context, key, modifiers)
        .or_else(|| navigation_named_action(context, key, modifiers))
}

fn modal_character_action(
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
) -> Option<Action> {
    let LogicalKey::Character(character) = key else {
        return None;
    };
    if command_modifiers(modifiers) {
        return None;
    }
    MODAL_CHARACTER_BINDINGS
        .iter()
        .find_map(|&(owner, candidate, action)| {
            (owner == context && candidate == character).then_some(action)
        })
}

fn text_named_action(
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
) -> Option<Action> {
    let shifted = modifiers.contains(LogicalModifiers::SHIFT);
    match key {
        LogicalKey::Enter if !command_modifiers(modifiers) => enter_action(context),
        LogicalKey::Backspace if is_text_context(context) => Some(Action::Backspace),
        LogicalKey::Delete
            if matches!(context, Context::Board | Context::InsertionBoundary)
                && modifiers.is_empty() =>
        {
            Some(Action::Delete)
        }
        LogicalKey::Delete if is_editor_context(context) || is_query_cursor_context(context) => {
            Some(Action::DeleteForward)
        }
        LogicalKey::Tab
            if matches!(context, Context::Invocation | Context::InvocationQuery) && !shifted =>
        {
            Some(Action::Confirm)
        }
        LogicalKey::Tab if is_editor_context(context) && shifted => Some(Action::BackTab),
        LogicalKey::Tab if is_editor_context(context) => Some(Action::Tab),
        LogicalKey::BackTab if is_editor_context(context) => Some(Action::BackTab),
        _ => None,
    }
}

const fn enter_action(context: Context) -> Option<Action> {
    match context {
        Context::Board => Some(Action::Edit),
        Context::InsertionBoundary => Some(Action::New),
        Context::Compose
        | Context::Edit
        | Context::Commands
        | Context::Search
        | Context::Invocation
        | Context::InvocationQuery
        | Context::Transfer
        | Context::GlobalDeliveryQuery
        | Context::GlobalDeliveryDisposition
        | Context::Browser
        | Context::BrowserQuery
        | Context::Rename
        | Context::BrowserRename
        | Context::Update
        | Context::Screenshot
        | Context::Direction => Some(Action::Confirm),
        _ => None,
    }
}

fn navigation_named_action(
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
) -> Option<Action> {
    let shifted = modifiers.contains(LogicalModifiers::SHIFT);
    match key {
        LogicalKey::PageUp => Some(if shifted {
            Action::FastExtendPrevious
        } else {
            Action::FastPrevious
        }),
        LogicalKey::PageDown => Some(if shifted {
            Action::FastExtendNext
        } else {
            Action::FastNext
        }),
        LogicalKey::Home if is_editor_context(context) || is_query_cursor_context(context) => {
            Some(if shifted {
                Action::ExtendLineStart
            } else {
                Action::MoveLineStart
            })
        }
        LogicalKey::End if is_editor_context(context) || is_query_cursor_context(context) => {
            Some(if shifted {
                Action::ExtendLineEnd
            } else {
                Action::MoveLineEnd
            })
        }
        _ => None,
    }
}
