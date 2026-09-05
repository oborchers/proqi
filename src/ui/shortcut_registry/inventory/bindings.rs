//! Canonical platform defaults and configuration-derived effective aliases.

mod named;
pub(in crate::ui::shortcut_registry) mod vocabulary;

use std::collections::{BTreeMap, BTreeSet};

use crate::ui::{LogicalKey, LogicalModifiers, settings::KeyBindings};

use super::{Action, Context};
use crate::ui::shortcut_registry::{
    context_policy::effective_board_bindings,
    model::{
        ShortcutBinding, ShortcutBindingClaim, ShortcutBindingPresentation, ShortcutModifiers,
    },
};
use named::named_action;
use vocabulary::{
    FIXED_KEYS, KEYBOARD_CONTEXTS, command_modifiers, is_editor_context, is_list_context,
    is_query_cursor_context, modifier_combinations,
};

pub(super) fn default_claims(macos: bool) -> BTreeMap<Action, Vec<ShortcutBindingClaim>> {
    collect_claims(FIXED_KEYS.iter().copied(), |context, key, modifiers| {
        let action = fixed_action(context, key, modifiers, macos)?;
        Some((
            action,
            default_presentation(action, context, key, modifiers, macos),
        ))
    })
}

pub(super) fn alias_claims(
    keys: &KeyBindings,
    macos: bool,
) -> BTreeMap<Action, Vec<ShortcutBindingClaim>> {
    let board = effective_board_bindings(keys);
    let mut candidates = board
        .keys()
        .copied()
        .map(LogicalKey::Character)
        .collect::<BTreeSet<_>>();
    for character in [
        'r',
        'w',
        keys.transform,
        keys.delete_sentence,
        keys.select_visual_row_start,
        keys.select_visual_row_end,
        keys.help,
        keys.quit,
    ] {
        candidates.insert(LogicalKey::Character(character));
        if character.is_ascii_alphabetic() {
            candidates.insert(LogicalKey::Character(character.to_ascii_lowercase()));
            candidates.insert(LogicalKey::Character(character.to_ascii_uppercase()));
        }
    }
    collect_claims(candidates, |context, key, modifiers| {
        if fixed_action(context, key, modifiers, macos).is_some() {
            None
        } else {
            configured_action(context, key, modifiers, macos, keys, &board)
                .map(|action| (action, ShortcutBindingPresentation::DispatchOnly))
        }
    })
}

fn collect_claims(
    keys: impl IntoIterator<Item = LogicalKey>,
    mut resolve: impl FnMut(
        Context,
        LogicalKey,
        LogicalModifiers,
    ) -> Option<(Action, ShortcutBindingPresentation)>,
) -> BTreeMap<Action, Vec<ShortcutBindingClaim>> {
    let keys = keys.into_iter().collect::<Vec<_>>();
    let mut grouped: BTreeMap<(Action, LogicalKey, LogicalModifiers, bool, bool), Vec<Context>> =
        BTreeMap::new();
    let candidates = KEYBOARD_CONTEXTS.iter().copied().flat_map(|context| {
        keys.iter().copied().flat_map(move |key| {
            modifier_combinations().map(move |modifiers| (context, key, modifiers))
        })
    });
    for (context, key, modifiers) in candidates {
        let Some((action, presentation)) = resolve(context, key, modifiers) else {
            continue;
        };
        let (is_primary_presentation, canonical) = match presentation {
            ShortcutBindingPresentation::DispatchOnly => (false, false),
            ShortcutBindingPresentation::Primary { canonical } => (true, canonical),
        };
        grouped
            .entry((action, key, modifiers, is_primary_presentation, canonical))
            .or_default()
            .push(context);
    }
    let mut claims: BTreeMap<Action, Vec<ShortcutBindingClaim>> = BTreeMap::new();
    for ((action, key, modifiers, is_primary_presentation, canonical), contexts) in grouped {
        claims
            .entry(action)
            .or_default()
            .push(ShortcutBindingClaim {
                binding: ShortcutBinding {
                    key,
                    modifiers: ShortcutModifiers::Exact(modifiers),
                },
                contexts,
                presentation: if is_primary_presentation {
                    ShortcutBindingPresentation::Primary { canonical }
                } else {
                    ShortcutBindingPresentation::DispatchOnly
                },
            });
    }
    claims
}

fn default_presentation(
    action: Action,
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
    macos: bool,
) -> ShortcutBindingPresentation {
    let shifted = modifiers.contains(LogicalModifiers::SHIFT);
    let primary_action = is_primary(modifiers, macos)
        && (primary_action(context, key, shifted) == Some(action)
            || action == Action::Quit
                && !shifted
                && matches!(key, LogicalKey::Character('q' | 'Q')));
    if !primary_action {
        return ShortcutBindingPresentation::DispatchOnly;
    }
    let canonical =
        action != Action::Redo || shifted && matches!(key, LogicalKey::Character('z' | 'Z'));
    ShortcutBindingPresentation::Primary { canonical }
}

