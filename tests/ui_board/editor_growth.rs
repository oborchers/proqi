//! Same-frame natural editor growth and board viewport reconciliation.

use proqi::{
    application::Effect,
    domain::{ContentAnnotation, ContentAnnotationKind},
    ports::attachment_accessibility::{
        AttachmentAccessFailure, AttachmentCheckBatchResult, AttachmentCheckResult,
    },
    ui::PastePayload,
};

use super::{Fixture, durable_thought, key_input, text};
use proqi::{
    domain::ThoughtPresentation,
    ui::{HitTarget, PointerButton, PointerKind, UiInput},
};
use proqi::{
    ports::editor::CursorMovement,
    ui::{Theme, ThemePreference, UiKey, render},
};
use ratatui_core::{
    backend::{Backend, TestBackend},
    layout::Rect,
    terminal::Terminal,
};

#[test]
fn final_editor_grows_in_one_frame_at_the_bottom() {
    let mut fixture = Fixture::new();
    for content in ["alpha", "beta", "gamma", "delta", "Line 1"] {
        durable_thought(&mut fixture, content);
    }
    let area = Rect::new(0, 0, 40, 10);
    fixture.app.prepare_frame(area);
    fixture.input(key_input(UiKey::Enter));
    fixture.input(key_input(UiKey::Move {
        movement: CursorMovement::DocumentEnd,
        extend_selection: false,
    }));
    let before = fixture.app.prepare_frame(area);
    let id = fixture.app.active_thought_id().expect("active thought");
    let before_text = before.thought(id).expect("visible").text_area;
    assert_eq!(before_text.height, 1);
    assert_eq!(before_text.bottom(), before.board.bottom());
    fixture.input(key_input(UiKey::Enter));
    for character in "Line 2".chars() {
        fixture.input(key_input(UiKey::Character(character)));
    }
    let layout = fixture.app.prepare_frame(area);
    let grown = layout.thought(id).expect("grown thought");
    assert_eq!(grown.text_area.height, 2);
    assert_eq!(grown.text_area.y + 1, before_text.y);
    assert_eq!(grown.text_area.bottom(), layout.board.bottom());
    assert_eq!(fixture.app.editor_snapshot().expect("editor").scroll_row, 0);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).expect("terminal");
    terminal
        .draw(|frame| {
            render(
                frame,
                &fixture.app,
                &layout,
                &Theme::resolve(ThemePreference::Auto, true),
            );
        })
        .expect("draw prepared frame");
    let rendered = text(terminal.backend().buffer());
    assert!(terminal.backend().cursor_visible());
    assert!(rendered.contains("Line 1"));
    assert!(rendered.contains("Line 2"));
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("cursor");
    assert_eq!(cursor.y, grown.text_area.y + 1);
    let separator = grown.separator_before.expect("separator");
    assert_eq!(separator.bottom(), grown.text_area.y);
    assert_eq!(layout.hit_test(separator.x, separator.y), None);
    insta::assert_snapshot!(
        "bottom_editor_natural_growth",
        super::snapshot_support::snapshot_buffer(terminal.backend().buffer())
    );
    assert_eq!(fixture.app.prepare_frame(area), layout);
}

fn editor_fixture(count: usize, index: usize, preference: ThoughtPresentation) -> Fixture {
    let mut fixture = Fixture::new();
    for _ in 0..count {
        durable_thought(&mut fixture, "seed");
    }
    let id = fixture.app.state.board.live_thoughts()[index].id;
    fixture.app.state.focused_thought = Some(id);
    fixture
        .app
        .state
        .board
        .thought_mut(id)
        .expect("thought")
        .presentation = preference;
    fixture.input(key_input(UiKey::Enter));
    fixture.input(key_input(UiKey::Move {
        movement: CursorMovement::DocumentEnd,
        extend_selection: false,
    }));
    fixture
}

