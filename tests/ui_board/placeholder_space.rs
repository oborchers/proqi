//! Placeholder-aware Space behavior across semantic projection and durable editing.

use proqi::{
    application::{DurabilityState, Effect},
    domain::{ContentAnnotation, ContentAnnotationKind, TextPosition},
    ports::{
        attachment_accessibility::{
            AttachmentAccessFailure, AttachmentCheckBatchResult, AttachmentCheckResult,
        },
        editor::CursorMovement,
    },
    ui::{PastePayload, PointerButton, PointerKind, ThemePreference, UiInput, UiKey},
};
use ratatui_core::layout::Rect;
use unicode_segmentation::UnicodeSegmentation as _;

use super::{Fixture, draw, draw_theme, snapshot_support::snapshot_buffer, text};

#[path = "placeholder_space/stress.rs"]
mod stress;

fn substitution(kind: ContentAnnotationKind, start: usize, end: usize) -> ContentAnnotation {
    ContentAnnotation { start, end, kind }
}

fn attachment(image: bool) -> ContentAnnotationKind {
    ContentAnnotationKind::Attachment {
        image,
        display_name: if image { "image.png" } else { "context.txt" }.to_owned(),
    }
}

fn annotated(prefix: &str, value: &str, suffix: &str, kind: ContentAnnotationKind) -> Fixture {
    let content = format!("{prefix}{value}{suffix}");
    Fixture::with_annotated_thought(
        &content,
        vec![substitution(kind, prefix.len(), prefix.len() + value.len())],
    )
}

fn move_key(movement: CursorMovement, extend_selection: bool) -> UiInput {
    UiInput::Key(UiKey::Move {
        movement,
        extend_selection,
    })
}

fn select_forward(fixture: &mut Fixture, prefix: &str) {
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(move_key(CursorMovement::DocumentStart, false));
    for _ in prefix.graphemes(true) {
        fixture.input(move_key(CursorMovement::GraphemeForward, false));
    }
    if fixture
        .app
        .editor_snapshot()
        .expect("editor")
        .selection
        .is_none()
    {
        fixture.input(move_key(CursorMovement::GraphemeForward, false));
    }
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("editor")
            .selection
            .is_some()
    );
}

fn select_reverse(fixture: &mut Fixture, suffix: &str) {
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(move_key(CursorMovement::DocumentEnd, false));
    for _ in suffix.graphemes(true) {
        fixture.input(move_key(CursorMovement::GraphemeBack, false));
    }
    if fixture
        .app
        .editor_snapshot()
        .expect("editor")
        .selection
        .is_none()
    {
        fixture.input(move_key(CursorMovement::GraphemeBack, false));
    }
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("editor")
            .selection
            .is_some()
    );
}

fn space_revision(fixture: &mut Fixture) -> proqi::domain::ThoughtRevision {
    let effects = fixture.effects(UiInput::Key(UiKey::UnmodifiedSpace));
    let [Effect::CommitRevision(revision)] = effects.as_slice() else {
        panic!("expected one durable revision, got {effects:?}");
    };
    revision.clone()
}

fn assert_shifted(
    fixture: &Fixture,
    revision: &proqi::domain::ThoughtRevision,
    before: &str,
    expected: &str,
    old_start: usize,
) {
    assert_eq!(revision.before_content, before);
    assert_eq!(revision.after_content, expected);
    assert_eq!(revision.before_annotations.len(), 1);
    assert_eq!(revision.after_annotations.len(), 1);
    assert_eq!(
        revision.after_annotations[0].kind,
        revision.before_annotations[0].kind
    );
    assert_eq!(revision.after_annotations[0].start, old_start + 1);
    assert_eq!(
        revision.after_annotations[0].end,
        revision.before_annotations[0].end + 1
    );
    let snapshot = fixture.app.editor_snapshot().expect("editor");
    assert_eq!(snapshot.content, expected);
    assert_eq!(snapshot.selection, None);
    assert_eq!(
        snapshot.cursor,
        TextPosition::new(0, expected[..=old_start].graphemes(true).count())
    );
}

