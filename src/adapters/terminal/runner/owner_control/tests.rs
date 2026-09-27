//! Narrow effect-shape validation for ordinary owner-control mutations.

use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{
        AppState, Effect, FailureCode, SequencedMutationEffectError, SequencedMutationEffects,
    },
    domain::{ContentAnnotation, ContentAnnotationKind, Session, SessionBoard, Timestamp},
    ports::{
        control::{ControlMutation, ControlResult},
        environment::IdGenerator as _,
    },
    ui::BoardApp,
};

use super::{effect_rejection, validate_control_effects};

#[test]
fn one_durable_mutation_and_attachment_checks_share_the_existing_owners() {
    let (mut app, mut ids) = fixture();
    let content = "/tmp/active-transfer-image.png";
    let effects = app
        .handle_control(
            &ControlMutation::PreserveAdd {
                operation_id: ids.operation_id(),
                thought_id: ids.thought_id(),
                content: content.to_owned(),
                annotations: vec![ContentAnnotation {
                    start: 0,
                    end: content.len(),
                    kind: ContentAnnotationKind::Attachment {
                        ordinal: None,
                        image: true,
                        display_name: "active-transfer-image.png".to_owned(),
                    },
                }],
                name: None,
                position: None,
            },
            &FakeClock::new(Timestamp::from_millis(2)),
        )
        .expect("annotated control mutation");

    let routed = SequencedMutationEffects::new(effects).expect("valid routed effects");
    assert_eq!(routed.sequence.get(), 1);
    assert!(matches!(
        routed.auxiliary.as_slice(),
        [Effect::CheckAttachments(_)]
    ));
}

#[test]
fn malformed_effect_shapes_are_rejected_before_the_owner_routes_them() {
    let (mut app, mut ids) = fixture();
    let previous_state = app.state.clone();
    let (_durable, mutation) = optimistic_effect(&mut app, &mut ids);
    assert_shape_restores(
        &mut app,
        &previous_state,
        Vec::new(),
        &mutation,
        SequencedMutationEffectError::MissingDurable,
    );
    let (durable, mutation) = optimistic_effect(&mut app, &mut ids);
    assert_shape_restores(
        &mut app,
        &previous_state,
        vec![durable.clone(), durable],
        &mutation,
        SequencedMutationEffectError::MultipleDurable,
    );
    let (durable, mutation) = optimistic_effect(&mut app, &mut ids);
    assert_shape_restores(
        &mut app,
        &previous_state,
        vec![
            durable,
            Effect::Notify {
                code: FailureCode::StorageFailed,
            },
        ],
        &mutation,
        SequencedMutationEffectError::UnsupportedAuxiliary,
    );

    let (next, _) = optimistic_effect(&mut app, &mut ids);
    let batch = next.persistence_batch().expect("next durable batch");
    assert_eq!(batch.sequence().expect("next sequence").get(), 1);
}

fn optimistic_effect(app: &mut BoardApp, ids: &mut FakeIdGenerator) -> (Effect, ControlMutation) {
    let mutation = ControlMutation::Add {
        operation_id: ids.operation_id(),
        thought_id: ids.thought_id(),
        content: "optimistic".to_owned(),
        annotations: Vec::new(),
        position: None,
    };
    let effects = app
        .handle_control(&mutation, &FakeClock::new(Timestamp::from_millis(2)))
        .expect("control mutation");
    assert_eq!(effects.len(), 1);
    (
        effects.into_iter().next().expect("durable effect"),
        mutation,
    )
}

fn assert_shape_restores(
    app: &mut BoardApp,
    previous_state: &AppState,
    effects: Vec<Effect>,
    mutation: &ControlMutation,
    expected: SequencedMutationEffectError,
) {
    assert_ne!(&app.state, previous_state);
    let error = validate_control_effects(
        app,
        previous_state.clone(),
        effects,
        previous_state.board.session.id,
        mutation,
    )
    .expect_err("invalid owner effect shape");
    assert_eq!(error, expected);
    assert_eq!(&app.state, previous_state);
    let ControlResult::Rejected { code, .. } = effect_rejection(error) else {
        panic!("effect shape must be rejected");
    };
    assert_eq!(
        code,
        if error == SequencedMutationEffectError::MissingDurable {
            "no_durable_mutation"
        } else {
            "invalid_control_request"
        }
    );
}

fn fixture() -> (BoardApp, FakeIdGenerator) {
    let mut ids = FakeIdGenerator::new(1_725_800_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-owner-control-effects"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let state = AppState::new(SessionBoard::new(session, Vec::new()).expect("board"));
    (BoardApp::new(state, RopeEditorFactory), ids)
}
