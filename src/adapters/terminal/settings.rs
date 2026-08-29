//! Bounded, platform-path-backed terminal configuration loading.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::ports::invocation::{
    AdditionalInvocationRoot, InvocationHarness, InvocationKind, InvocationScope,
};
use crate::ui::{
    BoardDensity, KeyBindings, KeyboardEnhancement, ThemeOverrides, ThemePreference, ThemeRecipe,
    UiSettings,
};

use super::TerminalError;
use crate::ports::screenshot::{
    ScreenshotBounds, ScreenshotError, ScreenshotImageType, ScreenshotInboxConfig,
};

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const THEME_SCHEMA_VERSION: u16 = 1;

/// Validated terminal settings and their resolved theme recipe.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct LoadedSettings {
    pub(crate) ui: UiSettings,
    pub(crate) theme: ThemeRecipe,
    pub(crate) invocation_roots: Vec<AdditionalInvocationRoot>,
    pub(crate) screenshot: ScreenshotSettings,
    theme_source: ThemeSource,
}

/// Validated settings whose native Desktop default is resolved only on enable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScreenshotSettings {
    directory: Option<PathBuf>,
    filename_patterns: Vec<String>,
    capture_all_new_images: bool,
    supported_types: Vec<ScreenshotImageType>,
    bounds: ScreenshotBounds,
    debounce_ms: u64,
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
        }
    }
}

