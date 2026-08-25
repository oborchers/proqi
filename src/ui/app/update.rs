//! Temporary update-barrier state around ordinary durable session resume.

use crate::{
    application::{DurabilityState, Effect, UpdateIntent},
    domain::{InstallationKind, RequestId, StableVersion, Timestamp},
    ui::{HitTarget, PointerButton, PointerKind, UiInput, UiKey},
};

use super::BoardApp;

pub(super) struct UpdateBarrier {
    operation_id: RequestId,
    deadline: Timestamp,
}

pub(super) struct UpdatePrompt {
    version: StableVersion,
    installation: InstallationKind,
    participants: usize,
    selected: usize,
}

impl BoardApp {
    pub(crate) fn present_update(
        &mut self,
        version: StableVersion,
        installation: InstallationKind,
        participants: usize,
    ) {
        if installation == InstallationKind::SourceOrUnknown {
            return;
        }
        self.help = false;
        self.palette = None;
        self.search = None;
        self.rename = None;
        self.transfer = None;
        self.update_prompt = Some(UpdatePrompt {
            version,
            installation,
            participants,
            selected: 0,
        });
        self.layout = None;
    }

    pub(super) fn handle_update_prompt_input(&mut self, input: &UiInput) -> Vec<Effect> {
        match input {
            UiInput::Key(UiKey::Escape) => self.choose_update(1),
            UiInput::Key(UiKey::Enter) => {
                let selected = self
                    .update_prompt
                    .as_ref()
                    .map_or(0, |prompt| prompt.selected);
                self.choose_update(selected)
            }
            UiInput::Key(UiKey::Move { movement, .. }) => {
                match movement {
                    crate::ports::editor::CursorMovement::VisualUp => {
                        self.move_update_selection(-1);
                    }
                    crate::ports::editor::CursorMovement::VisualDown => {
                        self.move_update_selection(1);
                    }
                    _ => {}
                }
                Vec::new()
            }
            UiInput::Key(UiKey::Character('j')) => {
                self.move_update_selection(1);
                Vec::new()
            }
            UiInput::Key(UiKey::Character('k')) => {
                self.move_update_selection(-1);
                Vec::new()
            }
            UiInput::Pointer(pointer)
                if matches!(pointer.kind, PointerKind::Down(PointerButton::Left)) =>
            {
                match self
                    .layout
                    .as_ref()
                    .and_then(|layout| layout.hit_test(pointer.column, pointer.row))
                {
                    Some(HitTarget::PaletteItem(index)) => self.choose_update(index),
                    Some(HitTarget::CloseOverlay) => self.choose_update(1),
                    _ => Vec::new(),
                }
            }
            UiInput::Resize { .. } => {
                self.layout = None;
                Vec::new()
            }
            UiInput::HostFocusGained
            | UiInput::Key(_)
            | UiInput::Pointer(_)
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_) => Vec::new(),
        }
    }

    pub(in crate::ui) fn update_prompt_view(&self) -> Option<(String, Vec<String>, usize)> {
        self.update_prompt.as_ref().map(|prompt| {
            let primary = match prompt.installation {
                InstallationKind::HomebrewFormula => format!(
                    "Update and restart all {} {}",
                    prompt.participants,
                    if prompt.participants == 1 {
                        "session"
                    } else {
                        "sessions"
                    }
                ),
                InstallationKind::StandaloneArchive | InstallationKind::SourceOrUnknown => {
                    "View update instructions".to_owned()
                }
            };
            (
                format!(" update available · {} ", prompt.version),
                vec![
                    primary,
                    "Not now".to_owned(),
                    format!("Skip {}", prompt.version),
                ],
                prompt.selected,
            )
        })
    }

    pub(crate) fn complete_update_action(&mut self, result: Result<String, String>) {
        match result {
            Ok(message) => self.set_success(message),
            Err(message) => self.set_error(message),
        }
    }

    fn move_update_selection(&mut self, delta: isize) {
        if let Some(prompt) = &mut self.update_prompt {
            prompt.selected = prompt.selected.saturating_add_signed(delta).min(2);
        }
    }

    fn choose_update(&mut self, index: usize) -> Vec<Effect> {
        let Some(prompt) = self.update_prompt.take() else {
            return Vec::new();
        };
        self.layout = None;
        let intent = match index {
            0 if prompt.installation == InstallationKind::HomebrewFormula => {
                self.set_warning(format!(
                    "Preparing {} Proqi {} for update.",
                    prompt.participants,
                    if prompt.participants == 1 {
                        "session"
                    } else {
                        "sessions"
                    }
                ));
                UpdateIntent::Install(prompt.version)
            }
            0 => UpdateIntent::ViewInstructions(prompt.version),
            2 => UpdateIntent::Skip(prompt.version),
            _ => UpdateIntent::Dismiss(prompt.version),
        };
        vec![Effect::Update(intent)]
    }

    pub(crate) fn begin_update_barrier(
        &mut self,
        operation_id: RequestId,
        deadline: Timestamp,
    ) -> bool {
        if self
            .update_barrier
            .as_ref()
            .is_some_and(|barrier| barrier.operation_id != operation_id)
        {
            return false;
        }
        self.update_barrier = Some(UpdateBarrier {
            operation_id,
            deadline,
        });
        self.set_warning("Ready for Proqi update. Waiting for all sessions.");
        true
    }

    pub(crate) fn release_update_barrier(&mut self, operation_id: RequestId) -> bool {
        if self
            .update_barrier
            .as_ref()
            .is_none_or(|barrier| barrier.operation_id != operation_id)
        {
            return false;
        }
        self.update_barrier = None;
        self.set_info("Update cancelled. Session is ready.");
        true
    }

    pub(crate) fn expire_update_barrier(&mut self, now: Timestamp) -> bool {
        if self
            .update_barrier
            .as_ref()
            .is_none_or(|barrier| now < barrier.deadline)
        {
            return false;
        }
        self.update_barrier = None;
        self.set_warning("Update coordinator timed out. Session is ready.");
        true
    }

    pub(crate) fn request_update_restart(
        &mut self,
        operation_id: RequestId,
        installed: StableVersion,
    ) -> bool {
        if self
            .update_barrier
            .as_ref()
            .is_none_or(|barrier| barrier.operation_id != operation_id)
        {
            return false;
        }
        self.update_restart = Some(installed);
        self.quit = true;
        true
    }

    pub(crate) fn update_restart(&self) -> Option<&StableVersion> {
        self.update_restart.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn update_barrier_operation(&self) -> Option<RequestId> {
        self.update_barrier
            .as_ref()
            .map(|barrier| barrier.operation_id)
    }

    pub(crate) fn update_preflight_ready(&self) -> bool {
        self.pending_edit.is_none()
            && matches!(self.state.durability, DurabilityState::Durable { .. })
    }

    pub(crate) fn update_preflight_failed(&self) -> bool {
        matches!(self.state.durability, DurabilityState::Failed { .. })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::{
            editor::RopeEditorFactory,
            memory::{FakeClock, FakeIdGenerator},
        },
        application::{AppState, Effect, UpdateIntent},
        domain::{InstallationKind, Session, SessionBoard, StableVersion, Timestamp},
        ports::environment::IdGenerator as _,
        ui::{
            PointerButton, PointerInput, PointerKind, Theme, ThemePreference, UiInput, UiKey,
            render,
        },
    };
    use ratatui_core::{backend::TestBackend, layout::Rect, terminal::Terminal};

    use super::BoardApp;

    fn app() -> (BoardApp, FakeIdGenerator, FakeClock) {
        let mut ids = FakeIdGenerator::new(1_800_000_000_000);
        let mut session = Session::new(
            ids.session_id(),
            std::env::temp_dir(),
            Timestamp::from_millis(1),
        )
        .expect("session");
        session
            .rename(Some("fixture".to_owned()))
            .expect("fixture session name");
        let board = SessionBoard::new(session, Vec::new()).expect("board");
        (
            BoardApp::new(AppState::new(board), RopeEditorFactory),
            ids,
            FakeClock::new(Timestamp::from_millis(2)),
        )
    }

    fn version() -> StableVersion {
        StableVersion::parse("1.2.3").expect("stable version")
    }

    #[test]
    fn barrier_blocks_competing_attempts_and_expires_safely() {
        let (mut app, mut ids, _) = app();
        let operation = ids.request_id();
        assert!(app.begin_update_barrier(operation, Timestamp::from_millis(10)));
        assert!(!app.begin_update_barrier(ids.request_id(), Timestamp::from_millis(11)));
        assert!(!app.expire_update_barrier(Timestamp::from_millis(9)));
        assert!(app.expire_update_barrier(Timestamp::from_millis(10)));
        assert_eq!(app.update_barrier_operation(), None);
    }

    #[test]
    fn keyboard_choices_emit_one_explicit_update_intent() {
        let (mut app, mut ids, clock) = app();
        app.present_update(version(), InstallationKind::HomebrewFormula, 3);

        let effects = app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);

        assert_eq!(
            effects,
            vec![Effect::Update(UpdateIntent::Install(version()))]
        );

        app.present_update(version(), InstallationKind::StandaloneArchive, 1);
        let effects = app.handle(UiInput::Key(UiKey::Escape), &mut ids, &clock);
        assert_eq!(
            effects,
            vec![Effect::Update(UpdateIntent::Dismiss(version()))]
        );
    }

    #[test]
    fn mouse_can_skip_the_offered_release() {
        let (mut app, mut ids, clock) = app();
        app.present_update(version(), InstallationKind::HomebrewFormula, 2);
        let layout = app.prepare_frame(Rect::new(0, 0, 60, 12));
        let skip = layout.overlay.expect("update overlay").items[2];

        let effects = app.handle(
            UiInput::Pointer(PointerInput {
                column: skip.x,
                row: skip.y,
                kind: PointerKind::Down(PointerButton::Left),
            }),
            &mut ids,
            &clock,
        );

        assert_eq!(effects, vec![Effect::Update(UpdateIntent::Skip(version()))]);
    }

    #[test]
    fn update_prompt_has_a_complete_reviewed_buffer() {
        let (mut app, _, _) = app();
        app.present_update(version(), InstallationKind::HomebrewFormula, 12);
        let mut terminal = Terminal::new(TestBackend::new(64, 12)).expect("terminal");
        terminal
            .draw(|frame| {
                let layout = app.prepare_frame(frame.area());
                render(
                    frame,
                    &app,
                    &layout,
                    &Theme::resolve(ThemePreference::Dark, true),
                );
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let text = (0..buffer.area.height)
            .map(|row| {
                (0..buffer.area.width)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n");

        insta::assert_snapshot!(text);
    }
}
