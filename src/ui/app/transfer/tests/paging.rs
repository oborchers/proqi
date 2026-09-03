use super::view::SessionHitLabel as _;
use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::AppState,
    domain::{Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
    ports::{environment::IdGenerator as _, store::SessionHit},
    ui::{BoardApp, FastNavigation, UiInput, UiKey},
};

#[test]
fn transfer_picker_fast_navigation_counts_only_destination_entries() {
    let mut ids = FakeIdGenerator::new(1_725_210_000_000);
    let clock = FakeClock::new(Timestamp::from_millis(3));
    let source = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-transfer-paging"),
        Timestamp::from_millis(1),
    )
    .expect("source session");
    let thought = Thought::new(
        ids.thought_id(),
        source.id,
        "source".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let board = SessionBoard::new(source, vec![thought]).expect("board");
    let mut app = BoardApp::new(AppState::new(board), RopeEditorFactory);
    app.begin_session_transfer(false, &mut ids, &clock);
    let mut hits = (0..8)
        .map(|index| {
            let mut hit = session_hit(ids.session_id());
            hit.name = Some(format!("destination {index}"));
            hit
        })
        .collect::<Vec<_>>();
    let expected = hits[5].label();
    app.complete_transfer_discovery(Ok(std::mem::take(&mut hits)));
    app.handle_transfer_input(
        &UiInput::Key(UiKey::FastNavigation {
            direction: FastNavigation::Next,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    let (_, visible, selected) = app.transfer_view().expect("transfer picker");
    assert_eq!(visible[selected], expected);

    let replacement = (0..2)
        .map(|index| {
            let mut hit = session_hit(ids.session_id());
            hit.name = Some(format!("replacement {index}"));
            hit
        })
        .collect();
    app.complete_transfer_discovery(Ok(replacement));
    let (_, visible, selected) = app.transfer_view().expect("replacement picker");
    assert_eq!(selected, 0);
    assert!(visible[selected].starts_with("replacement 1"));
}

fn session_hit(id: crate::domain::SessionId) -> SessionHit {
    SessionHit {
        id,
        name: Some("destination".to_owned()),
        origin_cwd: std::env::temp_dir(),
        last_opened_cwd: std::env::temp_dir(),
        last_opened_at: Timestamp::from_millis(1),
        last_active_at: Timestamp::from_millis(1),
        thought_count: 0,
        excerpt: String::new(),
        previews: Vec::new(),
        search_content: String::new(),
        integration_context: None,
        trashed: false,
    }
}
