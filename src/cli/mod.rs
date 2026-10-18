//! CLI entry point: clap definitions plus `run` called by `main.rs`.

pub(crate) mod commands;

use crate::config::settings::Target;
use clap::{Parser, Subcommand};
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
        /// What to capture; defaults to config.toml's `default_target`.
        #[arg(long, value_enum)]
        target: Option<TargetArg>,
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
    /// Run the background control daemon in the foreground.
    Daemon,
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

/// Exit codes: 0 ok, 1 not recording / already recording / no display,
/// 2 backend missing, unsupported backend or clap parse error, 3 capture
/// failure. Every path exits the process: never returns.
pub fn run<I: IntoIterator<Item = OsString>>(args: I) -> ! {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => e.exit(), // clap prints usage; DisplayHelp exits 0, errors 2
    };
    match cli.cmd {
        None => {
            // Bare `niri-recorder`: launch the GUI. `ui::run` runs the
            // main loop until the window closes and exits the process
            // itself (code 1) when no display is available.
            std::process::exit(i32::from(crate::ui::run().get()));
        }
        Some(Cmd::Start { target, output }) => commands::start(target, output),
        Some(Cmd::Stop) => commands::stop(),
        Some(Cmd::Status) => commands::status(),
        Some(Cmd::Diagnostics) => commands::diagnostics(),
        Some(Cmd::Daemon) => crate::daemon::run(),
    }
}
