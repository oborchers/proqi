//! One semantic projection for concise, expanded, and searched Commands rows.

use super::{CommandContext, ranking};
#[cfg(test)]
use crate::ui::shortcut_registry::CommandCategory;
use crate::ui::{
    CommandDiscoverability, CommandMetadata, ShortcutActionId, shortcut_registry::CommandExecution,
};

const CONCISE_COMMAND_LIMIT: usize = 7;

pub(super) struct CommandRecord {
    pub(super) action: ShortcutActionId,
    pub(super) metadata: CommandMetadata,
    pub(super) execution: CommandExecution,
    pub(super) shortcut: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RowAction {
    Command {
        id: ShortcutActionId,
        execution: CommandExecution,
    },
    Expand,
    None,
}

pub(super) struct ProjectedRow {
    pub(super) primary: String,
    pub(super) secondary: Option<String>,
    pub(super) secondary_fallbacks: Vec<String>,
    pub(super) protected_secondaries: Vec<String>,
    pub(super) group: Option<&'static str>,
    pub(super) enabled: bool,
    pub(super) action: RowAction,
}

impl ProjectedRow {
    pub(super) fn selectable(&self) -> bool {
        self.enabled && !matches!(self.action, RowAction::None)
    }
}

pub(super) fn rows(
    commands: &[CommandRecord],
    context: &CommandContext,
    query: &str,
    expanded: bool,
) -> Vec<ProjectedRow> {
    if !query.is_empty() {
        return searched(commands, context, query);
    }
    if expanded {
        return expanded_rows(commands, context);
    }
    concise(commands, context)
}

fn concise(commands: &[CommandRecord], context: &CommandContext) -> Vec<ProjectedRow> {
    let mut relevant = commands
        .iter()
        .filter_map(|command| {
            discoverable(command).then(|| {
                context
                    .relevance(command.metadata)
                    .map(|priority| (priority, command.metadata.order, command.action, command))
            })?
        })
        .collect::<Vec<_>>();
    relevant.sort_unstable_by_key(|(priority, order, action, _)| (*priority, *order, *action));
    let mut rows = relevant
        .into_iter()
        .take(CONCISE_COMMAND_LIMIT)
        .enumerate()
        .map(|(index, (_, _, _, command))| {
            command_row(command, context, (index == 0).then_some("Relevant now"))
        })
        .collect::<Vec<_>>();
    rows.push(ProjectedRow {
        primary: "More commands...".to_owned(),
        secondary: Some(format!(
            "all {}",
            commands
                .iter()
                .filter(|command| discoverable(command))
                .count()
        )),
        secondary_fallbacks: Vec::new(),
        protected_secondaries: Vec::new(),
        group: rows.is_empty().then_some("Relevant now"),
        enabled: true,
        action: RowAction::Expand,
    });
    rows
}

fn expanded_rows(commands: &[CommandRecord], context: &CommandContext) -> Vec<ProjectedRow> {
    let mut commands = commands
        .iter()
        .filter(|command| discoverable(command))
        .collect::<Vec<_>>();
    commands.sort_unstable_by_key(|command| {
        (
            command.metadata.category,
            command.metadata.order,
            command.action,
        )
    });
    let mut previous = None;
    commands
        .into_iter()
        .map(|command| {
            let category = command.metadata.category;
            let group = (previous != Some(category)).then(|| category.label());
            previous = Some(category);
            command_row(command, context, group)
        })
        .collect()
}

fn searched(
    commands: &[CommandRecord],
    context: &CommandContext,
    query: &str,
) -> Vec<ProjectedRow> {
    let mut ranked = commands
        .iter()
        .filter(|command| discoverable(command))
        .filter_map(|command| {
            ranking::rank(context.command_label(command.metadata.label), query)
                .map(|rank| (rank, command.action, command.metadata.order, command))
        })
        .collect::<Vec<_>>();
    ranked.sort_unstable_by_key(|(rank, action, order, _)| (*rank, *action, *order));
    let rows = ranked
        .into_iter()
        .map(|(_, _, _, command)| command_row(command, context, None))
        .collect::<Vec<_>>();
    if rows.is_empty() {
        vec![ProjectedRow {
            primary: "No matching commands".to_owned(),
            secondary: None,
            secondary_fallbacks: Vec::new(),
            protected_secondaries: Vec::new(),
            group: None,
            enabled: false,
            action: RowAction::None,
        }]
    } else {
        rows
    }
}

fn command_row(
    command: &CommandRecord,
    context: &CommandContext,
    group: Option<&'static str>,
) -> ProjectedRow {
    let applicability = context.applicability(command.metadata);
    let scope = context
        .command_scope_label(command.metadata.scope)
        .to_owned();
    let (secondary, secondary_fallbacks, protected_secondaries) = if applicability.enabled {
        let secondary = command
            .shortcut
            .as_ref()
            .map_or_else(|| scope.clone(), |shortcut| format!("{shortcut} · {scope}"));
        let fallbacks = command
            .shortcut
            .iter()
            .cloned()
            .chain(std::iter::once(scope))
            .collect();
        (Some(secondary), fallbacks, Vec::new())
    } else {
        let reason = applicability.reason.map(str::to_owned);
        (reason.clone(), Vec::new(), reason.into_iter().collect())
    };
    ProjectedRow {
        primary: context.command_label(command.metadata.label).to_owned(),
        secondary,
        secondary_fallbacks,
        protected_secondaries,
        group,
        enabled: applicability.enabled,
        action: RowAction::Command {
            id: command.action,
            execution: command.execution,
        },
    }
}

const fn discoverable(command: &CommandRecord) -> bool {
    matches!(
        command.metadata.discoverability,
        CommandDiscoverability::Discoverable
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_vocabulary_is_closed_and_stable() {
        assert_eq!(
            [
                CommandCategory::Thought,
                CommandCategory::Edit,
                CommandCategory::Selection,
                CommandCategory::Delivery,
                CommandCategory::Session,
                CommandCategory::AttachmentsAndCapture,
                CommandCategory::ApplicationAndRecovery,
            ]
            .map(CommandCategory::label),
            [
                "Thought",
                "Edit",
                "Selection",
                "Delivery",
                "Session",
                "Attachments and capture",
                "Application and recovery",
            ]
        );
    }
}
