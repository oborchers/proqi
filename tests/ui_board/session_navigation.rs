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
