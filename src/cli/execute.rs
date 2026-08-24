//! CLI dispatch into the shared session service.

mod capabilities;
mod forwarding;
mod sessions;
mod transfer;

use std::{io::Read, process::ExitCode, str::FromStr};

use serde_json::{Value, json};

use crate::{
    adapters::terminal,
    application::SessionService,
    domain::{OperationId, SessionId, ThoughtId, UndoScope},
    ports::{
        environment::Clock,
        store::{CommitReceipt, DurableIdentity},
    },
};

use super::{
    args::{Cli, Command, HistoryArgs, ThoughtCommand},
    output::{CliError, render_error, render_success},
    runtime::RuntimeContext,
};

use sessions::{browser_items, execute_sessions, list_sessions};

const MAX_THOUGHT_STDIN_BYTES: usize = 128 * 1024;

struct Outcome {
    data: Value,
    human: String,
}

enum ResumeRequest {
    Fresh,
    Picker,
    Target(String),
}

pub(super) fn execute(cli: Cli) -> ExitCode {
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
    let context = RuntimeContext::open(cli.state_dir.as_deref())?;
    match cli.command {
        Some(Command::Sessions(arguments)) => {
            let mut context = context;
            execute_sessions(&mut context, arguments.command)
        }
        Some(Command::Thoughts(arguments)) => {
            let mut context = context;
            execute_thoughts(&mut context, arguments.command)
        }
        Some(Command::Capabilities) => Ok(capabilities::outcome()),
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
            ResumeRequest::Fresh => session_service(&mut context)?.create_session()?,
        }
    };
    let id = session.state.board.session.id;
    if interactive {
        let resources = context.into_terminal(
            session,
            settings.unwrap_or_else(crate::ui::UiSettings::default),
        );
        let _closed = terminal::run(resources)?;
    }
    Ok(opened_session(id))
}

fn browse_for_session(
    context: &mut RuntimeContext,
    settings: &crate::ui::UiSettings,
) -> Result<
    Option<crate::application::LeasedSession<crate::adapters::runtime::FileSessionLease>>,
    CliError,
> {
    loop {
        let items = browser_items(context)?;
        let now = context.clock.now();
        match terminal::pick_session(items, now, settings)? {
            crate::ui::BrowserAction::Open(id) => {
                return session_service(context)?
                    .resume(id)
                    .map(Some)
                    .map_err(Into::into);
            }
            crate::ui::BrowserAction::Rename { session_id, name } => {
                session_service(context)?.rename_session(session_id, name.as_deref())?;
            }
            crate::ui::BrowserAction::Trash(id) => {
                session_service(context)?.trash_session(id)?;
            }
            crate::ui::BrowserAction::Cancel => return Ok(None),
            crate::ui::BrowserAction::Continue => {
                return Err(CliError::new(
                    "terminal_failed",
                    "session browser returned an incomplete action".to_owned(),
                    1,
                ));
            }
        }
    }
}

fn cancelled_browser() -> Outcome {
    Outcome {
        data: json!({ "cancelled": true }),
        human: "No session opened".to_owned(),
    }
}

fn opened_session(id: SessionId) -> Outcome {
    let resume = format!("proqi -r {id}");
    Outcome {
        data: json!({ "session_id": id, "resume_command": resume }),
        human: format!("Session {id}\nResume later: {resume}"),
    }
}

fn execute_thoughts(
    context: &mut RuntimeContext,
    command: ThoughtCommand,
) -> Result<Outcome, CliError> {
    match command {
        ThoughtCommand::List { session } => list_thoughts(context, &session),
        ThoughtCommand::Inspect { session, thought } => {
            inspect_thought(context, &session, &thought)
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
        ThoughtCommand::Send {
            source,
            thought,
            destination,
            remove,
            operation_id,
            remove_operation_id,
        } => transfer::send_thought(
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

fn list_thoughts(context: &mut RuntimeContext, reference: &str) -> Result<Outcome, CliError> {
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(reference, true)?;
    let snapshot = service.inspect_session(session_id)?;
    let thoughts: Vec<_> = snapshot
        .board
        .live_thoughts()
        .into_iter()
        .map(|thought| {
            json!({
                "id": thought.id,
                "position": thought.position,
                "content": thought.content,
                "collapsed": thought.collapsed,
                "updated_at": thought.updated_at,
            })
        })
        .collect();
    let human = snapshot
        .board
        .live_thoughts()
        .into_iter()
        .map(|thought| {
            format!(
                "{}  {}  {}",
                thought.position.get(),
                thought.id,
                excerpt(&thought.content)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Outcome {
        data: json!({ "session_id": session_id, "thoughts": thoughts }),
        human,
    })
}

fn inspect_thought(
    context: &mut RuntimeContext,
    session: &str,
    thought: &str,
) -> Result<Outcome, CliError> {
    let thought_id = parse_thought_id(thought)?;
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(session, true)?;
    let snapshot = service.inspect_session(session_id)?;
    let thought = snapshot.board.thought(thought_id).ok_or_else(|| {
        CliError::new(
            "thought_not_found",
            format!("thought not found: {thought_id}"),
            3,
        )
    })?;
    Ok(Outcome {
        data: json!({
            "session_id": session_id,
            "thought": {
                "id": thought.id,
                "content": thought.content,
                "position": thought.position,
                "collapsed": thought.collapsed,
                "deleted_at": thought.deleted_at,
            }
        }),
        human: thought.content.clone(),
    })
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

fn mutation_outcome(thought_id: ThoughtId, receipt: CommitReceipt) -> Outcome {
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

fn session_service(
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

fn parse_thought_id(value: &str) -> Result<ThoughtId, CliError> {
    ThoughtId::from_str(value).map_err(|error| {
        CliError::identifier(format!("invalid thought identifier {value}: {error}"))
    })
}

fn parse_operation_id(value: Option<&str>) -> Result<Option<OperationId>, CliError> {
    value
        .map(|value| {
            OperationId::from_str(value).map_err(|error| {
                CliError::identifier(format!("invalid operation identifier {value}: {error}"))
            })
        })
        .transpose()
}

fn read_standard_input() -> Result<String, CliError> {
    let mut content = String::new();
    std::io::stdin()
        .take((MAX_THOUGHT_STDIN_BYTES + 1) as u64)
        .read_to_string(&mut content)
        .map_err(|error| CliError::input(format!("read standard input: {error}")))?;
    if content.len() > MAX_THOUGHT_STDIN_BYTES {
        return Err(CliError::input(format!(
            "thought content exceeds the {MAX_THOUGHT_STDIN_BYTES}-byte standard-input limit"
        )));
    }
    Ok(content)
}

fn excerpt(content: &str) -> String {
    content
        .chars()
        .take(80)
        .collect::<String>()
        .replace('\n', " ")
}
