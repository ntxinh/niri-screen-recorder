//! CLI entry point: clap definitions plus `run` called by `main.rs`.

mod commands;

use crate::config::settings::Target;
use clap::{CommandFactory, Parser, Subcommand};
use std::ffi::OsString;

#[derive(Parser)]
#[command(
    name = "niri-recorder",
    version,
    about = "Screen recorder for Wayland/Niri"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start recording (foreground; Ctrl-C or `stop` to finish).
    Start {
        /// What to capture.
        #[arg(long, value_enum, default_value_t = TargetArg::Display)]
        target: TargetArg,
        /// Display name to capture; default = sole/first output.
        /// Ignored for `--target region`.
        #[arg(long)]
        output: Option<String>,
    },
    /// Ask the running recorder (from the pidfile) to stop.
    Stop,
    /// Show whether a recording is in progress.
    Status,
    /// Print environment/tool/backend diagnostics.
    Diagnostics,
}

/// Clap-facing copy of [`Target`]; kept separate so `ValueEnum` isn't
/// forced onto the config type.
#[derive(clap::ValueEnum, Clone, Copy)]
enum TargetArg {
    Display,
    Region,
}

impl From<TargetArg> for Target {
    fn from(t: TargetArg) -> Self {
        match t {
            TargetArg::Display => Target::Display,
            TargetArg::Region => Target::Region,
        }
    }
}

/// Exit codes: 0 ok, 1 not recording (`stop`/`status`-adjacent), 2 backend
/// missing or clap parse error, 3 capture failure.
pub fn run<I: IntoIterator<Item = OsString>>(args: I) -> anyhow::Result<()> {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => e.exit(), // clap prints usage; DisplayHelp exits 0, errors 2
    };
    match cli.cmd {
        None => {
            // Bare `niri-recorder`: help on stdout, exit 0.
            Cli::command().print_help()?;
        }
        Some(Cmd::Start { target, output }) => commands::start(target.into(), output),
        Some(Cmd::Stop) => commands::stop(),
        Some(Cmd::Status) => commands::status(),
        Some(Cmd::Diagnostics) => commands::diagnostics(),
    }
    Ok(())
}
