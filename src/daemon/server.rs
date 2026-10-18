//! Background capture daemon: `niri-recorder daemon`.
//!
//! The process stays in the foreground (run it from a systemd user unit)
//! and owns capture for every forwarded request. A Unix socket accepts
//! one newline-JSON request per connection; capture itself runs on a
//! dedicated worker thread so `status`/`stop` stay responsive even while
//! a region selection is blocking on `slurp`.
//!
//! The pidfile contract is unchanged: whichever process records owns it,
//! so `stop` still works against a daemon that died (stale pidfile) and
//! the GUI correctly shows "controlled by another session".

use super::protocol::{encode_response, parse_request, Request, Response};
use crate::app::controller::Recorder;
use crate::app::pidfile::{clear_pidfile, write_pidfile};
use crate::app::state::AppState;
use crate::capture::backends::wf_recorder::WfRecorder;
use crate::capture::process::is_zombie;
use crate::cli::commands::check_preferred_backend;
use crate::config::settings::{self, Quality, Target};
use nix::sys::signal::kill;
use nix::unistd::Pid;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// What `status` reports without touching the worker.
#[derive(Debug, Clone)]
struct Snapshot {
    state: String,
    pid: Option<u32>,
    part: Option<String>,
}

impl Snapshot {
    fn idle() -> Self {
        Self {
            state: "idle".into(),
            pid: None,
            part: None,
        }
    }
    fn failed() -> Self {
        Self {
            state: "failed".into(),
            ..Self::idle()
        }
    }
}

type Shared = Arc<Mutex<Snapshot>>;

enum Command {
    Start {
        target: Target,
        output: Option<String>,
        fps: u32,
        quality: Quality,
        notify: bool,
        reply: mpsc::Sender<Response>,
    },
    Stop {
        reply: mpsc::Sender<Response>,
    },
    Quit,
}

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Snapshot> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

fn publish(shared: &Shared, snapshot: Snapshot) {
    *lock(shared) = snapshot;
}

fn snapshot_response(snapshot: &Snapshot) -> Response {
    match (snapshot.state.as_str(), snapshot.pid) {
        ("recording", Some(pid)) => Response::recording(pid, snapshot.part.clone()),
        (state, _) => Response {
            ok: state != "failed",
            state: state.to_string(),
            ..Response::idle()
        },
    }
}

/// Run the daemon until SIGINT/SIGTERM/SIGHUP/SIGQUIT. Never returns.
pub fn run() -> ! {
    let Some(path) = super::client::socket_path() else {
        eprintln!("niri-recorder: cannot determine runtime directory for the control socket");
        std::process::exit(3);
    };
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut handlers_ok = ctrlc::set_handler({
        let shutdown = Arc::clone(&shutdown);
        move || shutdown.store(true, Ordering::SeqCst)
    })
    .is_ok();
    for sig in [
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
        signal_hook::consts::SIGQUIT,
    ] {
        handlers_ok &= signal_hook::flag::register(sig, Arc::clone(&shutdown)).is_ok();
    }
    if !handlers_ok {
        eprintln!("niri-recorder: cannot install daemon signal handlers");
        std::process::exit(3);
    }
    println!("niri-recorder daemon listening on {}", path.display());
    match serve_at(&path, shutdown) {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("niri-recorder: {e}");
            std::process::exit(3);
        }
    }
}

/// Serve on `path` until `shutdown` is set. Exposed for tests; removes
/// the socket file on exit. A second daemon gets an error instead of
/// stealing the socket.
pub fn serve_at(path: &Path, shutdown: Arc<AtomicBool>) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("invalid socket path {}", path.display()))?;
    prepare_private_dir(dir)?;

    if path.exists() {
        match UnixStream::connect(path) {
            Ok(_) => return Err(format!("daemon already running on {}", path.display())),
            Err(_) => {
                let _ = fs::remove_file(path);
            }
        }
    }
    let listener =
        UnixListener::bind(path).map_err(|e| format!("cannot bind {}: {e}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("cannot secure {}: {e}", path.display()))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("cannot poll {}: {e}", path.display()))?;

    let (tx, rx) = mpsc::channel::<Command>();
    let shared: Shared = Arc::new(Mutex::new(Snapshot::idle()));
    let cancel = Arc::new(AtomicBool::new(false));
    let worker = std::thread::spawn({
        let shared = Arc::clone(&shared);
        let cancel = Arc::clone(&cancel);
        let shutdown = Arc::clone(&shutdown);
        move || worker(rx, shared, cancel, shutdown)
    });

    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let tx = tx.clone();
                let shared = Arc::clone(&shared);
                let cancel = Arc::clone(&cancel);
                std::thread::spawn(move || handle_connection(stream, tx, shared, cancel));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }

    // A region selection parked in slurp must not keep shutdown waiting.
    cancel.store(true, Ordering::SeqCst);
    let _ = tx.send(Command::Quit);
    let _ = worker.join();
    let _ = fs::remove_file(path);
    Ok(())
}

