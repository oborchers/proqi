//! CLI dispatch into the shared session service.

use std::{collections::HashSet, io::Read, process::ExitCode, str::FromStr};

use serde_json::{Value, json};

use crate::{
    application::SessionService,
    domain::{OperationId, SessionId, ThoughtId, UndoScope},
    ports::{
        runtime::RuntimeCoordinator,
        store::{CommitReceipt, DurableIdentity},
    },
};

use super::{
    args::{Cli, Command, HistoryArgs, SessionCommand, ThoughtCommand},
    output::{CliError, render_error, render_success},
    runtime::RuntimeContext,
};

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
        return Ok(capabilities());
    }
    let mut context = RuntimeContext::open(cli.state_dir.as_deref())?;
    match cli.command {
        Some(Command::Sessions(arguments)) => execute_sessions(&mut context, arguments.command),
        Some(Command::Thoughts(arguments)) => execute_thoughts(&mut context, arguments.command),
        Some(Command::Capabilities) => Ok(capabilities()),
        None => {
            let resume = match cli.resume {
                None => ResumeRequest::Fresh,
                Some(None) => ResumeRequest::Picker,
                Some(Some(reference)) => ResumeRequest::Target(reference),
            };
            execute_launch(&mut context, cli.continue_latest, resume)
        }
    }
}

fn capabilities() -> Outcome {
    Outcome {
        data: json!({
            "cli_schema_version": 1,
            "identifier_encoding": "prefix_base32hex_uuidv7",
            "commands": ["sessions", "thoughts"],
            "active_session_control": false,
            "herdr_submission": false,
        }),
        human: "CLI schema 1\nSessions and thoughts are available\nActive control: unavailable\nHerdr: unavailable".to_owned(),
    }
}

fn execute_launch(
    context: &mut RuntimeContext,
    continue_latest: bool,
    resume: ResumeRequest,
) -> Result<Outcome, CliError> {
    let mut service = session_service(context)?;
    if continue_latest {
        let session = service.continue_current()?;
        return Ok(opened_session(session.state.board.session.id));
    }
    match resume {
        ResumeRequest::Target(reference) => {
            let id = service.resolve_session(&reference, false)?;
            let session = service.resume(id)?;
            Ok(opened_session(session.state.board.session.id))
        }
        ResumeRequest::Picker => list_sessions(context, None, false),
        ResumeRequest::Fresh => {
            let session = service.create_session()?;
            Ok(opened_session(session.state.board.session.id))
        }
    }
}

fn opened_session(id: SessionId) -> Outcome {
    let resume = format!("proqi -r {id}");
    Outcome {
        data: json!({ "session_id": id, "resume_command": resume }),
        human: format!("Session {id}\nResume later: {resume}"),
    }
}

fn execute_sessions(
    context: &mut RuntimeContext,
    command: Option<SessionCommand>,
) -> Result<Outcome, CliError> {
    match command.unwrap_or(SessionCommand::List {
        query: None,
        all: false,
    }) {
        SessionCommand::List { query, all } => list_sessions(context, query, all),
        SessionCommand::Rename {
            session,
            name,
            clear,
        } => {
            let mut service = session_service(context)?;
            let id = service.resolve_session(&session, true)?;
            let name = if clear { None } else { name.as_deref() };
            service.rename_session(id, name)?;
            Ok(simple_session_outcome(id, "renamed"))
        }
        SessionCommand::Trash { session } => manage_session(context, &session, "trashed"),
        SessionCommand::Restore { session } => manage_session(context, &session, "restored"),
        SessionCommand::Prune { session, yes } => {
            if !yes {
                return Err(CliError::arguments(
                    "permanent pruning requires --yes".to_owned(),
                ));
            }
            manage_session(context, &session, "pruned")
        }
    }
}

fn manage_session(
    context: &mut RuntimeContext,
    reference: &str,
    action: &str,
) -> Result<Outcome, CliError> {
    let mut service = session_service(context)?;
    let id = service.resolve_session(reference, true)?;
    match action {
        "trashed" => service.trash_session(id)?,
        "restored" => service.restore_session(id)?,
        "pruned" => service.prune_session(id)?,
        _ => return Err(CliError::unsupported(action.to_owned())),
    }
    Ok(simple_session_outcome(id, action))
}

fn simple_session_outcome(id: SessionId, action: &str) -> Outcome {
    Outcome {
        data: json!({ "session_id": id, "status": action }),
        human: format!("Session {id} {action}"),
    }
}

fn list_sessions(
    context: &mut RuntimeContext,
    query: Option<String>,
    all: bool,
) -> Result<Outcome, CliError> {
    let active: HashSet<_> = context
        .coordinator
        .active_instances()?
        .into_iter()
        .map(|instance| instance.session_id)
        .collect();
    let hits = session_service(context)?.list_sessions(query, all)?;
    let data: Vec<_> = hits
        .iter()
        .map(|hit| {
            json!({
                "id": hit.id,
                "name": hit.name,
                "last_opened_cwd": hit.last_opened_cwd,
                "last_active_at": hit.last_active_at,
                "thought_count": hit.thought_count,
                "excerpt": hit.excerpt,
                "state": if hit.trashed { "trashed" } else if active.contains(&hit.id) { "active" } else { "resumable" },
            })
        })
        .collect();
    let human = if hits.is_empty() {
        "No sessions".to_owned()
    } else {
        hits.iter()
            .map(|hit| {
                let state = if hit.trashed {
                    "trashed"
                } else if active.contains(&hit.id) {
                    "active"
                } else {
                    "resumable"
                };
                let label = hit.name.as_deref().unwrap_or(&hit.excerpt);
                format!(
                    "{}  {state}  {}  {label}",
                    hit.id,
                    hit.last_opened_cwd.display()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(Outcome {
        data: json!({ "sessions": data }),
        human,
    })
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
        .read_to_string(&mut content)
        .map_err(|error| CliError::input(format!("read standard input: {error}")))?;
    Ok(content)
}

fn excerpt(content: &str) -> String {
    content
        .chars()
        .take(80)
        .collect::<String>()
        .replace('\n', " ")
}
