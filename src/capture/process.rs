//! Managed capture subprocess: stderr drained on a thread into a bounded
//! ring so the child never blocks on a full pipe, and a bounded stop
//! sequence (SIGINT → SIGTERM → SIGKILL) that always reaps the child.

use crate::capture::backend::CaptureError;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

const STDERR_RING_CAP: usize = 50;
const POLL_TICK: Duration = Duration::from_millis(50);
const SIGINT_TIMEOUT: Duration = Duration::from_secs(5);
const SIGTERM_TIMEOUT: Duration = Duration::from_secs(3);

/// A spawned child process owned by the capture controller.
#[derive(Debug)]
pub struct ManagedChild {
    /// The underlying child handle; `child.stdout` is piped for callers to take.
    pub child: Child,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
}

/// Spawn `argv[0]` with args `argv[1..]` and extra env pairs; stderr is
/// piped and drained, stdout is piped for the caller.
pub fn spawn(argv: &[&str], env: &[(&str, &str)]) -> Result<ManagedChild, CaptureError> {
    let (prog, args) = argv
        .split_first()
        .ok_or_else(|| CaptureError::Spawn(std::io::Error::other("empty argv")))?;
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().map_err(CaptureError::Spawn)?;

    let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_RING_CAP)));
    if let Some(stderr) = child.stderr.take() {
        let ring = Arc::clone(&stderr_tail);
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut ring = ring.lock();
                if ring.len() == STDERR_RING_CAP {
                    ring.pop_front();
                }
                ring.push_back(line);
            }
        });
    }

    Ok(ManagedChild { child, stderr_tail })
}

impl ManagedChild {
    /// OS process id of the child.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Last ~50 stderr lines, joined with newlines.
    pub fn stderr_tail(&self) -> String {
        self.stderr_tail
            .lock()
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// SIGINT (≤5s) → SIGTERM (≤3s) → SIGKILL, then always `wait()`.
    pub fn stop_graceful(&mut self) -> Result<ExitStatus, CaptureError> {
        let pid = Pid::from_raw(self.child.id() as i32);
        if self
            .child
            .try_wait()
            .map_err(CaptureError::Spawn)?
            .is_none()
        {
            self.signal_and_wait(pid, Signal::SIGINT, SIGINT_TIMEOUT)?;
        }
        if self
            .child
            .try_wait()
            .map_err(CaptureError::Spawn)?
            .is_none()
        {
            self.signal_and_wait(pid, Signal::SIGTERM, SIGTERM_TIMEOUT)?;
        }
        if self
            .child
            .try_wait()
            .map_err(CaptureError::Spawn)?
            .is_none()
        {
            let _ = kill(pid, Signal::SIGKILL);
            self.child.wait().map_err(CaptureError::Spawn)?;
        }
        self.child.wait().map_err(CaptureError::Spawn)
    }

    /// Send `sig`, then poll `try_wait` every 50ms until exit or `timeout`.
    fn signal_and_wait(
        &mut self,
        pid: Pid,
        sig: Signal,
        timeout: Duration,
    ) -> Result<(), CaptureError> {
        let _ = kill(pid, sig); // ESRCH if it raced to exit; try_wait handles it
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self
                .child
                .try_wait()
                .map_err(CaptureError::Spawn)?
                .is_some()
            {
                return Ok(());
            }
            std::thread::sleep(POLL_TICK);
        }
        Ok(())
    }
}
