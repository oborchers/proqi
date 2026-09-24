//! CLI dispatch into the shared session service.

use crate::cli::error_code::ErrorCode;
mod board_items;
mod capabilities;
mod diagnostics;
mod doctor;
mod external_thoughts;
mod forwarding;
mod helpers;
mod pagination;
mod queries;
mod runtime_open;
mod sessions;
mod thought_names;
mod thoughts;
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
    domain::{BoardItemId, ThoughtId},
    ports::store::{CommitReceipt, DurableIdentity},
};

use super::{
    args::{Cli, Command},
    output::{CliError, render_error, render_success},
    runtime::RuntimeContext,
};

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
            thoughts::execute(&mut context, arguments.command)
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
                return list_sessions(&mut context, None, false, &super::args::PageArgs::default());
            }
            ResumeRequest::Picker => {
                let settings = settings.as_ref().ok_or_else(|| {
                    CliError::new(
                        ErrorCode::TerminalFailed,
                        "terminal settings unavailable".to_owned(),
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

pub(super) fn mutation_outcome(thought_id: ThoughtId, receipt: CommitReceipt) -> Outcome {
    let mut outcome = receipt_outcome(receipt);
    outcome.data["thought_id"] = json!(thought_id);
    outcome.human = format!("Thought {thought_id}\n{}", outcome.human);
    outcome
}

pub(super) fn receipt_outcome(receipt: CommitReceipt) -> Outcome {
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
            .map(ToString::to_string)
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
