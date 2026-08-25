//! Private, bounded, content-redacted file diagnostics.

use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use thiserror::Error;
use tracing::Level;
use tracing_subscriber::util::SubscriberInitExt as _;

const MAX_LOG_BYTES: u64 = 1024 * 1024;
static INITIALIZED: OnceLock<()> = OnceLock::new();

/// File-diagnostics initialization failure.
#[derive(Debug, Error)]
pub enum DiagnosticsError {
    /// A private diagnostics directory or file could not be prepared.
    #[error("diagnostics I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// Another tracing subscriber prevented Proqi from installing its file sink.
    #[error("diagnostics subscriber could not be installed: {0}")]
    Subscriber(String),
}

/// Install one process-wide, content-redacted file subscriber.
///
/// The log contains only events whose call sites explicitly provide safe typed
/// metadata. Thought text, clipboard content, and command arguments are never
/// attached automatically.
///
/// # Errors
///
/// Returns a typed error when the private directory, file, or subscriber cannot
/// be initialized.
pub fn initialize(data_dir: &Path) -> Result<PathBuf, DiagnosticsError> {
    let directory = data_dir.join("diagnostics");
    let path = directory.join("proqi.log");
    if INITIALIZED.get().is_some() {
        return Ok(path);
    }
    create_private_dir(&directory)?;
    let rotate = fs::metadata(&path).is_ok_and(|metadata| metadata.len() >= MAX_LOG_BYTES);
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(!rotate)
        .truncate(rotate)
        .open(&path)?;
    make_private(&path)?;
    tracing_subscriber::fmt()
        .compact()
        .with_max_level(Level::INFO)
        .with_target(true)
        .with_writer(Mutex::new(file))
        .finish()
        .try_init()
        .map_err(|error| DiagnosticsError::Subscriber(error.to_string()))?;
    INITIALIZED
        .set(())
        .map_err(|()| DiagnosticsError::Subscriber("initialization raced".to_owned()))?;
    tracing::info!(
        event = "diagnostics_initialized",
        version = env!("CARGO_PKG_VERSION")
    );
    Ok(path)
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)?;
    make_private(path)
}

fn make_private(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let mode = if path.is_dir() { 0o700 } else { 0o600 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}
