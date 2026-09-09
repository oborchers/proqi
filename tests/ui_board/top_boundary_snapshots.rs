//! Representative complete-board rendering after top-boundary creation.

use super::navigation::{durable_thought, visual};
use super::*;

use super::{platform_suffix, snapshot_support::snapshot_buffer};
use proqi::{application::InteractionMode, domain::ThoughtId, ui::LayoutSnapshot};

#[test]
fn top_boundary_blank_and_cursor_are_immediately_visible() {
    let mut fixture = Fixture::new();
    durable_thought(
        &mut fixture,
        "Former first thought wraps across a narrow pane with Grüße and 界.",
    );
    fixture.input(visual(CursorMovement::VisualUp, false));
    fixture.input(crate::key_input(UiKey::Character('k')));

    let terminal = draw_theme(&mut fixture, 38, 8, ThemePreference::Dark);
    insta::assert_snapshot!(snapshot_buffer(terminal.backend().buffer()));
}

fn nearly_full_board(settings: UiSettings) -> Fixture {
    let mut fixture = Fixture::with_settings(settings);
    for content in ["alpha", "beta", "gamma", "delta"] {
        let sequence = fixture.paste(content);
        fixture.app.acknowledge_persistence(sequence, true);
        fixture.input(crate::key_input(UiKey::Escape));
    }
    for _ in 0..3 {
        fixture.input(crate::key_input(UiKey::Character('k')));
    }
    fixture
}

fn inserted_nearly_full_board(settings: UiSettings) -> Fixture {
    let mut fixture = nearly_full_board(settings);
    fixture.input(visual(CursorMovement::VisualUp, false));
    fixture.input(visual(CursorMovement::VisualUp, false));
    fixture
}

fn assert_comfortable_cadence(layout: &LayoutSnapshot) {
    assert_eq!(layout.density, proqi::ui::BoardDensity::Comfortable);
    assert_eq!(layout.thoughts[0].area.y, layout.board.y + 1);
    for pair in layout.thoughts.windows(2) {
        let separator = pair[1].separator_before.expect("comfortable separator");
        assert_eq!(separator.y, pair[0].area.bottom());
        assert_eq!(pair[1].area.y, separator.bottom() + 1);
    }
}

fn create_blank_at_top(fixture: &mut Fixture, original: &[ThoughtId]) -> ThoughtId {
    let previous_sequence = fixture.app.state.board.session.last_durable_sequence;
    assert!(
        fixture
            .effects(visual(CursorMovement::VisualUp, false))
            .is_empty()
    );
    let effects = fixture.effects(visual(CursorMovement::VisualUp, false));
    let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
        panic!("boundary creation must request one durable board operation");
    };
    assert_eq!(operation.kind, proqi::domain::BoardOperationKind::Create);
    assert_eq!(operation.sequence.get(), previous_sequence.get() + 1);

    let live = fixture.app.state.board.live_thoughts();
    assert_eq!(live.len(), 5);
    assert!(live[0].content.is_empty());
    assert_eq!(
        live[1..]
            .iter()
            .map(|thought| thought.id)
            .collect::<Vec<_>>(),
        original
    );
    let inserted = live[0].id;
    assert!(matches!(
        fixture.app.interaction_mode(),
        InteractionMode::Edit { thought_id } if thought_id == inserted
    ));
    inserted
}

#[test]
fn top_creation_preserves_comfortable_cadence_and_scrolls_overflow() {
    let mut fixture = nearly_full_board(UiSettings::default());
    let area = Rect::new(0, 0, 60, 17);
    let before = fixture.app.prepare_frame(area);
    assert_eq!(before.board.height, 13);
    assert_eq!(before.board_content_height, 13);
    assert_eq!(
        (before.viewport_offset, before.maximum_viewport_offset),
        (0, 0)
    );
    assert_comfortable_cadence(&before);
    let original = fixture
        .app
        .state
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.id)
        .collect::<Vec<_>>();

    let inserted = create_blank_at_top(&mut fixture, &original);
    let after = fixture.app.prepare_frame(area);
    assert_eq!(
        after.thought(inserted).expect("visible editor").area.height,
        1
    );
    assert_eq!(after.footer.y, after.board.bottom());
    assert_eq!(after.board_content_height, 14);
    assert_eq!(
        (after.viewport_offset, after.maximum_viewport_offset),
        (0, 1)
    );
    assert_comfortable_cadence(&after);
    assert!(after.max_first_index > 0);
    let last = fixture.app.state.board.live_thoughts()[4].id;
    assert!(after.thought(last).is_none());
}

