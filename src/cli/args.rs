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
    /// Internal attachment accessibility worker.
    #[command(name = "__attachment-check", hide = true)]
    AttachmentCheckWorker,
    /// Describe the current CLI and optional integrations.
    Capabilities,
    /// Generate a shell completion script on standard output.
    Completions {
        /// Shell whose completion syntax should be generated.
        shell: CompletionShell,
    },
    /// Check the verified installation channel on demand.
    Update(UpdateArgs),
    /// Collect a private, content-redacted local support bundle.
    Diagnostics(DiagnosticsArgs),
    /// Run read-only local health checks without repairing state.
    Doctor,
    /// List and manage resumable sessions.
    Sessions(SessionArgs),
    /// Mutate typed thoughts and separators in shared Board order.
    Items(ItemArgs),
    /// Inspect and mutate thoughts in one explicit session.
    Thoughts(ThoughtArgs),
}

#[derive(Debug, Args)]
pub(super) struct ItemArgs {
    #[command(subcommand)]
    pub(super) command: ItemCommand,
}

#[derive(Debug, Subcommand)]
pub(super) enum ItemCommand {
    /// Insert one payload-free durable separator.
    InsertSeparator {
        session: String,
        /// Zero-based position in the shared Board item order. Defaults to the end.
        #[arg(long)]
        position: Option<usize>,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Move one typed thought or separator in shared Board order.
    Move {
        session: String,
        item: String,
        position: usize,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Soft-delete Board-ordered typed items as one Board operation.
    Delete {
        session: String,
        #[arg(required = true, num_args = 1..)]
        items: Vec<String>,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Duplicate Board-ordered typed items as one Board operation.
    Duplicate {
        session: String,
        #[arg(required = true, num_args = 1..)]
        items: Vec<String>,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
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
    /// Capture one logical key with bounded, content-redacted contextual resolution.
    Keypress {
        /// Context stack, bottom to top. Uses stable keymap context identifiers.
        #[arg(long, default_value = "board", value_delimiter = ',')]
        context: Vec<String>,
        /// Bounded capture duration, in milliseconds.
        #[arg(long, default_value_t = 5000, value_parser = clap::value_parser!(u64).range(100..=60000))]
        timeout_ms: u64,
        /// Inspect factory bindings without loading configuration, including invalid files.
        #[arg(long)]
        defaults: bool,
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
    /// Report the latest stable version installable through this channel.
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
        #[command(flatten)]
        page: PageArgs,
    },
    /// Return the one live session with this name and directory, creating it when absent.
    Ensure {
        /// Exact session name.
        #[arg(long)]
        name: String,
        /// Existing origin directory that identifies the session with its name.
        #[arg(long, value_name = "PATH")]
        cwd: PathBuf,
    },
    /// Create one additional named session without opening it.
    Create {
        /// Exact session name. Existing sessions may already use it.
        #[arg(long)]
        name: String,
        /// Existing origin directory. Defaults to the current directory.
        #[arg(long, value_name = "PATH")]
        cwd: Option<PathBuf>,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
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
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Move a session to recoverable trash. An already trashed session is unchanged.
    Trash {
        session: String,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Restore a session from recoverable trash.
    Restore {
        session: String,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Undo the latest session rename, trash, or restore from Browser history.
    Undo {
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Redo the next session rename, trash, or restore from Browser history.
    Redo {
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Permanently delete an already trashed session.
    Prune {
        session: String,
        /// Confirm permanent deletion.
        #[arg(long)]
        yes: bool,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
}

/// Bounded list pagination shared by list commands.
#[derive(Clone, Debug, Default, Args)]
pub(super) struct PageArgs {
    /// Return at most this many entries.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..))]
    pub(super) limit: Option<u32>,
    /// Continue after this entry identifier from a previous `next_after`.
    #[arg(long, value_name = "ID")]
    pub(super) after: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct ThoughtArgs {
    #[command(subcommand)]
    pub(super) command: ThoughtCommand,
}

#[derive(Debug, Subcommand)]
pub(super) enum ThoughtCommand {
    /// List thoughts and separators in Board order.
    List {
        session: String,
        #[command(flatten)]
        page: PageArgs,
    },
    /// Print one exact thought body and metadata.
    Inspect { session: String, thought: String },
    /// Add standard input as one thought.
    Add {
        session: String,
        /// Optional organizational name created in the same Board operation.
        #[arg(long)]
        name: Option<String>,
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
    /// Set or clear one thought's optional organizational name.
    Rename {
        session: String,
        thought: String,
        /// Replacement name. An empty value clears it.
        #[arg(required_unless_present = "clear", conflicts_with = "clear")]
        name: Option<String>,
        /// Clear the optional name.
        #[arg(long)]
        clear: bool,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Replace standard input as one exact editor revision.
    Replace {
        session: String,
        thought: String,
        /// Durable idempotency identity for this editor revision.
        #[arg(long, value_name = "REV_ID")]
        revision_id: Option<String>,
        /// Required SHA-256 of current content.
        #[arg(long, value_name = "HEX", required_unless_present = "force")]
        expected_sha256: Option<String>,
        /// Deliberately replace without a content precondition.
        #[arg(long, conflicts_with = "expected_sha256")]
        force: bool,
    },
    /// Set the durable collapsed state of one thought.
    Collapse {
        session: String,
        thought: String,
        /// True collapses; false expands.
        #[arg(long, action = clap::ArgAction::Set)]
        collapsed: bool,
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
    /// Split one thought at an exact UTF-8 byte boundary.
    Split {
        session: String,
        thought: String,
        at_byte: usize,
        /// Required SHA-256 of current content.
        #[arg(long, value_name = "HEX")]
        expected_sha256: String,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Extract one exact nonempty UTF-8 byte range into a new thought.
    Extract {
        session: String,
        thought: String,
        start_byte: usize,
        end_byte: usize,
        /// Required SHA-256 of current content.
        #[arg(long, value_name = "HEX")]
        expected_sha256: String,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Merge exact Board-contiguous thoughts in the supplied order.
    Merge {
        session: String,
        #[arg(required = true, num_args = 2..)]
        thoughts: Vec<String>,
        /// SHA-256 preconditions paired with the thoughts in the same order.
        #[arg(long = "expected-sha256", required = true, num_args = 1)]
        expected_sha256: Vec<String>,
        /// Durable idempotency identity.
        #[arg(long, value_name = "OP_ID")]
        operation_id: Option<String>,
    },
    /// Clean one thought with Proqi's canonical spacing policy.
    Reflow {
        session: String,
        thought: String,
        /// Required SHA-256 of current content.
        #[arg(long, value_name = "HEX")]
        expected_sha256: String,
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
