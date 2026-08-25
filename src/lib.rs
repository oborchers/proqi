//! Proqi application library.
//!
//! The library owns product behavior. The `proqi` binary is a thin adapter.
//!
//! ```
//! assert!(proqi::domain::StableVersion::parse("0.1.0").is_ok());
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::unwrap_used
    )
)]

pub mod adapters;
pub mod application;
pub mod cli;
pub mod domain;
pub mod ports;
pub mod ui;
