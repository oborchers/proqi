//! Board submission chord aliases at selection and insertion boundaries.

use super::{Fixture, draw, text};

use proqi::{
    application::Effect,
    domain::Direction,
    ports::agent::{AgentError, AgentState, AgentTarget, SubmissionDisposition, SubmissionReceipt},
    ui::{
        HitTarget, KeyBindings, PointerButton, PointerInput, PointerKind, UiInput, UiKey,
        UiSettings,
    },
};
use ratatui_core::layout::Rect;
use unicode_width::UnicodeWidthStr as _;

#[test]
fn board_help_and_footer_share_both_submission_spellings() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));
    let primary = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    };
    let remove = format!("s/{primary}Enter Submit");
    let keep = format!("S/{primary}Shift+Enter Submit & keep");

    let footer = text(draw(&mut fixture, 120, 12).backend().buffer());
    assert!(footer.contains(&remove));
    assert!(footer.contains(&keep));

    fixture.input(UiInput::Key(UiKey::Character('?')));
    let help = text(draw(&mut fixture, 120, 32).backend().buffer());
    assert!(help.contains(&remove));
    assert!(help.contains(&keep));
}

#[test]
fn remapped_wide_keys_keep_primary_aliases_and_exact_responsive_hit_geometry() {
    let bindings = KeyBindings {
        submit_remove: '界',
        submit_keep: '語',
        ..KeyBindings::default()
    };
    let settings = UiSettings {
        keybindings: bindings,
        ..UiSettings::default()
    };
    let target = super::agent::target(Direction::Right, "w1:p2");
    assert_remapped_aliases(&settings, &target);

    let mut fixture = Fixture::with_settings(settings);
    super::agent::prepare_thought(&mut fixture);
    fixture.app.complete_agent_discovery(Ok(vec![target]));
    for old in ['s', 'S'] {
        assert!(
            fixture
                .effects(UiInput::Key(UiKey::Character(old)))
                .is_empty()
        );
    }

    let primary = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    };
    let labels = [
        (
            SubmissionDisposition::RemoveAfterSuccess,
            format!("界/{primary}Enter Submit"),
            format!("界/{primary}↵ Send"),
        ),
        (
            SubmissionDisposition::Keep,
            format!("語/{primary}Shift+Enter Submit & keep"),
            format!("語/{primary}⇧↵ Keep"),
        ),
    ];
    let wide = text(draw(&mut fixture, 120, 12).backend().buffer());
    for (_, full, _) in &labels {
        let rendered = full.replace("界/", "界 /").replace("語/", "語 /");
        assert!(wide.contains(&rendered), "missing {full:?} in:\n{wide}");
    }
    fixture.input(UiInput::Key(UiKey::Character('?')));
    let help = text(draw(&mut fixture, 120, 32).backend().buffer());
    for (_, full, _) in &labels {
        let rendered = full.replace("界/", "界 /").replace("語/", "語 /");
        assert!(help.contains(&rendered), "missing {full:?} in:\n{help}");
    }
    fixture.input(UiInput::Key(UiKey::Escape));

    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 58, 8));
    let narrow = text(draw(&mut fixture, 58, 8).backend().buffer());
    for (disposition, _, compact) in labels {
        let target = HitTarget::Deliver(Direction::Right, disposition);
        let area = layout
            .controls
            .iter()
            .find_map(|(candidate, area)| (*candidate == target).then_some(*area))
            .expect("responsive submission control");
        assert_eq!(usize::from(area.width), compact.width());
        let rendered = compact.replace("界/", "界 /").replace("語/", "語 /");
        assert!(
            narrow.contains(&rendered),
            "missing {compact:?} in:\n{narrow}"
        );
    }
}

fn assert_remapped_aliases(settings: &UiSettings, target: &AgentTarget) {
    for (key, disposition) in [
        (
            UiKey::Character('界'),
            SubmissionDisposition::RemoveAfterSuccess,
        ),
        (UiKey::Submit, SubmissionDisposition::RemoveAfterSuccess),
        (UiKey::Character('語'), SubmissionDisposition::Keep),
        (UiKey::SubmitKeep, SubmissionDisposition::Keep),
    ] {
        let mut fixture = Fixture::with_settings(settings.clone());
        super::agent::prepare_thought(&mut fixture);
        fixture
            .app
            .complete_agent_discovery(Ok(vec![target.clone()]));
        let effects = fixture.effects(UiInput::Key(key));
        assert!(matches!(
            effects.as_slice(),
            [Effect::PrepareSubmission(attempt)] if attempt.disposition == disposition
        ));
    }
}

