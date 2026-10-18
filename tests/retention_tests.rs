use niri_recorder::files::naming::is_recording_path;
use niri_recorder::files::retention::{apply, victims, CleanupRules};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn touch(dir: &Path, name: &str, age_secs: u64) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, b"x").unwrap();
    let f = fs::OpenOptions::new().write(true).open(&p).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(age_secs))
        .unwrap();
    p
}

const DAY: u64 = 86_400;

#[test]
fn disabled_or_ruleless_is_noop() {
    let dir = tempfile::tempdir().unwrap();
    touch(
        dir.path(),
        "screen-recording-2026-10-01_10-00-00.mp4",
        DAY * 10,
    );
    let now = SystemTime::now();

    let disabled = CleanupRules {
        enabled: false,
        max_files: Some(1),
        max_age_days: Some(1),
    };
    assert!(victims(dir.path(), &disabled, now).is_empty());

    let ruleless = CleanupRules {
        enabled: true,
        ..CleanupRules::default()
    };
    assert!(victims(dir.path(), &ruleless, now).is_empty());
}

#[test]
fn max_files_keeps_newest_and_ignores_foreign_files() {
    let dir = tempfile::tempdir().unwrap();
    let old = touch(dir.path(), "screen-recording-2026-10-01_10-00-00.mp4", 300);
    let mid = touch(dir.path(), "screen-recording-2026-10-02_10-00-00.webm", 200);
    let new = touch(dir.path(), "screen-recording-2026-10-03_10-00-00.mp4", 100);
    // Never cleanup targets, even when the oldest files in the dir:
    let part = touch(
        dir.path(),
        "screen-recording-2026-09-01_10-00-00.mp4.part",
        900,
    );
    let foreign = touch(dir.path(), "holiday-video.mp4", 900);

    let rules = CleanupRules {
        enabled: true,
        max_files: Some(2),
        max_age_days: None,
    };
    assert_eq!(
        victims(dir.path(), &rules, SystemTime::now()),
        vec![old.clone()]
    );

    let rules = CleanupRules {
        enabled: true,
        max_files: Some(1),
        max_age_days: None,
    };
    let mut got = victims(dir.path(), &rules, SystemTime::now());
    got.sort();
    let mut want = vec![mid, old.clone()];
    want.sort();
    assert_eq!(got, want);

    assert!(part.exists() && foreign.exists() && new.exists());
}

#[test]
fn max_age_drops_old_entries_only() {
    let dir = tempfile::tempdir().unwrap();
    let old = touch(
        dir.path(),
        "screen-recording-2026-09-01_10-00-00.mp4",
        40 * DAY,
    );
    let recent = touch(dir.path(), "screen-recording-2026-10-17_10-00-00.mp4", DAY);
    let rules = CleanupRules {
        enabled: true,
        max_files: None,
        max_age_days: Some(30),
    };
    assert_eq!(victims(dir.path(), &rules, SystemTime::now()), vec![old]);
    assert!(recent.exists());

    // Age and count combine (union).
    let dir = tempfile::tempdir().unwrap();
    let a = touch(
        dir.path(),
        "screen-recording-2026-09-01_10-00-00.mp4",
        40 * DAY,
    );
    let b = touch(
        dir.path(),
        "screen-recording-2026-10-16_10-00-00.mp4",
        2 * DAY,
    );
    let _c = touch(dir.path(), "screen-recording-2026-10-17_10-00-00.mp4", DAY);
    let rules = CleanupRules {
        enabled: true,
        max_files: Some(2),
        max_age_days: Some(30),
    };
    // Only the age rule fires: max_files=2 keeps b and c.
    assert_eq!(victims(dir.path(), &rules, SystemTime::now()), vec![a]);
    assert!(b.exists());
}

#[test]
fn apply_deletes_only_victims() {
    let dir = tempfile::tempdir().unwrap();
    let old = touch(
        dir.path(),
        "screen-recording-2026-09-01_10-00-00.mp4",
        40 * DAY,
    );
    let keep = touch(dir.path(), "screen-recording-2026-10-17_10-00-00.mp4", DAY);
    let rules = CleanupRules {
        enabled: true,
        max_files: None,
        max_age_days: Some(30),
    };
    assert_eq!(apply(dir.path(), &rules), vec![old.clone()]);
    assert!(!old.exists());
    assert!(keep.exists());
    // Idempotent: nothing left to delete.
    assert!(apply(dir.path(), &rules).is_empty());
}

#[test]
fn recording_name_filter_is_strict() {
    let dir = tempfile::tempdir().unwrap();
    assert!(is_recording_path(&touch(
        dir.path(),
        "screen-recording-2026-10-17_10-00-00.mp4",
        0
    )));
    assert!(!is_recording_path(&touch(
        dir.path(),
        "screen-recording-2026-10-17_10-00-00.mp4.part",
        0
    )));
    assert!(!is_recording_path(&touch(
        dir.path(),
        "screen-recording-notes.txt",
        0
    )));
    assert!(!is_recording_path(dir.path()));
}
