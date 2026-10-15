//! Command implementations. Handlers needing a specific exit code print
//! their message and call `std::process::exit` — `run` never returns.
//!
//! `stop`/`status` operate on the pidfile alone: the recording process is
//! the `start` invocation itself, signalled with SIGINT.

use super::TargetArg;
use crate::app::controller::Recorder;
use crate::app::pidfile::{clear_pidfile, read_pidfile, write_pidfile};
use crate::app::state::AppState;
use crate::capture::backend::CaptureBackend;
use crate::capture::backends::wf_recorder::WfRecorder;
use crate::capture::process::is_zombie;
use crate::config::settings::{self, Settings};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Exit codes: 0 ok, 1 already recording / not recording, 2 backend
/// missing or unsupported, 3 capture failure.
pub(super) fn start(target_arg: Option<TargetArg>, output: Option<String>) -> ! {
    let settings = settings::load();
    if let Err(msg) = check_preferred_backend(&settings) {
        eprintln!("niri-recorder: {msg}");
        std::process::exit(2);
    }
    let target = target_arg.map_or(settings.default_target, Into::into);
    // Single instance: create_new is the lock — a live pidfile claims it
    // (exit 1), a stale one is replaced (stale check in pidfile.rs).
    if let Err(e) = write_pidfile() {
        match e.kind() {
            std::io::ErrorKind::AlreadyExists => {
                eprintln!("niri-recorder: {e}");
                std::process::exit(1);
            }
            _ => {
                eprintln!("niri-recorder: cannot write pidfile: {e}");
                std::process::exit(3);
            }
        }
    }
    let backend = match WfRecorder::detect() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("niri-recorder: {e}");
            clear_pidfile();
            std::process::exit(2);
        }
    };
    // SIGINT (Ctrl-C or `niri-recorder stop`) / SIGTERM flips the flag; the
    // stop itself runs on this thread so Recorder stays single-threaded.
    let quit = Arc::new(AtomicBool::new(false));
    let handler_ok = ctrlc::set_handler({
        let quit = Arc::clone(&quit);
        move || quit.store(true, Ordering::SeqCst)
    });
    if handler_ok.is_err() {
        eprintln!("niri-recorder: cannot install signal handler");
        clear_pidfile();
        std::process::exit(3);
    }

    let mut recorder = Recorder::new(backend, settings);
    recorder.set_quit_flag(Arc::clone(&quit));
    if let Err(e) = recorder.start(target, output) {
        eprintln!("niri-recorder: {e}");
        clear_pidfile();
        std::process::exit(3);
    }
    let AppState::Recording { pid } = *recorder.state() else {
        // Ok without Recording = cancelled region selection.
        if *recorder.state() == AppState::Idle {
            println!("cancelled");
            clear_pidfile();
            std::process::exit(0);
        }
        eprintln!("niri-recorder: unexpected state {:?}", recorder.state());
        clear_pidfile();
        std::process::exit(3);
    };
    match recorder.part_path() {
        Some(p) => println!("recording → {}", p.display()),
        None => println!("recording pid={pid}"),
    }

    // ponytail: 100ms poll on flag + child state; try_wait would be nicer
    // but the session child isn't exposed by Recorder. kill(pid, 0) is Ok
    // on zombies, so a dead-but-unreaped child needs a /proc stat check.
    let mut child_died = false;
    while !quit.load(Ordering::SeqCst) {
        if is_zombie(pid) || kill(Pid::from_raw(pid as i32), None).is_err() {
            child_died = true;
            eprintln!("niri-recorder: recorder child exited unexpectedly");
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    let stopped = recorder.stop();
    // Child death is a capture failure even when stop() finalized the
    // partial .part — report it as such, don't claim a clean save.
    match (stopped, child_died) {
        (Ok(path), false) => {
            println!("saved → {}", path.display());
            clear_pidfile();
            std::process::exit(0);
        }
        (Ok(path), true) => {
            eprintln!(
                "niri-recorder: capture ended early; saved {}",
                path.display()
            );
            clear_pidfile();
            std::process::exit(3);
        }
        (Err(e), _) => {
            eprintln!("niri-recorder: stop failed: {e}");
            if let AppState::Failed(reason) = recorder.state() {
                eprintln!("niri-recorder: {reason}");
            }
            clear_pidfile();
            std::process::exit(3);
        }
    }
}

/// Spec §23.6: only `auto`/wf-recorder exist; any other value is an
/// actionable unsupported-backend error rather than a silent ignore.
pub(crate) fn check_preferred_backend(settings: &Settings) -> Result<(), String> {
    match settings.preferred_backend.as_str() {
        "auto" | "wf-recorder" => Ok(()),
        other => Err(format!(
            "unsupported backend {other:?}: only wf-recorder is available on \
             this system (auto resolves to it); set preferred_backend = \"auto\" \
             in config.toml"
        )),
    }
}

/// Signal the recorder recorded in the pidfile to stop.
pub(super) fn stop() -> ! {
    match read_pidfile() {
        Some(e) => match kill(Pid::from_raw(e.pid as i32), Signal::SIGINT) {
            Ok(()) => {
                println!("stop requested (pid {})", e.pid);
                std::process::exit(0);
            }
            Err(err) => {
                eprintln!("niri-recorder: cannot signal pid {}: {err}", e.pid);
                std::process::exit(1);
            }
        },
        None => {
            println!("not recording");
            std::process::exit(1);
        }
    }
}

/// Print whether a recording is in progress (pidfile + liveness).
pub(super) fn status() -> ! {
    match read_pidfile() {
        Some(e) => println!("recording pid={}", e.pid),
        None => println!("idle"),
    }
    std::process::exit(0);
}

/// Print environment, tool, config and backend diagnostics.
pub(super) fn diagnostics() -> ! {
    println!("niri-recorder {}", env!("CARGO_PKG_VERSION"));
    println!(
        "niri: {}",
        first_line("niri", &["--version"]).unwrap_or_else(|| "missing".into())
    );
    println!("outputs: {}", output_list());
    println!(
        "wf-recorder: {}",
        first_line("wf-recorder", &["--version"])
            .or_else(|| first_line("wf-recorder", &["--help"]))
            .unwrap_or_else(|| "missing".into())
    );
    println!(
        "slurp: {}",
        first_line("slurp", &["-h"]).unwrap_or_else(|| "missing".into())
    );
    println!(
        "ffprobe: {}",
        first_line("ffprobe", &["-version"]).unwrap_or_else(|| "missing".into())
    );
    println!("config: {}", config_path());
    match WfRecorder::detect() {
        Ok(b) => match b.capabilities() {
            Ok(c) => println!(
                "backend: wf-recorder (region={} codecs={:?})",
                c.region, c.codecs
            ),
            Err(e) => println!("backend: wf-recorder (capabilities error: {e})"),
        },
        Err(e) => println!("backend: unavailable ({e})"),
    }
    std::process::exit(0);
}

/// Comma-separated output names from `niri msg outputs` (via the backend's
/// probe); `(none detected)` on failure.
fn output_list() -> String {
    match WfRecorder::list_outputs() {
        o if o.is_empty() => "(none detected)".into(),
        o => o.join(", "),
    }
}

/// First non-empty output line of `<binary> <args>` — stdout, falling back
/// to stderr (usage text often lands there). `None` when unrunnable/empty.
fn first_line(binary: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(binary).args(args).output().ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let text = if stdout.trim().is_empty() {
        String::from_utf8_lossy(&out.stderr).into_owned()
    } else {
        stdout.into_owned()
    };
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_owned)
}

