//! Reusable behavioral contract for multiline editor backends.

use proqi::adapters::editor::RopeEditor;
use proqi::{
    domain::TextPosition,
    ports::editor::{CursorMovement, EditCommand, Editor, SelectionGranularity, TextViewport},
};

fn editor(text: &str) -> impl Editor {
    RopeEditor::new(text)
}

#[test]
fn large_multiline_paste_is_one_exact_undo_unit() {
    let mut editor = editor("");
    let unit = "Grüße e\u{301} 日本語 👩🏽‍💻 مرحبا\r\n";
    let paste = unit.repeat(20_000);

    let inserted = editor.apply(EditCommand::Paste(paste.clone()));
    assert!(inserted.content_changed);
    assert_eq!(inserted.snapshot.content, paste);

    let undone = editor.apply(EditCommand::Undo);
    assert!(undone.content_changed);
    assert_eq!(undone.snapshot.content, "");
    assert!(!editor.apply(EditCommand::Undo).content_changed);

    let redone = editor.apply(EditCommand::Redo);
    assert_eq!(redone.snapshot.content, paste);
}

#[test]
fn cursor_moves_by_grapheme_without_splitting_unicode() {
    let samples = ["ä", "e\u{301}", "👩🏽‍💻", "日本", "مرحبا"];
    let text = samples.join(" ");
    let expected_graphemes =
        unicode_segmentation::UnicodeSegmentation::graphemes(text.as_str(), true).count();
    let mut editor = editor(&text);

    for expected in 1..=expected_graphemes {
        editor.apply(EditCommand::Move {
            movement: CursorMovement::GraphemeForward,
            extend_selection: false,
        });
        assert_eq!(editor.snapshot().cursor.grapheme, expected);
    }
    assert_eq!(editor.snapshot().content, text);
}

#[test]
fn logical_cursor_and_selection_are_backend_independent() {
    let mut editor = editor("first\nGrüße 日本語\nthird");
    editor.apply(EditCommand::SetCursor {
        position: TextPosition::new(1, 2),
        extend_selection: false,
    });
    editor.apply(EditCommand::Move {
        movement: CursorMovement::LineEnd,
        extend_selection: true,
    });

    let snapshot = editor.snapshot();
    let selection = snapshot.selection.expect("selection");
    assert_eq!(selection.start, TextPosition::new(1, 2));
    assert_eq!(selection.end, snapshot.cursor);
    assert_eq!(editor.selected_text().as_deref(), Some("üße 日本語"));
}

#[test]
fn mouse_cursor_and_drag_selection_follow_wrapped_cells() {
    let mut editor = editor("ab日本cd");
    editor.set_viewport(TextViewport::new(4, 3));

    editor.apply(EditCommand::PointerStart {
        position: editor.position_at_cell(0, 2),
        granularity: SelectionGranularity::Grapheme,
        extend_selection: false,
    });
    assert_eq!(editor.snapshot().cursor, TextPosition::new(0, 2));

    editor.apply(EditCommand::PointerDrag {
        position: editor.position_at_cell(1, 3),
    });
    assert_eq!(editor.selected_text().as_deref(), Some("日本c"));
}

#[test]
fn cursor_crosses_logical_and_wrapped_line_boundaries() {
    let mut editor = editor("ab\ncd");
    editor.set_viewport(TextViewport::new(1, 2));
    editor.apply(EditCommand::SetCursor {
        position: TextPosition::new(0, 2),
        extend_selection: false,
    });
    editor.apply(EditCommand::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: false,
    });
    assert_eq!(editor.snapshot().cursor, TextPosition::new(1, 0));

    editor.apply(EditCommand::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    });
    assert_eq!(editor.snapshot().cursor, TextPosition::new(1, 1));
}

#[test]
fn pointer_click_without_drag_does_not_create_an_empty_selection() {
    let mut editor = editor("abc");
    editor.apply(EditCommand::PointerStart {
        position: editor.position_at_cell(0, 2),
        granularity: SelectionGranularity::Grapheme,
        extend_selection: false,
    });
    assert_eq!(editor.snapshot().selection, None);

    let deleted = editor.apply(EditCommand::DeleteBack);
    assert_eq!(deleted.snapshot.content, "ac");
}

#[test]
fn pointer_word_selection_uses_unicode_boundaries_and_safe_fallbacks() {
    let text = "alpha_beta Grüße e\u{301} 👩🏽‍💻!";
    let mut editor = editor(text);

    for (position, expected) in [
        (TextPosition::new(0, 12), "Grüße"),
        (TextPosition::new(0, 17), "e\u{301}"),
        (TextPosition::new(0, 19), "👩🏽‍💻"),
        (TextPosition::new(0, 20), "!"),
    ] {
        editor.apply(EditCommand::PointerStart {
            position,
            granularity: SelectionGranularity::Word,
            extend_selection: false,
        });
        assert_eq!(editor.selected_text().as_deref(), Some(expected));
        editor.apply(EditCommand::PointerEnd);
    }

    editor.apply(EditCommand::PointerStart {
        position: TextPosition::new(0, 21),
        granularity: SelectionGranularity::Word,
        extend_selection: false,
    });
    assert_eq!(editor.selected_text(), None);
}

