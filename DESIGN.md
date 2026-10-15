# DESIGN — niri-recorder (Plan A: core recorder + CLI)

Architecture summary of the implemented system. The canonical product and
technical spec is
`docs/superpowers/specs/2026-10-15-screen-recorder-design.md`; capture
feasibility evidence lives in its §22 and in
`docs/CAPTURE-BACKENDS.md`. Where this file and the code disagree, the
code wins — file an issue.

## Module map

main.rs ──► cli (clap: start/stop/status/diagnostics)
              │ bare `niri-recorder`
              ├──► ui (GTK4/libadwaita window; worker owns Recorder)
              ▼
app::controller::Recorder<B: CaptureBackend>
   • drives app::state (explicit AppState/Event machine)
   • resolves output name, runs slurp for regions
   • plans output via files::naming, owns CaptureSession
              │
              ▼
capture::backend::CaptureBackend (trait)
              │ implemented by
              ▼
capture::backends::wf_recorder::WfRecorder
   • detect(): probe wf-recorder (+slurp) binaries
   • capabilities(): outputs from `niri msg --json outputs`
   • argv(req): pure argv builder (testable, no shell)
   • start(): spawn via capture::process
              │
              ▼
capture::process::ManagedChild
   • argv-only spawn, stdin null, stdout/stderr piped
   • stderr drained on a thread into a bounded 50-line ring
   • stop_graceful(): SIGINT ≤5s → SIGTERM ≤3s → SIGKILL → wait()
```

Supporting: `app::pidfile` (single-instance control file),
`app::state` (`transition` is pure; invalid pairs are no-ops),
`config::settings` (TOML, validation, tilde expansion),
`files::naming` (`plan_output`/`finalize` with `.part` staging).

## Control model (no daemon)

`niri-recorder start` is the recorder: it runs in the foreground of its
own process, spawns `wf-recorder` as a child, and polls a stop flag plus
child liveness (including zombie detection via `/proc/<pid>/stat`) every
100 ms. Coordination between invocations is a pidfile:

- `$XDG_RUNTIME_DIR/niri-recorder/recorder.pid`, mode 0600 (fallback
  `/tmp/niri-recorder-<uid>` when no runtime dir exists; the dir is
  forced to 0700 and refused when foreign-owned).
- Content: pid + argv[0] basename; staleness check compares the live
  `/proc/<pid>/comm`. Dead-pid, mismatched-comm, or malformed files are
  treated as absent and removed.
- `stop` reads the pidfile and sends SIGINT to the `start` process, whose
  `ctrlc` handler (INT+TERM+HUP+QUIT) flips the flag; the real teardown
  runs in `Recorder::stop`. `status` reports pidfile + liveness.
- `start` claims the pidfile atomically with `O_EXCL` (`create_new`): a
  live pidfile exits 1 ("already recording"), a stale one is replaced.
  A Ctrl-C during `slurp` cancels the start — no recorder is spawned.

## State machine

Implemented in `src/app/state.rs` (spec §10). `Failed` covers the spec's
Error/Recovery states.

```text
Idle ──StartDisplay──► Starting ──Spawned(pid)──► Recording{pid}
 │                       │                          │
 │                    SpawnFailed                 StopRequested
 │                       │                          │
 │                       ▼                          ▼
 │                    Failed                   Stopping{pid}
 │                                                │
 │   Idle ──StartRegion──► SelectingRegion     Exited{ok,path}
 │                            │   │            /          \
 │            SelectionCancelled  SelectionDone(g)          \
 │                            │   │           ▼              ▼
 │                            ▼   └► Starting   Completed(path)  Failed
 │                          Idle
 │
 └── Recording ──Died(e)──► Failed
