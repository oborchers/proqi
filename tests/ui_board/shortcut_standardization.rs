//! Standard shortcut discovery, scope, conservative no-op, and modal precedence contracts.

use super::{Fixture, draw, text};
use proqi::{
    application::Effect,
    domain::Direction,
    ports::editor::CursorMovement,
    ui::{KeyStroke, LogicalKey, PointerButton, PointerInput, PointerKind, UiInput, UiKey},
};
use ratatui_core::layout::Rect;

fn primary() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd+"
    } else {
        "Ctrl+"
    }
}

#[test]
fn two_raw_arrows_keep_editor_ownership_until_neighbor_navigation_completes() {
    let mut fixture = Fixture::new();
    super::navigation::durable_thought(&mut fixture, "first");
    super::navigation::durable_thought(&mut fixture, "second");
    let first = fixture.app.state.board.live_thoughts()[0].id;
    fixture.input(crate::key_input(UiKey::Enter));
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));

    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Up)));
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Up)));

    assert_eq!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Board
    );
    assert_eq!(fixture.app.state.focused_thought, Some(first));
}

#[test]
fn printable_board_alias_after_one_raw_boundary_arrow_remains_editor_text() {
    let mut fixture = Fixture::new();
    super::navigation::durable_thought(&mut fixture, "first");
    super::navigation::durable_thought(&mut fixture, "second");
    fixture.input(crate::key_input(UiKey::Enter));
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));

    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Up)));
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Character(
        'n',
    ))));

    assert!(matches!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { .. }
    ));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "nsecond"
    );
}

#[test]
fn board_help_discloses_standard_non_history_chords_and_portable_aliases() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    super::navigation::durable_thought(&mut fixture, "redo candidate");
    fixture.input(crate::key_input(UiKey::Undo));
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));
    fixture.input(crate::key_input(UiKey::Character('?')));
    let rendered = text(draw(&mut fixture, 150, 42).backend().buffer());
    for expected in [
        format!("{}C/y", primary()),
        format!("{}X/x", primary()),
        format!("{}A/a", primary()),
        format!("{}Q/q", primary()),
        format!("{}V", primary()),
        format!("{}D", primary()),
        "PageUp/PageDown".to_owned(),
        "Shift+PageUp/PageDown".to_owned(),
    ] {
        assert!(
            rendered.contains(&expected),
            "missing {expected:?}: {rendered}"
        );
    }
    let submission_labels = if cfg!(target_os = "macos") {
        ["Ctrl+Enter", "Ctrl+Shift+Enter"]
    } else {
        ["Ctrl+Enter/s", "Ctrl+Shift+Enter/S"]
    };
    for expected in submission_labels {
        assert!(
            rendered.contains(expected),
            "missing {expected:?}: {rendered}"
        );
    }
    for action in [
        "Copy",
        "Cut",
        "Select all",
        "Submit",
        "Submit & keep",
        "Quit",
        "Paste",
        "Duplicate",
        "Move 5",
        "Range 5",
    ] {
        assert!(rendered.contains(action), "missing {action:?}: {rendered}");
    }
    assert!(!rendered.contains("Undo") && !rendered.contains("Redo"));
}

#[test]
fn edit_help_hides_history_but_discloses_paste() {
    let mut fixture = Fixture::new();
    fixture.paste("edit help");
    fixture.input(crate::key_input(UiKey::Character('!')));
    fixture.input(crate::key_input(UiKey::Undo));
    fixture.app.help = true;
    let rendered = text(draw(&mut fixture, 150, 34).backend().buffer());
    assert!(rendered.contains(&format!("{}V", primary())));
    assert!(rendered.contains("Paste"));
    assert!(!rendered.contains("Undo") && !rendered.contains("Redo"));
}

#[test]
fn full_board_footer_labels_and_mouse_targets_use_the_same_chord_projection() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    let rendered = text(draw(&mut fixture, 120, 12).backend().buffer());
    for expected in [
        format!("{}C/y Copy", primary()),
        format!("{}X/x Cut", primary()),
        if cfg!(target_os = "macos") {
            "Ctrl+Z Undo".to_owned()
        } else {
            format!("{}Z/u Undo", primary())
        },
    ] {
        assert!(
            rendered.contains(&expected),
            "missing {expected:?}: {rendered}"
        );
    }

    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 120, 12));
    let copy = layout
        .controls
        .iter()
        .find_map(|(target, area)| (*target == proqi::ui::HitTarget::Copy).then_some(*area))
        .expect("copy target");
    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: copy.right().saturating_sub(1),
        row: copy.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert!(matches!(
        effects.as_slice(),
        [Effect::WriteClipboard { .. }]
    ));
}

