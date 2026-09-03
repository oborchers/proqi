use super::*;

use proqi::{
    domain::Direction,
    ports::attachment_accessibility::{
        AttachmentAccessFailure, AttachmentCheckBatch, AttachmentCheckBatchResult,
        AttachmentCheckResult,
    },
};
use ratatui_core::style::Modifier;

#[path = "attachment_accessibility/flicker_regressions.rs"]
mod flicker_regressions;
#[path = "attachment_accessibility/frame_presentation.rs"]
mod frame_presentation;
#[path = "attachment_accessibility/review.rs"]
mod review;
#[path = "attachment_accessibility/transformations.rs"]
mod transformations;

#[test]
fn inaccessible_image_and_file_use_exact_plain_labels_and_warning_semantics_after_resize() {
    for (image, label) in [
        (true, "[Image 1 · inaccessible]"),
        (false, "[File 1 · inaccessible]"),
    ] {
        let mut fixture = Fixture::new();
        let path = "/private/var/folders/TemporaryItems/Grüße 第一.png";
        let effects = fixture.effects(UiInput::PasteAnnotated(attachment_payload(path, image)));
        let batch = attachment_batch(&effects);
        let checking = text(draw(&mut fixture, 80, 8).backend().buffer());
        assert!(checking.contains(if image { "[Image 1]" } else { "[File 1]" }));
        assert!(!checking.contains("inaccessible"));
        let completion = complete(batch, Err(AttachmentAccessFailure::Missing));
        assert!(
            fixture
                .app
                .complete_attachment_checks(completion)
                .is_empty()
        );

        for width in [80, 31] {
            let terminal = draw_theme(&mut fixture, width, 8, ThemePreference::Dark);
            let rendered = text(terminal.backend().buffer());
            assert!(rendered.contains(label));
            assert!(!rendered.contains("TemporaryItems"));
            let area = fixture
                .app
                .prepare_frame(Rect::new(0, 0, width, 8))
                .thoughts[0]
                .text_area;
            let cell = &terminal.backend().buffer()[(area.x, area.y)];
            assert_eq!(cell.fg, Theme::resolve(ThemePreference::Dark, true).warning);
            assert!(cell.modifier.contains(Modifier::BOLD));
            assert!(!cell.modifier.contains(Modifier::CROSSED_OUT));
        }
    }
}

#[test]
fn manual_refresh_recovers_a_restored_unicode_path_and_is_present_in_commands() {
    let mut fixture = Fixture::new();
    let path = "/Volumes/Archiv/Grüße 第一.txt";
    let effects = fixture.effects(UiInput::PasteAnnotated(attachment_payload(path, false)));
    let batch = attachment_batch(&effects);
    let completion = complete(batch, Err(AttachmentAccessFailure::Unmounted));
    fixture.app.complete_attachment_checks(completion);
    assert!(text(draw(&mut fixture, 60, 8).backend().buffer()).contains("inaccessible"));

    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "refresh attachments".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let entries = fixture.app.palette_view().expect("palette").1;
    assert_eq!(entries, ["Refresh attachments"]);
    let refresh = fixture.effects(UiInput::Key(UiKey::Enter));
    let batch = attachment_batch(&refresh);
    fixture
        .app
        .complete_attachment_checks(complete(batch, Ok(())));
    let rendered = text(draw(&mut fixture, 60, 8).backend().buffer());
    assert!(rendered.contains("[File 1]"));
    assert!(!rendered.contains("inaccessible"));
}

#[test]
fn resize_cursor_and_passive_pointer_events_never_repeat_filesystem_work() {
    let mut fixture = Fixture::new();
    let effects = fixture.effects(UiInput::PasteAnnotated(attachment_payload(
        "/tmp/Grüße 第一.txt",
        false,
    )));
    let batch = attachment_batch(&effects);
    fixture
        .app
        .complete_attachment_checks(complete(batch, Ok(())));

    for input in [
        UiInput::Resize {
            width: 31,
            height: 8,
        },
        UiInput::Key(UiKey::Move {
            movement: proqi::ports::editor::CursorMovement::GraphemeBack,
            extend_selection: false,
        }),
        UiInput::Pointer(PointerInput {
            column: 10,
            row: 2,
            kind: PointerKind::Move,
            extend_selection: false,
        }),
    ] {
        assert!(
            fixture
                .effects(input)
                .iter()
                .all(|effect| !matches!(effect, Effect::CheckAttachments(_)))
        );
    }
    for (width, height) in [(12, 4), (80, 8), (31, 6), (60, 8)] {
        let _terminal = draw(&mut fixture, width, height);
    }
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].annotations.len(),
        1
    );
}