#[test]
fn beginning_middle_end_and_reversed_placeholders_shift_exactly_once() {
    for (prefix, value, suffix, reverse) in [
        ("", "/tmp/start.png", " tail", false),
        ("Grüße ", "/tmp/middle.png", "\r\n界", false),
        ("head\t", "/tmp/end.png", "", true),
    ] {
        let before = format!("{prefix}{value}{suffix}");
        let expected = format!("{prefix} {value}{suffix}");
        let mut fixture = annotated(prefix, value, suffix, attachment(true));
        if reverse {
            select_reverse(&mut fixture, suffix);
        } else {
            select_forward(&mut fixture, prefix);
        }
        let revision = space_revision(&mut fixture);
        assert_shifted(&fixture, &revision, &before, &expected, prefix.len());
    }
}

#[test]
fn every_substitution_kind_and_adjacent_placeholders_use_the_same_projection_rule() {
    let kinds = [
        attachment(true),
        attachment(false),
        ContentAnnotationKind::LargePaste {
            lines: 14,
            graphemes: 1_234,
        },
        ContentAnnotationKind::InvocationReference {
            display_name: "@reviewer · codex".to_owned(),
        },
    ];
    for kind in kinds {
        let mut fixture = annotated("before", "\t\0e\u{301}👩🏽‍💻\r\n", "after", kind);
        select_forward(&mut fixture, "before");
        let revision = space_revision(&mut fixture);
        assert_eq!(revision.after_content, "before \t\0e\u{301}👩🏽‍💻\r\nafter");
        assert_eq!(revision.after_annotations[0].start, "before ".len());
    }

    let first = "/tmp/one.png";
    let second = "/tmp/two.png";
    let content = format!("{first}{second}");
    let mut fixture = Fixture::with_annotated_thought(
        &content,
        vec![
            substitution(attachment(true), 0, first.len()),
            substitution(attachment(true), first.len(), content.len()),
        ],
    );
    select_reverse(&mut fixture, "");
    let revision = space_revision(&mut fixture);
    assert_eq!(revision.after_content, format!("{first} {second}"));
    assert_eq!(revision.after_annotations[0].start, 0);
    assert_eq!(revision.after_annotations[1].start, first.len() + 1);
}

#[test]
fn repeated_space_keeps_the_placeholder_and_ordinary_followup_spaces() {
    let value = "/tmp/repeat.png";
    let mut fixture = annotated("", value, "", attachment(true));
    select_forward(&mut fixture, "");
    let first = space_revision(&mut fixture);
    assert_eq!(first.after_content, format!(" {value}"));
    assert!(
        fixture
            .effects(UiInput::Key(UiKey::UnmodifiedSpace))
            .is_empty()
    );
    assert!(
        fixture
            .effects(UiInput::Key(UiKey::UnmodifiedSpace))
            .is_empty()
    );
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        format!("   {value}")
    );
    let followup = fixture
        .app
        .flush_pending_edit(&mut fixture.ids, &fixture.clock);
    let [Effect::CommitRevision(revision)] = followup.as_slice() else {
        panic!("ordinary spaces coalesce through the existing typing policy");
    };
    assert_eq!(revision.before_content, format!(" {value}"));
    assert_eq!(revision.after_content, format!("   {value}"));
}

#[test]
fn delete_backspace_enter_replacement_and_paste_keep_existing_semantics() {
    for key in [UiKey::Delete, UiKey::Backspace] {
        let mut fixture = annotated("a", "TOKEN", "z", attachment(false));
        select_forward(&mut fixture, "a");
        fixture.input(UiInput::Key(key));
        let effects = fixture
            .app
            .flush_pending_edit(&mut fixture.ids, &fixture.clock);
        let [Effect::CommitRevision(revision)] = effects.as_slice() else {
            panic!("one deletion revision");
        };
        assert_eq!(revision.after_content, "az");
        assert!(revision.after_annotations.is_empty());
    }

    let mut entered = annotated("a", "TOKEN", "z", attachment(false));
    select_forward(&mut entered, "a");
    assert!(entered.effects(UiInput::Key(UiKey::Enter)).is_empty());
    assert_eq!(
        entered.app.editor_snapshot().expect("editor").content,
        "aTOKENz"
    );
    assert!(text(draw(&mut entered, 40, 8).backend().buffer()).contains("TOKEN"));

    for input in [
        UiInput::Key(UiKey::Character('x')),
        UiInput::Key(UiKey::Character(' ')),
        UiInput::Paste("first\r\nsecond".to_owned()),
    ] {
        let mut fixture = annotated("a", "TOKEN", "z", attachment(false));
        select_forward(&mut fixture, "a");
        let mut effects = fixture.effects(input);
        if effects.is_empty() {
            effects = fixture
                .app
                .flush_pending_edit(&mut fixture.ids, &fixture.clock);
        }
        assert!(
            fixture
                .app
                .editor_snapshot()
                .expect("editor")
                .selection
                .is_none()
        );
        let [Effect::CommitRevision(revision)] = effects.as_slice() else {
            panic!("one replacement revision");
        };
        assert!(revision.after_annotations.is_empty());
    }
}

