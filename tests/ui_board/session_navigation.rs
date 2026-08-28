use super::*;

use super::navigation::durable_thought;

#[test]
fn current_session_can_be_renamed_from_the_palette_and_footer() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, "existing");
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "rename session".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    fixture.input(UiInput::Key(UiKey::Enter));
    assert_eq!(fixture.app.session_rename_view(), Some(""));
    let rename_layout = fixture.app.prepare_frame(Rect::new(0, 0, 70, 10));
    let rename_input = rename_layout.overlay.expect("rename overlay").area;
    let terminal = draw_theme(&mut fixture, 70, 10, ThemePreference::Dark);
    assert_eq!(
        terminal.backend().buffer()[(rename_input.x + 1, rename_input.y + 1)].bg,
        Theme::resolve(ThemePreference::Dark, true)
            .focused_surface
            .expect("surface")
    );
    for character in "Agent research".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    assert!(matches!(
        effects.as_slice(),
        [Effect::RenameSession { name: Some(name), .. }] if name == "Agent research"
    ));
    assert_eq!(
        fixture.app.state.board.session.name.as_deref(),
        Some("Agent research")
    );
    fixture.app.complete_session_rename(None, Ok(()));
    let confirmation = draw_theme(&mut fixture, 70, 10, ThemePreference::Dark);
    let confirmation_text = text(confirmation.backend().buffer());
    assert!(confirmation_text.contains("session renamed"));
    assert!(confirmation_text.contains("Agent research"));
    assert!(confirmation_text.contains("1 thought · board · saving"));

    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 70, 10));
    let (_, area) = layout
        .controls
        .iter()
        .find(|(target, _)| *target == HitTarget::RenameSession)
        .expect("rename target");
    fixture.pointer(area.x, area.y, PointerKind::Move);
    let terminal = draw_theme(&mut fixture, 70, 10, ThemePreference::Dark);
    assert_eq!(
        terminal.backend().buffer()[(area.x, area.y)].bg,
        Theme::resolve(ThemePreference::Dark, true)
            .focused_surface
            .expect("focused surface")
    );
    assert_eq!(terminal.backend().buffer()[(area.x, area.y)].symbol(), "A");
    fixture.pointer(area.x, area.y, PointerKind::Down(PointerButton::Left));
    assert_eq!(fixture.app.session_rename_view(), Some("Agent research"));
}

#[test]
fn failed_session_rename_restores_the_previous_durable_name() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, "existing");
    fixture.app.state.board.session.name = Some("Durable".to_owned());
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "rename session".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    fixture.input(UiInput::Key(UiKey::Enter));
    for _ in 0.."Durable".len() {
        fixture.input(UiInput::Key(UiKey::Backspace));
    }
    fixture.input(UiInput::Key(UiKey::Character('N')));
    let _effects = fixture.effects(UiInput::Key(UiKey::Enter));
    fixture.app.complete_session_rename(
        Some("Durable".to_owned()),
        Err(proqi::ports::store::StoreError::Busy),
    );
    assert_eq!(
        fixture.app.state.board.session.name.as_deref(),
        Some("Durable")
    );
    assert!(
        fixture
            .app
            .status_text()
            .is_some_and(|status| status.contains("failed"))
    );
    let failed = draw_theme(&mut fixture, 70, 10, ThemePreference::Dark);
    let failed_text = text(failed.backend().buffer());
    assert!(failed_text.contains("session rename failed"));
    assert!(!failed_text.contains("Durable · 1 thought · board · saving"));

    fixture.input(UiInput::Key(UiKey::Escape));
    let restored = draw_theme(&mut fixture, 70, 10, ThemePreference::Dark);
    let restored_text = text(restored.backend().buffer());
    assert!(restored_text.contains("Durable"));
    assert!(restored_text.contains("1 thought · board · saving"));
}

#[test]
fn optional_complete_session_id_has_independent_geometry_across_themes_and_resize() {
    let settings = UiSettings {
        show_session_id: true,
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    fixture
        .app
        .state
        .board
        .session
        .rename(Some("Mouse selection QA".to_owned()))
        .expect("session name");
    let session_id = fixture.app.state.board.session.id.to_string();

    for theme_preference in [
        ThemePreference::Auto,
        ThemePreference::Light,
        ThemePreference::Dark,
        ThemePreference::Limited,
    ] {
        let terminal = draw_theme(&mut fixture, 80, 8, theme_preference);
        let layout = fixture.app.prepare_frame(Rect::new(0, 0, 80, 8));
        let rename = layout
            .controls
            .iter()
            .find_map(|(target, area)| (*target == HitTarget::RenameSession).then_some(*area))
            .expect("rename target");
        let copy = layout
            .controls
            .iter()
            .find_map(|(target, area)| (*target == HitTarget::CopySessionId).then_some(*area))
            .expect("session ID target");
        assert!(rename.right() < copy.x);
        assert_eq!(
            layout.hit_test(copy.x, copy.y),
            Some(HitTarget::CopySessionId)
        );
        assert_eq!(
            layout.footer_session_id.as_deref(),
            Some(session_id.as_str())
        );
        assert!(
            text(terminal.backend().buffer())
                .contains(&format!("Mouse selection QA · {session_id}"))
        );
        assert_eq!(
            terminal.backend().buffer()[(copy.x, copy.y)].fg,
            Theme::resolve(theme_preference, true).muted
        );
    }

    for (width, height, visible, renameable) in [
        (48, 5, false, true),
        (80, 5, true, true),
        (36, 5, false, true),
        (100, 5, true, true),
        (80, 3, false, false),
    ] {
        let layout = fixture.app.prepare_frame(Rect::new(0, 0, width, height));
        assert_eq!(layout.footer_session_id.is_some(), visible);
        assert_eq!(
            layout
                .controls
                .iter()
                .any(|(target, _)| *target == HitTarget::CopySessionId),
            visible
        );
        assert_eq!(
            layout
                .controls
                .iter()
                .any(|(target, area)| *target == HitTarget::RenameSession && area.width > 0),
            renameable
        );
    }
}