/// The socket lives next to the pidfile: same private, user-owned dir.
fn prepare_private_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let meta = fs::metadata(dir).map_err(|e| format!("cannot stat {}: {e}", dir.display()))?;
    let uid = fs::metadata("/proc/self")
        .map(|m| m.uid())
        .unwrap_or(u32::MAX);
    if meta.uid() != uid {
        return Err(format!(
            "refusing foreign-owned runtime dir {}",
            dir.display()
        ));
    }
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("cannot secure {}: {e}", dir.display()))?;
    Ok(())
}

fn handle_connection(
    mut stream: UnixStream,
    tx: mpsc::Sender<Command>,
    shared: Shared,
    cancel: Arc<AtomicBool>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let Ok(clone) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(clone);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
        return;
    }

    let response = match parse_request(&line) {
        Err(e) => Response::error(2, e),
        Ok(Request::Status) => snapshot_response(&lock(&shared)),
        Ok(Request::Start {
            target,
            output,
            fps,
            quality,
            notify,
        }) => {
            let settings = settings::load();
            let (reply, rx) = mpsc::channel();
            let cmd = Command::Start {
                target: target.unwrap_or(settings.default_target),
                output,
                fps: fps.unwrap_or(settings.default_fps),
                quality: quality.unwrap_or(settings.quality),
                notify: notify.unwrap_or(settings.notifications),
                reply,
            };
            if tx.send(cmd).is_err() {
                Response::error(3, "daemon is shutting down")
            } else {
                // No timeout: a region start legitimately waits for the
                // user's drag, matching foreground `start` semantics.
                rx.recv()
                    .unwrap_or_else(|_| Response::error(3, "daemon stopped"))
            }
        }
        Ok(Request::Stop) => {
            // Also aborts a slurp: the worker may be inside Recorder::start.
            cancel.store(true, Ordering::SeqCst);
            let (reply, rx) = mpsc::channel();
            if tx.send(Command::Stop { reply }).is_err() {
                Response::error(3, "daemon is shutting down")
            } else {
                rx.recv_timeout(Duration::from_secs(30))
                    .unwrap_or_else(|_| Response::error(3, "daemon timed out stopping"))
            }
        }
    };

    let _ = writeln!(stream, "{}", encode_response(&response));
    let _ = stream.flush();
}

/// Capture worker: owns the `Recorder`, the pidfile claim and the
/// liveness polling. Commands are serialized here so two starts can
/// never race.
fn worker(
    rx: mpsc::Receiver<Command>,
    shared: Shared,
    cancel: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) {
    let mut recorder: Option<Recorder<WfRecorder>> = None;
    let mut owns_pidfile = false;
    let mut notify_enabled = false;
    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        let recording = recorder
            .as_ref()
            .is_some_and(|r| matches!(r.state(), AppState::Recording { .. }));
        let cmd = if recording {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(cmd) => Some(cmd),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(cmd) => Some(cmd),
                Err(mpsc::RecvError) => break,
            }
        };

        match cmd {
            Some(Command::Start {
                target,
                output,
                fps,
                quality,
                notify,
                reply,
            }) => {
                let response = if shutdown.load(Ordering::SeqCst) {
                    Response::error(3, "daemon is shutting down")
                } else if recorder.is_some() {
                    Response::error(1, "already recording")
                } else {
                    notify_enabled = notify;
                    start_capture(
                        &shared,
                        &cancel,
                        &mut owns_pidfile,
                        &mut recorder,
                        target,
                        output,
                        fps,
                        quality,
                        notify,
                    )
                };
                let _ = reply.send(response);
            }
            Some(Command::Stop { reply }) => {
                let response = match recorder.take() {
                    Some(mut rec) => {
                        let child_died = match rec.state() {
                            AppState::Recording { pid } => !child_alive(*pid),
                            _ => false,
                        };
                        stop_capture(
                            &shared,
                            &mut rec,
                            &mut owns_pidfile,
                            child_died,
                            notify_enabled,
                        )
                    }
                    None => Response::error(1, "not recording"),
                };
                let _ = reply.send(response);
            }
            Some(Command::Quit) => break,
            None => {
                // Recording tick: finalize a child that died on its own.
                let dead = recorder
                    .as_ref()
                    .and_then(|rec| match rec.state() {
                        AppState::Recording { pid } => Some(*pid),
                        _ => None,
                    })
                    .filter(|pid| !child_alive(*pid));
                if dead.is_some() {
                    let mut rec = recorder.take().expect("checked above");
                    let resp =
                        stop_capture(&shared, &mut rec, &mut owns_pidfile, true, notify_enabled);
                    if let Some(err) = resp.error {
                        tracing::warn!(error = %err, "daemon capture ended early");
                    }
                }
            }
        }
    }

    if let Some(mut rec) = recorder.take() {
        if matches!(rec.state(), AppState::Recording { .. }) {
            let _ = rec.stop();
        }
    }
    if owns_pidfile {
        clear_pidfile();
    }
}

