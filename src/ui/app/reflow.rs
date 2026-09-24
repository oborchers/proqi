//! Whole-thought spacing cleanup through the annotation-safe paste policy.

use crate::{
    application::{
        Action, Effect, InteractionMode, OwnedThoughtEdit, OwnedThoughtReflow,
        OwnedThoughtReflowBatch,
    },
    domain::{ContentAnnotation, TextPosition},
    ports::{
        editor::{EditorSnapshot, OffsetAffinity, TextChangeSet},
        environment::{Clock, IdGenerator},
        text_layout::{byte_for_position, position_for_byte},
    },
    ui::{
        PastePayload,
        annotations::{PasteReflow, ReflowProjection},
    },
};

use super::{BoardApp, pending_types::EditFlush};

impl BoardApp {
    pub(super) fn reflow_thought_in_place(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if matches!(self.state.mode, InteractionMode::Board) && self.selection_len() > 1 {
            return self.reflow_selected_thoughts(ids, clock);
        }
        let mut effects = match self.flush_edit_boundary(ids, clock) {
            EditFlush::Complete(effects) => effects,
            EditFlush::Blocked(effects) => return effects,
        };
        let thought_id = self.reflow_target_thought_id();
        let Some(source) = thought_id
            .and_then(|id| self.state.board.thought(id))
            .cloned()
        else {
            self.set_warning("focus a thought before cleaning up its spacing");
            return effects;
        };
        if !self.reflow_mutable(source.id) {
            return effects;
        }
        let payload =
            PastePayload::preserved_clipboard(source.content.clone(), source.annotations.clone());
        let transformed = payload
            .map_err(|_| ())
            .and_then(|payload| payload.reflow_with_changes());
        let (payload, changes, origins) = match transformed {
            Ok(ReflowProjection {
                outcome: PasteReflow::Changed(payload),
                changes,
                annotation_origins,
            }) => (payload, changes, annotation_origins),
            Ok(ReflowProjection {
                outcome: PasteReflow::Unchanged,
                ..
            }) => {
                self.set_info("spacing already clean");
                return effects;
            }
            Ok(ReflowProjection {
                outcome: PasteReflow::Empty,
                ..
            }) => {
                self.set_warning("cleanup removed all content; thought kept unchanged");
                return effects;
            }
            Err(()) => {
                self.set_warning("could not clean up spacing; thought kept unchanged");
                return effects;
            }
        };
        let expanded = origins
            .iter()
            .filter_map(|(old, new)| {
                self.expanded_folds
                    .contains(&(source.id, *old))
                    .then_some(*new)
            })
            .collect::<Vec<_>>();
        let mutations = if let Some(before) = self.editor_snapshot() {
            self.reflow_editor(before, payload, &changes, &expanded, ids, clock)
        } else {
            self.reduce(Action::ReflowThought(OwnedThoughtReflow {
                thought_id: source.id,
                operation_id: ids.operation_id(),
                before_content: source.content,
                before_annotations: source.annotations,
                after_content: payload.content,
                after_annotations: payload.annotations,
                at: clock.now(),
            }))
        };
        if !mutations.is_empty() {
            self.clear_expanded_folds(source.id);
            self.expanded_folds
                .extend(expanded.into_iter().map(|index| (source.id, index)));
            self.note_reflow_success();
        }
        effects.extend(mutations);
        effects
    }

    fn reflow_selected_thoughts(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let mut effects = match self.flush_edit_boundary(ids, clock) {
            EditFlush::Complete(effects) => effects,
            EditFlush::Blocked(effects) => return effects,
        };
        let Some(selected) = self.reflow_eligible_selection() else {
            return effects;
        };
        let mut changes = Vec::new();
        let mut folds = Vec::new();
        let operation_id = ids.operation_id();
        let at = clock.now();
        for thought_id in selected {
            let Some(source) = self.state.board.thought(thought_id).cloned() else {
                self.set_warning("could not clean up selection; thoughts kept unchanged");
                return effects;
            };
            let payload = PastePayload::preserved_clipboard(
                source.content.clone(),
                source.annotations.clone(),
            );
            let Ok(projection) = payload
                .map_err(|_| ())
                .and_then(|payload| payload.reflow_with_changes())
            else {
                self.set_warning("could not clean up selection; thoughts kept unchanged");
                return effects;
            };
            let payload = match projection.outcome {
                PasteReflow::Changed(payload) => payload,
                PasteReflow::Empty => {
                    self.set_warning("cleanup removed all content; selection kept unchanged");
                    return effects;
                }
                PasteReflow::Unchanged => continue,
            };
            let expanded = projection
                .annotation_origins
                .iter()
                .filter_map(|(old, new)| {
                    self.expanded_folds
                        .contains(&(thought_id, *old))
                        .then_some((thought_id, *new))
                });
            folds.push((thought_id, expanded.collect::<Vec<_>>()));
            changes.push(OwnedThoughtReflow {
                thought_id,
                operation_id,
                before_content: source.content,
                before_annotations: source.annotations,
                after_content: payload.content,
                after_annotations: payload.annotations,
                at,
            });
        }
        if changes.is_empty() {
            self.set_info("spacing already clean");
            return effects;
        }
        let mutations = self.reduce(Action::ReflowThoughts(OwnedThoughtReflowBatch {
            operation_id,
            changes,
            at,
        }));
        if mutations
            .iter()
            .any(|effect| matches!(effect, Effect::CommitBoardOperation(_)))
        {
            for (thought_id, expanded) in folds {
                self.clear_expanded_folds(thought_id);
                self.expanded_folds.extend(expanded);
            }
            self.note_reflow_success();
        }
        effects.extend(mutations);
        effects
    }