fn assert_frame(fixture: &mut Fixture, area: Rect) -> proqi::ui::LayoutSnapshot {
    let layout = fixture.app.prepare_frame(area);
    let id = fixture.app.active_thought_id().expect("active");
    let thought = layout.thought(id).expect("visible editor");
    let editor = fixture.app.editor_snapshot().expect("editor");
    // These fixtures contain no substitutions, so canonical and visible rows agree.
    let top_padding =
        u16::from(area.height > 4 && fixture.app.state.board.live_thoughts().len() == 1);
    let capacity = usize::from(layout.board.height.saturating_sub(top_padding).max(1));
    assert_eq!(
        usize::from(thought.text_area.height),
        editor.visual_lines.len().min(capacity)
    );
    assert_eq!(editor.viewport.height, thought.text_area.height);
    if editor.visual_lines.len() <= capacity {
        assert_eq!(editor.scroll_row, 0);
        assert_eq!(thought.content_row_offset, 0);
    }
    assert!(thought.text_area.bottom() <= layout.board.bottom());
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).expect("terminal");
    terminal
        .draw(|frame| {
            render(
                frame,
                &fixture.app,
                &layout,
                &Theme::resolve(ThemePreference::Auto, true),
            );
        })
        .expect("render");
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("cursor");
    assert!(terminal.backend().cursor_visible());
    assert!(thought.text_area.contains(cursor));
    for row in thought.text_area.y..thought.text_area.bottom() {
        assert_eq!(
            layout.hit_test(thought.text_area.x, row),
            Some(HitTarget::Thought(id))
        );
    }
    assert_eq!(
        fixture.app.prepare_frame(area),
        layout,
        "no second-frame correction"
    );
    layout
}

#[test]
fn repeated_growth_shrink_and_resize_recompute_natural_and_capped_editors() {
    for preference in [
        ThoughtPresentation::Automatic,
        ThoughtPresentation::Collapsed,
        ThoughtPresentation::Expanded,
    ] {
        for (count, index) in [(1, 0), (9, 0), (9, 4), (9, 8)] {
            for area in [
                Rect::new(0, 0, 18, 6),
                Rect::new(0, 0, 40, 10),
                Rect::new(0, 0, 70, 24),
            ] {
                exercise_growth_cycle(count, index, preference, area);
            }
        }
    }
}

fn exercise_growth_cycle(count: usize, index: usize, preference: ThoughtPresentation, area: Rect) {
    let mut fixture = editor_fixture(count, index, preference);
    assert_frame(&mut fixture, area);
    for _ in 0..26 {
        fixture.input(key_input(UiKey::Enter));
        assert_frame(&mut fixture, area);
    }
    for _ in 0..26 {
        fixture.input(key_input(UiKey::Backspace));
        assert_frame(&mut fixture, area);
    }
    for resized in [Rect::new(0, 0, 8, 3), area, Rect::new(0, 0, 22, 8), area] {
        assert_frame(&mut fixture, resized);
    }
    fixture.input(key_input(UiKey::Escape));
    fixture.app.prepare_frame(area);
    fixture.input(key_input(UiKey::Enter));
    assert_frame(&mut fixture, area);
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "seed"
    );
}

