//! Explicit recorder state machine (spec §10). `transition` is pure;
//! invalid `(state, event)` pairs return the state unchanged — callers log.

use crate::capture::request::Geometry;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    Idle,
    SelectingRegion,
    Starting,
    Recording { pid: u32 },
    Stopping { pid: u32 },
    Completed(PathBuf),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    StartDisplay,
    StartRegion,
    SelectionDone(Geometry),
    SelectionCancelled,
    Spawned(u32),
    SpawnFailed(String),
    StopRequested,
    Exited { ok: bool, path: PathBuf },
    Died(String),
}

pub fn transition(state: AppState, event: Event) -> AppState {
    use AppState::*;
    use Event::*;
    match (state, event) {
        (Idle, StartDisplay) => Starting,
        (Idle, StartRegion) => SelectingRegion,
        (SelectingRegion, SelectionDone(_)) => Starting,
        (SelectingRegion, SelectionCancelled) => Idle,
        (Starting, Spawned(pid)) => Recording { pid },
        (Starting, SpawnFailed(e)) => Failed(e),
        (Recording { pid }, StopRequested) => Stopping { pid },
        (Recording { .. }, Died(e)) => Failed(e),
        (Stopping { .. }, Exited { ok: true, path }) => Completed(path),
        // File may be partial; files::stage owns the .part rename.
        (Stopping { .. }, Exited { ok: false, path }) => Failed(format!(
            "recorder exited with error; output may be partial: {}",
            path.display()
        )),
        (s, _) => s,
    }
}
