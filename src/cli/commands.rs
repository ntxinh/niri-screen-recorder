//! Command implementations. Handlers needing a specific exit code print
//! their message and call `std::process::exit` — `run` never returns.
//!
//! `stop`/`status` operate on the pidfile alone: the recording process is
//! whichever session claimed it — a `start` invocation or the GUI —
//! signalled with SIGINT.

use super::TargetArg;
use crate::app::controller::Recorder;
use crate::app::pidfile::{clear_pidfile, read_pidfile, write_pidfile};
use crate::app::state::AppState;
use crate::capture::backend::CaptureBackend;
use crate::capture::backends::wf_recorder::WfRecorder;
use crate::capture::process::is_zombie;
use crate::config::settings::{self, Settings, Target};
use crate::daemon::client;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Exit codes: 0 ok, 1 already recording / not recording, 2 backend
/// missing or unsupported, 3 capture failure.
pub(super) fn start(
    target_arg: Option<TargetArg>,
    output: Option<String>,
    notify_flag: bool,
    no_notify_flag: bool,
) -> ! {
    let settings = settings::load();
    let notify_enabled = super::resolve_notify(notify_flag, no_notify_flag, settings.notifications);
    if let Err(msg) = check_preferred_backend(&settings) {
        eprintln!("niri-recorder: {msg}");
        crate::app::notify::notify(notify_enabled, "Recording failed", &msg);
        std::process::exit(2);
    }
    let target = target_arg.map_or(settings.default_target, Into::into);
    // FR-09 parity with the GUI: a start naming a display remembers it for
    // subsequent bare starts (keybinds, DMS widget) on multi-monitor setups.
    if let Some(name) = &output {
        if settings.last_output.as_deref() != Some(name.as_str()) {
            let mut s = settings.clone();
            s.last_output = Some(name.clone());
            if let Err(e) = settings::save(&s) {
                eprintln!("niri-recorder: cannot save settings: {e}");
            }
        }
    }
    // A running daemon owns capture: forward and exit instead of
    // starting a second recorder. `None` = no daemon, use the pidfile.
    if let Some(result) = client::try_start(
        target,
        output.clone(),
        settings.default_fps,
        settings.quality,
        Some(notify_enabled),
    ) {
        match result {
            Ok(resp) if resp.ok && resp.state == "recording" => {
                match (&resp.part, resp.pid) {
                    (Some(part), _) => println!("recording → {part}"),
                    (_, Some(pid)) => println!("recording pid={pid}"),
                    _ => println!("recording"),
                }
                std::process::exit(0);
            }
            // Ok without Recording = cancelled region selection.
            Ok(resp) if resp.ok => {
                println!("cancelled");
                std::process::exit(0);
            }
            Ok(resp) => {
                let msg = resp
                    .error
                    .unwrap_or_else(|| "daemon refused the start".into());
                eprintln!("niri-recorder: {msg}");
                crate::app::notify::notify(notify_enabled, "Recording failed", &msg);
                std::process::exit(i32::from(resp.code.max(1)));
            }
            Err(e) => {
                let msg = e.to_string();
                eprintln!("niri-recorder: {msg}");
                crate::app::notify::notify(notify_enabled, "Recording failed", &msg);
                std::process::exit(3);
            }
        }
    }
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
    // SIGINT (Ctrl-C or `niri-recorder stop`) flips the flag; the stop
    // itself runs on this thread so Recorder stays single-threaded.
    // SIGTERM/SIGHUP/SIGQUIT flip the same flag — a kill or session
    // teardown still finalizes the .part instead of abandoning it.
    let quit = Arc::new(AtomicBool::new(false));
    let mut handlers_ok = ctrlc::set_handler({
        let quit = Arc::clone(&quit);
        move || quit.store(true, Ordering::SeqCst)
    })
    .is_ok();
    for sig in [
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
        signal_hook::consts::SIGQUIT,
    ] {
        handlers_ok &= signal_hook::flag::register(sig, Arc::clone(&quit)).is_ok();
    }
    if !handlers_ok {
        eprintln!("niri-recorder: cannot install signal handler");
        clear_pidfile();
        std::process::exit(3);
    }

    let mut recorder = Recorder::new(backend, settings.clone());
    recorder.set_quit_flag(Arc::clone(&quit));
    if let Err(e) = recorder.start(target, output.clone()) {
        let msg = e.to_string();
        eprintln!("niri-recorder: {msg}");
        crate::app::notify::notify(notify_enabled, "Recording failed", &msg);
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
        let msg = format!("unexpected state {:?}", recorder.state());
        eprintln!("niri-recorder: {msg}");
        crate::app::notify::notify(notify_enabled, "Recording failed", &msg);
        clear_pidfile();
        std::process::exit(3);
    };
    match recorder.part_path() {
        Some(p) => println!("recording → {}", p.display()),
        None => println!("recording pid={pid}"),
    }
    let target_desc = match target {
        Target::Region => "region",
        Target::Display => output.as_deref().unwrap_or("display"),
    };
    crate::app::notify::notify(
        notify_enabled,
        "Recording started",
        &format!("Capturing {target_desc}"),
    );
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
            crate::app::notify::notify(
                notify_enabled,
                "Recording saved",
                &path.display().to_string(),
            );
            crate::files::retention::apply_for_settings(&settings);
            clear_pidfile();
            std::process::exit(0);
        }
        (Ok(path), true) => {
            let msg = format!("capture ended early; saved {}", path.display());
            eprintln!("niri-recorder: {msg}");
            crate::app::notify::notify(notify_enabled, "Recording ended early", &msg);
            clear_pidfile();
            std::process::exit(3);
        }
        (Err(e), _) => {
            let msg = format!("stop failed: {e}");
            eprintln!("niri-recorder: {msg}");
            crate::app::notify::notify(notify_enabled, "Recording failed", &msg);
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
pub(super) fn stop(notify_flag: bool, no_notify_flag: bool) -> ! {
    let settings = settings::load();
    let notify_enabled = super::resolve_notify(notify_flag, no_notify_flag, settings.notifications);
    if let Some(result) = client::try_stop() {
        match result {
            Ok(resp) if resp.ok => {
                match resp.saved {
                    Some(path) => println!("saved → {path}"),
                    None => println!("stop requested (daemon)"),
                }
                std::process::exit(0);
            }
            // Code 1 = the daemon is idle. The pidfile may still belong
            // to a GUI/foreground recorder, so fall through.
            Ok(resp) if resp.code == 1 => {}
            Ok(resp) => {
                let msg = resp
                    .error
                    .unwrap_or_else(|| "daemon could not stop the capture".into());
                eprintln!("niri-recorder: {msg}");
                std::process::exit(i32::from(resp.code.max(1)));
            }
            Err(e) => {
                eprintln!("niri-recorder: {e}");
                std::process::exit(3);
            }
        }
    }
    match read_pidfile() {
        Some(e) => match kill(Pid::from_raw(e.pid as i32), Signal::SIGINT) {
            Ok(()) => {
                println!("stop requested (pid {})", e.pid);
                std::process::exit(0);
            }
            Err(err) => {
                let msg = format!("cannot signal pid {}: {err}", e.pid);
                eprintln!("niri-recorder: {msg}");
                crate::app::notify::notify(notify_enabled, "Stop failed", &msg);
                std::process::exit(1);
            }
        },
        None => {
            println!("not recording");
            if notify_flag {
                crate::app::notify::notify(true, "niri-recorder", "Not recording");
            }
            std::process::exit(1);
        }
    }
}

/// Print whether a recording is in progress (daemon, then pidfile).
pub(super) fn status() -> ! {
    if let Some(result) = client::try_status() {
        match result {
            Ok(resp) if resp.state == "recording" => {
                println!("recording pid={}", resp.pid.unwrap_or(0));
                std::process::exit(0);
            }
            // Daemon running but idle; a GUI/foreground recorder may still
            // own the pidfile.
            Ok(_) => {}
            Err(e) => {
                eprintln!("niri-recorder: {e}");
                std::process::exit(3);
            }
        }
    }
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
    println!(
        "daemon socket: {}",
        client::socket_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(unknown)".into())
    );
    match client::try_status() {
        Some(Ok(r)) => println!("daemon: running (state={})", r.state),
        Some(Err(e)) => println!("daemon: error ({e})"),
        None => println!("daemon: not running"),
    }
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
            Cmd::Start { target, output, .. } => {
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
        assert!(matches!(cmd(&["niri-recorder", "stop"]), Cmd::Stop { .. }));
        assert!(matches!(
            cmd(&["niri-recorder", "diagnostics"]),
            Cmd::Diagnostics
        ));
        assert!(matches!(cmd(&["niri-recorder", "daemon"]), Cmd::Daemon));
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

    #[test]
    fn notify_flags_override_precedence() {
        match cmd(&["niri-recorder", "start", "--notify"]) {
            Cmd::Start {
                notify, no_notify, ..
            } => {
                assert!(notify);
                assert!(!no_notify);
                assert!(super::super::resolve_notify(notify, no_notify, false));
            }
            _ => panic!("expected start"),
        }
        match cmd(&["niri-recorder", "start", "--no-notify"]) {
            Cmd::Start {
                notify, no_notify, ..
            } => {
                assert!(!notify);
                assert!(no_notify);
                assert!(!super::super::resolve_notify(notify, no_notify, true));
            }
            _ => panic!("expected start"),
        }
        assert!(super::super::resolve_notify(false, false, true));
        assert!(!super::super::resolve_notify(false, false, false));
    }
}
