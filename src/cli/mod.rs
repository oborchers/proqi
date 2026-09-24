//! Human and machine-readable command-line interface.

mod args;
mod error_code;
mod execute;
mod output;
mod runtime;

use std::{ffi::OsString, process::ExitCode};

use clap::{Parser, error::ErrorKind};
use serde_json::json;

use args::Cli;
use output::{CliError, render_error, render_success};

/// Parse supplied process arguments and execute the selected command.
///
/// The thin binary supplies operating-system arguments so environment access
/// remains in the composition layer.
#[must_use]
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let arguments: Vec<_> = arguments.into_iter().collect();
    let wants_json = json_flag_before_terminator(&arguments);
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => return render_parse_result(&error, wants_json),
    };
    if matches!(cli.command, Some(args::Command::AttachmentCheckWorker)) {
        return crate::adapters::attachment::worker::run_stdio();
    }
    execute::execute(cli)
}

/// Whether `--json` appeared as an option rather than as a positional value.
///
/// A successful parse always uses the parsed flag. This fallback is used only
/// when parsing stops early; values after `--` are positional in clap, so a
/// literal `--json` there cannot select the machine envelope.
fn json_flag_before_terminator(arguments: &[OsString]) -> bool {
    arguments
        .iter()
        .skip(1)
        .take_while(|argument| *argument != "--")
        .any(|argument| argument == "--json")
}

/// Render clap's informational displays and argument errors.
fn render_parse_result(error: &clap::Error, json_output: bool) -> ExitCode {
    let informational = matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    );
    if !json_output {
        let code = error.exit_code();
        let _printed = error.print();
        return ExitCode::from(u8::try_from(code).unwrap_or(2));
    }
    if !informational {
        return render_error(&CliError::arguments(error.to_string()), true);
    }
    let text = error.render().to_string();
    let data = if error.kind() == ErrorKind::DisplayVersion {
        json!({ "name": env!("CARGO_PKG_NAME"), "version": env!("CARGO_PKG_VERSION") })
    } else {
        json!({ "help": text })
    };
    render_success(&data, text.trim_end(), true)
}

#[cfg(test)]
mod tests {
    use super::args::Cli;
    use clap::{CommandFactory, Parser};

    #[test]
    fn only_an_option_before_the_terminator_selects_json_after_a_parse_failure() {
        let arguments = |values: &[&str]| {
            values
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        };
        assert!(super::json_flag_before_terminator(&arguments(&[
            "proqi", "sessions", "--json", "rename"
        ])));
        assert!(!super::json_flag_before_terminator(&arguments(&[
            "proqi", "sessions", "rename", "--", "--json"
        ])));
        assert!(!super::json_flag_before_terminator(&arguments(&[
            "proqi",
            "--query=--json"
        ])));
        assert!(!super::json_flag_before_terminator(&arguments(&["--json"])));
    }

    #[test]
    fn command_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn capability_help_describes_the_current_pre_one_contract() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("Describe the current CLI and optional integrations"));
        assert!(!help.contains("Describe the stable CLI"));
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

    #[test]
    fn completions_require_one_supported_shell() {
        let parsed =
            Cli::try_parse_from(["proqi", "completions", "zsh"]).expect("completion arguments");
        assert!(matches!(
            parsed.command,
            Some(super::args::Command::Completions { .. })
        ));
        assert!(Cli::try_parse_from(["proqi", "completions"]).is_err());
        assert!(Cli::try_parse_from(["proqi", "completions", "elvish"]).is_err());
        assert!(Cli::try_parse_from(["proqi", "completions", "unknown-shell"]).is_err());
    }

    #[test]
    fn keypress_inspector_is_an_explicit_diagnostic_command() {
        let parsed =
            Cli::try_parse_from(["proqi", "diagnostics", "keypress"]).expect("keypress arguments");
        assert!(matches!(
            parsed.command,
            Some(super::args::Command::Diagnostics(_))
        ));
    }
}
