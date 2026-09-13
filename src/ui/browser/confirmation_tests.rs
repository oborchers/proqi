//! Browser confirmation shares rendered keymap geometry and semantic outcomes.

use super::{BrowserAction, BrowserAvailability, SessionBrowser, SessionBrowserItem};
use crate::{
    domain::Timestamp,
    ports::{
        runtime::InstanceInfo,
        store::{BrowserHistoryEntry, BrowserHistoryStatus, SessionHit},
    },
    ui::{
        KeyStroke, KeymapDocument, LogicalKey, LogicalModifiers, PointerButton, PointerInput,
        PointerKind, ShortcutPlatform, Theme, ThemePreference, UiInput, render_browser,
    },
};
use ratatui_core::{backend::TestBackend, terminal::Terminal};

fn browser(name: &str, remapped: bool) -> SessionBrowser {
    let entry = SessionBrowserItem {
        hit: SessionHit {
            id: "ses_068q40p201o010000000000000"
                .parse()
                .expect("session ID"),
            name: Some(name.to_owned()),
            origin_cwd: "synthetic".into(),
            last_opened_cwd: "synthetic".into(),
            last_opened_at: Timestamp::from_millis(10),
            last_active_at: Timestamp::from_millis(10),
            thought_count: 0,
            excerpt: String::new(),
            previews: Vec::new(),
            search_content: String::new(),
            integration_context: None,
            trashed: false,
        },
        availability: BrowserAvailability::Resumable,
    };
    let mut browser = SessionBrowser::new(vec![entry], Timestamp::from_millis(20));
    if remapped {
        browser.shortcut_registry = toml::from_str::<KeymapDocument>(
            "schema_version=1\n[bindings.browser_rename]\n\"context.confirm\"=[{key=\"F35\",modifiers=[\"Control\",\"Alt\",\"Shift\"]}]",
        )
        .expect("keymap document")
        .resolve(ShortcutPlatform::Portable)
        .expect("resolved keymap");
    }
    browser
}

fn history_entry(
    browser: &SessionBrowser,
    kind: crate::domain::BrowserOperationKind,
) -> BrowserHistoryEntry {
    BrowserHistoryEntry {
        operation_id: "op_06g30t8fudrq55fdkjqr6mpe44"
            .parse()
            .expect("operation ID"),
        session_id: browser.items[0].hit.id,
        kind,
    }
}

fn key(key: LogicalKey) -> UiInput {
    UiInput::KeyStroke(KeyStroke::press(key))
}

fn primary_key(character: char, shift: bool) -> UiInput {
    let primary = if cfg!(target_os = "macos") {
        LogicalModifiers::SUPER
    } else {
        LogicalModifiers::CONTROL
    };
    let modifiers = if shift {
        primary.union(LogicalModifiers::SHIFT)
    } else {
        primary
    };
    UiInput::KeyStroke(KeyStroke::press(LogicalKey::Character(character)).with_modifiers(modifiers))
}

fn click(column: u16, row: u16) -> UiInput {
    UiInput::Pointer(PointerInput {
        column,
        row,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    })
}

fn draw(browser: &mut SessionBrowser, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| {
            let layout = browser.prepare_frame(frame.area());
            render_browser(
                frame,
                browser,
                &layout,
                &Theme::resolve(ThemePreference::Auto, true),
            );
        })
        .expect("draw");
    terminal
}

fn footer(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.width)
        .map(|x| buffer[(x, buffer.area.height - 1)].symbol())
        .collect()
}

fn label_column(line: &str, label: &str) -> u16 {
    let byte = line.find(label).expect("visible label");
    u16::try_from(crate::ports::text_layout::terminal_cell_width(
        &line[..byte],
    ))
    .expect("terminal column")
}

#[test]
fn rename_save_mouse_matches_keyboard_trim_and_clear() {
    for name in ["  Grüße 界 e\u{301}  ", "   "] {
        let mut keyboard = browser(name, false);
        keyboard.handle(key(LogicalKey::Function(2)));
        let expected = keyboard.handle(key(LogicalKey::Enter));
        assert!(
            matches!(&expected, BrowserAction::Rename { name: actual, .. }
            if actual.as_deref() == (!name.trim().is_empty()).then_some(name.trim()))
        );
        for (width, height) in [(80, 24), (24, 7), (12, 3)] {
            let mut mouse = browser(name, false);
            mouse.handle(key(LogicalKey::Function(2)));
            let terminal = draw(&mut mouse, width, height);
            let column = label_column(&footer(&terminal), "Save");
            assert_eq!(mouse.handle(click(column, height - 1)), expected);
            assert!(mouse.rename_value().is_none());
        }
    }
}

#[test]
fn remapped_save_uses_current_visible_geometry_and_cancel_survives() {
    let mut browser = browser("new name", true);
    browser.handle(key(LogicalKey::Function(2)));
    let terminal = draw(&mut browser, 80, 7);
    let label = footer(&terminal);
    assert!(label.contains("Ctrl+Alt+Shift+F35 Save"), "{label}");
    let column = label_column(&label, "Save");
    assert!(matches!(
        browser.handle(click(column, 6)),
        BrowserAction::Rename { .. }
    ));

    browser.handle(key(LogicalKey::Function(2)));
    let narrow = draw(&mut browser, 18, 7);
    let label = footer(&narrow);
    assert!(!label.contains("Save"));
    let cancel = label_column(&label, "Cancel");
    assert_eq!(browser.handle(click(17, 6)), BrowserAction::Continue);
    assert!(browser.rename_value().is_some());
    assert_eq!(browser.handle(click(cancel, 6)), BrowserAction::Continue);
    assert!(browser.rename_value().is_none());
}

