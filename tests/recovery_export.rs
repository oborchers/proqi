//! Atomic recovery export and UI recovery contracts.

use std::fs;

use proqi::{
    adapters::{
        memory::{FakeClock, FakeIdGenerator},
        recovery::FileRecoveryExporter,
    },
    application::{Action, AppState, Effect, FailureCode, capture_recovery, reduce},
    domain::{OperationSequence, Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
    ports::{
        environment::{Clock, IdGenerator},
        recovery::{RecoveryDocument, RecoveryExporter},
    },
    ui::{BoardApp, UiInput, UiKey},
};

fn state() -> (AppState, FakeIdGenerator, FakeClock) {
    let mut ids = FakeIdGenerator::new(1_725_100_000_000);
    let now = Timestamp::from_millis(10);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-recovery-contract"),
        now,
    )
    .expect("session");
    let thought = Thought::new(
        ids.thought_id(),
        session.id,
        " exact\r\nGrüße 界 ".to_owned(),
        ThoughtPosition::new(0),
        now,
    );
    let board = SessionBoard::new(session, vec![thought]).expect("board");
    (
        AppState::new(board),
        ids,
        FakeClock::new(Timestamp::from_millis(20)),
    )
}

#[test]
fn recovery_export_is_atomic_private_and_lossless() {
    let (state, mut ids, clock) = state();
    let directory = tempfile::tempdir().expect("temporary directory");
    let recovery = directory.path().join("recovery");
    let mut exporter = FileRecoveryExporter::new(recovery.clone());
    let request_id = ids.request_id();
    let document = capture_recovery(&state, clock.now());
    let path = exporter
        .export(request_id, &document)
        .expect("recovery export");

    assert_eq!(path.parent(), Some(recovery.as_path()));
    assert!(
        !recovery
            .join(format!(
                ".recovery-{}-{request_id}.tmp",
                state.board.session.id
            ))
            .exists()
    );
    let decoded: RecoveryDocument =
        serde_json::from_slice(&fs::read(&path).expect("read export")).expect("decode export");
    assert_eq!(decoded, document);
    assert_eq!(decoded.thoughts[0].content, " exact\r\nGrüße 界 ");
    assert!(exporter.export(request_id, &document).is_err());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&recovery)
                .expect("directory")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(path).expect("file").permissions().mode() & 0o777,
            0o600
        );
    }
}

#[cfg(unix)]
#[test]
fn recovery_directory_symlinks_are_refused() {
    use std::os::unix::fs::symlink;

    let (state, mut ids, clock) = state();
    let directory = tempfile::tempdir().expect("temporary directory");
    let target = directory.path().join("target");
    fs::create_dir(&target).expect("target");
    let linked = directory.path().join("linked");
    symlink(&target, &linked).expect("symlink");
    let result = FileRecoveryExporter::new(linked)
        .export(ids.request_id(), &capture_recovery(&state, clock.now()));
    assert!(result.is_err());
    assert!(
        fs::read_dir(target)
            .expect("target contents")
            .next()
            .is_none()
    );
}

#[test]
fn failed_ui_state_offers_retry_and_an_exact_recovery_effect() {
    let (mut state, mut ids, clock) = state();
    let operation_id = ids.operation_id();
    let thought_id = ids.thought_id();
    let effects = reduce(
        &mut state,
        Action::CreateThought {
            thought_id,
            operation_id,
            content: "unsaved".to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at: clock.now(),
        },
    )
    .expect("create");
    let sequence = effects[0]
        .persistence_batch()
        .and_then(|batch| batch.sequence())
        .expect("sequence");
    let mut app = BoardApp::new(state, proqi::adapters::editor::RopeEditorFactory);
    app.sync_editor_from_state();
    assert!(
        app.handle(UiInput::Key(UiKey::Character('x')), &mut ids, &clock)
            .is_empty()
    );
    app.acknowledge_persistence(sequence, false);

    assert!(
        app.handle(UiInput::Key(UiKey::Quit), &mut ids, &clock)
            .is_empty()
    );
    assert!(!app.quit);

    assert_eq!(
        app.handle(UiInput::Key(UiKey::Character('r')), &mut ids, &clock),
        vec![Effect::RetryPersistence { sequence }]
    );
    app.acknowledge_persistence(sequence, false);
    let export = app.handle(UiInput::Key(UiKey::Character('w')), &mut ids, &clock);
    let [
        Effect::ExportRecovery {
            request_id,
            document,
        },
    ] = export.as_slice()
    else {
        panic!("expected recovery effect");
    };
    assert_eq!(document.failed_sequence, Some(sequence));
    assert!(
        document
            .thoughts
            .iter()
            .any(|thought| thought.content == "unsavedx")
    );
    assert_eq!(
        app.state.durability,
        proqi::application::DurabilityState::Failed {
            durable: OperationSequence::ZERO,
            failed: sequence,
            code: FailureCode::StorageFailed,
        }
    );
    app.complete_recovery_export(*request_id, Ok(std::env::temp_dir().join("recovered.json")));
    app.handle(UiInput::Key(UiKey::Quit), &mut ids, &clock);
    assert!(app.quit);
}
