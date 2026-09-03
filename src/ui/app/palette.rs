//! Searchable command discovery and execution.

pub(super) mod command;
mod dispatch;
mod editor;

use crate::{
    application::Effect,
    ports::environment::{Clock, IdGenerator},
};

use super::{
    BoardApp, UiInput, UiKey, palette_handoff::EditorSelectionHandoff, query::QueryEditor,
    screenshot::ScreenshotPaletteAction,
};

use command::Command;

pub(super) struct PaletteState {
    query: QueryEditor,
    selected: usize,
    scroll: usize,
    submit_supported: bool,
    plain_newline_supported: bool,
    screenshot_action: ScreenshotPaletteAction,
    screenshot_retry: bool,
    selection_handoff: Option<EditorSelectionHandoff>,
    merge_handoff: Option<Vec<crate::domain::Thought>>,
}

impl PaletteState {
    fn new(
        submit_supported: bool,
        plain_newline_supported: bool,
        screenshot_action: ScreenshotPaletteAction,
        screenshot_retry: bool,
        selection_handoff: Option<EditorSelectionHandoff>,
        merge_handoff: Option<Vec<crate::domain::Thought>>,
    ) -> Self {
        Self {
            query: QueryEditor::default(),
            selected: 0,
            scroll: 0,
            submit_supported,
            plain_newline_supported,
            screenshot_action,
            screenshot_retry,
            selection_handoff,
            merge_handoff,
        }
    }

    pub(super) const fn query_cursor(&self) -> usize {
        self.query.cursor()
    }

    pub(super) fn view(&self) -> (String, Vec<String>, usize) {
        (
            self.query.text().to_owned(),
            self.matches()
                .into_iter()
                .skip(self.scroll)
                .map(|(_, label)| label.to_owned())
                .collect(),
            self.selected.saturating_sub(self.scroll),
        )
    }

    pub(super) fn match_count(&self) -> usize {
        self.matches().len()
    }

    pub(super) fn overflow(&self, visible: usize) -> (bool, bool) {
        (
            self.scroll > 0,
            self.scroll.saturating_add(visible) < self.match_count(),
        )
    }

    fn matches(&self) -> Vec<(Command, &'static str)> {
        let query = self.query.text().to_lowercase();
        Command::ALL
            .into_iter()
            .filter(|(command, _)| self.available(*command))
            .map(|(command, label)| {
                let label = if command == Command::ScreenshotInbox {
                    match self.screenshot_action {
                        ScreenshotPaletteAction::Enable => label,
                        ScreenshotPaletteAction::Disable => "Disable Screenshot Inbox",
                        ScreenshotPaletteAction::Resume => "Resume Screenshot Inbox",
                        ScreenshotPaletteAction::Unavailable => "Screenshot Inbox unavailable",
                    }
                } else {
                    label
                };
                (command, label)
            })
            .filter(|(_, label)| label.to_lowercase().contains(&query))
            .collect()
    }

    fn available(&self, command: Command) -> bool {
        match command {
            Command::SubmitRemove
            | Command::SubmitKeep
            | Command::SubmitAllRemove
            | Command::SubmitAllKeep => self.submit_supported,
            Command::PlainNewline
            | Command::DeleteLogicalLine
            | Command::DeleteSentence
            | Command::JumpUp
            | Command::JumpDown
            | Command::SelectVisualRowStart
            | Command::SelectVisualRowEnd
            | Command::ThoughtStart
            | Command::ThoughtEnd
            | Command::Indent
            | Command::Outdent => self.plain_newline_supported,
            Command::RetryScreenshotCapture => self.screenshot_retry,
            Command::SplitThought => self.selection_handoff.is_some(),
            Command::ExtractSelection => self
                .selection_handoff
                .as_ref()
                .is_some_and(EditorSelectionHandoff::has_selection),
            Command::MergeThoughts => self.merge_handoff.is_some(),
            Command::ScreenshotInbox => {
                self.screenshot_action != ScreenshotPaletteAction::Unavailable
            }
            _ => true,
        }
    }

    fn clamp(&mut self) {
        self.selected = self.selected.min(self.match_count().saturating_sub(1));
        self.scroll = self.scroll.min(self.selected);
    }
}

