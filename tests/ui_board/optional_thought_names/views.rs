//! Responsive optional-name presentation fixtures.

use super::{control_r, named_fixture};
use proqi::{
    domain::ThoughtPresentation,
    ui::{BoardDensity, ThemePreference, UiKey, UiSettings},
};
use ratatui_core::{backend::Backend as _, layout::Rect};

use super::super::{draw_theme, snapshot_support::snapshot_buffer};

pub(super) fn comfortable() -> String {
    let mut fixture = named_fixture(
        UiSettings::default(),
        "A body that remains visually separate from organizational metadata.",
        "Release 計画",
    );
    fixture.input(crate::key_input(UiKey::Escape));
    snapshot_buffer(
        draw_theme(&mut fixture, 58, 9, ThemePreference::Dark)
            .backend()
            .buffer(),
    )
}

pub(super) fn compact_and_collapsed() -> (String, String) {
    let settings = UiSettings {
        density: BoardDensity::Compact,
        ..UiSettings::default()
    };
    let mut fixture = named_fixture(settings, "Compact body line one\nline two", "Compact title");
    fixture.input(crate::key_input(UiKey::Escape));
    let compact = snapshot_buffer(
        draw_theme(&mut fixture, 42, 7, ThemePreference::Dark)
            .backend()
            .buffer(),
    );
    let thought_id = fixture.app.state.board.live_thoughts()[0].id;
    fixture
        .app
        .state
        .board
        .thought_mut(thought_id)
        .expect("thought")
        .presentation = ThoughtPresentation::Collapsed;
    let collapsed = snapshot_buffer(
        draw_theme(&mut fixture, 24, 5, ThemePreference::Dark)
            .backend()
            .buffer(),
    );
    (compact, collapsed)
}

pub(super) fn shallow() -> String {
    let settings = UiSettings {
        density: BoardDensity::Compact,
        ..UiSettings::default()
    };
    let mut fixture = named_fixture(settings, "Compact body line one\nline two", "Compact title");
    let thought_id = fixture.app.state.board.live_thoughts()[0].id;
    fixture.input(crate::key_input(UiKey::Escape));
    fixture
        .app
        .state
        .board
        .thought_mut(thought_id)
        .expect("thought")
        .presentation = ThoughtPresentation::Collapsed;
    let snapshot = snapshot_buffer(
        draw_theme(&mut fixture, 18, 4, ThemePreference::Dark)
            .backend()
            .buffer(),
    );
    fixture.input(crate::key_input(UiKey::Enter));
    let mut terminal = draw_theme(&mut fixture, 18, 4, ThemePreference::Dark);
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 18, 4));
    let body = layout
        .thought(thought_id)
        .expect("shallow body layout")
        .text_area;
    assert_eq!(body.height, 1);
    assert_eq!(
        terminal
            .backend_mut()
            .get_cursor_position()
            .expect("visible shallow body cursor")
            .y,
        body.y
    );
    snapshot
}

pub(super) fn editing_narrow() -> String {
    let long = "界".repeat(100);
    let mut fixture = named_fixture(UiSettings::default(), "body stays separate", &long);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0]
            .name
            .as_ref()
            .expect("bounded name")
            .as_str()
            .chars()
            .count(),
        80
    );
    fixture.input(control_r());
    let mut terminal = draw_theme(&mut fixture, 24, 5, ThemePreference::Dark);
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("title cursor");
    assert_eq!(cursor.y, 0);
    assert!(cursor.x < 24);
    snapshot_buffer(terminal.backend().buffer())
}
