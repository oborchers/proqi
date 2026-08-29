//! Terminal-independent screenshot inbox capability and validation values.

use std::{fmt, path::PathBuf, sync::Arc, time::Duration};

use thiserror::Error;

/// Maximum number of configurable filename fallback patterns.
pub const MAX_FILENAME_PATTERNS: usize = 32;
/// Maximum length of one fallback pattern in Unicode scalar values.
pub const MAX_FILENAME_PATTERN_CHARS: usize = 160;
/// Maximum entries inspected by one bounded non-recursive reconciliation.
pub const MAX_RECONCILIATION_ENTRIES: usize = 10_000;
/// Default inactivity safety bound for one listening lease.
pub const DEFAULT_INACTIVITY_TIMEOUT_MINUTES: u16 = 20;
/// Default unattended capture safety bound for one listening lease.
pub const DEFAULT_MAX_UNATTENDED_CAPTURES: u16 = 10;
/// Largest configurable inactivity safety bound.
pub const MAX_INACTIVITY_TIMEOUT_MINUTES: u16 = 1_440;
/// Largest configurable unattended capture safety bound.
pub const MAX_UNATTENDED_CAPTURES: u16 = 100;

/// Validated safety policy for one explicitly enabled capture lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScreenshotActivityPolicy {
    inactivity_timeout_minutes: u16,
    max_unattended_captures: u16,
}

impl Default for ScreenshotActivityPolicy {
    fn default() -> Self {
        Self {
            inactivity_timeout_minutes: DEFAULT_INACTIVITY_TIMEOUT_MINUTES,
            max_unattended_captures: DEFAULT_MAX_UNATTENDED_CAPTURES,
        }
    }
}

impl ScreenshotActivityPolicy {
    /// Construct an always-enabled, conservatively bounded policy.
    #[must_use]
    pub const fn new(
        inactivity_timeout_minutes: u16,
        max_unattended_captures: u16,
    ) -> Option<Self> {
        if inactivity_timeout_minutes == 0
            || inactivity_timeout_minutes > MAX_INACTIVITY_TIMEOUT_MINUTES
            || max_unattended_captures == 0
            || max_unattended_captures > MAX_UNATTENDED_CAPTURES
        {
            None
        } else {
            Some(Self {
                inactivity_timeout_minutes,
                max_unattended_captures,
            })
        }
    }

    /// Full unchanged interval required before automatic pause.
    #[must_use]
    pub const fn inactivity_timeout(self) -> Duration {
        Duration::from_secs(self.inactivity_timeout_minutes as u64 * 60)
    }

    /// Configured whole-minute value for truthful presentation.
    #[must_use]
    pub const fn inactivity_timeout_minutes(self) -> u16 {
        self.inactivity_timeout_minutes
    }

    /// Maximum candidates admitted without deliberate interaction.
    #[must_use]
    pub const fn max_unattended_captures(self) -> u16 {
        self.max_unattended_captures
    }
}

/// Image types accepted by the first screenshot inbox.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ScreenshotImageType {
    /// Portable Network Graphics.
    Png,
    /// JPEG image.
    Jpeg,
    /// Tagged Image File Format.
    Tiff,
}

impl ScreenshotImageType {
    /// Parse one stable lowercase configuration spelling.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "png" => Some(Self::Png),
            "jpeg" | "jpg" => Some(Self::Jpeg),
            "tiff" | "tif" => Some(Self::Tiff),
            _ => None,
        }
    }

    /// Stable configuration spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Tiff => "tiff",
        }
    }
}

/// Conservative byte and pixel bounds for accepted screenshots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScreenshotBounds {
    /// Smallest accepted file size.
    pub min_file_bytes: u64,
    /// Largest accepted file size.
    pub max_file_bytes: u64,
    /// Largest accepted width or height.
    pub max_dimension: u32,
    /// Largest accepted total pixel count.
    pub max_pixels: u64,
}

impl Default for ScreenshotBounds {
    fn default() -> Self {
        Self {
            min_file_bytes: 64,
            max_file_bytes: 64 * 1024 * 1024,
            max_dimension: 16_384,
            max_pixels: 100_000_000,
        }
    }
}

/// Validated screenshot inbox settings supplied to the watcher adapter.
#[derive(Clone, Eq, PartialEq)]
pub struct ScreenshotInboxConfig {
    /// Exact absolute directory watched without recursive traversal.
    pub directory: PathBuf,
    /// User-supplied glob-like filename fallbacks.
    pub filename_patterns: Vec<String>,
    /// Explicit opt-in to accept every otherwise valid new image.
    pub capture_all_new_images: bool,
    /// Supported magic-validated image types.
    pub supported_types: Vec<ScreenshotImageType>,
    /// File and image geometry limits.
    pub bounds: ScreenshotBounds,
    /// Delay between stability observations.
    pub debounce: Duration,
}

impl fmt::Debug for ScreenshotInboxConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScreenshotInboxConfig")
            .field("directory", &"<redacted>")
            .field("filename_patterns", &self.filename_patterns.len())
            .field("capture_all_new_images", &self.capture_all_new_images)
            .field("supported_types", &self.supported_types)
            .field("bounds", &self.bounds)
            .field("debounce", &self.debounce)
            .finish()
    }
}

