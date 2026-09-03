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
    fixture.input(UiInput::Key(UiKey::Escape));
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
fn palette_query_keeps_vim_letters_literal_and_delete_edits_the_query() {
    for delete in [UiKey::Delete, UiKey::ModifiedDelete] {
        let mut fixture = Fixture::new();
        fixture.input(UiInput::Key(UiKey::Escape));
        fixture.input(UiInput::Key(UiKey::Character(':')));
        for character in "hjklx".chars() {
            fixture.input(UiInput::Key(UiKey::Character(character)));
        }
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeBack,
            extend_selection: false,
        }));
        fixture.input(UiInput::Key(delete));

        let (query, _, _) = fixture.app.palette_view().expect("palette");
        assert_eq!(query, "hjkl");
    }
}

#[test]
fn palette_exposes_an_explicit_update_check() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Escape));
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
fn repeated_palette_click_does_not_pass_through_to_a_wrapped_thought() {
    let (mut fixture, item) = activate_jump_down_palette();
    let repeated = thought_cell_within_activation(&mut fixture, item, PALETTE_VIEWPORT);
    let after_command = fixture.app.editor_snapshot().expect("editor").clone();
    for _ in 0..3 {
        fixture.pointer(
            repeated.0,
            repeated.1,
            PointerKind::Down(PointerButton::Left),
        );
        let after_repeat = fixture.app.editor_snapshot().expect("editor");
        assert_eq!(after_repeat.cursor, after_command.cursor);
        assert_eq!(after_repeat.selection, after_command.selection);
        fixture.pointer(repeated.0, repeated.1, PointerKind::Up(PointerButton::Left));
    }
}

#[test]
fn passive_move_near_palette_activation_preserves_click_through_guard() {
    let (mut fixture, item) = activate_jump_down_palette();
    let repeated = thought_cell_within_activation(&mut fixture, item, PALETTE_VIEWPORT);
    let after_command = fixture.app.editor_snapshot().expect("editor").clone();

    fixture.pointer(repeated.0, repeated.1, PointerKind::Move);
    fixture.pointer(
        repeated.0,
        repeated.1,
        PointerKind::Down(PointerButton::Left),
    );

    let after_repeat = fixture.app.editor_snapshot().expect("editor");
    assert_eq!(after_repeat.cursor, after_command.cursor);
    assert_eq!(after_repeat.selection, after_command.selection);
}

#[test]
fn passive_move_outside_palette_activation_allows_intentional_click() {
    let (mut fixture, item) = activate_jump_down_palette();
    let outside = thought_cell_outside_activation(&mut fixture, item, PALETTE_VIEWPORT);
    let repeated = thought_cell_within_activation(&mut fixture, item, PALETTE_VIEWPORT);
    let after_command = fixture.app.editor_snapshot().expect("editor").cursor;

    fixture.pointer(outside.0, outside.1, PointerKind::Move);
    fixture.pointer(
        repeated.0,
        repeated.1,
        PointerKind::Down(PointerButton::Left),
    );

    assert_ne!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        after_command
    );
}

#[test]
fn resize_invalidates_palette_activation_coordinates() {
    let (mut fixture, item) = activate_jump_down_palette();
    fixture.input(UiInput::Resize {
        width: PALETTE_VIEWPORT.width,
        height: PALETTE_VIEWPORT.height,
    });
    let _board = draw(
        &mut fixture,
        PALETTE_VIEWPORT.width,
        PALETTE_VIEWPORT.height,
    );
    let target = thought_cell_within_activation(&mut fixture, item, PALETTE_VIEWPORT);
    let before_click = fixture.app.editor_snapshot().expect("editor").cursor;

    fixture.pointer(target.0, target.1, PointerKind::Down(PointerButton::Left));

    assert_ne!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        before_click
    );
}

#[test]
fn keyboard_input_invalidates_palette_activation_coordinates() {
    let (mut fixture, item) = activate_jump_down_palette();
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    let target = thought_cell_within_activation(&mut fixture, item, PALETTE_VIEWPORT);
    let before_click = fixture.app.editor_snapshot().expect("editor").cursor;

    fixture.pointer(target.0, target.1, PointerKind::Down(PointerButton::Left));

    assert_ne!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        before_click
    );
}

#[test]
fn rendered_geometry_change_invalidates_palette_activation_coordinates() {
    let (mut fixture, item) = activate_jump_down_palette();
    let resized = Rect::new(0, 0, 97, PALETTE_VIEWPORT.height);
    let _board = draw(&mut fixture, resized.width, resized.height);
    let target = thought_cell_within_activation(&mut fixture, item, resized);
    let before_click = fixture.app.editor_snapshot().expect("editor").cursor;

    fixture.pointer(target.0, target.1, PointerKind::Down(PointerButton::Left));

    assert_ne!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        before_click
    );
}

