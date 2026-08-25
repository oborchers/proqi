//! Clap-owned syntax without application behavior.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "proqi",
    version,
    about = "An agent-optimized scratchpad for follow-up prompts",
    long_about = None
)]
pub(super) struct Cli {
    /// Emit the versioned machine-readable contract.
    #[arg(long, global = true)]
    pub(super) json: bool,
    /// Use an isolated state root. Intended for diagnostics and tests.
    #[arg(long, global = true, hide = true, value_name = "DIR")]
    pub(super) state_dir: Option<PathBuf>,
    /// Continue the latest inactive session for the current directory.
    #[arg(short = 'c', long = "continue", conflicts_with = "resume")]
    pub(super) continue_latest: bool,
    /// Resume a session, or open the session picker when no reference follows.
    #[expect(
        clippy::option_option,
        reason = "absent, picker, and explicit target are distinct CLI states"
    )]
    #[arg(short = 'r', long = "resume", num_args = 0..=1, value_name = "ID_OR_NAME", conflicts_with = "continue_latest")]
    pub(super) resume: Option<Option<String>>,
    #[command(subcommand)]
    pub(super) command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub(super) enum Command {
    /// Describe the current CLI and optional integrations.
    Capabilities,
    /// Generate a shell completion script on standard output.
    Completions {
        /// Shell whose completion syntax should be generated.
        shell: CompletionShell,
    },
    /// Check the canonical GitHub stable release on demand.
    Update(UpdateArgs),
    /// Collect a private, content-redacted local support bundle.
    Diagnostics(DiagnosticsArgs),
    /// Run read-only local health checks without repairing state.
    Doctor,
    /// List and manage resumable sessions.
    Sessions(SessionArgs),
    /// Inspect and mutate thoughts in one explicit session.
    Thoughts(ThoughtArgs),
}

#[derive(Debug, Args)]
pub(super) struct DiagnosticsArgs {
    #[command(subcommand)]
    pub(super) command: DiagnosticsCommand,
}

#[derive(Debug, Subcommand)]
pub(super) enum DiagnosticsCommand {
    /// Write retained structured events without uploading them.
    Collect {
        /// New output path. Existing files are never overwritten.
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(super) enum CompletionShell {
    Bash,
    Fish,
    Zsh,
}

impl From<CompletionShell> for clap_complete::Shell {
    fn from(value: CompletionShell) -> Self {
        match value {
            CompletionShell::Bash => Self::Bash,
            CompletionShell::Fish => Self::Fish,
            CompletionShell::Zsh => Self::Zsh,
        }
    }
}

#[derive(Debug, Args)]
pub(super) struct UpdateArgs {
    #[command(subcommand)]
    pub(super) command: UpdateCommand,
}

#[derive(Clone, Copy, Debug, Subcommand)]
pub(super) enum UpdateCommand {
    /// Contact GitHub and report the latest stable release.
    Check,
}

#[derive(Debug, Args)]
pub(super) struct SessionArgs {
    #[command(subcommand)]
    pub(super) command: Option<SessionCommand>,
}

#[derive(Debug, Subcommand)]
pub(super) enum SessionCommand {
    /// List sessions, ranked for the current directory.
    List {
        /// Search optional names, paths, and thought content.
        #[arg(short, long)]
        query: Option<String>,
        /// Include recoverably trashed sessions.
        #[arg(long)]
        all: bool,
    },
    /// Set or clear an optional session name.
    Rename {
        session: String,
        /// New name. Omit only together with `--clear`.
        #[arg(required_unless_present = "clear")]
        name: Option<String>,
        /// Clear the optional name.
        #[arg(long, conflicts_with = "name")]
        clear: bool,
    },
    /// Move a session to recoverable trash.
    Trash { session: String },
    /// Restore a session from recoverable trash.
    Restore { session: String },
    /// Permanently delete an already trashed session.
    Prune {
        session: String,
        /// Confirm permanent deletion.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Args)]
pub(super) struct ThoughtArgs {
    #[command(subcommand)]
    pub(super) command: ThoughtCommand,
}

#[derive(Debug, Subcommand)]
pub(super) enum ThoughtCommand {
    /// List thoughts in board order.
    List { session: String },
    /// Print one exact thought body and metadata.
    Inspect { session: String, thought: String },
    /// Add standard input as one thought.
    Add {
        session: String,
        /// Zero-based insertion position. Defaults to the end.
        #[arg(long)]
        position: Option<usize>,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Soft-delete one thought.
    Delete {
        session: String,
        thought: String,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Move one thought to a zero-based position.
    Move {
        session: String,
        thought: String,
        position: usize,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Copy one thought into another Proqi session.
    Send {
        /// Session that currently contains the thought.
        source: String,
        /// Canonical thought identifier.
        thought: String,
        /// Destination session identifier or unique name.
        destination: String,
        /// Remove the source thought only after destination durability.
        #[arg(long)]
        remove: bool,
        /// Durable idempotency identity for destination creation.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
        /// Durable idempotency identity for optional source removal.
        #[arg(long, value_name = "OP_ID", requires = "remove")]
        remove_operation_id: Option<String>,
    },
    /// Undo one persistent board or editor operation.
    Undo(HistoryArgs),
    /// Redo one persistent board or editor operation.
    Redo(HistoryArgs),
}

#[derive(Debug, Args)]
pub(super) struct HistoryArgs {
    pub(super) session: String,
    /// Address one thought's editor history instead of board history.
    #[arg(long)]
    pub(super) thought: Option<String>,
    /// Durable idempotency identity.
    #[arg(long, value_name = "OP_ID")]
    pub(super) operation_id: Option<String>,
}