impl BoardApp {
    pub(super) fn refresh_screenshot_palette_action(&mut self) {
        let action = self.screenshot_palette_action();
        if let Some(palette) = &mut self.palette {
            palette.screenshot_action = action;
            palette.clamp();
        }
    }

    pub(super) fn open_palette(&mut self) {
        self.deactivate_range_latch();
        self.help = false;
        self.search = None;
        let merge_handoff = (self.selection_len() >= 2).then(|| {
            self.action_thought_ids()
                .into_iter()
                .filter_map(|id| self.state.board.thought(id).cloned())
                .collect()
        });
        self.palette = Some(PaletteState::new(
            self.supports_submission(),
            !self.insertion_focused() && self.state.focused_thought.is_some(),
            self.screenshot_palette_action(),
            self.screenshot_retry_ready(),
            self.palette_selection_handoff.take(),
            merge_handoff,
        ));
    }

    pub(super) fn close_overlay(&mut self) {
        self.cancel_screenshot_takeover();
        self.palette = None;
        self.search = None;
        self.transfer = None;
        self.close_invocation_picker();
        self.help = false;
    }

    pub(super) fn execute_palette_index(
        &mut self,
        index: usize,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let command = self
            .palette
            .as_ref()
            .and_then(|palette| palette.matches().get(index).copied())
            .map(|(command, _)| command);
        let selection_handoff = self
            .palette
            .as_mut()
            .and_then(|palette| palette.selection_handoff.take());
        let merge_handoff = self
            .palette
            .as_mut()
            .and_then(|palette| palette.merge_handoff.take());
        self.palette = None;
        command.map_or_else(Vec::new, |command| {
            self.execute_command(
                command,
                selection_handoff,
                merge_handoff.as_deref(),
                ids,
                clock,
            )
        })
    }

    pub(super) fn execute_palette_visible_index(
        &mut self,
        index: usize,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let absolute = self
            .palette
            .as_ref()
            .map_or(index, |palette| palette.scroll.saturating_add(index));
        self.execute_palette_index(absolute, ids, clock)
    }

