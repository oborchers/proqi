//! Searchable cross-session destination picker and completion handling.

use crate::{
    application::{Action, Effect},
    domain::BoardOperationKind,
    ports::{
        editor::CursorMovement,
        environment::{Clock, IdGenerator},
        store::{SessionHit, StoreError},
        transfer::SessionTransferRequest,
    },
};

use super::{BoardApp, UiInput, UiKey, query::QueryEditor};

pub(super) struct TransferState {
    query: QueryEditor,
    sessions: Vec<SessionHit>,
    selected: usize,
    scroll: usize,
    source_thought_id: crate::domain::ThoughtId,
    remove_source: bool,
    loading: bool,
}

impl BoardApp {
    pub(super) fn begin_session_transfer(
        &mut self,
        remove_source: bool,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(source_thought_id) = self.active_thought_id() else {
            self.set_warning("select a thought before sending it to another session");
            return Vec::new();
        };
        let mut effects = self.flush_pending_edit(ids, clock);
        self.transfer = Some(TransferState {
            query: QueryEditor::default(),
            sessions: Vec::new(),
            selected: 0,
            scroll: 0,
            source_thought_id,
            remove_source,
            loading: true,
        });
        effects.push(Effect::DiscoverTransferSessions);
        effects
    }

    pub(crate) fn complete_transfer_discovery(
        &mut self,
        result: Result<Vec<SessionHit>, StoreError>,
    ) {
        let Some(state) = &mut self.transfer else {
            return;
        };
        state.loading = false;
        match result {
            Ok(sessions) if sessions.is_empty() => {
                self.transfer = None;
                self.set_warning("no other resumable Proqi session is available");
            }
            Ok(sessions) => state.sessions = sessions,
            Err(error) => {
                self.transfer = None;
                self.set_error(format!("could not list destination sessions: {error}"));
            }
        }
    }

    pub(crate) fn complete_session_transfer(
        &mut self,
        request: &SessionTransferRequest,
        result: Result<crate::application::ThoughtMutation, String>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match result {
            Err(error) => {
                self.set_error(format!("thought was not sent: {error}"));
                Vec::new()
            }
            Ok(_) if !request.remove_source => {
                self.set_success("thought sent to the destination session");
                Vec::new()
            }
            Ok(_) => {
                self.set_info("thought sent; removing the source");
                self.reduce(Action::DeleteThought {
                    operation_id: ids.operation_id(),
                    thought_id: request.source_thought_id,
                    kind: BoardOperationKind::Delete,
                    at: clock.now(),
                })
            }
        }
    }

    pub(super) fn transfer_view(&self) -> Option<(String, Vec<String>, usize)> {
        let state = self.transfer.as_ref()?;
        let matches = state.matches();
        let entries = if state.loading {
            vec!["Loading sessions...".to_owned()]
        } else {
            matches
                .iter()
                .skip(state.scroll)
                .map(|hit| hit.label())
                .collect()
        };
        Some((
            state.query.text().to_owned(),
            entries,
            state.selected.saturating_sub(state.scroll),
        ))
    }

    pub(super) fn transfer_match_count(&self) -> usize {
        self.transfer
            .as_ref()
            .map_or(0, |state| state.matches().len())
    }

    pub(super) fn handle_transfer_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let UiInput::Key(key) = input else {
            return match input {
                UiInput::Pointer(pointer) => self.handle_pointer(*pointer, ids, clock),
                UiInput::Paste(value) => self.update_transfer_query(|query| query.paste(value)),
                UiInput::PasteAnnotated(payload) => {
                    self.update_transfer_query(|query| query.paste(&payload.content))
                }
                _ => Vec::new(),
            };
        };
        match *key {
            UiKey::Escape => self.transfer = None,
            UiKey::Enter => return self.choose_transfer(ids),
            UiKey::Backspace => {
                self.update_transfer_query(QueryEditor::backspace);
            }
            UiKey::Move {
                movement: CursorMovement::VisualUp,
                ..
            } => self.move_transfer(-1),
            UiKey::Move {
                movement: CursorMovement::VisualDown,
                ..
            } => self.move_transfer(1),
            UiKey::Move { movement, .. } => {
                self.update_transfer_query(|query| query.move_cursor(movement));
            }
            UiKey::Delete => {
                self.update_transfer_query(QueryEditor::delete);
            }
            UiKey::Character(character) if !character.is_control() => {
                self.update_transfer_query(|query| query.insert_char(character));
            }
            _ => {}
        }
        Vec::new()
    }

    pub(super) fn choose_transfer_visible(
        &mut self,
        index: usize,
        ids: &mut impl IdGenerator,
    ) -> Vec<Effect> {
        if let Some(state) = &mut self.transfer {
            state.selected = state.scroll.saturating_add(index);
        }
        self.choose_transfer(ids)
    }

    fn choose_transfer(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        if self.transfer.as_ref().is_some_and(|state| state.loading) {
            return Vec::new();
        }
        let request = self.transfer.as_ref().and_then(|state| {
            let destination = state.matches().get(state.selected)?.id;
            let thought = self.state.board.thought(state.source_thought_id)?;
            Some(SessionTransferRequest {
                destination_session_id: destination,
                source_thought_id: thought.id,
                operation_id: ids.operation_id(),
                content: thought.content.clone(),
                annotations: thought.annotations.clone(),
                remove_source: state.remove_source,
            })
        });
        self.transfer = None;
        request.map_or_else(Vec::new, |request| vec![Effect::TransferThought(request)])
    }

    fn update_transfer_query(&mut self, update: impl FnOnce(&mut QueryEditor)) -> Vec<Effect> {
        if let Some(state) = &mut self.transfer {
            update(&mut state.query);
            state.selected = 0;
            state.scroll = 0;
        }
        Vec::new()
    }

    fn move_transfer(&mut self, delta: isize) {
        let visible = self
            .layout
            .as_ref()
            .and_then(|layout| layout.overlay.as_ref())
            .map_or(1, |overlay| overlay.items.len().max(1));
        let Some(state) = &mut self.transfer else {
            return;
        };
        state.selected = state
            .selected
            .saturating_add_signed(delta)
            .min(state.matches().len().saturating_sub(1));
        if state.selected < state.scroll {
            state.scroll = state.selected;
        } else if state.selected >= state.scroll.saturating_add(visible) {
            state.scroll = state.selected + 1 - visible;
        }
    }
}

