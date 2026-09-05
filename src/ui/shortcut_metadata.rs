//! Canonical metadata for standard cross-mode Primary shortcuts.

use super::{KeyBindings, UiKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShortcutAction {
    Copy,
    Cut,
    Paste,
    PasteReflow,
    SelectAll,
    Duplicate,
    Undo,
    Redo,
    Submit,
    SubmitKeep,
    Quit,
    DeleteLogicalLine,
    PickerPrevious,
    PickerNext,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShortcutScope {
    BoardAndEdit,
    Board,
    Edit,
    Global,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShiftMeaning {
    Unshifted,
    Shifted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShortcutMetadata {
    pub(crate) action: ShortcutAction,
    pub(crate) primary_suffix: &'static str,
    pub(crate) scope: ShortcutScope,
    pub(crate) shift: ShiftMeaning,
    pub(crate) normalized: UiKey,
}

pub(crate) const STANDARD_SHORTCUTS: &[ShortcutMetadata] = &[
    standard(
        ShortcutAction::Copy,
        "C",
        ShortcutScope::BoardAndEdit,
        UiKey::Copy,
    ),
    standard(
        ShortcutAction::Cut,
        "X",
        ShortcutScope::BoardAndEdit,
        UiKey::Cut,
    ),
    standard(
        ShortcutAction::Paste,
        "V",
        ShortcutScope::BoardAndEdit,
        UiKey::PasteClipboard,
    ),
    shifted(
        ShortcutAction::PasteReflow,
        "Shift+V",
        ShortcutScope::BoardAndEdit,
        UiKey::PasteClipboardReflow,
    ),
    standard(
        ShortcutAction::SelectAll,
        "A",
        ShortcutScope::BoardAndEdit,
        UiKey::SelectAll,
    ),
    standard(
        ShortcutAction::Duplicate,
        "D",
        ShortcutScope::Board,
        UiKey::Duplicate,
    ),
    standard(
        ShortcutAction::Undo,
        "Z",
        ShortcutScope::BoardAndEdit,
        UiKey::Undo,
    ),
    shifted(
        ShortcutAction::Redo,
        "Shift+Z",
        ShortcutScope::BoardAndEdit,
        UiKey::Redo,
    ),
    standard(
        ShortcutAction::Redo,
        "Y",
        ShortcutScope::BoardAndEdit,
        UiKey::Redo,
    ),
    standard(
        ShortcutAction::Submit,
        "Enter",
        ShortcutScope::BoardAndEdit,
        UiKey::Submit,
    ),
    shifted(
        ShortcutAction::SubmitKeep,
        "Shift+Enter",
        ShortcutScope::BoardAndEdit,
        UiKey::SubmitKeep,
    ),
    standard(
        ShortcutAction::Quit,
        "Q",
        ShortcutScope::Global,
        UiKey::Quit,
    ),
    standard(
        ShortcutAction::DeleteLogicalLine,
        "U",
        ShortcutScope::Edit,
        UiKey::DeleteLogicalLine,
    ),
    standard(
        ShortcutAction::PickerPrevious,
        "P",
        ShortcutScope::Edit,
        UiKey::PickerPrevious,
    ),
    standard(
        ShortcutAction::PickerNext,
        "N",
        ShortcutScope::Edit,
        UiKey::PickerNext,
    ),
];

const fn standard(
    action: ShortcutAction,
    primary_suffix: &'static str,
    scope: ShortcutScope,
    normalized: UiKey,
) -> ShortcutMetadata {
    ShortcutMetadata {
        action,
        primary_suffix,
        scope,
        shift: ShiftMeaning::Unshifted,
        normalized,
    }
}

const fn shifted(
    action: ShortcutAction,
    primary_suffix: &'static str,
    scope: ShortcutScope,
    normalized: UiKey,
) -> ShortcutMetadata {
    ShortcutMetadata {
        action,
        primary_suffix,
        scope,
        shift: ShiftMeaning::Shifted,
        normalized,
    }
}

pub(crate) fn primary_label(action: ShortcutAction) -> String {
    label(action, ShiftMeaning::Unshifted)
}

pub(crate) fn shifted_primary_label(action: ShortcutAction) -> String {
    label(action, ShiftMeaning::Shifted)
}

pub(crate) fn redo_label() -> String {
    format!(
        "{}/{}",
        shifted_primary_label(ShortcutAction::Redo),
        primary_label(ShortcutAction::Redo)
    )
}

pub(crate) fn board_label(action: ShortcutAction, keys: &KeyBindings) -> String {
    let primary = canonical_label(action);
    let fallbacks = board_fallbacks(action, keys);
    if fallbacks.is_empty() {
        primary
    } else {
        format!("{primary}/{}", fallback_label(&fallbacks))
    }
}

pub(crate) fn board_control_label(
    action: ShortcutAction,
    keys: &KeyBindings,
    compact: bool,
) -> String {
    let fallbacks = board_fallbacks(action, keys);
    if fallbacks.is_empty() {
        return canonical_label(action);
    }
    if compact {
        fallback_label(&fallbacks)
    } else {
        board_label(action, keys)
    }
}

pub(crate) fn canonical_label(action: ShortcutAction) -> String {
    STANDARD_SHORTCUTS
        .iter()
        .find(|shortcut| shortcut.action == action)
        .map_or_else(
            || "Primary".to_owned(),
            |shortcut| super::settings::primary_key_label(shortcut.primary_suffix),
        )
}

pub(crate) fn reserved_unshifted_character(character: char) -> bool {
    STANDARD_SHORTCUTS.iter().any(|shortcut| {
        shortcut.shift == ShiftMeaning::Unshifted
            && single_character_suffix(shortcut)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(&character))
    })
}

pub(crate) fn reserved_shifted_configuration_suffix(character: char) -> bool {
    STANDARD_SHORTCUTS.iter().any(|shortcut| {
        shortcut.shift == ShiftMeaning::Unshifted
            && shortcut.action != ShortcutAction::DeleteLogicalLine
            && single_character_suffix(shortcut)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(&character))
    })
}

fn label(action: ShortcutAction, shift: ShiftMeaning) -> String {
    STANDARD_SHORTCUTS
        .iter()
        .find(|shortcut| shortcut.action == action && shortcut.shift == shift)
        .map_or_else(
            || "Primary".to_owned(),
            |shortcut| super::settings::primary_key_label(shortcut.primary_suffix),
        )
}

fn single_character_suffix(shortcut: &ShortcutMetadata) -> Option<char> {
    let mut characters = shortcut.primary_suffix.chars();
    let character = characters.next()?;
    characters.next().is_none().then_some(character)
}

fn board_fallbacks(action: ShortcutAction, keys: &KeyBindings) -> Vec<char> {
    match action {
        ShortcutAction::Copy => vec![keys.copy],
        ShortcutAction::Cut => vec![keys.cut],
        ShortcutAction::SelectAll => vec![keys.select_all],
        ShortcutAction::Undo => vec![keys.undo],
        ShortcutAction::Submit => vec![keys.submit_remove],
        ShortcutAction::SubmitKeep => vec![keys.submit_keep],
        ShortcutAction::Quit => vec![keys.quit],
        ShortcutAction::PasteReflow => keys.paste_reflow_fallbacks(),
        ShortcutAction::Paste
        | ShortcutAction::Duplicate
        | ShortcutAction::Redo
        | ShortcutAction::DeleteLogicalLine
        | ShortcutAction::PickerPrevious
        | ShortcutAction::PickerNext => Vec::new(),
    }
}

fn fallback_label(fallbacks: &[char]) -> String {
    fallbacks
        .iter()
        .map(|character| super::settings::key_label(*character))
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    type InventoryCase = (
        ShortcutAction,
        ShortcutScope,
        ShiftMeaning,
        UiKey,
        &'static [char],
    );

    const INVENTORY_CASES: &[InventoryCase] = &[
        (
            ShortcutAction::Copy,
            ShortcutScope::BoardAndEdit,
            ShiftMeaning::Unshifted,
            UiKey::Copy,
            &['y'],
        ),
        (
            ShortcutAction::Cut,
            ShortcutScope::BoardAndEdit,
            ShiftMeaning::Unshifted,
            UiKey::Cut,
            &['x'],
        ),
        (
            ShortcutAction::Paste,
            ShortcutScope::BoardAndEdit,
            ShiftMeaning::Unshifted,
            UiKey::PasteClipboard,
            &[],
        ),
        (
            ShortcutAction::PasteReflow,
            ShortcutScope::BoardAndEdit,
            ShiftMeaning::Shifted,
            UiKey::PasteClipboardReflow,
            &['p', 'P'],
        ),
        (
            ShortcutAction::SelectAll,
            ShortcutScope::BoardAndEdit,
            ShiftMeaning::Unshifted,
            UiKey::SelectAll,
            &['a'],
        ),
        (
            ShortcutAction::Duplicate,
            ShortcutScope::Board,
            ShiftMeaning::Unshifted,
            UiKey::Duplicate,
            &[],
        ),
        (
            ShortcutAction::Undo,
            ShortcutScope::BoardAndEdit,
            ShiftMeaning::Unshifted,
            UiKey::Undo,
            &['u'],
        ),
        (
            ShortcutAction::Redo,
            ShortcutScope::BoardAndEdit,
            ShiftMeaning::Shifted,
            UiKey::Redo,
            &[],
        ),
        (
            ShortcutAction::Submit,
            ShortcutScope::BoardAndEdit,
            ShiftMeaning::Unshifted,
            UiKey::Submit,
            &['s'],
        ),
        (
            ShortcutAction::SubmitKeep,
            ShortcutScope::BoardAndEdit,
            ShiftMeaning::Shifted,
            UiKey::SubmitKeep,
            &['S'],
        ),
        (
            ShortcutAction::Quit,
            ShortcutScope::Global,
            ShiftMeaning::Unshifted,
            UiKey::Quit,
            &['q'],
        ),
    ];

    #[test]
    fn inventory_records_scopes_shifted_variants_and_board_fallbacks() {
        let keys = KeyBindings::default();
        for &(action, scope, shift, normalized, fallback) in INVENTORY_CASES {
            let shortcut = STANDARD_SHORTCUTS
                .iter()
                .find(|shortcut| shortcut.action == action && shortcut.shift == shift)
                .expect("inventory entry");
            assert_eq!(shortcut.scope, scope, "action {action:?}");
            assert_eq!(shortcut.normalized, normalized, "action {action:?}");
            assert_eq!(
                board_fallbacks(action, &keys),
                fallback,
                "action {action:?}"
            );
        }
        assert!(STANDARD_SHORTCUTS.iter().any(|shortcut| {
            shortcut.action == ShortcutAction::Redo
                && shortcut.shift == ShiftMeaning::Unshifted
                && shortcut.primary_suffix == "Y"
        }));
    }

    #[test]
    fn public_contracts_record_platform_and_host_boundaries() {
        let readme = include_str!("../../README.md");
        let product = include_str!("../../context/PRODUCT.md");
        let architecture = include_str!("../../context/ARCHITECTURE.md");
        for contract in [readme, product] {
            assert!(contract.contains("Primary"));
            assert!(contract.contains("bracketed paste"));
            assert!(contract.contains("command palette"));
        }
        assert!(readme.contains("Cmd+Shift+V"));
        assert!(readme.contains("keybind = super+shift+v=unbind"));
        assert!(readme.contains("Raw `Ctrl` is not a second Primary"));
        assert!(!readme.contains("Command+"));
        assert!(readme.contains("proqi diagnostics keypress"));
        assert!(product.contains("Primary+Shift+V"));
        assert!(!product.contains("Command+"));
        assert!(product.contains("raw key diagnostics"));
        assert!(architecture.contains("logical Super or Meta on macOS"));
        assert!(architecture.contains("only to logical Control elsewhere"));
        assert!(!architecture.contains("Command+"));
    }
}
