use super::*;

fn image_payload(path: &str) -> PastePayload {
    attachment_payload(path, true)
}

fn attachment_payload(path: &str, image: bool) -> PastePayload {
    PastePayload::annotated(
        path.to_owned(),
        vec![ContentAnnotation {
            start: 0,
            end: path.len(),
            kind: ContentAnnotationKind::Attachment {
                image,
                display_name: "screenshot.png".to_owned(),
            },
        }],
    )
}

#[test]
fn image_path_folds_immediately_but_every_exact_content_path_is_preserved() {
    let mut fixture = Fixture::new();
    let path = "/private/temporary/location/screenshot.png";
    fixture.input(UiInput::PasteAnnotated(image_payload(path)));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, path);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].annotations.len(),
        1
    );

    let rendered = text(draw(&mut fixture, 60, 8).backend().buffer());
    assert!(rendered.contains("[Image 1]"));
    assert!(!rendered.contains("screenshot.png"));
    assert!(!rendered.contains("/private/temporary"));

    fixture.input(UiInput::Key(UiKey::Enter));
    assert!(
        text(draw(&mut fixture, 60, 8).backend().buffer())
            .contains("/private/temporary/location/screenshot.png")
    );
    fixture.input(UiInput::Key(UiKey::Escape));

    let effects = fixture.effects(UiInput::Key(UiKey::Copy));
    assert!(matches!(
        effects.as_slice(),
        [Effect::WriteClipboard { content, .. }] if content == path
    ));
    fixture.input(UiInput::Key(UiKey::Enter));
    assert_eq!(fixture.app.editor_snapshot().expect("editor").content, path);
    assert!(text(draw(&mut fixture, 60, 8).backend().buffer()).contains("[Image 1]"));
}

#[test]
fn files_use_the_minimal_accent_placeholder_without_exposing_the_path() {
    let mut fixture = Fixture::new();
    let path = "/private/temporary/location/context.pdf";
    fixture.input(UiInput::PasteAnnotated(attachment_payload(path, false)));
    let terminal = draw_theme(&mut fixture, 60, 8, ThemePreference::Dark);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("[File 1]"));
    assert!(!rendered.contains("context.pdf"));
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 60, 8)).thoughts[0].text_area;
    let cell = &terminal.backend().buffer()[(area.x, area.y)];
    assert_eq!(cell.fg, Theme::resolve(ThemePreference::Dark, true).accent);
}

#[test]
fn large_paste_is_folded_while_editing_and_editor_undo_restores_its_fold() {
    let mut fixture = Fixture::new();
    let content = (0..14)
        .map(|line| format!("context line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fixture.input(UiInput::Paste(content.clone()));
    let rendered = text(draw(&mut fixture, 60, 8).backend().buffer());
    assert!(rendered.contains("[Pasted text · 14 lines · 213 characters]"));
    assert!(!rendered.contains("context line 13"));

    fixture.input(UiInput::Key(UiKey::Enter));
    let expanded = text(draw(&mut fixture, 60, 8).backend().buffer());
    assert!(expanded.contains("context line 13"));
    assert!(!expanded.contains("[Pasted text"));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Character('!')));
    let effects = fixture.effects(UiInput::Key(UiKey::Undo));
    assert_eq!(effects.len(), 2);
    fixture.input(UiInput::Key(UiKey::Escape));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, content);
    assert!(
        text(draw(&mut fixture, 60, 8).backend().buffer()).contains("[Pasted text · 14 lines ·")
    );
}

#[test]
fn collapsed_folds_are_atomic_for_cursor_deletion_and_mouse_expansion() {
    let mut fixture = Fixture::new();
    let path = "/tmp/screenshot.png";
    fixture.input(UiInput::PasteAnnotated(image_payload(path)));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        proqi::domain::TextPosition::new(0, 0)
    );
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: false,
    }));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        proqi::domain::TextPosition::new(0, path.len())
    );

    let _rendered = draw(&mut fixture, 40, 8);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 40, 8)).thoughts[0].text_area;
    fixture.pointer(
        area.x.saturating_add(2),
        area.y,
        PointerKind::Down(PointerButton::Left),
    );
    assert!(text(draw(&mut fixture, 40, 8).backend().buffer()).contains(path));

    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Backspace));
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("editor")
            .content
            .is_empty()
    );
}

#[test]
fn folded_editor_keeps_a_visible_terminal_cursor_at_the_token_boundary() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(image_payload(
        "/tmp/screenshot.png",
    )));
    let mut terminal = draw(&mut fixture, 40, 8);
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("visible cursor");
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 40, 8)).thoughts[0].text_area;
    assert_eq!((cursor.x, cursor.y), (area.x + 9, area.y));
}

#[test]
fn folded_tokens_use_the_accent_and_bold_non_color_cue() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(image_payload(
        "/tmp/screenshot.png",
    )));
    let terminal = draw_theme(&mut fixture, 40, 8, ThemePreference::Dark);
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 40, 8));
    let text = layout.thoughts[0].text_area;
    let cell = &terminal.backend().buffer()[(text.x, text.y)];
    let theme = Theme::resolve(ThemePreference::Dark, true);
    assert_eq!(cell.fg, theme.accent);
    assert!(cell.modifier.contains(ratatui_core::style::Modifier::BOLD));
}
