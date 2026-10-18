//! Background service: Unix-socket control protocol, CLI client and the
//! foreground daemon process. See `docs/DAEMON.md`.

pub mod client;
pub mod protocol;
mod server;

pub use server::{run, serve_at};
