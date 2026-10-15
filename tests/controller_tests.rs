use niri_recorder::app::controller::Recorder;
use niri_recorder::app::pidfile::{clear_pidfile_in, read_pidfile_in, write_pidfile_in};
use niri_recorder::app::state::AppState;
use niri_recorder::capture::backend::{CaptureBackend, CaptureError, CaptureSession};
use niri_recorder::capture::capabilities::CaptureCapabilities;
use niri_recorder::capture::process::spawn;
use niri_recorder::capture::request::CaptureRequest;
use niri_recorder::config::settings::{Settings, Target};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Backend that "captures" by appending to the .part file via the
/// fake_child.sh fixture (writes 'finalized' on SIGINT), while a real
/// `CaptureSession` carries the part/final paths the controller uses.
struct FakeBackend {
    outputs: Vec<String>,
}

impl CaptureBackend for FakeBackend {
    fn capabilities(&self) -> Result<CaptureCapabilities, CaptureError> {
        Ok(CaptureCapabilities {
            outputs: self.outputs.clone(),
            region: true,
            codecs: vec!["libopenh264".into()],
        })
    }

    fn start(&self, req: CaptureRequest) -> Result<CaptureSession, CaptureError> {
        // Seed .part so a quick stop still has content to finalize.
        std::fs::write(&req.part_path, b"fake-frames\n").map_err(CaptureError::Spawn)?;
        let child = spawn(
            &["bash", "tests/fake_child.sh"],
            &[("TARGET", req.part_path.to_str().unwrap())],
        )?;
        let final_path = req
            .part_path
            .to_str()
            .and_then(|s| s.strip_suffix(".part"))
            .map_or_else(|| req.part_path.clone(), PathBuf::from);
        Ok(CaptureSession {
            child,
            part_path: req.part_path,
            final_path,
        })
    }
}

fn recorder(dir: &Path, outputs: Vec<String>) -> Recorder<FakeBackend> {
    let settings = Settings {
        output_directory: dir.to_str().unwrap().into(),
        ..Settings::default()
    };
    Recorder::new(FakeBackend { outputs }, settings)
}

#[test]
fn start_display_then_stop_finalizes() {
    let dir = tempfile::tempdir().unwrap();
    let mut rec = recorder(dir.path(), vec!["DP-1".into()]);
    rec.start(Target::Display, None).unwrap();
    assert!(matches!(rec.state(), AppState::Recording { .. }));
    std::thread::sleep(Duration::from_millis(300));
    let path = rec.stop().unwrap();
    assert!(matches!(rec.state(), AppState::Completed(_)));
    assert!(path.exists());
    assert_eq!(path.extension().unwrap(), "mp4");
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("fake-frames"));
    assert!(content.contains("finalized")); // child reaped before finalize
}

#[test]
fn second_start_while_recording_errors() {
    let dir = tempfile::tempdir().unwrap();
    let mut rec = recorder(dir.path(), vec!["DP-1".into()]);
    rec.start(Target::Display, None).unwrap();
    let err = rec.start(Target::Display, None).unwrap_err();
    assert!(matches!(err, CaptureError::Unsupported(_)));
    assert!(matches!(rec.state(), AppState::Recording { .. }));
    rec.stop().unwrap(); // reap the fake child
}

#[test]
fn stop_from_idle_errors() {
    let dir = tempfile::tempdir().unwrap();
    let mut rec = recorder(dir.path(), vec!["DP-1".into()]);
    assert!(rec.stop().is_err());
    assert!(matches!(rec.state(), AppState::Idle));
}

#[test]
fn multiple_outputs_require_explicit_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut rec = recorder(dir.path(), vec!["DP-1".into(), "HDMI-A-1".into()]);
    let err = rec.start(Target::Display, None).unwrap_err();
    assert!(matches!(err, CaptureError::Unsupported(_)));
    assert!(matches!(rec.state(), AppState::Idle));
    // Explicit name bypasses the ambiguity.
    rec.start(Target::Display, Some("HDMI-A-1".into())).unwrap();
    assert!(matches!(rec.state(), AppState::Recording { .. }));
    rec.stop().unwrap();
}

#[test]
fn pidfile_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_pidfile_in(dir.path()).unwrap();
    assert_eq!(path, dir.path().join("recorder.pid"));
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let entry = read_pidfile_in(dir.path()).expect("own live pid is not stale");
    assert_eq!(entry.pid, std::process::id());
    clear_pidfile_in(dir.path());
    assert!(!path.exists());
}

#[test]
fn stale_pidfile_nonascii_argv0_no_panic_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    // argv0 basename >15 bytes with a multi-byte char straddling byte 15 —
    // the old `&argv0[..15]` str-slice would panic; byte-compare must not.
    let argv0 = "niri-recorder-é-x"; // é is bytes 15-16
    assert!(argv0.as_bytes()[14] != b'x'); // sanity: byte 15 is mid-char
    let path = dir.path().join("recorder.pid");
    std::fs::write(&path, format!("{}\n{}\n", std::process::id(), argv0)).unwrap();
    assert!(read_pidfile_in(dir.path()).is_none());
    assert!(!path.exists());
}

#[test]
fn malformed_pidfile_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("recorder.pid");
    std::fs::write(&path, "not-a-pid\nniri-recorder\n").unwrap();
    assert!(read_pidfile_in(dir.path()).is_none());
    assert!(!path.exists());
}

#[test]
fn stale_pidfile_dead_pid_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    // A pid we just reaped is guaranteed dead (no zombie, ESRCH).
    let dead_pid = {
        let mut c = std::process::Command::new("true").spawn().unwrap();
        let pid = c.id();
        c.wait().unwrap();
        pid
    };
    let path = dir.path().join("recorder.pid");
    std::fs::write(&path, format!("{dead_pid}\nniri-recorder\n")).unwrap();
    assert!(read_pidfile_in(dir.path()).is_none());
    assert!(!path.exists());
}

#[test]
fn stale_pidfile_argv0_mismatch_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    // Live pid (us) but a different argv[0] → not our recorder → stale.
    let path = dir.path().join("recorder.pid");
    std::fs::write(&path, format!("{}\nsome-other-tool\n", std::process::id())).unwrap();
    assert!(read_pidfile_in(dir.path()).is_none());
    assert!(!path.exists());
}
