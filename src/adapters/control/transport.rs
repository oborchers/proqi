//! Bounded framed transport over Unix sockets or Windows named pipes.

use std::{
    io::{Read, Write},
    thread,
    time::{Duration, Instant},
};

use interprocess::local_socket::{
    GenericFilePath, ListenerNonblockingMode, ListenerOptions, ToFsName as _,
    prelude::{LocalSocketListener, LocalSocketStream},
    traits::{Listener as _, Stream as _, StreamCommon as _},
};

#[cfg(windows)]
use interprocess::os::windows::{
    local_socket::ListenerOptionsExt as _, security_descriptor::SecurityDescriptor,
};
#[cfg(windows)]
use widestring::U16CString;

use crate::ports::control::{
    ControlError, ControlRequest, ControlResponse, MAX_CONTROL_MESSAGE_BYTES,
};

const IO_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) type LocalStream = LocalSocketStream;

pub(super) struct LocalListener {
    inner: LocalSocketListener,
    #[cfg(unix)]
    endpoint: String,
}

impl LocalListener {
    pub(super) fn bind(endpoint: &str) -> Result<Self, ControlError> {
        #[cfg(unix)]
        secure_parent(endpoint)?;
        let name = endpoint.to_fs_name::<GenericFilePath>().map_err(io_error)?;
        let options = ListenerOptions::new()
            .name(name)
            .nonblocking(ListenerNonblockingMode::Accept);
        #[cfg(windows)]
        let options = options.security_descriptor(windows_user_only_descriptor()?);
        let inner = options.create_sync().map_err(io_error)?;
        #[cfg(unix)]
        secure_endpoint(endpoint)?;
        Ok(Self {
            inner,
            #[cfg(unix)]
            endpoint: endpoint.to_owned(),
        })
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

pub(super) fn read_request(stream: &LocalStream) -> Result<ControlRequest, ControlError> {
    serde_json::from_slice(&read_frame(stream)?).map_err(|error| {
        ControlError::Protocol(format!("request is not valid protocol JSON: {error}"))
    })
}

pub(super) fn read_response(stream: &LocalStream) -> Result<ControlResponse, ControlError> {
    serde_json::from_slice(&read_frame(stream)?).map_err(|error| {
        ControlError::Protocol(format!("response is not valid protocol JSON: {error}"))
    })
}

pub(super) fn write_request(
    stream: &LocalStream,
    request: &ControlRequest,
) -> Result<(), ControlError> {
    write_json(stream, request)
}

pub(super) fn write_response(
    stream: &LocalStream,
    response: &ControlResponse,
) -> Result<(), ControlError> {
    write_json(stream, response)
}

fn write_json(stream: &LocalStream, value: &impl serde::Serialize) -> Result<(), ControlError> {
    let mut bytes =
        serde_json::to_vec(value).map_err(|error| ControlError::Protocol(error.to_string()))?;
    bytes.push(b'\n');
    if bytes.len() > MAX_CONTROL_MESSAGE_BYTES {
        return Err(ControlError::MessageTooLarge);
    }
    write_all(stream, &bytes)
}

fn read_frame(stream: &LocalStream) -> Result<Vec<u8>, ControlError> {
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut output = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
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
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => wait(deadline)?,
            Err(error) => return Err(io_error(error)),
        }
    }
}

fn write_all(stream: &LocalStream, bytes: &[u8]) -> Result<(), ControlError> {
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut written = 0;
    while written < bytes.len() {
        match (&*stream).write(&bytes[written..]) {
            Ok(0) => return Err(ControlError::Io("control connection closed".to_owned())),
            Ok(count) => written = written.saturating_add(count),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => wait(deadline)?,
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(())
}

fn wait(deadline: Instant) -> Result<(), ControlError> {
    if Instant::now() >= deadline {
        return Err(ControlError::Timeout);
    }
    thread::sleep(Duration::from_millis(2));
    Ok(())
}

#[cfg(unix)]
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

#[cfg(unix)]
fn secure_parent(endpoint: &str) -> Result<(), ControlError> {
    use std::os::unix::fs::PermissionsExt;
    let parent = std::path::Path::new(endpoint)
        .parent()
        .ok_or_else(|| ControlError::Io("control endpoint has no parent".to_owned()))?;
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).map_err(io_error)
}

#[cfg(unix)]
fn validate_private_parent(endpoint: &str) -> Result<(), ControlError> {
    use std::os::unix::fs::PermissionsExt;
    let parent = std::path::Path::new(endpoint)
        .parent()
        .ok_or_else(|| ControlError::Io("control endpoint has no parent".to_owned()))?;
    let mode = std::fs::metadata(parent)
        .map_err(io_error)?
        .permissions()
        .mode()
        & 0o077;
    (mode == 0).then_some(()).ok_or(ControlError::InvalidPeer)
}

#[cfg(unix)]
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

#[cfg(windows)]
fn validate_client_peer(
    stream: &LocalStream,
    _listener: &LocalListener,
) -> Result<(), ControlError> {
    stream
        .peer_creds()
        .map_err(io_error)?
        .pid()
        .map(|_| ())
        .ok_or(ControlError::InvalidPeer)
}

#[cfg(unix)]
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

#[cfg(windows)]
fn validate_server_peer(
    stream: &LocalStream,
    _endpoint: &str,
    owner_pid: u32,
) -> Result<(), ControlError> {
    let peer = stream.peer_creds().map_err(io_error)?;
    (peer.pid().and_then(|pid| u32::try_from(pid).ok()) == Some(owner_pid))
        .then_some(())
        .ok_or(ControlError::InvalidPeer)
}

fn io_error(error: impl std::fmt::Display) -> ControlError {
    ControlError::Io(error.to_string())
}

#[cfg(windows)]
fn windows_user_only_descriptor() -> Result<SecurityDescriptor, ControlError> {
    let sddl = U16CString::from_str("D:P(A;;GA;;;OW)").map_err(io_error)?;
    SecurityDescriptor::deserialize(sddl.as_ucstr()).map_err(io_error)
}
