//! Ordered, bounded clipboard effect lane.

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel},
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    adapters::{
        attachment::FileAttachmentStore,
        clipboard::PlatformClipboard,
        herdr::{HerdrGateway, HerdrPauseNotifier},
        invocation::FilesystemInvocationCatalog,
        process::{CancellationFlag, SystemProcessRunner},
        recovery::FileRecoveryExporter,
    },
    application::{ClipboardIntent, Effect, ScreenshotPauseReason},
    domain::RequestId,
    ports::{
        agent::{
            AgentError, AgentGateway, AgentTarget, PanePresentation, SubmissionReceipt,
            SubmissionRequest,
        },
        attachment::AttachmentStore,
        clipboard::{Clipboard, ClipboardContent, ClipboardError, ClipboardWrite},
        invocation::{
            AdditionalInvocationRoot, InvocationCatalog, InvocationCatalogError,
            InvocationDiscovery, InvocationDiscoveryRequest, InvocationReferenceCatalog,
            InvocationReferenceDiscovery, InvocationReferenceDiscoveryRequest,
        },
        recovery::{RecoveryDocument, RecoveryError, RecoveryExporter},
    },
    ui::PastePayload,
};

use super::{
    TerminalError,
    supervisor::{ShutdownDeadline, WorkerLifecycle, join_before},
};

enum ExternalRequest {
    DiscoverAgents,
    DiscoverInvocations(InvocationDiscoveryRequest),
    DiscoverInvocationReferences(InvocationReferenceDiscoveryRequest),
    SubmitAgent(Box<SubmissionRequest>),
    PublishPane {
        pane_id: String,
        sequence: u64,
        ttl: Duration,
    },
    ClearPane {
        pane_id: String,
        sequence: u64,
    },
    NotifyScreenshotPause(ScreenshotPauseReason),
    Write {
        request_id: RequestId,
        intent: ClipboardIntent,
        content: String,
    },
    Read {
        request_id: RequestId,
    },
    Export {
        request_id: RequestId,
        document: Box<RecoveryDocument>,
    },
}