#[test]
fn partial_wide_multi_expanded_inline_and_plain_selections_are_not_eligible() {
    let mut partial = annotated("", "TOKEN", "", attachment(false));
    partial.input(UiInput::Key(UiKey::Enter));
    partial.input(move_key(CursorMovement::DocumentStart, false));
    partial.input(move_key(CursorMovement::GraphemeForward, true));
    partial.input(UiInput::Key(UiKey::UnmodifiedSpace));
    assert_eq!(
        partial.app.editor_snapshot().expect("editor").content,
        " OKEN"
    );
    let partial = partial
        .app
        .flush_pending_edit(&mut partial.ids, &partial.clock);
    let [Effect::CommitRevision(partial)] = partial.as_slice() else {
        panic!("partial selection follows ordinary replacement");
    };
    assert!(partial.after_annotations.is_empty());

    let first = "ONE";
    let second = "TWO";
    let mut wide = Fixture::with_annotated_thought(
        "pONEmTWOs",
        vec![
            substitution(attachment(false), 1, 1 + first.len()),
            substitution(attachment(false), 5, 5 + second.len()),
        ],
    );
    wide.input(UiInput::Key(UiKey::Enter));
    wide.input(move_key(CursorMovement::DocumentStart, false));
    wide.input(move_key(CursorMovement::DocumentEnd, true));
    wide.input(UiInput::Key(UiKey::UnmodifiedSpace));
    assert_eq!(wide.app.editor_snapshot().expect("editor").content, " ");

    let mut expanded = annotated("", "TOKEN", "", attachment(false));
    select_forward(&mut expanded, "");
    expanded.input(UiInput::Key(UiKey::Enter));
    expanded.input(move_key(CursorMovement::DocumentStart, false));
    expanded.input(move_key(CursorMovement::DocumentEnd, true));
    expanded.input(UiInput::Key(UiKey::UnmodifiedSpace));
    assert_eq!(expanded.app.editor_snapshot().expect("editor").content, " ");

    let inline: ContentAnnotation = serde_json::from_value(serde_json::json!({
        "start": 0, "end": 5, "kind": { "kind": "shortcut_emphasis" }
    }))
    .expect("inline fixture");
    for (content, annotations) in [
        ("Enter", vec![inline]),
        ("[Image 1]", Vec::new()),
        ("https://example.test", Vec::new()),
    ] {
        let mut fixture = Fixture::with_annotated_thought(content, annotations);
        fixture.input(UiInput::Key(UiKey::Enter));
        fixture.input(UiInput::Key(UiKey::SelectAll));
        fixture.input(UiInput::Key(UiKey::UnmodifiedSpace));
        assert_eq!(fixture.app.editor_snapshot().expect("editor").content, " ");
    }
}

#[test]
fn board_compose_and_search_retain_their_space_behavior() {
    let mut board =
        Fixture::with_annotated_thought("TOKEN", vec![substitution(attachment(false), 0, 5)]);
    assert!(
        board
            .effects(UiInput::Key(UiKey::UnmodifiedSpace))
            .is_empty()
    );
    assert!(
        board
            .app
            .thought_selected(board.app.state.board.live_thoughts()[0].id)
    );

    let mut compose = Fixture::new();
    let effects = compose.effects(UiInput::Key(UiKey::UnmodifiedSpace));
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(compose.app.editor_snapshot().expect("editor").content, " ");

    let mut search = Fixture::new();
    search.paste("alpha beta");
    search.input(UiInput::Key(UiKey::Escape));
    search.input(UiInput::Key(UiKey::Character('/')));
    search.input(UiInput::Key(UiKey::UnmodifiedSpace));
    assert_eq!(search.app.search_view().expect("search").0, " ");
}