#[test]
fn expired_palette_activation_allows_a_later_intentional_click() {
    let (mut fixture, item) = activate_jump_down_palette();
    fixture.clock.set(Timestamp::from_millis(521));
    let repeated = thought_cell_within_activation(&mut fixture, item, PALETTE_VIEWPORT);
    let before_click = fixture.app.editor_snapshot().expect("editor").cursor;

    fixture.pointer(repeated.0, repeated.1, PointerKind::Move);
    fixture.pointer(
        repeated.0,
        repeated.1,
        PointerKind::Down(PointerButton::Left),
    );

    assert_ne!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        before_click
    );
}

const PALETTE_VIEWPORT: Rect = Rect::new(0, 0, 98, 6);

fn activate_jump_down_palette() -> (Fixture, Rect) {
    let content = (0..24)
        .map(|row| format!("segment {row}: 0123456789 alpha beta gamma delta epsilon zeta eta"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut fixture = Fixture::new();
    navigation::durable_thought(&mut fixture, &content);
    navigation::durable_thought(&mut fixture, "short thought below");
    fixture.input(navigation::visual(CursorMovement::VisualUp, false));
    fixture.input(UiInput::Key(UiKey::Enter));
    move_editor_cursor(&mut fixture, CursorMovement::DocumentStart);
    move_editor_cursor(&mut fixture, CursorMovement::VisualJumpDown);
    fixture.input(UiInput::Key(UiKey::Escape));
    let _board = draw(
        &mut fixture,
        PALETTE_VIEWPORT.width,
        PALETTE_VIEWPORT.height,
    );
    let commands = fixture
        .app
        .prepare_frame(PALETTE_VIEWPORT)
        .controls
        .into_iter()
        .find_map(|(target, area)| (target == HitTarget::Commands).then_some(area))
        .expect("commands control");
    fixture.pointer(
        commands.x,
        commands.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.pointer(commands.x, commands.y, PointerKind::Up(PointerButton::Left));
    for character in "jump cursor down".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let _palette = draw(
        &mut fixture,
        PALETTE_VIEWPORT.width,
        PALETTE_VIEWPORT.height,
    );
    let item = fixture
        .app
        .prepare_frame(PALETTE_VIEWPORT)
        .overlay
        .as_ref()
        .expect("command overlay")
        .items[0];
    let activation = Rect::new(item.x, item.y, 1, 1);

    fixture.pointer(
        activation.x,
        activation.y,
        PointerKind::Down(PointerButton::Left),
    );
    assert!(matches!(
        fixture.app.state.mode,
        proqi::application::InteractionMode::Edit { .. }
    ));
    fixture.pointer(
        activation.x,
        activation.y,
        PointerKind::Up(PointerButton::Left),
    );
    let _underlying = draw(
        &mut fixture,
        PALETTE_VIEWPORT.width,
        PALETTE_VIEWPORT.height,
    );
    (fixture, activation)
}

fn thought_cell_within_activation(
    fixture: &mut Fixture,
    activation: Rect,
    viewport: Rect,
) -> (u16, u16) {
    let layout = fixture.app.prepare_frame(viewport);
    for row in activation.y.saturating_sub(1)..=activation.y.saturating_add(1) {
        for column in activation.x.saturating_sub(1)..=activation.x.saturating_add(1) {
            if matches!(layout.hit_test(column, row), Some(HitTarget::Thought(_))) {
                return (column, row);
            }
        }
    }
    panic!("visible thought cell within overlay activation tolerance");
}

fn thought_cell_outside_activation(
    fixture: &mut Fixture,
    activation: Rect,
    viewport: Rect,
) -> (u16, u16) {
    let layout = fixture.app.prepare_frame(viewport);
    for row in layout.board.y..layout.board.bottom() {
        for column in layout.board.x..layout.board.right() {
            if activation.x.abs_diff(column) > 1
                && activation.y.abs_diff(row) > 1
                && matches!(layout.hit_test(column, row), Some(HitTarget::Thought(_)))
            {
                return (column, row);
            }
        }
    }
    panic!("visible thought cell outside overlay activation tolerance");
}

fn move_editor_cursor(fixture: &mut Fixture, movement: CursorMovement) {
    fixture.input(UiInput::Key(UiKey::Move {
        movement,
        extend_selection: false,
    }));
}

#[test]
fn palette_copies_typed_session_metadata_exactly_and_reports_results() {
    let mut fixture = Fixture::new();
    let session_id = fixture.app.state.board.session.id.to_string();
    fixture.input(UiInput::Key(UiKey::Escape));
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
            ..
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
            ..
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
