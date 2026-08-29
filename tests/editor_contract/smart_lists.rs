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
        let outcome = editor.apply(EditCommand::InsertSmartNewline { indent_width: 2 });
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
    let outcome = editor.apply(EditCommand::InsertSmartNewline { indent_width: 2 });
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
        let outcome = editor.apply(EditCommand::InsertSmartNewline { indent_width: 2 });
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
    let outcome = sole_item.apply(EditCommand::InsertSmartNewline { indent_width: 2 });
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
    let outcome = editor.apply(EditCommand::InsertSmartNewline { indent_width: 2 });
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
        let outcome = editor.apply(EditCommand::InsertSmartNewline { indent_width: 2 });
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
    let outcome = editor.apply(EditCommand::InsertSmartNewline { indent_width: 2 });
    assert_eq!(outcome.snapshot.content, format!("{before}\n  + "));
    assert_eq!(outcome.snapshot.cursor, TextPosition::new(4, 4));
}

#[test]
fn empty_nested_items_outdent_one_level_before_top_level_exit() {
    let before = "- parent\n  - ";
    let mut editor = at_end(before);
    let outcome = editor.apply(EditCommand::InsertSmartNewline { indent_width: 2 });
    assert_eq!(outcome.snapshot.content, "- parent\n- ");
    assert_eq!(outcome.snapshot.cursor, TextPosition::new(1, 2));
    assert_eq!(editor.apply(EditCommand::Undo).snapshot.content, before);
    assert_eq!(
        editor.apply(EditCommand::Redo).snapshot.content,
        "- parent\n- "
    );
    let exit = editor.apply(EditCommand::InsertSmartNewline { indent_width: 2 });
    assert_eq!(exit.snapshot.content, "- parent\n");
}

#[test]
fn tab_uses_configured_width_without_renumbering() {
    let before = "10. parent\r\n11. child\r\n12. later";
    let mut editor = at_end(before);
    editor.apply(EditCommand::SetCursor {
        position: TextPosition::new(1, 9),
        extend_selection: false,
    });
    let indented = editor.apply(EditCommand::Indent {
        width: 2,
        smart_lists: true,
    });
    assert_eq!(
        indented.snapshot.content,
        "10. parent\r\n  11. child\r\n12. later"
    );
    assert_eq!(indented.snapshot.cursor, TextPosition::new(1, 11));
    assert_eq!(indented.changes.len(), 1);

    let outdented = editor.apply(EditCommand::Outdent {
        width: 2,
        smart_lists: true,
    });
    assert_eq!(outdented.snapshot.content, before);
    assert_eq!(outdented.snapshot.cursor, TextPosition::new(1, 9));
    assert_eq!(
        editor.apply(EditCommand::Undo).snapshot.content,
        indented.snapshot.content
    );
}

#[test]
fn tab_preserves_existing_tabs_and_otherwise_uses_configured_spaces() {
    let mut tabbed = at_end("- root\n  1. parent\n\t - first\n\t - child");
    let outcome = tabbed.apply(EditCommand::Indent {
        width: 2,
        smart_lists: true,
    });
    assert_eq!(
        outcome.snapshot.content,
        "- root\n  1. parent\n\t - first\n\t \t- child"
    );

    let mut first = at_end("- first");
    let outcome = first.apply(EditCommand::Indent {
        width: 3,
        smart_lists: true,
    });
    assert_eq!(outcome.snapshot.content, "   - first");
}

#[test]
fn configured_levels_remain_reversibly_nested_beside_a_list() {
    let mut editor = at_end("- parent\n- child");
    for expected in ["- parent\n  - child", "- parent\n    - child"] {
        let outcome = editor.apply(EditCommand::Indent {
            width: 2,
            smart_lists: true,
        });
        assert_eq!(outcome.snapshot.content, expected);
    }
    for expected in ["- parent\n  - child", "- parent\n- child"] {
        let outcome = editor.apply(EditCommand::Outdent {
            width: 2,
            smart_lists: true,
        });
        assert_eq!(outcome.snapshot.content, expected);
    }

    let mut code = at_end("paragraph\n    - code");
    assert!(
        code.apply(EditCommand::Outdent {
            width: 2,
            smart_lists: true,
        })
        .changes
        .is_empty()
    );
    assert_eq!(code.snapshot().content, "paragraph\n    - code");
}

#[test]
fn marker_digit_width_never_changes_the_configured_indent_unit() {
    for before in [
        "- parent\n- child\n- later",
        "9. parent\n10. child\n11. later",
        "10. parent\n11. child\n12. later",
        "99. parent\n100. child\n101. later",
        "100. parent\n101. child\n102. later",
    ] {
        let child = before.lines().nth(1).expect("child line");
        let later = before.lines().nth(2).expect("later line");
        let mut editor = editor(before);
        editor.apply(EditCommand::SetCursor {
            position: TextPosition::new(1, child.chars().count()),
            extend_selection: false,
        });

        let once = editor.apply(EditCommand::Indent {
            width: 2,
            smart_lists: true,
        });
        assert_eq!(
            once.snapshot.content,
            format!(
                "{}\n  {child}\n{later}",
                before.lines().next().expect("parent line")
            )
        );
        assert_eq!(once.changes.len(), 1);

        let twice = editor.apply(EditCommand::Indent {
            width: 2,
            smart_lists: true,
        });
        assert_eq!(
            twice.snapshot.content,
            format!(
                "{}\n    {child}\n{later}",
                before.lines().next().expect("parent line")
            )
        );
        assert_eq!(twice.changes.len(), 1);

        for expected in [&once.snapshot.content, before] {
            let outdented = editor.apply(EditCommand::Outdent {
                width: 2,
                smart_lists: true,
            });
            assert_eq!(outdented.snapshot.content, expected);
            assert_eq!(outdented.changes.len(), 1);
        }
    }
}

