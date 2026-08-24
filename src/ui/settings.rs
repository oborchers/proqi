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

/// Complete UI configuration loaded from the platform config directory.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct UiSettings {
    /// Theme preference.
    pub theme: ThemePreference,
    /// Remappable direct board keys.
    pub keybindings: KeyBindings,
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
    /// Submit focused thought to a verified adjacent agent.
    pub submit: char,
    /// Submit focused thought and remove it after acceptance.
    pub submit_remove: char,
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
            submit: 's',
            submit_remove: 'S',
            undo: 'u',
            focus_up: 'k',
            focus_down: 'j',
            move_up: 'K',
            move_down: 'J',
            collapse: ' ',
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
    Submit,
    SubmitRemove,
    Undo,
    FocusUp,
    FocusDown,
    MoveUp,
    MoveDown,
    Collapse,
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
            (self.submit, BoardCommand::Submit),
            (self.submit_remove, BoardCommand::SubmitRemove),
            (self.undo, BoardCommand::Undo),
            (self.focus_up, BoardCommand::FocusUp),
            (self.focus_down, BoardCommand::FocusDown),
            (self.move_up, BoardCommand::MoveUp),
            (self.move_down, BoardCommand::MoveDown),
            (self.collapse, BoardCommand::Collapse),
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
            self.submit,
            self.submit_remove,
            self.undo,
            self.focus_up,
            self.focus_down,
            self.move_up,
            self.move_down,
            self.collapse,
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
    use super::{BoardCommand, KeyBindings, UiSettings};

    #[test]
    fn partial_toml_uses_defaults_and_remaps_one_action() {
        let settings: UiSettings = toml::from_str("[keybindings]\nnew = 't'").expect("settings");
        assert_eq!(settings.keybindings.command('t'), Some(BoardCommand::New));
        assert_eq!(settings.keybindings.edit, 'e');
        assert!(settings.keybindings.validate().is_ok());
    }

    #[test]
    fn ambiguous_bindings_are_rejected() {
        let mut bindings = KeyBindings::default();
        bindings.edit = bindings.new;
        assert!(bindings.validate().is_err());
    }
}