#[test]
fn commands_advertise_only_the_visible_query_owners_available_history() {
    let mut fixture = Fixture::new();
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Character(':')));
    let (_, entries, _) = fixture.app.palette_view().expect("Commands");
    assert!(!entries.iter().any(|entry| entry == "Undo"));
    assert!(!entries.iter().any(|entry| entry == "Redo"));

    fixture.input(crate::key_input(UiKey::Character('u')));
    let (query, entries, _) = fixture.app.palette_view().expect("Commands");
    assert!(
        entries.iter().any(|entry| entry == "Undo"),
        "query {query:?}, entries {entries:?}"
    );
    assert!(!entries.iter().any(|entry| entry == "Redo"));

    fixture.input(crate::key_input(UiKey::Undo));
    let (query, entries, _) = fixture.app.palette_view().expect("Commands");
    assert_eq!(query, "");
    assert!(!entries.iter().any(|entry| entry == "Undo"));
    assert!(entries.iter().any(|entry| entry == "Redo"));
}

#[test]
fn blocking_help_hides_history_while_surface_footer_and_mouse_share_availability() {
    let mut fixture = Fixture::new();
    fixture.app.help = true;
    let empty_help = text(draw(&mut fixture, 150, 42).backend().buffer());
    assert!(!empty_help.contains(" Undo"));
    assert!(!empty_help.contains(" Redo"));
    fixture.app.help = false;
    let empty = fixture.app.prepare_frame(Rect::new(0, 0, 120, 12));
    assert!(!empty.controls.iter().any(|(target, _)| {
        matches!(
            target,
            proqi::ui::HitTarget::Undo | proqi::ui::HitTarget::Redo
        )
    }));

    super::navigation::durable_thought(&mut fixture, "history");
    fixture.app.help = true;
    let undo_help = text(draw(&mut fixture, 150, 42).backend().buffer());
    assert!(!undo_help.contains(" Undo"));
    assert!(!undo_help.contains(" Redo"));
    fixture.app.help = false;
    let undo_layout = fixture.app.prepare_frame(Rect::new(0, 0, 120, 12));
    let undo = undo_layout
        .controls
        .iter()
        .find_map(|(target, area)| (*target == proqi::ui::HitTarget::Undo).then_some(*area))
        .expect("available Undo geometry");
    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: undo.x,
        row: undo.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitHistoryMove { undo: true, .. }]
    ));

    let redo_layout = fixture.app.prepare_frame(Rect::new(0, 0, 120, 12));
    assert!(
        !redo_layout
            .controls
            .iter()
            .any(|(target, _)| *target == proqi::ui::HitTarget::Undo)
    );
    let redo = redo_layout
        .controls
        .iter()
        .find_map(|(target, area)| (*target == proqi::ui::HitTarget::Redo).then_some(*area))
        .expect("available Redo geometry");
    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: redo.x,
        row: redo.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitHistoryMove { undo: false, .. }]
    ));
}

#[test]
fn shifted_reserved_character_chords_are_conservative_in_board_and_edit() {
    let shifted = ['A', 'C', 'D', 'Q', 'X', 'Y'];
    let mut board = Fixture::new();
    super::agent::prepare_thought(&mut board);
    let focused = board.app.state.focused_thought;
    for character in shifted {
        assert!(
            board
                .effects(crate::key_input(UiKey::PrimaryShiftCharacter(character)))
                .is_empty(),
            "board chord {character:?}"
        );
    }
    assert_eq!(board.app.state.focused_thought, focused);
    assert!(!board.app.quit);

    board.input(crate::key_input(UiKey::Enter));
    let before = board.app.editor_snapshot().expect("editor");
    for character in shifted {
        assert!(
            board
                .effects(crate::key_input(UiKey::PrimaryShiftCharacter(character)))
                .is_empty(),
            "edit chord {character:?}"
        );
    }
    assert_eq!(
        board.app.editor_snapshot().expect("unchanged editor"),
        before
    );
    assert!(!board.app.quit);
}

#[test]
fn global_quit_precedes_help_while_help_navigation_keeps_modal_precedence() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.input(crate::key_input(UiKey::Character('?')));
    assert!(fixture.app.help);
    fixture.input(crate::key_input(UiKey::Character('j')));
    assert!(fixture.app.help);
    let effects = fixture.effects(crate::key_input(UiKey::Quit));
    assert!(effects.is_empty());
    assert!(fixture.app.quit);
}
