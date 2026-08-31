use std::{path::PathBuf, sync::mpsc::sync_channel, time::Duration};

use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{AppState, Effect, ScreenshotPauseReason},
    domain::{RequestId, Session, SessionBoard, Timestamp},
    ports::{
        agent::{AgentFailureCode, AgentState, HarnessKind},
        attachment::{AttachmentError, AttachmentStore, RasterImage},
        clipboard::{Clipboard, ClipboardContent, ClipboardError, ClipboardWrite},
        environment::IdGenerator as _,
        invocation::{
            InvocationCatalog, InvocationCatalogError, InvocationDiscovery,
            InvocationDiscoveryRequest, InvocationReferenceCatalog, InvocationReferenceProvider,
            LiveAgentReference,
        },
    },
    ui::{BoardApp, UiInput, UiKey},
};

use super::{
    ExternalLane, ExternalReadError, ExternalRequest, ExternalResult, discover_invocations,
    read_clipboard,
};

struct FakeClipboard(Result<ClipboardContent, ClipboardError>);

impl Clipboard for FakeClipboard {
    fn write(&mut self, _content: &str) -> Result<ClipboardWrite, ClipboardError> {
        Err(ClipboardError::Unavailable("unused".to_owned()))
    }

    fn read(&mut self) -> Result<ClipboardContent, ClipboardError> {
        self.0.clone()
    }
}

#[derive(Default)]
struct FakeAttachments {
    saved: Option<(RequestId, RasterImage)>,
    result: Option<Result<PathBuf, AttachmentError>>,
}

impl AttachmentStore for FakeAttachments {
    fn save_clipboard_image(
        &mut self,
        request_id: RequestId,
        image: &RasterImage,
    ) -> Result<PathBuf, AttachmentError> {
        self.saved = Some((request_id, image.clone()));
        self.result
            .take()
            .ok_or_else(|| AttachmentError::Io("no fake result".to_owned()))?
    }
}

#[test]
fn image_read_materializes_exact_pixels_before_returning_a_path() {
    let image = RasterImage::new(1, 1, vec![1, 2, 3, 255]).expect("image");
    let mut clipboard = FakeClipboard(Ok(ClipboardContent::Image(image.clone())));
    let path = PathBuf::from("/private/proqi/clipboard.png");
    let mut attachments = FakeAttachments {
        saved: None,
        result: Some(Ok(path.clone())),
    };
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let request = ids.request_id();

    assert_eq!(
        read_clipboard(&mut clipboard, &mut attachments, request),
        Ok(super::super::path_import::attachment_payload(
            path.to_string_lossy().into_owned(),
            true,
        )
        .with_verified_attachments())
    );
    assert_eq!(attachments.saved, Some((request, image)));
}

#[test]
fn materialized_clipboard_image_is_accessible_before_its_immediate_recheck() {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let clock = FakeClock::new(Timestamp::from_millis(2));
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("clipboard-proof"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let board = SessionBoard::new(session, Vec::new()).expect("board");
    let mut app = BoardApp::new(AppState::new(board), RopeEditorFactory);
    let read = app.handle(UiInput::Key(UiKey::PasteClipboard), &mut ids, &clock);
    let request_id = read
        .iter()
        .find_map(|effect| match effect {
            Effect::ReadClipboard { request_id } => Some(*request_id),
            _ => None,
        })
        .expect("clipboard read request");
    let image = RasterImage::new(1, 1, vec![1, 2, 3, 255]).expect("image");
    let mut clipboard = FakeClipboard(Ok(ClipboardContent::Image(image)));
    let mut attachments = FakeAttachments {
        saved: None,
        result: Some(Ok(PathBuf::from("/private/proqi/clipboard-proof.png"))),
    };
    let payload = read_clipboard(&mut clipboard, &mut attachments, request_id)
        .expect("materialized image payload");
    let effects = app.complete_clipboard_read_payload(request_id, Ok(payload), &mut ids, &clock);
    let thought_id = app.state.focused_thought.expect("created thought");
    assert!(!app.state.attachments.inaccessible(thought_id, 0));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::CheckAttachments(_)))
    );
}