/// Claim pidfile → detect backend → `Recorder::start`. Mirrors
/// `cli::commands::start` so daemon and foreground behavior stay aligned.
#[allow(clippy::too_many_arguments)]
fn start_capture(
    shared: &Shared,
    cancel: &Arc<AtomicBool>,
    owns_pidfile: &mut bool,
    recorder: &mut Option<Recorder<WfRecorder>>,
    target: Target,
    output: Option<String>,
    fps: u32,
    quality: Quality,
    notify: bool,
) -> Response {
    let mut settings = settings::load();
    if let Err(msg) = check_preferred_backend(&settings) {
        return Response::error(2, msg);
    }
    settings.default_fps = if matches!(fps, 24 | 30 | 60) {
        fps
    } else {
        settings.default_fps
    };
    settings.quality = quality;
    // FR-09 parity: remember an explicitly named display for later bare
    // starts (keybinds/DMS) on multi-monitor setups.
    if let Some(name) = &output {
        if settings.last_output.as_deref() != Some(name.as_str()) {
            settings.last_output = Some(name.clone());
            if let Err(e) = settings::save(&settings) {
                tracing::warn!(error = %e, "cannot persist last_output");
            }
        }
    }

    if let Err(e) = write_pidfile() {
        let code = if e.kind() == std::io::ErrorKind::AlreadyExists {
            1
        } else {
            3
        };
        return Response::error(code, e.to_string());
    }
    *owns_pidfile = true;
    let backend = match WfRecorder::detect() {
        Ok(backend) => backend,
        Err(e) => {
            release_pidfile(owns_pidfile);
            return Response::error(2, e.to_string());
        }
    };
    let mut rec = Recorder::new(backend, settings);
    // A stale cancel flag would abort a fresh region selection.
    cancel.store(false, Ordering::SeqCst);
    rec.set_quit_flag(Arc::clone(cancel));
    publish(
        shared,
        Snapshot {
            state: "starting".into(),
            ..Snapshot::idle()
        },
    );

    match rec.start(target, output.clone()) {
        Ok(()) => match rec.state() {
            AppState::Recording { pid } => {
                let pid = *pid;
                let part = rec.part_path().map(|p| p.display().to_string());
                publish(
                    shared,
                    Snapshot {
                        state: "recording".into(),
                        pid: Some(pid),
                        part: part.clone(),
                    },
                );
                let target_desc = match target {
                    Target::Region => "region",
                    Target::Display => output.as_deref().unwrap_or("display"),
                };
                crate::app::notify::notify(
                    notify,
                    "Recording started",
                    &format!("Capturing {target_desc}"),
                );
                *recorder = Some(rec);
                Response::recording(pid, part)
            }
            _ => {
                release_pidfile(owns_pidfile);
                publish(shared, Snapshot::idle());
                Response::idle()
            }
        },
        Err(e) => {
            release_pidfile(owns_pidfile);
            publish(shared, Snapshot::failed());
            Response::error(3, e.to_string())
        }
    }
}

fn stop_capture(
    shared: &Shared,
    rec: &mut Recorder<WfRecorder>,
    owns_pidfile: &mut bool,
    child_died: bool,
    notify: bool,
) -> Response {
    if let AppState::Recording { pid } = rec.state() {
        publish(
            shared,
            Snapshot {
                state: "stopping".into(),
                pid: Some(*pid),
                part: None,
            },
        );
    }
    match (rec.stop(), child_died) {
        (Ok(path), false) => {
            // Same retention hook as the CLI/GUI save paths.
            crate::files::retention::apply_for_settings(&settings::load());
            release_pidfile(owns_pidfile);
            publish(shared, Snapshot::idle());
            crate::app::notify::notify(notify, "Recording saved", &path.display().to_string());
            Response::saved(&path)
        }
        (Ok(path), true) => {
            release_pidfile(owns_pidfile);
            publish(shared, Snapshot::failed());
            let msg = format!("capture ended early; saved {}", path.display());
            crate::app::notify::notify(notify, "Recording ended early", &msg);
            Response::error(3, msg)
        }
        (Err(e), _) => {
            release_pidfile(owns_pidfile);
            publish(shared, Snapshot::failed());
            let msg = format!("stop failed: {e}");
            crate::app::notify::notify(notify, "Recording failed", &msg);
            Response::error(3, msg)
        }
    }
}

fn release_pidfile(owns: &mut bool) {
    if *owns {
        clear_pidfile();
        *owns = false;
    }
}

fn child_alive(pid: u32) -> bool {
    // kill(pid, 0) is Ok on zombies, so check /proc too.
    !is_zombie(pid) && kill(Pid::from_raw(pid as i32), None).is_ok()
}
