use serde_json::json;

use crate::{
    adapters::memory::FakeIdGenerator,
    ports::{
        agent::{AgentGateway, AgentState, SubmissionRequest},
        environment::IdGenerator,
    },
};

use super::{agent, discovery_responses, gateway, right_rect, source, success, target};

#[test]
fn matching_prompt_receipt_accepts_every_advisory_post_state() {
    for (status, expected) in [
        (Some("blocked"), Some(AgentState::Blocked)),
        (Some("unknown"), Some(AgentState::Unknown)),
        (None, None),
    ] {
        let context = source();
        let agents = vec![agent("w1:p2", "w1", "w1:t1", "idle")];
        let mut responses =
            discovery_responses(&context, json!(agents), Some(("w1:p2", right_rect())));
        let mut prompted = agent("w1:p2", "w1", "w1:t1", status.unwrap_or("idle"));
        if status.is_none() {
            prompted
                .as_object_mut()
                .expect("agent object")
                .remove("agent_status");
        }
        responses.push(success(json!({"result":{
            "type":"agent_prompted",
            "agent":prompted
        }})));
        let (mut gateway, _) = gateway(responses);
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let receipt = gateway
            .submit(SubmissionRequest {
                submission_id: ids.submission_id(),
                target: target(&context),
                content: "accepted exact thought".to_owned(),
            })
            .expect("matching agent_prompted receipt is acceptance");
        assert_eq!(receipt.post_state, expected);
    }
}