    fn note_reflow_success(&mut self) {
        self.edit_generation = self.edit_generation.wrapping_add(1);
        self.board_viewport = self.board_viewport.follow_focus();
        self.scroll_geometry = None;
        self.layout = None;
        self.set_info("spacing cleaned up");
    }

    fn reflow_eligible_selection(&mut self) -> Option<Vec<crate::domain::ThoughtId>> {
        let selected = self.action_thought_ids();
        if selected.is_empty() {
            self.set_warning("select a thought before cleaning up spacing");
            return None;
        }
        if selected.iter().any(|id| !self.thought_mutable(*id)) {
            self.set_warning("selected thought has an operation in progress");
            return None;
        }
        Some(selected)
    }

    fn reflow_mutable(&mut self, thought_id: crate::domain::ThoughtId) -> bool {
        if self.thought_mutable(thought_id) {
            true
        } else {
            self.set_warning("thought has an operation in progress");
            false
        }
    }

    fn reflow_target_thought_id(&self) -> Option<crate::domain::ThoughtId> {
        match self.state.mode {
            InteractionMode::Edit { thought_id } => Some(thought_id),
            InteractionMode::Board if !self.insertion_focused() => self.state.focused_thought_id(),
            InteractionMode::Board | InteractionMode::Compose => None,
        }
    }

    fn reflow_editor(
        &mut self,
        before: EditorSnapshot,
        payload: PastePayload,
        changes: &TextChangeSet,
        expanded: &[usize],
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(thought_id) = self.active_thought_id() else {
            return Vec::new();
        };
        let Ok((cursor, anchor)) = project_editor(&before, &payload, changes, expanded) else {
            self.set_warning("could not clean up spacing; thought kept unchanged");
            return Vec::new();
        };
        let effects = self.reduce(Action::EditOwnedThought(OwnedThoughtEdit::rebased(
            thought_id,
            ids.revision_id(),
            before.content,
            payload.content.clone(),
            self.current_annotations(thought_id),
            payload.annotations,
            before.cursor,
            before.selection_anchor,
            cursor,
            anchor,
            clock.now(),
        )));
        if !effects.is_empty()
            && let Some((_, editor)) = &mut self.editor
        {
            editor.replace_state(payload.content, cursor, anchor);
        }
        effects
    }
}

fn project_editor(
    before: &EditorSnapshot,
    after: &PastePayload,
    changes: &TextChangeSet,
    expanded: &[usize],
) -> Result<(TextPosition, Option<TextPosition>), ()> {
    let changes =
        crate::ui::paste_reflow::position_changes(&before.content, &after.content, changes)
            .map_err(|_| ())?;
    let backwards = before
        .selection
        .is_some_and(|range| before.cursor == range.start);
    let affinity = if backwards {
        OffsetAffinity::Before
    } else {
        OffsetAffinity::After
    };
    let project = |position, affinity| {
        let byte = byte_for_position(&before.content, position);
        // A retained span ends before a following replacement starts. Keep
        // its boundary exact instead of absorbing normalized whitespace.
        let affinity = if changes
            .as_slice()
            .iter()
            .any(|change| !change.is_insertion() && change.old_range().start == byte)
        {
            OffsetAffinity::Before
        } else {
            affinity
        };
        changes
            .map_old_offset(&before.content, byte, affinity)
            .map(|byte| {
                position_for_byte(
                    &after.content,
                    fold_boundary(byte, &after.annotations, expanded, affinity),
                )
            })
            .map_err(|_| ())
    };
    let cursor = project(before.cursor, affinity)?;
    let anchor = before
        .selection
        .map(|range| {
            if backwards {
                project(range.end, OffsetAffinity::After)
            } else {
                project(range.start, OffsetAffinity::Before)
            }
        })
        .transpose()?;
    Ok((cursor, anchor))
}

fn fold_boundary(
    byte: usize,
    annotations: &[ContentAnnotation],
    expanded: &[usize],
    affinity: OffsetAffinity,
) -> usize {
    annotations
        .iter()
        .enumerate()
        .find(|(index, annotation)| {
            !expanded.contains(index)
                && annotation.kind.behavior() == crate::domain::AnnotationBehavior::Substitution
                && annotation.start < byte
                && byte < annotation.end
        })
        .map_or(byte, |(_, annotation)| match affinity {
            OffsetAffinity::Before => annotation.start,
            OffsetAffinity::After => annotation.end,
        })
}
