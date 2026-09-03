//! Canonical first-run practice-board policy, copy, and semantic shortcut emphasis.

use crate::{
    domain::{DomainError, Session, SessionBoard, Timestamp},
    ports::{
        environment::IdGenerator,
        store::{FirstRunBoard, OnboardingVersion},
    },
};

use super::{
    AppState, ApplicationError, ApplicationResult, Effect, PrimaryKeyPlatform,
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
    first_run_board_for_platform(session, ids, environment, PrimaryKeyPlatform::current())
}

fn first_run_board_for_platform(
    session: Session,
    ids: &mut impl IdGenerator,
    environment: FirstRunEnvironment,
    platform: PrimaryKeyPlatform,
) -> ApplicationResult<FirstRunBoard> {
    let now = session.created_at;
    let empty = SessionBoard::new(session, Vec::new())?;
    let mut state = AppState::new(empty);
    let instructions = instructions(environment, platform)?;
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

fn instructions(
    environment: FirstRunEnvironment,
    platform: PrimaryKeyPlatform,
) -> Result<[InstructionalText; 6], DomainError> {
    let delete_line = platform.label("U");
    let delete_sentence = platform.label("Shift+U");
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
        .shortcut(&delete_line)?
        .text(" to delete this logical line. Press ")
        .shortcut(&delete_sentence)?
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
    let integration = integration_instruction(environment, platform)?;
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
    platform: PrimaryKeyPlatform,
) -> Result<InstructionalText, DomainError> {
    let submit = platform.label("Enter");
    let submit_keep = platform.label("Shift+Enter");
    match environment {
        FirstRunEnvironment::HerdrManaged => InstructionalTextBuilder::new()
            .text("/plan starts a planning prompt when a compatible adjacent Codex or Claude Code agent is verified.\n\nHerdr is detected. With a compatible adjacent agent verified, press ")
            .shortcut("s")?
            .text(" to submit and remove or ")
            .shortcut("S")?
            .text(" to keep in board mode. In edit mode, use ")
            .shortcut(&submit)?
            .text(" to submit and remove or ")
            .shortcut(&submit_keep)?
            .text(" to keep. Learn more at https://herdr.dev")
            .finish(),
        FirstRunEnvironment::Standalone => InstructionalTextBuilder::new()
            .text("Proqi works on its own. Herdr adds verified adjacent submission. In board mode, use ")
            .shortcut("s")?
            .text(" to submit and remove or ")
            .shortcut("S")?
            .text(" to keep. In edit mode, use ")
            .shortcut(&submit)?
            .text(" to submit and remove or ")
            .shortcut(&submit_keep)?
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

    #[test]
    fn variants_have_six_exact_ordered_platform_specific_thoughts() {
        for platform in [PrimaryKeyPlatform::MacOs, PrimaryKeyPlatform::Portable] {
            for environment in [
                FirstRunEnvironment::HerdrManaged,
                FirstRunEnvironment::Standalone,
            ] {
                let board = board(environment, platform);
                assert_eq!(contents(&board), expected_contents(environment, platform));
                assert!(
                    contents(&board)
                        .iter()
                        .all(|content| !content.contains("Primary+"))
                );
            }
        }
    }

    #[test]
    fn reviewed_shortcut_literals_are_the_only_semantic_emphasis() {
        for platform in [PrimaryKeyPlatform::MacOs, PrimaryKeyPlatform::Portable] {
            let expected = expected_shortcuts(platform);
            for environment in [
                FirstRunEnvironment::HerdrManaged,
                FirstRunEnvironment::Standalone,
            ] {
                assert_eq!(shortcut_literals(&board(environment, platform)), expected);
            }
        }
    }

    fn board(environment: FirstRunEnvironment, platform: PrimaryKeyPlatform) -> SessionBoard {
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-onboarding-copy"),
            Timestamp::from_millis(1),
        )
        .expect("session");
        first_run_board_for_platform(session, &mut ids, environment, platform)
            .expect("practice board")
            .board()
            .clone()
    }

    fn expected_contents(
        environment: FirstRunEnvironment,
        platform: PrimaryKeyPlatform,
    ) -> Vec<String> {
        let delete_line = platform.label("U");
        let delete_sentence = platform.label("Shift+U");
        let submit = platform.label("Enter");
        let submit_keep = platform.label("Shift+Enter");
        let integration = match environment {
            FirstRunEnvironment::HerdrManaged => format!(
                "/plan starts a planning prompt when a compatible adjacent Codex or Claude Code agent is verified.\n\nHerdr is detected. With a compatible adjacent agent verified, press s to submit and remove or S to keep in board mode. In edit mode, use {submit} to submit and remove or {submit_keep} to keep. Learn more at https://herdr.dev"
            ),
            FirstRunEnvironment::Standalone => format!(
                "Proqi works on its own. Herdr adds verified adjacent submission. In board mode, use s to submit and remove or S to keep. In edit mode, use {submit} to submit and remove or {submit_keep} to keep. Learn more at https://herdr.dev"
            ),
        };
        vec![
            "Welcome to Proqi!\n\nProqi is a prompt composer designed to replace common agent input methods. Capture, refine, organize, and submit prompts here.".to_owned(),
            format!("Press Enter to edit the focused thought. Press Esc to return to board mode.\n\n- Press Enter to continue this unordered list. Press {delete_line} to delete this logical line. Press {delete_sentence} to delete this sentence."),
            "Press n to create a new thought, or paste in board mode to create one from the pasted text. Press y to copy the focused thought.\n\nIn edit mode, type $name, /name, or supported @name to complete discovered local invocations.".to_owned(),
            "Use j or ↓ to move to the next thought, and k or ↑ to move to the previous one. Press Space to select, d to delete, and u to undo.".to_owned(),
            integration,
            "Press a and d in board mode to delete this entire practice board.".to_owned(),
        ]
    }

    fn expected_shortcuts(platform: PrimaryKeyPlatform) -> Vec<String> {
        [
            "Enter".to_owned(),
            "Esc".to_owned(),
            "Enter".to_owned(),
            platform.label("U"),
            platform.label("Shift+U"),
            "n".to_owned(),
            "y".to_owned(),
            "j".to_owned(),
            "↓".to_owned(),
            "k".to_owned(),
            "↑".to_owned(),
            "Space".to_owned(),
            "d".to_owned(),
            "u".to_owned(),
            "s".to_owned(),
            "S".to_owned(),
            platform.label("Enter"),
            platform.label("Shift+Enter"),
            "a".to_owned(),
            "d".to_owned(),
        ]
        .into()
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
