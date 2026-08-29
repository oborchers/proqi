use super::*;

fn revision(effects: &[Effect]) -> &proqi::domain::ThoughtRevision {
    let [Effect::CommitRevision(revision)] = effects else {
        panic!("expected one durable editor revision, got {effects:?}");
    };
    revision
}

#[test]
fn enter_continues_each_required_list_form_as_one_persistent_revision() {
    for (before, after) in [
        ("- first item", "- first item\n- "),
        ("9) item", "9) item\n10) "),
        ("- [x] done", "- [x] done\n- [ ] "),
        ("  *\tGrüße 👩🏽‍💻", "  *\tGrüße 👩🏽‍💻\n  *\t"),
    ] {
        let mut fixture = Fixture::new();
        fixture.paste(before);
        let effects = fixture.effects(UiInput::Key(UiKey::Enter));
        let revision = revision(&effects);
        assert_eq!(revision.before_content, before, "{before:?}");
        assert_eq!(revision.after_content, after, "{before:?}");
        assert_eq!(
            fixture.app.editor_snapshot().expect("editor").content,
            after,
            "{before:?}"
        );
        assert!(!fixture.app.has_pending_edit());
    }
}

#[test]
fn empty_item_exit_and_continuation_are_separate_restart_safe_undo_steps() {
    let mut fixture = Fixture::new();
    fixture.paste("- first");
    let continuation = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(revision(&continuation).after_content, "- first\n- ");

    let exit = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(revision(&exit).before_content, "- first\n- ");
    assert_eq!(revision(&exit).after_content, "- first\n");
    assert_eq!(
        revision(&exit).after_cursor,
        proqi::domain::TextPosition::new(1, 0)
    );

    assert_eq!(fixture.effects(UiInput::Key(UiKey::Undo)).len(), 1);
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "- first\n- "
    );
    assert_eq!(fixture.effects(UiInput::Key(UiKey::Undo)).len(), 1);
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "- first"
    );
}

#[test]
fn nested_empty_enter_outdents_then_exits_as_separate_persistent_revisions() {
    let mut fixture = Fixture::new();
    fixture.paste("- parent\n  - ");
    let outdent = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(revision(&outdent).after_content, "- parent\n- ");
    assert_eq!(
        revision(&outdent).after_cursor,
        proqi::domain::TextPosition::new(1, 2)
    );

    let exit = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(revision(&exit).after_content, "- parent\n");
    assert_eq!(fixture.effects(UiInput::Key(UiKey::Undo)).len(), 1);
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "- parent\n- "
    );
    assert_eq!(fixture.effects(UiInput::Key(UiKey::Undo)).len(), 1);
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "- parent\n  - "
    );
}

#[test]
fn tab_and_backtab_use_one_configured_unit_per_persistent_revision() {
    let settings = UiSettings {
        list_indent_width: 3,
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    fixture.paste("10. parent\r\n11. child\r\n12. later");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::VisualUp,
        extend_selection: false,
    }));
    let indent = fixture.effects(UiInput::Key(UiKey::Tab));
    assert_eq!(
        revision(&indent).after_content,
        "10. parent\r\n   11. child\r\n12. later"
    );
    assert!(!fixture.app.has_pending_edit());

    let outdent = fixture.effects(UiInput::Key(UiKey::BackTab));
    assert_eq!(
        revision(&outdent).after_content,
        "10. parent\r\n11. child\r\n12. later"
    );
    assert!(!fixture.app.has_pending_edit());
}

#[test]
fn maximum_width_selected_list_round_trips_as_two_persistent_revisions() {
    let before = "- parent\r\n9. child\r\n100. tail";
    let mut fixture = Fixture::with_settings(UiSettings {
        list_indent_width: 8,
        ..UiSettings::default()
    });
    fixture.paste(before);
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentEnd,
        extend_selection: true,
    }));

    let indent = fixture.effects(UiInput::Key(UiKey::Tab));
    assert_eq!(
        revision(&indent).after_content,
        "        - parent\r\n        9. child\r\n        100. tail"
    );
    assert!(!fixture.app.has_pending_edit());

    let outdent = fixture.effects(UiInput::Key(UiKey::BackTab));
    assert_eq!(revision(&outdent).after_content, before);
    assert!(!fixture.app.has_pending_edit());
}

