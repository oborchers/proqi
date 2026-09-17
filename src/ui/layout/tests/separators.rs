use super::*;
use crate::domain::{Separator, SeparatorId};

fn separator_state(count: usize) -> (AppState, Vec<SeparatorId>) {
    let now = Timestamp::from_millis(1);
    let session = Session::new(
        SessionId::from_uuid(uuid_v7(1)).expect("session ID"),
        "/tmp".into(),
        now,
    )
    .expect("session");
    let separators = (0..count)
        .map(|index| {
            Separator::new(
                SeparatorId::from_uuid(uuid_v7(u8::try_from(index + 30).expect("seed")))
                    .expect("separator ID"),
                session.id,
                ThoughtPosition::new(u32::try_from(index).expect("position")),
                now,
            )
        })
        .collect::<Vec<_>>();
    let ids = separators.iter().map(|separator| separator.id).collect();
    (
        AppState::new(
            SessionBoard::with_separators(session, Vec::new(), separators).expect("board"),
        ),
        ids,
    )
}

#[test]
fn rendered_separator_geometry_is_the_only_separator_hit_geometry() {
    let (state, ids) = separator_state(2);
    let layout = compute(&state, None, Rect::new(0, 0, 16, 12), 0, false, false);
    assert_eq!(layout.separators.len(), 2);
    for (separator, expected_id) in layout.separators.iter().zip(ids) {
        assert_eq!(separator.separator_id, expected_id);
        let line = separator.line.expect("visible line");
        assert_eq!(line.x, layout.board.x);
        assert_eq!(line.width, layout.board.width);
        for row in separator.area.y..separator.area.bottom() {
            assert_eq!(
                layout.hit_test(separator.area.x, row),
                Some(HitTarget::SeparatorDragHandle(expected_id))
            );
            if separator.area.width > 1 {
                assert_eq!(
                    layout.hit_test(separator.area.x + 1, row),
                    Some(HitTarget::Separator(expected_id))
                );
            }
        }
    }
}

#[test]
fn narrow_and_shallow_separator_clipping_never_invents_an_identity() {
    let (mut state, ids) = separator_state(3);
    state.focused_item = Some(ids[2].into());
    let presentation = crate::ui::projection::FramePresentation::canonical(&state, None);
    let layout = crate::ui::layout::compute_with_density(
        &state,
        &presentation,
        Rect::new(0, 0, 1, 6),
        0,
        false,
        false,
        false,
        crate::ui::settings::BoardDensity::Comfortable,
        0,
        &crate::ui::ShortcutRegistry::default(),
    );
    assert!(!layout.separators.is_empty());
    assert!(
        layout
            .separators
            .iter()
            .all(|separator| ids.contains(&separator.separator_id))
    );
    for separator in &layout.separators {
        assert_eq!(separator.area.width, 1);
        assert_eq!(separator.gutter, separator.area);
        for row in separator.area.y..separator.area.bottom() {
            assert_eq!(
                layout.hit_test(separator.area.x, row),
                Some(HitTarget::SeparatorDragHandle(separator.separator_id))
            );
        }
    }
}
