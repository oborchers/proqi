//! Verified mutation and update clients over one owner-control endpoint.

use crate::{
    adapters::process::CancellationFlag,
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

use super::transport::{
    connect, read_response, read_response_until, write_request, write_request_until,
};

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
            ControlResult::Metadata(_) => Err(ControlError::Protocol(
                "owner returned a metadata receipt for a durable mutation".to_owned(),
            )),
            ControlResult::Capture(_) => Err(ControlError::Protocol(
                "owner returned a capture receipt for a durable mutation".to_owned(),
            )),
        }
    }
}

/// Runtime-owned mutation client that stops waiting when Proqi shuts down.
pub(crate) struct CancellableLocalControlClient {
    cancellation: CancellationFlag,
}

impl CancellableLocalControlClient {
    pub(crate) const fn new(cancellation: CancellationFlag) -> Self {
        Self { cancellation }
    }

    pub(crate) fn request_capture_takeover(
        &self,
        owner: &crate::ports::runtime::CaptureOwnerInfo,
        requester_instance_id: crate::domain::InstanceId,
        request_id: crate::domain::RequestId,
    ) -> Result<(), ControlError> {
        let (instance, request) =
            capture_takeover_request(owner, requester_instance_id, request_id);
        capture_takeover_result(
            owner,
            exchange_until(&instance, &request, &self.cancellation)?,
        )
    }
}

impl ControlClient for CancellableLocalControlClient {
    fn send(
        &self,
        owner: &InstanceInfo,
        request: &ControlRequest,
    ) -> Result<crate::ports::control::ControlReceipt, ControlError> {
        match exchange_until(owner, request, &self.cancellation)? {
            ControlResult::Accepted(receipt) => Ok(receipt),
            ControlResult::Rejected { code, message } => {
                Err(ControlError::Rejected { code, message })
            }
            ControlResult::Update(_) => Err(ControlError::Protocol(
                "owner returned an update receipt for a durable mutation".to_owned(),
            )),
            ControlResult::Metadata(_) => Err(ControlError::Protocol(
                "owner returned a metadata receipt for a durable mutation".to_owned(),
            )),
            ControlResult::Capture(_) => Err(ControlError::Protocol(
                "owner returned a capture receipt for a durable mutation".to_owned(),
            )),
        }
    }
}

/// Verified update client with an injected transport-request identity source.
pub struct LocalUpdateControlClient<I> {
    ids: I,
    cancellation: Option<CancellationFlag>,
}

impl<I> LocalUpdateControlClient<I> {
    /// Bind deterministic request identities to the update transport.
    #[must_use]
    pub const fn new(ids: I) -> Self {
        Self {
            ids,
            cancellation: None,
        }
    }

    pub(crate) const fn cancellable(ids: I, cancellation: CancellationFlag) -> Self {
        Self {
            ids,
            cancellation: Some(cancellation),
        }
    }
}

impl LocalControlClient {
    pub(crate) fn send_metadata(
        owner: &InstanceInfo,
        request: &ControlRequest,
    ) -> Result<crate::ports::control::ControlMetadataReceipt, ControlError> {
        match exchange(owner, request)? {
            ControlResult::Metadata(receipt) => Ok(receipt),
            ControlResult::Rejected { code, message } => {
                Err(ControlError::Rejected { code, message })
            }
            ControlResult::Accepted(_) | ControlResult::Update(_) | ControlResult::Capture(_) => {
                Err(ControlError::Protocol(
                    "owner returned the wrong receipt for a metadata mutation".to_owned(),
                ))
            }
        }
    }
}

fn capture_takeover_request(
    owner: &crate::ports::runtime::CaptureOwnerInfo,
    requester_instance_id: crate::domain::InstanceId,
    request_id: crate::domain::RequestId,
) -> (InstanceInfo, ControlRequest) {
    let instance = InstanceInfo {
        instance_id: owner.instance_id,
        session_id: owner.session_id,
        pid: owner.pid,
        version: owner.version.clone(),
        storage_protocol: 0,
        control_protocol: Some(owner.control_protocol),
        control_endpoint: Some(owner.control_endpoint.clone()),
        update: None,
        launch_directory: String::new(),
        started_at: owner.started_at,
    };
    let request = ControlRequest {
        protocol: CONTROL_PROTOCOL_VERSION,
        request_id,
        session_id: owner.session_id,
        mutation: ControlMutation::CaptureTakeover {
            expected_owner_instance_id: owner.instance_id,
            requester_instance_id,
            capture_protocol: crate::ports::control::CAPTURE_CONTROL_PROTOCOL_VERSION,
        },
    };
    (instance, request)
}

fn capture_takeover_result(
    owner: &crate::ports::runtime::CaptureOwnerInfo,
    result: ControlResult,
) -> Result<(), ControlError> {
    match result {
        ControlResult::Capture(
            crate::ports::control::ControlCaptureReceipt::TakeoverScheduled { owner_instance_id },
        ) if owner_instance_id == owner.instance_id => Ok(()),
        ControlResult::Rejected { code, message } => Err(ControlError::Rejected { code, message }),
        _ => Err(ControlError::Protocol(
            "owner returned the wrong screenshot takeover receipt".to_owned(),
        )),
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
        let exchange = self.cancellation.as_ref().map_or_else(
            || exchange(owner, &request),
            |cancellation| exchange_until(owner, &request, cancellation),
        );
        match exchange.map_err(|error| map_control_error(&error))? {
            ControlResult::Update(receipt) => Ok(receipt),
            ControlResult::Rejected { code, .. } => Err(UpdateError::Coordination(code)),
            ControlResult::Accepted(_) => Err(coordination_error(
                "owner returned a durable mutation receipt",
            )),
            ControlResult::Metadata(_) => {
                Err(coordination_error("owner returned a metadata receipt"))
            }
            ControlResult::Capture(_) => {
                Err(coordination_error("owner returned a capture receipt"))
            }
        }
    }
}

fn exchange_until(
    owner: &InstanceInfo,
    request: &ControlRequest,
    cancellation: &CancellationFlag,
) -> Result<ControlResult, ControlError> {
    validate_exchange(owner, request)?;
    if cancellation.is_cancelled() {
        return Err(ControlError::Io("control request was cancelled".to_owned()));
    }
    let endpoint = owner
        .control_endpoint
        .as_deref()
        .ok_or(ControlError::Unsupported)?;
    let stream = connect(endpoint, owner.pid)?;
    write_request_until(&stream, request, cancellation.signal())?;
    let response = read_response_until(&stream, cancellation.signal())?;
    validate_response(request, response)
}

fn exchange(owner: &InstanceInfo, request: &ControlRequest) -> Result<ControlResult, ControlError> {
    validate_exchange(owner, request)?;
    let endpoint = owner
        .control_endpoint
        .as_deref()
        .ok_or(ControlError::Unsupported)?;
    let stream = connect(endpoint, owner.pid)?;
    write_request(&stream, request)?;
    let response = read_response(&stream)?;
    validate_response(request, response)
}

fn validate_exchange(owner: &InstanceInfo, request: &ControlRequest) -> Result<(), ControlError> {
    if owner.control_protocol != Some(request.protocol)
        || !(MIN_CONTROL_PROTOCOL_VERSION..=CONTROL_PROTOCOL_VERSION).contains(&request.protocol)
        || request.protocol < request.mutation.minimum_protocol()
        || owner.session_id != request.session_id
    {
        return Err(ControlError::Unsupported);
    }
    Ok(())
}

fn validate_response(
    request: &ControlRequest,
    response: crate::ports::control::ControlResponse,
) -> Result<ControlResult, ControlError> {
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
