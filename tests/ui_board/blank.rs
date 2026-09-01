use super::*;

#[test]
fn explicit_blank_creation_is_durable_and_accepts_the_first_edit() {
    let mut fixture = Fixture::new();
    super::navigation::durable_thought(&mut fixture, "existing");
    let effects = fixture.effects(UiInput::Key(UiKey::Character('n')));
    assert_eq!(effects.len(), 1);
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
    assert!(
        fixture.app.state.board.live_thoughts()[1]
            .content
            .is_empty()
    );
    assert!(matches!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { .. }
    ));

    let effects = fixture.effects(UiInput::Key(UiKey::Character('界')));
    assert!(effects.is_empty());
    fixture.input(UiInput::Key(UiKey::Escape));
    assert_eq!(fixture.app.state.board.live_thoughts()[1].content, "界");
}

#[test]
fn escape_keeps_an_unchanged_durable_blank_and_its_history() {
    let mut fixture = Fixture::new();
    super::navigation::durable_thought(&mut fixture, "existing");
    fixture.input(UiInput::Key(UiKey::Character('n')));
    fixture.input(UiInput::Key(UiKey::Escape));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
    assert!(
        fixture.app.state.board.live_thoughts()[1]
            .content
            .is_empty()
    );
    assert_eq!(fixture.app.state.board_history().len(), 2);
}

#[test]
fn an_empty_clipboard_paste_is_a_no_op_but_an_explicit_blank_survives_exit() {
    let mut fixture = Fixture::new();
    assert!(fixture.effects(UiInput::Paste(String::new())).is_empty());
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    fixture.input(UiInput::Paste("existing".to_owned()));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character('n')));
    fixture.input(UiInput::Key(UiKey::Quit));
    assert!(fixture.app.quit);
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
    assert!(
        fixture.app.state.board.live_thoughts()[1]
            .content
            .is_empty()
    );
}

#[test]
fn focused_blank_keeps_board_shortcuts_available_until_edit_is_explicit() {
    let mut fixture = Fixture::new();
    super::navigation::durable_thought(&mut fixture, "existing");
    fixture.input(UiInput::Key(UiKey::Character('n')));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character('n')));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 3);
    assert!(
        fixture.app.state.board.live_thoughts()[1]
            .content
            .is_empty()
    );
}

#[test]
fn board_paste_creates_a_new_thought_even_when_a_blank_is_focused() {
    let mut fixture = Fixture::new();
    super::navigation::durable_thought(&mut fixture, "existing");
    fixture.input(UiInput::Key(UiKey::Character('n')));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Paste("pasted".to_owned()));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 3);
    assert!(
        fixture.app.state.board.live_thoughts()[1]
            .content
            .is_empty()
    );
    assert_eq!(fixture.app.state.board.live_thoughts()[2].content, "pasted");
}
