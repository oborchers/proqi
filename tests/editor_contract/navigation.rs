use proqi::{
    adapters::editor::RopeEditor,
    domain::TextPosition,
    ports::editor::{CursorMovement, EditCommand, Editor, FAST_NAVIGATION_ROWS, TextViewport},
};

fn move_cursor(editor: &mut impl Editor, movement: CursorMovement, extend_selection: bool) {
    let _outcome = editor.apply(EditCommand::Move {
        movement,
        extend_selection,
    });
}

#[test]
fn five_row_jumps_use_canonical_wrapped_cells_and_restore_the_preferred_column() {
    for target in ["x", "\tZ", "界界q", "e\u{301}🙂a", "\u{1}bc"] {
        let content = [
            "0123456789",
            "one",
            "two",
            "three",
            "four",
            target,
            "six",
            "seven",
            "eight",
            "nine",
            "uvwxyz0123",
        ]
        .join("\n");
        let mut editor = RopeEditor::new(&content);
        editor.set_viewport(TextViewport::new(20, 20));
        let _outcome = editor.apply(EditCommand::SetCursor {
            position: TextPosition::new(0, 6),
            extend_selection: false,
        });
        let expected_short_row = editor.position_at_cell(5, 6);

        move_cursor(&mut editor, CursorMovement::VisualJumpDown, false);
        assert_eq!(
            editor.snapshot().cursor,
            expected_short_row,
            "target {target:?}"
        );
        move_cursor(&mut editor, CursorMovement::VisualJumpDown, false);
        assert_eq!(
            editor.snapshot().cursor,
            TextPosition::new(10, 6),
            "preferred column after {target:?}"
        );

        move_cursor(&mut editor, CursorMovement::VisualJumpUp, false);
        assert_eq!(
            editor.snapshot().cursor,
            expected_short_row,
            "target {target:?}"
        );
        move_cursor(&mut editor, CursorMovement::VisualJumpUp, false);
        assert_eq!(editor.snapshot().cursor, TextPosition::new(0, 6));
    }
    assert_eq!(FAST_NAVIGATION_ROWS, 5);
}

#[test]
fn five_row_jumps_reflow_without_losing_the_preferred_cell_or_visible_cursor() {
    let mut editor = RopeEditor::new("0123456789abcdefghijklmnopqrstuvwxyz");
    editor.set_viewport(TextViewport::new(4, 3));
    let _outcome = editor.apply(EditCommand::SetCursor {
        position: TextPosition::new(0, 2),
        extend_selection: false,
    });

    move_cursor(&mut editor, CursorMovement::VisualJumpDown, false);
    assert_eq!(editor.snapshot().cursor, TextPosition::new(0, 22));
    assert_eq!(editor.snapshot().scroll_row, 3);

    editor.set_viewport(TextViewport::new(3, 2));
    move_cursor(&mut editor, CursorMovement::VisualJumpUp, false);
    let upward = editor.snapshot();
    assert_eq!(upward.cursor, TextPosition::new(0, 8));
    assert!(upward.scroll_row <= 2 && 2 < upward.scroll_row + 2);

    move_cursor(&mut editor, CursorMovement::VisualJumpDown, false);
    let downward = editor.snapshot();
    assert_eq!(downward.cursor, TextPosition::new(0, 23));
    assert!(downward.scroll_row <= 7 && 7 < downward.scroll_row + 2);
}

#[test]
fn accelerated_and_boundary_movements_clamp_and_follow_existing_selection_rules() {
    let content = (0..8)
        .map(|row| format!("row {row}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut editor = RopeEditor::new(&content);
    editor.set_viewport(TextViewport::new(20, 2));

    move_cursor(&mut editor, CursorMovement::VisualJumpUp, false);
    assert_eq!(editor.snapshot().cursor, TextPosition::new(0, 0));
    move_cursor(&mut editor, CursorMovement::DocumentEnd, true);
    assert!(editor.snapshot().selection.is_some());
    move_cursor(&mut editor, CursorMovement::VisualJumpUp, false);
    assert_eq!(editor.snapshot().cursor, TextPosition::new(2, 5));
    assert_eq!(editor.snapshot().selection, None);
    move_cursor(&mut editor, CursorMovement::VisualJumpDown, false);
    assert_eq!(editor.snapshot().cursor, TextPosition::new(7, 5));
    move_cursor(&mut editor, CursorMovement::VisualJumpDown, false);
    assert_eq!(editor.snapshot().cursor, TextPosition::new(7, 5));

    move_cursor(&mut editor, CursorMovement::DocumentStart, false);
    assert_eq!(editor.snapshot().cursor, TextPosition::new(0, 0));
    move_cursor(&mut editor, CursorMovement::DocumentEnd, false);
    assert_eq!(editor.snapshot().cursor, TextPosition::new(7, 5));
}
