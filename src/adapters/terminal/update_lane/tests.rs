use std::str::FromStr as _;

use crate::{
    application::UpdateIntent,
    domain::{ReleaseHighlightAnnouncement, SessionId, StableVersion},
};

use super::{UpdateActionResult, concludes_prompt, highlight_acknowledgement_result};

fn version(value: &str) -> StableVersion {
    StableVersion::parse(value).expect("stable version")
}

#[test]
fn absent_acknowledgement_state_closes_quietly_but_write_failure_remains_visible() {
    assert!(matches!(
        highlight_acknowledgement_result(Ok(false)),
        UpdateActionResult::HighlightsAcknowledged(Ok(()))
    ));
    assert!(matches!(
        highlight_acknowledgement_result(Err(crate::ports::update::UpdateError::State(
            "injected".to_owned()
        ))),
        UpdateActionResult::HighlightsAcknowledged(Err(crate::ports::update::UpdateError::State(
            _
        )))
    ));
}

#[test]
fn highlight_acknowledgement_never_releases_an_actionable_update_prompt() {
    let announcement = ReleaseHighlightAnnouncement::pending(
        SessionId::from_str("ses_06g30t7dv5qv55n1ppn3clis3k").expect("session"),
        version("1.0.0"),
        version("1.1.0"),
    )
    .expect("announcement");

    assert!(!concludes_prompt(
        &UpdateIntent::AcknowledgeReleaseHighlights(announcement)
    ));
    assert!(!concludes_prompt(&UpdateIntent::CheckNow));
    for intent in [
        UpdateIntent::Dismiss(version("1.1.0")),
        UpdateIntent::Skip(version("1.1.0")),
        UpdateIntent::ViewInstructions(version("1.1.0")),
        UpdateIntent::Install(version("1.1.0")),
    ] {
        assert!(concludes_prompt(&intent));
    }
}
