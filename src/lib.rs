//! Proqi application library.
//!
//! The library owns product behavior. The `proqi` binary is a thin adapter.

#![forbid(unsafe_code)]

pub mod adapters;
pub mod application;
pub mod cli;
pub mod domain;
pub mod ports;
pub mod ui;