#[test]
fn rename_without_visible_footer_cannot_save() {
    for (width, height) in [(0, 7), (80, 0), (80, 2), (8, 7)] {
        let mut browser = browser("unchanged", false);
        browser.handle(key(LogicalKey::Function(2)));
        draw(&mut browser, width, height);
        assert_eq!(
            browser.handle(click(7, height.saturating_sub(1))),
            BrowserAction::Continue
        );
        assert_eq!(browser.rename_value(), Some("unchanged"));
    }
}

#[test]
fn browser_open_mouse_matches_keyboard_and_availability() {
    for available in [true, false] {
        let mut browser = browser("session", false);
        if !available {
            browser.items[0].availability = BrowserAvailability::Trashed;
        }
        let expected = browser.handle(key(LogicalKey::Enter));
        browser.status = None;
        let terminal = draw(&mut browser, 80, 7);
        let label = footer(&terminal);
        let column = label_column(&label, "Open");
        assert_eq!(browser.handle(click(column, 6)), expected);
    }
}

#[test]
fn rename_transition_and_status_frame_have_no_stale_confirm_target() {
    let mut browser = browser("unchanged", false);
    draw(&mut browser, 80, 7);
    browser.handle(key(LogicalKey::Function(2)));
    assert_eq!(browser.handle(click(7, 6)), BrowserAction::Continue);
    assert!(browser.rename_value().is_some());
    draw(&mut browser, 80, 7);
    browser.handle(key(LogicalKey::Escape));
    assert_eq!(browser.handle(click(7, 6)), BrowserAction::Continue);
    assert!(browser.rename_value().is_none());

    browser.items[0].availability = BrowserAvailability::Trashed;
    browser.handle(key(LogicalKey::Enter));
    let terminal = draw(&mut browser, 80, 7);
    assert!(footer(&terminal).contains("Restore this session"));
    assert_eq!(browser.handle(click(3, 6)), BrowserAction::Continue);
    assert!(
        browser.rename_value().is_none(),
        "hidden Rename cannot be clicked"
    );
}

#[test]
fn browser_query_and_rename_history_absorb_underlying_browser_history() {
    let mut browser = browser("session", false);
    let underlying = history_entry(&browser, crate::domain::BrowserOperationKind::Trash);
    browser.history = BrowserHistoryStatus {
        undo: Some(underlying),
        redo: None,
    };
    browser.handle(key(LogicalKey::Character('界')));
    assert_eq!(browser.query(), "界");
    assert_eq!(
        browser.handle(primary_key('z', false)),
        BrowserAction::Continue
    );
    assert_eq!(browser.query(), "");
    assert_eq!(
        browser.handle(primary_key('z', false)),
        BrowserAction::Continue
    );
    assert_eq!(
        browser.handle(primary_key('z', true)),
        BrowserAction::Continue
    );
    assert_eq!(browser.query(), "界");

    let mut rename_browser = self::browser("session", false);
    rename_browser.history = browser.history_status();
    rename_browser.handle(key(LogicalKey::Function(2)));
    rename_browser.handle(key(LogicalKey::Character('x')));
    assert_eq!(
        rename_browser.handle(primary_key('z', false)),
        BrowserAction::Continue
    );
    assert_eq!(rename_browser.rename_value(), Some("session"));
    rename_browser.handle(key(LogicalKey::Escape));

    let mut reopened = self::browser("session", false);
    reopened.history = browser.history_status();
    assert_eq!(
        reopened.handle(primary_key('z', false)),
        BrowserAction::History {
            undo: true,
            target: underlying,
        }
    );
}

#[test]
fn browser_history_footer_label_and_pointer_share_the_same_owner() {
    let mut browser = browser("session", false);
    let underlying = history_entry(&browser, crate::domain::BrowserOperationKind::Rename);
    browser.history = BrowserHistoryStatus {
        undo: Some(underlying),
        redo: None,
    };
    let terminal = draw(&mut browser, 120, 7);
    let line = footer(&terminal);
    assert!(line.contains("Undo session rename"), "{line}");
    let column = label_column(&line, "Undo session rename");
    assert_eq!(
        browser.handle(click(column, 6)),
        BrowserAction::History {
            undo: true,
            target: underlying,
        }
    );
}

#[test]
fn active_session_history_is_unavailable_in_keys_and_footer() {
    let mut source = browser("active", false);
    let target = history_entry(&source, crate::domain::BrowserOperationKind::Trash);
    source.items[0].availability = BrowserAvailability::Active(InstanceInfo {
        instance_id: "ins_06g3cnfeelq3707alnmfsvn1vo"
            .parse()
            .expect("instance ID"),
        session_id: target.session_id,
        pid: 419,
        version: "0.8.0".to_owned(),
        storage_protocol: 14,
        control_protocol: Some(9),
        control_endpoint: Some("synthetic-control-endpoint".to_owned()),
        update: None,
        launch_directory: "synthetic-launch-directory".to_owned(),
        started_at: Timestamp::from_millis(10),
    });
    let mut browser = SessionBrowser::with_shortcut_registry(
        source.items,
        Timestamp::from_millis(20),
        crate::ui::ShortcutRegistry::default(),
        BrowserHistoryStatus {
            undo: Some(target),
            redo: None,
        },
    );

    assert_eq!(browser.history_status().undo, None);
    assert_eq!(
        browser.handle(primary_key('z', false)),
        BrowserAction::Continue
    );
    let terminal = draw(&mut browser, 120, 7);
    assert!(!footer(&terminal).contains("Undo"));
}
