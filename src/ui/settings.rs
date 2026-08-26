//! User-configurable terminal appearance and board bindings.

use serde::Deserialize;

/// Explicit or capability-derived terminal theme.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    /// Inherit terminal foreground and background, adapting to capabilities.
    #[default]
    Auto,
    /// Explicit light palette.
    Light,
    /// Explicit dark palette.
    Dark,
    /// Terminal-native limited-color fallback.
    Limited,
}

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
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct UiSettings {
    /// Permit on-demand stable release checks in interactive release builds.
    pub check_for_updates: bool,
    /// Theme preference.
    pub theme: ThemePreference,
    /// Keyboard protocol negotiation.
    pub keyboard_enhancement: KeyboardEnhancement,
    /// Remappable direct board keys.
    pub keybindings: KeyBindings,
    /// Vertical separation between thoughts.
    pub density: BoardDensity,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            check_for_updates: true,
            theme: ThemePreference::default(),
            keyboard_enhancement: KeyboardEnhancement::default(),
            keybindings: KeyBindings::default(),
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

/// Direct character bindings for common board actions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct KeyBindings {
    /// Create thought.
    pub new: char,
    /// Edit focused thought.
    pub edit: char,
    /// Delete focused thought.
    pub delete: char,
    /// Copy focused thought.
    pub copy: char,
    /// Cut focused thought.
    pub cut: char,
    /// Submit and remove the focused thought after acceptance.
    #[serde(alias = "send")]
    pub submit_remove: char,
    /// Submit and preserve the focused thought.
    #[serde(alias = "submit")]
    pub submit_keep: char,
    /// Undo board action.
    pub undo: char,
    /// Move focus upward.
    pub focus_up: char,
    /// Move focus downward.
    pub focus_down: char,
    /// Reorder upward.
    pub move_up: char,
    /// Reorder downward.
    pub move_down: char,
    /// Toggle expanded presentation.
    pub collapse: char,
    /// Toggle the focused thought in the multi-selection.
    pub select: char,
    /// Search thought content.
    pub search: char,
    /// Discover commands.
    pub commands: char,
    /// Show help.
    pub help: char,
    /// Exit.
    pub quit: char,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            new: 'n',
            edit: 'e',
            delete: 'd',
            copy: 'y',
            cut: 'x',
            submit_remove: 's',
            submit_keep: 'S',
            undo: 'u',
            focus_up: 'k',
            focus_down: 'j',
            move_up: 'K',
            move_down: 'J',
            collapse: 'c',
            select: ' ',
            search: '/',
            commands: ':',
            help: '?',
            quit: 'q',
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BoardCommand {
    New,
    Edit,
    Delete,
    Copy,
    Cut,
    SubmitRemove,
    SubmitKeep,
    Undo,
    FocusUp,
    FocusDown,
    MoveUp,
    MoveDown,
    Collapse,
    Select,
    Search,
    Commands,
    Help,
    Quit,
}

impl KeyBindings {
    pub(super) fn command(&self, character: char) -> Option<BoardCommand> {
        let bindings = [
            (self.new, BoardCommand::New),
            (self.edit, BoardCommand::Edit),
            (self.delete, BoardCommand::Delete),
            (self.copy, BoardCommand::Copy),
            (self.cut, BoardCommand::Cut),
            (self.submit_remove, BoardCommand::SubmitRemove),
            (self.submit_keep, BoardCommand::SubmitKeep),
            (self.undo, BoardCommand::Undo),
            (self.focus_up, BoardCommand::FocusUp),
            (self.focus_down, BoardCommand::FocusDown),
            (self.move_up, BoardCommand::MoveUp),
            (self.move_down, BoardCommand::MoveDown),
            (self.collapse, BoardCommand::Collapse),
            (self.select, BoardCommand::Select),
            (self.search, BoardCommand::Search),
            (self.commands, BoardCommand::Commands),
            (self.help, BoardCommand::Help),
            (self.quit, BoardCommand::Quit),
        ];
        bindings
            .into_iter()
            .find_map(|(binding, command)| (binding == character).then_some(command))
    }

    /// Reject ambiguous board characters before the terminal starts.
    ///
    /// # Errors
    ///
    /// Returns an error for control characters or duplicate bindings.
    pub fn validate(&self) -> Result<(), &'static str> {
        let values = [
            self.new,
            self.edit,
            self.delete,
            self.copy,
            self.cut,
            self.submit_remove,
            self.submit_keep,
            self.undo,
            self.focus_up,
            self.focus_down,
            self.move_up,
            self.move_down,
            self.collapse,
            self.select,
            self.search,
            self.commands,
            self.help,
            self.quit,
        ];
        for (index, value) in values.iter().enumerate() {
            if value.is_control() || values[index + 1..].contains(value) {
                return Err("keybindings must be distinct printable characters");
            }
        }
        Ok(())
    }
}

pub(crate) fn key_label(key: char) -> String {
    match key {
        ' ' => "Space".to_owned(),
        '\t' => "Tab".to_owned(),
        '\n' | '\r' => "Enter".to_owned(),
        _ => key.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{BoardCommand, BoardDensity, KeyBindings, UiSettings};

    #[test]
    fn partial_toml_uses_defaults_and_remaps_one_action() {
        let settings: UiSettings = toml::from_str("[keybindings]\nnew = 't'").expect("settings");
        assert!(settings.check_for_updates);
        assert_eq!(settings.keybindings.command('t'), Some(BoardCommand::New));
        assert_eq!(settings.keybindings.edit, 'e');
        assert!(settings.keybindings.validate().is_ok());
    }

    #[test]
    fn update_checks_can_be_disabled_globally() {
        let settings: UiSettings = toml::from_str("check_for_updates = false").expect("settings");
        assert!(!settings.check_for_updates);
    }

    #[test]
    fn compact_board_density_is_configurable() {
        let settings: UiSettings = toml::from_str("density = 'compact'").expect("settings");
        assert_eq!(settings.density, BoardDensity::Compact);
    }

    #[test]
    fn ambiguous_bindings_are_rejected() {
        let mut bindings = KeyBindings::default();
        bindings.edit = bindings.new;
        assert!(bindings.validate().is_err());
    }

    #[test]
    fn legacy_delivery_names_map_to_the_new_submission_dispositions() {
        let settings: UiSettings =
            toml::from_str("[keybindings]\nsend = 'a'\nsubmit = 'A'").expect("settings");
        assert_eq!(
            settings.keybindings.command('a'),
            Some(BoardCommand::SubmitRemove)
        );
        assert_eq!(
            settings.keybindings.command('A'),
            Some(BoardCommand::SubmitKeep)
        );
    }
}
