//! Recorder controller: drives the [`AppState`] machine over a
//! [`CaptureBackend`]. Selection geometry is kept here between
//! `SelectionDone` and `backend.start` because `AppState::Starting`
//! carries no payload. A start is only claimed once the child has been
//! reaped and the `.part` file assessed in [`Recorder::stop`].

use crate::app::state::{transition, AppState, Event};
use crate::capture::backend::{CaptureBackend, CaptureError, CaptureSession};
use crate::capture::process::spawn;
use crate::capture::request::{CaptureRequest, CaptureTarget, Geometry};
use crate::config::settings::{expand_tilde, Settings, Target};
use crate::files::naming::{finalize, plan_output, OutputPlan};
use nix::unistd::Pid;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Orchestrates one recording session over backend `B`.
///
/// Call surface: `start(target, output)` — `output` names the display to
/// capture (e.g. `"DP-1"`); `Some` bypasses output auto-resolution and is
/// ignored for `Target::Region`. `stop()` returns the finalized path.
pub struct Recorder<B: CaptureBackend> {
    state: AppState,
    backend: B,
    settings: Settings,
    session: Option<CaptureSession>,
    /// Optional SIGINT flag shared by the CLI; lets `start` abandon a
    /// region selection that was already cancelled mid-`slurp`.
    quit: Option<Arc<AtomicBool>>,
}

impl<B: CaptureBackend> Recorder<B> {
    pub fn new(backend: B, settings: Settings) -> Self {
        Self {
            state: AppState::Idle,
            backend,
            settings,
            session: None,
            quit: None,
        }
    }

