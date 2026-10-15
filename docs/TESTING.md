# Testing

Run everything with `make check` (fmt → clippy `-D warnings` → `cargo
test --all-targets` → build). All automated tests run headless — none
require a live Wayland session.

## Automated test inventory

| File | Covers |
|---|---|
| `tests/state_machine_tests.rs` | `transition` table: Idle→Starting, region select/cancel paths, Spawned/SpawnFailed, Recording→Stopping→Completed, failed exit reports path, recorder death → Failed, Completed terminal, duplicate start rejected, invalid `(state,event)` pairs are no-ops |
| `tests/controller_tests.rs` | `Recorder` over a fake backend: display start→stop finalizes `.part`, second start while recording errors, stop from idle errors, multi-output requires `--output`; pidfile round-trip and staleness (dead pid, argv0 mismatch incl. non-ASCII comm, malformed file removed) |
| `tests/backend_tests.rs` | `WfRecorder::argv`: `-o`/`-g` flags, `-m`/`-c` from inner extension (mp4→mp4+libopenh264, webm→webm+libvpx-vp9), `-r` fps, Standard emits no `-p` opts vs High's bitrate AVOptions, **no `-a`/audio flag ever**, `.part` always paired with explicit muxer |
| `tests/process_tests.rs` | `ManagedChild`: graceful SIGINT stop reaps child and reports exit, missing binary → `CaptureError::Spawn`, SIGINT-ignoring child escalates to SIGTERM within the bounded window |
| `tests/config_tests.rs` | Defaults when file missing, malformed TOML falls back with warning, `default_fps` restricted to 24/30/60, `~` tilde expansion to home |
| `tests/filename_tests.rs` | `plan_output`: timestamped `screen-recording-*.mp4` in dir, never overwrites (collision suffix), `finalize` renames `.part`→final |
| `tests/geometry_tests.rs` | `Geometry::parse_slurp`: `x,y WxH`, negative origins accepted, zero size and garbage rejected; `to_wf_arg` round-trip |
| `src/cli/commands.rs` `mod tests` | Clap surface: `start` defaults/`--target`/`--output` parsing, invalid target rejected, `stop`/`status`/`diagnostics`/bare invocation; `is_zombie` detects an exited-but-unreaped child via `/proc/<pid>/stat` (the poll loop's child-death seam, since `kill(pid,0)` is `Ok` on zombies). `first_line`/`diagnostics` internals are untested |

## Fake-child harness

`tests/fake_child.sh` is a stand-in recorder: requires `$TARGET`, prints
`fake-ready`, appends `data` to `$TARGET` every 200 ms, and traps `INT`
to write `finalized` and exit 0 (`TERM` exits 143). `process_tests` spawns
it through the real `spawn`/`stop_graceful` path, so the SIGINT→reap
sequence, stdout piping, and exit-status reporting are exercised without
a compositor. `tests/fake_child_ignore_int.sh` traps `INT` to nothing —
it verifies the bounded escalation: SIGINT ignored → SIGTERM within ~5s
window → (untested last resort) SIGKILL → always `wait()`.

## Manual acceptance tests (spec §16)

Not automatable — run on a real Fedora 44 + Niri session before claiming
support. Mark results in `docs/WAYLAND-NIRI-COMPATIBILITY.md`.

- [ ] Full-display capture on a single monitor (`start --target display`)
- [ ] Interactive region capture (`start --target region`, draw with slurp)
- [ ] Record while switching workspaces repeatedly, then stop
- [ ] Record while focusing other applications
- [ ] Cancel region selection (Esc) → `cancelled`, exit 0, no file
- [ ] Two monitors with different resolutions; `--output NAME` each
- [ ] Region selection on each monitor and across output boundaries
- [ ] Mixed scaling / negative-coordinate layouts if present
- [ ] Monitor disconnect during recording → clean stop, file preserved
- [ ] `dnf remove`/`mv` a required tool → actionable missing-dependency error
- [ ] Full output filesystem → safe stop, partial `.part` preserved
- [ ] Long recording stops and the file is playable end-to-end
- [ ] `ffprobe` confirms no audio stream in output
- [ ] CPU usage / file size at `quality = standard` vs `high`

Per AGENTS.md: results reported as passing must have been run and
inspected; unverified items stay on this list.
