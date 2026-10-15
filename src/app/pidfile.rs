//! Single-instance pidfile under `$XDG_RUNTIME_DIR/niri-recorder/recorder.pid`
//! (mode 0600). Content: line 1 = pid, line 2 = argv[0] basename. Stale —
//! dead pid or argv0 mismatch — entries are ignored and removed.

use directories::BaseDirs;
use nix::sys::signal::kill;
use nix::unistd::Pid;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const PIDFILE_NAME: &str = "recorder.pid";
/// Linux `/proc/<pid>/comm` is TASK_COMM_LEN-1 = 15 chars.
const COMM_LEN: usize = 15;

/// Parsed pidfile entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PidEntry {
    pub pid: u32,
}

fn runtime_dir() -> PathBuf {
    if let Some(dir) = BaseDirs::new().and_then(|b| b.runtime_dir().map(Path::to_path_buf)) {
        return dir.join("niri-recorder");
    }
    let uid = fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(0);
    PathBuf::from(format!("/tmp/niri-recorder-{uid}"))
}

fn argv0_basename() -> String {
    std::env::args_os()
        .next()
        .and_then(|a| {
            PathBuf::from(a)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "niri-recorder".into())
}

/// Write `dir/recorder.pid` (0600) containing our pid and argv[0] basename.
pub fn write_pidfile_in(dir: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(PIDFILE_NAME);
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    writeln!(f, "{}", std::process::id())?;
    writeln!(f, "{}", argv0_basename())?;
    drop(f);
    // In case the file pre-existed with looser permissions.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    Ok(path)
}

/// Write the pidfile under `$XDG_RUNTIME_DIR/niri-recorder`
/// (`/tmp/niri-recorder-<uid>` when there is no runtime dir).
pub fn write_pidfile() -> io::Result<PathBuf> {
    write_pidfile_in(&runtime_dir())
}

/// Read `dir/recorder.pid`; `None` when missing, malformed, or stale.
/// Stale files are removed.
pub fn read_pidfile_in(dir: &Path) -> Option<PidEntry> {
    let path = dir.join(PIDFILE_NAME);
    let remove = |path: &Path| {
        let _ = fs::remove_file(path);
        None
    };
    let Ok(content) = fs::read_to_string(&path) else {
        return None; // absent — nothing to clean up
    };
    let mut lines = content.lines();
    let Some(pid) = lines.next().and_then(|l| l.trim().parse::<u32>().ok()) else {
        return remove(&path); // malformed — treated as stale
    };
    let argv0 = lines.next().unwrap_or("").trim().to_owned();

    let alive = kill(Pid::from_raw(pid as i32), None).is_ok();
    // `/proc/<pid>/comm` byte-truncates to TASK_COMM_LEN-1; compare the same
    // byte-level truncation of the recorded argv[0] basename (byte-level so a
    // multi-byte char straddling byte 15 can't panic a &str slice).
    let comm_matches = fs::read(format!("/proc/{pid}/comm"))
        .map(|c| c.trim_ascii_end() == &argv0.as_bytes()[..argv0.len().min(COMM_LEN)])
        .unwrap_or(false);

    if !alive || !comm_matches {
        return remove(&path);
    }
    Some(PidEntry { pid })
}

/// Read the runtime-dir pidfile; `None` when absent or stale.
pub fn read_pidfile() -> Option<PidEntry> {
    read_pidfile_in(&runtime_dir())
}

/// Remove `dir/recorder.pid` if present.
pub fn clear_pidfile_in(dir: &Path) {
    let _ = fs::remove_file(dir.join(PIDFILE_NAME));
}

/// Remove the runtime-dir pidfile.
pub fn clear_pidfile() {
    clear_pidfile_in(&runtime_dir());
}
