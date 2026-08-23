//! Typed identifier contract across public text, Serde, and SQLite BLOBs.

use std::{collections::HashSet, str::FromStr};

use proptest::prelude::*;
use proqi::{
    adapters::{memory::FakeIdGenerator, runtime::SystemIdGenerator},
    domain::{InstanceId, OperationId, RequestId, RevisionId, SessionId, SubmissionId, ThoughtId},
    ports::environment::IdGenerator,
};
use rusqlite::{Connection, params};
use uuid::Uuid;

fn v7_bytes(mut bytes: [u8; 16]) -> [u8; 16] {
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes
}

#[test]
fn every_registered_type_generates_canonical_uuid_v7() {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let values = [
        ids.session_id().to_string(),
        ids.thought_id().to_string(),
        ids.revision_id().to_string(),
        ids.operation_id().to_string(),
        ids.instance_id().to_string(),
        ids.request_id().to_string(),
        ids.submission_id().to_string(),
    ];
    for (value, prefix) in values
        .iter()
        .zip(["ses_", "tht_", "rev_", "op_", "ins_", "req_", "sub_"])
    {
        assert!(value.starts_with(prefix));
        assert_eq!(value.len(), prefix.len() + 26);
        assert!(
            value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        );
    }
}

#[test]
fn complete_uuid_entropy_survives_public_and_database_round_trips() {
    let bytes = v7_bytes([
        0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x0f, 0xf1, 0x3f, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x89,
    ]);
    let id = SessionId::from_database_bytes(bytes).expect("valid UUIDv7 bytes");
    let reparsed = SessionId::from_str(&id.to_string()).expect("canonical public ID");
    assert_eq!(reparsed.database_bytes(), bytes);
    assert_eq!(reparsed.into_uuid(), Uuid::from_bytes(bytes));
}

#[test]
fn prefixes_are_type_checked() {
    let mut ids = FakeIdGenerator::new(123);
    let session = ids.session_id().to_string();
    let thought = ids.thought_id().to_string();
    assert!(ThoughtId::from_str(&session).is_err());
    assert!(SessionId::from_str(&thought).is_err());
    assert!(OperationId::from_str(&session).is_err());
}

#[test]
fn noncanonical_or_non_v7_inputs_are_rejected() {
    let mut ids = FakeIdGenerator::new(123);
    let canonical = ids.session_id().to_string();
    assert!(SessionId::from_str(&canonical.to_uppercase()).is_err());
    assert!(SessionId::from_str(&canonical.replace('_', "-")).is_err());
    assert!(SessionId::from_str("ses_short").is_err());

    let mut invalid_padding = canonical;
    invalid_padding.pop();
    invalid_padding.push('1');
    assert!(SessionId::from_str(&invalid_padding).is_err());

    let v4 = Uuid::parse_str("f47ac10b-58cc-4372-a567-0e02b2c3d479").expect("fixture");
    assert!(SessionId::from_uuid(v4).is_err());
}

#[test]
fn serde_uses_the_same_stable_public_representation() {
    let mut ids = FakeIdGenerator::new(123);
    let id = RequestId::from_str(&ids.request_id().to_string()).expect("request fixture");
    let json = serde_json::to_string(&id).expect("serialize");
    assert_eq!(json, format!("\"{id}\""));
    assert_eq!(
        serde_json::from_str::<RequestId>(&json).expect("deserialize"),
        id
    );
    assert!(serde_json::from_str::<SessionId>(&json).is_err());
}

#[test]
fn sqlite_blob_round_trips_are_lossless_for_every_type() {
    let connection = Connection::open_in_memory().expect("SQLite");
    connection
        .execute(
            "CREATE TABLE ids (kind TEXT PRIMARY KEY, value BLOB NOT NULL)",
            [],
        )
        .expect("schema");
    let mut ids = FakeIdGenerator::new(123);

    macro_rules! round_trip {
        ($kind:literal, $value:expr, $type:ty) => {{
            let value = $value;
            connection
                .execute(
                    "INSERT INTO ids(kind, value) VALUES (?1, ?2)",
                    params![$kind, value.database_bytes().as_slice()],
                )
                .expect("insert");
            let stored: Vec<u8> = connection
                .query_row("SELECT value FROM ids WHERE kind = ?1", [$kind], |row| {
                    row.get(0)
                })
                .expect("read");
            let bytes: [u8; 16] = stored.try_into().expect("16-byte BLOB");
            assert_eq!(
                <$type>::from_database_bytes(bytes).expect("typed BLOB"),
                value
            );
        }};
    }

    round_trip!("session", ids.session_id(), SessionId);
    round_trip!("thought", ids.thought_id(), ThoughtId);
    round_trip!("revision", ids.revision_id(), RevisionId);
    round_trip!("operation", ids.operation_id(), OperationId);
    round_trip!("instance", ids.instance_id(), InstanceId);
    round_trip!("request", ids.request_id(), RequestId);
    round_trip!("submission", ids.submission_id(), SubmissionId);
}

#[test]
fn public_order_matches_uuid_and_database_byte_order() {
    let mut ids = FakeIdGenerator::new(123);
    let values: Vec<_> = (0..1_000).map(|_| ids.session_id()).collect();
    assert!(values.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(
        values
            .windows(2)
            .all(|pair| pair[0].to_string() < pair[1].to_string())
    );
    assert!(
        values
            .windows(2)
            .all(|pair| pair[0].database_bytes() < pair[1].database_bytes())
    );
}

#[test]
fn generated_sample_has_no_collisions() {
    let mut generator = SystemIdGenerator;
    let mut values = HashSet::with_capacity(50_000);
    for _ in 0..50_000 {
        assert!(values.insert(generator.operation_id()));
    }
}

proptest! {
    #[test]
    fn arbitrary_uuid_v7_bytes_round_trip_without_loss(seed in any::<[u8; 16]>()) {
        let bytes = v7_bytes(seed);
        let id = SessionId::from_database_bytes(bytes).expect("adjusted UUIDv7");
        prop_assert_eq!(SessionId::from_str(&id.to_string()).expect("public round trip"), id);
        prop_assert_eq!(id.database_bytes(), bytes);
    }
}
