//! Canonical first-run practice-board policy, copy, and semantic shortcut emphasis.

use crate::{
    domain::{DomainError, Session, SessionBoard, Timestamp},
    ports::{
        environment::IdGenerator,
        store::{FirstRunBoard, OnboardingVersion},
    },
};

use super::{
    AppState, ApplicationError, ApplicationResult, Effect,
    instructional_text::{InstructionalText, InstructionalTextBuilder},
    reduce,
};

/// Cheap, truthful local environment distinction used only to select practice copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirstRunEnvironment {
    /// The current Proqi pane carries Herdr's managed-pane signal.
    HerdrManaged,
    /// The current Proqi pane is not managed by Herdr.
    Standalone,
}

/// Build the current practice board from ordinary application-created thoughts.
///
/// # Errors
///
/// Returns an application error if the session, generated ordering, or reviewed
/// instructional annotations are invalid.
pub fn first_run_board(
    session: Session,
    ids: &mut impl IdGenerator,
    environment: FirstRunEnvironment,
) -> ApplicationResult<FirstRunBoard> {
    let now = session.created_at;
    let empty = SessionBoard::new(session, Vec::new())?;
    let mut state = AppState::new(empty);
    let instructions = instructions(environment)?;
    for (insertion_index, instruction) in instructions.into_iter().enumerate() {
        add_instruction(&mut state, ids, instruction, insertion_index, now)?;
    }
    Ok(FirstRunBoard::new(
        OnboardingVersion::PRACTICE_BOARD,
        state.board,
    ))
}

fn add_instruction(
    state: &mut AppState,
    ids: &mut impl IdGenerator,
    instruction: InstructionalText,
    insertion_index: usize,
    at: Timestamp,
) -> ApplicationResult<()> {
    let effects = reduce(
        state,
        instruction.create_action(
            ids.thought_id(),
            ids.operation_id(),
            Some(insertion_index),
            at,
        ),
    )?;
    if !matches!(effects.as_slice(), [Effect::CommitBoardOperation(_)]) {
        return Err(ApplicationError::InvalidState);
    }
    Ok(())
}

fn instructions(environment: FirstRunEnvironment) -> Result<[InstructionalText; 6], DomainError> {
    let welcome = InstructionalTextBuilder::new()
        .text("Welcome to Proqi!\n\nProqi is a prompt composer designed to replace common agent input methods. Capture, refine, organize, and submit prompts here.")
        .finish()?;
    let editing = InstructionalTextBuilder::new()
        .text("Press ")
        .shortcut("Enter")?
        .text(" to edit the focused thought. Press ")
        .shortcut("Esc")?
        .text(" to return to board mode.\n\n- Press ")
        .shortcut("Enter")?
        .text(" to continue this unordered list. Press ")
        .shortcut("Primary+U")?
        .text(" to delete this logical line. Press ")
        .shortcut("Primary+Shift+U")?
        .text(" to delete this sentence.")
        .finish()?;
    let creation = InstructionalTextBuilder::new()
        .text("Press ")
        .shortcut("n")?
        .text(
            " to create a new thought, or paste in board mode to create one from the pasted text.",
        )
        .text(" Press ")
        .shortcut("y")?
        .text(" to copy the focused thought.\n\nIn edit mode, type $name, /name, or supported @name to complete discovered local invocations.")
        .finish()?;
    let navigation = InstructionalTextBuilder::new()
        .text("Use ")
        .shortcut("j")?
        .text(" or ")
        .shortcut("↓")?
        .text(" to move to the next thought, and ")
        .shortcut("k")?
        .text(" or ")
        .shortcut("↑")?
        .text(" to move to the previous one. Press ")
        .shortcut("Space")?
        .text(" to select, ")
        .shortcut("d")?
        .text(" to delete, and ")
        .shortcut("u")?
        .text(" to undo.")
        .finish()?;
    let integration = integration_instruction(environment)?;
    let deletion = InstructionalTextBuilder::new()
        .text("Press ")
        .shortcut("a")?
        .text(" and ")
        .shortcut("d")?
        .text(" in board mode to delete this entire practice board.")
        .finish()?;
    Ok([
        welcome,
        editing,
        creation,
        navigation,
        integration,
        deletion,
    ])
}

