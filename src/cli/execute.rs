//! CLI dispatch into the shared session service.

mod board_items;
mod capabilities;
mod diagnostics;
mod doctor;
mod external_thoughts;
mod forwarding;
mod helpers;
mod queries;
mod runtime_open;
mod sessions;
mod thought_names;
mod transfer;
mod transformations;
mod update;

use std::process::ExitCode;

use clap::CommandFactory;
use clap_complete::generate;
use serde_json::{Value, json};

use crate::{
    adapters::terminal,
    application::{FirstRunEnvironment, SessionService},
    domain::{BoardItemId, ThoughtId, UndoScope},
    ports::store::{CommitReceipt, DurableIdentity},
};

use super::{
    args::{Cli, Command, HistoryArgs, ThoughtCommand},
    output::{CliError, render_error, render_success},
    runtime::RuntimeContext,
};

use helpers::{parse_operation_id, parse_thought_id, read_standard_input};
use runtime_open::ResumeRequest;
use sessions::{
    browse_for_session, cancelled_browser, execute_sessions, list_sessions, opened_session,
};

pub(super) struct Outcome {
    data: Value,
    human: String,
}

pub(super) fn execute(cli: Cli) -> ExitCode {
    if let Some(Command::Completions { shell }) = cli.command.as_ref() {
        let mut command = Cli::command();
        let generator: clap_complete::Shell = (*shell).into();
        generate(generator, &mut command, "proqi", &mut std::io::stdout());
        return ExitCode::SUCCESS;
    }
    let json_output = cli.json;
    match execute_inner(cli) {
        Ok(outcome) => render_success(&outcome.data, &outcome.human, json_output),
        Err(error) => render_error(&error, json_output),
    }
}

fn execute_inner(cli: Cli) -> Result<Outcome, CliError> {
    if cli.command.is_some() && (cli.continue_latest || cli.resume.is_some()) {
        return Err(CliError::arguments(
            "-c and -r cannot be combined with a subcommand".to_owned(),
        ));
    }
    if matches!(cli.command, Some(Command::Capabilities)) {
        return Ok(capabilities::outcome());
    }
    if let Some(Command::Update(arguments)) = &cli.command {
        let paths = super::runtime::resolve_paths(cli.state_dir.as_deref())?;
        return update::execute(arguments, &paths.cache_dir);
    }
    if let Some(outcome) = diagnostics::early_outcome(&cli)? {
        return Ok(outcome);
    }
    let context = runtime_open::open(&cli)?;
    match cli.command {
        Some(Command::Sessions(arguments)) => {
            let mut context = context;
            execute_sessions(&mut context, arguments.command)
        }
        Some(Command::Items(arguments)) => {
            let mut context = context;
            board_items::execute(&mut context, arguments.command)
        }
        Some(Command::Thoughts(arguments)) => {
            let mut context = context;
            execute_thoughts(&mut context, arguments.command)
        }
        Some(Command::Diagnostics(_) | Command::Doctor) => Err(CliError::arguments(
            "diagnostic command was not dispatched".to_owned(),
        )),
        Some(Command::Capabilities) => Ok(capabilities::outcome()),
        Some(Command::Completions { .. }) => Err(CliError::arguments(
            "completion generation was not dispatched".to_owned(),
        )),
        Some(Command::Update(_)) => Err(CliError::arguments("invalid update command".to_owned())),
        Some(Command::AttachmentCheckWorker) => Err(CliError::arguments(
            "internal attachment worker was not dispatched".to_owned(),
        )),
        None => {
            let resume = match cli.resume {
                None => ResumeRequest::Fresh,
                Some(None) => ResumeRequest::Picker,
                Some(Some(reference)) => ResumeRequest::Target(reference),
            };
            execute_launch(context, cli.continue_latest, resume, !cli.json)
        }
    }
}

