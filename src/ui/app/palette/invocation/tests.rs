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
