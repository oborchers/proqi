use std::fmt::Write as _;

use proqi::{
    adapters::editor::RopeEditor,
    domain::TextPosition,
    ports::editor::{CursorMovement, EditCommand, Editor, TextChange, TextViewport},
};
use unicode_segmentation::UnicodeSegmentation as _;

const DELETE_SENTENCE: EditCommand = EditCommand::DeleteSentence {
    list_indent_width: 2,
};

fn position_for_byte(text: &str, byte: usize) -> TextPosition {
    let prefix = &text[..byte];
    let line = prefix.bytes().filter(|value| *value == b'\n').count();
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    TextPosition::new(line, text[line_start..byte].graphemes(true).count())
}

fn delete_at(text: &str, byte: usize) -> proqi::ports::editor::EditOutcome {
    let mut editor = RopeEditor::new(text);
    editor.apply(EditCommand::SetCursor {
        position: position_for_byte(text, byte),
        extend_selection: false,
    });
    editor.apply(DELETE_SENTENCE)
}

fn selected_delete(text: &str, start: usize, end: usize) -> proqi::ports::editor::EditOutcome {
    let mut editor = RopeEditor::new(text);
    editor.apply(EditCommand::SetCursor {
        position: position_for_byte(text, start),
        extend_selection: false,
    });
    editor.apply(EditCommand::SetCursor {
        position: position_for_byte(text, end),
        extend_selection: true,
    });
    editor.apply(DELETE_SENTENCE)
}

#[test]
fn supplied_multiline_example_has_two_sentences() {
    let text = "The quick brown fox\njumped over the hoop. It failed to do anything.";
    let first = delete_at(text, text.find("quick").expect("first sentence"));
    assert_eq!(first.snapshot.content, "It failed to do anything.");
    assert_eq!(first.changes.as_slice()[0].old_range(), 0..42);

    let second = delete_at(text, text.find("failed").expect("second sentence"));
    assert_eq!(
        second.snapshot.content,
        "The quick brown fox\njumped over the hoop."
    );
    assert_eq!(second.snapshot.cursor, TextPosition::new(1, 21));
}

#[test]
fn empty_whitespace_and_unterminated_text_are_deterministic() {
    assert!(delete_at("", 0).changes.is_empty());
    assert!(delete_at(" \t\r\n ", 2).changes.is_empty());

    let text = "  No terminator 👩🏽‍💻  ";
    let deleted = delete_at(text, text.find('N').expect("content"));
    assert_eq!(deleted.snapshot.content, "");
    assert_eq!(deleted.changes.as_slice()[0].old_range(), 0..text.len());
}

#[test]
fn terminator_quote_whitespace_and_end_positions_have_stable_ownership() {
    let text = "  One?”  Two!  ";
    for byte in [
        text.find("One").expect("text"),
        text.find('?').expect("terminator"),
        text.find('”').expect("closing quote"),
        text.find("  Two").expect("separator"),
    ] {
        assert_eq!(delete_at(text, byte).snapshot.content, "Two!  ");
    }
    assert_eq!(delete_at(text, text.len()).snapshot.content, "  One?”");
    assert_eq!(
        delete_at(text, text.find("Two").expect("second sentence"))
            .snapshot
            .content,
        "  One?”"
    );
}

#[test]
fn selection_deletes_every_touched_sentence_after_ranges_merge() {
    let text = "One. Two? Three! Four.";
    let one = selected_delete(text, 1, 3);
    assert_eq!(one.snapshot.content, "Two? Three! Four.");
    assert_eq!(one.changes.len(), 1);

    let start = text.find("Two").expect("second");
    let end = text.find("Four").expect("fourth");
    let several = selected_delete(text, start, end);
    assert_eq!(several.snapshot.content, "One. Four.");
    assert_eq!(several.changes.len(), 1);
    assert_eq!(several.snapshot.selection, None);
}

#[test]
fn blank_paragraphs_are_preserved_as_hard_boundaries() {
    for text in [
        "First line\ncontinues here.\n\nSecond paragraph.",
        "First line\r\ncontinues here.\r\n\r\nSecond paragraph.",
        "First line\rcontinues here.\r\rSecond paragraph.",
        "First line\ncontinues here.\n \t\nSecond paragraph.",
    ] {
        let deleted = delete_at(text, text.find("continues").expect("first paragraph"));
        let boundary = &text[text.find("here.").expect("terminator") + "here.".len()
            ..text.find("Second").expect("second paragraph")];
        assert_eq!(
            deleted.snapshot.content,
            format!("{boundary}Second paragraph.")
        );
    }

    let text = "First.\n\nSecond.";
    let on_boundary = delete_at(text, text.find("\n\n").expect("boundary"));
    assert_eq!(on_boundary.snapshot.content, "\n\nSecond.");
    let after_boundary = delete_at(text, text.find("Second").expect("second"));
    assert_eq!(after_boundary.snapshot.content, "First.\n\n");
}

