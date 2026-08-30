//! Application state, normalized actions, effects, and errors.

mod effect;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::error::{ApplicationError, ApplicationResult, FailureCode};
use crate::domain::{
    BoardOperation, OperationId, OperationSequence, RequestId, SessionBoard, StableVersion,
    TextPosition, Thought, ThoughtId, ThoughtRevision, Timestamp,
};

use crate::ports::runtime::CaptureOwnerInfo;

pub use effect::Effect;

/// Active interaction context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractionMode {
    /// Navigate and operate on whole thoughts.
    Board,
    /// Edit the focused thought.
    Edit {
        /// Thought being edited.
        thought_id: ThoughtId,
    },
}

/// Durability state shown truthfully by the interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DurabilityState {
    /// No operation is awaiting storage acknowledgement.
    Durable {
        /// Latest acknowledged sequence.
        sequence: OperationSequence,
    },
    /// One or more operations await acknowledgement.
    Pending {
        /// Latest acknowledged sequence.
        durable: OperationSequence,
        /// Highest pending sequence.
        latest: OperationSequence,
    },
    /// A write failed while the in-memory state remains available.
    Failed {
        /// Latest acknowledged sequence.
        durable: OperationSequence,
        /// Sequence that failed.
        failed: OperationSequence,
        /// Stable error code.
        code: FailureCode,
    },
}

/// Clipboard purpose, kept separate from terminal keys.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardIntent {
    /// Preserve the thought after writing exact content.
    Copy,
    /// Delete only after a successful write.
    Cut,
    /// Copy the complete canonical identity of the current session.
    CopySessionId,
    /// Copy the exact command that resumes the current session.
    CopyResumeCommand,
}

impl ClipboardIntent {
    /// Whether a successful OSC 52 emission is sufficient for this non-destructive intent.
    #[must_use]
    pub const fn supports_osc52(self) -> bool {
        !matches!(self, Self::Cut)
    }
}

/// Explicit user decision from an elected installation-wide update prompt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateIntent {
    /// Perform one explicit background lookup from the command palette.
    CheckNow,
    /// Coordinate one verified Homebrew upgrade and restart all compatible sessions.
    Install(StableVersion),
    /// Defer this exact version until the next successful startup refresh.
    Dismiss(StableVersion),
    /// Suppress this exact version until a newer stable version exists.
    Skip(StableVersion),
    /// Show accurate standalone replacement instructions.
    ViewInstructions(StableVersion),
}

/// Explicit screenshot-inbox runtime decision requested by the UI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScreenshotIntent {
    /// Acquire the installation-wide capture lock and start listening.
    Enable,
    /// Reconcile accepted files and stop listening.
    Disable,
    /// Ask the verified compatible owner to relinquish capture, then retry.
    TakeOver {
        /// Owner metadata verified again by local control transport.
        owner: CaptureOwnerInfo,
        /// Idempotent takeover request identity.
        request_id: RequestId,
    },
}

/// Safety threshold that automatically stopped one screenshot listening lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreenshotPauseReason {
    /// No deliberate Proqi interaction occurred for the configured interval.
    Inactivity {
        /// Configured whole-minute interval.
        minutes: u16,
    },
    /// The configured number of candidates was admitted without interaction.
    CaptureLimit {
        /// Configured unattended capture limit.
        captures: u16,
    },
}

