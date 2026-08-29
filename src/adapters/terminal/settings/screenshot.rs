//! Typed Screenshot Inbox configuration and native Desktop resolution.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ports::screenshot::{
    DEFAULT_INACTIVITY_TIMEOUT_MINUTES, DEFAULT_MAX_UNATTENDED_CAPTURES, ScreenshotActivityPolicy,
    ScreenshotBounds, ScreenshotError, ScreenshotImageType, ScreenshotInboxConfig,
};

use super::super::TerminalError;

/// Validated settings whose native Desktop default is resolved only on enable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScreenshotSettings {
    pub(super) directory: Option<PathBuf>,
    pub(super) filename_patterns: Vec<String>,
    pub(super) capture_all_new_images: bool,
    supported_types: Vec<ScreenshotImageType>,
    pub(super) bounds: ScreenshotBounds,
    pub(super) debounce_ms: u64,
    activity_policy: ScreenshotActivityPolicy,
    notify_terminal_on_auto_pause: bool,
}

impl Default for ScreenshotSettings {
    fn default() -> Self {
        Self {
            directory: None,
            filename_patterns: Vec::new(),
            capture_all_new_images: false,
            supported_types: vec![
                ScreenshotImageType::Png,
                ScreenshotImageType::Jpeg,
                ScreenshotImageType::Tiff,
            ],
            bounds: ScreenshotBounds::default(),
            debounce_ms: 350,
            activity_policy: ScreenshotActivityPolicy::default(),
            notify_terminal_on_auto_pause: false,
        }
    }
}

impl ScreenshotSettings {
    pub(crate) const fn activity_policy(&self) -> ScreenshotActivityPolicy {
        self.activity_policy
    }

    pub(crate) const fn notify_terminal_on_auto_pause(&self) -> bool {
        self.notify_terminal_on_auto_pause
    }

    pub(crate) fn watcher_config(&self) -> Result<ScreenshotInboxConfig, ScreenshotError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = self;
            Err(ScreenshotError::UnsupportedPlatform)
        }
        #[cfg(target_os = "macos")]
        {
            let directory = self.directory.clone().or_else(default_screenshot_directory);
            let config = watcher_config(self, directory.ok_or(ScreenshotError::InvalidConfig(
                "the current user's Desktop directory is unavailable; configure screenshot_inbox.directory",
            ))?);
            config.validate()?;
            Ok(config)
        }
    }
}

#[cfg(target_os = "macos")]
fn default_screenshot_directory() -> Option<PathBuf> {
    directories::UserDirs::new()
        .and_then(|directories| directories.desktop_dir().map(Path::to_path_buf))
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct ScreenshotSettingsDocument {
    directory: Option<String>,
    filename_patterns: Vec<String>,
    capture_all_new_images: bool,
    supported_types: Vec<String>,
    min_file_bytes: u64,
    max_file_bytes: u64,
    max_dimension: u32,
    max_pixels: u64,
    debounce_ms: u64,
    inactivity_timeout_minutes: u16,
    max_unattended_captures: u16,
    notify_terminal_on_auto_pause: bool,
}

impl Default for ScreenshotSettingsDocument {
    fn default() -> Self {
        let bounds = ScreenshotBounds::default();
        Self {
            directory: None,
            filename_patterns: Vec::new(),
            capture_all_new_images: false,
            supported_types: vec!["png".to_owned(), "jpeg".to_owned(), "tiff".to_owned()],
            min_file_bytes: bounds.min_file_bytes,
            max_file_bytes: bounds.max_file_bytes,
            max_dimension: bounds.max_dimension,
            max_pixels: bounds.max_pixels,
            debounce_ms: 350,
            inactivity_timeout_minutes: DEFAULT_INACTIVITY_TIMEOUT_MINUTES,
            max_unattended_captures: DEFAULT_MAX_UNATTENDED_CAPTURES,
            notify_terminal_on_auto_pause: false,
        }
    }
}

pub(super) fn validate(
    document: ScreenshotSettingsDocument,
) -> Result<ScreenshotSettings, TerminalError> {
    let directory = document.directory.map(PathBuf::from);
    if directory.as_ref().is_some_and(|path| !path.is_absolute()) {
        return Err(TerminalError::Config(
            "screenshot_inbox.directory must be absolute".to_owned(),
        ));
    }
    let supported_types = parse_types(&document.supported_types)?;
    let activity_policy = ScreenshotActivityPolicy::new(
        document.inactivity_timeout_minutes,
        document.max_unattended_captures,
    )
    .ok_or_else(|| TerminalError::Config(
        "screenshot_inbox inactivity_timeout_minutes must be 1..=1440 and max_unattended_captures must be 1..=100".to_owned(),
    ))?;
    let settings = ScreenshotSettings {
        directory,
        filename_patterns: document.filename_patterns,
        capture_all_new_images: document.capture_all_new_images,
        supported_types,
        bounds: ScreenshotBounds {
            min_file_bytes: document.min_file_bytes,
            max_file_bytes: document.max_file_bytes,
            max_dimension: document.max_dimension,
            max_pixels: document.max_pixels,
        },
        debounce_ms: document.debounce_ms,
        activity_policy,
        notify_terminal_on_auto_pause: document.notify_terminal_on_auto_pause,
    };
    watcher_config(
        &settings,
        settings
            .directory
            .clone()
            .unwrap_or_else(|| PathBuf::from("/")),
    )
    .validate()
    .map(|()| settings)
    .map_err(|error| TerminalError::Config(error.to_string()))
}

fn parse_types(values: &[String]) -> Result<Vec<ScreenshotImageType>, TerminalError> {
    values
        .iter()
        .map(|value| ScreenshotImageType::parse(value))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            TerminalError::Config(
                "screenshot_inbox.supported_types accepts png, jpeg, and tiff".to_owned(),
            )
        })
}

fn watcher_config(settings: &ScreenshotSettings, directory: PathBuf) -> ScreenshotInboxConfig {
    ScreenshotInboxConfig {
        directory,
        filename_patterns: settings.filename_patterns.clone(),
        capture_all_new_images: settings.capture_all_new_images,
        supported_types: settings.supported_types.clone(),
        bounds: settings.bounds,
        debounce: std::time::Duration::from_millis(settings.debounce_ms),
    }
}
