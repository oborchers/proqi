//! Cross-platform local owner-control client and server.

mod client;
#[cfg(all(test, unix))]
mod protocol_tests;
mod transport;

pub use client::{LocalControlClient, LocalUpdateControlClient};

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

use crate::ports::control::{
    CONTROL_PROTOCOL_VERSION, ControlError, ControlRequest, ControlResponse, ControlResult,
    MIN_CONTROL_PROTOCOL_VERSION,
};

use transport::{LocalListener, accept, read_request, write_response};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_CACHED_REQUESTS: usize = 64;

/// One verified request waiting for the owner reducer.
pub(crate) struct ControlEnvelope {
    pub(crate) request: ControlRequest,
    response: SyncSender<ControlResponse>,
}

impl ControlEnvelope {
    /// Respond exactly once to the waiting transport request.
    pub(crate) fn respond(self, result: ControlResult) {
        let response = ControlResponse {
            protocol: self.request.protocol,
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
    if !(MIN_CONTROL_PROTOCOL_VERSION..=CONTROL_PROTOCOL_VERSION).contains(&request.protocol) {
        let response = rejected(
            &request,
            "protocol_mismatch",
            "unsupported control protocol",
        );
        let _written = write_response(stream, &response);
        return;
    }
    if request.protocol < request.mutation.minimum_protocol() {
        let response = rejected(
            &request,
            "protocol_mismatch",
            "presentation annotations require control protocol 2",
        );
        let _written = write_response(stream, &response);
        return;
    }
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
                    "outcome_unknown",
                    "owner did not answer before the deadline; retry with the same operation id",
                )
            })
    };
    if matches!(
        response.result,
        ControlResult::Accepted(_) | ControlResult::Update(_)
    ) {
        cache_response(cache, cache_order, request, response.clone());
    }
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
        protocol: request.protocol,
        request_id: request.request_id,
        result: ControlResult::Rejected {
            code: code.to_owned(),
            message: message.to_owned(),
        },
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::{path::Path, time::Duration};

    use crate::{
        adapters::memory::FakeIdGenerator,
        domain::{OperationSequence, Timestamp},
        ports::{
            control::{
                CONTROL_PROTOCOL_VERSION, ControlClient, ControlError, ControlMutation,
                ControlReceipt, ControlRequest, ControlResult,
            },
            environment::IdGenerator,
            runtime::InstanceInfo,
            store::{CommitReceipt, DurableIdentity},
        },
    };

    use super::{ControlServer, LocalControlClient};

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
    fn owner_response_timeout_is_bounded_indeterminate_and_not_cached() {
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
                ControlError::Rejected { code, .. } if code == "outcome_unknown"
            ));
            drop(envelope);

            let retry_owner = owner.clone();
            let retry_request = request.clone();
            let retry = scope.spawn(move || LocalControlClient.send(&retry_owner, &retry_request));
            let retry_envelope = server
                .receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("retry reaches owner");
            retry_envelope.respond(ControlResult::Accepted(receipt(&request)));
            retry.join().expect("retry thread").expect("retry receipt");
        });
        server.stop().expect("server stop");
    }

    #[cfg(unix)]
    #[test]
    fn unix_endpoint_is_user_only() {
        use std::os::unix::fs::PermissionsExt as _;

        let temporary = tempfile::tempdir().expect("temporary endpoint");
        let parent = temporary.path().join("control");
        private_directory(&parent);
        let path = parent.join("control.sock");
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
        assert!(!path.exists());
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
                annotations: Vec::new(),
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
            update: None,
            launch_directory: std::env::temp_dir()
                .join("proqi-control")
                .to_string_lossy()
                .into_owned(),
            started_at: Timestamp::from_millis(1),
        }
    }

    fn receipt(request: &ControlRequest) -> ControlReceipt {
        ControlReceipt {
            thought_id: request.mutation.thought_id(),
            durable: CommitReceipt {
                session_id: request.session_id,
                sequence: OperationSequence::new(1),
                identity: DurableIdentity::Operation(
                    request
                        .mutation
                        .durable_operation_id()
                        .expect("durable test request"),
                ),
                idempotent_replay: false,
            },
        }
    }

    #[cfg(unix)]
    fn endpoint(directory: &Path, suffix: &str) -> String {
        let private = directory.join("control");
        private_directory(&private);
        private
            .join(format!("{suffix}.sock"))
            .to_string_lossy()
            .into_owned()
    }

    #[cfg(unix)]
    fn private_directory(path: &Path) {
        use std::os::unix::fs::DirBuilderExt as _;
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        builder.create(path).expect("private endpoint parent");
    }

    #[cfg(windows)]
    fn endpoint(_directory: &Path, suffix: &str) -> String {
        format!(r"\\.\pipe\proqi-test-{suffix}")
    }
}

#[cfg(all(test, windows))]
mod windows_tests;
