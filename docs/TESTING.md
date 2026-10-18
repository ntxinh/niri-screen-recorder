# Testing

Run everything with `make check` (fmt → clippy `-D warnings` → `cargo
test --all-targets` → build). No automated test needs a compositor.
Thumbnail generation needs the `ffmpeg` binary and skips itself when it
is missing; the shortcut test avoids `gtk::init` by exercising only the
empty/disabled path.

## Automated test inventory

| File | Covers |
|---|---|
| `tests/state_machine_tests.rs` | `transition` table: Idle→Starting, region select/cancel paths, Spawned/SpawnFailed, Recording→Stopping→Completed, failed exit reports path, recorder death → Failed, Completed terminal, duplicate start rejected, invalid `(state,event)` pairs are no-ops |
| `tests/controller_tests.rs` | `Recorder` over a fake backend: display start→stop finalizes `.part`, second start while recording errors, stop from idle errors, multi-output uses `last_output` when it matches an enabled output and otherwise requires `--output` (stale `last_output` still errors), non-zero child exit keeps the finalized partial and reports `UnexpectedExit`; pidfile round-trip, atomic double-claim (`AlreadyExists`), stale-pidfile re-claim, dir forced 0700, and staleness (dead pid, argv0 mismatch incl. non-ASCII comm, malformed file removed) |
| `tests/backend_tests.rs` | `WfRecorder::argv`: `-o`/`-g` flags, `-m`/`-c` from inner extension (mp4→mp4+libopenh264, webm→webm+libvpx-vp9), `-r` fps, Standard emits no `-p` opts vs High's bitrate AVOptions, **no `-a`/audio flag ever**, `.part` always paired with explicit muxer |
| `tests/process_tests.rs` | `ManagedChild`: graceful SIGINT stop reaps child and reports exit, missing binary → `CaptureError::Spawn`, SIGINT-ignoring child escalates to SIGTERM within the bounded window |
| `tests/config_tests.rs` | Defaults when file missing, malformed TOML falls back with warning, `default_fps` restricted to 24/30/60, `countdown_seconds` restricted to 0/3/5/10, Phase 6 keys default on pre-Phase-6 configs, cleanup rules map 0 → no limit, `~` tilde expansion to home |
| `tests/retention_tests.rs` | Disabled/no-rule no-op, max-files keeps newest and ignores `.part`/foreign files, max-age, union of both rules, `apply` deletes only victims, strict recording-name filter |
| `tests/daemon_tests.rs` | Control socket lifecycle: `status` over a real Unix socket, 0600 socket, second daemon refused, malformed/unknown request → code 2, stop-when-idle → code 1, shutdown removes the socket; absent daemon → `None` (CLI fallback) |
| `src/daemon/{protocol,client}.rs` tests | Request/response JSON round-trips and defaults, invalid requests rejected, empty fields omitted, socket round-trip and silent-socket error |
| `src/capture/encoders.rs` tests | `ffmpeg -encoders` parser (video only, legend skipped), auto/software container default, hardware preference order, explicit encoder, software fallback + warning, quality AVOptions |
| `src/ui/thumbnails.rs` tests | Cache key stable and mtime/size-sensitive, argv is shell-free/video-only, real PNG from a synthetic clip (skipped without ffmpeg) |
| `tests/filename_tests.rs` | `plan_output`: timestamped `screen-recording-*.mp4` in dir, never overwrites (collision suffix), `finalize` renames `.part`→final |
| `tests/geometry_tests.rs` | `Geometry::parse_slurp`: `x,y WxH`, negative origins accepted, zero size and garbage rejected; `to_wf_arg` round-trip |
| `src/cli/commands.rs` `mod tests` | Clap surface: `--target`/`--output` parsing (`--target` optional; config `default_target` applies), invalid target rejected, `stop`/`status`/`diagnostics`/bare invocation; `preferred_backend` accepts `auto`/`wf-recorder` only; `is_zombie` detects an exited-but-unreaped child via `/proc/<pid>/stat` (the poll loop's child-death seam, since `kill(pid,0)` is `Ok` on zombies). `first_line`/`diagnostics` internals are untested |

## Live reliability pass (2026-10-15, dual output DP-3 + eDP-1)

Exercised on the target session; full command log in
`.superpowers/sdd/2026-10-15-gui-and-packaging/task-6-report.md`.

- `niri msg outputs`: DP-3 at (0,0), eDP-1 enabled at (−1920,0).
- CLI `start --target display --output eDP-1` and `--output DP-3`:
  h264/1920×1080, video-only stream, finalized playable files. Static
  eDP-1 captured 1 frame (VFR); with on-screen activity ~30 fps.
- `start --target display` with two outputs enabled → `backend
  unsupported: multiple displays; --output required`, exit 3. (Phase 6
  added a `last_output` fallback; with no remembered display the error
  is unchanged.)
- Disk pressure (`unshare -rm` + 20 MB tmpfs, `XDG_CONFIG_HOME`
  override): 0-byte-free dir → wf-recorder `No space left on device`
  surfaced, empty `.part` deleted, exit 3. `ulimit -f` below ~8.3 MB →
  wf-recorder's raw-frame buffer fails at spawn, surfaced, exit 3.
  Mid-capture fill: wf-recorder buffers muxer output and may exit 0
  with a truncated file — documented caveat.
- 60 s DP-3 capture with ~20 workspace switches: 60.53 s, 1817 frames,
  137 KB, playable.
- GUI (driven via AT-SPI `click` action; keyboard/pointer input is not
  injectable on niri): with DP-3 off the Display dropdown enumerated
  eDP-1; Start/Stop produced a 12.9 s, 388-frame eDP-1 capture.
- Bug fixed during this pass: `list_outputs` offered disabled
  connectors (`logical: null`) as capture targets — now filtered, with
  a regression test (`disabled_outputs_are_filtered`).

## Phase 6 live verification (2026-10-18, target session)

- **Daemon + forwarded capture** (isolated `XDG_CONFIG_HOME` /
  `XDG_RUNTIME_DIR` / output dir, `WAYLAND_DISPLAY` pointed at the real
  socket): `daemon` → `status` = `idle` → `start --output DP-3` returned
  the `.part` path immediately → `status` = `recording pid=…` → `stop`
  printed `saved → …mp4` (H.264 2560×1440, 1.97 s) → second `stop` =
  `not recording`, exit 1 → SIGTERM shut the daemon down and removed the
  socket.
- **Retention**: dir seeded with two old recordings, a `.part` and a
  foreign `holiday.mp4`; `cleanup_enabled = true`, `cleanup_max_files =
  1`. After a daemon-recorded save only the new recording remained; the
  `.part` and foreign file survived.
- **Foreground fallback**: with no daemon, `start` (background process) +
  `status` (`recording pid=N`) + `stop` (`stop requested`, exit 0) still
  finalized normally.
- **GUI**: 4 s launch under niri with a synthetic existing recording — no
  stderr, and a 160×90 PNG appeared in the thumbnail cache.
- **GUI countdown (AT-SPI click)**: with `countdown_seconds = 3`, Start
  changed the button to `Cancel countdown` and the status to `Starting in
  2s…`; Cancel returned to `Start Recording` / `Idle` with no file or
  pidfile. A second run let the countdown finish → `Recording · 00:01`,
  Stop → `Completed`, H.264 2560×1440, 1.47 s, and the recent list
  updated.
- **Settings dialog (AT-SPI click)**: the gear opened the dialog with the
  Countdown, Shortcuts, Encoder and Retention groups showing the saved
  values (`3 seconds`, `h264_vaapi`, cleanup on). No panic; the earlier
  RefCell panic from passing `ui` into `present` was found and fixed
  during this run.
- **DMS 1.6**: `make install-dms-plugin` → `plugins list` showed
  `niriRecorder [disabled]` → `plugins reload niriRecorder` →
  `PLUGIN_RELOAD_SUCCESS` / `[loaded]` → DMS log `DankBar: Plugin loaded:
  niriRecorder`, no QML errors. Plugin removed afterwards.

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

- [x] Full-display capture on a single monitor (`start --target display`) — DP-3 verified; eDP-1 while it was the sole enabled output
- [ ] Interactive region capture (`start --target region`, draw with slurp) — cannot inject a drag headlessly; manual steps in `docs/WAYLAND-NIRI-COMPATIBILITY.md`
- [x] Record while switching workspaces repeatedly, then stop — ~20 switches in a 60 s capture, uninterrupted
- [x] Record while focusing other applications — focus/workspace churn during the 60 s run
- [ ] Cancel region selection (Esc) → `cancelled`, exit 0, no file — needs a physical keyboard Esc during slurp
- [x] `--output NAME` on each of two monitors — both are 1920×1080; mixed resolutions remains unverified (pending in `docs/WAYLAND-NIRI-COMPATIBILITY.md`)
- [x] Countdown: start → cancel, and start → complete → stop (AT-SPI clicks; see above)
- [ ] Window shortcuts: focused-window `Ctrl+R` / `Ctrl+S`, plus a remapped and a disabled binding — niri rejects synthetic key events
- [ ] DMS bar/Control Center: add the widget to a layout, confirm state rendering and click-to-record/stop
- [ ] Region selection on each monitor and across output boundaries — needs a real drag
- [x] Mixed scaling / negative-coordinate layouts if present — negative-coordinate layout present (eDP-1 at −1920,0); mixed scale absent
- [ ] Monitor disconnect during recording → clean stop, file preserved
- [ ] `dnf remove`/`mv` a required tool → actionable missing-dependency error
- [x] Full output filesystem → safe stop, partial `.part` preserved — honest exit 3; empty `.part` deleted; mid-capture fill caveat documented
- [x] Long recording stops and the file is playable end-to-end — 60 s / 1817 frames
- [x] `ffprobe` confirms no audio stream in output — every capture probed: single `video` stream
- [ ] CPU usage / file size at `quality = standard` vs `high` — standard verified (137 KB/min idle-ish); high measured on tmpfs only

Per AGENTS.md: results reported as passing must have been run and
inspected; unverified items stay on this list.