#[test]
fn selected_line_indentation_excludes_a_column_zero_endpoint_and_preserves_annotations() {
    let path = "context.txt";
    let content = format!("- one\n- Grüße 👩🏽‍💻\n{path}");
    let annotation_start = content.find(path).expect("path");
    let annotation = ContentAnnotation {
        start: annotation_start,
        end: annotation_start + path.len(),
        kind: ContentAnnotationKind::Attachment {
            image: false,
            display_name: "context.txt".to_owned(),
        },
    };
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(PastePayload::annotated(
        content.clone(),
        vec![annotation.clone()],
    )));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    for _ in 0..2 {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: true,
        }));
    }
    let effects = fixture.effects(UiInput::Key(UiKey::Tab));
    let revision = revision(&effects);
    assert_eq!(
        revision.after_content,
        format!("  - one\n  - Grüße 👩🏽‍💻\n{path}")
    );
    let mut rebased = annotation;
    rebased.start += 4;
    rebased.end += 4;
    assert_eq!(revision.after_annotations, vec![rebased]);
    assert!(revision.after_content.ends_with(path));
    let cursor = fixture.app.editor_snapshot().expect("editor").cursor;
    for (width, height) in [(9, 4), (80, 8), (12, 12), (40, 3)] {
        let _terminal = draw(&mut fixture, width, height);
        assert_eq!(
            fixture.app.editor_snapshot().expect("editor").cursor,
            cursor
        );
    }
}

#[test]
fn ordinary_tab_is_exact_while_backtab_and_disabled_smart_lists_preserve_text() {
    let mut ordinary = Fixture::with_settings(UiSettings {
        list_indent_width: 3,
        ..UiSettings::default()
    });
    ordinary.paste("ordinary");
    let tab = ordinary.effects(UiInput::Key(UiKey::Tab));
    assert_eq!(revision(&tab).after_content, "ordinary   ");
    assert!(ordinary.effects(UiInput::Key(UiKey::BackTab)).is_empty());

    let mut disabled = Fixture::with_settings(UiSettings {
        smart_lists: false,
        ..UiSettings::default()
    });
    disabled.paste("- item");
    let tab = disabled.effects(UiInput::Key(UiKey::Tab));
    assert_eq!(revision(&tab).after_content, "- item  ");
    assert!(disabled.effects(UiInput::Key(UiKey::BackTab)).is_empty());
}

#[test]
fn disabled_setting_and_selection_replacement_keep_plain_newline_behavior() {
    let settings = UiSettings {
        smart_lists: false,
        ..UiSettings::default()
    };
    let mut disabled = Fixture::with_settings(settings);
    disabled.paste("- first");
    assert!(disabled.effects(UiInput::Key(UiKey::Enter)).is_empty());
    assert_eq!(
        disabled.app.editor_snapshot().expect("editor").content,
        "- first\n"
    );

    let mut selected = Fixture::new();
    selected.paste("- first");
    selected.input(UiInput::Key(UiKey::SelectAll));
    assert!(selected.effects(UiInput::Key(UiKey::Enter)).is_empty());
    assert_eq!(
        selected.app.editor_snapshot().expect("editor").content,
        "\n"
    );
}

#[test]
fn paste_is_exact_and_smart_newlines_preserve_annotations_through_resize() {
    let mut fixture = Fixture::new();
    fixture.paste("- first");
    fixture.input(UiInput::Paste("\n9) pasted".to_owned()));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "- first\n9) pasted"
    );

    let path = "/tmp/context.txt";
    let content = format!("{path}\n- item");
    let annotation = ContentAnnotation {
        start: 0,
        end: path.len(),
        kind: ContentAnnotationKind::Attachment {
            image: false,
            display_name: "context.txt".to_owned(),
        },
    };
    let mut annotated = Fixture::new();
    annotated.input(UiInput::PasteAnnotated(PastePayload::annotated(
        content,
        vec![annotation.clone()],
    )));
    let effects = annotated.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(revision(&effects).after_annotations, vec![annotation]);
    let cursor = annotated.app.editor_snapshot().expect("editor").cursor;
    for (width, height) in [(12, 4), (80, 8), (7, 12), (40, 3)] {
        let _terminal = draw(&mut annotated, width, height);
        assert_eq!(
            annotated.app.editor_snapshot().expect("editor").cursor,
            cursor
        );
    }
}

