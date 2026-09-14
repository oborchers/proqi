//! Update-aware runtime opening for commands that can touch the session schema.

use super::{Cli, CliError, RuntimeContext};

pub(super) enum ResumeRequest {
    Fresh,
    Picker,
    Target(String),
}

pub(super) fn open(cli: &Cli) -> Result<RuntimeContext, CliError> {
    RuntimeContext::open(
        cli.state_dir.as_deref(),
        cli.resume.as_ref().and_then(Option::as_deref),
    )
}
