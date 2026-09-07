//! Transactional attachment high-water marks and introducing-mutation validation.

use rusqlite::{Connection, params};

use crate::{
    domain::{
        AttachmentCounters, ContentAnnotation, ContentAnnotationKind, SessionBoard, SessionId,
    },
    ports::store::StoreError,
};

use super::support::map_sql_error;

pub(super) fn load(
    connection: &Connection,
    session: SessionId,
) -> Result<AttachmentCounters, StoreError> {
    let (image, file): (i64, i64) = connection
        .query_row(
            "SELECT attachment_image_high, attachment_file_high FROM sessions WHERE id = ?1",
            [session.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(map_sql_error)?;
    AttachmentCounters::new(
        u64::try_from(image).map_err(corrupt)?,
        u64::try_from(file).map_err(corrupt)?,
    )
    .map_err(corrupt)
}

pub(super) fn restore(connection: &Connection, board: &mut SessionBoard) -> Result<(), StoreError> {
    board
        .restore_attachment_counters(load(connection, board.session.id)?)
        .map_err(corrupt)
}

pub(super) fn persist(
    connection: &Connection,
    session: SessionId,
    counters: AttachmentCounters,
) -> Result<(), StoreError> {
    connection
        .execute(
            "UPDATE sessions SET attachment_image_high = max(attachment_image_high, ?2),
         attachment_file_high = max(attachment_file_high, ?3) WHERE id = ?1",
            params![
                session.database_bytes().as_slice(),
                i64::try_from(counters.image()).map_err(corrupt)?,
                i64::try_from(counters.file()).map_err(corrupt)?
            ],
        )
        .map_err(map_sql_error)?;
    Ok(())
}

pub(super) fn revision(
    connection: &Connection,
    revision: &crate::domain::ThoughtRevision,
) -> Result<(), StoreError> {
    let mut counters = load(connection, revision.session_id)?;
    for annotation in &revision.after_annotations {
        let ContentAnnotationKind::Attachment { image, ordinal, .. } = &annotation.kind else {
            continue;
        };
        let ordinal = ordinal.ok_or_else(|| corrupt("unassigned durable attachment"))?;
        let retained = revision.before_annotations.iter().any(|before| matches!(
            &before.kind, ContentAnnotationKind::Attachment { image: old_image, ordinal: old, .. }
            if old_image == image && *old == Some(ordinal)
        ));
        if !retained
            && ordinal.get()
                <= if *image {
                    counters.image()
                } else {
                    counters.file()
                }
        {
            return Err(StoreError::Conflict(
                "attachment ordinal was already allocated".to_owned(),
            ));
        }
    }
    let mut board = super::load::load_board(connection, revision.session_id)?;
    let thought = board
        .thought_mut(revision.thought_id)
        .ok_or_else(|| corrupt("missing revision thought"))?;
    thought.content.clone_from(&revision.after_content);
    thought.annotations.clone_from(&revision.after_annotations);
    board.validate().map_err(corrupt)?;
    counters
        .observe(&revision.after_annotations)
        .map_err(corrupt)?;
    persist(connection, revision.session_id, counters)
}

pub(super) fn validate_new(
    counters: AttachmentCounters,
    annotations: &[ContentAnnotation],
) -> Result<(), StoreError> {
    for annotation in annotations {
        if let ContentAnnotationKind::Attachment { image, ordinal, .. } = &annotation.kind {
            let ordinal = ordinal.ok_or_else(|| corrupt("unassigned durable attachment"))?;
            if ordinal.get()
                <= if *image {
                    counters.image()
                } else {
                    counters.file()
                }
            {
                return Err(StoreError::Conflict(
                    "attachment ordinal was already allocated".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_creation(
    board: &SessionBoard,
    operation: &crate::domain::BoardOperation,
) -> Result<(), StoreError> {
    use crate::domain::{BoardMutation, BoardOperationKind};
    fn visit(counters: AttachmentCounters, mutation: &BoardMutation) -> Result<(), StoreError> {
        match mutation {
            BoardMutation::AddThought { thought } => validate_new(counters, &thought.annotations),
            BoardMutation::Batch { mutations } => {
                for mutation in mutations {
                    visit(counters, mutation)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    if !matches!(
        operation.kind,
        BoardOperationKind::Create | BoardOperationKind::Duplicate
    ) {
        return Ok(());
    }
    visit(board.attachment_counters(), &operation.forward)
}

fn corrupt(error: impl std::fmt::Display) -> StoreError {
    StoreError::Corrupt(error.to_string())
}
