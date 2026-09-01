use super::*;
use proqi::domain::{Thought, ThoughtPosition};

impl Fixture {
    /// Seed durable metadata directly for projection tests. This models same-user
    /// store bytes and intentionally bypasses every supported authoring surface.
    pub(super) fn with_annotated_thought(
        content: &str,
        annotations: Vec<ContentAnnotation>,
    ) -> Self {
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-ui-annotated-contract"),
            Timestamp::from_millis(10),
        )
        .expect("session");
        let mut thought = Thought::new(
            ids.thought_id(),
            session.id,
            content.to_owned(),
            ThoughtPosition::new(0),
            Timestamp::from_millis(10),
        );
        thought
            .set_annotations(annotations)
            .expect("valid direct durable fixture");
        let board = SessionBoard::new(session, vec![thought]).expect("board");
        Self {
            app: BoardApp::new(
                AppState::new(board),
                proqi::adapters::editor::RopeEditorFactory,
            ),
            ids,
            clock: FakeClock::new(Timestamp::from_millis(20)),
        }
    }
}
