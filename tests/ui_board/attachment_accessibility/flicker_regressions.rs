//! First-frame attachment health and editor-history flicker regressions.

use super::{Fixture, attachment_batch, attachment_payload, complete, draw, execute_palette, text};
use crate::agent::target;
use proqi::{
    application::Effect,
    domain::Direction,
    ports::attachment_accessibility::AttachmentAccessFailure,
    ui::{UiInput, UiKey},
};

#[test]
fn modeled_macos_file_drop_is_neutral_while_fresh_submission_preflight_is_mandatory() {
    let files = tempfile::tempdir().expect("temporary files");
    let file = files.path().join("Grüße 第一.png");
    std::fs::write(&file, b"readable image fixture").expect("readable attachment");
    let path = file.to_string_lossy();
    let mut fixture = Fixture::new();

    let insertion = fixture.effects(UiInput::PasteAnnotated(attachment_payload(&path, true)));
    let background = attachment_batch(&insertion);
    assert_normal_attachment(&mut fixture, "[Image 1]");
    acknowledge_persistence(&mut fixture, &insertion);

    fixture.input(UiInput::Key(UiKey::Escape));
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target(Direction::Left, "w1:p2")]));
    let waiting = execute_palette(&mut fixture, "submit and keep");
    assert!(
        waiting.is_empty(),
        "background proof is not submission proof"
    );
    assert_eq!(fixture.app.status_text(), Some("checking attachments"));

    let preflight = fixture
        .app
        .complete_attachment_checks(complete(background, Ok(())));
    assert!(
        preflight
            .iter()
            .all(|effect| !matches!(effect, Effect::PrepareSubmission(_)))
    );
    let preflight = attachment_batch(&preflight);
    let prepared = fixture
        .app
        .complete_attachment_checks(complete(preflight, Ok(())));
    assert!(matches!(
        prepared.as_slice(),
        [Effect::PrepareSubmission(_)]
    ));
}

#[test]
fn deleting_a_complete_placeholder_then_undoing_restores_neutral_pending_health() {
    let path = "/tmp/history-restored.png";
    let mut fixture = Fixture::new();
    let insertion = fixture.effects(UiInput::PasteAnnotated(attachment_payload(path, true)));
    let background = attachment_batch(&insertion);
    fixture
        .app
        .complete_attachment_checks(complete(background, Ok(())));
    acknowledge_persistence(&mut fixture, &insertion);

    fixture.input(UiInput::Key(UiKey::SelectAll));
    fixture.input(UiInput::Key(UiKey::Delete));
    let deletion = fixture
        .app
        .flush_pending_edit(&mut fixture.ids, &fixture.clock);
    assert!(
        fixture.app.state.board.live_thoughts()[0]
            .annotations
            .is_empty()
    );
    acknowledge_persistence(&mut fixture, &deletion);

    let undo = fixture.effects(UiInput::Key(UiKey::Undo));
    let recheck = attachment_batch(&undo);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].annotations.len(),
        1
    );
    assert_normal_attachment(&mut fixture, "[Image 1]");
    acknowledge_persistence(&mut fixture, &undo);

    fixture.input(UiInput::Key(UiKey::Escape));
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target(Direction::Left, "w1:p2")]));
    assert!(execute_palette(&mut fixture, "submit and keep").is_empty());
    let preflight = fixture
        .app
        .complete_attachment_checks(complete(recheck, Ok(())));
    let preflight = attachment_batch(&preflight);
    assert!(
        fixture
            .app
            .complete_attachment_checks(complete(preflight, Err(AttachmentAccessFailure::Missing)))
            .is_empty()
    );
    assert_eq!(
        fixture.app.status_text(),
        Some("Proqi cannot access 1 attachment")
    );
    assert!(
        text(draw(&mut fixture, 60, 8).backend().buffer()).contains("[Image 1 · inaccessible]")
    );
}

fn assert_normal_attachment(fixture: &mut Fixture, label: &str) {
    let rendered = text(draw(fixture, 60, 8).backend().buffer());
    assert!(rendered.contains(label), "rendered frame:\n{rendered}");
    assert!(
        !rendered.contains("inaccessible"),
        "rendered frame:\n{rendered}"
    );
}

fn acknowledge_persistence(fixture: &mut Fixture, effects: &[Effect]) {
    let sequence = effects
        .iter()
        .find_map(Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("persistence sequence");
    assert!(
        fixture
            .app
            .acknowledge_persistence(sequence, true)
            .is_empty()
    );
}