fn integration_instruction(
    environment: FirstRunEnvironment,
) -> Result<InstructionalText, DomainError> {
    match environment {
        FirstRunEnvironment::HerdrManaged => InstructionalTextBuilder::new()
            .text("/plan starts a planning prompt when a compatible adjacent Codex or Claude Code agent is verified.\n\nHerdr is detected. With a compatible adjacent agent verified, press ")
            .shortcut("s")?
            .text(" to submit and remove or ")
            .shortcut("S")?
            .text(" to keep in board mode. In edit mode, use ")
            .shortcut("Primary+Enter")?
            .text(" to submit and remove or ")
            .shortcut("Primary+Shift+Enter")?
            .text(" to keep. Learn more at https://herdr.dev")
            .finish(),
        FirstRunEnvironment::Standalone => InstructionalTextBuilder::new()
            .text("Proqi works on its own. Herdr adds verified adjacent submission. In board mode, use ")
            .shortcut("s")?
            .text(" to submit and remove or ")
            .shortcut("S")?
            .text(" to keep. In edit mode, use ")
            .shortcut("Primary+Enter")?
            .text(" to submit and remove or ")
            .shortcut("Primary+Shift+Enter")?
            .text(" to keep. Learn more at https://herdr.dev")
            .finish(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        adapters::memory::FakeIdGenerator,
        domain::{AnnotationBehavior, InlineStyleKind},
    };

    const MANAGED_CONTENT: [&str; 6] = [
        "Welcome to Proqi!\n\nProqi is a prompt composer designed to replace common agent input methods. Capture, refine, organize, and submit prompts here.",
        "Press Enter to edit the focused thought. Press Esc to return to board mode.\n\n- Press Enter to continue this unordered list. Press Primary+U to delete this logical line. Press Primary+Shift+U to delete this sentence.",
        "Press n to create a new thought, or paste in board mode to create one from the pasted text. Press y to copy the focused thought.\n\nIn edit mode, type $name, /name, or supported @name to complete discovered local invocations.",
        "Use j or ↓ to move to the next thought, and k or ↑ to move to the previous one. Press Space to select, d to delete, and u to undo.",
        "/plan starts a planning prompt when a compatible adjacent Codex or Claude Code agent is verified.\n\nHerdr is detected. With a compatible adjacent agent verified, press s to submit and remove or S to keep in board mode. In edit mode, use Primary+Enter to submit and remove or Primary+Shift+Enter to keep. Learn more at https://herdr.dev",
        "Press a and d in board mode to delete this entire practice board.",
    ];
    const STANDALONE_INTEGRATION: &str = "Proqi works on its own. Herdr adds verified adjacent submission. In board mode, use s to submit and remove or S to keep. In edit mode, use Primary+Enter to submit and remove or Primary+Shift+Enter to keep. Learn more at https://herdr.dev";

    #[test]
    fn variants_have_six_exact_ordered_ordinary_thoughts() {
        let managed = board(FirstRunEnvironment::HerdrManaged);
        let standalone = board(FirstRunEnvironment::Standalone);
        assert_eq!(contents(&managed), MANAGED_CONTENT);
        let mut expected = MANAGED_CONTENT;
        expected[4] = STANDALONE_INTEGRATION;
        assert_eq!(contents(&standalone), expected);
    }

    #[test]
    fn reviewed_shortcut_literals_are_the_only_semantic_emphasis() {
        let managed = board(FirstRunEnvironment::HerdrManaged);
        let standalone = board(FirstRunEnvironment::Standalone);
        assert_eq!(
            shortcut_literals(&managed),
            [
                "Enter",
                "Esc",
                "Enter",
                "Primary+U",
                "Primary+Shift+U",
                "n",
                "y",
                "j",
                "↓",
                "k",
                "↑",
                "Space",
                "d",
                "u",
                "s",
                "S",
                "Primary+Enter",
                "Primary+Shift+Enter",
                "a",
                "d"
            ]
        );
        assert_eq!(
            shortcut_literals(&standalone),
            [
                "Enter",
                "Esc",
                "Enter",
                "Primary+U",
                "Primary+Shift+U",
                "n",
                "y",
                "j",
                "↓",
                "k",
                "↑",
                "Space",
                "d",
                "u",
                "s",
                "S",
                "Primary+Enter",
                "Primary+Shift+Enter",
                "a",
                "d"
            ]
        );
    }

    fn board(environment: FirstRunEnvironment) -> SessionBoard {
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-onboarding-copy"),
            Timestamp::from_millis(1),
        )
        .expect("session");
        first_run_board(session, &mut ids, environment)
            .expect("practice board")
            .board()
            .clone()
    }

    fn contents(board: &SessionBoard) -> Vec<&str> {
        board
            .live_thoughts()
            .iter()
            .map(|thought| thought.content.as_str())
            .collect()
    }

    fn shortcut_literals(board: &SessionBoard) -> Vec<&str> {
        board
            .live_thoughts()
            .iter()
            .flat_map(|thought| {
                thought.annotations.iter().map(|annotation| {
                    assert_eq!(
                        annotation.kind.behavior(),
                        AnnotationBehavior::InlineStyle(InlineStyleKind::ShortcutEmphasis)
                    );
                    &thought.content[annotation.start..annotation.end]
                })
            })
            .collect()
    }
}
