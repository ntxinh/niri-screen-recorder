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
