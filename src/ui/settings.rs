//! User-configurable terminal appearance and board bindings.

pub use super::shortcut_registry::legacy::KeyBindings;
use serde::Deserialize;

/// Optional enhanced keyboard reporting for compatible terminal emulators.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum KeyboardEnhancement {
    /// Enable only the flags compatible with the detected terminal transport.
    #[default]
    Auto,
    /// Use portable Crossterm key events without enhancement negotiation.
    Disabled,
}

/// Complete UI configuration loaded from the platform config directory.
#[derive(Clone, Debug, Eq, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent user-facing toggles with no shared state machine"
)]
pub struct UiSettings {
    /// Permit automatic stable-release checks on interactive release startup.
    pub check_for_updates: bool,
    /// Show the complete canonical session identifier beside the session name when it fits.
    pub show_session_id: bool,
    /// Continue recognized Markdown list items when Enter inserts a newline.
    pub smart_lists: bool,
    /// Spaces inserted for every list indentation level.
    pub list_indent_width: u8,
    /// Exact text inserted between thoughts by the merge transformation.
    pub merge_separator: String,
    /// Keyboard protocol negotiation.
    pub keyboard_enhancement: KeyboardEnhancement,
    /// Permanently hide the footer (session name and shortcut hints).
    pub footer_hidden: bool,
    /// Fully resolved and validated contextual keyboard map.
    pub shortcuts: super::ShortcutRegistry,
    /// Vertical separation between thoughts.
    pub density: BoardDensity,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            check_for_updates: true,
            show_session_id: false,
            smart_lists: true,
            list_indent_width: 2,
            merge_separator: "\n\n".to_owned(),
            keyboard_enhancement: KeyboardEnhancement::default(),
            footer_hidden: false,
            shortcuts: super::ShortcutRegistry::default(),
            density: BoardDensity::default(),
        }
    }
}

/// Board spacing preference.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BoardDensity {
    /// A restrained separator row between thoughts.
    #[default]
    Comfortable,
    /// Minimize vertical separation in constrained panes.
    Compact,
}

#[cfg(test)]
#[path = "settings/tests.rs"]
mod tests;
