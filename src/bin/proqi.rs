//! Thin executable entry point for Proqi.

#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unwrap_used
    )
)]

use std::process::ExitCode;

fn main() -> ExitCode {
    proqi::cli::run()
}
