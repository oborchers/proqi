//! Migration 14: synthesize legacy identities once, including dormant history.

mod history;
mod lineage;

use rusqlite::{Connection, params};
use serde_json::Value;

use super::support::{map_sql_error, session_id_from_blob, thought_id_from_blob};
use crate::{
    domain::{BoardOperation, SessionId, ThoughtRevision},
    ports::store::StoreError,
};
use lineage::Lineage;

struct ThoughtRow {
    id: Vec<u8>,
    identity: String,
    content: String,
    annotations: Value,
    live: bool,
}

struct PayloadRow {
    table: String,
    id: Vec<u8>,
    original: Value,
}

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let mut statement = connection
        .prepare("SELECT id FROM sessions ORDER BY id")
        .map_err(map_sql_error)?;
    let sessions = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(map_sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_sql_error)?;
    for session in sessions {
        migrate_session(connection, session_id_from_blob(session)?)?;
    }
    Ok(())
}

fn migrate_session(connection: &Connection, session: SessionId) -> Result<(), StoreError> {
    let mut thoughts = thoughts(connection, session)?;
    let payloads = payloads(connection, session)?;
    let mut lineage = Lineage::default();
    for payload in &payloads {
        lineage.collect(&payload.original)?;
    }
    for payload in &payloads {
        lineage.link_payload(&payload.original)?;
    }
    lineage.current_board();
    // Current live board anchors all identities before any dormant snapshot receives a number.
    for thought in thoughts.iter_mut().filter(|thought| thought.live) {
        lineage.assign(
            &thought.identity,
            &thought.content,
            &mut thought.annotations,
        )?;
    }
    for payload in payloads {
        let mut assigned = payload.original.clone();
        lineage.assign_payload(&mut assigned)?;
        if assigned == payload.original {
            continue;
        }
        let encoded = canonical_payload(assigned)?;
        let (sql, column) = match payload.table.as_str() {
            "board_operations" => (
                "UPDATE board_operations SET payload_json = ?2 WHERE id = ?1",
                "id",
            ),
            "thought_revisions" => (
                "UPDATE thought_revisions SET payload_json = ?2 WHERE id = ?1",
                "id",
            ),
            "commit_receipts" => (
                "UPDATE commit_receipts SET request_json = ?2 WHERE external_id = ?1",
                "external_id",
            ),
            _ => return Err(corrupt("invalid migration payload owner")),
        };
        let _ = column;
        connection
            .execute(sql, params![payload.id, encoded])
            .map_err(map_sql_error)?;
    }
    lineage.current_board();
    for thought in &mut thoughts {
        if !thought.live {
            lineage.assign(
                &thought.identity,
                &thought.content,
                &mut thought.annotations,
            )?;
        }
        let annotations: Vec<crate::domain::ContentAnnotation> =
            serde_json::from_value(thought.annotations.clone()).map_err(corrupt)?;
        connection
            .execute(
                "UPDATE thoughts SET annotations_json = ?2 WHERE id = ?1",
                params![
                    thought.id,
                    serde_json::to_string(&annotations).map_err(corrupt)?
                ],
            )
            .map_err(map_sql_error)?;
    }
    super::attachment_numbering::persist(connection, session, lineage.counters())?;
    // Check the synthesized live uniqueness invariant before the migration transaction commits.
    super::load::load_board(connection, session)?;
    Ok(())
}

fn thoughts(connection: &Connection, session: SessionId) -> Result<Vec<ThoughtRow>, StoreError> {
    let mut statement = connection.prepare("SELECT id, content, annotations_json, deleted_at IS NULL FROM thoughts WHERE session_id = ?1 ORDER BY deleted_at IS NOT NULL, position, id").map_err(map_sql_error)?;
    let rows = statement
        .query_map([session.database_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })
        .map_err(map_sql_error)?;
    rows.map(|row| {
        let (id, content, annotations, live) = row.map_err(map_sql_error)?;
        Ok(ThoughtRow {
            identity: thought_id_from_blob(id.clone())?.to_string(),
            id,
            content,
            annotations: serde_json::from_str(&annotations).map_err(corrupt)?,
            live,
        })
    })
    .collect()
}

fn payloads(connection: &Connection, session: SessionId) -> Result<Vec<PayloadRow>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT owner, id, payload FROM (
         SELECT 'board_operations' AS owner, id, payload_json AS payload, sequence, 0 AS rank FROM board_operations WHERE session_id = ?1
         UNION ALL SELECT 'thought_revisions', id, payload_json, sequence, 1 FROM thought_revisions WHERE session_id = ?1
         UNION ALL SELECT 'commit_receipts', external_id, request_json, sequence, 2 FROM commit_receipts WHERE session_id = ?1
         ) ORDER BY sequence, rank, id"
    ).map_err(map_sql_error)?;
    let rows = statement
        .query_map([session.database_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(map_sql_error)?;
    let mut result = Vec::new();
    for row in rows {
        let (table, id, encoded) = row.map_err(map_sql_error)?;
        let original: Value = serde_json::from_str(&encoded).map_err(corrupt)?;
        if original.get("compacted_version").is_some() {
            super::receipt_compaction::decode(&encoded)?
                .ok_or_else(|| corrupt("invalid compacted receipt"))?;
            continue;
        }
        // Validate typed history before introducing any metadata.
        canonical_payload(original.clone())?;
        result.push(PayloadRow {
            table,
            id,
            original,
        });
    }
    Ok(result)
}

fn canonical_payload(value: Value) -> Result<String, StoreError> {
    if value.get("forward").is_some() {
        let operation: BoardOperation = serde_json::from_value(value).map_err(corrupt)?;
        operation.validate_annotations().map_err(corrupt)?;
        serde_json::to_string(&operation).map_err(corrupt)
    } else if value.get("before_content").is_some() {
        let revision: ThoughtRevision = serde_json::from_value(value).map_err(corrupt)?;
        revision.validate_annotations().map_err(corrupt)?;
        serde_json::to_string(&revision).map_err(corrupt)
    } else {
        serde_json::to_string(&value).map_err(corrupt)
    }
}

fn corrupt(error: impl std::fmt::Display) -> StoreError {
    StoreError::Corrupt(error.to_string())
}
