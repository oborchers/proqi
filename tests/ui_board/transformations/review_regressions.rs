use super::*;

#[test]
fn primary_transform_keeps_the_escape_handoff_for_commands_with_real_strokes() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("left right");
    fixture.app.acknowledge_persistence(sequence, true);
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Home)));
    for _ in 0..4 {
        fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Right)));
    }
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Escape)));
    let primary = if cfg!(target_os = "macos") {
        LogicalModifiers::SUPER
    } else {
        LogicalModifiers::CONTROL
    };
    fixture.input(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Character('t')).with_modifiers(primary),
    ));
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Character(
        ':',
    ))));
    for character in "split thought".chars() {
        fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Character(
            character,
        ))));
    }

    let effects = fixture.effects(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Enter)));
    assert_eq!(board_operation(&effects).kind, BoardOperationKind::Split);
    assert_eq!(
        fixture
            .app
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>(),
        ["left", " right"]
    );
}

#[test]
fn dirty_primary_transform_emits_one_board_operation_and_undoes_the_edit_with_it() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("A界 B");
    fixture.app.acknowledge_persistence(sequence, true);
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Character('!')));

    let effects = fixture.effects(crate::key_input(UiKey::PrimaryCharacter('t')));
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(
        fixture
            .app
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>(),
        ["A!", "界 B"]
    );

    fixture.input(crate::key_input(UiKey::Undo));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, "A界 B");
}

#[test]
fn transform_undo_reports_the_newer_edit_that_must_move_first() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("left right");
    fixture.app.acknowledge_persistence(sequence, true);
    for _ in 0..6 {
        fixture.input(crate::key_input(UiKey::Move {
            movement: CursorMovement::GraphemeBack,
            extend_selection: false,
        }));
    }
    let split = fixture.effects(crate::key_input(UiKey::PrimaryCharacter('t')));
    let split_sequence = board_operation(&split).sequence;
    fixture.app.acknowledge_persistence(split_sequence, true);
    fixture.input(crate::key_input(UiKey::Character('!')));
    let revision = fixture.effects(crate::key_input(UiKey::Escape));
    let revision_sequence = revision
        .first()
        .and_then(Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("newer editor revision");
    fixture.app.acknowledge_persistence(revision_sequence, true);
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::VisualUp,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Enter));

    let effects = fixture.effects(crate::key_input(UiKey::Undo));
    assert!(effects.is_empty());
    assert_eq!(
        fixture.app.status_text(),
        Some("Undo unavailable: undo the newer edit in the affected thought first")
    );
    assert_eq!(
        fixture
            .app
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>(),
        ["left", "! right"]
    );
}

#[test]
fn mouse_commands_preserves_the_exact_selection_captured_on_edit_exit() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("exact selection");
    fixture.app.acknowledge_persistence(sequence, true);
    fixture.input(crate::key_input(UiKey::SelectAll));
    let commit = fixture.effects(crate::key_input(UiKey::Escape));
    assert!(commit.is_empty());
    let commands = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 100, 12))
        .controls
        .into_iter()
        .find_map(|(target, area)| (target == HitTarget::Commands).then_some(area))
        .expect("commands footer target");
    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: commands.x,
        row: commands.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert!(effects.is_empty());

    for character in "extract selection".chars() {
        fixture.input(crate::key_input(UiKey::Character(character)));
    }
    assert_eq!(
        fixture.app.palette_view().expect("palette").1,
        ["Extract selection as new thought"]
    );
    let item = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 100, 12))
        .overlay
        .expect("palette overlay")
        .items[0];
    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: item.x,
        row: item.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert_eq!(board_operation(&effects).kind, BoardOperationKind::Extract);
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, "");
    assert_eq!(
        fixture.app.state.board.live_thoughts()[1].content,
        "exact selection"
    );
}

#[test]
fn edit_mode_redo_reapplies_the_transformation_just_undone_from_its_source() {
    let mut fixture = Fixture::new();
    fixture.paste("left right");
    for _ in 0..6 {
        fixture.input(crate::key_input(UiKey::Move {
            movement: CursorMovement::GraphemeBack,
            extend_selection: false,
        }));
    }
    fixture.input(crate::key_input(UiKey::PrimaryCharacter('t')));
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::VisualUp,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Enter));

    let undo = fixture.effects(crate::key_input(UiKey::Undo));
    assert!(matches!(
        undo.as_slice(),
        [Effect::CommitHistoryMove {
            scope: UndoScope::Board,
            undo: true,
            ..
        }]
    ));
    let redo = fixture.effects(crate::key_input(UiKey::Redo));
    assert!(matches!(
        redo.as_slice(),
        [Effect::CommitHistoryMove {
            scope: UndoScope::Board,
            undo: false,
            ..
        }]
    ));
    assert_eq!(
        fixture
            .app
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>(),
        ["left", " right"]
    );
}
