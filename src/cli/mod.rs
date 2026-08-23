//! Human and machine-readable command-line interface.

use std::process::ExitCode;

use clap::{CommandFactory, Parser};

/// Proqi command-line arguments.
#[derive(Debug, Parser)]
#[command(
    name = "proqi",
    version,
    about = "A terminal-native scratchpad for agent work",
    long_about = None
)]
struct Cli {}

/// Parse arguments and execute the selected command.
#[must_use]
pub fn run() -> ExitCode {
    let _arguments = Cli::parse();
    let mut command = Cli::command();
    if command.print_help().is_err() {
        return ExitCode::FAILURE;
    }
    println!();
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn command_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
