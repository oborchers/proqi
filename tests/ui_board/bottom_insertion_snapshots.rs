//! Representative narrow final-row append after the top-boundary insertion inverse.

use super::{
    Fixture, ThemePreference, UiInput, UiKey, draw_theme,
    navigation::{durable_thought, visual},
};

use super::snapshot_support::snapshot_buffer;
use proqi::ports::editor::CursorMovement;

#[test]
fn bottom_append_after_top_insertion_keeps_the_new_editor_at_the_durable_tail() {
    let mut fixture = Fixture::new();
    for index in 0..6 {
        durable_thought(
            &mut fixture,
            &format!("thought {index} wraps with Grüße 界 in a narrow board"),
        );
    }
    for _ in 1..6 {
        fixture.input(UiInput::Key(UiKey::Character('k')));
    }
    fixture.input(visual(CursorMovement::VisualUp, false));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Escape));
    for _ in 0..7 {
        fixture.input(visual(CursorMovement::VisualDown, false));
    }
    fixture.input(UiInput::Key(UiKey::Character('j')));
    fixture.input(visual(CursorMovement::VisualDown, false));

    let terminal = draw_theme(&mut fixture, 34, 9, ThemePreference::Dark);
    insta::assert_snapshot!(snapshot_buffer(terminal.backend().buffer()));
}
