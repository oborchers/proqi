//! Cross-layer frame geometry for health-aware attachment presentation.

use super::*;
use proqi::domain::ThoughtPresentation;
use unicode_segmentation::UnicodeSegmentation as _;

#[test]
fn inaccessible_suffix_wrap_is_measured_from_the_visible_presentation() {
    let mut fixture = Fixture::new();
    let path = "/tmp/Grüße-第一.png";
    let effects = fixture.effects(UiInput::PasteAnnotated(attachment_payload(path, true)));
    fixture.app.complete_attachment_checks(complete(
        attachment_batch(&effects),
        Err(AttachmentAccessFailure::Missing),
    ));
    fixture.input(UiInput::Key(UiKey::Escape));

    let area = Rect::new(0, 0, 18, 12);
    let layout = fixture.app.prepare_frame(area);
    let thought = &layout.thoughts[0];
    assert_eq!(thought.text_area.width, 16, "fixture wrap width");
    assert_eq!(
        usize::from(thought.text_area.height),
        2,
        "layout must allocate every row that rendering paints"
    );
    let rendered = text(
        draw(&mut fixture, area.width, area.height)
            .backend()
            .buffer(),
    );
    assert!(
        rendered.contains("[Image 1 ·"),
        "rendered frame:\n{rendered}"
    );
    assert!(rendered.contains("sible]"), "rendered frame:\n{rendered}");
}

#[test]
fn public_layout_and_render_path_preserves_attachment_presentation() {
    let mut fixture = Fixture::new();
    let path = "/tmp/public-contract.png";
    let effects = fixture.effects(UiInput::PasteAnnotated(attachment_payload(path, true)));
    fixture.app.complete_attachment_checks(complete(
        attachment_batch(&effects),
        Err(AttachmentAccessFailure::Missing),
    ));
    fixture.input(UiInput::Key(UiKey::Escape));

    let area = Rect::new(0, 0, 80, 8);
    let layout = proqi::ui::compute_layout(&fixture.app.state, None, area, 0, false, false);
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
        .expect("draw");
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("[Image 1 · inaccessible]"));
    assert!(!rendered.contains(path));
}

#[test]
fn collapsed_overflow_and_next_separator_use_health_aware_natural_rows() {
    let mut fixture = Fixture::new();
    let path = "/tmp/missing.png";
    let content = format!("one\ntwo\n{path}");
    let effects = fixture.effects(UiInput::PasteAnnotated(embedded_attachment(
        content,
        8..8 + path.len(),
    )));
    fixture.app.complete_attachment_checks(complete(
        attachment_batch(&effects),
        Err(AttachmentAccessFailure::Missing),
    ));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.paste("neighbor");
    fixture.input(UiInput::Key(UiKey::Escape));
    let first_id = fixture.app.state.board.live_thoughts()[0].id;
    fixture
        .app
        .state
        .board
        .thought_mut(first_id)
        .expect("first thought")
        .presentation = ThoughtPresentation::Collapsed;

    let area = Rect::new(0, 0, 18, 14);
    let terminal = draw(&mut fixture, area.width, area.height);
    let layout = fixture.app.prepare_frame(area);
    let first = &layout.thoughts[0];
    let second = &layout.thoughts[1];

    assert_eq!(first.hidden_rows, 3);
    assert_eq!(first.overflow.expect("overflow").y, first.text_area.y + 1);
    assert_eq!(
        second.separator_before.expect("separator").y,
        first.area.bottom(),
    );
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("3 more lines"));
    assert!(rendered.contains("neighbor"));
}

