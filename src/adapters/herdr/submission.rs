//! Target revalidation and atomic semantic prompt submission.

use crate::ports::{
    agent::{AgentError, AgentGateway as _, AgentState, SubmissionReceipt, SubmissionRequest},
    environment::ProcessRunner,
};

use super::{
    HerdrGateway, SUBMISSION_TIMEOUT,
    contract::{Envelope, PromptBody, RawReadiness},
};

pub(super) fn submit<R: ProcessRunner>(
    gateway: &mut HerdrGateway<R>,
    request: &SubmissionRequest,
) -> Result<SubmissionReceipt, AgentError> {
    let verified = gateway
        .adjacent_targets(&request.target.source)?
        .into_iter()
        .filter(|target| {
            target.direction == request.target.direction
                && target.pane_id == request.target.pane_id
                && target.agent_kind == request.target.agent_kind
                && target.agent_session_id == request.target.agent_session_id
        })
        .collect::<Vec<_>>();
    let [target] = verified.as_slice() else {
        return Err(if verified.is_empty() {
            AgentError::Unsupported("target changed before submission".to_owned())
        } else {
            AgentError::Ambiguous("target identity is no longer unique".to_owned())
        });
    };
    let response: Envelope<PromptBody> = gateway.json(
        &["agent", "prompt", &target.pane_id, &request.content],
        SUBMISSION_TIMEOUT,
    )?;
    verify_prompted(target, &response.result)?;
    Ok(SubmissionReceipt {
        submission_id: request.submission_id,
        target: target.clone(),
        post_state: response_state(response.result.agent.agent_status),
    })
}

fn verify_prompted(
    target: &crate::ports::agent::AgentTarget,
    response: &PromptBody,
) -> Result<(), AgentError> {
    let session = response.agent.agent_session.as_ref();
    if response.kind != "agent_prompted"
        || response.agent.pane_id != target.pane_id
        || response.agent.workspace_id != target.workspace_id
        || response.agent.tab_id != target.tab_id
        || response.agent.agent.as_deref() != Some(&target.agent_kind)
        || session.map(|value| value.value.as_str()) != Some(&target.agent_session_id)
    {
        return Err(AgentError::Malformed(
            "prompt receipt does not match the verified target".to_owned(),
        ));
    }
    Ok(())
}

const fn response_state(value: Option<RawReadiness>) -> Option<AgentState> {
    match value {
        Some(RawReadiness::Idle) => Some(AgentState::Idle),
        Some(RawReadiness::Working) => Some(AgentState::Working),
        Some(RawReadiness::Done) => Some(AgentState::Done),
        Some(RawReadiness::Blocked) => Some(AgentState::Blocked),
        Some(RawReadiness::Unknown) => Some(AgentState::Unknown),
        None => None,
    }
}
