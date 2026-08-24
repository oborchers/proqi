//! Search, availability, responsive rendering, and mouse contracts for session resume.

use std::path::PathBuf;

use proqi::{
    adapters::memory::FakeIdGenerator,
    domain::{Direction, IntegrationContext, Timestamp},
    ports::{environment::IdGenerator, runtime::InstanceInfo, store::SessionHit},
    ui::{
        BrowserAction, BrowserAvailability, PointerButton, PointerInput, PointerKind,
        SessionBrowser, SessionBrowserItem, Theme, ThemePreference, UiInput, UiKey, render_browser,
    },
};
use ratatui_core::{backend::TestBackend, buffer::Buffer, terminal::Terminal};

fn item(
    ids: &mut FakeIdGenerator,
    name: Option<&str>,
    path: PathBuf,
    preview: &str,
    active_at: i64,
    availability: fn() -> BrowserAvailability,
) -> SessionBrowserItem {
    let id = ids.session_id();
    SessionBrowserItem {
        hit: SessionHit {
            id,
            name: name.map(str::to_owned),
            origin_cwd: path.clone(),
            last_opened_cwd: path,
            last_opened_at: Timestamp::from_millis(active_at),
            last_active_at: Timestamp::from_millis(active_at),
            thought_count: 2,
            excerpt: preview.to_owned(),
            previews: vec![preview.to_owned()],
            search_content: preview.to_owned(),
            integration_context: None,
            trashed: matches!(availability(), BrowserAvailability::Trashed),
        },
        availability: availability(),
    }
}

fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

fn resumable() -> BrowserAvailability {
    BrowserAvailability::Resumable
}

fn recovered() -> BrowserAvailability {
    BrowserAvailability::Recovered
}

fn trashed() -> BrowserAvailability {
    BrowserAvailability::Trashed
}

fn active(ids: &mut FakeIdGenerator, hit: SessionHit) -> SessionBrowserItem {
    let instance = InstanceInfo {
        instance_id: ids.instance_id(),
        session_id: hit.id,
        pid: 419,
        version: "0.0.1".to_owned(),
        storage_protocol: 1,
        control_protocol: Some(1),
        control_endpoint: Some(
            test_path("proqi-control.sock")
                .to_string_lossy()
                .into_owned(),
        ),
        launch_directory: test_path("agent").to_string_lossy().into_owned(),
        started_at: Timestamp::from_millis(1_000),
    };
    SessionBrowserItem {
        hit,
        availability: BrowserAvailability::Active(instance),
    }
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
        .expect("draw browser");
    terminal
}

