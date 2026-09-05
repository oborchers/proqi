//! Named-key and modal-character bindings in the effective shortcut graph.

use crate::ui::{LogicalKey, LogicalModifiers};

use crate::ui::shortcut_registry::model::{ShortcutActionId as Action, ShortcutContext as Context};

use super::super::ESCAPE_CONTEXTS;
use super::vocabulary::{
    command_modifiers, is_editor_context, is_query_cursor_context, is_text_context,
};

pub(super) fn named_action(
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
) -> Option<Action> {
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
    match key {
        LogicalKey::Character('R')
            if context == Context::Browser && !command_modifiers(modifiers) =>
        {
            Some(Action::RenameSession)
        }
        LogicalKey::Character('D')
            if context == Context::Browser && !command_modifiers(modifiers) =>
        {
            Some(Action::BrowserTrash)
        }
        LogicalKey::Character('r')
            if context == Context::Recovery && !command_modifiers(modifiers) =>
        {
            Some(Action::RetryStorage)
        }
        LogicalKey::Character('w')
            if context == Context::Recovery && !command_modifiers(modifiers) =>
        {
            Some(Action::ExportRecovery)
        }
        _ => None,
    }
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
        LogicalKey::Delete
            if matches!(
                context,
                Context::Browser | Context::BrowserQuery | Context::BrowserRename
            ) =>
        {
            Some(Action::Backspace)
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
        LogicalKey::PageUp => Some(Action::FastPrevious),
        LogicalKey::PageDown => Some(Action::FastNext),
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
        LogicalKey::Home if matches!(context, Context::Browser | Context::BrowserQuery) => {
            Some(Action::FocusPrevious)
        }
        LogicalKey::End if matches!(context, Context::Browser | Context::BrowserQuery) => {
            Some(Action::FocusNext)
        }
        _ => None,
    }
}
