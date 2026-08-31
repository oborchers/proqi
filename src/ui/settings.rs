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
    /// Show the complete canonical session identifier beside the session name when it fits.
    pub show_session_id: bool,
    /// Continue recognized Markdown list items when Enter inserts a newline.
    pub smart_lists: bool,
    /// Spaces inserted for every list indentation level.
    pub list_indent_width: u8,
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
            show_session_id: false,
            smart_lists: true,
            list_indent_width: 2,
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
    /// Submit the focused thought, removing it after acceptance.
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
    /// Extend a range upward; Primary plus this shifted key reorders upward.
    #[serde(alias = "move_up")]
    pub range_up: char,
    /// Extend a range downward; Primary plus this shifted key reorders downward.
    #[serde(alias = "move_down")]
    pub range_down: char,
    /// Toggle expanded presentation.
    pub collapse: char,
    /// Toggle the focused thought in the multi-selection.
    pub select: char,
    /// Select every live thought in board order.
    pub select_all: char,
    /// Latch contiguous range selection.
    pub range_select: char,
    /// Search thought content.
    pub search: char,
    /// Discover commands.
    pub commands: char,
    /// Show help.
    pub help: char,
    /// Exit.
    pub quit: char,
    /// Toggle the macOS screenshot inbox.
    pub screenshot_inbox: char,
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
            range_up: 'K',
            range_down: 'J',
            collapse: 'c',
            select: ' ',
            select_all: 'a',
            range_select: 'v',
            search: '/',
            commands: ':',
            help: '?',
            quit: 'q',
            screenshot_inbox: 'i',
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
    RangeUp,
    RangeDown,
    Collapse,
    Select,
    SelectAll,
    RangeSelect,
    Search,
    Commands,
    Help,
    Quit,
    ScreenshotInbox,
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
            (self.range_up, BoardCommand::RangeUp),
            (self.range_down, BoardCommand::RangeDown),
            (self.collapse, BoardCommand::Collapse),
            (self.select, BoardCommand::Select),
            (self.select_all, BoardCommand::SelectAll),
            (self.range_select, BoardCommand::RangeSelect),
            (self.search, BoardCommand::Search),
            (self.commands, BoardCommand::Commands),
            (self.help, BoardCommand::Help),
            (self.quit, BoardCommand::Quit),
            (self.screenshot_inbox, BoardCommand::ScreenshotInbox),
        ];
        bindings
            .into_iter()
            .find_map(|(binding, command)| (binding == character).then_some(command))
    }

    /// Resolve a normalized key through the Board command map.
    ///
    /// Physical Delete is an invariant spelling of the remappable delete
    /// command. Backspace deliberately remains unassigned in Board mode.
    pub(super) fn command_for_key(&self, key: super::UiKey) -> Option<BoardCommand> {
        match key {
            super::UiKey::Delete => Some(BoardCommand::Delete),
            super::UiKey::Character(character) => self.command(character),
            _ => None,
        }
    }

    pub(crate) fn delete_label(&self) -> String {
        format!("{}/Del", key_label(self.delete))
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
            self.range_up,
            self.range_down,
            self.collapse,
            self.select,
            self.select_all,
            self.range_select,
            self.search,
            self.commands,
            self.help,
            self.quit,
            self.screenshot_inbox,
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

pub(crate) fn primary_key_label(suffix: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{suffix}")
    } else {
        format!("Ctrl+{suffix}")
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
