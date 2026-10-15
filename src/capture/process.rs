//! Managed capture subprocess: stderr drained on a thread into a bounded
//! ring so the child never blocks on a full pipe, and a bounded stop
//! sequence (SIGINT → SIGTERM → SIGKILL) that always reaps the child.

use crate::capture::backend::CaptureError;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, ErrorKind};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

const STDERR_RING_CAP: usize = 50;
const POLL_TICK: Duration = Duration::from_millis(50);

const SIGINT_TIMEOUT: Duration = Duration::from_secs(5);
const SIGTERM_TIMEOUT: Duration = Duration::from_secs(3);

/// True when `pid` is a zombie (exited, not yet reaped). Reads
/// `/proc/<pid>/stat` — field 3 after the last `)` is the state char;
/// reading proc does not reap, so `stop_graceful` still works afterwards.
pub(crate) fn is_zombie(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| s.rsplit(')').next().map(|rest| rest.to_owned()))
        .and_then(|rest| rest.split_whitespace().next().map(str::to_owned))
        .is_some_and(|state| state == "Z")
}

/// A spawned child process owned by the capture controller.
#[derive(Debug)]
pub struct ManagedChild {
    /// The underlying child handle; `child.stdout` is piped for callers to take.
    pub child: Child,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    stderr_join: Option<std::thread::JoinHandle<()>>,
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
    let stderr_join = child.stderr.take().map(|stderr| {
        let ring = Arc::clone(&stderr_tail);
        std::thread::spawn(move || {
            // Read raw bytes, not `lines()`: child stderr may hold non-UTF-8
            // and transient errors must not kill the drainer — a dead drainer
            // leaves the child blocked on a full pipe.
            let mut reader = BufReader::new(stderr);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf) {
                    Ok(0) => break, // EOF: child closed its stderr
                    Ok(_) => push_line(
                        &ring,
                        String::from_utf8_lossy(&buf)
                            .trim_end_matches(['\r', '\n'])
                            .to_string(),
                    ),
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(e) => {
                        // fd is dead; the child can't block on it, so exiting
                        // is safe — record why for stderr_tail().
                        push_line(&ring, format!("<stderr read error: {e}>"));
                        break;
                    }
                }
            }
        })
    });

    Ok(ManagedChild {
        child,
        stderr_tail,
        stderr_join,
    })
}

/// Append `line` to the bounded ring, dropping the oldest when full.
fn push_line(ring: &Mutex<VecDeque<String>>, line: String) {
    let mut ring = ring.lock();
    if ring.len() == STDERR_RING_CAP {
        ring.pop_front();
    }
    ring.push_back(line);
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
        // Child reaped ⇒ its stderr write end is closed ⇒ the drainer hits
        // EOF and exits; join it so the ring holds the final bytes.
        let status = self.child.wait().map_err(CaptureError::Spawn)?;
        if let Some(h) = self.stderr_join.take() {
            let _ = h.join();
        }
        Ok(status)
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
