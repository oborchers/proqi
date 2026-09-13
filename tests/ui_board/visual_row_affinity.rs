//! Cursor-affinity contracts at shared and synthetic wrapped-row boundaries.

use super::{Fixture, draw};
use proqi::{
    domain::TextPosition,
    ports::editor::{CursorMovement, VisualCursorAffinity, VisualLine},
    ui::{UiKey, VisualRowEdge},
};
use ratatui_core::{backend::Backend as _, layout::Rect};

fn move_to_grapheme(fixture: &mut Fixture, grapheme: usize) {
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    for _ in 0..grapheme {
        fixture.input(crate::key_input(UiKey::Move {
            movement: CursorMovement::GraphemeForward,
            extend_selection: false,
        }));
    }
}

fn row_end(row: &VisualLine) -> TextPosition {
    TextPosition::new(row.logical_line, row.end_grapheme)
}

fn move_to_end(fixture: &mut Fixture) {
    fixture.input(crate::key_input(UiKey::MoveVisualRow {
        edge: VisualRowEdge::End,
    }));
}

#[test]
fn exact_width_synthetic_final_row_keeps_next_row_affinity() {
    let mut fixture = Fixture::new();
    fixture.paste(&"x".repeat(28));
    let _frame = draw(&mut fixture, 16, 8);
    let rows = fixture.app.editor_snapshot().expect("editor").visual_lines;
    let final_index = rows.len() - 1;
    let final_row = &rows[final_index];
    assert_eq!(final_row.start_grapheme, final_row.end_grapheme);
    move_to_grapheme(&mut fixture, final_row.end_grapheme);

    move_to_end(&mut fixture);
    move_to_end(&mut fixture);

    let snapshot = fixture.app.editor_snapshot().expect("synthetic row end");
    assert_eq!(snapshot.cursor, row_end(final_row));
    assert_eq!(snapshot.cursor_affinity, VisualCursorAffinity::NextRow);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 16, 8)).thoughts[0].text_area;
    let mut terminal = draw(&mut fixture, 16, 8);
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("cursor");
    assert_eq!(cursor.x, area.x);
    assert_eq!(cursor.y, area.y + u16::try_from(final_index).expect("row"));
}