```

Key rules from the implementation:

- `Recording` implies a live `CaptureSession` (child + `.part` path).
- `stop()` claims success only after the child is reaped AND the `.part`
  file exists with `len > 0`; an empty `.part` is deleted and reported as
  `UnexpectedExit` with the captured stderr tail.
- Region cancel (slurp exits non-zero or emits unparseable text) returns
  to `Idle` and is *not* an error — `start` prints `cancelled`, exit 0.

## Output finalization

`files::naming` picks `screen-recording-<YYYY-MM-DD_HH-MM-SS>.mp4` under
`output_directory` (created if missing), probing `-1`…`-99` suffixes when
the base name or its `.part` exists — nothing is ever overwritten.
`wf-recorder` writes to `name.mp4.part` (the `-m mp4` muxer flag is
passed explicitly because `.part` defeats format inference); on clean
stop the `.part` is renamed to the final name, so incomplete captures are
visibly marked. If the child dies early, the finalized partial file is
kept but the CLI reports failure (exit 3).

## CLI surface

```text
niri-recorder            bare → GTK4/libadwaita GUI (exit 1 headless)
  start [--target display|region] [--output NAME]   foreground recorder
  stop      SIGINT via pidfile; "not recording" → exit 1
  status    "recording pid=N" | "idle"; always exit 0
  diagnostics  tool versions, outputs, backend capabilities; exit 0
```

Exit codes: 0 ok · 1 not recording / already recording / cannot signal ·
2 backend missing or clap parse error · 3 capture failure.

## Backend decision and evidence (spec §22, verified 2026-10-15)

`wf-recorder` 0.6.0 (`wf-recorder-0.6.0-2.fc44`) + `slurp` is the only
backend, behind the `CaptureBackend` trait so alternatives can slot in
without touching the controller.

Verified on the target (Fedora 44, Wayland `wayland-1`, Niri, DP-3
1920×1080@60, Intel CometLake-H iHD):

- Full display: `wf-recorder -f out.mp4 -c libopenh264 -r 30` → playable
  1080p MP4, video-only stream, finalized on SIGINT.
- Region: `-g "x,y WxH"` takes slurp's output verbatim.
- Workspace switching mid-capture: uninterrupted (494 frames over 16.4s).
- No audio stream in output; `-a` is never passed.

Rejected: `wl-screenrec` (unconditional `scale_vaapi` filter; iHD exposes
zero H.264 encode profiles on CometLake → hard panic), XDG ScreenCast
portal (niri installs no ScreenCast impl; Fedora ffmpeg lacks a pipewire
demuxer). Details: `docs/CAPTURE-BACKENDS.md`.

## Configuration and paths

`~/.config/niri-recorder/config.toml` — `output_directory`,
`default_target`, `default_fps` (24/30/60 only), `quality`
(standard|high), `preferred_backend`. A missing config is silent; an
unreadable or malformed one falls back to defaults with a warning; `~`
expands only at the leading component. Runtime state under
`$XDG_RUNTIME_DIR/niri-recorder/`. See
`docs/REQUIREMENTS.md` FR-09.

## Security boundaries

- argv-array subprocess spawning only; no shell strings, no env passthrough.
- Video only — no audio flags, no microphone access.
- All I/O local: output under the user's home, pidfile in a user-private
  runtime dir (0600). No network use, no telemetry. Runs unprivileged by
  design — no setuid or privileged code paths (don't run it as root).
- wf-recorder stderr is captured to a bounded ring for diagnostics and
  surfaced on failure; nothing is logged to a file.

## Known limitations

- VFR output (damage-tracked wlr-screencopy, ~30 fps average).
- No hardware encoding; High quality = openh264 AVOptions
  `-p b=10000000 -p rc_mode=bitrate -p allow_skip_frames=true`.
- `preferred_backend`: `"auto"` and `"wf-recorder"` are accepted; any
  other value exits 2 with an unsupported-backend error (spec §23.6)
  until a second backend lands.
- Multi-monitor and interactive slurp verified only as geometry contract;
  both are manual-verification items (`docs/WAYLAND-NIRI-COMPATIBILITY.md`).
