//! Ordered, bounded clipboard effect lane.

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, SyncSender, sync_channel},
    thread::{self, JoinHandle},
};

use crate::{
    adapters::{clipboard::PlatformClipboard, herdr::HerdrGateway, recovery::FileRecoveryExporter},
    application::{ClipboardIntent, Effect},
    domain::RequestId,
    ports::{
        agent::{AgentError, AgentGateway, AgentTarget, SubmissionReceipt, SubmissionRequest},
        clipboard::{Clipboard, ClipboardError, ClipboardWrite},
        recovery::{RecoveryDocument, RecoveryError, RecoveryExporter},
    },
};

use super::TerminalError;

enum ExternalRequest {
    DiscoverAgents,
    SubmitAgent(Box<SubmissionRequest>),
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
    AgentsDiscovered(Result<Vec<AgentTarget>, AgentError>),
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
        result: Result<String, ClipboardError>,
    },
    Exported {
        request_id: RequestId,
        result: Result<PathBuf, RecoveryError>,
    },
}

pub(super) struct ExternalLane {
    sender: Option<SyncSender<ExternalRequest>>,
    pub(super) receiver: Receiver<ExternalResult>,
    handle: Option<JoinHandle<()>>,
}

impl ExternalLane {
    pub(super) fn spawn(recovery_directory: PathBuf) -> Self {
        let (request_sender, request_receiver) = sync_channel(32);
        let (result_sender, result_receiver) = sync_channel(32);
        let handle = thread::spawn(move || {
            external_loop(&request_receiver, &result_sender, recovery_directory);
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
            .send(request)
            .map_err(|_| TerminalError::Worker("external lane disconnected"))?;
        Ok(true)
    }

    pub(super) fn stop(mut self) -> Result<(), TerminalError> {
        drop(self.sender.take());
        match self.handle.take().map(JoinHandle::join) {
            None | Some(Ok(())) => Ok(()),
            Some(Err(_)) => Err(TerminalError::Worker("external lane panicked")),
        }
    }
}

fn external_loop(
    requests: &Receiver<ExternalRequest>,
    results: &SyncSender<ExternalResult>,
    recovery_directory: PathBuf,
) {
    let mut clipboard = PlatformClipboard::new();
    let mut recovery = FileRecoveryExporter::new(recovery_directory);
    let mut agents = HerdrGateway::from_environment();
    while let Ok(request) = requests.recv() {
        let outcome = match request {
            ExternalRequest::DiscoverAgents => ExternalResult::AgentsDiscovered(
                agents
                    .capabilities()
                    .and_then(|capability| agents.adjacent_targets(&capability.context)),
            ),
            ExternalRequest::SubmitAgent(request) => {
                let submission_id = request.submission_id;
                ExternalResult::AgentSubmitted {
                    submission_id,
                    result: Box::new(agents.submit(*request)),
                }
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
                result: clipboard.read(),
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
