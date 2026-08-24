//! Ordered, bounded clipboard effect lane.

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    adapters::{
        attachment::FileAttachmentStore, clipboard::PlatformClipboard, herdr::HerdrGateway,
        recovery::FileRecoveryExporter,
    },
    application::{ClipboardIntent, Effect},
    domain::RequestId,
    ports::{
        agent::{
            AgentError, AgentGateway, AgentTarget, PanePresentation, SubmissionReceipt,
            SubmissionRequest,
        },
        attachment::AttachmentStore,
        clipboard::{Clipboard, ClipboardContent, ClipboardError, ClipboardWrite},
        recovery::{RecoveryDocument, RecoveryError, RecoveryExporter},
    },
    ui::PastePayload,
};

use super::TerminalError;

enum ExternalRequest {
    DiscoverAgents,
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
}

impl ExternalLane {
    pub(super) fn spawn(
        recovery_directory: PathBuf,
        attachment_directory: PathBuf,
        presentation_source: String,
    ) -> Self {
        let (request_sender, request_receiver) = sync_channel(32);
        let (result_sender, result_receiver) = sync_channel(32);
        let handle = thread::spawn(move || {
            external_loop(
                &request_receiver,
                &result_sender,
                recovery_directory,
                attachment_directory,
                presentation_source,
            );
        });
        Self {
            sender: Some(request_sender),
            receiver: result_receiver,
            handle: Some(handle),
        }
    }

    pub(super) fn send(&self, effect: &Effect) -> Result<bool, TerminalError> {
        let request = match effect {
            Effect::DiscoverAgents => ExternalRequest::DiscoverAgents,
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

    fn send_request(&self, request: ExternalRequest) -> Result<(), TerminalError> {
        self.sender
            .as_ref()
            .ok_or(TerminalError::Worker("external lane is closed"))?
            .try_send(request)
            .map_err(|error| map_send_error(&error))
    }

    pub(super) fn stop(self) -> Result<(), TerminalError> {
        let Self {
            sender,
            receiver,
            mut handle,
        } = self;
        drop(sender);
        if handle.is_some() {
            while receiver.recv().is_ok() {}
        }
        match handle.take().map(JoinHandle::join) {
            None | Some(Ok(())) => Ok(()),
            Some(Err(_)) => Err(TerminalError::Worker("external lane panicked")),
        }
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
) {
    let mut clipboard = PlatformClipboard::new();
    let mut recovery = FileRecoveryExporter::new(recovery_directory);
    let mut attachments = FileAttachmentStore::new(attachment_directory);
    let mut agents = HerdrGateway::from_environment(presentation_source);
    while let Ok(request) = requests.recv() {
        let outcome = match request {
            ExternalRequest::DiscoverAgents => discover_agents(&mut agents),
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
            .map(|path| super::path_import::attachment_payload(path, true))
            .map_err(|_| ExternalReadError::NonUnicodePath),
    }
}

#[cfg(test)]
mod tests;
