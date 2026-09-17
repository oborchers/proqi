use super::navigation::{durable_thought, visual};
use super::*;

#[test]
fn arrows_and_jk_share_focus_and_shift_range_intentions() {
    let mut arrows = Fixture::new();
    for content in ["first", "second", "third"] {
        durable_thought(&mut arrows, content);
    }
    arrows.input(visual(CursorMovement::VisualUp, false));
    let arrow_focus = arrows.app.state.focused_thought_id();

    let mut letters = Fixture::new();
    for content in ["first", "second", "third"] {
        durable_thought(&mut letters, content);
    }
    letters.input(crate::key_input(UiKey::Character('k')));
    assert_eq!(letters.app.state.focused_thought_id(), arrow_focus);

    arrows.input(visual(CursorMovement::VisualUp, true));
    letters.input(crate::key_input(UiKey::Character('K')));
    let arrow_selected = selected(&arrows);
    let letter_selected = selected(&letters);
    assert_eq!(arrow_selected, ["first", "second"]);
    assert_eq!(letter_selected, arrow_selected);

    arrows.input(visual(CursorMovement::VisualDown, true));
    letters.input(crate::key_input(UiKey::Character('J')));
    assert_eq!(selected(&arrows), ["second"]);
    assert_eq!(selected(&letters), ["second"]);
    assert_eq!(order(&letters), ["first", "second", "third"]);
}

#[test]
fn primary_shift_arrows_and_characters_share_reorder_intentions() {
    for (up, down) in [
        (
            UiKey::PrimaryShiftCharacter('K'),
            UiKey::PrimaryShiftCharacter('J'),
        ),
        (
            UiKey::PrimaryShiftCharacter('k'),
            UiKey::PrimaryShiftCharacter('j'),
        ),
    ] {
        let mut arrows = Fixture::new();
        let mut letters = Fixture::new();
        for content in ["first", "second", "third"] {
            durable_thought(&mut arrows, content);
            durable_thought(&mut letters, content);
        }
        arrows.input(crate::key_input(UiKey::PrimaryShiftMove {
            movement: CursorMovement::VisualUp,
        }));
        letters.input(crate::key_input(up));
        assert_eq!(order(&letters), order(&arrows));
        assert_eq!(order(&letters), ["first", "third", "second"]);

        arrows.input(crate::key_input(UiKey::PrimaryShiftMove {
            movement: CursorMovement::VisualDown,
        }));
        letters.input(crate::key_input(down));
        assert_eq!(order(&arrows), ["first", "second", "third"]);
        assert_eq!(order(&letters), ["first", "second", "third"]);
    }
}

#[test]
fn remapped_shifted_vertical_key_keeps_range_and_primary_reorder_semantics() {
    let settings = UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_legacy(&proqi::ui::KeyBindings {
            focus_up: 'i',
            focus_down: 'm',
            range_up: 'I',
            range_down: 'M',
            screenshot_inbox: 'b',
            ..proqi::ui::KeyBindings::default()
        })
        .expect("valid translated keymap"),
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    for content in ["first", "second", "third"] {
        durable_thought(&mut fixture, content);
    }

    fixture.input(crate::key_input(UiKey::Character('I')));
    assert_eq!(selected(&fixture), ["second", "third"]);
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::PrimaryShiftCharacter('i')));
    assert_eq!(order(&fixture), ["second", "first", "third"]);
}

pub(super) fn selected(fixture: &Fixture) -> Vec<&str> {
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

pub(super) fn order(fixture: &Fixture) -> Vec<&str> {
    fixture
        .app
        .state
        .board
        .live_thoughts()
        .into_iter()
        .map(|thought| thought.content.as_str())
        .collect()
}
