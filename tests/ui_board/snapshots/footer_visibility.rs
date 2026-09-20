//! Optional footer visibility owns Board, Compose, and Edit projection coverage.

use proqi::ui::{LogicalKey, LogicalModifiers, ThemePreference, UiInput, UiKey, UiSettings};
use ratatui_core::layout::Rect;

use super::{Fixture, snapshot};

fn toggle_footer() -> UiInput {
    UiInput::KeyStroke(
        proqi::ui::KeyStroke::press(LogicalKey::Character('h'))
            .with_modifiers(LogicalModifiers::CONTROL.union(LogicalModifiers::SHIFT)),
    )
}

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

#[test]
fn runtime_toggle_reclaims_geometry_without_stale_hits_and_restarts_from_configuration() {
    let area = Rect::new(0, 0, 42, 12);
    let mut fixture = Fixture::new();
    let seed = fixture.paste("board toggle seed");
    fixture.app.acknowledge_persistence(seed, true);
    fixture.input(crate::key_input(UiKey::Escape));
    let visible = fixture.app.prepare_frame(area);
    let stale_control = visible.controls.first().expect("ordinary footer control").1;
    assert!(visible.footer.height > 0);

    assert!(fixture.effects(toggle_footer()).is_empty());
    let hidden = fixture.app.prepare_frame(area);
    assert_eq!(hidden.board, area);
    assert_eq!(hidden.footer.height, 0);
    assert!(hidden.controls.is_empty());
    assert_eq!(hidden.hit_test(stale_control.x, stale_control.y), None);

    fixture.input(UiInput::KeyStroke(proqi::ui::KeyStroke::press(
        LogicalKey::Character('n'),
    )));
    assert!(fixture.effects(toggle_footer()).is_empty());
    let compose = fixture.app.prepare_frame(area);
    assert!(compose.footer.height > 0);

    fixture.paste("runtime footer owner");
    fixture.acknowledge_all_persistence();
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Enter));
    assert!(fixture.effects(toggle_footer()).is_empty());
    let edit = fixture.app.prepare_frame(area);
    assert_eq!(edit.footer.height, 0);

    let restart = Fixture::new().app.prepare_frame(area);
    assert!(restart.footer.height > 0);

    let settings = UiSettings {
        footer_hidden: true,
        ..UiSettings::default()
    };
    let mut configured_hidden = Fixture::with_settings(settings.clone());
    let seed = configured_hidden.paste("configured hidden");
    configured_hidden.app.acknowledge_persistence(seed, true);
    configured_hidden.input(crate::key_input(UiKey::Escape));
    assert_eq!(configured_hidden.app.prepare_frame(area).footer.height, 0);
    configured_hidden.input(toggle_footer());
    assert!(configured_hidden.app.prepare_frame(area).footer.height > 0);
    assert_eq!(
        Fixture::with_settings(settings)
            .app
            .prepare_frame(area)
            .footer
            .height,
        0
    );
    insta::assert_snapshot!(snapshot(
        &mut fixture,
        area.width,
        area.height,
        ThemePreference::Dark
    ));
}

#[test]
fn commands_discovers_and_executes_the_same_footer_toggle_action() {
    let area = Rect::new(0, 0, 42, 12);
    let mut fixture = Fixture::new();
    let seed = fixture.paste("commands footer toggle");
    fixture.app.acknowledge_persistence(seed, true);
    fixture.input(crate::key_input(UiKey::Escape));
    assert!(fixture.app.prepare_frame(area).footer.height > 0);

    fixture.input(UiInput::KeyStroke(proqi::ui::KeyStroke::press(
        LogicalKey::Character(':'),
    )));
    fixture.input(UiInput::Paste("toggle footer visibility".to_owned()));
    let (_, entries, selected) = fixture.app.palette_view().expect("Commands overlay");
    assert_eq!(
        entries.get(selected).map(String::as_str),
        Some("Toggle footer visibility")
    );

    fixture.input(crate::key_input(UiKey::Enter));
    let hidden = fixture.app.prepare_frame(area);
    assert_eq!(hidden.footer.height, 0);
    assert!(hidden.controls.is_empty());
}

#[test]
fn pending_editor_text_does_not_save_or_block_the_footer_toggle() {
    let area = Rect::new(0, 0, 42, 12);
    let mut fixture = Fixture::new();
    let seed = fixture.paste("presentation-only footer toggle");
    fixture.app.acknowledge_persistence(seed, true);
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Enter));
    fixture.input(crate::key_input(UiKey::Character('!')));

    assert!(fixture.effects(toggle_footer()).is_empty());
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("editor stays open")
            .content,
        "presentation-only footer toggle!"
    );
    let layout = fixture.app.prepare_frame(area);
    // Local editor dirtiness remains visible as the mandatory one-line saving
    // state, while every optional footer row has been reclaimed.
    assert_eq!(layout.footer.height, 1);
    assert!(layout.controls.is_empty());
}
