//! Canonical first-run practice-board policy and copy.

use crate::{
    domain::{DomainError, Session, SessionBoard, Thought, ThoughtPosition},
    ports::{
        environment::IdGenerator,
        store::{FirstRunBoard, OnboardingVersion},
    },
};

const WELCOME: &str = "Welcome to Proqi, a prompt composer designed to replace common agent input methods. Capture, refine, organize, and submit prompts here.";
const EDITING: &str = "Press Enter to edit the focused thought. Press Esc to return to board mode.";
const CREATION: &str =
    "Press n to create a new thought, or paste in board mode to create one from the pasted text.";
const NAVIGATION: &str = "Use j or ↓ to move to the next thought, and k or ↑ to move to the previous one. Press d to delete the focused thought and u to undo.";
const HERDR_MANAGED: &str = "Herdr is detected. It organizes agent panes and lets Proqi submit with s when it verifies a compatible adjacent agent. Learn more at https://herdr.dev";
const STANDALONE: &str = "Proqi works on its own. Herdr adds a power-user workflow for organized agent panes and verified adjacent submission when a compatible agent is available. Learn more at https://herdr.dev";

/// Exact final practice thought required by the first-run contract.
pub const PRACTICE_BOARD_DELETION: &str =
    "Press a and d in board mode to delete this entire practice board.";

/// Cheap, truthful local environment distinction used only to select practice copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirstRunEnvironment {
    /// The current Proqi pane carries Herdr's managed-pane signal.
    HerdrManaged,
    /// The current Proqi pane is not managed by Herdr.
    Standalone,
}

impl FirstRunEnvironment {
    /// The exact six canonical thought bodies in board order.
    #[must_use]
    pub const fn thought_contents(self) -> [&'static str; 6] {
        let integration = match self {
            Self::HerdrManaged => HERDR_MANAGED,
            Self::Standalone => STANDALONE,
        };
        [
            WELCOME,
            EDITING,
            CREATION,
            NAVIGATION,
            integration,
            PRACTICE_BOARD_DELETION,
        ]
    }
}

/// Build the current practice board from ordinary domain thoughts.
///
/// # Errors
///
/// Returns a domain error if the session or generated ordering is invalid.
pub fn first_run_board(
    session: Session,
    ids: &mut impl IdGenerator,
    environment: FirstRunEnvironment,
) -> Result<FirstRunBoard, DomainError> {
    let session_id = session.id;
    let now = session.created_at;
    let thoughts = environment
        .thought_contents()
        .into_iter()
        .zip(0_u32..)
        .map(|(content, position)| {
            Thought::new(
                ids.thought_id(),
                session_id,
                content.to_owned(),
                ThoughtPosition::new(position),
                now,
            )
        })
        .collect();
    let board = SessionBoard::new(session, thoughts)?;
    Ok(FirstRunBoard::new(OnboardingVersion::PRACTICE_BOARD, board))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_are_distinct_bounded_and_use_canonical_copy_and_keys() {
        let managed = FirstRunEnvironment::HerdrManaged.thought_contents();
        let standalone = FirstRunEnvironment::Standalone.thought_contents();
        assert_eq!(managed.len(), 6);
        assert_eq!(standalone.len(), 6);
        assert_ne!(managed[4], standalone[4]);
        assert!(managed[4].contains("Herdr is detected"));
        assert!(standalone[4].contains("Proqi works on its own"));
        assert!(managed[4].contains("https://herdr.dev"));
        assert!(standalone[4].contains("https://herdr.dev"));
        assert!(managed[1].contains("Press Enter"));
        assert!(managed[1].contains("Press Esc"));
        assert!(managed[2].contains("Press n"));
        assert!(managed[3].contains("j or ↓"));
        assert!(managed[3].contains("k or ↑"));
        assert!(managed[3].contains("Press d"));
        assert!(managed[3].contains("u to undo"));
        assert!(managed[4].contains("submit with s"));
        assert_eq!(managed[5], PRACTICE_BOARD_DELETION);
        assert_eq!(standalone[5], PRACTICE_BOARD_DELETION);
    }
}
