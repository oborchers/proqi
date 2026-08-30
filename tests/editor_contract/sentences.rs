use proqi::{
    adapters::editor::RopeEditor,
    domain::TextPosition,
    ports::editor::{CursorMovement, EditCommand, Editor, TextViewport},
};
use unicode_segmentation::UnicodeSegmentation as _;

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
    editor.apply(EditCommand::DeleteSentence)
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
    editor.apply(EditCommand::DeleteSentence)
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
fn repeated_deletion_and_history_are_single_operations() {
    let mut editor = RopeEditor::new("One. Two. Three.");
    editor.apply(EditCommand::Move {
        movement: CursorMovement::DocumentEnd,
        extend_selection: false,
    });
    for expected in ["One. Two.", "One.", ""] {
        assert_eq!(
            editor.apply(EditCommand::DeleteSentence).snapshot.content,
            expected
        );
    }
    assert!(editor.apply(EditCommand::DeleteSentence).changes.is_empty());
    assert_eq!(editor.apply(EditCommand::Undo).snapshot.content, "One.");
    assert_eq!(
        editor.apply(EditCommand::Undo).snapshot.content,
        "One. Two."
    );
    assert_eq!(editor.apply(EditCommand::Redo).snapshot.content, "One.");
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
            editor.apply(EditCommand::DeleteSentence).snapshot.content,
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
