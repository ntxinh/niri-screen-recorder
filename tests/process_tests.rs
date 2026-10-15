use niri_recorder::capture::process::spawn;
use niri_recorder::capture::CaptureError;

#[test]
fn graceful_stop_reaps_and_reports_exit() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("out");
    let mut c = spawn(
        &["bash", "tests/fake_child.sh"],
        &[("TARGET", target.to_str().unwrap())],
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let status = c.stop_graceful().unwrap();
    assert!(status.success());
    assert!(std::fs::read_to_string(&target)
        .unwrap()
        .contains("finalized"));
}

#[test]
fn missing_binary_maps_to_missing_tool() {
    let e = spawn(&["definitely-not-a-real-binary-xyz"], &[]).unwrap_err();
    assert!(matches!(e, CaptureError::Spawn(_))); // io ErrorKind::NotFound
}

#[test]
fn stop_escalates_to_sigterm_when_int_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("out");
    let mut c = spawn(
        &["bash", "tests/fake_child_ignore_int.sh"],
        &[("TARGET", target.to_str().unwrap())],
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    // Child ignores SIGINT (trap '' INT) → stop_graceful must escalate to
    // SIGTERM within ~5s and reap; the fixture's TERM trap exits 143.
    let status = c.stop_graceful().unwrap();
    assert_eq!(status.code(), Some(143));
}
