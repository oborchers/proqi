use super::*;
use proqi::application::ApplicationError;

#[test]
fn submission_lock_blocks_every_mutating_board_path_until_released() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.create("protected");
    reduce(
        &mut fixture.state,
        Action::BeginSubmission {
            thought_ids: vec![thought_id],
        },
    )
    .expect("lock");

    let cases = [
        Action::DeleteThought {
            operation_id: fixture.operation_id(),
            thought_id,
            kind: BoardOperationKind::Delete,
            at: fixture.time(),
        },
        Action::MoveThought {
            operation_id: fixture.operation_id(),
            thought_id,
            to: 0,
            at: fixture.time(),
        },
        Action::SetPresentation {
            operation_id: fixture.operation_id(),
            thought_id,
            presentation: ThoughtPresentation::Collapsed,
            at: fixture.time(),
        },
        Action::DuplicateThoughts {
            operation_id: fixture.operation_id(),
            thought_ids: vec![thought_id],
            duplicate_ids: vec![fixture.ids.thought_id()],
            at: fixture.time(),
        },
        Action::Undo {
            operation_id: fixture.operation_id(),
            scope: UndoScope::Board,
            at: fixture.time(),
        },
    ];
    for action in cases {
        assert_eq!(
            reduce(&mut fixture.state, action),
            Err(ApplicationError::ThoughtLocked(thought_id))
        );
    }

    reduce(
        &mut fixture.state,
        Action::EndSubmission {
            thought_ids: vec![thought_id],
        },
    )
    .expect("unlock");
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::DeleteThought {
            operation_id,
            thought_id,
            kind: BoardOperationKind::Delete,
            at,
        },
    )
    .expect("delete after unlock");
}

#[test]
fn locked_thoughts_can_be_copied_but_never_cut_after_clipboard_success() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.create("protected");
    reduce(
        &mut fixture.state,
        Action::BeginSubmission {
            thought_ids: vec![thought_id],
        },
    )
    .expect("lock");

    let copy_request = fixture.ids.request_id();
    reduce(
        &mut fixture.state,
        Action::CopyThoughts {
            request_id: copy_request,
            thought_ids: vec![thought_id],
        },
    )
    .expect("copy request");
    reduce(
        &mut fixture.state,
        Action::ClipboardResult {
            request_id: copy_request,
            result: Ok(()),
        },
    )
    .expect("copy completion");

    let cut_request = fixture.ids.request_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    assert_eq!(
        reduce(
            &mut fixture.state,
            Action::CutThoughts {
                request_id: cut_request,
                operation_id,
                thought_ids: vec![thought_id],
                at,
            },
        ),
        Err(ApplicationError::ThoughtLocked(thought_id))
    );
}