#[test]
fn minimum_pair_width_keeps_both_wide_key_hit_targets_and_mouse_delivery() {
    let settings = UiSettings {
        keybindings: KeyBindings {
            submit_remove: '界',
            submit_keep: '語',
            ..KeyBindings::default()
        },
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    super::agent::prepare_thought(&mut fixture);
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));

    let rendered = text(draw(&mut fixture, 11, 8).backend().buffer());
    assert!(rendered.contains('界'));
    assert!(rendered.contains('語'));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 11, 8));
    let controls = [
        SubmissionDisposition::RemoveAfterSuccess,
        SubmissionDisposition::Keep,
    ]
    .map(|disposition| {
        layout
            .controls
            .iter()
            .find_map(|(target, area)| {
                (*target == HitTarget::Deliver(Direction::Right, disposition)).then_some(*area)
            })
            .expect("minimal submission control")
    });
    assert_eq!((controls[0].width, controls[1].width), (2, 2));
    assert_eq!(controls[0].right().saturating_add(3), controls[1].x);

    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: controls[1].x,
        row: controls[1].y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert!(matches!(
        effects.as_slice(),
        [Effect::PrepareSubmission(attempt)]
            if attempt.disposition == SubmissionDisposition::Keep
    ));
}

#[test]
fn primary_enter_submits_the_focused_thought_and_removes_only_after_acceptance() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    let target = super::agent::target(Direction::Right, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));

    let effects = fixture.effects(UiInput::Key(UiKey::Submit));
    let request = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(request.content, "exact prompt\nGrüße 第二行");
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);

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
        [Effect::StoreIntegrationContext { .. }]
    ));
    assert!(fixture.app.state.board.live_thoughts().is_empty());

    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "exact prompt\nGrüße 第二行"
    );
    fixture.input(UiInput::Key(UiKey::Redo));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "exact prompt\nGrüße 第二行"
    );
}

#[test]
fn primary_shift_enter_keeps_one_contiguous_selection_after_one_ordered_delivery() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.paste("second thought with 👩‍💻 and e\u{301}");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.acknowledge_all_persistence();
    fixture.input(UiInput::Key(UiKey::Character('K')));
    let target = super::agent::target(Direction::Left, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));

    let effects = fixture.effects(UiInput::Key(UiKey::SubmitKeep));
    let request = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(
        request.content,
        "exact prompt\nGrüße 第二行\n\nsecond thought with 👩‍💻 and e\u{301}"
    );
    let completion = super::agent::finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target,
            post_state: Some(AgentState::Blocked),
        }),
    );
    assert!(matches!(
        completion.as_slice(),
        [Effect::StoreIntegrationContext { .. }]
    ));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
}

#[test]
fn insertion_row_alias_matches_the_board_command_and_failure_keeps_the_source() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.input(UiInput::Key(UiKey::Character('j')));
    assert!(fixture.app.insertion_focused());
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));

    let effects = fixture.effects(UiInput::Key(UiKey::Submit));
    let request = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(request.content, "exact prompt\nGrüße 第二行");
    assert!(
        super::agent::finish_submission(&mut fixture, &request, Err(AgentError::TimedOut))
            .is_empty()
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert!(fixture.app.insertion_focused());
}

#[test]
fn empty_board_and_duplicate_alias_input_do_not_create_extra_attempts() {
    for key in [UiKey::Submit, UiKey::SubmitKeep] {
        let mut empty = Fixture::new();
        empty.input(UiInput::Key(UiKey::Escape));
        empty
            .app
            .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));
        assert!(empty.effects(UiInput::Key(key)).is_empty());
        assert!(empty.app.state.board.live_thoughts().is_empty());
    }

    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));
    let first = fixture.effects(UiInput::Key(UiKey::Submit));
    assert!(matches!(first.as_slice(), [Effect::PrepareSubmission(_)]));
    assert!(fixture.effects(UiInput::Key(UiKey::SubmitKeep)).is_empty());
    assert_eq!(
        fixture.app.status_text(),
        Some("this thought already has a submission in progress")
    );
}

#[test]
fn primary_keep_alias_enters_the_existing_direction_and_mouse_path() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    let up = super::agent::target(Direction::Up, "w1:p2");
    let right = super::agent::target(Direction::Right, "w1:p3");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![up.clone(), right]));

    assert!(fixture.effects(UiInput::Key(UiKey::SubmitKeep)).is_empty());
    assert_eq!(
        fixture.app.submission_mode(),
        Some(SubmissionDisposition::Keep)
    );
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 100, 10));
    let (_, control) = layout
        .controls
        .iter()
        .find(|(target, _)| {
            *target == proqi::ui::HitTarget::Deliver(Direction::Up, SubmissionDisposition::Keep)
        })
        .expect("mouse keep control");
    let clicked = fixture.effects(UiInput::Pointer(PointerInput {
        column: control.x,
        row: control.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    let request = super::agent::start_submission(&mut fixture, &clicked);
    assert_eq!(request.target, up);
}