#[test]
fn top_creation_preserves_density_across_exact_fit_boundaries() {
    for terminal_height in [16, 17, 18, 22] {
        let mut fixture = nearly_full_board(UiSettings::default());
        let area = Rect::new(0, 0, 60, terminal_height);
        let before = fixture.app.prepare_frame(area);
        assert_eq!(before.board.height, terminal_height - 4);
        assert_eq!(before.density, proqi::ui::BoardDensity::Comfortable);
        assert_eq!(before.board_content_height, 13);
        assert_eq!(
            before.maximum_viewport_offset,
            13_usize.saturating_sub(usize::from(before.board.height))
        );

        fixture.input(visual(CursorMovement::VisualUp, false));
        fixture.input(visual(CursorMovement::VisualUp, false));
        let after = fixture.app.prepare_frame(area);
        assert_eq!(after.density, proqi::ui::BoardDensity::Comfortable);
        assert_eq!(after.board_content_height, 14);
        assert_eq!(
            after.maximum_viewport_offset,
            14_usize.saturating_sub(usize::from(after.board.height))
        );
        assert_eq!(after.viewport_offset, 0);
    }
}

#[test]
fn top_boundary_density_policy_has_representative_snapshots() {
    insta::with_settings!({ snapshot_suffix => platform_suffix() }, {
        let mut standard = inserted_nearly_full_board(UiSettings::default());
        insta::assert_snapshot!(
            "top_boundary_standard_density",
            snapshot_buffer(
                draw_theme(&mut standard, 60, 17, ThemePreference::Dark)
                    .backend()
                    .buffer()
            )
        );

        let mut explicit_compact = inserted_nearly_full_board(UiSettings {
            density: proqi::ui::BoardDensity::Compact,
            ..UiSettings::default()
        });
        insta::assert_snapshot!(
            "top_boundary_explicit_compact_density",
            snapshot_buffer(
                draw_theme(&mut explicit_compact, 60, 17, ThemePreference::Dark)
                    .backend()
                    .buffer()
            )
        );

        let mut narrow = inserted_nearly_full_board(UiSettings::default());
        insta::assert_snapshot!(
            "top_boundary_narrow_standard_density",
            snapshot_buffer(
                draw_theme(&mut narrow, 28, 17, ThemePreference::Dark)
                    .backend()
                    .buffer()
            )
        );

        let mut shallow = inserted_nearly_full_board(UiSettings::default());
        insta::assert_snapshot!(
            "top_boundary_shallow_responsive_density",
            snapshot_buffer(
                draw_theme(&mut shallow, 60, 8, ThemePreference::Dark)
                    .backend()
                    .buffer()
            )
        );
    });
}

#[test]
fn active_top_editor_repeatedly_crosses_the_responsive_breakpoint() {
    let mut fixture = inserted_nearly_full_board(UiSettings::default());
    for (height, board_height, density) in [
        (8, 4, proqi::ui::BoardDensity::Compact),
        (9, 5, proqi::ui::BoardDensity::Comfortable),
        (8, 4, proqi::ui::BoardDensity::Compact),
        (9, 5, proqi::ui::BoardDensity::Comfortable),
    ] {
        let layout = fixture.app.prepare_frame(Rect::new(0, 0, 60, height));
        assert_eq!(layout.board.height, board_height);
        assert_eq!(layout.density, density);
        assert_eq!(layout.viewport_offset, 0);
        assert!(layout.maximum_viewport_offset > 0);
        let active = fixture.app.active_thought_id().expect("active editor");
        assert_eq!(
            layout.thought(active).expect("visible editor").area.height,
            1
        );
    }
}