#[test]
fn attachment_failure_returns_no_insertable_path() {
    let image = RasterImage::new(1, 1, vec![1, 2, 3, 255]).expect("image");
    let mut clipboard = FakeClipboard(Ok(ClipboardContent::Image(image)));
    let mut attachments = FakeAttachments {
        saved: None,
        result: Some(Err(AttachmentError::Io("disk full".to_owned()))),
    };
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);

    assert!(matches!(
        read_clipboard(&mut clipboard, &mut attachments, ids.request_id()),
        Err(ExternalReadError::Attachment)
    ));
}

#[test]
fn full_and_disconnected_request_lanes_fail_without_blocking() {
    let (request_sender, request_receiver) = sync_channel(1);
    let (_result_sender, result_receiver) = sync_channel(1);
    request_sender
        .try_send(ExternalRequest::DiscoverAgents)
        .expect("fill request lane");
    let lane = ExternalLane {
        sender: Some(request_sender),
        receiver: result_receiver,
        handle: None,
        lifecycle: super::super::supervisor::WorkerLifecycle::default(),
    };
    assert!(matches!(
        lane.send(&crate::application::Effect::DiscoverAgents),
        Err(super::TerminalError::Worker("external lane is full"))
    ));
    assert!(matches!(
        lane.notify_screenshot_pause(ScreenshotPauseReason::CaptureLimit { captures: 10 }),
        Err(super::TerminalError::Worker("external lane is full"))
    ));
    drop(request_receiver);
    assert!(matches!(
        lane.send(&crate::application::Effect::DiscoverAgents),
        Err(super::TerminalError::Worker("external lane disconnected"))
    ));
    assert!(matches!(
        lane.notify_screenshot_pause(ScreenshotPauseReason::Inactivity { minutes: 20 }),
        Err(super::TerminalError::Worker("external lane disconnected"))
    ));
    lane.stop(super::super::supervisor::ShutdownDeadline::after(
        Duration::from_secs(1),
    ))
    .expect("stop detached lane");
}

struct FakeInvocations;

impl InvocationCatalog for FakeInvocations {
    fn discover(
        &mut self,
        request: InvocationDiscoveryRequest,
    ) -> Result<InvocationDiscovery, InvocationCatalogError> {
        Ok(InvocationDiscovery {
            generation: request.generation,
            cwd: request.cwd,
            global: Vec::new(),
            project: Vec::new(),
            live: Vec::new(),
        })
    }
}

struct FakeReferences(Result<Vec<LiveAgentReference>, AgentFailureCode>);

impl InvocationReferenceCatalog for FakeReferences {
    fn discover_live_references(&mut self) -> Result<Vec<LiveAgentReference>, AgentFailureCode> {
        self.0.clone()
    }
}

fn reference() -> LiveAgentReference {
    LiveAgentReference::new(
        InvocationReferenceProvider::Herdr,
        Some("reviewer".to_owned()),
        HarnessKind::new("codex").expect("harness"),
        "w1".to_owned(),
        Some("Workspace".to_owned()),
        "w1:t1".to_owned(),
        Some("Tab".to_owned()),
        "w1:p2".to_owned(),
        AgentState::Idle,
    )
    .expect("reference")
}

#[test]
fn live_reference_failure_never_breaks_filesystem_invocation_refresh() {
    let request = InvocationDiscoveryRequest {
        generation: 7,
        cwd: PathBuf::from("/fixture"),
    };
    let mut invocations = FakeInvocations;
    let mut references = FakeReferences(Err(AgentFailureCode::TimedOut));

    let ExternalResult::InvocationsDiscovered(Ok(discovery)) =
        discover_invocations(&mut invocations, &mut references, request)
    else {
        panic!("invocation discovery result");
    };
    assert_eq!(discovery.generation, 7);
    assert!(discovery.live.is_empty());
}

#[test]
fn live_references_join_the_same_generation_tagged_discovery_result() {
    let request = InvocationDiscoveryRequest {
        generation: 9,
        cwd: PathBuf::from("/fixture"),
    };
    let mut invocations = FakeInvocations;
    let mut references = FakeReferences(Ok(vec![reference()]));

    let ExternalResult::InvocationsDiscovered(Ok(discovery)) =
        discover_invocations(&mut invocations, &mut references, request)
    else {
        panic!("invocation discovery result");
    };
    assert_eq!(discovery.generation, 9);
    assert_eq!(discovery.live, vec![reference()]);
}