fn text(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn search_matches_name_path_and_thought_content_without_reordering() {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut items = vec![
        item(
            &mut ids,
            Some("Release notes"),
            test_path("alpha"),
            "ordinary prompt",
            900_000_000,
            resumable,
        ),
        item(
            &mut ids,
            None,
            test_path("gamma-project"),
            "Unicode resize investigation",
            800_000_000,
            recovered,
        ),
    ];
    items[1].hit.search_content =
        "first preview\nsecond preview\nUnicode resize investigation".to_owned();
    let second = items[1].hit.id;
    let mut browser = SessionBrowser::new(items, Timestamp::from_millis(900_000_000));
    for character in "gamma unicode".chars() {
        assert_eq!(
            browser.handle(UiInput::Key(UiKey::Character(character))),
            BrowserAction::Continue
        );
    }
    assert_eq!(
        browser.selected_item().map(|(_, item)| item.hit.id),
        Some(second)
    );
    assert_eq!(
        browser.handle(UiInput::Key(UiKey::Enter)),
        BrowserAction::Open(second)
    );
}

#[test]
fn active_and_trashed_results_are_visible_but_cannot_open() {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let active_seed = item(
        &mut ids,
        Some("Live agent"),
        test_path("live"),
        "still editing",
        20,
        resumable,
    );
    let active_item = active(&mut ids, active_seed.hit);
    let trashed_item = item(
        &mut ids,
        Some("Old"),
        test_path("old"),
        "recover me",
        10,
        trashed,
    );
    let mut browser =
        SessionBrowser::new(vec![active_item, trashed_item], Timestamp::from_millis(100));

    let rendered = draw(&mut browser, 100, 14);
    let rendered = text(rendered.backend().buffer());
    assert!(rendered.contains("[active]"));
    assert!(rendered.contains("owner: pid 419 from"));

    assert_eq!(
        browser.handle(UiInput::Key(UiKey::Enter)),
        BrowserAction::Continue
    );
    assert!(
        browser
            .status
            .as_deref()
            .is_some_and(|value| value.contains("419"))
    );
    browser.handle(UiInput::Key(UiKey::Character('j')));
    assert_eq!(
        browser.handle(UiInput::Key(UiKey::Enter)),
        BrowserAction::Continue
    );
    assert!(
        browser
            .status
            .as_deref()
            .is_some_and(|value| value.contains("Restore"))
    );
}

#[test]
fn wide_and_narrow_buffers_show_states_groups_and_selected_detail() {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut items = vec![
        item(
            &mut ids,
            Some("Current"),
            test_path("current"),
            "First preview",
            900_000_000,
            resumable,
        ),
        item(
            &mut ids,
            None,
            test_path("older"),
            "Derived unnamed excerpt",
            1,
            recovered,
        ),
    ];
    let original = test_path("original");
    items[0].hit.origin_cwd.clone_from(&original);
    items[0].hit.integration_context = Some(IntegrationContext {
        provider: "herdr".to_owned(),
        direction: Direction::Left,
        agent_kind: "codex".to_owned(),
        agent_name: "Codex review".to_owned(),
        workspace_hint: Some("proqi".to_owned()),
        tab_hint: Some("main".to_owned()),
        pane_hint: Some("right".to_owned()),
        verified_at: Timestamp::from_millis(899_000_000),
    });
    let mut browser = SessionBrowser::new(items, Timestamp::from_millis(900_000_000));
    let wide = draw(&mut browser, 100, 18);
    let wide_text = text(wide.backend().buffer());
    assert!(wide_text.contains("Resume a Proqi session"));
    assert!(wide_text.contains("Today"));
    assert!(wide_text.contains("Older"));
    assert!(wide_text.contains("state: resumable"));
    assert!(wide_text.contains("origin:"));
    assert!(wide_text.contains("opened from:"));
    assert!(wide_text.contains("integration: herdr / left"));
    assert!(wide_text.contains("agent: Codex review (codex)"));

    let narrow = draw(&mut browser, 44, 14);
    let narrow_text = text(narrow.backend().buffer());
    assert!(narrow_text.contains("Current  [resumable]"));
    assert!(narrow_text.contains("origin:"));
    assert!(narrow_text.contains("opened from:"));
    assert!(narrow_text.contains("Search:"));
}

#[test]
fn mouse_uses_rendered_rows_and_footer_geometry() {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let first = item(
        &mut ids,
        Some("First"),
        test_path("first"),
        "first",
        11,
        resumable,
    );
    let second = item(
        &mut ids,
        Some("Mouse target"),
        test_path("mouse"),
        "clickable",
        10,
        recovered,
    );
    let id = second.hit.id;
    let mut browser = SessionBrowser::new(vec![first, second], Timestamp::from_millis(20));
    let layout = browser.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 100, 10));
    let row = layout.entries[1].row;
    assert_eq!(
        browser.handle(UiInput::Pointer(PointerInput {
            column: row.x,
            row: row.y,
            kind: PointerKind::Down(PointerButton::Left),
        })),
        BrowserAction::Open(id)
    );

    let footer = layout.footer;
    assert_eq!(
        browser.handle(UiInput::Pointer(PointerInput {
            column: footer.x.saturating_add(12),
            row: footer.y,
            kind: PointerKind::Down(PointerButton::Left),
        })),
        BrowserAction::Trash(id)
    );
    assert_eq!(
        browser.handle(UiInput::Pointer(PointerInput {
            column: footer.x.saturating_add(30),
            row: footer.y,
            kind: PointerKind::Down(PointerButton::Left),
        })),
        BrowserAction::Cancel
    );
}

#[test]
fn keyboard_rename_and_trash_are_explicit_browser_actions() {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let entry = item(
        &mut ids,
        None,
        test_path("manage"),
        "managed",
        10,
        resumable,
    );
    let id = entry.hit.id;
    let mut browser = SessionBrowser::new(vec![entry], Timestamp::from_millis(20));
    assert_eq!(
        browser.handle(UiInput::Key(UiKey::Character('R'))),
        BrowserAction::Continue
    );
    for character in "Release queue".chars() {
        browser.handle(UiInput::Key(UiKey::Character(character)));
    }
    assert_eq!(
        browser.handle(UiInput::Key(UiKey::Enter)),
        BrowserAction::Rename {
            session_id: id,
            name: Some("Release queue".to_owned()),
        }
    );
    assert_eq!(
        browser.handle(UiInput::Key(UiKey::Character('D'))),
        BrowserAction::Trash(id)
    );
}