#[test]
fn ordered_empty_item_outdents_one_fixed_level_per_enter() {
    let mut editor = at_end("100. parent\n    101. ");
    for expected in [
        "100. parent\n  101. ",
        "100. parent\n101. ",
        "100. parent\n",
    ] {
        let outcome = editor.apply(EditCommand::InsertSmartNewline { indent_width: 2 });
        assert_eq!(outcome.snapshot.content, expected);
        assert_eq!(outcome.changes.len(), 1);
    }
}

#[test]
fn selected_lines_indent_and_outdent_as_one_change_set_and_undo_step() {
    let before = "- one\n- Grüße 👩🏽‍💻\n- untouched";
    let mut editor = editor(before);
    editor.apply(EditCommand::SetCursor {
        position: TextPosition::new(0, 0),
        extend_selection: false,
    });
    editor.apply(EditCommand::SetCursor {
        position: TextPosition::new(2, 0),
        extend_selection: true,
    });
    let indented = editor.apply(EditCommand::Indent {
        width: 2,
        smart_lists: true,
    });
    assert_eq!(
        indented.snapshot.content,
        "  - one\n  - Grüße 👩🏽‍💻\n- untouched"
    );
    assert_eq!(indented.changes.len(), 2);
    assert_eq!(
        indented.snapshot.selection,
        Some(proqi::ports::editor::TextSelection {
            start: TextPosition::new(0, 2),
            end: TextPosition::new(2, 0),
        })
    );
    assert_eq!(editor.apply(EditCommand::Undo).snapshot.content, before);
    assert_eq!(
        editor.apply(EditCommand::Redo).snapshot.content,
        indented.snapshot.content
    );

    let outdented = editor.apply(EditCommand::Outdent {
        width: 2,
        smart_lists: true,
    });
    assert_eq!(outdented.snapshot.content, before);
    assert_eq!(outdented.changes.len(), 2);
}

#[test]
fn tab_is_exact_and_backtab_is_conservative_outside_list_context() {
    let mut editor = editor("alpha");
    editor.apply(EditCommand::SetCursor {
        position: TextPosition::new(0, 2),
        extend_selection: false,
    });
    let tab = editor.apply(EditCommand::Indent {
        width: 3,
        smart_lists: true,
    });
    assert_eq!(tab.snapshot.content, "al   pha");
    assert_eq!(tab.snapshot.cursor, TextPosition::new(0, 5));

    let backtab = editor.apply(EditCommand::Outdent {
        width: 3,
        smart_lists: true,
    });
    assert!(backtab.changes.is_empty());
    assert_eq!(backtab.snapshot.content, "al   pha");
}

#[test]
fn selected_list_continuations_and_ordinary_lines_are_all_indented_exactly() {
    let mut list = editor("- one\n  continuation\n- two\n- untouched");
    list.apply(EditCommand::SetCursor {
        position: TextPosition::new(0, 0),
        extend_selection: false,
    });
    list.apply(EditCommand::SetCursor {
        position: TextPosition::new(3, 0),
        extend_selection: true,
    });
    let indented = list.apply(EditCommand::Indent {
        width: 2,
        smart_lists: true,
    });
    assert_eq!(
        indented.snapshot.content,
        "  - one\n    continuation\n  - two\n- untouched"
    );
    assert_eq!(indented.changes.len(), 3);
    let outdented = list.apply(EditCommand::Outdent {
        width: 2,
        smart_lists: true,
    });
    assert_eq!(
        outdented.snapshot.content,
        "- one\n  continuation\n- two\n- untouched"
    );
    assert_eq!(outdented.changes.len(), 3);

    let mut ordinary = editor("one\ntwo\nuntouched");
    ordinary.apply(EditCommand::SetCursor {
        position: TextPosition::new(0, 0),
        extend_selection: false,
    });
    ordinary.apply(EditCommand::SetCursor {
        position: TextPosition::new(2, 0),
        extend_selection: true,
    });
    let indented = ordinary.apply(EditCommand::Indent {
        width: 3,
        smart_lists: true,
    });
    assert_eq!(indented.snapshot.content, "   one\n   two\nuntouched");
    assert_eq!(indented.changes.len(), 2);
    assert!(
        ordinary
            .apply(EditCommand::Outdent {
                width: 3,
                smart_lists: true,
            })
            .changes
            .is_empty()
    );
}

#[test]
fn smart_lists_false_keeps_tab_plain_and_backtab_exact() {
    let mut editor = at_end("- item");
    let tab = editor.apply(EditCommand::Indent {
        width: 2,
        smart_lists: false,
    });
    assert_eq!(tab.snapshot.content, "- item  ");
    let backtab = editor.apply(EditCommand::Outdent {
        width: 2,
        smart_lists: false,
    });
    assert!(backtab.changes.is_empty());
    assert_eq!(backtab.snapshot.content, "- item  ");
}