#[test]
fn health_transitions_resize_reflow_and_preserve_scroll_selection() {
    let mut fixture = Fixture::new();
    let path = "/tmp/health.png";
    let prefix = "Grüße 界\nbeta\n";
    let content = format!("{prefix}{path}\nomega\nlast");
    let effects = fixture.effects(UiInput::PasteAnnotated(embedded_attachment(
        content,
        prefix.len()..prefix.len() + path.len(),
    )));
    fixture
        .app
        .complete_attachment_checks(complete(attachment_batch(&effects), Ok(())));
    fixture.input(UiInput::Key(UiKey::Escape));
    let thought_id = fixture.app.state.board.live_thoughts()[0].id;
    fixture
        .app
        .state
        .board
        .thought_mut(thought_id)
        .expect("thought")
        .presentation = ThoughtPresentation::Expanded;
    fixture.input(UiInput::Key(UiKey::UnmodifiedSpace));

    let narrow = Rect::new(0, 0, 18, 8);
    let _initial = draw(&mut fixture, narrow.width, narrow.height);
    for _ in 0..2 {
        fixture.input(UiInput::Pointer(PointerInput {
            column: 0,
            row: 0,
            kind: PointerKind::ScrollDown,
            extend_selection: false,
        }));
        let _scrolled = draw(&mut fixture, narrow.width, narrow.height);
    }
    let before = fixture.app.prepare_frame(narrow);
    assert!(before.first_row_offset > 0);
    assert!(fixture.app.thought_selected(thought_id));

    complete_refresh(&mut fixture, Err(AttachmentAccessFailure::Missing));
    let failed = draw(&mut fixture, narrow.width, narrow.height);
    let failed_layout = fixture.app.prepare_frame(narrow);
    assert!(text(failed.backend().buffer()).contains("inaccessible"));
    assert!(failed_layout.first_row_offset > 0);
    let failed_thought = failed_layout.thought(thought_id).expect("failed thought");
    assert!(failed_thought.viewport_clipped);
    assert!(failed_thought.scrollable_hidden);
    assert!(fixture.app.thought_selected(thought_id));

    let wide = Rect::new(0, 0, 40, 8);
    let wide_layout = fixture.app.prepare_frame(wide);
    assert_eq!(wide_layout.first_index, 0);
    assert!(wide_layout.thought(thought_id).is_some());

    complete_refresh(&mut fixture, Ok(()));
    let recovered = draw(&mut fixture, narrow.width, narrow.height);
    let recovered_layout = fixture.app.prepare_frame(narrow);
    assert!(!text(recovered.backend().buffer()).contains("inaccessible"));
    assert_eq!(recovered_layout.first_index, 0);
    assert!(recovered_layout.thought(thought_id).is_some());
    assert!(fixture.app.thought_selected(thought_id));
}

#[test]
fn health_transitions_keep_the_same_post_attachment_row_at_the_viewport_edge() {
    let mut fixture = Fixture::new();
    let path = "/tmp/health-anchor.png";
    let content =
        format!("{path}\nPOST-ANCHOR\nbelow-1\nbelow-2\nbelow-3\nbelow-4\nbelow-5\nbelow-6");
    let effects = fixture.effects(UiInput::PasteAnnotated(embedded_attachment(
        content,
        0..path.len(),
    )));
    fixture
        .app
        .complete_attachment_checks(complete(attachment_batch(&effects), Ok(())));
    fixture.input(UiInput::Key(UiKey::Escape));
    let thought_id = fixture.app.state.board.live_thoughts()[0].id;
    fixture
        .app
        .state
        .board
        .thought_mut(thought_id)
        .expect("thought")
        .presentation = ThoughtPresentation::Expanded;

    let area = Rect::new(0, 0, 18, 8);
    let mut accessible = fixture.app.prepare_frame(area);
    for _ in 0..8 {
        if accessible.first_row_offset == 1 {
            break;
        }
        fixture.pointer(1, accessible.board.y, PointerKind::ScrollDown);
        accessible = fixture.app.prepare_frame(area);
    }
    assert_eq!(accessible.first_row_offset, 1, "accessible anchor");
    assert_top_thought_row(&mut fixture, area, "POST-ANCHOR");

    complete_refresh(&mut fixture, Err(AttachmentAccessFailure::Missing));
    let inaccessible = fixture.app.prepare_frame(area);
    assert_eq!(inaccessible.first_row_offset, 2, "inaccessible reflow");
    assert_top_thought_row(&mut fixture, area, "POST-ANCHOR");

    complete_refresh(&mut fixture, Ok(()));
    let recovered = fixture.app.prepare_frame(area);
    assert_eq!(recovered.first_row_offset, 1, "recovered reflow");
    assert_top_thought_row(&mut fixture, area, "POST-ANCHOR");
}

