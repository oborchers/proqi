use proqi::{
    domain::TextPosition,
    ports::editor::{CursorMovement, EditCommand, Editor},
};

use super::{assert_change, editor};

fn at_end(text: &str) -> impl Editor {
    let mut editor = editor(text);
    editor.apply(EditCommand::Move {
        movement: CursorMovement::DocumentEnd,
        extend_selection: false,
    });
    editor
}

#[test]
fn bullet_ordered_and_task_items_continue_with_exact_markup() {
    for (before, after) in [
        ("- first item", "- first item\n- "),
        (" *  indented", " *  indented\n *  "),
        ("\t+ tabbed", "\t+ tabbed\n"),
        ("9) item", "9) item\n10) "),
        ("000000009. item", "000000009. item\n10. "),
        ("- [x] done", "- [x] done\n- [ ] "),
        ("+ [X]\tchecked", "+ [X]\tchecked\n+ [ ]\t"),
        ("2. [ ]  open", "2. [ ]  open\n3. [ ]  "),
    ] {
        let mut editor = at_end(before);
        let outcome = editor.apply(EditCommand::InsertSmartNewline);
        assert_eq!(outcome.snapshot.content, after, "{before:?}");
        assert_eq!(outcome.snapshot.selection, None);
    }
}

#[test]
fn smart_continuation_preserves_crlf_and_changes_only_the_inserted_marker() {
    let mut editor = at_end("1) first\r\n9) item\r\n10) existing");
    editor.apply(EditCommand::SetCursor {
        position: TextPosition::new(1, 7),
        extend_selection: false,
    });
    let outcome = editor.apply(EditCommand::InsertSmartNewline);
    let inserted = "\r\n10) ";
    let cursor = "1) first\r\n9) item".len();
    assert_change(&outcome, cursor..cursor, cursor..cursor + inserted.len());
    assert_eq!(
        outcome.snapshot.content,
        "1) first\r\n9) item\r\n10) \r\n10) existing"
    );
    assert_eq!(outcome.snapshot.cursor, TextPosition::new(2, 4));
}

#[test]
fn empty_top_level_items_exit_as_one_undoable_transaction() {
    for before in [
        "- first\n- ",
        "9) item\n10) ",
        "- [x] done\n- [ ] ",
        "- first\r\n- ",
    ] {
        let mut editor = at_end(before);
        let line_start = before.rfind('\n').map_or(0, |index| index + 1);
        let outcome = editor.apply(EditCommand::InsertSmartNewline);
        assert_change(&outcome, line_start..before.len(), line_start..line_start);
        assert_eq!(outcome.snapshot.content, &before[..line_start]);
        assert_eq!(outcome.snapshot.cursor, TextPosition::new(1, 0));
        assert_eq!(
            editor.apply(EditCommand::Undo).snapshot.content,
            before,
            "{before:?}"
        );
        assert_eq!(
            editor.apply(EditCommand::Redo).snapshot.content,
            &before[..line_start]
        );
    }

    let mut sole_item = at_end("- ");
    let outcome = sole_item.apply(EditCommand::InsertSmartNewline);
    assert_eq!(outcome.snapshot.content, "");
    assert_eq!(outcome.snapshot.cursor, TextPosition::default());
}

#[test]
fn selection_replacement_is_an_exact_plain_newline() {
    let mut editor = at_end("- first item");
    editor.apply(EditCommand::Move {
        movement: CursorMovement::WordBack,
        extend_selection: true,
    });
    let before = editor.snapshot();
    let _selection = before.selection.expect("selection");
    let outcome = editor.apply(EditCommand::InsertSmartNewline);
    assert_eq!(outcome.snapshot.content, "- first \n");
    assert_eq!(outcome.snapshot.cursor, TextPosition::new(1, 0));
}

#[test]
fn paste_never_invokes_list_continuation() {
    let mut editor = at_end("- first item");
    let outcome = editor.apply(EditCommand::Paste("\n- pasted".to_owned()));
    assert_eq!(outcome.snapshot.content, "- first item\n- pasted");
}

#[test]
fn ambiguous_markdown_and_code_contexts_fall_back_to_plain_newline() {
    for before in [
        ".",
        "\\- escaped",
        "---",
        "- - -",
        "* * *",
        "    - indented code",
        "\t- tab-indented code",
        "1234567890. too many digits",
        "1.no spacing",
        "```\n- fenced",
        "~~~rust\n9) fenced",
    ] {
        let mut editor = at_end(before);
        let outcome = editor.apply(EditCommand::InsertSmartNewline);
        assert_eq!(
            outcome.snapshot.content,
            format!("{before}\n"),
            "{before:?}"
        );
    }
}

#[test]
fn closed_fences_and_unicode_list_content_continue_normally() {
    let before = "```\n- code\n```\n  + Grüße 👩🏽‍💻 第二行";
    let mut editor = at_end(before);
    let outcome = editor.apply(EditCommand::InsertSmartNewline);
    assert_eq!(outcome.snapshot.content, format!("{before}\n  + "));
    assert_eq!(outcome.snapshot.cursor, TextPosition::new(4, 4));
}

#[test]
fn indented_empty_items_do_not_implement_nested_list_exit() {
    let before = "- parent\n  - ";
    let mut editor = at_end(before);
    let outcome = editor.apply(EditCommand::InsertSmartNewline);
    assert_eq!(outcome.snapshot.content, format!("{before}\n  - "));
}