#[test]
fn selection_confined_to_a_blank_boundary_is_a_no_op() {
    let text = "First.\r\n\r\nSecond.";
    let boundary = text.find("\r\n\r\n").expect("blank boundary");
    let deleted = selected_delete(text, boundary, boundary + 4);
    assert_eq!(deleted.snapshot.content, text);
    assert!(deleted.changes.is_empty());
}

#[test]
fn selection_can_delete_sentences_across_paragraphs_without_deleting_the_boundary() {
    let text = "One. Two.\n\nThree. Four.";
    let deleted = selected_delete(
        text,
        text.find("Two").expect("second"),
        text.find("Four").expect("fourth"),
    );
    assert_eq!(deleted.snapshot.content, "One.\n\nFour.");
    assert_eq!(deleted.changes.len(), 2);
}

#[test]
fn unicode_terminators_combining_marks_and_emoji_remain_whole() {
    let cjk = "第一文。第二文！第三文？";
    assert_eq!(
        delete_at(cjk, cjk.find("第二").expect("second CJK sentence"))
            .snapshot
            .content,
        "第一文。第三文？"
    );

    let mixed = "Cafe\u{301} 👩🏽‍💻 works。 次です。 End!";
    assert_eq!(
        delete_at(mixed, mixed.find('👩').expect("emoji"))
            .snapshot
            .content,
        "次です。 End!"
    );

    let thai_without_terminator = "ภาษาไทยไม่มีเครื่องหมายจบ";
    assert_eq!(delete_at(thai_without_terminator, 0).snapshot.content, "");

    let combining_separator = "One. \u{301}Two.";
    assert_eq!(delete_at(combining_separator, 0).snapshot.content, "Two.");
}

#[test]
fn urls_decimals_versions_abbreviations_and_code_follow_uax29() {
    for (text, cursor, expected) in [
        ("Visit https://example.com/docs. Next.", "https", "Next."),
        ("Value 3.14 is pi. Next.", "3.14", "Next."),
        ("Version 1.2.3 works. Next.", "1.2.3", "Next."),
        ("Dr. Smith left. Next.", "Dr", "Smith left. Next."),
        ("Wait... Really? Next.", "Wait", "Really? Next."),
        ("Call foo.bar(); then continue. Next.", "foo", "Next."),
    ] {
        assert_eq!(
            delete_at(text, text.find(cursor).expect("cursor token"))
                .snapshot
                .content,
            expected,
            "{text:?}"
        );
    }
}

#[test]
fn list_prefixes_are_structural_and_cursor_owned_by_the_first_sentence() {
    let text = "- First. Second.";
    for byte in [0, text.find("First").expect("first sentence")] {
        assert_eq!(delete_at(text, byte).snapshot.content, "- Second.");
    }
    assert_eq!(
        delete_at(text, text.find("Second").expect("second sentence"))
            .snapshot
            .content,
        "- First."
    );

    let task = "  - [x] Only sentence.";
    assert_eq!(
        delete_at(task, task.find("Only").expect("task content"))
            .snapshot
            .content,
        "  - [x] "
    );

    let mut empty_item = RopeEditor::new("- Only.");
    assert_eq!(empty_item.apply(DELETE_SENTENCE).snapshot.content, "- ");
    assert!(empty_item.apply(DELETE_SENTENCE).changes.is_empty());
}

#[test]
fn list_items_are_structural_boundaries_without_renumbering() {
    let ordered = "7. First item\r\n8. Second item";
    assert_eq!(
        delete_at(ordered, ordered.find("First").expect("first item"))
            .snapshot
            .content,
        "7. \r\n8. Second item"
    );

    let unterminated = "- First item\n- Second item";
    assert_eq!(
        delete_at(
            unterminated,
            unterminated.find("First").expect("first item")
        )
        .snapshot
        .content,
        "- \n- Second item"
    );
}

#[test]
fn prose_to_list_newline_and_whitespace_prelude_have_exact_ownership() {
    let prose_then_list = "Notes. Buy later.\r\n- Milk. More.";
    assert_eq!(
        delete_at(
            prose_then_list,
            prose_then_list.find("Buy").expect("trailing prose")
        )
        .snapshot
        .content,
        "Notes.\r\n- Milk. More."
    );

    let whitespace_then_list = " \n- One.";
    let deleted = selected_delete(whitespace_then_list, 0, 1);
    assert_eq!(deleted.snapshot.content, "- ");
    assert_eq!(deleted.changes.len(), 2);
}

#[test]
fn sentence_less_list_item_is_a_conservative_no_op() {
    let text = "- \n- Real sentence.";
    for cursor in [0, 1, 2] {
        let deleted = delete_at(text, cursor);
        assert_eq!(deleted.snapshot.content, text);
        assert!(deleted.changes.is_empty());
    }
}

