//! Deterministic in-memory adapters used by fast contract tests.

use std::{collections::VecDeque, path::PathBuf, time::Duration};

use uuid::Uuid;

use crate::{
    domain::{
        InstanceId, OperationId, RequestId, RevisionId, SeparatorId, SessionId, SubmissionId,
        ThoughtId, Timestamp,
    },
    ports::environment::{
        AppPaths, Clock, Environment, IdGenerator, MonotonicClock, PathError, Paths, ProcessError,
        ProcessOutput, ProcessRequest, ProcessRunner,
    },
};

/// Manually controlled clock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FakeClock {
    now: Timestamp,
}

impl FakeClock {
    /// Construct at a known UTC millisecond.
    #[must_use]
    pub const fn new(now: Timestamp) -> Self {
        Self { now }
    }

    /// Replace current time.
    pub const fn set(&mut self, now: Timestamp) {
        self.now = now;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        self.now
    }
}

/// Manually controlled process-relative monotonic clock.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FakeMonotonicClock {
    now: Duration,
}

impl FakeMonotonicClock {
    /// Advance without consulting wall-clock time.
    pub fn advance(&mut self, duration: Duration) {
        self.now = self.now.saturating_add(duration);
    }
}

impl MonotonicClock for FakeMonotonicClock {
    fn now(&self) -> Duration {
        self.now
    }
}

/// Deterministic, unique `UUIDv7` generator.
#[derive(Clone, Debug)]
pub struct FakeIdGenerator {
    timestamp_ms: u64,
    counter: u64,
}

impl FakeIdGenerator {
    /// Construct with a fixed 48-bit `UUIDv7` timestamp.
    #[must_use]
    pub const fn new(timestamp_ms: u64) -> Self {
        Self {
            timestamp_ms,
            counter: 0,
        }
    }

    fn uuid(&mut self) -> Uuid {
        let mut bytes = [0_u8; 16];
        let timestamp = self.timestamp_ms.to_be_bytes();
        bytes[..6].copy_from_slice(&timestamp[2..]);
        bytes[6] = 0x70;
        let counter = self.counter.to_be_bytes();
        bytes[8..].copy_from_slice(&counter);
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let (next_counter, wrapped) = self.counter.overflowing_add(1);
        self.counter = next_counter;
        if wrapped {
            self.timestamp_ms = self.timestamp_ms.wrapping_add(1);
        }
        Uuid::from_bytes(bytes)
    }
}

macro_rules! generate {
    ($self:ident, $type:ty) => {
        loop {
            if let Ok(id) = <$type>::from_uuid($self.uuid()) {
                break id;
            }
        }
    };
}

impl IdGenerator for FakeIdGenerator {
    fn session_id(&mut self) -> SessionId {
        generate!(self, SessionId)
    }

    fn thought_id(&mut self) -> ThoughtId {
        generate!(self, ThoughtId)
    }

    fn separator_id(&mut self) -> SeparatorId {
        generate!(self, SeparatorId)
    }

    fn revision_id(&mut self) -> RevisionId {
        generate!(self, RevisionId)
    }

    fn operation_id(&mut self) -> OperationId {
        generate!(self, OperationId)
    }

    fn instance_id(&mut self) -> InstanceId {
        generate!(self, InstanceId)
    }

    fn request_id(&mut self) -> RequestId {
        generate!(self, RequestId)
    }

    fn submission_id(&mut self) -> SubmissionId {
        generate!(self, SubmissionId)
    }
}

/// Fixed path resolver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FakePaths {
    /// Result returned by every call.
    pub result: Result<AppPaths, PathError>,
}

impl Paths for FakePaths {
    fn resolve(&self) -> Result<AppPaths, PathError> {
        self.result.clone()
    }
}

/// Fixed process environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FakeEnvironment {
    /// Current-directory result returned by every call.
    pub current_directory: Result<PathBuf, PathError>,
    /// Current-executable result returned by every call.
    pub current_executable: Result<PathBuf, PathError>,
    /// Home directory returned by every call.
    pub home_directory: Option<PathBuf>,
}

