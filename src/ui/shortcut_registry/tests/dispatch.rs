use crate::ui::{
    KeyBindings, KeyPhase, LogicalKey, LogicalModifiers, ShortcutActionId as Action,
    ShortcutContext as Context, ShortcutContextStack, UiKey,
};

use super::super::{ShortcutPlatform, ShortcutRegistry};
use super::stroke;

pub(super) fn dispatched(
    registry: &ShortcutRegistry,
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
) -> super::super::ResolvedShortcut {
    registry
        .dispatch(
            &ShortcutContextStack::new([context]),
            stroke(key, modifiers),
        )
        .expect("resolved input")
}

#[test]
fn every_default_board_override_resolves_to_its_typed_action() {
    let keys = KeyBindings::default();
    let registry =
        ShortcutRegistry::resolve(&keys, ShortcutPlatform::Portable).expect("valid registry");
    for (character, expected) in [
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
        (keys.transform, Action::ContextualTransform),
        (keys.paste, Action::PasteExact),
        (keys.paste.to_ascii_uppercase(), Action::PasteReflow),
    ] {
        assert_eq!(
            dispatched(
                &registry,
                Context::Board,
                LogicalKey::Character(character),
                LogicalModifiers::NONE
            )
            .action,
            Some(expected),
            "binding {character:?}"
        );
    }
}

#[test]
fn historical_transform_and_paste_shadowing_resolve_to_one_effective_winner() {
    let transform = KeyBindings {
        new: 't',
        ..KeyBindings::default()
    };
    let registry = ShortcutRegistry::resolve(&transform, ShortcutPlatform::Portable)
        .expect("compatible transform shadow");
    assert_eq!(
        dispatched(
            &registry,
            Context::Board,
            LogicalKey::Character('t'),
            LogicalModifiers::NONE
        )
        .action,
        Some(Action::New)
    );

    let paste = KeyBindings {
        new: 'p',
        ..KeyBindings::default()
    };
    let registry = ShortcutRegistry::resolve(&paste, ShortcutPlatform::Portable)
        .expect("compatible paste shadow");
    assert_eq!(
        dispatched(
            &registry,
            Context::Board,
            LogicalKey::Character('p'),
            LogicalModifiers::NONE
        )
        .action,
        Some(Action::New)
    );
}

#[test]
fn every_text_owner_reserves_plain_shifted_and_unicode_input() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry");
    for context in [
        Context::Compose,
        Context::Edit,
        Context::Commands,
        Context::Search,
        Context::Invocation,
        Context::InvocationQuery,
        Context::Transfer,
        Context::Browser,
        Context::BrowserQuery,
        Context::Rename,
        Context::BrowserRename,
    ] {
        for (character, modifiers) in [
            ('j', LogicalModifiers::NONE),
            ('?', LogicalModifiers::SHIFT),
            ('界', LogicalModifiers::NONE),
            ('é', LogicalModifiers::NONE),
            ('\u{301}', LogicalModifiers::NONE),
            ('e', LogicalModifiers::ALT),
        ] {
            let resolved = dispatched(
                &registry,
                context,
                LogicalKey::Character(character),
                modifiers,
            );
            assert_eq!(resolved.action, None, "context {context:?}");
            assert!(matches!(resolved.intention, UiKey::Character(value) if value == character));
        }
    }
}

#[test]
fn shift_and_alt_enter_keep_their_existing_context_intentions() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
        .expect("valid registry");
    for context in [Context::Board, Context::Edit, Context::Commands] {
        let expected = if context == Context::Board {
            Action::Edit
        } else {
            Action::Confirm
        };
        for modifiers in [
            LogicalModifiers::SHIFT,
            LogicalModifiers::ALT,
            LogicalModifiers::SHIFT.union(LogicalModifiers::ALT),
        ] {
            assert_eq!(
                dispatched(&registry, context, LogicalKey::Enter, modifiers).action,
                Some(expected),
                "context {context:?}, modifiers {modifiers:?}",
            );
        }
    }
}

#[test]
fn automatic_invocation_keeps_modified_backspace_editor_owned() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
        .expect("valid registry");
    for modifiers in [
        LogicalModifiers::NONE,
        LogicalModifiers::ALT,
        LogicalModifiers::CONTROL,
    ] {
        assert_eq!(
            dispatched(
                &registry,
                Context::Invocation,
                LogicalKey::Backspace,
                modifiers,
            )
            .action,
            Some(Action::Backspace),
        );
    }
}

