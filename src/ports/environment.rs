//! Deterministic boundaries for time, identity, paths, and child processes.

use std::{ffi::OsString, path::PathBuf, time::Duration};

use thiserror::Error;

use crate::domain::{
    InstanceId, OperationId, RequestId, RevisionId, SessionId, SubmissionId, ThoughtId, Timestamp,
};

/// Source of UTC domain time.
pub trait Clock {
    /// Current UTC time.
    fn now(&self) -> Timestamp;
}

/// Source of strongly typed `UUIDv7` identities.
pub trait IdGenerator {
    /// Generate a session identity.
    fn session_id(&mut self) -> SessionId;
    /// Generate a thought identity.
    fn thought_id(&mut self) -> ThoughtId;
    /// Generate a revision identity.
    fn revision_id(&mut self) -> RevisionId;
    /// Generate a durable operation identity.
    fn operation_id(&mut self) -> OperationId;
    /// Generate a running-instance identity.
    fn instance_id(&mut self) -> InstanceId;
    /// Generate an idempotent control-request identity.
    fn request_id(&mut self) -> RequestId;
    /// Generate a Proqi submission identity.
    fn submission_id(&mut self) -> SubmissionId;
}

/// Platform-native application locations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppPaths {
    /// Durable databases and backups.
    pub data_dir: PathBuf,
    /// User-editable configuration.
    pub config_dir: PathBuf,
    /// Rebuildable private caches shared by one installation.
    pub cache_dir: PathBuf,
    /// Short-lived locks, sockets, and instance metadata.
    pub runtime_dir: PathBuf,
}

/// Resolver for platform-native paths.
pub trait Paths {
    /// Resolve all application paths without creating them.
    ///
    /// # Errors
    ///
    /// Returns a typed error when required platform directories are unavailable or invalid.
    fn resolve(&self) -> Result<AppPaths, PathError>;
}

/// Process environment values required by application composition.
pub trait Environment {
    /// Resolve the absolute current working directory.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the directory cannot be resolved.
    fn current_directory(&self) -> Result<PathBuf, PathError>;
}

/// Path resolution failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PathError {
    /// The operating system did not expose a required user directory.
    #[error("platform user directory is unavailable: {0}")]
    Unavailable(&'static str),
    /// A resolved path was not absolute.
    #[error("resolved application path is not absolute: {0}")]
    Relative(PathBuf),
}

/// Direct child-process request. No shell string is accepted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessRequest {
    /// Executable path or operating-system program name.
    pub program: OsString,
    /// Distinct, uninterpreted arguments.
    pub args: Vec<OsString>,
    /// Optional exact standard-input bytes.
    pub stdin: Option<Vec<u8>>,
    /// Hard execution deadline.
    pub timeout: Duration,
}

/// Captured child-process result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessOutput {
    /// Portable exit code when the process exited normally.
    pub exit_code: Option<i32>,
    /// Exact standard output bytes.
    pub stdout: Vec<u8>,
    /// Exact standard error bytes.
    pub stderr: Vec<u8>,
}

/// Child-process execution failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProcessError {
    /// Process could not be started or observed.
    #[error("process I/O failed: {0}")]
    Io(String),
    /// Deadline elapsed and the child was terminated.
    #[error("process timed out")]
    TimedOut,
    /// Captured output exceeded an adapter limit.
    #[error("process output exceeded the configured limit")]
    OutputLimit,
}

/// Executes one child directly without shell interpolation.
pub trait ProcessRunner {
    /// Run one bounded process request.
    ///
    /// # Errors
    ///
    /// Returns a typed I/O, timeout, or output-limit failure.
    fn run(&mut self, request: ProcessRequest) -> Result<ProcessOutput, ProcessError>;
}