fn execute_launch(
    mut context: RuntimeContext,
    continue_latest: bool,
    resume: ResumeRequest,
    interactive: bool,
) -> Result<Outcome, CliError> {
    if interactive {
        terminal::require_interactive()?;
    }
    let settings = interactive
        .then(|| context.terminal_settings())
        .transpose()?;
    let session = if continue_latest {
        session_service(&mut context)?.continue_current()?
    } else {
        match resume {
            ResumeRequest::Target(reference) => {
                let mut service = session_service(&mut context)?;
                let id = service.resolve_session(&reference, false)?;
                service.resume(id)?
            }
            ResumeRequest::Picker if !interactive => {
                return list_sessions(&mut context, None, false);
            }
            ResumeRequest::Picker => {
                let settings = settings.as_ref().ok_or_else(|| {
                    CliError::new(
                        "terminal_failed",
                        "terminal settings unavailable".to_owned(),
                        1,
                    )
                })?;
                let Some(session) = browse_for_session(&mut context, settings)? else {
                    return Ok(cancelled_browser());
                };
                session
            }
            ResumeRequest::Fresh if interactive => {
                let environment = if crate::adapters::herdr::HerdrEnvironment::detect().is_managed()
                {
                    FirstRunEnvironment::HerdrManaged
                } else {
                    FirstRunEnvironment::Standalone
                };
                session_service(&mut context)?.create_first_run_session(environment)?
            }
            ResumeRequest::Fresh => session_service(&mut context)?.create_session()?,
        }
    };
    let id = session.state.board.session.id;
    context.finish_exact_resume(id)?;
    if interactive {
        let resources = context.into_terminal(session, settings.unwrap_or_default());
        let _closed = terminal::run(resources)?;
    }
    Ok(opened_session(id))
}

#[expect(
    clippy::too_many_lines,
    reason = "the exhaustive typed command dispatcher keeps each route visible"
)]
fn execute_thoughts(
    context: &mut RuntimeContext,
    command: ThoughtCommand,
) -> Result<Outcome, CliError> {
    match command {
        ThoughtCommand::List { session } => queries::list(context, &session),
        ThoughtCommand::Inspect { session, thought } => {
            queries::inspect(context, &session, &thought)
        }
        ThoughtCommand::Add {
            session,
            position,
            operation_id,
        } => add_thought(context, &session, position, operation_id.as_deref()),
        ThoughtCommand::Delete {
            session,
            thought,
            operation_id,
        } => delete_thought(context, &session, &thought, operation_id.as_deref()),
        ThoughtCommand::Rename {
            session,
            thought,
            name,
            clear: _,
            operation_id,
        } => thought_names::rename_thought(
            context,
            &session,
            &thought,
            name.as_deref(),
            operation_id.as_deref(),
        ),
        ThoughtCommand::Replace {
            session,
            thought,
            revision_id,
            expected_sha256,
            force,
        } => external_thoughts::replace(
            context,
            &session,
            &thought,
            revision_id.as_deref(),
            expected_sha256.as_deref(),
            force,
        ),
        ThoughtCommand::Collapse {
            session,
            thought,
            collapsed,
            operation_id,
        } => external_thoughts::collapse(
            context,
            &session,
            &thought,
            collapsed,
            operation_id.as_deref(),
        ),
        ThoughtCommand::Move {
            session,
            thought,
            position,
            operation_id,
        } => move_thought(
            context,
            &session,
            &thought,
            position,
            operation_id.as_deref(),
        ),
        ThoughtCommand::Split {
            session,
            thought,
            at_byte,
            expected_sha256,
            operation_id,
        } => transformations::split(
            context,
            &session,
            &thought,
            at_byte,
            &expected_sha256,
            operation_id.as_deref(),
        ),
        ThoughtCommand::Extract {
            session,
            thought,
            start_byte,
            end_byte,
            expected_sha256,
            operation_id,
        } => transformations::extract(
            context,
            &session,
            &thought,
            start_byte..end_byte,
            &expected_sha256,
            operation_id.as_deref(),
        ),
        ThoughtCommand::Merge {
            session,
            thoughts,
            expected_sha256,
            operation_id,
        } => transformations::merge(
            context,
            &session,
            &thoughts,
            &expected_sha256,
            operation_id.as_deref(),
        ),
        ThoughtCommand::Reflow {
            session,
            thought,
            expected_sha256,
            operation_id,
        } => transformations::reflow(
            context,
            &session,
            &thought,
            &expected_sha256,
            operation_id.as_deref(),
        ),
        ThoughtCommand::Send {
            source,
            thought,
            destination,
            remove,
            operation_id,
            remove_operation_id,
        } => execute_send_thought(
            context,
            &source,
            &thought,
            &destination,
            remove,
            operation_id.as_deref(),
            remove_operation_id.as_deref(),
        ),
        ThoughtCommand::Undo(arguments) => move_history(context, &arguments, true),
        ThoughtCommand::Redo(arguments) => move_history(context, &arguments, false),
    }
}

fn execute_send_thought(
    context: &mut RuntimeContext,
    source: &str,
    thought: &str,
    destination: &str,
    remove: bool,
    operation_id: Option<&str>,
    remove_operation_id: Option<&str>,
) -> Result<Outcome, CliError> {
    transfer::send_thought(
        context,
        source,
        thought,
        destination,
        remove,
        operation_id,
        remove_operation_id,
    )
}

