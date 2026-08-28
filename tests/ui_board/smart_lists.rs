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
