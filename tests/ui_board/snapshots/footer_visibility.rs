//! Persisted `footer_hidden` config setting snapshot scenarios.

use proqi::ui::{ThemePreference, UiSettings};
use ratatui_core::layout::Rect;

use super::{Fixture, snapshot};

#[test]
fn footer_hidden_setting_removes_the_footer_band_and_reclaims_its_rows() {
    let settings = UiSettings {
        footer_hidden: true,
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    super::super::agent::prepare_thought(&mut fixture);
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 42, 12));
    assert!(layout.controls.is_empty());
    insta::assert_snapshot!(snapshot(&mut fixture, 42, 12, ThemePreference::Dark));
}
