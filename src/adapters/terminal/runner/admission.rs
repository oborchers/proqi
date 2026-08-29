//! Canonical admission for session-sequence producers.

use crate::ports::control::{ControlMutation, ControlRejectionCode, ControlResult};
use crate::ui::BoardApp;

use super::PendingWork;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MutationBlocker {
    CaptureCommit,
    ControlLookup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MutationAdmission {
    capture_reserved: bool,
    unresolved_control_lookups: usize,
}

impl MutationAdmission {
    const fn owner_control(self) -> Result<(), MutationBlocker> {
        if self.capture_reserved {
            Err(MutationBlocker::CaptureCommit)
        } else {
            Ok(())
        }
    }

    const fn capture(self) -> Result<(), MutationBlocker> {
        if self.capture_reserved {
            Err(MutationBlocker::CaptureCommit)
        } else if self.unresolved_control_lookups == 0 {
            Ok(())
        } else {
            Err(MutationBlocker::ControlLookup)
        }
    }
}

pub(super) fn owner_control(app: &BoardApp) -> Result<(), MutationBlocker> {
    MutationAdmission {
        capture_reserved: app.screenshot_sequence_reserved(),
        unresolved_control_lookups: 0,
    }
    .owner_control()
}

pub(super) fn owner_control_rejection(
    app: &BoardApp,
    mutation: &ControlMutation,
) -> Option<ControlResult> {
    (control_may_produce_sequence(mutation) && owner_control(app).is_err()).then(|| {
        ControlResult::Rejected {
            code: ControlRejectionCode::OwnerBusy.as_str().to_owned(),
            message: "active owner is committing a screenshot; retry the control request"
                .to_owned(),
        }
    })
}

pub(super) const fn control_may_produce_sequence(mutation: &ControlMutation) -> bool {
    !matches!(
        mutation,
        ControlMutation::CaptureTakeover { .. }
            | ControlMutation::UpdateRelease { .. }
            | ControlMutation::UpdateRestart { .. }
    )
}

pub(super) fn capture(app: &BoardApp, pending: &PendingWork) -> Result<(), MutationBlocker> {
    MutationAdmission {
        capture_reserved: app.screenshot_sequence_reserved(),
        unresolved_control_lookups: pending.control_lookups.len(),
    }
    .capture()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        adapters::{editor::RopeEditorFactory, memory::FakeIdGenerator},
        application::{AppState, DurabilityState, Effect, InteractionMode},
        domain::{Session, SessionBoard, Timestamp, UndoScope},
        ports::{
            environment::IdGenerator as _,
            screenshot::{ScreenshotCandidate, ScreenshotFingerprint, ScreenshotImageType},
            store::{
                CaptureCommit, CaptureCommitOutcome, CaptureReceipt, CommitReceipt, DurableIdentity,
            },
        },
        ui::{BoardApp, UiInput, UiKey},
    };

    #[test]
    fn capture_and_unresolved_control_lookup_exclude_each_other() {
        let (mut app, mut ids) = app();
        let pending = PendingWork::default();
        assert_eq!(capture(&app, &pending), Ok(()));

        app.screenshot_started(std::time::Duration::ZERO);
        app.queue_screenshot_candidates([ScreenshotCandidate {
            fingerprint: ScreenshotFingerprint([3; 32]),
            path: std::env::temp_dir().join("admission.png"),
            image_type: ScreenshotImageType::Png,
        }]);
        let clock = crate::adapters::memory::FakeClock::new(Timestamp::from_millis(2));
        assert!(matches!(
            app.advance_screenshot_capture(&mut ids, &clock).as_slice(),
            [Effect::CommitCapture(_)]
        ));
        assert_eq!(owner_control(&app), Err(MutationBlocker::CaptureCommit));

        assert_eq!(
            MutationAdmission {
                capture_reserved: false,
                unresolved_control_lookups: 1,
            }
            .capture(),
            Err(MutationBlocker::ControlLookup)
        );
    }

    #[test]
    fn capture_first_retryably_rejects_every_sequence_producing_control() {
        let (mut app, mut ids) = app();
        let clock = crate::adapters::memory::FakeClock::new(Timestamp::from_millis(2));
        let capture = reserve_capture(&mut app, &mut ids, &clock, 4);
        let mutations = [
            ControlMutation::Add {
                operation_id: ids.operation_id(),
                thought_id: ids.thought_id(),
                content: "add".to_owned(),
                annotations: Vec::new(),
                position: None,
            },
            ControlMutation::Replace {
                revision_id: ids.revision_id(),
                thought_id: ids.thought_id(),
                expected_digest: None,
                content: "replace".to_owned(),
            },
            ControlMutation::History {
                operation_id: ids.operation_id(),
                scope: UndoScope::Board,
                undo: true,
            },
            ControlMutation::Sync,
        ];
        for mutation in mutations {
            assert_eq!(
                owner_control_rejection(&app, &mutation),
                Some(ControlResult::Rejected {
                    code: ControlRejectionCode::OwnerBusy.as_str().to_owned(),
                    message: "active owner is committing a screenshot; retry the control request"
                        .to_owned(),
                })
            );
        }
        app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
        assert_eq!(app.state.board.live_thoughts().len(), 1);
        assert!(matches!(
            app.state.durability,
            DurabilityState::Durable { .. }
        ));
    }

    #[test]
    fn control_lookup_first_then_capture_uses_distinct_ordered_sequences() {
        let (mut app, mut ids) = app();
        let clock = crate::adapters::memory::FakeClock::new(Timestamp::from_millis(2));
        assert_eq!(
            MutationAdmission {
                capture_reserved: false,
                unresolved_control_lookups: 1,
            }
            .capture(),
            Err(MutationBlocker::ControlLookup)
        );
        let add = ControlMutation::Add {
            operation_id: ids.operation_id(),
            thought_id: ids.thought_id(),
            content: "control first".to_owned(),
            annotations: Vec::new(),
            position: None,
        };
        let effects = app.handle_control(&add, &clock).expect("control mutation");
        let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
            panic!("control persistence");
        };
        let control_sequence = operation.sequence;
        app.acknowledge_persistence_result(control_sequence, Ok(()));

        let capture = reserve_capture(&mut app, &mut ids, &clock, 5);
        assert!(capture.operation.sequence > control_sequence);
        app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
        assert_eq!(app.state.board.live_thoughts().len(), 2);
        assert!(matches!(
            app.state.durability,
            DurabilityState::Durable { .. }
        ));
    }

    #[test]
    fn sync_pending_edit_completes_before_capture_reserves_a_sequence() {
        let (mut app, mut ids) = app();
        let clock = crate::adapters::memory::FakeClock::new(Timestamp::from_millis(2));
        let add = ControlMutation::Add {
            operation_id: ids.operation_id(),
            thought_id: ids.thought_id(),
            content: "active".to_owned(),
            annotations: Vec::new(),
            position: None,
        };
        let add_effects = app.handle_control(&add, &clock).expect("seed thought");
        let [Effect::CommitBoardOperation(add_operation)] = add_effects.as_slice() else {
            panic!("seed persistence");
        };
        let thought_id = add.thought_id().expect("added thought");
        app.acknowledge_persistence_result(add_operation.sequence, Ok(()));
        app.state.mode = InteractionMode::Edit { thought_id };
        app.state.focused_thought = Some(thought_id);
        app.sync_editor_from_state();
        app.handle(UiInput::Key(UiKey::Character('!')), &mut ids, &clock);

        let edit_effects = app.flush_pending_edit(&mut ids, &clock);
        let [Effect::CommitRevision(revision)] = edit_effects.as_slice() else {
            panic!("sync revision");
        };
        let edit_sequence = revision.sequence;
        app.acknowledge_persistence_result(edit_sequence, Ok(()));
        let capture = reserve_capture(&mut app, &mut ids, &clock, 6);
        assert!(capture.operation.sequence > edit_sequence);
        app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
        assert_eq!(app.state.board.live_thoughts().len(), 2);
        assert!(matches!(
            app.state.durability,
            DurabilityState::Durable { .. }
        ));
    }

    fn reserve_capture(
        app: &mut BoardApp,
        ids: &mut FakeIdGenerator,
        clock: &crate::adapters::memory::FakeClock,
        byte: u8,
    ) -> CaptureCommit {
        app.screenshot_started(std::time::Duration::ZERO);
        app.queue_screenshot_candidates([ScreenshotCandidate {
            fingerprint: ScreenshotFingerprint([byte; 32]),
            path: std::env::temp_dir().join("admission.png"),
            image_type: ScreenshotImageType::Png,
        }]);
        let effects = app.advance_screenshot_capture(ids, clock);
        let [Effect::CommitCapture(capture)] = effects.as_slice() else {
            panic!("capture persistence");
        };
        capture.clone()
    }

    fn created(capture: &CaptureCommit) -> CaptureCommitOutcome {
        let crate::domain::BoardMutation::AddThought { thought } = &capture.operation.forward
        else {
            panic!("capture thought");
        };
        CaptureCommitOutcome::Created {
            durable: CommitReceipt {
                session_id: capture.operation.session_id,
                sequence: capture.operation.sequence,
                identity: DurableIdentity::Operation(capture.operation.id),
                idempotent_replay: false,
            },
            capture: CaptureReceipt {
                source: capture.source,
                session_id: capture.operation.session_id,
                thought_id: thought.id,
                operation_id: capture.operation.id,
                accepted_at: capture.operation.created_at,
            },
        }
    }

    fn app() -> (BoardApp, FakeIdGenerator) {
        let mut ids = FakeIdGenerator::new(1_725_400_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir(),
            Timestamp::from_millis(1),
        )
        .expect("session");
        (
            BoardApp::new(
                AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
                RopeEditorFactory,
            ),
            ids,
        )
    }
}