#[test]
fn pointer_word_drag_extends_in_both_directions_by_complete_words() {
    let mut editor = editor("first middle last");
    editor.apply(EditCommand::PointerStart {
        position: TextPosition::new(0, 7),
        granularity: SelectionGranularity::Word,
        extend_selection: false,
    });
    assert_eq!(editor.selected_text().as_deref(), Some("middle"));

    editor.apply(EditCommand::PointerDrag {
        position: TextPosition::new(0, 16),
    });
    assert_eq!(editor.selected_text().as_deref(), Some("middle last"));

    editor.apply(EditCommand::PointerDrag {
        position: TextPosition::new(0, 1),
    });
    assert_eq!(editor.selected_text().as_deref(), Some("first middle"));
}

#[test]
fn pointer_line_selection_uses_logical_lines_and_preserves_delimiters() {
    let mut editor = editor("one\r\ntwo\n\nlast");
    for (position, expected) in [
        (TextPosition::new(0, 1), "one\r\n"),
        (TextPosition::new(1, 1), "two\n"),
        (TextPosition::new(2, 0), "\n"),
        (TextPosition::new(3, 2), "last"),
    ] {
        editor.apply(EditCommand::PointerStart {
            position,
            granularity: SelectionGranularity::LogicalLine,
            extend_selection: false,
        });
        assert_eq!(editor.selected_text().as_deref(), Some(expected));
        editor.apply(EditCommand::PointerEnd);
    }
}

#[test]
fn pointer_word_selection_never_splits_cjk_graphemes() {
    let mut editor = editor("日本語 次");
    editor.apply(EditCommand::PointerStart {
        position: TextPosition::new(0, 1),
        granularity: SelectionGranularity::Word,
        extend_selection: false,
    });
    assert_eq!(editor.selected_text().as_deref(), Some("本"));
}

#[test]
fn shifted_pointer_selection_extends_from_the_existing_anchor() {
    let mut editor = editor("first middle last");
    editor.apply(EditCommand::PointerStart {
        position: TextPosition::new(0, 7),
        granularity: SelectionGranularity::Word,
        extend_selection: false,
    });
    editor.apply(EditCommand::PointerEnd);
    editor.apply(EditCommand::PointerStart {
        position: TextPosition::new(0, 16),
        granularity: SelectionGranularity::Word,
        extend_selection: true,
    });
    assert_eq!(editor.selected_text().as_deref(), Some("middle last"));
}

#[test]
fn logical_line_delete_is_one_undoable_operation() {
    let mut editor = editor("one\r\ntwo\r\nthree");
    editor.apply(EditCommand::SetCursor {
        position: TextPosition::new(1, 1),
        extend_selection: false,
    });

    let deleted = editor.apply(EditCommand::DeleteLogicalLine);
    assert_eq!(deleted.snapshot.content, "one\r\nthree");
    assert_eq!(
        editor.apply(EditCommand::Undo).snapshot.content,
        "one\r\ntwo\r\nthree"
    );
}

#[test]
fn repeated_resize_reflows_without_mutating_logical_state() {
    let text = "Grüße 日本語 👩🏽‍💻 and a deliberately long logical line";
    let mut editor = editor(text);
    editor.apply(EditCommand::SetCursor {
        position: TextPosition::new(0, 3),
        extend_selection: false,
    });
    editor.apply(EditCommand::SetCursor {
        position: TextPosition::new(0, 20),
        extend_selection: true,
    });
    let cursor = editor.snapshot().cursor;
    let selection = editor.snapshot().selection;

    for viewport in [
        TextViewport::new(80, 20),
        TextViewport::new(8, 3),
        TextViewport::new(3, 2),
        TextViewport::new(30, 1),
        TextViewport::new(5, 8),
        TextViewport::new(120, 40),
    ] {
        editor.set_viewport(viewport);
        let snapshot = editor.snapshot();
        assert_eq!(snapshot.content, text);
        assert_eq!(snapshot.cursor, cursor);
        assert_eq!(snapshot.selection, selection);
        assert!(
            snapshot
                .visual_lines
                .iter()
                .all(|line| line.cell_width <= usize::from(viewport.width).max(2))
        );
    }
}

#[test]
fn replacement_clamps_cursor_and_resets_transient_history() {
    let mut editor = editor("first\nvery long second line");
    editor.apply(EditCommand::Paste(" changed".to_owned()));
    editor.replace_content("short\n日本語".to_owned(), TextPosition::new(9, 99));

    let snapshot = editor.snapshot();
    assert_eq!(snapshot.cursor, TextPosition::new(1, 3));
    assert_eq!(snapshot.content, "short\n日本語");
    assert!(!editor.apply(EditCommand::Undo).content_changed);
}

#[test]
fn crlf_and_whitespace_are_preserved_exactly() {
    let text = "  first\r\n\r\nsecond\t \r\n";
    let mut editor = editor("");
    assert_eq!(
        editor
            .apply(EditCommand::Paste(text.to_owned()))
            .snapshot
            .content,
        text
    );
}

#[test]
fn tabs_and_controls_have_safe_consistent_visual_geometry() {
    let mut editor = editor("a\tb\rc");
    editor.set_viewport(TextViewport::new(20, 2));
    let snapshot = editor.snapshot();
    assert_eq!(snapshot.content, "a\tb\rc");
    assert_eq!(snapshot.visual_lines[0].text, "a   b�c");
    assert_eq!(snapshot.visual_lines[0].cell_width, 7);
    assert_eq!(editor.position_at_cell(0, 2), TextPosition::new(0, 1));
    assert_eq!(editor.position_at_cell(0, 4), TextPosition::new(0, 2));
    assert_eq!(editor.position_at_cell(0, 5), TextPosition::new(0, 3));
}