impl ScreenshotPauseReason {
    /// Content-free threshold description shared by user-facing adapters.
    #[must_use]
    pub fn description(self) -> String {
        match self {
            Self::Inactivity { minutes: 1 } => "1 minute without activity".to_owned(),
            Self::Inactivity { minutes } => format!("{minutes} minutes without activity"),
            Self::CaptureLimit { captures: 1 } => "1 unattended capture".to_owned(),
            Self::CaptureLimit { captures } => format!("{captures} unattended captures"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PendingClipboard {
    pub(super) thought_ids: Vec<ThoughtId>,
    pub(super) intent: ClipboardIntent,
    pub(super) operation_id: Option<OperationId>,
    pub(super) at: Timestamp,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct EditorHistory {
    pub(super) revisions: Vec<ThoughtRevision>,
    pub(super) cursor: usize,
}

/// Complete mutable state owned by the reducer lane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppState {
    /// Validated current board.
    pub board: SessionBoard,
    /// Current board or editor context.
    pub mode: InteractionMode,
    /// Focused live thought, if any.
    pub focused_thought: Option<ThoughtId>,
    /// Current insertion position used for new thoughts.
    pub insertion_index: usize,
    /// Current durability status.
    pub durability: DurabilityState,
    pub(super) board_history: Vec<BoardOperation>,
    pub(super) board_history_cursor: usize,
    pub(super) editor_histories: HashMap<ThoughtId, EditorHistory>,
    pub(super) pending_clipboard: BTreeMap<RequestId, PendingClipboard>,
    pub(super) locked_thoughts: BTreeSet<ThoughtId>,
    pub(super) pending_sequences: BTreeSet<OperationSequence>,
    pub(super) highest_sequence: OperationSequence,
}

impl AppState {
    /// Construct application state from a validated session snapshot.
    #[must_use]
    pub fn new(board: SessionBoard) -> Self {
        let focused_thought = board.live_thoughts().first().map(|thought| thought.id);
        let insertion_index = board.live_thoughts().len();
        let sequence = board.session.last_durable_sequence;
        Self {
            board,
            mode: InteractionMode::Board,
            focused_thought,
            insertion_index,
            durability: DurabilityState::Durable { sequence },
            board_history: Vec::new(),
            board_history_cursor: 0,
            editor_histories: HashMap::new(),
            pending_clipboard: BTreeMap::new(),
            locked_thoughts: BTreeSet::new(),
            pending_sequences: BTreeSet::new(),
            highest_sequence: sequence,
        }
    }

    /// Board history entries retained for undo and redo.
    #[must_use]
    pub fn board_history(&self) -> &[BoardOperation] {
        &self.board_history
    }

    /// Number of currently applied board history entries.
    #[must_use]
    pub const fn board_history_cursor(&self) -> usize {
        self.board_history_cursor
    }

    /// Number of currently applied revisions for one thought.
    #[must_use]
    pub fn editor_history_cursor(&self, thought_id: ThoughtId) -> usize {
        self.editor_histories
            .get(&thought_id)
            .map_or(0, |history| history.cursor)
    }

    /// Restore the logical cursor represented by the currently applied revision prefix.
    #[must_use]
    pub fn restored_editor_cursor(&self, thought_id: ThoughtId) -> Option<TextPosition> {
        let history = self.editor_histories.get(&thought_id)?;
        if history.cursor == 0 {
            history
                .revisions
                .first()
                .map(|revision| revision.before_cursor)
        } else {
            history
                .revisions
                .get(history.cursor - 1)
                .map(|revision| revision.after_cursor)
        }
    }

    pub(super) fn next_sequence(&self) -> ApplicationResult<OperationSequence> {
        self.highest_sequence
            .checked_next()
            .ok_or(ApplicationError::SequenceExhausted)
    }

    pub(super) fn track_pending(&mut self, sequence: OperationSequence) {
        self.highest_sequence = sequence;
        self.pending_sequences.insert(sequence);
        self.refresh_durability();
    }

    pub(super) fn refresh_durability(&mut self) {
        let durable = self.board.session.last_durable_sequence;
        if matches!(
            self.durability,
            DurabilityState::Failed { failed, .. } if self.pending_sequences.contains(&failed)
        ) {
            return;
        }
        self.durability = self.pending_sequences.last().map_or(
            DurabilityState::Durable { sequence: durable },
            |latest| DurabilityState::Pending {
                durable,
                latest: *latest,
            },
        );
    }

    pub(super) fn live_thought(&self, id: ThoughtId) -> ApplicationResult<&Thought> {
        self.board
            .thought(id)
            .filter(|thought| thought.is_live())
            .ok_or(ApplicationError::ThoughtNotFound(id))
    }

    /// Whether one source thought is protected by an in-flight submission.
    #[must_use]
    pub fn thought_locked(&self, id: ThoughtId) -> bool {
        self.locked_thoughts.contains(&id)
    }

    /// Pending board clipboard purpose for UI-only success feedback.
    #[must_use]
    pub fn pending_clipboard_intent(&self, request_id: RequestId) -> Option<ClipboardIntent> {
        self.pending_clipboard
            .get(&request_id)
            .map(|pending| pending.intent)
    }

    /// Pending board cuts whose successful clipboard completion can allocate a sequence.
    #[must_use]
    pub(crate) fn pending_board_cut_count(&self) -> usize {
        self.pending_clipboard
            .values()
            .filter(|pending| pending.intent == ClipboardIntent::Cut)
            .count()
    }

    pub(super) fn record_board_operation(
        &mut self,
        operation: &BoardOperation,
    ) -> ApplicationResult<()> {
        let mut board = self.board.clone();
        board.apply_mutation(&operation.forward, operation.created_at)?;
        self.board = board;
        self.board_history.truncate(self.board_history_cursor);
        self.board_history.push(operation.clone());
        self.board_history_cursor += 1;
        self.track_pending(operation.sequence);
        self.keep_focus_valid();
        Ok(())
    }

    pub(super) fn apply_durable_capture(
        &mut self,
        operation: &BoardOperation,
    ) -> ApplicationResult<()> {
        if operation.sequence != self.next_sequence()? {
            return Err(ApplicationError::InvalidState);
        }
        let mut board = self.board.clone();
        board.apply_mutation(&operation.forward, operation.created_at)?;
        board.session.last_durable_sequence = operation.sequence;
        self.board = board;
        self.board_history.truncate(self.board_history_cursor);
        self.board_history.push(operation.clone());
        self.board_history_cursor += 1;
        self.highest_sequence = operation.sequence;
        self.refresh_durability();
        self.keep_focus_valid();
        Ok(())
    }

    pub(super) fn keep_focus_valid(&mut self) {
        self.insertion_index = self.insertion_index.min(self.board.live_thoughts().len());
        if self
            .focused_thought
            .is_some_and(|id| self.live_thought(id).is_ok())
        {
            return;
        }
        self.focused_thought = self.board.live_thoughts().first().map(|thought| thought.id);
        if matches!(self.mode, InteractionMode::Edit { .. }) {
            self.mode = InteractionMode::Board;
        }
    }
}
