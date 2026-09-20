//! Optional footer visibility owns global startup state and a Board-only runtime toggle.

use proqi::ui::{LogicalKey, LogicalModifiers, ThemePreference, UiInput, UiKey, UiSettings};
use ratatui_core::layout::Rect;

use super::{Fixture, snapshot};

fn toggle_footer() -> UiInput {
    UiInput::KeyStroke(proqi::ui::KeyStroke::press(LogicalKey::Character('h')))
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

    for (key, modifiers) in [
        (LogicalKey::Character('H'), LogicalModifiers::SHIFT),
        (LogicalKey::Character('h'), LogicalModifiers::CONTROL),
        (LogicalKey::Character('h'), LogicalModifiers::ALT),
        (LogicalKey::Character('h'), LogicalModifiers::SUPER),
    ] {
        assert!(
            fixture
                .effects(UiInput::KeyStroke(
                    proqi::ui::KeyStroke::press(key).with_modifiers(modifiers),
                ))
                .is_empty(),
            "only the unmodified lowercase physical H key toggles the footer",
        );
        assert!(fixture.app.prepare_frame(area).footer.height > 0);
    }

    assert!(fixture.effects(toggle_footer()).is_empty());
    let hidden = fixture.app.prepare_frame(area);
    assert_eq!(hidden.board, area);
    assert_eq!(hidden.footer.height, 0);
    assert!(hidden.controls.is_empty());
    assert_eq!(hidden.hit_test(stale_control.x, stale_control.y), None);

    fixture.input(UiInput::KeyStroke(proqi::ui::KeyStroke::press(
        LogicalKey::Character('n'),
    )));
    let compose = fixture.app.prepare_frame(area);
    // Starting a compose session is pending durability, so only its required
    // saving status remains while all optional chrome stays hidden.
    assert_eq!(compose.footer.height, 1);

    fixture.paste("runtime footer owner");
    fixture.acknowledge_all_persistence();
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Enter));
    let edit = fixture.app.prepare_frame(area);
    assert_eq!(edit.footer.height, 0);

    fixture.input(crate::key_input(UiKey::Escape));
    assert!(fixture.effects(toggle_footer()).is_empty());
    assert!(fixture.app.prepare_frame(area).footer.height > 0);

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
fn board_insertion_boundary_toggles_without_creating_a_commands_route() {
    let area = Rect::new(0, 0, 42, 12);
    let mut fixture = Fixture::new();
    fixture.input(crate::key_input(UiKey::Escape));
    assert!(fixture.app.insertion_focused());
    assert!(fixture.app.prepare_frame(area).footer.height > 0);

    assert!(fixture.effects(toggle_footer()).is_empty());
    assert_eq!(fixture.app.prepare_frame(area).footer.height, 0);

    fixture.input(crate::key_input(UiKey::Character(':')));
    fixture.input(UiInput::Paste("toggle footer visibility".to_owned()));
    let (_, entries, _) = fixture.app.palette_view().expect("Commands overlay");
    assert_eq!(entries, ["No matching commands"]);
}

#[test]
fn compose_and_edit_h_remains_literal_and_never_toggles_footer_chrome() {
    let area = Rect::new(0, 0, 42, 12);
    let mut fixture = Fixture::new();
    let seed = fixture.paste("footer text ownership");
    fixture.app.acknowledge_persistence(seed, true);
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(toggle_footer());
    assert_eq!(fixture.app.prepare_frame(area).footer.height, 0);

    fixture.input(crate::key_input(UiKey::Character('n')));
    fixture.input(toggle_footer());
    assert_eq!(fixture.app.prepare_frame(area).footer.height, 1);
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("compose editor")
            .content,
        "h"
    );
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Enter));
    fixture.input(toggle_footer());
    assert_eq!(
        fixture.app.editor_snapshot().expect("edit editor").content,
        "hh"
    );
    let layout = fixture.app.prepare_frame(area);
    assert_eq!(layout.footer.height, 1);
    assert!(layout.controls.is_empty());
}
