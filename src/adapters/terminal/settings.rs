//! Bounded, platform-path-backed terminal configuration loading.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::ui::{
    BoardDensity, KeyBindings, KeyboardEnhancement, ThemeOverrides, ThemePreference, ThemeRecipe,
    UiSettings,
};

use super::TerminalError;

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const THEME_SCHEMA_VERSION: u16 = 1;

/// Validated terminal settings and their resolved theme recipe.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct LoadedSettings {
    pub(crate) ui: UiSettings,
    pub(crate) theme: ThemeRecipe,
    theme_source: ThemeSource,
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
    smart_lists: bool,
    theme: String,
    theme_overrides: ThemeOverrides,
    keyboard_enhancement: KeyboardEnhancement,
    keybindings: KeyBindings,
    density: BoardDensity,
}

impl Default for SettingsDocument {
    fn default() -> Self {
        Self {
            check_for_updates: true,
            smart_lists: true,
            theme: "auto".to_owned(),
            theme_overrides: ThemeOverrides::default(),
            keyboard_enhancement: KeyboardEnhancement::default(),
            keybindings: KeyBindings::default(),
            density: BoardDensity::default(),
        }
    }
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
    document
        .keybindings
        .validate()
        .map_err(|error| TerminalError::Config(error.to_owned()))?;
    let ui = UiSettings {
        check_for_updates: document.check_for_updates,
        smart_lists: document.smart_lists,
        keyboard_enhancement: document.keyboard_enhancement,
        keybindings: document.keybindings,
        density: document.density,
    };
    let (theme, theme_source) = load_theme(config_dir, &document.theme, document.theme_overrides)?;
    Ok(LoadedSettings {
        ui,
        theme,
        theme_source,
    })
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
