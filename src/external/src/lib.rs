//! `instinctagents` library surface — exposes the modules that the binary
//! wires together so integration tests under `tests/` can drive the
//! install/remove paths directly (US-018).

pub mod catalog;
pub mod cli;
pub mod harness;
pub mod http;
pub mod installer;
pub mod state;
pub mod tui;
pub mod update;