#[test]
fn arrows_and_configured_vertical_aliases_share_every_modifier_ladder() {
    let keys = KeyBindings::default();
    for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
        let registry = ShortcutRegistry::resolve(&keys, platform).expect("valid registry");
        for modifiers in [
            LogicalModifiers::NONE,
            LogicalModifiers::SHIFT,
            LogicalModifiers::CONTROL,
            LogicalModifiers::ALT,
            LogicalModifiers::SUPER,
            LogicalModifiers::META,
            LogicalModifiers::CONTROL.union(LogicalModifiers::ALT),
            LogicalModifiers::SUPER.union(LogicalModifiers::SHIFT),
            LogicalModifiers::META.union(LogicalModifiers::SHIFT),
        ] {
            assert_vertical_alias_parity(&registry, platform, modifiers);
        }
    }
}

fn assert_vertical_alias_parity(
    registry: &ShortcutRegistry,
    platform: ShortcutPlatform,
    modifiers: LogicalModifiers,
) {
    let aliases = if modifiers.contains(LogicalModifiers::SHIFT) {
        [(LogicalKey::Up, 'K'), (LogicalKey::Down, 'J')]
    } else {
        [(LogicalKey::Up, 'k'), (LogicalKey::Down, 'j')]
    };
    for (arrow, alias) in aliases {
        let arrow_action = dispatched(registry, Context::Board, arrow, modifiers).action;
        let alias_action = dispatched(
            registry,
            Context::Board,
            LogicalKey::Character(alias),
            modifiers,
        )
        .action;
        assert_eq!(arrow_action, alias_action, "{platform:?} {modifiers:?}");
    }
}

#[test]
fn press_and_repeat_dispatch_but_release_does_not() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry");
    let contexts = ShortcutContextStack::new([Context::Board]);
    let mut input = stroke(LogicalKey::Character('j'), LogicalModifiers::NONE);

    assert_eq!(
        registry
            .dispatch(&contexts, input)
            .and_then(|resolved| resolved.action),
        Some(Action::FocusNext)
    );
    input.phase = KeyPhase::Repeat;
    assert_eq!(
        registry
            .dispatch(&contexts, input)
            .and_then(|resolved| resolved.action),
        Some(Action::FocusNext)
    );
    input.phase = KeyPhase::Release;
    assert_eq!(registry.dispatch(&contexts, input), None);
}

#[test]
fn top_context_owns_modal_navigation_and_escape() {
    let keys = KeyBindings {
        focus_down: 'g',
        help: 'j',
        ..KeyBindings::default()
    };
    let registry =
        ShortcutRegistry::resolve(&keys, ShortcutPlatform::Portable).expect("valid remap");
    let stack = ShortcutContextStack::new([Context::Board, Context::Help]);
    let navigation = registry
        .dispatch(
            &stack,
            stroke(LogicalKey::Character('j'), LogicalModifiers::CONTROL),
        )
        .expect("Help navigation");
    assert_eq!(navigation.action, Some(Action::FocusNext));
    let close = registry
        .dispatch(&stack, stroke(LogicalKey::Escape, LogicalModifiers::NONE))
        .expect("Help close");
    assert_eq!(close.action, Some(Action::Close));
}

#[test]
fn browser_management_aliases_exist_only_for_an_empty_query() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry");
    assert_eq!(
        dispatched(
            &registry,
            Context::Browser,
            LogicalKey::Character('R'),
            LogicalModifiers::SHIFT
        )
        .action,
        Some(Action::RenameSession)
    );
    assert_eq!(
        dispatched(
            &registry,
            Context::BrowserQuery,
            LogicalKey::Character('R'),
            LogicalModifiers::SHIFT
        )
        .action,
        None
    );
}

#[test]
fn recovery_routes_and_configured_quit_remain_reachable() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry");
    for (key, action) in [
        ('r', Action::RetryStorage),
        ('w', Action::ExportRecovery),
        ('q', Action::Quit),
    ] {
        assert_eq!(
            dispatched(
                &registry,
                Context::Recovery,
                LogicalKey::Character(key),
                LogicalModifiers::NONE
            )
            .action,
            Some(action)
        );
    }
}