#[test]
fn command_palette_inserts_a_plain_newline_without_a_modifier_by_keyboard_and_mouse() {
    let mut keyboard = Fixture::new();
    keyboard.paste("- item");
    keyboard.input(UiInput::Key(UiKey::Escape));
    keyboard.input(UiInput::Key(UiKey::Character(':')));
    for character in "plain newline".chars() {
        keyboard.input(UiInput::Key(UiKey::Character(character)));
    }
    let effects = keyboard.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(revision(&effects).after_content, "- item\n");

    let mut mouse = Fixture::new();
    mouse.paste("9) item");
    let commands = mouse
        .app
        .prepare_frame(Rect::new(0, 0, 80, 8))
        .controls
        .into_iter()
        .find_map(|(target, area)| (target == HitTarget::Commands).then_some(area))
        .expect("commands control");
    mouse.pointer(
        commands.x,
        commands.y,
        PointerKind::Down(PointerButton::Left),
    );
    for character in "plain newline".chars() {
        mouse.input(UiInput::Key(UiKey::Character(character)));
    }
    let item = mouse
        .app
        .prepare_frame(Rect::new(0, 0, 80, 8))
        .overlay
        .expect("palette")
        .items[0];
    let effects = mouse.effects(UiInput::Pointer(PointerInput {
        column: item.x,
        row: item.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert_eq!(revision(&effects).after_content, "9) item\n");
}

#[test]
fn command_palette_indents_by_keyboard_and_outdents_by_mouse() {
    let mut fixture = Fixture::new();
    fixture.paste("- parent\n- child");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "indent line".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let indent = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(revision(&indent).after_content, "- parent\n  - child");

    let commands = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 80, 8))
        .controls
        .into_iter()
        .find_map(|(target, area)| (target == HitTarget::Commands).then_some(area))
        .expect("commands control");
    fixture.pointer(
        commands.x,
        commands.y,
        PointerKind::Down(PointerButton::Left),
    );
    for character in "outdent line".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let item = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 80, 8))
        .overlay
        .expect("palette")
        .items[0];
    let outdent = fixture.effects(UiInput::Pointer(PointerInput {
        column: item.x,
        row: item.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert_eq!(revision(&outdent).after_content, "- parent\n- child");
}

#[test]
fn keyboard_palette_restores_a_column_zero_multiline_selection_once() {
    let before = "- parent\n- child\n- untouched";
    let mut fixture = Fixture::new();
    fixture.paste(before);
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    for _ in 0..2 {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: true,
        }));
    }
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "indent line".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }

    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(
        revision(&effects).after_content,
        "  - parent\n  - child\n- untouched"
    );
    let snapshot = fixture.app.editor_snapshot().expect("editor");
    assert_eq!(
        snapshot.selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 2),
            end: proqi::domain::TextPosition::new(2, 0),
        })
    );
    assert_eq!(fixture.effects(UiInput::Key(UiKey::Undo)).len(), 1);
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        before
    );
}

#[test]
fn mouse_palette_restores_a_reverse_multiline_selection() {
    let mut fixture = Fixture::new();
    fixture.paste("  - parent\n  - child\n- untouched");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    for _ in 0..2 {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }));
    }
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: true,
    }));
    fixture.input(UiInput::Key(UiKey::Escape));

    let commands = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 80, 8))
        .controls
        .into_iter()
        .find_map(|(target, area)| (target == HitTarget::Commands).then_some(area))
        .expect("commands control");
    fixture.pointer(
        commands.x,
        commands.y,
        PointerKind::Down(PointerButton::Left),
    );
    for character in "outdent line".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let item = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 80, 8))
        .overlay
        .expect("palette")
        .items[0];
    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: item.x,
        row: item.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert_eq!(
        revision(&effects).after_content,
        "- parent\n- child\n- untouched"
    );
    let snapshot = fixture.app.editor_snapshot().expect("editor");
    assert_eq!(snapshot.cursor, proqi::domain::TextPosition::new(0, 0));
    assert_eq!(
        snapshot.selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 0),
            end: proqi::domain::TextPosition::new(2, 0),
        })
    );
}

#[test]
fn cancelling_the_palette_discards_the_selection_handoff() {
    let mut fixture = Fixture::new();
    fixture.paste("- parent\n- child\n- untouched");
    fixture.input(UiInput::Key(UiKey::SelectAll));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "indent line".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }

    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(
        revision(&effects).after_content,
        "- parent\n- child\n  - untouched"
    );
}

#[test]
fn board_navigation_discards_the_selection_handoff() {
    let mut fixture = Fixture::new();
    fixture.paste("- first");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.paste("- second");
    fixture.input(UiInput::Key(UiKey::SelectAll));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "indent line".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }

    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(revision(&effects).after_content, "  - first");
}
