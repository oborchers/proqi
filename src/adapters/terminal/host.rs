//! One bounded owner for terminal-host capability and display-name detection.

const FALLBACK_LABEL: &str = "the terminal host running Proqi";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TerminalHost {
    program: String,
    term: String,
}

impl TerminalHost {
    pub(super) fn detect() -> Self {
        Self::from_values(
            std::env::var("TERM_PROGRAM").unwrap_or_default(),
            std::env::var("TERM").unwrap_or_default(),
        )
    }

    pub(super) fn from_values(program: impl Into<String>, term: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            term: term.into(),
        }
    }

    pub(super) fn label(&self) -> String {
        let value = valid_label(&self.program).unwrap_or(FALLBACK_LABEL);
        match value {
            "Apple_Terminal" => "Terminal".to_owned(),
            "iTerm.app" => "iTerm2".to_owned(),
            "ghostty" | "Ghostty" => "Ghostty".to_owned(),
            "vscode" => "Visual Studio Code".to_owned(),
            _ => value.to_owned(),
        }
    }

    pub(super) fn supports_osc9(&self) -> bool {
        matches!(self.program.as_str(), "iTerm.app" | "ghostty" | "Ghostty")
            && !self.term.starts_with("tmux")
    }

    pub(super) fn supports_keyboard_event_types(&self) -> bool {
        !matches!(self.program.as_str(), "iTerm.app" | "ghostty" | "Ghostty")
            && !self.term.starts_with("tmux")
    }
}

fn valid_label(value: &str) -> Option<&str> {
    (!value.is_empty() && value.chars().count() <= 80 && !value.chars().any(char::is_control))
        .then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_labels_and_osc9_capability_share_one_detection_table() {
        for (program, label) in [("ghostty", "Ghostty"), ("iTerm.app", "iTerm2")] {
            let host = TerminalHost::from_values(program, "xterm-256color");
            assert_eq!(host.label(), label);
            assert!(host.supports_osc9());
            assert!(!host.supports_keyboard_event_types());
        }
        assert!(!TerminalHost::from_values("iTerm.app", "tmux-256color").supports_osc9());
        assert_eq!(TerminalHost::from_values("\n", "").label(), FALLBACK_LABEL);
    }
}
