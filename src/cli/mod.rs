//! Human and machine-readable command-line interface.

mod args;
mod execute;
mod output;
mod runtime;

use std::{ffi::OsString, process::ExitCode};

use clap::Parser;

use args::Cli;
use output::{CliError, render_error};

/// Parse supplied process arguments and execute the selected command.
///
/// The thin binary supplies operating-system arguments so environment access
/// remains in the composition layer.
#[must_use]
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let arguments: Vec<_> = arguments.into_iter().collect();
    let wants_json = arguments.iter().any(|value| value == "--json");
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => {
            if wants_json {
                return render_error(&CliError::arguments(error.to_string()), true);
            }
            let code = error.exit_code();
            let _printed = error.print();
            return ExitCode::from(u8::try_from(code).unwrap_or(2));
        }
    };
    execute::execute(cli)
}

#[cfg(test)]
mod tests {
    use super::args::Cli;
    use clap::{CommandFactory, Parser};

    #[test]
    fn command_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn resume_accepts_an_optional_reference() {
        let picker = Cli::try_parse_from(["proqi", "-r"]).expect("picker arguments");
        assert_eq!(picker.resume, Some(None));
        let target = Cli::try_parse_from(["proqi", "-r", "work"]).expect("resume arguments");
        assert_eq!(target.resume, Some(Some("work".to_owned())));
    }

    #[test]
    fn explicit_update_check_has_a_stable_command_shape() {
        let parsed =
            Cli::try_parse_from(["proqi", "--json", "update", "check"]).expect("update arguments");
        assert!(parsed.json);
        assert!(matches!(
            parsed.command,
            Some(super::args::Command::Update(_))
        ));
    }
}