fn fixed_action(
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
    macos: bool,
) -> Option<Action> {
    let primary = is_primary(modifiers, macos);
    let shifted = modifiers.contains(LogicalModifiers::SHIFT);
    if primary && !shifted && matches!(key, LogicalKey::Character('q' | 'Q')) {
        return Some(Action::Quit);
    }
    if let Some(action) = owner_navigation(context, key, modifiers, macos) {
        return Some(action);
    }
    if command_modifiers(modifiers)
        && !primary
        && matches!(key, LogicalKey::Character(_) | LogicalKey::Enter)
    {
        return None;
    }
    if primary && let Some(action) = primary_action(context, key, shifted) {
        return Some(action);
    }
    named_action(context, key, modifiers)
}

fn primary_action(context: Context, key: LogicalKey, shifted: bool) -> Option<Action> {
    if matches!(context, Context::Invocation | Context::InvocationQuery) && !shifted {
        match key {
            LogicalKey::Character('p' | 'P') => return Some(Action::PickerPrevious),
            LogicalKey::Character('n' | 'N') => return Some(Action::PickerNext),
            _ => {}
        }
    }
    let editor_backed = matches!(
        context,
        Context::Board
            | Context::InsertionBoundary
            | Context::Compose
            | Context::Edit
            | Context::Invocation
    );
    if editor_backed {
        let action = match key {
            LogicalKey::Enter if shifted => Action::SubmitKeep,
            LogicalKey::Enter => Action::SubmitRemove,
            LogicalKey::Character('v' | 'V') if shifted => Action::PasteReflow,
            LogicalKey::Character('a' | 'A') if !shifted => Action::SelectAll,
            LogicalKey::Character('c' | 'C') if !shifted => Action::Copy,
            LogicalKey::Character('x' | 'X') if !shifted => Action::Cut,
            LogicalKey::Character('v' | 'V') if !shifted => Action::PasteExact,
            LogicalKey::Character('d' | 'D') if !shifted => Action::Duplicate,
            LogicalKey::Character('u' | 'U') if !shifted => Action::DeleteLogicalLine,
            LogicalKey::Character('z' | 'Z') if shifted => Action::Redo,
            LogicalKey::Character('y' | 'Y') if !shifted => Action::Redo,
            LogicalKey::Character('z' | 'Z') if !shifted => Action::Undo,
            _ => return None,
        };
        return Some(action);
    }
    None
}

fn owner_navigation(
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
    macos: bool,
) -> Option<Action> {
    if context == Context::Direction {
        return match key {
            LogicalKey::Left | LogicalKey::Character('h' | 'H') => Some(Action::ChooseLeft),
            LogicalKey::Down | LogicalKey::Character('j' | 'J') => Some(Action::ChooseDown),
            LogicalKey::Up | LogicalKey::Character('k' | 'K') => Some(Action::ChooseUp),
            LogicalKey::Right | LogicalKey::Character('l' | 'L') => Some(Action::ChooseRight),
            _ => None,
        };
    }
    if matches!(
        context,
        Context::Help | Context::Update | Context::Screenshot | Context::ReleaseHighlights
    ) {
        match key {
            LogicalKey::Character('j' | 'J') => return Some(Action::FocusNext),
            LogicalKey::Character('k' | 'K') => return Some(Action::FocusPrevious),
            _ => {}
        }
    }
    match key {
        LogicalKey::Up => vertical_action(context, true, modifiers, macos),
        LogicalKey::Down => vertical_action(context, false, modifiers, macos),
        LogicalKey::Left => horizontal_action(context, true, modifiers, macos),
        LogicalKey::Right => horizontal_action(context, false, modifiers, macos),
        _ => None,
    }
}

fn vertical_action(
    context: Context,
    previous: bool,
    modifiers: LogicalModifiers,
    macos: bool,
) -> Option<Action> {
    let primary = is_primary(modifiers, macos);
    let shifted = modifiers.contains(LogicalModifiers::SHIFT);
    if matches!(context, Context::Board | Context::InsertionBoundary) {
        return Some(match (previous, primary && shifted, shifted) {
            (true, true, _) => Action::MoveUp,
            (false, true, _) => Action::MoveDown,
            (true, false, true) => Action::ExtendPrevious,
            (false, false, true) => Action::ExtendNext,
            (true, false, false) => Action::FocusPrevious,
            (false, false, false) => Action::FocusNext,
        });
    }
    if is_list_context(context) {
        if modifiers.contains(LogicalModifiers::ALT) && !primary {
            return Some(if previous {
                Action::FastPrevious
            } else {
                Action::FastNext
            });
        }
        return Some(if previous {
            Action::FocusPrevious
        } else {
            Action::FocusNext
        });
    }
    if is_editor_context(context) {
        if modifiers.contains(LogicalModifiers::ALT) && !primary {
            return Some(if previous {
                Action::FastPrevious
            } else {
                Action::FastNext
            });
        }
        return Some(match (previous, primary, shifted) {
            (true, true, true) => Action::ExtendDocumentStart,
            (false, true, true) => Action::ExtendDocumentEnd,
            (true, true, false) => Action::MoveDocumentStart,
            (false, true, false) => Action::MoveDocumentEnd,
            (true, false, true) => Action::ExtendVisualUp,
            (false, false, true) => Action::ExtendVisualDown,
            (true, false, false) => Action::MoveVisualUp,
            (false, false, false) => Action::MoveVisualDown,
        });
    }
    None
}

