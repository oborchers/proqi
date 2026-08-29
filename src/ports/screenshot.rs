//! Terminal-independent screenshot inbox capability and validation values.

use std::{fmt, path::PathBuf, time::Duration};

use thiserror::Error;

/// Maximum number of configurable filename fallback patterns.
pub const MAX_FILENAME_PATTERNS: usize = 32;
/// Maximum length of one fallback pattern in Unicode scalar values.
pub const MAX_FILENAME_PATTERN_CHARS: usize = 160;

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
