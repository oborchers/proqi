//! Verified mutation and update clients over one owner-control endpoint.

use crate::{
    domain::RequestId,
    ports::{
        control::{
            CONTROL_PROTOCOL_VERSION, ControlClient, ControlError, ControlMutation, ControlRequest,
            ControlResult, ControlUpdateReceipt, MIN_CONTROL_PROTOCOL_VERSION,
        },
        environment::IdGenerator,
        runtime::InstanceInfo,
        update::{
            UpdateError, UpdateParticipantGateway, UpdatePrepareReply, UpdatePrepareRequest,
            UpdateRestartReply, UpdateRestartRequest,
        },
    },
};

use super::transport::{connect, read_response, write_request};

/// Verified local durable-mutation client.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalControlClient;

impl ControlClient for LocalControlClient {
    fn send(
        &self,
        owner: &InstanceInfo,
        request: &ControlRequest,
    ) -> Result<crate::ports::control::ControlReceipt, ControlError> {
        match exchange(owner, request)? {
            ControlResult::Accepted(receipt) => Ok(receipt),
            ControlResult::Rejected { code, message } => {
                Err(ControlError::Rejected { code, message })
            }
            ControlResult::Update(_) => Err(ControlError::Protocol(
                "owner returned an update receipt for a durable mutation".to_owned(),
            )),
        }
    }
}

/// Verified update client with an injected transport-request identity source.
pub struct LocalUpdateControlClient<I> {
    ids: I,
}

impl<I> LocalUpdateControlClient<I> {
    /// Bind deterministic request identities to the update transport.
    #[must_use]
    pub const fn new(ids: I) -> Self {
        Self { ids }
    }
}

impl<I: IdGenerator> UpdateParticipantGateway for LocalUpdateControlClient<I> {
    fn prepare(
        &mut self,
        participant: &InstanceInfo,
        request: &UpdatePrepareRequest,
    ) -> Result<UpdatePrepareReply, UpdateError> {
        let result = self.send_update(
            participant,
            ControlMutation::UpdatePrepare {
                request: request.clone(),
            },
        )?;
        match result {
            ControlUpdateReceipt::Prepared(reply) => Ok(reply),
            _ => Err(coordination_error(
                "owner returned the wrong update receipt",
            )),
        }
    }

    fn release(
        &mut self,
        participant: &InstanceInfo,
        operation_id: RequestId,
    ) -> Result<(), UpdateError> {
        let result =
            self.send_update(participant, ControlMutation::UpdateRelease { operation_id })?;
        match result {
            ControlUpdateReceipt::Released { instance_id }
                if instance_id == participant.instance_id =>
            {
                Ok(())
            }
            _ => Err(coordination_error(
                "owner returned the wrong release receipt",
            )),
        }
    }

    fn restart(
        &mut self,
        participant: &InstanceInfo,
        request: &UpdateRestartRequest,
    ) -> Result<UpdateRestartReply, UpdateError> {
        let result = self.send_update(
            participant,
            ControlMutation::UpdateRestart {
                request: request.clone(),
            },
        )?;
        match result {
            ControlUpdateReceipt::Restart(reply) => Ok(reply),
            _ => Err(coordination_error(
                "owner returned the wrong restart receipt",
            )),
        }
    }
}

impl<I: IdGenerator> LocalUpdateControlClient<I> {
    fn send_update(
        &mut self,
        owner: &InstanceInfo,
        mutation: ControlMutation,
    ) -> Result<ControlUpdateReceipt, UpdateError> {
        let request = ControlRequest {
            protocol: CONTROL_PROTOCOL_VERSION,
            request_id: self.ids.request_id(),
            session_id: owner.session_id,
            mutation,
        };
        match exchange(owner, &request).map_err(|error| map_control_error(&error))? {
            ControlResult::Update(receipt) => Ok(receipt),
            ControlResult::Rejected { code, .. } => Err(UpdateError::Coordination(code)),
            ControlResult::Accepted(_) => Err(coordination_error(
                "owner returned a durable mutation receipt",
            )),
        }
    }
}

fn exchange(owner: &InstanceInfo, request: &ControlRequest) -> Result<ControlResult, ControlError> {
    if owner.control_protocol != Some(request.protocol)
        || !(MIN_CONTROL_PROTOCOL_VERSION..=CONTROL_PROTOCOL_VERSION).contains(&request.protocol)
        || request.protocol < request.mutation.minimum_protocol()
        || owner.session_id != request.session_id
    {
        return Err(ControlError::Unsupported);
    }
    let endpoint = owner
        .control_endpoint
        .as_deref()
        .ok_or(ControlError::Unsupported)?;
    let stream = connect(endpoint, owner.pid)?;
    write_request(&stream, request)?;
    let response = read_response(&stream)?;
    if response.protocol != request.protocol || response.request_id != request.request_id {
        return Err(ControlError::Protocol(
            "response version or request identity differs".to_owned(),
        ));
    }
    Ok(response.result)
}

fn map_control_error(error: &ControlError) -> UpdateError {
    UpdateError::Coordination(
        match error {
            ControlError::Unsupported => "unsupported_update_control",
            ControlError::InvalidPeer => "invalid_update_peer",
            ControlError::MessageTooLarge => "update_message_too_large",
            ControlError::Protocol(_) => "update_protocol_mismatch",
            ControlError::Timeout => "update_participant_timeout",
            ControlError::Io(_) => "update_transport_failed",
            ControlError::Rejected { .. } => "update_participant_rejected",
        }
        .to_owned(),
    )
}

fn coordination_error(message: &str) -> UpdateError {
    UpdateError::Coordination(message.to_owned())
}