impl ScreenshotSettings {
    pub(crate) fn watcher_config(&self) -> Result<ScreenshotInboxConfig, ScreenshotError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = self;
            Err(ScreenshotError::UnsupportedPlatform)
        }
        #[cfg(target_os = "macos")]
        {
            let directory = self.directory.clone().or_else(default_screenshot_directory);
            let config = ScreenshotInboxConfig {
                directory: directory.ok_or(ScreenshotError::InvalidConfig(
                    "the current user's Desktop directory is unavailable; configure screenshot_inbox.directory",
                ))?,
                filename_patterns: self.filename_patterns.clone(),
                capture_all_new_images: self.capture_all_new_images,
                supported_types: self.supported_types.clone(),
                bounds: self.bounds,
                debounce: std::time::Duration::from_millis(self.debounce_ms),
            };
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

impl LoadedSettings {
    pub(crate) fn theme_source(&self) -> &'static str {
        match self.theme_source {
            ThemeSource::BuiltIn => "built_in",
            ThemeSource::File => "file",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ThemeSource {
    #[default]
    BuiltIn,
    File,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SettingsDocument {
    check_for_updates: bool,
    show_session_id: bool,
    smart_lists: bool,
    list_indent_width: u8,
    theme: String,
    theme_overrides: ThemeOverrides,
    keyboard_enhancement: KeyboardEnhancement,
    keybindings: KeyBindings,
    density: BoardDensity,
    invocation_roots: Vec<InvocationRootDocument>,
    screenshot_inbox: ScreenshotSettingsDocument,
}

impl Default for SettingsDocument {
    fn default() -> Self {
        Self {
            check_for_updates: true,
            show_session_id: false,
            smart_lists: true,
            list_indent_width: 2,
            theme: "auto".to_owned(),
            theme_overrides: ThemeOverrides::default(),
            keyboard_enhancement: KeyboardEnhancement::default(),
            keybindings: KeyBindings::default(),
            density: BoardDensity::default(),
            invocation_roots: Vec::new(),
            screenshot_inbox: ScreenshotSettingsDocument::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ScreenshotSettingsDocument {
    directory: Option<String>,
    filename_patterns: Vec<String>,
    capture_all_new_images: bool,
    supported_types: Vec<String>,
    min_file_bytes: u64,
    max_file_bytes: u64,
    max_dimension: u32,
    max_pixels: u64,
    debounce_ms: u64,
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
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InvocationRootDocument {
    path: String,
    kind: InvocationKind,
    harness: InvocationHarness,
    scope: InvocationScope,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeDocument {
    schema_version: u16,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    base: ThemePreference,
    #[serde(default)]
    colors: ThemeOverrides,
}

pub(crate) fn load_settings(config_dir: &Path) -> Result<LoadedSettings, TerminalError> {
    let path = config_dir.join("config.toml");
    let Some(content) = read_optional_config(&path)? else {
        return Ok(LoadedSettings::default());
    };
    ensure_private(&path)?;
    parse_settings(config_dir, &content)
}

/// Validate the same configuration graph without mutating permissions.
pub(crate) fn inspect_settings(config_dir: &Path) -> Result<LoadedSettings, TerminalError> {
    let path = config_dir.join("config.toml");
    let Some(content) = read_optional_config(&path)? else {
        return Ok(LoadedSettings::default());
    };
    parse_settings(config_dir, &content)
}

fn parse_settings(config_dir: &Path, content: &str) -> Result<LoadedSettings, TerminalError> {
    let document: SettingsDocument = parse_toml(content, "config.toml")?;
    if !(1..=8).contains(&document.list_indent_width) {
        return Err(TerminalError::Config(
            "list_indent_width must be between 1 and 8 spaces".to_owned(),
        ));
    }
    document
        .keybindings
        .validate()
        .map_err(|error| TerminalError::Config(error.to_owned()))?;
    let ui = UiSettings {
        check_for_updates: document.check_for_updates,
        show_session_id: document.show_session_id,
        smart_lists: document.smart_lists,
        list_indent_width: document.list_indent_width,
        keyboard_enhancement: document.keyboard_enhancement,
        keybindings: document.keybindings,
        density: document.density,
    };
    let (theme, theme_source) = load_theme(config_dir, &document.theme, document.theme_overrides)?;
    let invocation_roots = validate_invocation_roots(document.invocation_roots)?;
    let screenshot = validate_screenshot_settings(document.screenshot_inbox)?;
    Ok(LoadedSettings {
        ui,
        theme,
        invocation_roots,
        screenshot,
        theme_source,
    })
}

fn validate_screenshot_settings(
    document: ScreenshotSettingsDocument,
) -> Result<ScreenshotSettings, TerminalError> {
    let directory = document.directory.map(PathBuf::from);
    if directory.as_ref().is_some_and(|path| !path.is_absolute()) {
        return Err(TerminalError::Config(
            "screenshot_inbox.directory must be absolute".to_owned(),
        ));
    }
    let supported_types = document
        .supported_types
        .iter()
        .map(|value| ScreenshotImageType::parse(value))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            TerminalError::Config(
                "screenshot_inbox.supported_types accepts png, jpeg, and tiff".to_owned(),
            )
        })?;
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
    };
    let validation = ScreenshotInboxConfig {
        directory: settings
            .directory
            .clone()
            .unwrap_or_else(|| PathBuf::from("/")),
        filename_patterns: settings.filename_patterns.clone(),
        capture_all_new_images: settings.capture_all_new_images,
        supported_types: settings.supported_types.clone(),
        bounds: settings.bounds,
        debounce: std::time::Duration::from_millis(settings.debounce_ms),
    };
    validation
        .validate()
        .map(|()| settings)
        .map_err(|error| TerminalError::Config(error.to_string()))
}

fn validate_invocation_roots(
    roots: Vec<InvocationRootDocument>,
) -> Result<Vec<AdditionalInvocationRoot>, TerminalError> {
    if roots.len() > 32 {
        return Err(TerminalError::Config(
            "invocation_roots supports at most 32 entries".to_owned(),
        ));
    }
    roots
        .into_iter()
        .map(|root| {
            if root.path.trim().is_empty()
                || root.path.chars().count() > 1_024
                || root.path.chars().any(char::is_control)
                || root.path.contains("://")
                || root.scope == InvocationScope::Plugin
                || (root.scope == InvocationScope::Global && !Path::new(&root.path).is_absolute())
            {
                return Err(TerminalError::Config(
                    "each invocation root needs a local path; global roots must be absolute"
                        .to_owned(),
                ));
            }
            Ok(AdditionalInvocationRoot {
                path: PathBuf::from(root.path),
                kind: root.kind,
                harness: root.harness,
                scope: root.scope,
            })
        })
        .collect()
}

fn load_theme(
    config_dir: &Path,
    selector: &str,
    inline: ThemeOverrides,
) -> Result<(ThemeRecipe, ThemeSource), TerminalError> {
    if let Some(preference) = built_in(selector) {
        if matches!(preference, ThemePreference::Limited) && !inline.is_empty() {
            return Err(TerminalError::Config(
                "theme_overrides cannot be combined with the limited theme".to_owned(),
            ));
        }
        return Ok((
            ThemeRecipe::built_in(preference, inline),
            ThemeSource::BuiltIn,
        ));
    }
    let path = resolve_theme_path(config_dir, selector)?;
    let content = read_theme_file(&path)?;
    let document: ThemeDocument = parse_toml(&content, "theme file")?;
    validate_theme_document(&document)?;
    Ok((
        ThemeRecipe::custom(document.base, document.colors.overlay(inline)),
        ThemeSource::File,
    ))
}

fn built_in(selector: &str) -> Option<ThemePreference> {
    match selector {
        "auto" => Some(ThemePreference::Auto),
        "light" => Some(ThemePreference::Light),
        "dark" => Some(ThemePreference::Dark),
        "limited" => Some(ThemePreference::Limited),
        _ => None,
    }
}

fn resolve_theme_path(config_dir: &Path, selector: &str) -> Result<PathBuf, TerminalError> {
    if selector.trim().is_empty() {
        return Err(TerminalError::Config(
            "theme must name a built-in theme or a local TOML file".to_owned(),
        ));
    }
    if selector.contains("://") {
        return Err(TerminalError::Config(
            "theme must be a local TOML path; remote URLs are not supported".to_owned(),
        ));
    }
    let selected = PathBuf::from(selector);
    Ok(if selected.is_absolute() {
        selected
    } else {
        config_dir.join(selected)
    })
}

fn read_optional_config(path: &Path) -> Result<Option<String>, TerminalError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(TerminalError::Config(error.to_string())),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES
    {
        return Err(TerminalError::Config(
            "config.toml must be a regular file no larger than 64 KiB".to_owned(),
        ));
    }
    fs::read_to_string(path)
        .map(Some)
        .map_err(|error| TerminalError::Config(error.to_string()))
}

fn read_theme_file(path: &Path) -> Result<String, TerminalError> {
    let metadata = fs::metadata(path).map_err(|error| {
        TerminalError::Config(format!(
            "theme file '{}' cannot be read: {error}",
            path.display()
        ))
    })?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err(TerminalError::Config(
            "theme must resolve to a regular file no larger than 64 KiB".to_owned(),
        ));
    }
    fs::read_to_string(path).map_err(|error| TerminalError::Config(error.to_string()))
}

fn validate_theme_document(document: &ThemeDocument) -> Result<(), TerminalError> {
    if document.schema_version != THEME_SCHEMA_VERSION {
        return Err(TerminalError::Config(format!(
            "unsupported theme schema version {}; expected {THEME_SCHEMA_VERSION}",
            document.schema_version
        )));
    }
    if matches!(document.base, ThemePreference::Limited) {
        return Err(TerminalError::Config(
            "custom themes cannot use the limited base".to_owned(),
        ));
    }
    if document.name.as_deref().is_some_and(|name| {
        name.trim().is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control)
    }) {
        return Err(TerminalError::Config(
            "theme name must contain visible text without control characters".to_owned(),
        ));
    }
    Ok(())
}

fn parse_toml<T: for<'de> Deserialize<'de>>(
    content: &str,
    label: &str,
) -> Result<T, TerminalError> {
    toml::from_str(content)
        .map_err(|error| TerminalError::Config(format!("invalid {label}: {error}")))
}

fn ensure_private(path: &Path) -> Result<(), TerminalError> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, permissions).map_err(|error| TerminalError::Config(error.to_string()))
}

#[cfg(test)]
mod tests;
