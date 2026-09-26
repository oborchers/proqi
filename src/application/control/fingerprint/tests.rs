use std::{fmt::Write as _, str::FromStr as _};

use sha2::{Digest as _, Sha256};

use crate::{
    domain::{
        AttachmentOrdinal, BoardItemId, ContentAnnotation, ContentAnnotationKind, OperationId,
        SessionId, ThoughtId,
    },
    ports::control::ControlMutation,
};

use super::semantic_fingerprint;

fn session() -> SessionId {
    SessionId::from_str("ses_06g30t7dv5qv55n1ppn3clis3k").expect("session fixture")
}

fn operation() -> OperationId {
    OperationId::from_str("op_06g30t8fudrq55fdkjqr6mpe44").expect("operation fixture")
}

fn thought() -> ThoughtId {
    ThoughtId::from_str("tht_06g30t8fudrq55fdkk348i7388").expect("thought fixture")
}

#[test]
fn legacy_and_typed_item_aliases_share_one_semantic_identity() {
    let legacy_delete = ControlMutation::Delete {
        operation_id: operation(),
        thought_id: thought(),
    };
    let typed_delete = ControlMutation::DeleteItems {
        operation_id: operation(),
        item_ids: vec![BoardItemId::Thought(thought())],
    };
    assert_eq!(fingerprint(&legacy_delete), fingerprint(&typed_delete));

    let legacy_move = ControlMutation::Move {
        operation_id: operation(),
        thought_id: thought(),
        position: 7,
    };
    let typed_move = ControlMutation::MoveItem {
        operation_id: operation(),
        item_id: BoardItemId::Thought(thought()),
        position: 7,
    };
    assert_eq!(fingerprint(&legacy_move), fingerprint(&typed_move));
}

#[test]
fn ordinary_and_provenance_preserving_adds_have_distinct_identities() {
    let ordinary = ControlMutation::Add {
        operation_id: operation(),
        thought_id: thought(),
        content: "same".to_owned(),
        annotations: Vec::new(),
        position: None,
    };
    let preserved = ControlMutation::PreserveAdd {
        operation_id: operation(),
        thought_id: thought(),
        content: "same".to_owned(),
        annotations: Vec::new(),
        name: None,
        position: None,
    };
    assert_ne!(fingerprint(&ordinary), fingerprint(&preserved));
}

#[test]
fn provenance_preserving_adds_fingerprint_exact_attachment_ordinals() {
    let preserved = |ordinal: u64| ControlMutation::PreserveAdd {
        operation_id: operation(),
        thought_id: thought(),
        content: "/tmp/asset".to_owned(),
        annotations: vec![ContentAnnotation {
            start: 0,
            end: 10,
            kind: ContentAnnotationKind::Attachment {
                ordinal: Some(AttachmentOrdinal::try_from(ordinal).expect("attachment ordinal")),
                image: false,
                display_name: "asset".to_owned(),
            },
        }],
        name: None,
        position: None,
    };
    assert_ne!(fingerprint(&preserved(1)), fingerprint(&preserved(2)));
}

#[test]
fn selected_transfer_fingerprint_binds_the_complete_ordered_cohort() {
    use crate::ports::transfer::TransferItem;

    let item = TransferItem {
        source_thought_id: thought(),
        destination_thought_id: ThoughtId::from_database_bytes(operation().database_bytes())
            .expect("destination"),
        content: "first".to_owned(),
        annotations: Vec::new(),
        name: None,
    };
    let second = TransferItem {
        source_thought_id: item.destination_thought_id,
        destination_thought_id: thought(),
        content: "second".to_owned(),
        annotations: Vec::new(),
        name: None,
    };
    let mutation = |items| ControlMutation::PreserveAddMany {
        operation_id: operation(),
        items,
    };
    let original = fingerprint(&mutation(vec![item.clone(), second.clone()]));
    assert_ne!(
        original,
        fingerprint(&mutation(vec![second.clone(), item.clone()]))
    );
    let mut changed = second;
    changed.content.push('!');
    assert_ne!(original, fingerprint(&mutation(vec![item, changed])));
}

