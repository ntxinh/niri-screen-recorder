//! `GuiController`: owns the recorder worker thread and the two channels
//! that cross the thread boundary. Every blocking `Recorder` call (slurp
//! selection, child wait/finalize) happens on the worker — never on the
//! GTK main thread.
//!
//! Events travel worker→main over `async_channel`, pumped by
//! `glib::spawn_future_local` in `app.rs` (`glib::MainContext::channel`
//! was removed in glib-rs 0.22).

use crate::app::controller::Recorder;
use crate::app::pidfile::{clear_pidfile, read_pidfile, write_pidfile};
use crate::app::state::AppState;
use crate::capture::backend::CaptureBackend;
use crate::capture::backends::wf_recorder::WfRecorder;
use crate::capture::process::is_zombie;
use crate::config::settings::{Quality, Settings, Target};
use nix::sys::signal::kill;
use nix::unistd::Pid;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

/// main → worker commands.
pub enum UiCommand {
    /// Start a capture; `output` names the display (ignored for Region).
    Start {
        target: Target,
        output: Option<String>,
        fps: u32,
        quality: Quality,
    },
    /// Stop the running capture (also sent by the SIGINT handler, so a
    /// `niri-recorder stop` keybinding can drive the GUI).
    Stop,
    /// Worker exits; on Drop. Blocks until it finishes any in-flight stop.
    Quit,
}

/// worker → main events. `glib::MainContext::channel` no longer exists in
/// glib-rs 0.22, so the sink is an `async_channel::Sender` drained by a
/// `spawn_future_local` loop on the main thread.
#[derive(Debug)]
pub enum UiEvent {
    /// Backend probe finished on the worker; capabilities for the
    /// display dropdown. Emitted once at startup, off the main thread.
    BackendReady(crate::capture::capabilities::CaptureCapabilities),
    /// Backend probe failed — recording can't start.
    BackendError(String),
    StateChanged(AppState),
    /// Recording is live; carries the in-flight `.part` path.
    RecordingStarted {
        part_path: Option<PathBuf>,
    },
    Completed(PathBuf),
    Failed(String),
}

pub struct GuiController {
    tx: mpsc::Sender<UiCommand>,
    /// Set by the process SIGINT handler; passed to `Recorder` so Ctrl-C
    /// during `slurp` cancels the pending region capture.
    quit: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl GuiController {
    /// Spawn the worker thread. `settings` is a snapshot; per-Start
    /// overrides (fps/quality) ride on the command. `tx_events` must be
    /// drained on the GTK main thread.
    pub fn spawn(settings: Settings, tx_events: async_channel::Sender<UiEvent>) -> Self {
        let (tx, rx) = mpsc::channel::<UiCommand>();
        let quit = Arc::new(AtomicBool::new(false));
        let worker = thread::spawn({
            let quit = Arc::clone(&quit);
            move || worker_loop(rx, tx_events, settings, quit)
        });
        Self {
            tx,
            quit,
            worker: Some(worker),
        }
    }

    /// Sender for the process-level SIGINT handler (`niri-recorder stop`
    /// signals this process the same as the CLI).
    pub fn command_sender(&self) -> mpsc::Sender<UiCommand> {
        self.tx.clone()
    }

    /// Flag consumed by `Recorder::set_quit_flag` on the worker.
    pub fn quit_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.quit)
    }

    pub fn request_start(
        &self,
        target: Target,
        output: Option<String>,
        fps: u32,
        quality: Quality,
    ) {
        let _ = self.tx.send(UiCommand::Start {
            target,
            output,
            fps,
            quality,
        });
    }

    pub fn request_stop(&self) {
        let _ = self.tx.send(UiCommand::Stop);
    }
}

