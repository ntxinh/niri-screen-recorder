//! One test per row of the spec §10 state diagram.

use niri_recorder::app::state::{transition, AppState, Event};
use niri_recorder::capture::request::Geometry;
use std::path::PathBuf;

fn geo() -> Geometry {
    Geometry {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    }
}

#[test]
fn idle_starts() {
    assert_eq!(
        transition(AppState::Idle, Event::StartDisplay),
        AppState::Starting
    );
    assert_eq!(
        transition(AppState::Idle, Event::StartRegion),
        AppState::SelectingRegion
    );
}

#[test]
fn region_selection() {
    let sel = AppState::SelectingRegion;
    assert_eq!(
        transition(sel.clone(), Event::SelectionDone(geo())),
        AppState::Starting
    );
    assert_eq!(transition(sel, Event::SelectionCancelled), AppState::Idle);
}

#[test]
fn spawn_outcome() {
    assert_eq!(
        transition(AppState::Starting, Event::Spawned(42)),
        AppState::Recording { pid: 42 }
    );
    assert_eq!(
        transition(AppState::Starting, Event::SpawnFailed("no portal".into())),
        AppState::Failed("no portal".into())
    );
}

#[test]
fn recording_to_stopping_to_completed() {
    let rec = AppState::Recording { pid: 7 };
    assert_eq!(
        transition(rec, Event::StopRequested),
        AppState::Stopping { pid: 7 }
    );
    let path = PathBuf::from("/tmp/out.mp4");
    assert_eq!(
        transition(
            AppState::Stopping { pid: 7 },
            Event::Exited {
                ok: true,
                path: path.clone()
            }
        ),
        AppState::Completed(path)
    );
}

#[test]
fn failed_exit_reports_path() {
    let path = PathBuf::from("/tmp/out.mp4.part");
    let AppState::Failed(msg) = transition(
        AppState::Stopping { pid: 7 },
        Event::Exited {
            ok: false,
            path: path.clone(),
        },
    ) else {
        panic!("expected Failed");
    };
    assert!(msg.contains(path.to_str().unwrap()), "message: {msg}");
}

#[test]
fn recorder_death_fails() {
    assert_eq!(
        transition(
            AppState::Recording { pid: 3 },
            Event::Died("SIGPIPE".into())
        ),
        AppState::Failed("SIGPIPE".into())
    );
}

#[test]
fn completed_is_terminal() {
    let done = AppState::Completed(PathBuf::from("/tmp/out.mp4"));
    for e in [
        Event::StartDisplay,
        Event::StartRegion,
        Event::StopRequested,
        Event::Spawned(1),
    ] {
        assert_eq!(transition(done.clone(), e), done);
    }
}

#[test]
fn duplicate_start_rejected_while_recording() {
    let rec = AppState::Recording { pid: 1 };
    assert_eq!(transition(rec.clone(), Event::StartDisplay), rec);
    assert_eq!(transition(rec.clone(), Event::StartRegion), rec);
    // ...and while already Starting or SelectingRegion
    assert_eq!(
        transition(AppState::Starting, Event::StartDisplay),
        AppState::Starting
    );
    assert_eq!(
        transition(AppState::SelectingRegion, Event::StartRegion),
        AppState::SelectingRegion
    );
}

#[test]
fn invalid_pairs_are_noops() {
    for (s, e) in [
        (AppState::Idle, Event::StopRequested),
        (AppState::Idle, Event::Spawned(9)),
        (AppState::Idle, Event::SelectionDone(geo())),
        (
            AppState::Recording { pid: 2 },
            Event::Exited {
                ok: true,
                path: PathBuf::from("/tmp/x"),
            },
        ),
        (AppState::Stopping { pid: 2 }, Event::Died("x".into())),
        (AppState::Failed("e".into()), Event::StartDisplay),
    ] {
        assert_eq!(transition(s.clone(), e), s);
    }
}