impl Environment for FakeEnvironment {
    fn current_directory(&self) -> Result<PathBuf, PathError> {
        self.current_directory.clone()
    }

    fn current_executable(&self) -> Result<PathBuf, PathError> {
        self.current_executable.clone()
    }

    fn home_directory(&self) -> Option<PathBuf> {
        self.home_directory.clone()
    }
}

/// Queue-driven process adapter that records exact requests.
#[derive(Clone, Debug, Default)]
pub struct FakeProcessRunner {
    /// Requests observed in order.
    pub requests: Vec<ProcessRequest>,
    /// Results returned in order.
    pub results: VecDeque<Result<ProcessOutput, ProcessError>>,
}

impl ProcessRunner for FakeProcessRunner {
    fn run(&mut self, request: ProcessRequest) -> Result<ProcessOutput, ProcessError> {
        self.requests.push(request);
        self.results
            .pop_front()
            .unwrap_or_else(|| Err(ProcessError::Io("no fake result queued".to_owned())))
    }
}

/// Clipboard that returns one scripted read and records every read.
#[derive(Clone, Debug)]
pub struct ScriptedClipboard {
    /// Result every read returns.
    pub content:
        Result<crate::ports::clipboard::ClipboardContent, crate::ports::clipboard::ClipboardError>,
    /// Plain-text result, when it differs from the text in `content`.
    pub plain_text: Option<Result<Option<String>, crate::ports::clipboard::ClipboardError>>,
    /// Number of reads observed.
    pub reads: usize,
}

impl ScriptedClipboard {
    /// Clipboard holding exact plain text.
    #[must_use]
    pub fn text(text: &str) -> Self {
        Self::with(Ok(crate::ports::clipboard::ClipboardContent::Text(
            crate::ports::clipboard::ClipboardText::plain(text.to_owned()),
        )))
    }

    /// Clipboard returning one fixed result.
    #[must_use]
    pub fn with(
        content: Result<
            crate::ports::clipboard::ClipboardContent,
            crate::ports::clipboard::ClipboardError,
        >,
    ) -> Self {
        Self {
            content,
            plain_text: None,
            reads: 0,
        }
    }

    /// A rich copy: an image flavor beside exact text.
    #[must_use]
    pub fn rich(text: &str) -> Self {
        let image = crate::ports::attachment::RasterImage::new(1, 1, vec![0; 4]);
        Self {
            content: image
                .map(crate::ports::clipboard::ClipboardContent::Image)
                .map_err(|_| crate::ports::clipboard::ClipboardError::InvalidImage),
            plain_text: Some(Ok(Some(text.to_owned()))),
            reads: 0,
        }
    }
}

impl crate::ports::clipboard::Clipboard for ScriptedClipboard {
    fn write(
        &mut self,
        _request_id: RequestId,
        _content: &crate::ports::clipboard::ClipboardText,
    ) -> Result<crate::ports::clipboard::ClipboardWrite, crate::ports::clipboard::ClipboardError>
    {
        Err(crate::ports::clipboard::ClipboardError::Unavailable(
            "scripted clipboard is read-only".to_owned(),
        ))
    }

    fn read(
        &mut self,
    ) -> Result<crate::ports::clipboard::ClipboardContent, crate::ports::clipboard::ClipboardError>
    {
        self.reads += 1;
        self.content.clone()
    }

    fn read_text(&mut self) -> Result<Option<String>, crate::ports::clipboard::ClipboardError> {
        self.reads += 1;
        if let Some(plain) = &self.plain_text {
            return plain.clone();
        }
        match self.content.clone()? {
            crate::ports::clipboard::ClipboardContent::Text(text) => Ok(Some(text.into_parts().0)),
            crate::ports::clipboard::ClipboardContent::Image(_) => Ok(None),
        }
    }
}
