//! Cross-platform local owner-control client and server.

mod transport;

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::ports::{
    control::{
        CONTROL_PROTOCOL_VERSION, ControlClient, ControlError, ControlRequest, ControlResponse,
        ControlResult,
    },
    runtime::InstanceInfo,
};

use transport::{
    LocalListener, accept, connect, read_request, read_response, write_request, write_response,
};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_CACHED_REQUESTS: usize = 64;

/// Verified local control client.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalControlClient;

impl ControlClient for LocalControlClient {
    fn send(
        &self,
        owner: &InstanceInfo,
        request: &ControlRequest,
    ) -> Result<crate::ports::control::ControlReceipt, ControlError> {
        if owner.control_protocol != Some(CONTROL_PROTOCOL_VERSION)
            || request.protocol != CONTROL_PROTOCOL_VERSION
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
        if response.protocol != CONTROL_PROTOCOL_VERSION
            || response.request_id != request.request_id
        {
            return Err(ControlError::Protocol(
                "response version or request identity differs".to_owned(),
            ));
        }
        match response.result {
            ControlResult::Accepted(receipt) => Ok(receipt),
            ControlResult::Rejected { code, message } => {
                Err(ControlError::Rejected { code, message })
            }
        }
    }
}

/// One verified request waiting for the owner reducer.
pub(crate) struct ControlEnvelope {
    pub(crate) request: ControlRequest,
    response: SyncSender<ControlResponse>,
}

impl ControlEnvelope {
    /// Respond exactly once to the waiting transport request.
    pub(crate) fn respond(self, result: ControlResult) {
        let response = ControlResponse {
            protocol: CONTROL_PROTOCOL_VERSION,
            request_id: self.request.request_id,
            result,
        };
        let _sent = self.response.send(response);
    }
}

