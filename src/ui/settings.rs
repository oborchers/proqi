//! User-configurable terminal appearance and board bindings.

use serde::Deserialize;

pub(crate) const RECOVERY_RETRY_KEY: char = 'r';
pub(crate) const RECOVERY_EXPORT_KEY: char = 'w';

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
pub struct UiSettings {
    /// Permit automatic stable-release checks on interactive release startup.
    pub check_for_updates: bool,
    /// Continue recognized Markdown list items when Enter inserts a newline.
    pub smart_lists: bool,
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
            smart_lists: true,
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
        if matches!(self.quit, RECOVERY_RETRY_KEY | RECOVERY_EXPORT_KEY) {
            return Err("the quit binding cannot use the reserved recovery keys r or w");
        }
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
    use super::KeyBindings;

    #[test]
    fn ambiguous_bindings_are_rejected() {
        let mut bindings = KeyBindings::default();
        bindings.edit = bindings.new;
        assert!(bindings.validate().is_err());
    }

    #[test]
    fn recovery_keys_cannot_be_used_for_quit() {
        for reserved in ['r', 'w'] {
            let bindings = KeyBindings {
                quit: reserved,
                ..KeyBindings::default()
            };
            assert!(bindings.validate().is_err());
        }
    }
}