#[test]
fn exact_paste_unicode_wrap_and_reverse_selection_share_grown_geometry() {
    for payload in [
        "\nnext\nlast",
        "\r\nnext\r\nlast",
        " 界界 e\u{301} 👩‍💻\tcontrol\u{1b} wraps around a narrow viewport",
    ] {
        let mut fixture = editor_fixture(9, 8, ThoughtPresentation::Collapsed);
        let area = Rect::new(0, 0, 18, 12);
        fixture.app.prepare_frame(area);
        fixture.input(UiInput::Paste(payload.to_owned()));
        assert_frame(&mut fixture, area);
        let exact = format!("seed{payload}");
        assert_eq!(
            fixture.app.editor_snapshot().expect("editor").content,
            exact
        );
        for (movement, extend_selection) in [
            (CursorMovement::DocumentStart, true),
            (CursorMovement::DocumentEnd, true),
            (CursorMovement::DocumentStart, false),
        ] {
            fixture.input(key_input(UiKey::Move {
                movement,
                extend_selection,
            }));
            assert_frame(&mut fixture, area);
        }
        fixture.input(key_input(UiKey::Undo));
        assert_frame(&mut fixture, area);
        assert_eq!(fixture.app.editor_snapshot().expect("undo").content, "seed");
        fixture.input(key_input(UiKey::Redo));
        assert_frame(&mut fixture, area);
        assert_eq!(fixture.app.editor_snapshot().expect("redo").content, exact);
    }
}

#[test]
fn every_grown_row_supports_mouse_placement_drag_and_wheel_isolation() {
    let mut fixture = editor_fixture(9, 8, ThoughtPresentation::Automatic);
    let area = Rect::new(0, 0, 40, 10);
    fixture.app.prepare_frame(area);
    fixture.input(UiInput::Paste("\nsecond\nthird".to_owned()));
    let layout = assert_frame(&mut fixture, area);
    let id = fixture.app.active_thought_id().expect("active");
    let text = layout.thought(id).expect("thought").text_area;
    for row in 0..3 {
        fixture.pointer(
            text.x + 1,
            text.y + row,
            PointerKind::Down(PointerButton::Left),
        );
        assert_eq!(
            fixture.app.editor_snapshot().expect("placed").cursor,
            proqi::domain::TextPosition::new(usize::from(row), 1)
        );
        assert_frame(&mut fixture, area);
        fixture.pointer(
            text.x + 2,
            text.y + row,
            PointerKind::Drag(PointerButton::Left),
        );
        assert!(
            fixture
                .app
                .editor_snapshot()
                .expect("dragged")
                .selection
                .is_some()
        );
        fixture.pointer(
            text.x + 2,
            text.y + row,
            PointerKind::Up(PointerButton::Left),
        );
        assert_frame(&mut fixture, area);
    }
    fixture.pointer(text.x, text.y, PointerKind::ScrollDown);
    let after = assert_frame(&mut fixture, area);
    assert_eq!(after.thought(id).expect("thought").text_area, text);
}