fn add_thought(
    context: &mut RuntimeContext,
    session: &str,
    position: Option<usize>,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let body = read_standard_input()?;
    let operation = parse_operation_id(operation)?;
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(session, false)?;
    drop(service);
    if let Some(result) = forwarding::add(context, session_id, &body, position, operation)? {
        return Ok(mutation_outcome(result.thought_id, result.receipt));
    }
    let mut service = session_service(context)?;
    let result = service.add_thought(session_id, body, position, operation)?;
    Ok(mutation_outcome(result.thought_id, result.receipt))
}

fn delete_thought(
    context: &mut RuntimeContext,
    session: &str,
    thought: &str,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let thought_id = parse_thought_id(thought)?;
    let operation = parse_operation_id(operation)?;
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(session, false)?;
    drop(service);
    if let Some(result) = forwarding::delete(context, session_id, thought_id, operation)? {
        return Ok(mutation_outcome(result.thought_id, result.receipt));
    }
    let mut service = session_service(context)?;
    let result = service.delete_thought(session_id, thought_id, operation)?;
    Ok(mutation_outcome(result.thought_id, result.receipt))
}

fn move_thought(
    context: &mut RuntimeContext,
    session: &str,
    thought: &str,
    position: usize,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let thought_id = parse_thought_id(thought)?;
    let operation = parse_operation_id(operation)?;
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(session, false)?;
    drop(service);
    if let Some(result) =
        forwarding::move_thought(context, session_id, thought_id, position, operation)?
    {
        return Ok(mutation_outcome(result.thought_id, result.receipt));
    }
    let mut service = session_service(context)?;
    let result = service.move_thought(session_id, thought_id, position, operation)?;
    Ok(mutation_outcome(result.thought_id, result.receipt))
}

fn move_history(
    context: &mut RuntimeContext,
    arguments: &HistoryArgs,
    undo: bool,
) -> Result<Outcome, CliError> {
    let thought = arguments
        .thought
        .as_deref()
        .map(parse_thought_id)
        .transpose()?;
    let scope = thought.map_or(UndoScope::Board, |thought_id| UndoScope::Editor {
        thought_id,
    });
    let operation = parse_operation_id(arguments.operation_id.as_deref())?;
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(&arguments.session, false)?;
    drop(service);
    if let Some(receipt) = forwarding::history(context, session_id, scope, undo, operation)? {
        return Ok(receipt_outcome(receipt));
    }
    let mut service = session_service(context)?;
    let receipt = service.move_history(session_id, scope, undo, operation)?;
    Ok(receipt_outcome(receipt))
}

pub(super) fn mutation_outcome(thought_id: ThoughtId, receipt: CommitReceipt) -> Outcome {
    let mut outcome = receipt_outcome(receipt);
    outcome.data["thought_id"] = json!(thought_id);
    outcome.human = format!("Thought {thought_id}\n{}", outcome.human);
    outcome
}

fn receipt_outcome(receipt: CommitReceipt) -> Outcome {
    let operation_id = match receipt.identity {
        DurableIdentity::Operation(id) => id.to_string(),
        DurableIdentity::Revision(id) => id.to_string(),
    };
    Outcome {
        data: json!({
            "receipt": {
                "session_id": receipt.session_id,
                "sequence": receipt.sequence,
                "operation_id": operation_id,
                "idempotent_replay": receipt.idempotent_replay,
            }
        }),
        human: format!(
            "Committed {operation_id} at sequence {}{}",
            receipt.sequence.get(),
            if receipt.idempotent_replay {
                " (replay)"
            } else {
                ""
            }
        ),
    }
}

pub(super) fn item_mutation_outcome(item_ids: &[BoardItemId], receipt: CommitReceipt) -> Outcome {
    let mut outcome = receipt_outcome(receipt);
    outcome.data["item_ids"] = json!(item_ids);
    outcome.human = format!(
        "Items {}\n{}",
        item_ids
            .iter()
            .map(|id| match id {
                BoardItemId::Thought(id) => id.to_string(),
                BoardItemId::Separator(id) => id.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", "),
        outcome.human
    );
    outcome
}

pub(super) fn session_service(
    context: &mut RuntimeContext,
) -> Result<
    SessionService<
        '_,
        crate::adapters::sqlite::SqliteStore,
        crate::adapters::runtime::FileRuntimeCoordinator,
        crate::adapters::runtime::SystemClock,
        crate::adapters::runtime::SystemIdGenerator,
    >,
    CliError,
> {
    SessionService::new(
        &mut context.store,
        &context.coordinator,
        &context.clock,
        &mut context.ids,
        context.cwd.clone(),
    )
    .map_err(Into::into)
}
