//! Standard shortcut discovery, scope, conservative no-op, and modal precedence contracts.

use super::{Fixture, draw, text};
use proqi::{
    application::Effect,
    domain::Direction,
    ui::{PointerButton, PointerInput, PointerKind, UiInput, UiKey},
};
use ratatui_core::layout::Rect;

fn primary() -> &'static str {
    if cfg!(target_os = "macos") {
        "Command+"
    } else {
        "Ctrl+"
    }
}

#[test]
fn board_help_discloses_standard_chords_and_portable_aliases() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));
    fixture.input(UiInput::Key(UiKey::Character('?')));
    let rendered = text(draw(&mut fixture, 150, 42).backend().buffer());
    for expected in [
        format!("{}C/y", primary()),
        format!("{}X/x", primary()),
        format!("{}Z/u", primary()),
        format!("{}A/a", primary()),
        format!("{}Enter/s", primary()),
        format!("{}Shift+Enter/S", primary()),
        format!("{}Q/q", primary()),
        format!("{}V", primary()),
        format!("{}D", primary()),
        format!("{}Shift+Z/{}Y", primary(), primary()),
    ] {
        assert!(
            rendered.contains(&expected),
            "missing {expected:?}: {rendered}"
        );
    }
    for action in [
        "Copy",
        "Cut",
        "Undo",
        "Select all",
        "Submit",
        "Submit & keep",
        "Quit",
        "Paste",
        "Redo",
        "Duplicate",
    ] {
        assert!(rendered.contains(action), "missing {action:?}: {rendered}");
    }
}

#[test]
fn edit_help_discloses_paste_and_both_existing_redo_spellings() {
    let mut fixture = Fixture::new();
    fixture.paste("edit help");
    fixture.app.help = true;
    let rendered = text(draw(&mut fixture, 150, 34).backend().buffer());
    assert!(rendered.contains(&format!("{}V", primary())));
    assert!(rendered.contains(&format!("{}Shift+Z/{}Y", primary(), primary())));
    assert!(rendered.contains("Paste"));
    assert!(rendered.contains("Redo"));
}

#[test]
fn full_board_footer_labels_and_mouse_targets_use_the_same_chord_projection() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    let rendered = text(draw(&mut fixture, 120, 12).backend().buffer());
    for expected in [
        format!("{}C/y Copy", primary()),
        format!("{}X/x Cut", primary()),
        format!("{}Z/u Undo", primary()),
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
fn shifted_reserved_character_chords_are_conservative_in_board_and_edit() {
    let shifted = ['A', 'C', 'D', 'Q', 'V', 'X', 'Y'];
    let mut board = Fixture::new();
    super::agent::prepare_thought(&mut board);
    let focused = board.app.state.focused_thought;
    for character in shifted {
        assert!(
            board
                .effects(UiInput::Key(UiKey::PrimaryShiftCharacter(character)))
                .is_empty()
        );
    }
    assert_eq!(board.app.state.focused_thought, focused);
    assert!(!board.app.quit);

    board.input(UiInput::Key(UiKey::Enter));
    let before = board.app.editor_snapshot().expect("editor");
    for character in shifted {
        assert!(
            board
                .effects(UiInput::Key(UiKey::PrimaryShiftCharacter(character)))
                .is_empty()
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
    fixture.input(UiInput::Key(UiKey::Character('?')));
    assert!(fixture.app.help);
    fixture.input(UiInput::Key(UiKey::Character('j')));
    assert!(fixture.app.help);
    let effects = fixture.effects(UiInput::Key(UiKey::Quit));
    assert!(effects.is_empty());
    assert!(fixture.app.quit);
}