#[test]
fn grown_editor_survives_pending_failure_acknowledgement_and_reentry() {
    let mut fixture = editor_fixture(9, 8, ThoughtPresentation::Automatic);
    fixture.acknowledge_all_persistence();
    let area = Rect::new(0, 0, 40, 10);
    fixture.app.prepare_frame(area);
    let effects = fixture.effects(UiInput::Paste("\nsecond".to_owned()));
    let sequence = effects
        .iter()
        .find_map(proqi::application::Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("revision sequence");
    assert_frame(&mut fixture, area);
    let before = fixture.app.editor_snapshot().expect("pending editor");
    fixture.app.acknowledge_persistence(sequence, false);
    assert_frame(&mut fixture, area);
    let failed = fixture.app.editor_snapshot().expect("failed editor");
    assert_eq!(failed.content, before.content);
    assert_eq!(failed.cursor, before.cursor);
    fixture.app.acknowledge_persistence(sequence, true);
    assert_frame(&mut fixture, area);
    fixture.input(key_input(UiKey::Escape));
    fixture.app.prepare_frame(area);
    fixture.pointer(2, 1, PointerKind::ScrollDown);
    fixture.app.prepare_frame(area);
    fixture.pointer(2, 1, PointerKind::ScrollDown);
    let board = fixture.app.prepare_frame(area);
    assert_eq!(
        board.insert.expect("final insertion row").bottom(),
        board.board.bottom()
    );
    fixture.input(key_input(UiKey::Enter));
    assert_frame(&mut fixture, area);
    assert_eq!(
        fixture.app.editor_snapshot().expect("reentered").content,
        before.content
    );
}

#[test]
fn health_projection_growth_and_fold_expansion_use_the_same_frame() {
    let mut fixture = editor_fixture(9, 8, ThoughtPresentation::Collapsed);
    let area = Rect::new(0, 0, 18, 10);
    fixture.app.prepare_frame(area);
    let payload = image_payload();
    fixture.input(key_input(UiKey::SelectAll));
    let effects = fixture.effects(UiInput::PasteAnnotated(payload));
    let batch = effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::CheckAttachments(batch) => Some(batch),
            _ => None,
        })
        .expect("checks");
    let id = fixture.app.active_thought_id().expect("active");
    let before = fixture.app.prepare_frame(area);
    assert_eq!(before.thought(id).expect("fold").text_area.height, 1);
    let canonical = fixture.app.editor_snapshot().expect("editor");
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
    let grown = fixture.app.prepare_frame(area);
    let text_area = grown.thought(id).expect("grown fold").text_area;
    assert_eq!(text_area.height, 2);
    assert_eq!(text_area.bottom(), grown.board.bottom());
    assert_eq!(
        fixture.app.editor_snapshot().expect("unchanged").content,
        canonical.content
    );
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).expect("terminal");
    terminal
        .draw(|frame| {
            render(
                frame,
                &fixture.app,
                &grown,
                &Theme::resolve(ThemePreference::Auto, true),
            );
        })
        .expect("render");
    assert!(text(terminal.backend().buffer()).contains("sible]"));
    assert!(terminal.backend().cursor_visible());
    fixture.pointer(
        text_area.x + 1,
        text_area.y + 1,
        PointerKind::Down(PointerButton::Left),
    );
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("selected fold")
            .selection
            .is_some()
    );
    fixture.input(key_input(UiKey::Enter));
    let expanded = fixture.app.prepare_frame(area);
    assert!(expanded.thought(id).expect("expanded").text_area.height > 2);
    assert_eq!(fixture.app.prepare_frame(area), expanded);
}

#[test]
fn large_paste_fold_expands_to_real_cap_and_collapses_without_stale_height() {
    let mut fixture = editor_fixture(9, 8, ThoughtPresentation::Automatic);
    let area = Rect::new(0, 0, 48, 12);
    fixture.app.prepare_frame(area);
    fixture.input(key_input(UiKey::SelectAll));
    let payload = (0..30)
        .map(|row| format!("paste row {row}"))
        .collect::<Vec<_>>()
        .join("\n");
    fixture.input(UiInput::Paste(payload.clone()));
    let id = fixture.app.active_thought_id().expect("active");
    let folded = fixture.app.prepare_frame(area);
    assert!(folded.thought(id).expect("fold").text_area.height <= 2);
    fixture.input(key_input(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(key_input(UiKey::Enter));
    let expanded = fixture.app.prepare_frame(area);
    assert_eq!(
        expanded.thought(id).expect("expanded").text_area.height,
        expanded.board.height
    );
    assert!(fixture.app.editor_snapshot().expect("editor").scroll_row > 0);
    assert_eq!(fixture.app.prepare_frame(area), expanded);
    fixture.input(key_input(UiKey::Escape));
    fixture.app.prepare_frame(area);
    fixture.input(key_input(UiKey::Enter));
    let collapsed = fixture.app.prepare_frame(area);
    assert!(collapsed.thought(id).expect("collapsed").text_area.height <= 2);
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        payload
    );
}

fn image_payload() -> PastePayload {
    let path = "/fixture/image.png";
    PastePayload::annotated(
        path.to_owned(),
        vec![ContentAnnotation {
            start: 0,
            end: path.len(),
            kind: ContentAnnotationKind::Attachment {
                ordinal: None,
                image: true,
                display_name: "fixture.png".to_owned(),
            },
        }],
    )
    .expect("annotation")
}