#[test]
fn pointer_rows_share_canonical_mapping_after_health_changes() {
    let prefix = "界 ";
    let path = "/tmp/missing.png";
    let suffix = " tail";
    let content = format!("{prefix}{path}{suffix}");
    let range = prefix.len()..prefix.len() + path.len();
    let expected_selection = proqi::ports::editor::TextSelection {
        start: proqi::domain::TextPosition::new(0, prefix.graphemes(true).count()),
        end: proqi::domain::TextPosition::new(
            0,
            prefix.graphemes(true).count() + path.graphemes(true).count(),
        ),
    };

    for (row, column) in [(0, 4), (1, 0)] {
        let mut fixture = inaccessible_embedded_fixture(&content, range.clone());
        let layout = fixture.app.prepare_frame(Rect::new(0, 0, 18, 10));
        let area = layout.thoughts[0].text_area;
        fixture.pointer(
            area.x + column,
            area.y + row,
            PointerKind::Down(PointerButton::Left),
        );
        assert_eq!(
            fixture.app.editor_snapshot().expect("editor").selection,
            Some(expected_selection),
            "visual row {row} must select the same canonical attachment",
        );
    }

    let mut prefix_fixture = inaccessible_embedded_fixture(&content, range);
    let layout = prefix_fixture.app.prepare_frame(Rect::new(0, 0, 18, 10));
    let area = layout.thoughts[0].text_area;
    prefix_fixture.pointer(area.x, area.y, PointerKind::Down(PointerButton::Left));
    let snapshot = prefix_fixture.app.editor_snapshot().expect("prefix editor");
    assert_eq!(snapshot.cursor, proqi::domain::TextPosition::new(0, 0));
    assert!(snapshot.selection.is_none());
}

#[test]
fn expanded_warning_suffix_rows_map_to_the_canonical_attachment_end() {
    let mut fixture =
        inaccessible_embedded_fixture("/tmp/Grüße-第一.png", 0.."/tmp/Grüße-第一.png".len());
    let collapsed = fixture.app.prepare_frame(Rect::new(0, 0, 18, 10));
    let collapsed_area = collapsed.thoughts[0].text_area;
    fixture.pointer(
        collapsed_area.x + 2,
        collapsed_area.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.input(UiInput::Key(UiKey::Enter));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 18, 10));
    let area = layout.thoughts[0].text_area;
    assert!(area.height >= 3, "expanded path and warning must wrap");
    fixture.pointer(
        area.x,
        area.y + area.height - 1,
        PointerKind::Down(PointerButton::Left),
    );
    let snapshot = fixture.app.editor_snapshot().expect("expanded editor");
    assert_eq!(
        snapshot.cursor,
        proqi::domain::TextPosition::new(0, snapshot.content.graphemes(true).count()),
    );
    assert!(snapshot.selection.is_none());
}

fn embedded_attachment(content: String, range: std::ops::Range<usize>) -> PastePayload {
    PastePayload::annotated(
        content,
        vec![ContentAnnotation {
            start: range.start,
            end: range.end,
            kind: ContentAnnotationKind::Attachment {
                image: true,
                display_name: "fixture.png".to_owned(),
            },
        }],
    )
    .expect("valid embedded attachment")
}

fn inaccessible_embedded_fixture(content: &str, range: std::ops::Range<usize>) -> Fixture {
    let mut fixture = Fixture::new();
    let effects = fixture.effects(UiInput::PasteAnnotated(embedded_attachment(
        content.to_owned(),
        range,
    )));
    fixture
        .app
        .complete_attachment_checks(complete(attachment_batch(&effects), Ok(())));
    complete_refresh(&mut fixture, Err(AttachmentAccessFailure::Missing));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture
}

fn complete_refresh(fixture: &mut Fixture, result: Result<(), AttachmentAccessFailure>) {
    let batch = attachment_batch(&fixture.app.refresh_attachments(false));
    fixture
        .app
        .complete_attachment_checks(complete(batch, result));
}

fn assert_top_thought_row(fixture: &mut Fixture, area: Rect, expected: &str) {
    let terminal = draw(fixture, area.width, area.height);
    let layout = fixture.app.prepare_frame(area);
    let thought = layout.thoughts.first().expect("visible thought");
    let row = (thought.text_area.x..thought.text_area.right())
        .map(|column| {
            terminal.backend().buffer()[(column, thought.text_area.y)]
                .symbol()
                .to_owned()
        })
        .collect::<String>();
    assert_eq!(row.trim_end(), expected);
}
