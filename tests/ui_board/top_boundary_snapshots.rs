//! Representative complete-board rendering after top-boundary creation.

use super::navigation::{durable_thought, visual};
use super::*;

use super::snapshot_support::snapshot_buffer;

#[test]
fn top_boundary_blank_and_cursor_are_immediately_visible() {
    let mut fixture = Fixture::new();
    durable_thought(
        &mut fixture,
        "Former first thought wraps across a narrow pane with Grüße and 界.",
    );
    fixture.input(visual(CursorMovement::VisualUp, false));
    fixture.input(UiInput::Key(UiKey::Character('k')));

    let terminal = draw_theme(&mut fixture, 38, 8, ThemePreference::Dark);
    insta::assert_snapshot!(snapshot_buffer(terminal.backend().buffer()));
}