#[test]
fn inaccessible_mouse_selection_survives_resize_and_shifts_without_recheck() {
    let path = "/tmp/placeholder-space-missing.png";
    let mut fixture = Fixture::new();
    let insertion = fixture.effects(UiInput::PasteAnnotated(
        PastePayload::annotated(
            path.to_owned(),
            vec![substitution(attachment(true), 0, path.len())],
        )
        .expect("payload"),
    ));
    let batch = insertion
        .iter()
        .find_map(|effect| match effect {
            Effect::CheckAttachments(batch) => Some(batch.clone()),
            _ => None,
        })
        .expect("attachment check");
    fixture
        .app
        .complete_attachment_checks(AttachmentCheckBatchResult {
            id: batch.id,
            purpose: batch.purpose,
            results: batch
                .checks
                .into_iter()
                .map(|key| AttachmentCheckResult {
                    key,
                    result: Err(AttachmentAccessFailure::Missing),
                })
                .collect(),
        });
    let _wide = draw_theme(&mut fixture, 60, 8, ThemePreference::Dark);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 60, 8)).thoughts[0].text_area;
    fixture.pointer(area.x + 2, area.y, PointerKind::Down(PointerButton::Left));
    fixture.input(UiInput::Resize {
        width: 22,
        height: 5,
    });
    let _narrow = draw_theme(&mut fixture, 22, 5, ThemePreference::Dark);
    let effects = fixture.effects(UiInput::Key(UiKey::UnmodifiedSpace));
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, Effect::CheckAttachments(_)))
    );
    let [Effect::CommitRevision(revision)] = effects.as_slice() else {
        panic!("one shifted revision");
    };
    assert_eq!(revision.after_content, format!(" {path}"));
    let rendered = text(draw(&mut fixture, 40, 8).backend().buffer());
    assert!(rendered.contains("[Image 1 · inaccessible]"));
}

#[test]
fn failure_retry_undo_and_redo_keep_one_revision_and_exact_metadata() {
    let value = "/tmp/history.png";
    let mut fixture = annotated("a", value, "z", attachment(true));
    select_forward(&mut fixture, "a");
    let revision = space_revision(&mut fixture);
    fixture
        .app
        .acknowledge_persistence(revision.sequence, false);
    assert!(matches!(
        fixture.app.state.durability,
        DurabilityState::Failed { .. }
    ));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        format!("a {value}z")
    );
    assert_eq!(
        fixture.effects(UiInput::Key(UiKey::Character('r'))),
        vec![Effect::RetryPersistence {
            sequence: revision.sequence,
        }]
    );
    fixture.app.acknowledge_persistence(revision.sequence, true);

    let undo = fixture.effects(UiInput::Key(UiKey::Undo));
    assert!(matches!(
        undo.as_slice(),
        [Effect::CommitHistoryMove { undo: true, .. }]
    ));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        format!("a{value}z")
    );
    let undo_sequence = undo[0]
        .persistence_batch()
        .and_then(|batch| batch.sequence())
        .expect("undo sequence");
    fixture.app.acknowledge_persistence(undo_sequence, true);
    let redo = fixture.effects(UiInput::Key(UiKey::Redo));
    assert!(matches!(
        redo.as_slice(),
        [Effect::CommitHistoryMove { undo: false, .. }]
    ));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        format!("a {value}z")
    );
}

#[test]
fn shifted_placeholder_has_a_reviewed_narrow_editor_snapshot() {
    let value = "/tmp/snapshot.png";
    let mut fixture = annotated("before ", value, " after", attachment(true));
    select_forward(&mut fixture, "before ");
    let _revision = space_revision(&mut fixture);
    let terminal = draw_theme(&mut fixture, 32, 7, ThemePreference::Dark);
    insta::assert_snapshot!(snapshot_buffer(terminal.backend().buffer()));
}
