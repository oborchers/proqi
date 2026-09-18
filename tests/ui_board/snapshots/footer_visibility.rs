//! Optional footer visibility owns Board, Compose, and Edit projection coverage.

use proqi::ui::{ThemePreference, UiKey, UiSettings};
use ratatui_core::layout::Rect;

use super::{Fixture, snapshot};

#[test]
fn hidden_optional_footer_reclaims_every_optional_row_in_board_compose_and_edit() {
    let settings = UiSettings {
        footer_hidden: true,
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    for area in [Rect::new(0, 0, 42, 12), Rect::new(0, 0, 12, 3)] {
        let layout = fixture.app.prepare_frame(area);
        assert_eq!(layout.board, area);
        assert_eq!(layout.footer.height, 0);
        assert!(layout.controls.is_empty());
    }
    fixture.paste("wide 界 e\u{301} control \u{0007} content");
    fixture.acknowledge_all_persistence();
    let compose = fixture.app.prepare_frame(Rect::new(0, 0, 42, 12));
    assert_eq!(compose.footer.height, 0);
    assert!(compose.controls.is_empty());
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Enter));
    let edit = fixture.app.prepare_frame(Rect::new(0, 0, 42, 12));
    assert_eq!(edit.footer.height, 0);
    assert!(edit.controls.is_empty());
    insta::assert_snapshot!(snapshot(&mut fixture, 42, 12, ThemePreference::Dark));
}
