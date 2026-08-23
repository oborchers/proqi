//! Thin executable entry point for Proqi.

use std::process::ExitCode;

fn main() -> ExitCode {
    proqi::cli::run()
}
