#![cfg(unix)]
//! End-to-end Herdr 19 and 20 adapter contracts through real fake executables.

use proqi::{
    adapters::{herdr::HerdrGateway, memory::FakeIdGenerator, process::SystemProcessRunner},
    ports::{
        agent::{AgentGateway, SubmissionRequest},
        environment::IdGenerator,
        invocation::InvocationReferenceCatalog,
    },
};

#[path = "support/herdr.rs"]
mod herdr_fixture;

#[test]
fn recorded_fake_executables_prove_both_direct_semantic_cli_contracts() {
    for protocol in [19, 20] {
        prove_protocol(protocol);
    }
}

fn prove_protocol(protocol: u32) {
    let fixture = herdr_fixture::HerdrFixture::new(protocol);
    let mut gateway = HerdrGateway::new(fixture.program(), SystemProcessRunner::default(), true);
    let references = gateway.discover_live_references().expect("live references");
    let [reference] = references.references.as_slice() else {
        panic!("expected one live reference");
    };
    assert_eq!(reference.agent_name(), Some("fixture"));
    assert_eq!(reference.workspace_label(), Some("Fixture workspace"));
    assert_eq!(reference.tab_label(), Some("Fixture tab"));
    assert_eq!(reference.pane_id(), "w1:p2");
    let repeated_references = gateway
        .discover_live_references()
        .expect("repeated live references");
    assert_eq!(repeated_references, references);

    let capabilities = gateway.capabilities().expect("capabilities");
    assert_eq!(capabilities.protocol, protocol);
    let targets = gateway
        .adjacent_targets(&capabilities.context)
        .expect("verified targets");
    let [target] = targets.as_slice() else {
        panic!("expected one verified target");
    };
    assert_eq!(target.protocol, protocol);
    assert_eq!(
        gateway
            .adjacent_targets(&capabilities.context)
            .expect("repeated verified targets"),
        targets
    );

    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let exact = "$(touch never); Grüße\n第二行";
    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: target.clone(),
            content: exact.to_owned(),
        })
        .expect("accepted prompt");

    assert_eq!(receipt.target, *target);
    assert_eq!(fixture.prompt_bytes().as_deref(), Some(exact.as_bytes()));
}