#[test]
fn every_current_configuration_override_resolves_through_the_registry() {
    let registry = ShortcutRegistry::resolve(&complete_override(), ShortcutPlatform::Portable)
        .expect("valid complete override");
    assert_board_overrides(&registry);
    assert_editor_overrides(&registry);
}

fn complete_override() -> KeyBindings {
    KeyBindings {
        new: 'α',
        edit: 'β',
        delete: 'γ',
        copy: 'δ',
        cut: 'ε',
        submit_remove: 'ζ',
        submit_keep: 'η',
        undo: 'θ',
        focus_up: 'ι',
        focus_down: 'κ',
        range_up: 'λ',
        range_down: 'μ',
        collapse: 'ν',
        select: 'ξ',
        transform: 'ο',
        select_all: 'π',
        range_select: 'ρ',
        search: 'σ',
        commands: 'τ',
        help: 'υ',
        quit: 'φ',
        screenshot_inbox: 'χ',
        paste: 'b',
        delete_sentence: 'F',
        select_visual_row_start: 'M',
        select_visual_row_end: 'R',
    }
}

fn assert_board_overrides(registry: &ShortcutRegistry) {
    for (character, expected) in [
        ('α', Action::New),
        ('β', Action::Edit),
        ('γ', Action::Delete),
        ('δ', Action::Copy),
        ('ε', Action::Cut),
        ('ζ', Action::SubmitRemove),
        ('η', Action::SubmitKeep),
        ('θ', Action::Undo),
        ('ι', Action::FocusPrevious),
        ('κ', Action::FocusNext),
        ('λ', Action::ExtendPrevious),
        ('μ', Action::ExtendNext),
        ('ν', Action::Collapse),
        ('ξ', Action::Select),
        ('ο', Action::ContextualTransform),
        ('π', Action::SelectAll),
        ('ρ', Action::RangeSelect),
        ('σ', Action::OpenSearch),
        ('τ', Action::OpenCommands),
        ('υ', Action::Help),
        ('φ', Action::Quit),
        ('χ', Action::ScreenshotInbox),
        ('b', Action::PasteExact),
        ('B', Action::PasteReflow),
    ] {
        assert_eq!(
            dispatched(
                registry,
                Context::Board,
                LogicalKey::Character(character),
                LogicalModifiers::NONE,
            )
            .action,
            Some(expected),
            "configured binding {character:?}",
        );
    }
}

fn assert_editor_overrides(registry: &ShortcutRegistry) {
    for (character, expected) in [
        ('ο', Action::ContextualTransform),
        ('F', Action::DeleteSentence),
        ('M', Action::ExtendVisualRowStart),
        ('R', Action::ExtendVisualRowEnd),
    ] {
        let shifted = character.is_ascii_uppercase();
        let modifiers = LogicalModifiers::CONTROL.union(if shifted {
            LogicalModifiers::SHIFT
        } else {
            LogicalModifiers::NONE
        });
        assert_eq!(
            dispatched(
                registry,
                Context::Edit,
                LogicalKey::Character(character),
                modifiers,
            )
            .action,
            Some(expected),
        );
    }
}

#[test]
fn every_discovered_top_owner_precedes_the_underlying_board() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry");
    for (context, expected) in [
        (Context::Compose, None),
        (Context::Edit, None),
        (Context::Help, Some(Action::FocusNext)),
        (Context::Commands, None),
        (Context::Search, None),
        (Context::Invocation, None),
        (Context::Transfer, None),
        (Context::Browser, None),
        (Context::BrowserQuery, None),
        (Context::Rename, None),
        (Context::Update, Some(Action::FocusNext)),
        (Context::Screenshot, Some(Action::FocusNext)),
        (Context::Recovery, None),
        (Context::Direction, Some(Action::ChooseDown)),
        (Context::ReleaseHighlights, Some(Action::FocusNext)),
    ] {
        let result = registry
            .dispatch(
                &ShortcutContextStack::new([Context::Board, context]),
                stroke(LogicalKey::Character('j'), LogicalModifiers::NONE),
            )
            .expect("printable input remains classified");
        assert_eq!(result.action, expected, "top owner {context:?}");
    }
    assert_eq!(
        dispatched(
            &registry,
            Context::InsertionBoundary,
            LogicalKey::Enter,
            LogicalModifiers::NONE,
        )
        .action,
        Some(Action::New),
    );
}
