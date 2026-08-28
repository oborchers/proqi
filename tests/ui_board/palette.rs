use super::*;
use proqi::domain::TextPosition;

#[test]
fn command_palette_is_searchable_and_mouse_operable() {
    let mut fixture = Fixture::new();
    fixture.paste("existing");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "quit".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let terminal = draw(&mut fixture, 40, 12);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains(":quit"));
    assert!(rendered.contains("Quit Proqi"));

    let quit = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 40, 12))
        .overlay
        .expect("command overlay")
        .items[0];
    fixture.pointer(quit.x, quit.y, PointerKind::Down(PointerButton::Left));
    assert!(fixture.app.quit);
}

#[test]
fn palette_quit_is_global_and_shallow_navigation_stays_visible() {
    let mut fixture = Fixture::new();
    fixture.paste("existing");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    let _terminal = draw(&mut fixture, 30, 5);
    for _ in 0..10 {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }));
    }
    let _terminal = draw(&mut fixture, 30, 5);
    let (_, visible, selected) = fixture.app.palette_view().expect("palette");
    assert!(selected < visible.len());

    fixture.input(UiInput::Key(UiKey::Quit));
    assert!(fixture.app.quit);
}

#[test]
fn palette_query_accepts_normalized_paste_and_grapheme_cursor_edits() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Character(':')));
    fixture.input(UiInput::Paste("qu\nit".to_owned()));
    let (query, _, _) = fixture.app.palette_view().expect("palette");
    assert_eq!(query, "qu it");

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Character('!')));
    let (query, _, _) = fixture.app.palette_view().expect("palette");
    assert_eq!(query, "qu i!t");
}

#[test]
fn palette_exposes_an_explicit_update_check() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "check for updates".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let (_, entries, selected) = fixture.app.palette_view().expect("palette");
    assert_eq!(entries, vec!["Check for updates"]);
    assert_eq!(selected, 0);
    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(
        effects,
        vec![Effect::Update(proqi::application::UpdateIntent::CheckNow)]
    );
}

#[test]
fn palette_fallbacks_execute_all_four_fast_editor_movements() {
    let content = (0..8)
        .map(|row| format!("row {row}"))
        .collect::<Vec<_>>()
        .join("\n");
    for (query, expected) in [
        ("jump cursor up", TextPosition::new(2, 5)),
        ("jump cursor down", TextPosition::new(5, 0)),
        ("thought beginning", TextPosition::new(0, 0)),
        ("thought end", TextPosition::new(7, 5)),
    ] {
        let mut fixture = Fixture::new();
        navigation::durable_thought(&mut fixture, &content);
        fixture.input(UiInput::Key(UiKey::Enter));
        fixture.input(UiInput::Key(UiKey::Move {
            movement: if matches!(query, "jump cursor down" | "thought end") {
                CursorMovement::DocumentStart
            } else {
                CursorMovement::DocumentEnd
            },
            extend_selection: false,
        }));
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
        for character in query.chars() {
            fixture.input(UiInput::Key(UiKey::Character(character)));
        }
        let (_, entries, selected) = fixture.app.palette_view().expect("palette");
        assert_eq!(entries.len(), 1, "query {query:?}: {entries:?}");
        assert_eq!(selected, 0);
        fixture.input(UiInput::Key(UiKey::Enter));
        assert_eq!(
            fixture.app.editor_snapshot().expect("editor").cursor,
            expected,
            "query {query:?}"
        );
    }
}

#[test]
fn palette_copies_typed_session_metadata_exactly_and_reports_results() {
    let mut fixture = Fixture::new();
    let session_id = fixture.app.state.board.session.id.to_string();
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "copy session id".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let copy_id = fixture.effects(UiInput::Key(UiKey::Enter));
    let [
        Effect::WriteClipboard {
            request_id,
            thought_id: None,
            intent: ClipboardIntent::CopySessionId,
            content,
        },
    ] = copy_id.as_slice()
    else {
        panic!("expected typed session ID clipboard effect");
    };
    assert_eq!(content, &session_id);
    fixture
        .app
        .complete_clipboard_write(*request_id, Ok(()), &mut fixture.ids, &fixture.clock);
    assert_eq!(fixture.app.status_text(), Some("copied session ID"));

    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "copy resume command".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let copy_resume = fixture.effects(UiInput::Key(UiKey::Enter));
    let [
        Effect::WriteClipboard {
            request_id,
            thought_id: None,
            intent: ClipboardIntent::CopyResumeCommand,
            content,
        },
    ] = copy_resume.as_slice()
    else {
        panic!("expected typed resume-command clipboard effect");
    };
    assert_eq!(content, &format!("proqi -r {session_id}"));
    let durable_before = fixture.app.state.clone();
    fixture.app.complete_clipboard_write(
        *request_id,
        Err(FailureCode::ClipboardFailed),
        &mut fixture.ids,
        &fixture.clock,
    );
    assert_eq!(fixture.app.state, durable_before);
    assert!(
        fixture
            .app
            .status_text()
            .is_some_and(|status| status.contains("clipboard unavailable"))
    );
}
