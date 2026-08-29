use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{
        AgentState, CLAUDE_AGENT_KIND, CODEX_AGENT_KIND, OPENCODE_AGENT_KIND, SubmissionReceipt,
    },
    ui::UiKey,
};

#[test]
fn opencode_routes_correctly_in_both_mixed_harness_positions() {
    for (left_kind, right_kind, key, expected_kind) in [
        (
            CLAUDE_AGENT_KIND,
            OPENCODE_AGENT_KIND,
            'l',
            OPENCODE_AGENT_KIND,
        ),
        (
            OPENCODE_AGENT_KIND,
            CODEX_AGENT_KIND,
            'h',
            OPENCODE_AGENT_KIND,
        ),
    ] {
        let mut fixture = Fixture::new();
        super::agent::prepare_thought(&mut fixture);
        fixture.app.complete_agent_discovery(Ok(vec![
            super::agent::target_with_kind(Direction::Left, "w1:p2", left_kind),
            super::agent::target_with_kind(Direction::Right, "w1:p3", right_kind),
        ]));

        let rendered = text(draw(&mut fixture, 100, 10).backend().buffer());
        assert!(rendered.contains(&format!("← {}", capitalize(left_kind))));
        assert!(rendered.contains(&format!("→ {}", capitalize(right_kind))));
        assert!(
            fixture
                .effects(UiInput::Key(UiKey::Character('s')))
                .is_empty()
        );
        let effects = fixture.effects(UiInput::Key(UiKey::Character(key)));
        let request = super::agent::start_submission(&mut fixture, &effects);
        assert_eq!(request.target.agent_kind.as_str(), expected_kind);
    }
}

fn capitalize(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + characters.as_str()
    })
}

#[test]
fn selected_thoughts_submit_once_in_board_order_and_remove_as_one_undo_step() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.paste("second thought");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.acknowledge_all_persistence();
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    let target = super::agent::target(Direction::Left, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));

    let effects = fixture.effects(UiInput::Key(UiKey::Character('s')));
    let request = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(
        request.content,
        "exact prompt\nGrüße 第二行\n\nsecond thought"
    );
    let completion = super::agent::finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target,
            post_state: Some(AgentState::Working),
        }),
    );
    assert!(matches!(
        completion.as_slice(),
        [
            Effect::StoreIntegrationContext { .. },
            Effect::CommitBoardOperation(_)
        ]
    ));
    assert!(fixture.app.state.board.live_thoughts().is_empty());

    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
}

#[test]
fn merged_prompt_keeps_only_the_first_shared_starter() {
    for harness in [CODEX_AGENT_KIND, CLAUDE_AGENT_KIND] {
        for starter in ["/plan", "/goal"] {
            let mut fixture = Fixture::new();
            fixture.paste(&format!(
                "{starter} first task with an internal {starter} reference"
            ));
            fixture.input(UiInput::Key(UiKey::Escape));
            fixture.paste(&format!(
                "{starter} second task keeps internal {starter} prose"
            ));
            fixture.input(UiInput::Key(UiKey::Escape));
            fixture.acknowledge_all_persistence();
            fixture.input(UiInput::Key(UiKey::Character(' ')));
            fixture.input(UiInput::Key(UiKey::Character('k')));
            fixture.input(UiInput::Key(UiKey::Character(' ')));
            fixture
                .app
                .complete_agent_discovery(Ok(vec![super::agent::target_with_kind(
                    Direction::Left,
                    "w1:p2",
                    harness,
                )]));

            let effects = fixture.effects(UiInput::Key(UiKey::Character('s')));
            let request = super::agent::start_submission(&mut fixture, &effects);
            assert_eq!(
                request.content,
                format!(
                    "{starter} first task with an internal {starter} reference\n\nsecond task keeps internal {starter} prose"
                )
            );
            assert_eq!(
                fixture.app.state.board.live_thoughts()[1].content,
                format!("{starter} second task keeps internal {starter} prose")
            );

            let mut without_first_starter = Fixture::new();
            without_first_starter.paste(&format!("ordinary first task with {starter} prose"));
            without_first_starter.input(UiInput::Key(UiKey::Escape));
            without_first_starter.paste(&format!("{starter} later task"));
            without_first_starter.input(UiInput::Key(UiKey::Escape));
            without_first_starter.acknowledge_all_persistence();
            without_first_starter.input(UiInput::Key(UiKey::Character(' ')));
            without_first_starter.input(UiInput::Key(UiKey::Character('k')));
            without_first_starter.input(UiInput::Key(UiKey::Character(' ')));
            without_first_starter.app.complete_agent_discovery(Ok(vec![
                super::agent::target_with_kind(Direction::Left, "w1:p2", harness),
            ]));
            let effects = without_first_starter.effects(UiInput::Key(UiKey::Character('s')));
            let request = super::agent::start_submission(&mut without_first_starter, &effects);
            assert_eq!(
                request.content,
                format!("ordinary first task with {starter} prose\n\nlater task")
            );
        }
    }
}
