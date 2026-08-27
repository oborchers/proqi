use super::*;

use proqi::application::InteractionMode;

#[test]
fn two_down_movements_create_the_first_thought_and_enter_edit_mode() {
    let mut fixture = Fixture::new();

    assert!(
        fixture
            .effects(super::navigation::visual(CursorMovement::VisualDown, false))
            .is_empty()
    );
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    let effects = fixture.effects(super::navigation::visual(CursorMovement::VisualDown, false));

    assert_eq!(effects.len(), 1);
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert!(matches!(
        fixture.app.interaction_mode(),
        InteractionMode::Edit { .. }
    ));
    assert_eq!(
        fixture.app.editor_snapshot().expect("blank editor").content,
        ""
    );
}

#[test]
fn configured_next_and_arrow_down_share_the_insertion_confirmation() {
    let mut settings = UiSettings::default();
    settings.keybindings.focus_down = 'g';
    let mut fixture = Fixture::with_settings(settings);
    super::navigation::durable_thought(&mut fixture, "existing");
    fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));
    assert!(fixture.app.insertion_focused());

    fixture.input(UiInput::Key(UiKey::Character('g')));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));

    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
    assert!(matches!(
        fixture.app.interaction_mode(),
        InteractionMode::Edit { .. }
    ));
}

#[test]
fn unrelated_input_resets_insertion_confirmation() {
    let mut fixture = Fixture::new();
    fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));
    fixture.input(UiInput::Key(UiKey::Character('?')));
    fixture.input(UiInput::Key(UiKey::Escape));

    fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
}

#[test]
fn shifted_down_does_not_arm_insertion_creation() {
    let mut fixture = Fixture::new();
    fixture.input(super::navigation::visual(CursorMovement::VisualDown, true));
    fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));

    assert!(fixture.app.state.board.live_thoughts().is_empty());
}