pub(super) enum ExternalResult {
    AgentsDiscovered {
        pane_id: Option<String>,
        result: Result<Vec<AgentTarget>, AgentError>,
    },
    InvocationsDiscovered(Result<InvocationDiscovery, InvocationCatalogError>),
    InvocationReferencesDiscovered(InvocationReferenceDiscovery),
    AgentSubmitted {
        submission_id: crate::domain::SubmissionId,
        result: Box<Result<SubmissionReceipt, AgentError>>,
    },
    Written {
        request_id: RequestId,
        intent: ClipboardIntent,
        result: Result<ClipboardWrite, ClipboardError>,
    },
    Read {
        request_id: RequestId,
        result: Result<PastePayload, ExternalReadError>,
    },
    Exported {
        request_id: RequestId,
        result: Result<PathBuf, RecoveryError>,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum ExternalReadError {
    /// Native clipboard content was unavailable or invalid.
    Clipboard,
    /// A raw image could not be written durably.
    Attachment,
    /// The durable path cannot be represented in Proqi's UTF-8 text model.
    NonUnicodePath,
}

pub(super) struct ExternalLane {
    sender: Option<SyncSender<ExternalRequest>>,
    pub(super) receiver: Receiver<ExternalResult>,
    handle: Option<JoinHandle<()>>,
    lifecycle: WorkerLifecycle,
}

impl ExternalLane {
    pub(super) fn spawn_with_invocation_roots(
        recovery_directory: PathBuf,
        attachment_directory: PathBuf,
        presentation_source: String,
        cancellation: CancellationFlag,
        invocation_roots: Vec<AdditionalInvocationRoot>,
    ) -> Self {
        let (request_sender, request_receiver) = sync_channel(32);
        let (result_sender, result_receiver) = sync_channel(32);
        let lifecycle = WorkerLifecycle::default();
        let worker_lifecycle = lifecycle.clone();
        let handle = thread::spawn(move || {
            worker_lifecycle.run(super::supervisor::WorkerRole::External, || {
                external_loop(
                    &request_receiver,
                    &result_sender,
                    recovery_directory,
                    attachment_directory,
                    presentation_source,
                    cancellation,
                    invocation_roots,
                );
            });
        });
        Self {
            sender: Some(request_sender),
            receiver: result_receiver,
            handle: Some(handle),
            lifecycle,
        }
    }

    pub(super) fn send(&self, effect: &Effect) -> Result<bool, TerminalError> {
        let request = match effect {
            Effect::DiscoverAgents => ExternalRequest::DiscoverAgents,
            Effect::DiscoverInvocations(request) => {
                ExternalRequest::DiscoverInvocations(request.clone())
            }
            Effect::DiscoverInvocationReferences(request) => {
                ExternalRequest::DiscoverInvocationReferences(*request)
            }
            Effect::SubmitAgent(request) => ExternalRequest::SubmitAgent(Box::new(request.clone())),
            Effect::WriteClipboard {
                request_id,
                intent,
                content,
                ..
            } => ExternalRequest::Write {
                request_id: *request_id,
                intent: *intent,
                content: content.clone(),
            },
            Effect::ReadClipboard { request_id } => ExternalRequest::Read {
                request_id: *request_id,
            },
            Effect::ExportRecovery {
                request_id,
                document,
            } => ExternalRequest::Export {
                request_id: *request_id,
                document: document.clone(),
            },
            _ => return Ok(false),
        };
        self.sender
            .as_ref()
            .ok_or(TerminalError::Worker("external lane is closed"))?
            .try_send(request)
            .map_err(|error| map_send_error(&error))?;
        Ok(true)
    }

    pub(super) fn publish_pane(
        &self,
        pane_id: &str,
        sequence: u64,
        ttl: Duration,
    ) -> Result<(), TerminalError> {
        self.send_request(ExternalRequest::PublishPane {
            pane_id: pane_id.to_owned(),
            sequence,
            ttl,
        })
    }

    pub(super) fn clear_pane(&self, pane_id: &str, sequence: u64) -> Result<(), TerminalError> {
        self.send_request(ExternalRequest::ClearPane {
            pane_id: pane_id.to_owned(),
            sequence,
        })
    }

    pub(super) fn notify_screenshot_pause(
        &self,
        reason: ScreenshotPauseReason,
    ) -> Result<(), TerminalError> {
        self.send_request(ExternalRequest::NotifyScreenshotPause(reason))
    }

    fn send_request(&self, request: ExternalRequest) -> Result<(), TerminalError> {
        self.sender
            .as_ref()
            .ok_or(TerminalError::Worker("external lane is closed"))?
            .try_send(request)
            .map_err(|error| map_send_error(&error))
    }

    pub(super) fn request_stop(&mut self) {
        self.lifecycle.request_stop();
        self.sender = None;
    }

    pub(super) fn worker_failure(&self) -> Option<TerminalError> {
        self.lifecycle
            .failure(super::supervisor::WorkerRole::External)
    }

    pub(super) fn stopped_cleanly(&self) -> bool {
        self.lifecycle.stopped_cleanly()
    }

    pub(super) fn stop(mut self, deadline: ShutdownDeadline) -> Result<(), TerminalError> {
        self.request_stop();
        while !deadline.expired() {
            match self.receiver.recv_timeout(deadline.remaining()) {
                Ok(_) => {}
                Err(RecvTimeoutError::Disconnected | RecvTimeoutError::Timeout) => break,
            }
        }
        join_before(
            self.handle.take(),
            deadline,
            "external lane panicked",
            "external lane did not stop before the shutdown deadline",
        )
    }
}

impl Drop for ExternalLane {
    fn drop(&mut self) {
        self.request_stop();
    }
}

fn map_send_error(error: &TrySendError<ExternalRequest>) -> TerminalError {
    match error {
        TrySendError::Full(_) => TerminalError::Worker("external lane is full"),
        TrySendError::Disconnected(_) => TerminalError::Worker("external lane disconnected"),
    }
}

fn external_loop(
    requests: &Receiver<ExternalRequest>,
    results: &SyncSender<ExternalResult>,
    recovery_directory: PathBuf,
    attachment_directory: PathBuf,
    presentation_source: String,
    cancellation: CancellationFlag,
    invocation_roots: Vec<AdditionalInvocationRoot>,
) {
    let mut clipboard = PlatformClipboard::new();
    let mut recovery = FileRecoveryExporter::new(recovery_directory);
    let mut attachments = FileAttachmentStore::new(attachment_directory);
    let runner = SystemProcessRunner::cancellable(cancellation);
    let mut notifications = HerdrPauseNotifier::from_environment_with_runner(runner.clone());
    let mut agents = HerdrGateway::from_environment_with_runner(presentation_source, runner);
    let mut invocations = FilesystemInvocationCatalog::system(invocation_roots);
    while let Ok(request) = requests.recv() {
        let outcome = match request {
            ExternalRequest::DiscoverAgents => discover_agents(&mut agents),
            ExternalRequest::DiscoverInvocations(request) => {
                discover_invocations(&mut invocations, request)
            }
            ExternalRequest::DiscoverInvocationReferences(request) => {
                discover_invocation_references(&mut agents, request)
            }
            ExternalRequest::SubmitAgent(request) => {
                let submission_id = request.submission_id;
                ExternalResult::AgentSubmitted {
                    submission_id,
                    result: Box::new(agents.submit(*request)),
                }
            }
            ExternalRequest::PublishPane {
                pane_id,
                sequence,
                ttl,
            } => {
                let _published = agents.publish(&pane_id, sequence, ttl);
                continue;
            }
            ExternalRequest::ClearPane { pane_id, sequence } => {
                let _cleared = agents.clear(&pane_id, sequence);
                continue;
            }
            ExternalRequest::NotifyScreenshotPause(reason) => {
                let _notified = notifications.notify(reason);
                continue;
            }
            ExternalRequest::Write {
                request_id,
                intent,
                content,
            } => ExternalResult::Written {
                request_id,
                intent,
                result: clipboard.write(&content),
            },
            ExternalRequest::Read { request_id } => ExternalResult::Read {
                request_id,
                result: read_clipboard(&mut clipboard, &mut attachments, request_id),
            },
            ExternalRequest::Export {
                request_id,
                document,
            } => ExternalResult::Exported {
                request_id,
                result: recovery.export(request_id, &document),
            },
        };
        if results.send(outcome).is_err() {
            return;
        }
    }
}

fn discover_invocations(
    invocations: &mut impl InvocationCatalog,
    request: InvocationDiscoveryRequest,
) -> ExternalResult {
    ExternalResult::InvocationsDiscovered(invocations.discover(request))
}

fn discover_invocation_references(
    references: &mut impl InvocationReferenceCatalog,
    request: InvocationReferenceDiscoveryRequest,
) -> ExternalResult {
    ExternalResult::InvocationReferencesDiscovered(InvocationReferenceDiscovery {
        generation: request.generation,
        references: references.discover_live_references(),
    })
}

fn discover_agents(agents: &mut impl AgentGateway) -> ExternalResult {
    match agents.capabilities() {
        Ok(capability) => {
            let pane_id = Some(capability.context.pane_id.clone());
            let result = agents.adjacent_targets(&capability.context);
            ExternalResult::AgentsDiscovered { pane_id, result }
        }
        Err(error) => ExternalResult::AgentsDiscovered {
            pane_id: None,
            result: Err(error),
        },
    }
}

fn read_clipboard(
    clipboard: &mut impl Clipboard,
    attachments: &mut impl AttachmentStore,
    request_id: RequestId,
) -> Result<PastePayload, ExternalReadError> {
    match clipboard.read().map_err(|_| ExternalReadError::Clipboard)? {
        ClipboardContent::Text(content) => {
            Ok(super::path_import::annotate_existing_files(&content)
                .unwrap_or_else(|| PastePayload::text(content)))
        }
        ClipboardContent::Image(image) => attachments
            .save_clipboard_image(request_id, &image)
            .map_err(|_| ExternalReadError::Attachment)?
            .into_os_string()
            .into_string()
            .map(|path| {
                super::path_import::attachment_payload(path, true).with_verified_attachments()
            })
            .map_err(|_| ExternalReadError::NonUnicodePath),
    }
}

#[cfg(test)]
mod tests;