#[test]
fn every_submit_variant_fails_before_journaling_delivery_or_removal() {
    for query in [
        "submit",
        "submit and keep",
        "submit all",
        "submit all and keep",
    ] {
        let mut fixture = if query.contains("all") {
            restarted_submission_fixture()
        } else {
            submission_fixture()
        };
        if !query.contains("all") {
            fixture.input(UiInput::Key(UiKey::Character('k')));
        }
        let effects = execute_palette(&mut fixture, query);
        let preflight = attachment_batch(&effects);
        assert_eq!(fixture.app.status_text(), Some("checking attachments"));
        assert!(
            text(draw(&mut fixture, 80, 12).backend().buffer()).contains("[Image 1]"),
            "fresh preflight preserves the last presentation verdict: {query}"
        );
        let effects = fixture.app.complete_attachment_checks(complete(
            preflight,
            Err(AttachmentAccessFailure::TimedOut),
        ));
        assert!(
            effects.is_empty(),
            "no journal or delivery effect: {effects:?}"
        );
        assert_eq!(
            fixture.app.status_text(),
            Some("Proqi cannot access 1 attachment")
        );
        assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
        assert!(
            fixture
                .app
                .state
                .board
                .live_thoughts()
                .iter()
                .all(|thought| !fixture.app.state.thought_locked(thought.id))
        );
    }
}

#[test]
fn persistence_failure_prevents_attachment_preflight_and_releases_sources() {
    let mut fixture = restarted_submission_fixture();
    fixture.input(UiInput::Key(UiKey::Character('e')));
    fixture.input(UiInput::Key(UiKey::Character('!')));
    let (commit, submission) = execute_palette_from_edit(&mut fixture, "submit all");
    assert!(submission.is_empty());
    let sequence = commit
        .iter()
        .find_map(Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("pending edit commit");
    assert!(
        commit
            .iter()
            .all(|effect| !matches!(effect, Effect::CheckAttachments(_)))
    );

    assert!(
        fixture
            .app
            .acknowledge_persistence(sequence, false)
            .is_empty()
    );
    assert!(fixture.app.status_text().is_some_and(|status| {
        status.starts_with("Submission not started because changes were not saved.")
    }));
    assert!(
        fixture
            .app
            .state
            .board
            .live_thoughts()
            .iter()
            .all(|thought| !fixture.app.state.thought_locked(thought.id))
    );
}

#[test]
fn accessible_preflight_preserves_direct_herdr_submission_and_source_changes_fail_closed() {
    let mut fixture = submission_fixture();
    let effects = execute_palette(&mut fixture, "submit all and keep");
    let preflight = attachment_batch(&effects);
    let preparation = fixture
        .app
        .complete_attachment_checks(complete(preflight, Ok(())));
    let [Effect::PrepareSubmission(_)] = preparation.as_slice() else {
        panic!("accessible preflight must prepare the existing journal: {preparation:?}");
    };
    let request = super::agent::start_submission(&mut fixture, &preparation);
    assert!(
        request
            .content
            .contains("/private/TemporaryItems/expired.png")
    );

    let mut changed = submission_fixture();
    let effects = execute_palette(&mut changed, "submit all");
    let preflight = attachment_batch(&effects);
    let source = changed.app.state.board.live_thoughts()[0].id;
    changed
        .app
        .state
        .board
        .thought_mut(source)
        .expect("source")
        .content
        .push('x');
    assert!(
        changed
            .app
            .complete_attachment_checks(complete(preflight, Ok(())))
            .is_empty()
    );
    assert_eq!(
        changed.app.status_text(),
        Some("board changed during attachment check; thoughts kept")
    );
    assert_eq!(changed.app.state.board.live_thoughts().len(), 2);

    let mut annotation_changed = submission_fixture();
    let effects = execute_palette(&mut annotation_changed, "submit all");
    let preflight = attachment_batch(&effects);
    let source = annotation_changed.app.state.board.live_thoughts()[0].id;
    let annotation = &mut annotation_changed
        .app
        .state
        .board
        .thought_mut(source)
        .expect("source")
        .annotations[0];
    let ContentAnnotationKind::Attachment { display_name, .. } = &mut annotation.kind else {
        panic!("attachment annotation");
    };
    *display_name = "changed.png".to_owned();
    assert!(
        annotation_changed
            .app
            .complete_attachment_checks(complete(preflight, Ok(())))
            .is_empty()
    );
    assert_eq!(
        annotation_changed.app.status_text(),
        Some("board changed during attachment check; thoughts kept")
    );
}

fn submission_fixture() -> Fixture {
    let mut fixture = Fixture::new();
    let path = "/private/TemporaryItems/expired.png";
    let effects = fixture.effects(UiInput::PasteAnnotated(attachment_payload(path, true)));
    let sequence = effects
        .iter()
        .find_map(Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("attachment commit");
    fixture.app.acknowledge_persistence(sequence, true);
    let background = attachment_batch(&effects);
    fixture
        .app
        .complete_attachment_checks(complete(background, Ok(())));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.paste("second source");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.acknowledge_all_persistence();
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Left, "w1:p2")]));
    fixture
}

