//! Finite key, modifier, and context vocabulary used to build the effective graph.

use crate::ui::{LogicalKey, LogicalModifiers};

use super::Context;

pub(in crate::ui::shortcut_registry) const KEYBOARD_CONTEXTS: &[Context] = &[
    Context::Board,
    Context::Compose,
    Context::Edit,
    Context::Help,
    Context::Commands,
    Context::Search,
    Context::Invocation,
    Context::InvocationQuery,
    Context::Transfer,
    Context::Browser,
    Context::BrowserQuery,
    Context::Rename,
    Context::BrowserRename,
    Context::Update,
    Context::Screenshot,
    Context::Recovery,
    Context::Direction,
    Context::ReleaseHighlights,
    Context::InsertionBoundary,
];

pub(super) const FIXED_KEYS: &[LogicalKey] = &[
    LogicalKey::Character('a'),
    LogicalKey::Character('A'),
    LogicalKey::Character('c'),
    LogicalKey::Character('C'),
    LogicalKey::Character('d'),
    LogicalKey::Character('D'),
    LogicalKey::Character('h'),
    LogicalKey::Character('H'),
    LogicalKey::Character('j'),
    LogicalKey::Character('J'),
    LogicalKey::Character('k'),
    LogicalKey::Character('K'),
    LogicalKey::Character('l'),
    LogicalKey::Character('L'),
    LogicalKey::Character('n'),
    LogicalKey::Character('N'),
    LogicalKey::Character('p'),
    LogicalKey::Character('P'),
    LogicalKey::Character('q'),
    LogicalKey::Character('Q'),
    LogicalKey::Character('r'),
    LogicalKey::Character('R'),
    LogicalKey::Character('u'),
    LogicalKey::Character('U'),
    LogicalKey::Character('v'),
    LogicalKey::Character('V'),
    LogicalKey::Character('w'),
    LogicalKey::Character('x'),
    LogicalKey::Character('X'),
    LogicalKey::Character('y'),
    LogicalKey::Character('Y'),
    LogicalKey::Character('z'),
    LogicalKey::Character('Z'),
    LogicalKey::Backspace,
    LogicalKey::Enter,
    LogicalKey::Left,
    LogicalKey::Right,
    LogicalKey::Up,
    LogicalKey::Down,
    LogicalKey::Home,
    LogicalKey::End,
    LogicalKey::PageUp,
    LogicalKey::PageDown,
    LogicalKey::Tab,
    LogicalKey::BackTab,
    LogicalKey::Delete,
    LogicalKey::Escape,
];

pub(super) fn command_modifiers(modifiers: LogicalModifiers) -> bool {
    modifiers.intersects(
        LogicalModifiers::CONTROL
            .union(LogicalModifiers::SUPER)
            .union(LogicalModifiers::META)
            .union(LogicalModifiers::HYPER),
    )
}

pub(super) fn is_editor_context(context: Context) -> bool {
    matches!(
        context,
        Context::Compose | Context::Edit | Context::Invocation
    )
}

pub(super) fn is_query_cursor_context(context: Context) -> bool {
    matches!(
        context,
        Context::Commands | Context::Search | Context::Transfer
    )
}

pub(super) fn is_list_context(context: Context) -> bool {
    matches!(
        context,
        Context::Help
            | Context::Commands
            | Context::Search
            | Context::Invocation
            | Context::InvocationQuery
            | Context::Transfer
            | Context::Browser
            | Context::BrowserQuery
            | Context::Update
            | Context::Screenshot
            | Context::ReleaseHighlights
    )
}

pub(super) fn is_text_context(context: Context) -> bool {
    matches!(
        context,
        Context::Compose
            | Context::Edit
            | Context::Invocation
            | Context::Commands
            | Context::Search
            | Context::InvocationQuery
            | Context::Transfer
            | Context::Browser
            | Context::BrowserQuery
            | Context::Rename
            | Context::BrowserRename
    )
}

pub(super) fn modifier_combinations() -> impl Iterator<Item = LogicalModifiers> {
    (0_u8..64).map(|bits| {
        let mut modifiers = LogicalModifiers::NONE;
        for (bit, flag) in [
            LogicalModifiers::SHIFT,
            LogicalModifiers::CONTROL,
            LogicalModifiers::ALT,
            LogicalModifiers::SUPER,
            LogicalModifiers::META,
            LogicalModifiers::HYPER,
        ]
        .into_iter()
        .enumerate()
        {
            if bits & (1 << bit) != 0 {
                modifiers = modifiers.union(flag);
            }
        }
        modifiers
    })
}
