//! Active-owner mapping for exact text transformations.

use crate::{
    application::{Action, ApplicationError, exact_live_thought},
    domain::Timestamp,
    ports::control::ControlMutation,
};

use super::super::BoardApp;

impl BoardApp {
    pub(super) fn transformation_control_action(
        &self,
        mutation: &ControlMutation,
        at: Timestamp,
    ) -> Result<Option<Action>, ApplicationError> {
        match mutation {
            ControlMutation::SplitThought { .. } => self.split_control_action(mutation, at),
            ControlMutation::ExtractThought { .. } => self.extract_control_action(mutation, at),
            ControlMutation::MergeThoughts { .. } => self.merge_control_action(mutation, at),
            ControlMutation::ReflowThought { .. } => self.reflow_control_action(mutation, at),
            _ => Err(ApplicationError::InvalidState),
        }
    }

    fn split_control_action(
        &self,
        mutation: &ControlMutation,
        at: Timestamp,
    ) -> Result<Option<Action>, ApplicationError> {
        let ControlMutation::SplitThought {
            operation_id,
            thought_id,
            new_thought_id,
            expected_digest,
            at_byte,
        } = mutation
        else {
            return Err(ApplicationError::InvalidState);
        };
        require_derived_thought(*operation_id, *new_thought_id)?;
        let source = exact_live_thought(&self.state, *thought_id, Some(*expected_digest))?;
        Ok(Some(Action::SplitThought {
            thought_id: *thought_id,
            new_thought_id: *new_thought_id,
            operation_id: *operation_id,
            expected_content: source.content.clone(),
            expected_annotations: source.annotations.clone(),
            source_content: source.content.clone(),
            source_annotations: source.annotations.clone(),
            at_byte: *at_byte,
            at,
        }))
    }

    fn extract_control_action(
        &self,
        mutation: &ControlMutation,
        at: Timestamp,
    ) -> Result<Option<Action>, ApplicationError> {
        let ControlMutation::ExtractThought {
            operation_id,
            thought_id,
            new_thought_id,
            expected_digest,
            start_byte,
            end_byte,
        } = mutation
        else {
            return Err(ApplicationError::InvalidState);
        };
        require_derived_thought(*operation_id, *new_thought_id)?;
        let source = exact_live_thought(&self.state, *thought_id, Some(*expected_digest))?;
        Ok(Some(Action::ExtractThought {
            thought_id: *thought_id,
            new_thought_id: *new_thought_id,
            operation_id: *operation_id,
            expected_content: source.content.clone(),
            expected_annotations: source.annotations.clone(),
            source_content: source.content.clone(),
            source_annotations: source.annotations.clone(),
            range: *start_byte..*end_byte,
            at,
        }))
    }

    fn merge_control_action(
        &self,
        mutation: &ControlMutation,
        at: Timestamp,
    ) -> Result<Option<Action>, ApplicationError> {
        let ControlMutation::MergeThoughts {
            operation_id,
            thought_ids,
            expected_digests,
            separator,
        } = mutation
        else {
            return Err(ApplicationError::InvalidState);
        };
        if thought_ids.len() != expected_digests.len() {
            return Err(ApplicationError::InvalidState);
        }
        let expected_sources = thought_ids
            .iter()
            .zip(expected_digests)
            .map(|(id, digest)| exact_live_thought(&self.state, *id, Some(*digest)).cloned())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(Action::MergeThoughts {
            operation_id: *operation_id,
            thought_ids: thought_ids.clone(),
            expected_sources,
            separator: separator.clone(),
            at,
        }))
    }

    fn reflow_control_action(
        &self,
        mutation: &ControlMutation,
        at: Timestamp,
    ) -> Result<Option<Action>, ApplicationError> {
        let ControlMutation::ReflowThought {
            operation_id,
            thought_id,
            expected_digest,
        } = mutation
        else {
            return Err(ApplicationError::InvalidState);
        };
        let source = exact_live_thought(&self.state, *thought_id, Some(*expected_digest))?;
        let projection =
            crate::application::text_reflow::reflow(&source.content, &source.annotations)
                .map_err(|()| ApplicationError::InvalidState)?;
        let (content, annotations) = match projection.outcome {
            crate::application::text_reflow::TextReflowOutcome::Changed {
                content,
                annotations,
            } => (content, annotations),
            crate::application::text_reflow::TextReflowOutcome::Unchanged
            | crate::application::text_reflow::TextReflowOutcome::Empty => return Ok(None),
        };
        Ok(Some(Action::ReflowThought(
            crate::application::OwnedThoughtReflow {
                thought_id: *thought_id,
                operation_id: *operation_id,
                before_content: source.content.clone(),
                before_annotations: source.annotations.clone(),
                after_content: content,
                after_annotations: annotations,
                at,
            },
        )))
    }
}

fn require_derived_thought(
    operation_id: crate::domain::OperationId,
    thought_id: crate::domain::ThoughtId,
) -> Result<(), ApplicationError> {
    let expected = crate::domain::ThoughtId::from_database_bytes(operation_id.database_bytes())
        .map_err(|_| ApplicationError::InvalidState)?;
    if expected != thought_id {
        return Err(ApplicationError::InvalidState);
    }
    Ok(())
}
