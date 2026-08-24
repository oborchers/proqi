use super::*;

#[test]
fn blank_creation_stays_ephemeral_until_the_first_non_empty_edit() {
    let mut fixture = Fixture::new();
    assert!(
        fixture
            .effects(UiInput::Key(UiKey::Character('n')))
            .is_empty()
    );
    assert!(fixture.app.has_draft());
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert!(matches!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { .. }
    ));

    let effects = fixture.effects(UiInput::Key(UiKey::Character('界')));
    assert_eq!(effects.len(), 1);
    assert!(!fixture.app.has_draft());
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, "界");
}

#[test]
fn escape_discards_an_unchanged_draft_without_history() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Character('n')));
    fixture.input(UiInput::Key(UiKey::Escape));
    assert!(!fixture.app.has_draft());
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert!(fixture.app.state.board_history().is_empty());
}

#[test]
fn empty_board_paste_and_exit_do_not_leave_empty_thoughts() {
    let mut fixture = Fixture::new();
    assert!(fixture.effects(UiInput::Paste(String::new())).is_empty());
    assert!(!fixture.app.has_draft());
    fixture.input(UiInput::Key(UiKey::Character('n')));
    fixture.input(UiInput::Key(UiKey::Quit));
    assert!(fixture.app.quit);
    assert!(!fixture.app.has_draft());
    assert!(fixture.app.state.board.live_thoughts().is_empty());
}