impl TransferState {
    pub(super) const fn query_cursor(&self) -> usize {
        self.query.cursor()
    }

    fn matches(&self) -> Vec<&SessionHit> {
        let query = self.query.text().to_lowercase();
        self.sessions
            .iter()
            .filter(|hit| {
                query.is_empty()
                    || hit
                        .name
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
                    || hit
                        .last_opened_cwd
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&query)
                    || hit.excerpt.to_lowercase().contains(&query)
            })
            .collect()
    }
}

trait SessionHitLabel {
    fn label(&self) -> String;
}
impl SessionHitLabel for SessionHit {
    fn label(&self) -> String {
        let name = self.name.as_deref().unwrap_or_else(|| {
            if self.excerpt.is_empty() {
                "untitled"
            } else {
                &self.excerpt
            }
        });
        format!(
            "{name} · {} thoughts · {}",
            self.thought_count,
            self.last_opened_cwd.display()
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::{
            editor::RopeEditorFactory,
            memory::{FakeClock, FakeIdGenerator},
        },
        application::{AppState, Effect, ThoughtMutation},
        domain::{
            ContentAnnotation, ContentAnnotationKind, OperationSequence, Session, SessionBoard,
            Thought, ThoughtPosition, Timestamp,
        },
        ports::{
            environment::IdGenerator,
            store::{CommitReceipt, DurableIdentity, SessionHit},
        },
        ui::{BoardApp, UiInput, UiKey},
    };

    #[test]
    fn transfer_preserves_annotations_and_removes_only_after_destination_receipt() {
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let clock = FakeClock::new(Timestamp::from_millis(3));
        let source = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-transfer-source"),
            Timestamp::from_millis(1),
        )
        .expect("source session");
        let destination = ids.session_id();
        let mut thought = Thought::new(
            ids.thought_id(),
            source.id,
            "/tmp/image.png".to_owned(),
            ThoughtPosition::new(0),
            Timestamp::from_millis(1),
        );
        thought
            .set_annotations(vec![ContentAnnotation {
                start: 0,
                end: thought.content.len(),
                kind: ContentAnnotationKind::Attachment {
                    image: true,
                    display_name: "image.png".to_owned(),
                },
            }])
            .expect("annotation");
        let thought_id = thought.id;
        let board = SessionBoard::new(source, vec![thought.clone()]).expect("board");
        let mut app = BoardApp::new(AppState::new(board), RopeEditorFactory);
        assert_eq!(
            app.begin_session_transfer(true, &mut ids, &clock),
            vec![Effect::DiscoverTransferSessions]
        );
        assert_loading_input_is_ignored(&mut app, &mut ids, &clock);
        app.complete_transfer_discovery(Ok(vec![session_hit(destination)]));
        let effects = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock);
        let [Effect::TransferThought(request)] = effects.as_slice() else {
            panic!("expected transfer request");
        };
        assert_eq!(request.content, thought.content);
        assert_eq!(request.annotations, thought.annotations);
        assert_eq!(request.source_thought_id, thought_id);
        assert_thought_is_live(&app, thought_id);
        let failed = app.complete_session_transfer(
            request,
            Err("destination unavailable".to_owned()),
            &mut ids,
            &clock,
        );
        assert!(failed.is_empty());
        assert_thought_is_live(&app, thought_id);
        let receipt = CommitReceipt {
            session_id: destination,
            sequence: OperationSequence::new(1),
            identity: DurableIdentity::Operation(request.operation_id),
            idempotent_replay: false,
        };
        let completion = app.complete_session_transfer(
            request,
            Ok(ThoughtMutation {
                thought_id: ids.thought_id(),
                receipt,
            }),
            &mut ids,
            &clock,
        );
        assert!(matches!(
            completion.as_slice(),
            [Effect::CommitBoardOperation(_)]
        ));
        assert!(app.state.board.live_thoughts().is_empty());
    }

    fn assert_loading_input_is_ignored(
        app: &mut BoardApp,
        ids: &mut FakeIdGenerator,
        clock: &FakeClock,
    ) {
        let effects = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), ids, clock);
        assert!(effects.is_empty());
        assert!(app.transfer_view().is_some());
    }

    fn assert_thought_is_live(app: &BoardApp, thought_id: crate::domain::ThoughtId) {
        assert!(
            app.state
                .board
                .thought(thought_id)
                .is_some_and(Thought::is_live)
        );
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
}
