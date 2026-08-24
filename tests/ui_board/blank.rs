use super::*;

#[test]
fn explicit_blank_creation_is_durable_and_accepts_the_first_edit() {
    let mut fixture = Fixture::new();
    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(effects.len(), 1);
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert!(
        fixture.app.state.board.live_thoughts()[0]
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
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, "界");
}

#[test]
fn escape_keeps_an_unchanged_durable_blank_and_its_history() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Escape));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert!(
        fixture.app.state.board.live_thoughts()[0]
            .content
            .is_empty()
    );
    assert_eq!(fixture.app.state.board_history().len(), 1);
}

#[test]
fn an_empty_clipboard_paste_is_a_no_op_but_an_explicit_blank_survives_exit() {
    let mut fixture = Fixture::new();
    assert!(fixture.effects(UiInput::Paste(String::new())).is_empty());
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Quit));
    assert!(fixture.app.quit);
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
}

#[test]
fn focused_blank_treats_printable_shortcut_letters_as_content() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Escape));
    for character in ['n', 'd', 'j', 'x'] {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    fixture.input(UiInput::Key(UiKey::Escape));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, "ndjx");
}

#[test]
fn paste_populates_a_focused_blank_instead_of_creating_another_thought() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Paste("pasted".to_owned()));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, "pasted");
}