    pub(super) fn handle_palette_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let UiInput::Key(key) = input else {
            return match input {
                UiInput::Pointer(pointer) => match pointer.kind {
                    crate::ui::PointerKind::ScrollUp => {
                        self.move_palette(-1);
                        Vec::new()
                    }
                    crate::ui::PointerKind::ScrollDown => {
                        self.move_palette(1);
                        Vec::new()
                    }
                    _ => self.handle_pointer(*pointer, ids, clock),
                },
                UiInput::Paste(value) => self.update_palette_query(|query| query.paste(value)),
                UiInput::PasteAnnotated(payload) => {
                    self.update_palette_query(|query| query.paste(&payload.content))
                }
                UiInput::Resize { .. }
                | UiInput::HostFocusGained
                | UiInput::HostFocusLost
                | UiInput::Key(_) => Vec::new(),
            };
        };
        match *key {
            UiKey::Escape => self.close_overlay(),
            UiKey::Enter => {
                let selected = self.palette.as_ref().map_or(0, |palette| palette.selected);
                return self.execute_palette_index(selected, ids, clock);
            }
            UiKey::Backspace => {
                if let Some(palette) = &mut self.palette {
                    palette.query.backspace();
                    palette.clamp();
                }
            }
            UiKey::FastNavigation { direction, .. } => self.move_palette(direction.delta()),
            UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualUp,
                ..
            } => self.move_palette(-1),
            UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualDown,
                ..
            } => self.move_palette(1),
            UiKey::Move { movement, .. } => {
                if let Some(palette) = &mut self.palette {
                    palette.query.move_cursor(movement);
                }
            }
            UiKey::Delete | UiKey::ModifiedDelete => {
                if let Some(palette) = &mut self.palette {
                    palette.query.delete();
                    palette.clamp();
                }
            }
            UiKey::Character(character) if !character.is_control() => {
                return self.update_palette_query(|query| query.insert_char(character));
            }
            UiKey::UnmodifiedSpace => {
                return self.update_palette_query(|query| query.insert_char(' '));
            }
            _ => {}
        }
        Vec::new()
    }

    fn update_palette_query(&mut self, update: impl FnOnce(&mut QueryEditor)) -> Vec<Effect> {
        if let Some(palette) = &mut self.palette {
            update(&mut palette.query);
            palette.selected = 0;
            palette.scroll = 0;
            palette.clamp();
        }
        Vec::new()
    }

    fn move_palette(&mut self, delta: isize) {
        let visible = self
            .layout
            .as_ref()
            .and_then(|layout| layout.overlay.as_ref())
            .map_or(1, |overlay| overlay.items.len().max(1));
        let Some(palette) = &mut self.palette else {
            return;
        };
        palette.selected = palette
            .selected
            .saturating_add_signed(delta)
            .min(palette.match_count().saturating_sub(1));
        palette.scroll =
            crate::ui::paging::first_visible(palette.selected, palette.scroll, visible);
        self.layout = None;
    }

    pub(super) fn ensure_palette_visible(&mut self, visible: usize) {
        let Some(palette) = &mut self.palette else {
            return;
        };
        palette.selected = palette
            .selected
            .min(palette.match_count().saturating_sub(1));
        palette.scroll =
            crate::ui::paging::first_visible(palette.selected, palette.scroll, visible);
    }

    fn execute_command(
        &mut self,
        command: Command,
        selection_handoff: Option<EditorSelectionHandoff>,
        merge_handoff: Option<&[crate::domain::Thought]>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if let Some(effects) = self.execute_transformation_command(
            command,
            selection_handoff.as_ref(),
            merge_handoff,
            ids,
            clock,
        ) {
            return effects;
        }
        if let Some(effects) = self.execute_submission_command(command, ids, clock) {
            return effects;
        }
        if let Some(effects) = self.execute_editor_command(command, selection_handoff, ids, clock) {
            return effects;
        }
        if let Some(effects) = self.execute_entry_command(command, ids, clock) {
            return effects;
        }
        if let Some(effects) = self.execute_selection_command(command, ids, clock) {
            return effects;
        }
        if let Some(effects) = self.execute_runtime_command(command, ids, clock) {
            return effects;
        }
        self.execute_board_command(command, ids, clock)
    }

    fn execute_board_command(
        &mut self,
        command: Command,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match command {
            Command::New => self.create(crate::ui::PastePayload::text(String::new()), ids, clock),
            Command::RenameSession => {
                self.begin_session_rename();
                Vec::new()
            }
            Command::CopySessionId => self.copy_session_id(ids),
            Command::CopyResume => self.copy_resume_command(ids),
            Command::SendSession => self.begin_session_transfer(false, ids, clock),
            Command::SendSessionRemove => self.begin_session_transfer(true, ids, clock),
            Command::Delete => self.delete(ids, clock),
            Command::Copy => self.copy_active(ids),
            Command::Cut => self.cut_active(ids, clock),
            Command::Paste => self.read_clipboard(ids),
            Command::Duplicate => self.duplicate(ids, clock),
            Command::SubmitRemove
            | Command::SubmitKeep
            | Command::SubmitAllRemove
            | Command::SubmitAllKeep
            | Command::PlainNewline
            | Command::DeleteLogicalLine
            | Command::DeleteSentence
            | Command::JumpUp
            | Command::JumpDown
            | Command::SelectVisualRowStart
            | Command::SelectVisualRowEnd
            | Command::ThoughtStart
            | Command::ThoughtEnd
            | Command::Indent
            | Command::Outdent
            | Command::SplitThought
            | Command::ExtractSelection
            | Command::MergeThoughts
            | Command::Edit
            | Command::InsertInvocation
            | Command::RefreshAgents
            | Command::RefreshAttachments
            | Command::RefreshInvocations
            | Command::CheckUpdates
            | Command::WhatsNew
            | Command::ScreenshotInbox
            | Command::RetryScreenshotCapture
            | Command::RetryStorage
            | Command::ExportRecovery
            | Command::SelectAll
            | Command::Select
            | Command::RangeSelect => Vec::new(),
            Command::Undo => self.history(ids, clock, true),
            Command::Redo => self.history(ids, clock, false),
            Command::MoveUp => self.reorder(ids, clock, -1),
            Command::MoveDown => self.reorder(ids, clock, 1),
            Command::Collapse => self.collapse(ids, clock),
            Command::Help => {
                self.help = true;
                Vec::new()
            }
            Command::Quit => self.request_quit_after_edit_flush(ids, clock),
        }
    }
}