    /// Wire the quit flag the CLI's signal handler sets; checked between
    /// region selection and spawn so a Ctrl-C during `slurp` cancels.
    pub fn set_quit_flag(&mut self, quit: Arc<AtomicBool>) {
        self.quit = Some(quit);
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// `.part` path of the in-flight capture; `None` when not recording.
    /// Read-only accessor so the CLI can report where video is going.
    pub fn part_path(&self) -> Option<&std::path::Path> {
        self.session.as_ref().map(|s| s.part_path.as_path())
    }

    /// Start a capture for `target` (display or slurp-selected region).
    /// Errors when not `Idle`; a cancelled region selection is `Ok` and
    /// leaves the state `Idle`.
    pub fn start(&mut self, target: Target, output: Option<String>) -> Result<(), CaptureError> {
        if self.state != AppState::Idle {
            return Err(CaptureError::Unsupported(format!(
                "cannot start while {:?}",
                self.state
            )));
        }
        let capture_target = match target {
            Target::Display => {
                let name = self.resolve_output(output)?;
                self.state = transition(self.state.clone(), Event::StartDisplay);
                CaptureTarget::Display(name)
            }
            Target::Region => {
                self.state = transition(self.state.clone(), Event::StartRegion);
                let geom = match self.select_region() {
                    Ok(Some(g)) => g,
                    // Esc/cancel or unparseable output — not an error.
                    Ok(None) => {
                        self.state = transition(self.state.clone(), Event::SelectionCancelled);
                        return Ok(());
                    }
                    Err(e) => {
                        self.state = transition(self.state.clone(), Event::SelectionCancelled);
                        return Err(e);
                    }
                };
                // A Ctrl-C during `slurp` must not produce a recording.
                if self.quit.as_ref().is_some_and(|q| q.load(Ordering::SeqCst)) {
                    self.state = transition(self.state.clone(), Event::SelectionCancelled);
                    return Ok(());
                }
                self.state = transition(self.state.clone(), Event::SelectionDone(geom));
                CaptureTarget::Region(geom)
            }
        };
        self.spawn_capture(capture_target)
    }

    /// Stop the running capture: SIGINT→TERM→KILL, reap, then finalize the
    /// `.part` file if non-empty. Returns the finalized path.
    pub fn stop(&mut self) -> Result<PathBuf, CaptureError> {
        let AppState::Recording { .. } = self.state else {
            return Err(CaptureError::Unsupported(format!(
                "cannot stop while {:?}",
                self.state
            )));
        };
        self.state = transition(self.state.clone(), Event::StopRequested);
        let mut session = self.session.take().expect("Recording implies session");
        let part_path = session.part_path.clone();
        let status = match session.child.stop_graceful() {
            Ok(s) => s,
            Err(e) => {
                // io error mid-stop — the child may still be alive and about to
                // become unreachable; best-effort SIGKILL + reap before failing.
                let pid = nix::unistd::Pid::from_raw(session.child.pid() as i32);
                let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
                let _ = session.child.child.wait();
                self.state = transition(
                    self.state.clone(),
                    Event::Exited {
                        ok: false,
                        path: part_path,
                    },
                );
                return Err(e);
            }
        };
        // Success is decided only after the child is reaped and the part
        // file assessed — a clean exit writing nothing is still a failure.
        let has_data = std::fs::metadata(&part_path)
            .map(|m| m.len() > 0)
            .unwrap_or(false);
        if !has_data {
            let _ = std::fs::remove_file(&part_path); // empty .part is litter
            self.state = transition(
                self.state.clone(),
                Event::Exited {
                    ok: false,
                    path: part_path,
                },
            );
            return Err(CaptureError::UnexpectedExit(session.child.stderr_tail()));
        }
        match finalize(OutputPlan {
            part_path: session.part_path.clone(),
            final_path: session.final_path.clone(),
        }) {
            Ok(path) => {
                if !status.success() {
                    // Non-zero exit but recoverable data: keep the finalized
                    // partial (FR-07) yet report the failure (§15).
                    let tail = session.child.stderr_tail();
                    let tail = if tail.is_empty() {
                        String::new()
                    } else {
                        format!("; stderr tail: {tail}")
                    };
                    self.state = transition(
                        self.state.clone(),
                        Event::Exited {
                            ok: false,
                            path: path.clone(),
                        },
                    );
                    return Err(CaptureError::UnexpectedExit(format!(
                        "{status}; kept recoverable partial at {}{tail}",
                        path.display()
                    )));
                }
                self.state = transition(
                    self.state.clone(),
                    Event::Exited {
                        ok: true,
                        path: path.clone(),
                    },
                );
                Ok(path)
            }
            Err(e) => {
                self.state = transition(
                    self.state.clone(),
                    Event::Exited {
                        ok: false,
                        path: part_path,
                    },
                );
                Err(CaptureError::Spawn(e))
            }
        }
    }

    /// Display name: the explicit `output`, or the sole probed output.
    fn resolve_output(&self, output: Option<String>) -> Result<String, CaptureError> {
        if let Some(name) = output {
            return Ok(name);
        }
        let mut outputs = self.backend.capabilities()?.outputs;
        match outputs.len() {
            1 => Ok(outputs.remove(0)),
            0 => Err(CaptureError::Unsupported("no display detected".into())),
            _ => Err(CaptureError::Unsupported(
                "multiple displays; --output required".into(),
            )),
        }
    }

    /// Run `slurp` to completion and return its stdout geometry.
    /// `Ok(None)` = user cancelled (non-zero exit or unparseable output).
    /// A set `quit` flag kills slurp so close-while-selecting can't
    /// deadlock the worker on a crosshair left on screen.
    fn select_region(&mut self) -> Result<Option<Geometry>, CaptureError> {
        let mut child = match spawn(&["slurp"], &[]) {
            Ok(c) => c,
            Err(CaptureError::Spawn(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(CaptureError::MissingTool("slurp", "sudo dnf install slurp"));
            }
            Err(e) => return Err(e),
        };
        // ponytail: pid-reuse window — after wait() reaps slurp the
        // watcher can hold its dead pid for ≤50 ms and a quit landing in
        // that window would SIGKILL a recycled process. Accepted: the
        // quit flag is user-driven (window close), so the odds of it
        // arming in the exact ~50 ms after slurp exits *and* the pid
        // being recycled are negligible; a generation-checked kill would
        // double the code. Watcher exits on its own next tick.
        let watcher = self.quit.as_ref().map(|quit| {
            let quit = Arc::clone(quit);
            let pid = Pid::from_raw(child.pid() as i32);
            std::thread::spawn(move || loop {
                if quit.load(Ordering::SeqCst) {
                    let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
                    break;
                }
                if nix::sys::signal::kill(pid, None).is_err() {
                    break; // slurp already gone
                }
                std::thread::sleep(Duration::from_millis(50));
            })
        });
        // Read stdout BEFORE waiting: a full pipe would deadlock slurp.
        let mut out = String::new();
        if let Some(stdout) = child.child.stdout.as_mut() {
            let _ = stdout.read_to_string(&mut out);
        }
        let status = child.child.wait().map_err(CaptureError::Spawn)?;
        if let Some(h) = watcher {
            let _ = h.join();
        }
        if !status.success() {
            return Ok(None);
        }
        Ok(Geometry::parse_slurp(&out).ok())
    }

    /// Plan the output file and spawn the backend; on success the state is
    /// `Recording` and the session is stored.
    fn spawn_capture(&mut self, target: CaptureTarget) -> Result<(), CaptureError> {
        let dir = expand_tilde(&self.settings.output_directory);
        let plan = match plan_output(&dir, "mp4") {
            Ok(p) => p,
            Err(e) => {
                self.state = transition(
                    self.state.clone(),
                    Event::SpawnFailed(format!("cannot plan output: {e}")),
                );
                return Err(CaptureError::Spawn(e));
            }
        };
        let req = CaptureRequest {
            target,
            fps: self.settings.default_fps,
            quality: self.settings.quality,
            part_path: plan.part_path,
        };
        match self.backend.start(req) {
            Ok(session) => {
                self.state = transition(self.state.clone(), Event::Spawned(session.child.pid()));
                self.session = Some(session);
                Ok(())
            }
            Err(e) => {
                self.state = transition(self.state.clone(), Event::SpawnFailed(e.to_string()));
                Err(e)
            }
        }
    }
}