fn horizontal_action(
    context: Context,
    back: bool,
    modifiers: LogicalModifiers,
    macos: bool,
) -> Option<Action> {
    if matches!(context, Context::Browser | Context::BrowserQuery) {
        return Some(if back {
            Action::FocusPrevious
        } else {
            Action::FocusNext
        });
    }
    if !is_editor_context(context) && !is_query_cursor_context(context) {
        return None;
    }
    let primary = is_primary(modifiers, macos);
    let shifted = modifiers.contains(LogicalModifiers::SHIFT);
    if macos && primary {
        return Some(match (back, shifted) {
            (true, true) => Action::ExtendVisualRowStart,
            (false, true) => Action::ExtendVisualRowEnd,
            (true, false) => Action::MoveVisualRowStart,
            (false, false) => Action::MoveVisualRowEnd,
        });
    }
    let word = if macos {
        modifiers.contains(LogicalModifiers::ALT)
    } else {
        modifiers.contains(LogicalModifiers::CONTROL)
    };
    Some(match (back, word, shifted) {
        (true, true, true) => Action::ExtendWordBack,
        (false, true, true) => Action::ExtendWordForward,
        (true, true, false) => Action::MoveWordBack,
        (false, true, false) => Action::MoveWordForward,
        (true, false, true) => Action::ExtendGraphemeBack,
        (false, false, true) => Action::ExtendGraphemeForward,
        (true, false, false) => Action::MoveGraphemeBack,
        (false, false, false) => Action::MoveGraphemeForward,
    })
}

fn configured_action(
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
    macos: bool,
    keys: &KeyBindings,
    board: &BTreeMap<char, Action>,
) -> Option<Action> {
    let LogicalKey::Character(character) = key else {
        return None;
    };
    if matches!(context, Context::Board | Context::InsertionBoundary)
        && let Some(base) = board.get(&character).copied()
    {
        if matches!(
            base,
            Action::FocusPrevious | Action::FocusNext | Action::ExtendPrevious | Action::ExtendNext
        ) {
            let previous = matches!(base, Action::FocusPrevious | Action::ExtendPrevious);
            let shifted = modifiers.contains(LogicalModifiers::SHIFT)
                || matches!(base, Action::ExtendPrevious | Action::ExtendNext);
            return Some(
                match (previous, is_primary(modifiers, macos) && shifted, shifted) {
                    (true, true, _) => Action::MoveUp,
                    (false, true, _) => Action::MoveDown,
                    (true, false, true) => Action::ExtendPrevious,
                    (false, false, true) => Action::ExtendNext,
                    (true, false, false) => Action::FocusPrevious,
                    (false, false, false) => Action::FocusNext,
                },
            );
        }
        if !command_modifiers(modifiers) {
            return Some(base);
        }
    }
    if is_editor_context(context) && is_primary(modifiers, macos) {
        let shifted = modifiers.contains(LogicalModifiers::SHIFT) || character.is_ascii_uppercase();
        if !shifted && character.eq_ignore_ascii_case(&keys.transform) {
            return Some(Action::ContextualTransform);
        }
        if shifted && character.eq_ignore_ascii_case(&keys.delete_sentence) {
            return Some(Action::DeleteSentence);
        }
        if shifted && character.eq_ignore_ascii_case(&keys.select_visual_row_start) {
            return Some(Action::ExtendVisualRowStart);
        }
        if shifted && character.eq_ignore_ascii_case(&keys.select_visual_row_end) {
            return Some(Action::ExtendVisualRowEnd);
        }
    }
    if context == Context::Help && !command_modifiers(modifiers) && character == keys.help {
        return Some(Action::Close);
    }
    if context == Context::Recovery && !command_modifiers(modifiers) {
        return match character {
            value if value == keys.quit => Some(Action::Quit),
            _ => None,
        };
    }
    None
}

pub(in crate::ui::shortcut_registry) fn is_primary(
    modifiers: LogicalModifiers,
    macos: bool,
) -> bool {
    let eligible = if macos {
        let super_key = modifiers.contains(LogicalModifiers::SUPER);
        let meta_key = modifiers.contains(LogicalModifiers::META);
        if super_key == meta_key {
            return false;
        }
        if super_key {
            LogicalModifiers::SUPER
        } else {
            LogicalModifiers::META
        }
    } else {
        LogicalModifiers::CONTROL
    };
    modifiers.contains(eligible)
        && modifiers
            .difference(eligible.union(LogicalModifiers::SHIFT))
            .is_empty()
}
