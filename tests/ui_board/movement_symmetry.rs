use super::*;

#[test]
fn arrows_and_jk_share_focus_and_shift_range_intentions() {
    let mut arrows = Fixture::new();
    for content in ["first", "second", "third"] {
        durable_thought(&mut arrows, content);
    }
    arrows.input(visual(CursorMovement::VisualUp, false));
    let arrow_focus = arrows.app.state.focused_thought;

    let mut letters = Fixture::new();
    for content in ["first", "second", "third"] {
        durable_thought(&mut letters, content);
    }
    letters.input(UiInput::Key(UiKey::Character('k')));
    assert_eq!(letters.app.state.focused_thought, arrow_focus);

    arrows.input(visual(CursorMovement::VisualUp, true));
    letters.input(UiInput::Key(UiKey::Character('K')));
    let arrow_selected = selected(&arrows);
    let letter_selected = selected(&letters);
    assert_eq!(arrow_selected, ["first", "second"]);
    assert_eq!(letter_selected, arrow_selected);

    arrows.input(visual(CursorMovement::VisualDown, true));
    letters.input(UiInput::Key(UiKey::Character('J')));
    assert_eq!(selected(&arrows), ["second"]);
    assert_eq!(selected(&letters), ["second"]);
    assert_eq!(order(&letters), ["first", "second", "third"]);
}

#[test]
fn primary_shift_arrows_and_characters_share_reorder_intentions() {
    let mut arrows = Fixture::new();
    let mut letters = Fixture::new();
    for content in ["first", "second", "third"] {
        durable_thought(&mut arrows, content);
        durable_thought(&mut letters, content);
    }
    arrows.input(UiInput::Key(UiKey::PrimaryShiftMove {
        movement: CursorMovement::VisualUp,
    }));
    letters.input(UiInput::Key(UiKey::PrimaryCharacter('K')));
    assert_eq!(order(&letters), order(&arrows));
    assert_eq!(order(&letters), ["first", "third", "second"]);

    arrows.input(UiInput::Key(UiKey::PrimaryShiftMove {
        movement: CursorMovement::VisualDown,
    }));
    letters.input(UiInput::Key(UiKey::PrimaryCharacter('J')));
    assert_eq!(order(&arrows), ["first", "second", "third"]);
    assert_eq!(order(&letters), ["first", "second", "third"]);
}

#[test]
fn remapped_shifted_vertical_key_keeps_range_and_primary_reorder_semantics() {
    let mut settings = UiSettings::default();
    settings.keybindings.focus_up = 'i';
    settings.keybindings.focus_down = 'm';
    settings.keybindings.range_up = 'I';
    settings.keybindings.range_down = 'M';
    let mut fixture = Fixture::with_settings(settings);
    for content in ["first", "second", "third"] {
        durable_thought(&mut fixture, content);
    }

    fixture.input(UiInput::Key(UiKey::Character('I')));
    assert_eq!(selected(&fixture), ["second", "third"]);
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::PrimaryCharacter('I')));
    assert_eq!(order(&fixture), ["second", "first", "third"]);
}

fn selected(fixture: &Fixture) -> Vec<&str> {
    fixture
        .app
        .state
        .board
        .live_thoughts()
        .into_iter()
        .filter(|thought| fixture.app.thought_selected(thought.id))
        .map(|thought| thought.content.as_str())
        .collect()
}

fn order(fixture: &Fixture) -> Vec<&str> {
    fixture
        .app
        .state
        .board
        .live_thoughts()
        .into_iter()
        .map(|thought| thought.content.as_str())
        .collect()
}