fn restarted_submission_fixture() -> Fixture {
    let mut fixture = Fixture::new();
    fixture.paste("first source");
    fixture.input(UiInput::Key(UiKey::Escape));
    let path = "/private/var/folders/TemporaryItems/expired.png";
    fixture.input(UiInput::PasteAnnotated(attachment_payload(path, true)));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.acknowledge_all_persistence();

    let board = fixture.app.state.board.clone();
    let mut restarted = Fixture {
        app: BoardApp::with_settings(
            AppState::new(board),
            UiSettings::default(),
            proqi::adapters::editor::RopeEditorFactory,
        ),
        ids: fixture.ids,
        clock: fixture.clock,
    };
    assert_eq!(
        restarted.app.state.focused_thought,
        restarted
            .app
            .state
            .board
            .live_thoughts()
            .first()
            .map(|thought| thought.id)
    );
    let startup = restarted
        .app
        .start_attachment_checks(std::time::Duration::ZERO);
    let batch = attachment_batch(&startup);
    restarted
        .app
        .complete_attachment_checks(complete(batch, Ok(())));
    restarted
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Left, "w1:p2")]));
    restarted
}

fn execute_palette_from_edit(fixture: &mut Fixture, query: &str) -> (Vec<Effect>, Vec<Effect>) {
    let commit = fixture.effects(UiInput::Key(UiKey::Escape));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 80, 12));
    let commands = layout
        .controls
        .iter()
        .find_map(|(target, area)| (*target == HitTarget::Commands).then_some(*area))
        .expect("command palette control");
    fixture.input(UiInput::Pointer(PointerInput {
        column: commands.x,
        row: commands.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    for character in query.chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let submission = fixture.effects(UiInput::Key(UiKey::Enter));
    (commit, submission)
}

fn execute_palette(fixture: &mut Fixture, query: &str) -> Vec<Effect> {
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in query.chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    fixture.effects(UiInput::Key(UiKey::Enter))
}

fn attachment_payload(path: &str, image: bool) -> PastePayload {
    PastePayload::annotated(
        path.to_owned(),
        vec![ContentAnnotation {
            start: 0,
            end: path.len(),
            kind: ContentAnnotationKind::Attachment {
                image,
                display_name: path.rsplit('/').next().unwrap_or(path).to_owned(),
            },
        }],
    )
    .expect("valid attachment payload")
}

fn attachment_batch(effects: &[Effect]) -> AttachmentCheckBatch {
    effects
        .iter()
        .find_map(|effect| match effect {
            Effect::CheckAttachments(batch) => Some(batch.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("attachment batch missing: {effects:?}"))
}

fn complete(
    batch: AttachmentCheckBatch,
    result: Result<(), AttachmentAccessFailure>,
) -> AttachmentCheckBatchResult {
    AttachmentCheckBatchResult {
        id: batch.id,
        purpose: batch.purpose,
        results: batch
            .checks
            .into_iter()
            .map(|key| AttachmentCheckResult { key, result })
            .collect(),
    }
}
