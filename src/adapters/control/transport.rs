//! Bounded framed transport over Unix sockets.

use std::{
    io::{Read, Write},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use interprocess::local_socket::{
    GenericFilePath, ListenerNonblockingMode, ListenerOptions, ToFsName as _,
    prelude::{LocalSocketListener, LocalSocketStream},
    traits::{Listener as _, Stream as _, StreamCommon as _},
};

use crate::ports::control::{
    ControlError, ControlRequest, ControlResponse, MAX_CONTROL_MESSAGE_BYTES,
};

const IO_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) type LocalStream = LocalSocketStream;

pub(super) struct LocalListener {
    inner: LocalSocketListener,
    endpoint: String,
}

impl LocalListener {
    pub(super) fn bind(endpoint: &str) -> Result<Self, ControlError> {
        secure_parent(endpoint)?;
        let name = endpoint.to_fs_name::<GenericFilePath>().map_err(io_error)?;
        let options = ListenerOptions::new()
            .name(name)
            .nonblocking(ListenerNonblockingMode::Accept);
        let inner = options
            .create_sync()
            .map_err(|error| ControlError::Io(format!("bind failed: {error}")))?;
        secure_endpoint(endpoint)
            .map_err(|error| ControlError::Io(format!("endpoint validation failed: {error}")))?;
        Ok(Self {
            inner,
            endpoint: endpoint.to_owned(),
        })
    }
}

impl Drop for LocalListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.endpoint);
    }
}

