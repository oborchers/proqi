use crate::{
    adapters::editor::RopeEditor,
    domain::TextPosition,
    ports::editor::{EditCommand, Editor as _, TextViewport, VisualCursorAffinity},
    ui::annotations::Presentation,
};

use super::editor_presentation;

fn cursor_cell(editor: &RopeEditor) -> Option<(usize, usize)> {
    let snapshot = editor.snapshot();
    editor_presentation(&snapshot, Presentation::canonical(snapshot.content.clone()))
        .cursor_viewport_cell()
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