#[test]
fn list_continuations_remain_sentence_content_until_the_next_item() {
    let text = "- First line\n  continues. Second.\n    - Child one. Child two.\n- Next.";
    assert_eq!(
        delete_at(text, text.find("continues").expect("continuation"))
            .snapshot
            .content,
        "- Second.\n    - Child one. Child two.\n- Next."
    );
    assert_eq!(
        delete_at(text, text.find("Child one").expect("nested item"))
            .snapshot
            .content,
        "- First line\n  continues. Second.\n    - Child two.\n- Next."
    );
}

#[test]
fn selection_across_list_items_preserves_every_marker() {
    let text = "- One. Two.\n- Three. Four.";
    let deleted = selected_delete(
        text,
        text.find("Two").expect("second sentence"),
        text.find("Four").expect("fourth sentence"),
    );
    assert_eq!(deleted.snapshot.content, "- One.\n- Four.");
    assert_eq!(deleted.changes.len(), 2);
}

#[test]
fn preview_and_apply_share_the_exact_merged_range_owner() {
    let text = "- One. Two.\n- Three.";
    let mut editor = RopeEditor::new(text);
    editor.apply(EditCommand::SetCursor {
        position: position_for_byte(text, text.find("Two").expect("second sentence")),
        extend_selection: false,
    });
    let preview = editor.sentence_deletion_ranges(2);
    let outcome = editor.apply(DELETE_SENTENCE);
    assert_eq!(
        preview,
        outcome
            .changes
            .as_slice()
            .iter()
            .map(TextChange::old_range)
            .collect::<Vec<_>>()
    );
}

#[test]
fn repeated_deletion_and_history_are_single_operations() {
    let mut editor = RopeEditor::new("One. Two. Three.");
    editor.apply(EditCommand::Move {
        movement: CursorMovement::DocumentEnd,
        extend_selection: false,
    });
    for expected in ["One. Two.", "One.", ""] {
        assert_eq!(editor.apply(DELETE_SENTENCE).snapshot.content, expected);
    }
    assert!(editor.apply(DELETE_SENTENCE).changes.is_empty());
    assert_eq!(editor.apply(EditCommand::Undo).snapshot.content, "One.");
    assert_eq!(
        editor.apply(EditCommand::Undo).snapshot.content,
        "One. Two."
    );
    assert_eq!(editor.apply(EditCommand::Redo).snapshot.content, "One.");
}

#[test]
fn undo_restores_the_exact_pre_deletion_selection() {
    let text = "One. Two. Three.";
    let mut editor = RopeEditor::new(text);
    editor.apply(EditCommand::SetCursor {
        position: position_for_byte(text, text.find("Two").expect("selection start")),
        extend_selection: false,
    });
    editor.apply(EditCommand::SetCursor {
        position: position_for_byte(text, text.find("Three").expect("selection end")),
        extend_selection: true,
    });
    let before = editor.snapshot();
    editor.apply(DELETE_SENTENCE);
    let undone = editor.apply(EditCommand::Undo).snapshot;
    assert_eq!(undone.content, before.content);
    assert_eq!(undone.cursor, before.cursor);
    assert_eq!(undone.selection, before.selection);
}

#[test]
fn resize_does_not_change_the_sentence_target() {
    let text = "A deliberately wrapped first sentence. Second sentence.";
    for viewport in [
        TextViewport::new(4, 1),
        TextViewport::new(12, 2),
        TextViewport::new(120, 40),
    ] {
        let mut editor = RopeEditor::new(text);
        editor.set_viewport(viewport);
        editor.apply(EditCommand::SetCursor {
            position: position_for_byte(text, text.find("wrapped").expect("cursor")),
            extend_selection: false,
        });
        assert_eq!(
            editor.apply(DELETE_SENTENCE).snapshot.content,
            "Second sentence."
        );
    }
}

#[test]
fn large_unterminated_input_is_one_bounded_exact_deletion() {
    let text = "e\u{301}👩🏽‍💻界 ".repeat(100_000);
    let deleted = delete_at(&text, text.len() / 2);
    assert_eq!(deleted.snapshot.content, "");
    assert_eq!(deleted.changes.as_slice()[0].old_range(), 0..text.len());
}

#[test]
fn large_list_uses_bounded_canonical_structure_discovery() {
    let mut text = String::new();
    for index in 0..10_000 {
        writeln!(text, "- Item {index}. Keep.").expect("write list item");
    }
    text.pop();
    let cursor = text.rfind("Keep").expect("last sentence");
    let deleted = delete_at(&text, cursor);
    assert!(deleted.snapshot.content.ends_with("- Item 9999."));
    assert_eq!(deleted.changes.len(), 1);
}
