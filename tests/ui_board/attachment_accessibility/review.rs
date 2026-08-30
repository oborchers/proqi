use super::*;

#[test]
fn startup_manual_and_quiet_refreshes_never_render_unverified_as_accessible() {
    let path = "/private/TemporaryItems/missing.png";
    let mut source = Fixture::new();
    source.input(UiInput::PasteAnnotated(attachment_payload(path, true)));
    let board = source.app.state.board.clone();
    let mut fixture = Fixture {
        app: BoardApp::with_settings(
            AppState::new(board),
            UiSettings::default(),
            proqi::adapters::editor::RopeEditorFactory,
        ),
        ids: source.ids,
        clock: source.clock,
    };
    let startup = fixture
        .app
        .start_attachment_checks(std::time::Duration::ZERO);
    let startup = attachment_batch(&startup);

    let startup_render = text(draw(&mut fixture, 60, 8).backend().buffer());
    assert!(startup_render.contains("[Image 1 · inaccessible]"));
    fixture
        .app
        .complete_attachment_checks(complete(startup, Ok(())));
    assert!(text(draw(&mut fixture, 60, 8).backend().buffer()).contains("[Image 1]"));

    let manual = fixture.app.refresh_attachments(true);
    let manual = attachment_batch(&manual);
    assert_eq!(fixture.app.status_text(), Some("refreshing attachments"));
    assert!(
        text(draw(&mut fixture, 60, 8).backend().buffer()).contains("[Image 1 · inaccessible]")
    );
    fixture
        .app
        .complete_attachment_checks(complete(manual, Ok(())));
    assert_eq!(
        fixture.app.status_text(),
        Some("all attachments are accessible")
    );
    assert!(text(draw(&mut fixture, 60, 8).backend().buffer()).contains("[Image 1]"));

    let quiet = fixture.app.refresh_attachments(false);
    let quiet = attachment_batch(&quiet);
    assert_eq!(
        fixture.app.status_text(),
        Some("all attachments are accessible"),
        "host-focus refresh stays quiet"
    );
    assert!(
        text(draw(&mut fixture, 60, 8).backend().buffer()).contains("[Image 1 · inaccessible]")
    );
    fixture
        .app
        .complete_attachment_checks(complete(quiet, Ok(())));
    assert!(text(draw(&mut fixture, 60, 8).backend().buffer()).contains("[Image 1]"));
}

#[test]
fn manual_refresh_reports_failure_empty_board_and_only_the_latest_cycle() {
    let mut fixture = Fixture::new();
    let insertion = fixture.effects(UiInput::PasteAnnotated(attachment_payload(
        "/tmp/context.txt",
        false,
    )));
    let insertion = attachment_batch(&insertion);
    fixture
        .app
        .complete_attachment_checks(complete(insertion, Ok(())));

    let first = fixture.app.refresh_attachments(true);
    let first = attachment_batch(&first);
    assert!(fixture.app.refresh_attachments(true).is_empty());
    let second = fixture
        .app
        .complete_attachment_checks(complete(first, Ok(())));
    assert_eq!(fixture.app.status_text(), Some("refreshing attachments"));
    let second = attachment_batch(&second);
    fixture.app.complete_attachment_checks(complete(
        second,
        Err(AttachmentAccessFailure::PermissionDenied),
    ));
    assert_eq!(
        fixture.app.status_text(),
        Some("Proqi cannot access 1 attachment")
    );
    assert!(text(draw(&mut fixture, 60, 8).backend().buffer()).contains("[File 1 · inaccessible]"));

    let mut empty = Fixture::new();
    assert!(empty.app.refresh_attachments(true).is_empty());
    assert_eq!(empty.app.status_text(), Some("no attachments to refresh"));
}

#[test]
fn expanded_inaccessible_attachment_keeps_text_warning_and_exact_mappings() {
    let path = "/tmp/missing.png";
    let mut fixture = Fixture::new();
    let insertion = fixture.effects(UiInput::PasteAnnotated(attachment_payload(path, true)));
    let insertion = attachment_batch(&insertion);
    fixture
        .app
        .complete_attachment_checks(complete(insertion, Err(AttachmentAccessFailure::Missing)));

    fixture.input(UiInput::Key(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Enter));

    for width in [60, 24] {
        let terminal = draw_theme(&mut fixture, width, 8, ThemePreference::Dark);
        let rendered = text(terminal.backend().buffer());
        assert!(
            rendered.contains(path) && rendered.contains("[inaccessible]"),
            "expanded rendering:\n{rendered}"
        );
        assert!(!rendered.contains("[Image 1"));
        let area = fixture
            .app
            .prepare_frame(Rect::new(0, 0, width, 8))
            .thoughts[0]
            .text_area;
        assert_warning_style(&terminal.backend().buffer()[(area.x, area.y)]);
    }

    let terminal = draw_theme(&mut fixture, 60, 8, ThemePreference::Dark);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 60, 8)).thoughts[0].text_area;
    let suffix = &terminal.backend().buffer()[(
        area.x + u16::try_from(path.len() + 1).expect("short fixture"),
        area.y,
    )];
    assert_warning_style(suffix);

    fixture.pointer(
        area.x + u16::try_from(path.len() + 4).expect("short fixture"),
        area.y,
        PointerKind::Down(PointerButton::Left),
    );
    let snapshot = fixture.app.editor_snapshot().expect("expanded editor");
    assert_eq!(
        snapshot.cursor,
        proqi::domain::TextPosition::new(0, path.len())
    );
    assert!(snapshot.selection.is_none());
    assert_eq!(snapshot.content, path);

    fixture.input(UiInput::Key(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::DocumentEnd,
        extend_selection: true,
    }));
    let selected = fixture.app.editor_snapshot().expect("selected path");
    assert_eq!(
        selected.selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 0),
            end: proqi::domain::TextPosition::new(0, path.len()),
        })
    );
    let terminal = draw_theme(&mut fixture, 60, 8, ThemePreference::Dark);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 60, 8)).thoughts[0].text_area;
    assert!(
        terminal.backend().buffer()[(area.x, area.y)]
            .modifier
            .contains(Modifier::REVERSED)
    );
    assert!(
        !terminal.backend().buffer()[(
            area.x + u16::try_from(path.len() + 1).expect("short fixture"),
            area.y,
        )]
            .modifier
            .contains(Modifier::REVERSED),
        "display-only warning suffix is not part of canonical selection"
    );
}

fn assert_warning_style(cell: &ratatui_core::buffer::Cell) {
    assert_eq!(cell.fg, Theme::resolve(ThemePreference::Dark, true).warning);
    assert!(cell.modifier.contains(Modifier::BOLD));
}