pub(super) fn accept(listener: &LocalListener) -> Result<Option<LocalStream>, ControlError> {
    match listener.inner.accept() {
        Ok(stream) => {
            validate_client_peer(&stream, listener)?;
            stream.set_nonblocking(true).map_err(io_error)?;
            Ok(Some(stream))
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(error) => Err(io_error(error)),
    }
}

pub(super) fn connect(endpoint: &str, owner_pid: u32) -> Result<LocalStream, ControlError> {
    let name = endpoint.to_fs_name::<GenericFilePath>().map_err(io_error)?;
    let stream = LocalSocketStream::connect(name).map_err(io_error)?;
    validate_server_peer(&stream, endpoint, owner_pid)?;
    stream.set_nonblocking(true).map_err(io_error)?;
    Ok(stream)
}

pub(super) fn read_request(
    stream: &LocalStream,
    stopping: &AtomicBool,
) -> Result<ControlRequest, ControlError> {
    serde_json::from_slice(&read_frame(stream, Some(stopping))?).map_err(|error| {
        ControlError::Protocol(format!("request is not valid protocol JSON: {error}"))
    })
}

pub(super) fn read_response(stream: &LocalStream) -> Result<ControlResponse, ControlError> {
    serde_json::from_slice(&read_frame(stream, None)?).map_err(|error| {
        ControlError::Protocol(format!("response is not valid protocol JSON: {error}"))
    })
}

pub(super) fn read_response_until(
    stream: &LocalStream,
    stopping: &AtomicBool,
) -> Result<ControlResponse, ControlError> {
    serde_json::from_slice(&read_frame(stream, Some(stopping))?).map_err(|error| {
        ControlError::Protocol(format!("response is not valid protocol JSON: {error}"))
    })
}

pub(super) fn write_request(
    stream: &LocalStream,
    request: &ControlRequest,
) -> Result<(), ControlError> {
    write_json(stream, request, None)
}

pub(super) fn write_request_until(
    stream: &LocalStream,
    request: &ControlRequest,
    stopping: &AtomicBool,
) -> Result<(), ControlError> {
    write_json(stream, request, Some(stopping))
}

pub(super) fn write_response(
    stream: &LocalStream,
    response: &ControlResponse,
    stopping: Option<&AtomicBool>,
) -> Result<(), ControlError> {
    write_json(stream, response, stopping)
}

fn write_json(
    stream: &LocalStream,
    value: &impl serde::Serialize,
    stopping: Option<&AtomicBool>,
) -> Result<(), ControlError> {
    let mut bytes =
        serde_json::to_vec(value).map_err(|error| ControlError::Protocol(error.to_string()))?;
    bytes.push(b'\n');
    if bytes.len() > MAX_CONTROL_MESSAGE_BYTES {
        return Err(ControlError::MessageTooLarge);
    }
    write_all(stream, &bytes, stopping)
}

fn read_frame(
    stream: &LocalStream,
    stopping: Option<&AtomicBool>,
) -> Result<Vec<u8>, ControlError> {
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut output = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        ensure_running(stopping)?;
        match (&*stream).read(&mut chunk) {
            Ok(0) => {
                return Err(ControlError::Protocol(
                    "connection closed before frame terminator".to_owned(),
                ));
            }
            Ok(read) => {
                output.extend_from_slice(&chunk[..read]);
                if output.len() > MAX_CONTROL_MESSAGE_BYTES {
                    return Err(ControlError::MessageTooLarge);
                }
                if let Some(end) = output.iter().position(|byte| *byte == b'\n') {
                    output.truncate(end);
                    return Ok(output);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                wait(deadline, stopping)?;
            }
            Err(error) => return Err(io_error(error)),
        }
    }
}

fn write_all(
    stream: &LocalStream,
    bytes: &[u8],
    stopping: Option<&AtomicBool>,
) -> Result<(), ControlError> {
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut written = 0;
    while written < bytes.len() {
        match (&*stream).write(&bytes[written..]) {
            Ok(0) => return Err(ControlError::Io("control connection closed".to_owned())),
            Ok(count) => written = written.saturating_add(count),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                wait(deadline, stopping)?;
            }
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(())
}

fn wait(deadline: Instant, stopping: Option<&AtomicBool>) -> Result<(), ControlError> {
    ensure_running(stopping)?;
    if Instant::now() >= deadline {
        return Err(ControlError::Timeout);
    }
    thread::sleep(Duration::from_millis(2));
    Ok(())
}

fn ensure_running(stopping: Option<&AtomicBool>) -> Result<(), ControlError> {
    if stopping.is_some_and(|flag| flag.load(Ordering::Acquire)) {
        Err(ControlError::Io(
            "control server is shutting down".to_owned(),
        ))
    } else {
        Ok(())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn secure_endpoint(endpoint: &str) -> Result<(), ControlError> {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::set_permissions(endpoint, std::fs::Permissions::from_mode(0o600)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            validate_private_parent(endpoint)
        }
        Err(error) => Err(io_error(error)),
    }
}

#[cfg(target_os = "macos")]
fn secure_endpoint(endpoint: &str) -> Result<(), ControlError> {
    validate_private_parent(endpoint)
}

fn secure_parent(endpoint: &str) -> Result<(), ControlError> {
    let parent = std::path::Path::new(endpoint)
        .parent()
        .ok_or_else(|| ControlError::Io("control endpoint has no parent".to_owned()))?;
    validate_private_directory(parent)
}

fn validate_private_parent(endpoint: &str) -> Result<(), ControlError> {
    let parent = std::path::Path::new(endpoint)
        .parent()
        .ok_or_else(|| ControlError::Io("control endpoint has no parent".to_owned()))?;
    validate_private_directory(parent)
}

fn validate_private_directory(parent: &std::path::Path) -> Result<(), ControlError> {
    use std::os::unix::fs::PermissionsExt as _;
    let metadata = std::fs::symlink_metadata(parent).map_err(io_error)?;
    let valid = metadata.file_type().is_dir()
        && !metadata.file_type().is_symlink()
        && metadata.permissions().mode().trailing_zeros() >= 6;
    valid.then_some(()).ok_or(ControlError::InvalidPeer)
}

fn validate_client_peer(
    stream: &LocalStream,
    listener: &LocalListener,
) -> Result<(), ControlError> {
    use std::os::unix::fs::MetadataExt;
    let owner = std::fs::metadata(&listener.endpoint)
        .map_err(io_error)?
        .uid();
    let peer = stream.peer_creds().map_err(io_error)?.euid();
    (peer == Some(owner))
        .then_some(())
        .ok_or(ControlError::InvalidPeer)
}

fn validate_server_peer(
    stream: &LocalStream,
    endpoint: &str,
    owner_pid: u32,
) -> Result<(), ControlError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(endpoint).map_err(io_error)?;
    let peer = stream.peer_creds().map_err(io_error)?;
    let pid_matches = peer
        .pid()
        .is_none_or(|pid| u32::try_from(pid).ok() == Some(owner_pid));
    (pid_matches && peer.euid() == Some(metadata.uid()))
        .then_some(())
        .ok_or(ControlError::InvalidPeer)
}

fn io_error(error: impl std::fmt::Display) -> ControlError {
    ControlError::Io(error.to_string())
}
