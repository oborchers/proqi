use ratatui_core::layout::Rect;

use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{AppState, Effect, InteractionMode},
    domain::{
        ContentAnnotation, ContentAnnotationKind, Session, SessionBoard, Thought, ThoughtPosition,
        Timestamp,
    },
    ports::{editor::CursorMovement, environment::IdGenerator as _},
    ui::{BoardApp, UiInput, UiKey},
};

fn app() -> (BoardApp, FakeIdGenerator, FakeClock) {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("input-recovery-ui"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let board = SessionBoard::new(session, Vec::new()).expect("board");
    (
        BoardApp::new(AppState::new(board), RopeEditorFactory),
        ids,
        FakeClock::new(Timestamp::from_millis(2)),
    )
}

#[test]
fn edit_cursor_selection_affinity_and_scroll_survive_checkpoint() {
    let (mut original, mut ids, clock) = app();
    let content = (0..40)
        .map(|index| format!("row {index} Grüße 界"))
        .collect::<Vec<_>>()
        .join("\n");
    let effects = original.handle(UiInput::Paste(content), &mut ids, &clock);
    acknowledge(&mut original, &effects);
    let _layout = original.prepare_frame(Rect::new(0, 0, 24, 7));
    let _effects = original.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::DocumentStart,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    for _ in 0..20 {
        let _effects = original.handle(
            UiInput::Key(UiKey::Move {
                movement: CursorMovement::VisualDown,
                extend_selection: false,
            }),
            &mut ids,
            &clock,
        );
    }
    let _effects = original.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::WordForward,
            extend_selection: true,
        }),
        &mut ids,
        &clock,
    );
    let checkpoint = original.input_recovery_state().expect("Edit checkpoint");
    let expected_editor = checkpoint.editor.expect("editor state");
    let expected_mode = checkpoint.mode;

    let mut restored = BoardApp::new(original.state.clone(), RopeEditorFactory);
    assert!(restored.restore_input_recovery_state(checkpoint));
    let _layout = restored.prepare_frame(Rect::new(0, 0, 24, 7));
    let actual = restored
        .input_recovery_state()
        .expect("restored checkpoint");

    assert_eq!(actual.mode, expected_mode);
    assert_eq!(actual.editor, Some(expected_editor));
    assert!(matches!(
        restored.interaction_mode(),
        InteractionMode::Edit { .. }
    ));
}

#[test]
fn empty_compose_owner_survives_without_inventing_durable_content() {
    let (original, _ids, _clock) = app();
    let checkpoint = original.input_recovery_state().expect("Compose checkpoint");
    assert!(matches!(
        checkpoint.mode,
        crate::ports::runtime::InputRecoveryMode::Compose
    ));

    let mut restored = BoardApp::new(original.state.clone(), RopeEditorFactory);
    assert!(restored.restore_input_recovery_state(checkpoint));
    let _layout = restored.prepare_frame(Rect::new(0, 0, 40, 8));

    assert_eq!(restored.interaction_mode(), InteractionMode::Compose);
    assert_eq!(restored.visible_thought_count(), 0);
}

#[test]
fn board_selection_expanded_fold_and_semantic_scroll_survive_checkpoint() {
    let mut ids = FakeIdGenerator::new(1_725_000_100_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("input-recovery-board"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let mut first = Thought::new(
        ids.thought_id(),
        session.id,
        "large folded payload".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    first
        .set_annotations(vec![ContentAnnotation {
            start: 0,
            end: first.content.len(),
            kind: ContentAnnotationKind::LargePaste {
                lines: 1,
                graphemes: first.content.chars().count(),
            },
        }])
        .expect("annotation");
    let second = Thought::new(
        ids.thought_id(),
        session.id,
        "second".to_owned(),
        ThoughtPosition::new(1),
        Timestamp::from_millis(1),
    );
    let first_id = first.id;
    let second_id = second.id;
    let board = SessionBoard::new(session, vec![first, second]).expect("board");
    let mut original = BoardApp::new(AppState::new(board), RopeEditorFactory);
    original.replace_board_selection([first_id, second_id]);
    original.expanded_folds.insert((first_id, 0));
    original.scroll_board_to(crate::ui::layout::scroll::ScrollAnchor::Content {
        thought_id: first_id,
        position: crate::ui::layout::scroll::ContentAnchor::Canonical(3),
    });
    let checkpoint = original.input_recovery_state().expect("Board checkpoint");

    let mut restored = BoardApp::new(original.state.clone(), RopeEditorFactory);
    assert!(restored.restore_input_recovery_state(checkpoint.clone()));
    let actual = restored
        .input_recovery_state()
        .expect("restored Board checkpoint");

    assert_eq!(actual, checkpoint);
}

fn acknowledge(app: &mut BoardApp, effects: &[Effect]) {
    let sequence = effects
        .first()
        .and_then(Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("persistence sequence");
    let _effects = app.acknowledge_persistence(sequence, true);
}
