//! Generated model sequences preserve board normalization and reversible state.

use proptest::prelude::*;
use proqi::{
    adapters::memory::FakeIdGenerator,
    application::{Action, AppState, reduce},
    domain::{Session, SessionBoard, Timestamp, UndoScope},
    ports::environment::IdGenerator,
};

proptest! {
    #[test]
    fn arbitrary_reorders_fully_undo_to_initial_order(moves in prop::collection::vec(0_usize..16, 0..100)) {
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-properties"),
            Timestamp::from_millis(1),
        ).expect("session");
        let mut state = AppState::new(SessionBoard::new(session, Vec::new()).expect("board"));
        let mut created = Vec::new();
        for index in 0..16 {
            let thought_id = ids.thought_id();
            created.push(thought_id);
            reduce(&mut state, Action::CreateThought {
                thought_id,
                operation_id: ids.operation_id(),
                content: index.to_string(),
                insertion_index: None,
                at: Timestamp::from_millis(i64::from(index) + 2),
            }).expect("create");
        }
        let initial: Vec<_> = state.board.live_thoughts().iter().map(|thought| thought.id).collect();
        let history_before_moves = state.board_history_cursor();
        let mut applied = 0;
        for (step, raw_target) in moves.into_iter().enumerate() {
            let live = state.board.live_thoughts();
            let thought_id = live[step % live.len()].id;
            let target = raw_target % live.len();
            let effects = reduce(&mut state, Action::MoveThought {
                operation_id: ids.operation_id(),
                thought_id,
                to: target,
                at: Timestamp::from_millis(i64::try_from(step).unwrap_or(i64::MAX) + 100),
            }).expect("move");
            if !effects.is_empty() {
                applied += 1;
            }
            state.board.validate().expect("normalized after move");
        }
        for step in 0..applied {
            reduce(&mut state, Action::Undo {
                operation_id: ids.operation_id(),
                scope: UndoScope::Board,
                at: Timestamp::from_millis(i64::from(step) + 1_000),
            }).expect("undo move");
            state.board.validate().expect("normalized after undo");
        }
        prop_assert_eq!(state.board_history_cursor(), history_before_moves);
        let restored: Vec<_> = state.board.live_thoughts().iter().map(|thought| thought.id).collect();
        prop_assert_eq!(restored, initial.clone());
        prop_assert_eq!(created, initial);
    }
}
