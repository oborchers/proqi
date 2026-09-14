use crate::{
    adapters::editor::RopeEditor,
    domain::TextPosition,
    ports::editor::{EditCommand, Editor as _, TextViewport, VisualCursorAffinity},
    ui::{VisualRowEdge, annotations::Presentation},
};

use super::editor_presentation;

fn cursor_cell(editor: &RopeEditor) -> Option<(usize, usize)> {
    let snapshot = editor.snapshot();
    editor_presentation(&snapshot, Presentation::canonical(snapshot.content.clone()))
        .cursor_viewport_cell()
}

fn move_to_visual_edge(editor: &mut RopeEditor, edge: VisualRowEdge) {
    let snapshot = editor.snapshot();
    let target = editor_presentation(&snapshot, Presentation::canonical(snapshot.content.clone()))
        .visual_row_edge(edge, false);
    let _moved = editor.apply(EditCommand::SetVisualCursor {
        position: target.position,
        affinity: target.affinity,
        extend_selection: false,
    });
}

#[test]
fn semantic_edges_converge_on_the_requested_exact_width_row() {
    let mut editor = RopeEditor::new(&"x".repeat(56));
    editor.set_viewport(TextViewport::new(14, 3));
    let _positioned = editor.apply(EditCommand::SetVisualCursor {
        position: TextPosition::new(0, 15),
        affinity: VisualCursorAffinity::NextRow,
        extend_selection: false,
    });

    move_to_visual_edge(&mut editor, VisualRowEdge::Start);
    assert_eq!(editor.snapshot().cursor, TextPosition::new(0, 14));
    assert_eq!(cursor_cell(&editor), Some((0, 1)));
    move_to_visual_edge(&mut editor, VisualRowEdge::Start);
    assert_eq!(cursor_cell(&editor), Some((0, 1)));

    let _positioned = editor.apply(EditCommand::SetVisualCursor {
        position: TextPosition::new(0, 15),
        affinity: VisualCursorAffinity::NextRow,
        extend_selection: false,
    });
    move_to_visual_edge(&mut editor, VisualRowEdge::End);
    let at_end = editor.snapshot();
    assert_eq!(at_end.cursor, TextPosition::new(0, 28));
    assert_eq!(at_end.cursor_affinity, VisualCursorAffinity::PreviousRow);
    assert_eq!(cursor_cell(&editor), Some((13, 1)));
    move_to_visual_edge(&mut editor, VisualRowEdge::End);
    assert_eq!(editor.snapshot(), at_end);
    assert_eq!(cursor_cell(&editor), Some((13, 1)));
}

#[test]
fn semantic_end_preserves_the_exact_width_synthetic_final_row() {
    let mut editor = RopeEditor::new(&"x".repeat(28));
    editor.set_viewport(TextViewport::new(14, 3));
    let _positioned = editor.apply(EditCommand::SetVisualCursor {
        position: TextPosition::new(0, 28),
        affinity: VisualCursorAffinity::NextRow,
        extend_selection: false,
    });

    move_to_visual_edge(&mut editor, VisualRowEdge::End);
    let at_end = editor.snapshot();
    assert_eq!(at_end.cursor, TextPosition::new(0, 28));
    assert_eq!(at_end.cursor_affinity, VisualCursorAffinity::NextRow);
    assert_eq!(cursor_cell(&editor), Some((0, 2)));
    move_to_visual_edge(&mut editor, VisualRowEdge::End);
    assert_eq!(editor.snapshot(), at_end);
    assert_eq!(cursor_cell(&editor), Some((0, 2)));
}

#[test]
fn mutation_undo_and_noop_preserve_rendered_visual_affinity() {
    let mut editor = RopeEditor::new("abcdefgh");
    editor.set_viewport(TextViewport::new(4, 3));
    let boundary = TextPosition::new(0, 4);
    let _positioned = editor.apply(EditCommand::SetVisualCursor {
        position: boundary,
        affinity: VisualCursorAffinity::PreviousRow,
        extend_selection: false,
    });
    assert_eq!(cursor_cell(&editor), Some((3, 0)));

    let _inserted = editor.apply(EditCommand::InsertChar('!'));
    let undone = editor.apply(EditCommand::Undo).snapshot;
    assert_eq!(undone.cursor, boundary);
    assert_eq!(undone.cursor_affinity, VisualCursorAffinity::PreviousRow);
    assert_eq!(cursor_cell(&editor), Some((3, 0)));

    let _positioned = editor.apply(EditCommand::SetVisualCursor {
        position: TextPosition::new(0, 8),
        affinity: VisualCursorAffinity::PreviousRow,
        extend_selection: false,
    });
    let before_noop = cursor_cell(&editor);
    let deleted = editor.apply(EditCommand::DeleteForward);
    assert!(deleted.changes.is_empty());
    assert_eq!(
        deleted.snapshot.cursor_affinity,
        VisualCursorAffinity::PreviousRow
    );
    assert_eq!(cursor_cell(&editor), before_noop);
}