impl ScreenshotInboxConfig {
    /// Validate bounded configuration before accessing the directory.
    ///
    /// # Errors
    ///
    /// Returns a stable configuration failure without exposing the directory.
    pub fn validate(&self) -> Result<(), ScreenshotError> {
        if !self.directory.is_absolute() {
            return Err(ScreenshotError::InvalidConfig(
                "screenshot inbox directory must be absolute",
            ));
        }
        if self.filename_patterns.len() > MAX_FILENAME_PATTERNS
            || self.filename_patterns.iter().any(|pattern| {
                pattern.trim().is_empty()
                    || pattern.chars().count() > MAX_FILENAME_PATTERN_CHARS
                    || pattern.chars().any(char::is_control)
            })
        {
            return Err(ScreenshotError::InvalidConfig(
                "screenshot filename patterns are invalid or exceed their bounds",
            ));
        }
        if self.supported_types.is_empty()
            || self.bounds.min_file_bytes == 0
            || self.bounds.min_file_bytes > self.bounds.max_file_bytes
            || self.bounds.max_file_bytes > 512 * 1024 * 1024
            || self.bounds.max_dimension == 0
            || self.bounds.max_dimension > 65_535
            || self.bounds.max_pixels == 0
            || self.bounds.max_pixels > 1_000_000_000
            || !(Duration::from_millis(100)..=Duration::from_secs(1)).contains(&self.debounce)
        {
            return Err(ScreenshotError::InvalidConfig(
                "screenshot size, type, or debounce bounds are invalid",
            ));
        }
        Ok(())
    }
}

/// Rename-stable, content-redacted source identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ScreenshotFingerprint(pub [u8; 32]);

/// One completed screenshot candidate in first-observed detection order.
#[derive(Clone, Eq, PartialEq)]
pub struct ScreenshotCandidate {
    /// Stable source identity used for durable deduplication.
    pub fingerprint: ScreenshotFingerprint,
    /// Exact absolute path stored as thought content.
    pub path: PathBuf,
    /// Magic-validated type.
    pub image_type: ScreenshotImageType,
}

/// One active bounded screenshot directory reconciliation worker.
pub trait ActiveScreenshotWatcher: Send {
    /// Wait for one bounded interval and return newly stable candidates.
    ///
    /// # Errors
    ///
    /// Returns a typed permission, watcher, or reconciliation failure.
    fn poll(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError>;
    /// Reconcile without an unbounded wait before ownership release.
    ///
    /// # Errors
    ///
    /// Returns a typed permission or reconciliation failure.
    fn final_reconcile(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError>;
}

/// Idempotent cancellation observed inside bounded directory work.
pub trait ScreenshotCancellation: Send + Sync {
    /// Whether shutdown or ownership release has cancelled further work.
    fn is_cancelled(&self) -> bool;
}

/// Injected platform capability that starts a watcher before taking its baseline.
pub trait ScreenshotWatcherFactory: Send + Sync {
    /// Start the platform watcher.
    ///
    /// # Errors
    ///
    /// Returns a typed platform, configuration, permission, or watcher failure.
    fn start(
        &self,
        config: ScreenshotInboxConfig,
        terminal_host: &str,
        cancellation: Arc<dyn ScreenshotCancellation>,
    ) -> Result<Box<dyn ActiveScreenshotWatcher>, ScreenshotError>;
}

impl fmt::Debug for ScreenshotCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScreenshotCandidate")
            .field("fingerprint", &self.fingerprint)
            .field("path", &"<redacted>")
            .field("image_type", &self.image_type)
            .finish()
    }
}

/// Screenshot watcher, permission, validation, or platform failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ScreenshotError {
    /// The watcher is intentionally unavailable outside macOS.
    #[error("Screenshot Inbox is available on macOS only")]
    UnsupportedPlatform,
    /// Files & Folders access is denied or revoked.
    #[error("Desktop Files & Folders access is unavailable for {terminal_host}")]
    PermissionDenied {
        /// Best-effort terminal host label, never a path.
        terminal_host: String,
    },
    /// User configuration is outside conservative bounds.
    #[error("invalid screenshot inbox configuration: {0}")]
    InvalidConfig(&'static str),
    /// Watcher setup or event delivery failed.
    #[error("screenshot inbox watcher failed")]
    Watcher,
    /// Directory reconciliation failed after activation.
    #[error("screenshot inbox reconciliation failed")]
    Reconciliation,
    /// A directory exceeded the explicit per-reconciliation work cap.
    #[error("screenshot inbox directory exceeds the bounded reconciliation entry limit")]
    ReconciliationLimit,
    /// Shutdown cancelled directory work before its bounded limit.
    #[error("screenshot inbox reconciliation was cancelled")]
    Cancelled,
    /// A live owner is too old or otherwise incompatible with verified takeover.
    #[error(
        "Screenshot Inbox is owned by an incompatible live Proqi process; close that process to continue"
    )]
    IncompatibleOwner,
    /// Verified takeover was rejected or did not complete before its bound.
    #[error("Screenshot Inbox takeover did not complete; the current owner is still listening")]
    TakeoverFailed,
    /// Safe installation-wide ownership requires the verified local owner endpoint.
    #[error("Screenshot Inbox cannot start because verified local owner control is unavailable")]
    ControlUnavailable,
    /// The authoritative installation-wide lock could not be acquired safely.
    #[error("Screenshot Inbox ownership is unavailable; retry after the runtime error is resolved")]
    Ownership,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_rejects_relative_paths_and_unsafe_bounds() {
        let mut config = ScreenshotInboxConfig {
            directory: PathBuf::from("relative"),
            filename_patterns: Vec::new(),
            capture_all_new_images: false,
            supported_types: vec![ScreenshotImageType::Png],
            bounds: ScreenshotBounds::default(),
            debounce: Duration::from_millis(350),
        };
        assert!(config.validate().is_err());
        config.directory = PathBuf::from("/tmp/screenshots");
        config.bounds.max_file_bytes = 0;
        assert!(config.validate().is_err());
    }
}
