//! Optional thought names across the live destination-owner transfer path.

use proqi::adapters::runtime::SystemIdGenerator;
use proqi::ports::environment::IdGenerator;

use super::{active_transfer::TransferFixture, support::json_command};

#[test]
fn named_transfer_into_active_owner_survives_replay_and_restart() {
    let fixture = TransferFixture::new("Named transfer image.png");
    let _renamed = json_command(
        fixture.binary,
        fixture.state(),
        &[
            "thoughts",
            "rename",
            &fixture.source,
            &fixture.source_thought,
            "Agent delivery contract",
        ],
    );
    let (owner, done) = fixture.start_owner("named-copy");
    let operation = SystemIdGenerator.operation_id().to_string();

    let first = fixture.send(&operation);
    assert!(
        first.status.success(),
        "named active transfer failed: {}",
        String::from_utf8_lossy(&first.stdout)
    );
    let first: serde_json::Value = serde_json::from_slice(&first.stdout).expect("transfer JSON");
    let destination_thought = first["data"]["destination_thought_id"]
        .as_str()
        .expect("destination thought ID")
        .to_owned();
    let replay = fixture.send(&operation);
    assert!(replay.status.success(), "named transfer replay failed");
    let replay: serde_json::Value = serde_json::from_slice(&replay.stdout).expect("replay JSON");
    assert_eq!(
        replay["data"]["destination_receipt"]["idempotent_replay"],
        true
    );

    fixture.finish_owner(owner, &done);
    let inspected = json_command(
        fixture.binary,
        fixture.state(),
        &[
            "thoughts",
            "inspect",
            &fixture.destination,
            &destination_thought,
        ],
    );
    assert_eq!(
        inspected["data"]["thought"]["name"],
        "Agent delivery contract"
    );
    assert_eq!(
        inspected["data"]["thought"]["content"],
        fixture.image.to_string_lossy().as_ref()
    );
}
