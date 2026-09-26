//! Board completion after a durable plain-text export: removal, replacement, and undo.

use std::path::PathBuf;

use super::{
    Action, BoardItemId, BoardOperationKind, ContentAnnotationKind, Effect, Fixture, IdGenerator,
    ThoughtId, UndoScope, move_history, reduce,
};
use proqi::application::{ApplicationError, ExportBoardChange, ExportCompletion};

fn completion(fixture: &mut Fixture, ids: &[ThoughtId], change: ExportBoardChange) -> Action {
    let expected_sources = ids
        .iter()
        .map(|id| fixture.state.board.thought(*id).expect("source").clone())
        .collect();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    Action::CompleteExport(ExportCompletion {
        operation_id,
        thought_ids: ids.to_vec(),
        expected_sources,
        change,
        at,
    })
}

fn live_contents(fixture: &Fixture) -> Vec<String> {
    fixture
        .state
        .board
        .live_thoughts()
        .into_iter()
        .map(|thought| thought.content.clone())
        .collect()
}

#[test]
fn export_and_remove_is_one_batch_that_undo_restores() {
    let mut fixture = Fixture::new();
    let first = fixture.create("first");
    let kept = fixture.create("kept");
    let third = fixture.create("third");
    let action = completion(&mut fixture, &[first, third], ExportBoardChange::Remove);
    let effects = reduce(&mut fixture.state, action).expect("remove");
    let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
        panic!("one durable operation: {effects:?}");
    };
    assert_eq!(operation.kind, BoardOperationKind::ExportAndRemove);
    assert_eq!(live_contents(&fixture), ["kept"]);
    assert_eq!(
        fixture.state.board.live_thoughts()[0].id,
        kept,
        "unselected thought stays"
    );

    move_history(&mut fixture, UndoScope::Board, true);
    assert_eq!(live_contents(&fixture), ["first", "kept", "third"]);
    move_history(&mut fixture, UndoScope::Board, false);
    assert_eq!(live_contents(&fixture), ["kept"]);
}

#[test]
fn single_export_and_remove_uses_the_export_kind() {
    let mut fixture = Fixture::new();
    let only = fixture.create("only");
    let action = completion(&mut fixture, &[only], ExportBoardChange::Remove);
    let effects = reduce(&mut fixture.state, action).expect("remove");
    let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
        panic!("one durable operation");
    };
    assert_eq!(operation.kind, BoardOperationKind::ExportAndRemove);
    assert!(fixture.state.board.live_thoughts().is_empty());
}

#[test]
fn export_and_replace_inserts_a_numbered_file_reference_at_the_first_source() {
    let mut fixture = Fixture::new();
    let before = fixture.create("before");
    let first = fixture.create("first");
    let separator = fixture.insert_separator(2);
    let second = fixture.create("second");
    let reference = fixture.ids.thought_id();
    let path = PathBuf::from("/tmp/export dir/Grüße.txt");
    let action = completion(
        &mut fixture,
        &[first, second],
        ExportBoardChange::ReplaceWithReference {
            reference_thought_id: reference,
            path: path.clone(),
        },
    );
    let effects = reduce(&mut fixture.state, action).expect("replace");
    let [
        Effect::CommitBoardOperation(operation),
        Effect::CheckAttachments(_),
    ] = effects.as_slice()
    else {
        panic!("one durable operation plus an immediate health check: {effects:?}");
    };
    assert_eq!(operation.kind, BoardOperationKind::ExportAndReplace);
    let order = fixture
        .state
        .board
        .live_items()
        .iter()
        .map(|item| item.id())
        .collect::<Vec<_>>();
    assert_eq!(
        order,
        [
            BoardItemId::Thought(before),
            BoardItemId::Thought(reference),
            BoardItemId::Separator(separator),
        ]
    );
    let created = fixture.state.board.thought(reference).expect("reference");
    assert_eq!(created.content, "/tmp/export dir/Grüße.txt ");
    assert_eq!(created.name, None);
    assert_eq!(created.annotations.len(), 1);
    let annotation = &created.annotations[0];
    assert_eq!(
        (annotation.start, annotation.end),
        (0, created.content.len() - 1)
    );
    let ContentAnnotationKind::Attachment {
        ordinal,
        image,
        display_name,
    } = &annotation.kind
    else {
        panic!("file attachment");
    };
    assert!(!image);
    assert_eq!(ordinal.map(proqi::domain::AttachmentOrdinal::get), Some(1));
    assert_eq!(display_name, "Grüße.txt");
    assert_eq!(fixture.state.focused_thought_id(), Some(reference));

    move_history(&mut fixture, UndoScope::Board, true);
    assert_eq!(live_contents(&fixture), ["before", "first", "second"]);
    assert!(
        fixture
            .state
            .board
            .thought(reference)
            .is_some_and(|thought| !thought.is_live())
    );
    move_history(&mut fixture, UndoScope::Board, false);
    assert_eq!(
        live_contents(&fixture),
        ["before", "/tmp/export dir/Grüße.txt "]
    );
}

#[test]
fn a_changed_or_missing_source_keeps_the_board_unchanged() {
    let mut fixture = Fixture::new();
    let first = fixture.create("first");
    let action = completion(&mut fixture, &[first], ExportBoardChange::Remove);
    let revision_id = fixture.ids.revision_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::EditThought {
            thought_id: first,
            revision_id,
            before_content: "first".to_owned(),
            after_content: "first changed".to_owned(),
            before_annotations: Vec::new(),
            after_annotations: Vec::new(),
            before_cursor: proqi::domain::TextPosition::new(0, 0),
            after_cursor: proqi::domain::TextPosition::new(0, 0),
            at,
        },
    )
    .expect("edit");
    assert_eq!(
        reduce(&mut fixture.state, action),
        Err(ApplicationError::ContentConflict(first))
    );
    assert_eq!(live_contents(&fixture), ["first changed"]);

    let missing = fixture.ids.thought_id();
    let mut invalid = completion(&mut fixture, &[first], ExportBoardChange::Remove);
    if let Action::CompleteExport(ref mut request) = invalid {
        request.thought_ids = vec![missing];
        request.expected_sources[0].id = missing;
    }
    assert_eq!(
        reduce(&mut fixture.state, invalid),
        Err(ApplicationError::ThoughtNotFound(missing))
    );
}

#[test]
fn sources_must_arrive_in_board_order() {
    let mut fixture = Fixture::new();
    let first = fixture.create("first");
    let second = fixture.create("second");
    let action = completion(&mut fixture, &[second, first], ExportBoardChange::Remove);
    assert_eq!(
        reduce(&mut fixture.state, action),
        Err(ApplicationError::InvalidState)
    );
    assert_eq!(live_contents(&fixture), ["first", "second"]);
}
