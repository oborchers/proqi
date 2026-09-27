use super::*;
use crate::{
    adapters::{editor::RopeEditorFactory, memory::FakeIdGenerator},
    application::{AppState, DurabilityState, FailureCode},
    domain::{OperationSequence, Session, SessionBoard, Timestamp},
    ports::environment::IdGenerator as _,
    ui::{ShortcutActionId, UiSettings},
};

fn empty_app() -> BoardApp {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-commands-context"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let board = SessionBoard::new(session, Vec::new()).expect("board");
    BoardApp::with_settings(
        AppState::new(board),
        UiSettings::default(),
        RopeEditorFactory,
    )
}

fn metadata(app: &BoardApp, action: ShortcutActionId) -> CommandMetadata {
    app.settings
        .shortcuts
        .descriptor(action)
        .and_then(|descriptor| descriptor.commands)
        .expect("Commands metadata")
}

#[test]
fn recovery_actions_follow_durable_pending_and_failed_state_exactly() {
    let mut app = empty_app();
    let retry = metadata(&app, ShortcutActionId::RetryStorage);
    let export = metadata(&app, ShortcutActionId::ExportRecovery);
    let undo = metadata(&app, ShortcutActionId::Undo);

    let durable = app.capture_command_context();
    assert_eq!(
        durable.applicability(retry),
        Applicability::disabled("Available after a save failure")
    );

    app.state.durability = DurabilityState::Pending {
        durable: OperationSequence::ZERO,
        latest: OperationSequence::new(1),
    };
    let pending = app.capture_command_context();
    assert_eq!(
        pending.applicability(export),
        Applicability::disabled("Available after a save failure")
    );

    app.state.durability = DurabilityState::Failed {
        durable: OperationSequence::ZERO,
        failed: OperationSequence::new(1),
        code: FailureCode::StorageFailed,
    };
    let failed = app.capture_command_context();
    assert_eq!(failed.applicability(retry), Applicability::ENABLED);
    assert_eq!(failed.applicability(export), Applicability::ENABLED);
    assert_eq!(
        failed.applicability(undo),
        Applicability::disabled("Nothing to undo in the query")
    );
    assert_eq!(
        failed.relevance(retry, PaletteHistoryContext::EMPTY),
        Some(0)
    );
}

#[test]
fn selected_transfer_retry_is_available_only_for_its_matching_action() {
    use crate::{
        domain::{BoardItemId, Separator, Thought, ThoughtPosition},
        ports::transfer::{SessionTransferBatchRequest, TransferItem},
    };

    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let at = Timestamp::from_millis(1);
    let session = Session::new(ids.session_id(), std::env::temp_dir(), at).expect("session");
    let destination = ids.session_id();
    let thought = Thought::new(
        ids.thought_id(),
        session.id,
        "first".to_owned(),
        ThoughtPosition::new(0),
        at,
    );
    let separator = Separator::new(ids.separator_id(), session.id, ThoughtPosition::new(1), at);
    let mut app = BoardApp::new(
        AppState::new(
            SessionBoard::with_separators(session, vec![thought.clone()], vec![separator.clone()])
                .expect("board"),
        ),
        RopeEditorFactory,
    );
    app.state.focused_item = Some(thought.id.into());
    app.replace_board_selection([
        BoardItemId::Thought(thought.id),
        BoardItemId::Separator(separator.id),
    ]);
    let request = SessionTransferBatchRequest {
        source_session_id: app.state.board.session.id,
        destination_session_id: destination,
        operation_id: ids.operation_id(),
        removal_operation_id: ids.operation_id(),
        items: vec![TransferItem {
            source_thought_id: thought.id,
            destination_thought_id: ids.thought_id(),
            content: thought.content.clone(),
            annotations: thought.annotations.clone(),
            name: thought.name.clone(),
        }],
        remove_source: true,
    };
    app.pending_transfer_batches
        .insert(request.operation_id, request);
    let remove = metadata(&app, ShortcutActionId::SendSessionRemove);
    let keep = metadata(&app, ShortcutActionId::SendSession);
    let cleanup = metadata(&app, ShortcutActionId::ReflowThought);
    let context = app.capture_command_context();
    assert_eq!(context.applicability(remove), Applicability::ENABLED);
    assert_eq!(
        context.applicability(keep),
        Applicability::disabled("Thought has an operation in progress")
    );
    assert_eq!(
        context.applicability(cleanup),
        Applicability::disabled("Thought has an operation in progress")
    );
    for request in app.pending_transfer_batches.values_mut() {
        request.remove_source = false;
    }
    let keep_retry = app.capture_command_context();
    assert_eq!(keep_retry.applicability(keep), Applicability::ENABLED);
    assert_eq!(
        keep_retry.applicability(remove),
        Applicability::disabled("Thought has an operation in progress")
    );
}
