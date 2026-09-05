//! Target revalidation and atomic semantic prompt submission.

use crate::ports::{
    agent::{
        AgentError, AgentGateway as _, AgentState, SubmissionReceipt, SubmissionRequest,
        SubmissionRoute,
    },
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
    let require_protocol_match = matches!(request.target.route, SubmissionRoute::HerdrAgent(_));
    let refreshed = match &request.target.route {
        SubmissionRoute::AdjacentPane { source, .. } => gateway.adjacent_targets(source)?,
        SubmissionRoute::HerdrAgent(_) => gateway.global_targets()?,
    };
    let verified = refreshed
        .into_iter()
        .filter(|target| {
            target.identity() == request.target.identity()
                && (!require_protocol_match || target.protocol == request.target.protocol)
        })
        .collect::<Vec<_>>();
    let [target] = verified.as_slice() else {
        return Err(if verified.is_empty() {
            AgentError::Unsupported("target changed before submission".to_owned())
        } else {
            AgentError::Ambiguous("target identity is no longer unique".to_owned())
        });
    };
    if !target.can_submit() {
        return Err(AgentError::Unsupported(format!(
            "target is {} before submission",
            target.availability.as_str()
        )));
    }
    let response: Envelope<PromptBody> = gateway.json(
        &["agent", "prompt", target.pane_id(), &request.content],
        SUBMISSION_TIMEOUT,
    )?;
    let mut accepted_target = target.clone();
    accepted_target.bind_agent_session(verify_prompted(target, &response.result)?);
    Ok(SubmissionReceipt {
        submission_id: request.submission_id,
        target: accepted_target,
        post_state: response_state(response.result.agent.agent_status),
    })
}

fn verify_prompted(
    target: &crate::ports::agent::AgentTarget,
    response: &PromptBody,
) -> Result<crate::ports::agent::AgentSessionBinding, AgentError> {
    if response.kind != "agent_prompted"
        || response.agent.pane_id != target.pane_id()
        || response.agent.workspace_id != target.workspace_id()
        || response.agent.tab_id != target.tab_id()
        || response.agent.agent.as_deref() != Some(target.agent_kind().as_str())
    {
        return Err(AgentError::Malformed(
            "prompt receipt does not match the verified target".to_owned(),
        ));
    }
    super::harness::receipt_session(target, response.agent.agent_session.as_ref())
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