/// Bounded server lane attached to one active reducer owner.
pub(crate) struct ControlServer {
    pub(crate) receiver: Receiver<ControlEnvelope>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl ControlServer {
    /// Bind the advertised local endpoint before returning.
    pub(crate) fn spawn(endpoint: &str) -> Result<Self, ControlError> {
        let listener = LocalListener::bind(endpoint)?;
        let (sender, receiver) = sync_channel(64);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let handle = thread::spawn(move || server_loop(&listener, &sender, &worker_stop));
        Ok(Self {
            receiver,
            stop,
            handle: Some(handle),
        })
    }

    /// Stop accepting clients and join the server thread.
    pub(crate) fn stop(mut self) -> Result<(), ControlError> {
        self.join()
    }

    fn join(&mut self) -> Result<(), ControlError> {
        self.stop.store(true, Ordering::Release);
        match self.handle.take().map(JoinHandle::join) {
            None | Some(Ok(())) => Ok(()),
            Some(Err(_)) => Err(ControlError::Io("control server panicked".to_owned())),
        }
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        let _joined = self.join();
    }
}

fn server_loop(listener: &LocalListener, sender: &SyncSender<ControlEnvelope>, stop: &AtomicBool) {
    let mut cache = BTreeMap::new();
    let mut cache_order = VecDeque::new();
    while !stop.load(Ordering::Acquire) {
        match accept(listener) {
            Ok(Some(stream)) => handle_stream(&stream, sender, &mut cache, &mut cache_order),
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(_) => thread::sleep(Duration::from_millis(10)),
        }
    }
}

fn handle_stream(
    stream: &transport::LocalStream,
    sender: &SyncSender<ControlEnvelope>,
    cache: &mut BTreeMap<crate::domain::RequestId, (ControlRequest, ControlResponse)>,
    cache_order: &mut VecDeque<crate::domain::RequestId>,
) {
    let Ok(request) = read_request(stream) else {
        return;
    };
    if let Some((original, response)) = cache.get(&request.request_id) {
        let response = if original == &request {
            response.clone()
        } else {
            rejected(
                &request,
                "request_id_conflict",
                "request identity was reused",
            )
        };
        let _written = write_response(stream, &response);
        return;
    }
    if request.protocol != CONTROL_PROTOCOL_VERSION {
        let response = rejected(
            &request,
            "protocol_mismatch",
            "unsupported control protocol",
        );
        let _written = write_response(stream, &response);
        return;
    }
    let (response_sender, response_receiver) = sync_channel(1);
    let envelope = ControlEnvelope {
        request: request.clone(),
        response: response_sender,
    };
    let response = if sender.try_send(envelope).is_err() {
        rejected(&request, "owner_busy", "owner control lane is full")
    } else {
        response_receiver
            .recv_timeout(RESPONSE_TIMEOUT)
            .unwrap_or_else(|_| {
                rejected(
                    &request,
                    "owner_timeout",
                    "owner did not complete the request",
                )
            })
    };
    cache_response(cache, cache_order, request, response.clone());
    let _written = write_response(stream, &response);
}

fn cache_response(
    cache: &mut BTreeMap<crate::domain::RequestId, (ControlRequest, ControlResponse)>,
    order: &mut VecDeque<crate::domain::RequestId>,
    request: ControlRequest,
    response: ControlResponse,
) {
    while order.len() >= MAX_CACHED_REQUESTS {
        if let Some(expired) = order.pop_front() {
            cache.remove(&expired);
        }
    }
    order.push_back(request.request_id);
    cache.insert(request.request_id, (request, response));
}

fn rejected(request: &ControlRequest, code: &str, message: &str) -> ControlResponse {
    ControlResponse {
        protocol: CONTROL_PROTOCOL_VERSION,
        request_id: request.request_id,
        result: ControlResult::Rejected {
            code: code.to_owned(),
            message: message.to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, time::Duration};

    use crate::{
        adapters::memory::FakeIdGenerator,
        domain::{OperationSequence, Timestamp},
        ports::{
            control::{
                CONTROL_PROTOCOL_VERSION, ControlClient, ControlError, ControlMutation,
                ControlReceipt, ControlRequest, ControlResult, MAX_CONTROL_MESSAGE_BYTES,
            },
            environment::IdGenerator,
            runtime::InstanceInfo,
            store::{CommitReceipt, DurableIdentity},
        },
    };

    use super::{
        ControlServer, LocalControlClient,
        transport::{connect, read_response, write_request},
    };

    #[test]
    fn verified_request_is_durable_and_request_replay_is_cached() {
        let temporary = tempfile::tempdir().expect("temporary endpoint");
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let request = request(&mut ids, "exact");
        let endpoint = endpoint(temporary.path(), request.request_id.to_string().as_str());
        let server = ControlServer::spawn(&endpoint).expect("control server");
        let owner = owner(&mut ids, request.session_id, endpoint);
        let expected = receipt(&request);
        std::thread::scope(|scope| {
            let client_owner = owner.clone();
            let client_request = request.clone();
            let client =
                scope.spawn(move || LocalControlClient.send(&client_owner, &client_request));
            let envelope = server
                .receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("owner request");
            envelope.respond(ControlResult::Accepted(expected));
            let first = client
                .join()
                .expect("client thread")
                .expect("first receipt");
            let replay = LocalControlClient
                .send(&owner, &request)
                .expect("cached receipt");
            assert_eq!(first, expected);
            assert_eq!(replay, expected);
        });
        server.stop().expect("server stop");
    }

    #[test]
    fn request_identity_reuse_and_wrong_peer_are_rejected() {
        let temporary = tempfile::tempdir().expect("temporary endpoint");
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let request = request(&mut ids, "original");
        let endpoint = endpoint(temporary.path(), request.request_id.to_string().as_str());
        let server = ControlServer::spawn(&endpoint).expect("control server");
        let owner = owner(&mut ids, request.session_id, endpoint);
        std::thread::scope(|scope| {
            let client_owner = owner.clone();
            let client_request = request.clone();
            let client =
                scope.spawn(move || LocalControlClient.send(&client_owner, &client_request));
            let envelope = server
                .receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("owner request");
            envelope.respond(ControlResult::Accepted(receipt(&request)));
            client
                .join()
                .expect("client thread")
                .expect("first receipt");
        });
        let mut changed = request.clone();
        let ControlMutation::Add { content, .. } = &mut changed.mutation else {
            panic!("add request fixture");
        };
        *content = "changed".to_owned();
        assert!(matches!(
            LocalControlClient.send(&owner, &changed),
            Err(ControlError::Rejected { code, .. }) if code == "request_id_conflict"
        ));
        let wrong_owner = InstanceInfo {
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            pid: owner.pid.saturating_add(1),
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            control_protocol: None,
            ..owner
        };
        let error = LocalControlClient.send(&wrong_owner, &request);
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        assert!(matches!(error, Err(ControlError::InvalidPeer)));
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        assert!(matches!(error, Err(ControlError::Unsupported)));
        server.stop().expect("server stop");
    }

    #[test]
    fn protocol_boundary_rejects_wrong_identifier_prefixes() {
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let request = request(&mut ids, "body");
        let mut json = serde_json::to_value(&request).expect("request JSON");
        json["session_id"] = serde_json::json!(ids.thought_id().to_string());
        assert!(serde_json::from_value::<ControlRequest>(json).is_err());
    }

    #[test]
    fn server_negotiates_protocol_and_bounds_encoded_messages() {
        let temporary = tempfile::tempdir().expect("temporary endpoint");
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let mut request = request(&mut ids, "body");
        let endpoint = endpoint(temporary.path(), request.request_id.to_string().as_str());
        let server = ControlServer::spawn(&endpoint).expect("control server");
        let owner = owner(&mut ids, request.session_id, endpoint.clone());

        request.protocol = CONTROL_PROTOCOL_VERSION + 1;
        let stream = connect(&endpoint, owner.pid).expect("protocol stream");
        write_request(&stream, &request).expect("protocol request");
        let response = read_response(&stream).expect("protocol response");
        assert!(matches!(
            response.result,
            ControlResult::Rejected { code, .. } if code == "protocol_mismatch"
        ));

        request.protocol = CONTROL_PROTOCOL_VERSION;
        let ControlMutation::Add { content, .. } = &mut request.mutation else {
            panic!("add request fixture");
        };
        *content = "x".repeat(MAX_CONTROL_MESSAGE_BYTES);
        assert!(matches!(
            LocalControlClient.send(&owner, &request),
            Err(ControlError::MessageTooLarge)
        ));
        server.stop().expect("server stop");
    }

    #[test]
    fn owner_response_timeout_is_bounded_and_typed() {
        let temporary = tempfile::tempdir().expect("temporary endpoint");
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let request = request(&mut ids, "body");
        let endpoint = endpoint(temporary.path(), request.request_id.to_string().as_str());
        let server = ControlServer::spawn(&endpoint).expect("control server");
        let owner = owner(&mut ids, request.session_id, endpoint);
        std::thread::scope(|scope| {
            let client_owner = owner.clone();
            let client_request = request.clone();
            let client =
                scope.spawn(move || LocalControlClient.send(&client_owner, &client_request));
            let envelope = server
                .receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("owner request");
            let error = client
                .join()
                .expect("client thread")
                .expect_err("owner must time out");
            assert!(matches!(
                error,
                ControlError::Rejected { code, .. } if code == "owner_timeout"
            ));
            drop(envelope);
        });
        server.stop().expect("server stop");
    }

    #[cfg(unix)]
    #[test]
    fn unix_endpoint_is_user_only() {
        use std::os::unix::fs::PermissionsExt as _;

        let temporary = tempfile::tempdir().expect("temporary endpoint");
        let path = temporary.path().join("control.sock");
        let server = ControlServer::spawn(path.to_str().expect("UTF-8 path")).expect("server");
        let endpoint_mode = std::fs::metadata(&path)
            .expect("endpoint metadata")
            .permissions()
            .mode()
            & 0o777;
        let parent_mode = std::fs::metadata(path.parent().expect("endpoint parent"))
            .expect("parent metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(parent_mode, 0o700);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(endpoint_mode, 0o600);
        #[cfg(target_os = "macos")]
        assert_eq!(endpoint_mode & 0o022, 0);
        server.stop().expect("server stop");
    }

    fn request(ids: &mut FakeIdGenerator, content: &str) -> ControlRequest {
        ControlRequest {
            protocol: CONTROL_PROTOCOL_VERSION,
            request_id: ids.request_id(),
            session_id: ids.session_id(),
            mutation: ControlMutation::Add {
                operation_id: ids.operation_id(),
                thought_id: ids.thought_id(),
                content: content.to_owned(),
                position: None,
            },
        }
    }

    fn owner(
        ids: &mut FakeIdGenerator,
        session_id: crate::domain::SessionId,
        endpoint: String,
    ) -> InstanceInfo {
        InstanceInfo {
            instance_id: ids.instance_id(),
            session_id,
            pid: std::process::id(),
            version: "test".to_owned(),
            storage_protocol: 1,
            control_protocol: Some(CONTROL_PROTOCOL_VERSION),
            control_endpoint: Some(endpoint),
            launch_directory: "/tmp/proqi-control".to_owned(),
            started_at: Timestamp::from_millis(1),
        }
    }

    fn receipt(request: &ControlRequest) -> ControlReceipt {
        ControlReceipt {
            thought_id: request.mutation.thought_id(),
            durable: CommitReceipt {
                session_id: request.session_id,
                sequence: OperationSequence::new(1),
                identity: DurableIdentity::Operation(request.mutation.operation_id()),
                idempotent_replay: false,
            },
        }
    }

    #[cfg(unix)]
    fn endpoint(directory: &Path, suffix: &str) -> String {
        directory
            .join(format!("{suffix}.sock"))
            .to_string_lossy()
            .into_owned()
    }

    #[cfg(windows)]
    fn endpoint(_directory: &Path, suffix: &str) -> String {
        format!(r"\\.\pipe\proqi-test-{suffix}")
    }
}