#[test]
fn identical_extract_results_keep_distinct_exact_ranges() {
    let digest: [u8; 32] = Sha256::digest(b"aaa").into();
    let new_thought = ThoughtId::from_database_bytes(operation().database_bytes())
        .expect("derived thought fixture");
    let first = ControlMutation::ExtractThought {
        operation_id: operation(),
        thought_id: thought(),
        new_thought_id: new_thought,
        expected_digest: digest,
        start_byte: 0,
        end_byte: 1,
    };
    let second = ControlMutation::ExtractThought {
        operation_id: operation(),
        thought_id: thought(),
        new_thought_id: new_thought,
        expected_digest: digest,
        start_byte: 1,
        end_byte: 2,
    };
    assert_ne!(fingerprint(&first), fingerprint(&second));
}

#[test]
fn deterministic_duplicate_outputs_do_not_create_a_second_request_identity() {
    let first = ControlMutation::DuplicateItems {
        operation_id: operation(),
        item_ids: vec![BoardItemId::Thought(thought())],
        duplicate_ids: Vec::new(),
    };
    let second = ControlMutation::DuplicateItems {
        operation_id: operation(),
        item_ids: vec![BoardItemId::Thought(thought())],
        duplicate_ids: vec![BoardItemId::Thought(
            ThoughtId::from_database_bytes(operation().database_bytes())
                .expect("derived duplicate fixture"),
        )],
    };
    assert_eq!(fingerprint(&first), fingerprint(&second));
}

#[test]
fn unicode_control_merge_fingerprint_has_a_platform_stable_golden() {
    let second =
        ThoughtId::from_str("tht_06g30t8fudrq55fdkk348i739c").expect("second thought fixture");
    let mutation = ControlMutation::MergeThoughts {
        operation_id: operation(),
        thought_ids: vec![thought(), second],
        expected_digests: vec![Sha256::digest("Grüße 界"), Sha256::digest(b"line\t\n")]
            .into_iter()
            .map(Into::into)
            .collect(),
        separator: "\r\n\0界".to_owned(),
    };
    assert_eq!(
        hex(fingerprint(&mutation)),
        "de0ba38ddd272b51a308c01134277490c9f64c768a8ba40c751237287bd8a572"
    );
}

fn fingerprint(mutation: &ControlMutation) -> [u8; 32] {
    semantic_fingerprint(session(), mutation)
        .expect("fingerprint")
        .expect("durable mutation")
        .into_bytes()
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().fold(String::new(), |mut output, byte| {
        write!(output, "{byte:02x}").expect("write to string");
        output
    })
}

#[test]
fn export_identity_ignores_derived_digests_but_binds_path_and_mode() {
    let export = |digest: u8, path: &str, disposition| ControlMutation::ExportThoughts {
        operation_id: operation(),
        thought_ids: vec![thought()],
        expected_digests: vec![[digest; 32]],
        disposition,
        reference_thought_id: None,
        output_path: path.to_owned(),
    };
    let remove = crate::domain::ExportDisposition::Remove;
    let original = fingerprint(&export(1, "/work/out.txt", remove));
    assert_eq!(
        original,
        fingerprint(&export(2, "/work/out.txt", remove)),
        "a retry after later edits keeps its identity"
    );
    assert_ne!(original, fingerprint(&export(1, "/work/other.txt", remove)));
    let mut replace = export(
        1,
        "/work/out.txt",
        crate::domain::ExportDisposition::ReplaceWithReference,
    );
    if let ControlMutation::ExportThoughts {
        reference_thought_id,
        ..
    } = &mut replace
    {
        *reference_thought_id = Some(thought());
    }
    assert_ne!(original, fingerprint(&replace));
}
