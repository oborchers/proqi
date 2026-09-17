use super::*;
use crate::domain::{Separator, SeparatorId};

fn separator_id(seed: u8) -> SeparatorId {
    SeparatorId::from_uuid(uuid_v7(seed)).expect("separator ID")
}

fn mixed_state(pattern: &[char]) -> AppState {
    let now = Timestamp::from_millis(1);
    let session = Session::new(
        SessionId::from_uuid(uuid_v7(1)).expect("session ID"),
        "/tmp".into(),
        now,
    )
    .expect("session");
    let mut thoughts = Vec::new();
    let mut separators = Vec::new();
    for (index, item) in pattern.iter().enumerate() {
        let position = ThoughtPosition::new(u32::try_from(index).expect("position"));
        match item {
            't' => thoughts.push(Thought::new(
                ThoughtId::from_uuid(uuid_v7(u8::try_from(index + 2).expect("seed")))
                    .expect("thought ID"),
                session.id,
                format!("thought {index}"),
                position,
                now,
            )),
            's' => separators.push(Separator::new(
                separator_id(u8::try_from(index + 80).expect("seed")),
                session.id,
                position,
                now,
            )),
            _ => panic!("unknown test item"),
        }
    }
    AppState::new(
        SessionBoard::with_separators(session, thoughts, separators).expect("mixed board"),
    )
}

#[test]
fn thought_only_geometry_is_unchanged_and_explicit_boundaries_own_adjacent_spacing() {
    let thought_only = mixed_state(&['t', 't']);
    let comfortable = measure(
        &thought_only,
        None,
        40,
        20,
        crate::ui::settings::BoardDensity::Comfortable,
    );
    assert_eq!(comfortable.top_padding, 1);
    assert_eq!(comfortable.thoughts[0].content_start, 0);
    assert_eq!(comfortable.thoughts[0].end, 1);
    assert_eq!(comfortable.thoughts[1].gap_start, 1);
    assert_eq!(comfortable.thoughts[1].gap_rows, 2);
    assert_eq!(comfortable.thoughts[1].content_start, 3);

    let mixed = mixed_state(&['t', 's', 't']);
    let comfortable = measure(
        &mixed,
        None,
        40,
        20,
        crate::ui::settings::BoardDensity::Comfortable,
    );
    assert_eq!(comfortable.top_padding, 1);
    assert_eq!(comfortable.thoughts[0].end, 1);
    assert_eq!(comfortable.separators[0].start, 1);
    assert_eq!(comfortable.separators[0].line, 2);
    assert_eq!(comfortable.separators[0].end, 4);
    assert_eq!(comfortable.thoughts[1].gap_rows, 0);
    assert_eq!(comfortable.thoughts[1].content_start, 4);

    let compact = measure(
        &mixed,
        None,
        40,
        20,
        crate::ui::settings::BoardDensity::Compact,
    );
    assert_eq!(compact.top_padding, 0);
    assert_eq!(compact.thoughts[0].content_start, 0);
    assert_eq!(compact.separators[0].start, 1);
    assert_eq!(compact.separators[0].line, 1);
    assert_eq!(compact.separators[0].end, 2);
    assert_eq!(compact.thoughts[1].content_start, 2);
}

#[test]
fn edge_and_consecutive_separators_keep_distinct_rows_and_scroll_anchors() {
    let state = mixed_state(&['s', 's', 't', 's']);
    let flow = measure(
        &state,
        None,
        12,
        5,
        crate::ui::settings::BoardDensity::Comfortable,
    );
    assert_eq!(flow.top_padding, 0);
    assert_eq!(flow.separators.len(), 3);
    assert_eq!(flow.separators[0].start, 0);
    assert_eq!(flow.separators[0].line, 1);
    assert_eq!(flow.separators[0].end, 3);
    assert_eq!(flow.separators[1].start, 3);
    assert_eq!(flow.separators[1].line, 4);
    assert_eq!(flow.separators[1].end, 6);
    assert_eq!(flow.thoughts[0].content_start, 6);
    assert_eq!(flow.separators[2].start, 7);

    let second = flow.separators[1];
    let resolved = flow.resolve(
        BoardViewport::FollowFocus(ScrollAnchor::Start),
        Some(second.separator_id.into()),
        false,
        4,
    );
    assert!(resolved.offset <= second.line);
    assert!(second.line < resolved.offset + 4);
    let anchored = flow.resolve(
        BoardViewport::Manual(ScrollAnchor::Separator(second.separator_id)),
        None,
        false,
        4,
    );
    assert_eq!(
        anchored.geometry.current,
        ScrollAnchor::Separator(second.separator_id)
    );
}

#[test]
fn separator_only_board_has_no_phantom_thought_geometry() {
    for density in [
        crate::ui::settings::BoardDensity::Comfortable,
        crate::ui::settings::BoardDensity::Compact,
    ] {
        let state = mixed_state(&['s']);
        let flow = measure(&state, None, 3, 2, density);
        assert!(flow.thoughts.is_empty());
        assert_eq!(flow.separators.len(), 1);
        assert_eq!(flow.separators[0].start, 0);
        assert!(flow.insert_row.is_some());
    }
}