/// XDG config path via the settings resolver.
fn config_path() -> String {
    settings::config_dir()
        .map(|d| d.join("config.toml").display().to_string())
        .unwrap_or_else(|| "(unknown)".into())
}

#[cfg(test)]
mod tests {
    use super::super::{Cli, Cmd, TargetArg};
    use clap::Parser;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(args)
    }

    fn cmd(args: &[&str]) -> Cmd {
        parse(args).unwrap().cmd.unwrap()
    }

    #[test]
    fn start_target_is_optional() {
        match cmd(&["niri-recorder", "start"]) {
            Cmd::Start { target, output } => {
                assert!(target.is_none()); // config `default_target` applies
                assert!(output.is_none());
            }
            _ => panic!("expected start"),
        }
    }

    #[test]
    fn start_region_target() {
        match cmd(&["niri-recorder", "start", "--target", "region"]) {
            Cmd::Start { target, .. } => {
                assert!(matches!(target, Some(TargetArg::Region)))
            }
            _ => panic!("expected start"),
        }
    }

    #[test]
    fn start_output_name() {
        match cmd(&["niri-recorder", "start", "--output", "DP-3"]) {
            Cmd::Start { output, .. } => assert_eq!(output.as_deref(), Some("DP-3")),
            _ => panic!("expected start"),
        }
    }

    #[test]
    fn invalid_target_rejected() {
        assert!(parse(&["niri-recorder", "start", "--target", "bogus"]).is_err());
    }

    #[test]
    fn simple_subcommands_parse() {
        assert!(matches!(cmd(&["niri-recorder", "stop"]), Cmd::Stop));
        assert!(matches!(cmd(&["niri-recorder", "status"]), Cmd::Status));
        assert!(matches!(
            cmd(&["niri-recorder", "diagnostics"]),
            Cmd::Diagnostics
        ));
    }

    #[test]
    fn bare_invocation_has_no_subcommand() {
        assert!(parse(&["niri-recorder"]).unwrap().cmd.is_none());
    }

    /// A child that exited but wasn't waited on reads as zombie via /proc —
    /// the seam the poll loop uses, since kill(pid, 0) is Ok on zombies.
    #[test]
    fn is_zombie_detects_exited_unreaped_child() {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !super::is_zombie(pid) {
            assert!(
                std::time::Instant::now() < deadline,
                "child never reached zombie state"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        child.wait().unwrap(); // reap — also proves stop() can still reap
        assert!(!super::is_zombie(pid));
        assert!(!super::is_zombie(std::process::id())); // live process isn't Z
    }

    /// Spec §23.6: only auto/wf-recorder are accepted; anything else is an
    /// actionable unsupported-backend error naming the value.
    #[test]
    fn preferred_backend_validation() {
        let mut s = crate::config::settings::Settings::default();
        assert!(super::check_preferred_backend(&s).is_ok()); // auto
        s.preferred_backend = "wf-recorder".into();
        assert!(super::check_preferred_backend(&s).is_ok());
        s.preferred_backend = "wl-screenrec".into();
        let err = super::check_preferred_backend(&s).unwrap_err();
        assert!(err.contains("wl-screenrec"));
    }
}