impl Drop for GuiController {
    /// Stops an in-flight recording (finalizes the `.part`) before the
    /// worker exits. May block briefly while wf-recorder drains — the
    /// close-while-recording prompt lands in Task 4 and normally keeps
    /// this path unreachable.
    fn drop(&mut self) {
        let _ = self.tx.send(UiCommand::Quit);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}

/// Remove the pidfile only when this worker wrote it — a foreign
/// recorder's claim must survive GUI exit (the CLI can still stop it).
fn release_pidfile(own: &mut bool) {
    if *own {
        clear_pidfile();
        *own = false;
    }
}

fn emit(tx: &async_channel::Sender<UiEvent>, ev: UiEvent) {
    let _ = tx.send_blocking(ev);
}
fn worker_loop(
    rx: mpsc::Receiver<UiCommand>,
    tx: async_channel::Sender<UiEvent>,
    settings: Settings,
    quit: Arc<AtomicBool>,
) {
    let mut recorder: Option<Recorder<WfRecorder>> = None;
    let mut we_own_pidfile = false;
    // Backend probe (subprocess spawns: wf-recorder --help, slurp --help,
    // niri msg outputs) runs here, off the GTK main thread.
    match WfRecorder::detect() {
        Ok(b) => match b.capabilities() {
            Ok(c) => emit(&tx, UiEvent::BackendReady(c)),
            Err(e) => emit(&tx, UiEvent::BackendError(e.to_string())),
        },
        Err(e) => emit(&tx, UiEvent::BackendError(e.to_string())),
    }
    // ponytail: recording polls child liveness every 100ms; Idle blocks.
    loop {
        let cmd = if let Some(rec) = &recorder {
            let timeout = match rec.state() {
                AppState::Recording { .. } => Duration::from_millis(100),
                _ => Duration::MAX,
            };
            match rx.recv_timeout(timeout) {
                Ok(c) => Some(c),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(c) => Some(c),
                Err(mpsc::RecvError) => break,
            }
        };

        match cmd {
            Some(UiCommand::Start {
                target,
                output,
                fps,
                quality,
            }) => {
                if recorder.is_some() {
                    continue; // transitional state — UI already gates this
                }
                recorder = start_recording(
                    &tx,
                    &settings,
                    &quit,
                    &mut we_own_pidfile,
                    target,
                    output,
                    fps,
                    quality,
                );
            }
            Some(UiCommand::Stop) => {
                if let Some(mut rec) = recorder.take() {
                    stop_recording(&tx, &mut rec, false, &mut we_own_pidfile);
                }
            }
            Some(UiCommand::Quit) => break,
            None => {
                // Recording tick: is the wf-recorder child still alive?
                // kill(pid, 0) succeeds on zombies, so check /proc too.
                let Some(rec) = &recorder else { continue };
                let AppState::Recording { pid } = rec.state() else {
                    continue;
                };
                if is_zombie(*pid) || kill(Pid::from_raw(*pid as i32), None).is_err() {
                    let mut rec = recorder.take().expect("checked above");
                    stop_recording(&tx, &mut rec, true, &mut we_own_pidfile);
                }
            }
        }
    }
    if let Some(mut rec) = recorder.take() {
        if matches!(rec.state(), AppState::Recording { .. }) {
            stop_recording(&tx, &mut rec, false, &mut we_own_pidfile);
        }
    }
    release_pidfile(&mut we_own_pidfile);
}

/// Mirror of `cli::commands::start`: pidfile claim → backend detect →
/// Recorder::start. Settings fps/quality are baked into a fresh Recorder
/// per start so the dropdowns' values take effect.
// Arg list mirrors UiCommand::Start + shared worker state; a param
// struct would just rename the same eight values.
#[allow(clippy::too_many_arguments)]
fn start_recording(
    tx: &async_channel::Sender<UiEvent>,
    settings: &Settings,
    quit: &Arc<AtomicBool>,
    we_own_pidfile: &mut bool,
    target: Target,
    output: Option<String>,
    fps: u32,
    quality: Quality,
) -> Option<Recorder<WfRecorder>> {
    if let Some(e) = read_pidfile() {
        emit(
            tx,
            UiEvent::Failed(format!("already recording (pid {})", e.pid)),
        );
        return None;
    }
    if let Err(e) = write_pidfile() {
        emit(tx, UiEvent::Failed(format!("cannot claim recorder: {e}")));
        return None;
    }
    *we_own_pidfile = true;
    let backend = match WfRecorder::detect() {
        Ok(b) => b,
        Err(e) => {
            release_pidfile(we_own_pidfile);
            emit(tx, UiEvent::Failed(e.to_string()));
            return None;
        }
    };
    let mut s = settings.clone();
    s.default_fps = fps;
    s.quality = quality;
    let mut rec = Recorder::new(backend, s);
    // A SIGINT stale from a previous session would cancel a fresh region
    // selection before it even appears.
    quit.store(false, Ordering::SeqCst);
    rec.set_quit_flag(Arc::clone(quit));

    emit(
        tx,
        UiEvent::StateChanged(match target {
            Target::Display => AppState::Starting,
            Target::Region => AppState::SelectingRegion,
        }),
    );
    match rec.start(target, output) {
        Ok(()) => {
            let state = rec.state().clone();
            match &state {
                AppState::Recording { .. } => {
                    emit(
                        tx,
                        UiEvent::RecordingStarted {
                            part_path: rec.part_path().map(|p| p.to_path_buf()),
                        },
                    );
                    Some(rec)
                }
                // Cancelled region selection lands back on Idle.
                _ => {
                    release_pidfile(we_own_pidfile);
                    emit(tx, UiEvent::StateChanged(state));
                    None
                }
            }
        }
        Err(e) => {
            release_pidfile(we_own_pidfile);
            emit(tx, UiEvent::Failed(e.to_string()));
            None
        }
    }
}

/// Mirror of `cli::commands::start`'s stop block: clear the pidfile only
/// after the child is reaped, and report an early child death as a
/// capture failure even when the partial was finalized.
fn stop_recording(
    tx: &async_channel::Sender<UiEvent>,
    rec: &mut Recorder<WfRecorder>,
    child_died: bool,
    we_own_pidfile: &mut bool,
) {
    if let AppState::Recording { pid } = rec.state() {
        emit(tx, UiEvent::StateChanged(AppState::Stopping { pid: *pid }));
    }
    match (rec.stop(), child_died) {
        (Ok(path), false) => {
            release_pidfile(we_own_pidfile);
            emit(tx, UiEvent::Completed(path));
        }
        (Ok(path), true) => {
            release_pidfile(we_own_pidfile);
            emit(
                tx,
                UiEvent::Failed(format!("capture ended early; saved {}", path.display())),
            );
        }
        (Err(e), _) => {
            release_pidfile(we_own_pidfile);
            emit(tx, UiEvent::Failed(format!("stop failed: {e}")));
        }
    }
}
