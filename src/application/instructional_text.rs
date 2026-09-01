//! Application-owned construction of exact instructional text and shortcut ranges.

use crate::domain::{
    ContentAnnotation, DomainError, OperationId, ThoughtId, Timestamp, validate_annotations,
};

use super::{Action, OwnedThoughtCreation};

/// Exact canonical instructional content with reviewed semantic presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstructionalText {
    content: String,
    annotations: Vec<ContentAnnotation>,
}

impl InstructionalText {
    #[cfg(test)]
    pub(crate) fn into_parts(self) -> (String, Vec<ContentAnnotation>) {
        (self.content, self.annotations)
    }

    /// Seal reviewed instructional content into the private creation path.
    pub(crate) fn create_action(
        self,
        thought_id: ThoughtId,
        operation_id: OperationId,
        insertion_index: Option<usize>,
        at: Timestamp,
    ) -> Action {
        Action::CreateOwnedThought(OwnedThoughtCreation::preserved(
            thought_id,
            operation_id,
            self.content,
            self.annotations,
            insertion_index,
            at,
        ))
    }
}

/// Literal append-and-mark builder reserved for reviewed application policies.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct InstructionalTextBuilder {
    content: String,
    annotations: Vec<ContentAnnotation>,
}

impl InstructionalTextBuilder {
    pub(crate) const fn new() -> Self {
        Self {
            content: String::new(),
            annotations: Vec::new(),
        }
    }

    pub(crate) fn text(mut self, literal: &str) -> Self {
        self.content.push_str(literal);
        self
    }

    /// Append one exact shortcut literal and atomically mark only that range.
    ///
    /// # Errors
    ///
    /// Returns an annotation error for an empty shortcut literal.
    pub(crate) fn shortcut(mut self, literal: &str) -> Result<Self, DomainError> {
        if literal.is_empty() {
            return Err(DomainError::InvalidContentAnnotation);
        }
        let start = self.content.len();
        self.content.push_str(literal);
        self.annotations
            .push(ContentAnnotation::shortcut(start, self.content.len()));
        Ok(self)
    }

    /// Finish one validated canonical text and annotation pair.
    ///
    /// # Errors
    ///
    /// Returns an annotation error if the constructed ranges are invalid.
    pub(crate) fn finish(self) -> Result<InstructionalText, DomainError> {
        validate_annotations(&self.content, &self.annotations)?;
        Ok(InstructionalText {
            content: self.content,
            annotations: self.annotations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AnnotationBehavior, InlineStyleKind};
    use crate::{
        adapters::memory::FakeIdGenerator,
        application::{AppState, ApplicationError, reduce},
        domain::{Session, SessionBoard, TextPosition},
        ports::environment::IdGenerator as _,
    };

    #[test]
    fn literal_builder_owns_exact_repeated_and_unicode_ranges() {
        let built = InstructionalTextBuilder::new()
            .text("Press ")
            .shortcut("↓")
            .expect("down shortcut")
            .text(", then ")
            .shortcut("↓")
            .expect("second down shortcut")
            .text(". e\u{301} stays plain")
            .finish()
            .expect("instruction");
        let mut ids = FakeIdGenerator::new(1_725_100_000_000);
        assert!(matches!(
            built.clone().create_action(
                ids.thought_id(),
                ids.operation_id(),
                None,
                Timestamp::from_millis(1),
            ),
            Action::CreateOwnedThought(_)
        ));
        let (content, annotations) = built.into_parts();
        assert_eq!(content, "Press ↓, then ↓. e\u{301} stays plain");
        assert_eq!(
            annotations
                .iter()
                .map(|annotation| &content[annotation.start..annotation.end])
                .collect::<Vec<_>>(),
            ["↓", "↓"]
        );
        assert!(annotations.iter().all(|annotation| {
            annotation.kind.behavior()
                == AnnotationBehavior::InlineStyle(InlineStyleKind::ShortcutEmphasis)
        }));
    }

    #[test]
    fn empty_shortcuts_fail_without_partial_output() {
        assert_eq!(
            InstructionalTextBuilder::new().shortcut(""),
            Err(DomainError::InvalidContentAnnotation)
        );
    }

    #[test]
    fn only_the_sealed_builder_can_originate_and_rebase_shortcut_metadata() {
        let mut ids = FakeIdGenerator::new(1_725_110_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-instruction-authority"),
            Timestamp::from_millis(1),
        )
        .expect("session");
        let mut state = AppState::new(SessionBoard::new(session, Vec::new()).expect("board"));
        let rejected_id = ids.thought_id();
        assert_eq!(
            reduce(
                &mut state,
                Action::CreateThought {
                    thought_id: rejected_id,
                    operation_id: ids.operation_id(),
                    content: "Press Enter".to_owned(),
                    annotations: vec![ContentAnnotation::shortcut(6, 11)],
                    insertion_index: None,
                    at: Timestamp::from_millis(2),
                },
            ),
            Err(ApplicationError::InvalidState)
        );

        let thought_id = ids.thought_id();
        let built = InstructionalTextBuilder::new()
            .text("Press ")
            .shortcut("Enter")
            .expect("shortcut")
            .finish()
            .expect("instruction");
        reduce(
            &mut state,
            built.create_action(
                thought_id,
                ids.operation_id(),
                None,
                Timestamp::from_millis(3),
            ),
        )
        .expect("sealed creation");
        let thought = state
            .board
            .thought(thought_id)
            .expect("owned thought")
            .clone();
        assert_eq!(thought.annotations.len(), 1);

        assert_eq!(
            reduce(
                &mut state,
                Action::EditThought {
                    thought_id,
                    revision_id: ids.revision_id(),
                    before_content: thought.content.clone(),
                    after_content: thought.content,
                    before_annotations: thought.annotations.clone(),
                    after_annotations: thought.annotations,
                    before_cursor: TextPosition::default(),
                    after_cursor: TextPosition::default(),
                    at: Timestamp::from_millis(4),
                },
            ),
            Err(ApplicationError::InvalidState)
        );
    }
}
