//! Arbitrary folded cursor movement preserves canonical attachment boundaries.

use super::{image_payload, insert_accessible};
use crate::Fixture;
use proptest::prelude::*;
use proqi::{ports::editor::CursorMovement, ui::UiKey};

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    #[test]
    fn collapsed_fold_navigation_never_leaves_a_cursor_inside_hidden_content(
        forwards in proptest::collection::vec(any::<bool>(), 0..80),
    ) {
        let path = "/tmp/atomic-hidden-image.png";
        let mut fixture = Fixture::new();
        insert_accessible(&mut fixture, image_payload(path));
        for forward in forwards {
            fixture.input(crate::key_input(UiKey::Move {
                movement: if forward {
                    CursorMovement::GraphemeForward
                } else {
                    CursorMovement::GraphemeBack
                },
                extend_selection: false,
            }));
            let snapshot = fixture.app.editor_snapshot().expect("editor");
            if let Some(selection) = snapshot.selection {
                prop_assert_eq!(
                    selection,
                    proqi::ports::editor::TextSelection {
                        start: proqi::domain::TextPosition::new(0, 0),
                        end: proqi::domain::TextPosition::new(0, path.len()),
                    }
                );
            } else {
                prop_assert!(
                    snapshot.cursor == proqi::domain::TextPosition::new(0, 0)
                        || snapshot.cursor
                            == proqi::domain::TextPosition::new(0, path.len())
                );
            }
        }
    }
}
