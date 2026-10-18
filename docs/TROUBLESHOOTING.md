# Troubleshooting

First step for anything: `niri-recorder diagnostics`. It prints the niri
version, detected outputs (`niri msg --json outputs`), wf-recorder /
slurp / ffprobe presence, the resolved config path, and backend
capabilities. Exit codes: `0` ok · `1` not recording / already recording
/ cannot signal · `2` backend missing or CLI parse error · `3` capture
failure.

## Error messages and fixes

### `missing dependency: wf-recorder (install: sudo dnf install wf-recorder)`

`wf-recorder` is not on `PATH` (or cannot be executed). Install it, or
check `diagnostics` to see whether the probe fails. Exit code 2.

### `backend unsupported: …`

The backend refused the request. Produced messages:

- `no display detected` — `niri msg --json outputs` returned nothing.
  Confirm you're inside a Niri session (`echo $WAYLAND_DISPLAY`, run
  `niri msg outputs`).
- `multiple displays; --output required` — no remembered display
  matches the enabled outputs. Pass `niri-recorder start --output DP-3`
  with a name from `diagnostics`; that choice is saved as `last_output`
  and later bare starts (keybinds, DMS) reuse it while the output is
  still enabled.
- `cannot start while …` / `cannot stop while …` — internal state guard;
  a second start on the same process, or stop with nothing running.

### `niri-recorder: already recording (pid N)`

A live pidfile exists — the pidfile is the single-instance lock, claimed
atomically at `start`. Check `niri-recorder status`; use `niri-recorder
stop`, or Ctrl-C the `start` process. A stale file is removed and
re-claimed automatically, so persisting output here means a real
recorder is running. Exit 1.

### `not recording` (from `stop`, exit 1)

No pidfile or it was stale (dead pid / mismatched argv0 — cleaned up
automatically). Nothing to stop.

### `cannot signal pid N: …` (from `stop`, exit 1)

The pidfile pointed at a live process but SIGINT failed — typically a
permissions mismatch (recorder started as another user/root). Check `ps
-p N -o pid,user,comm`.

### `slurp` region problems

- Region mode but `slurp` missing → `missing dependency: slurp
  (install: sudo dnf install slurp)` (exit 3). Ctrl-C during `slurp`
  cancels the start — exit 0, `cancelled`, no recording spawned. Note
  `detect()` treats slurp as optional — region is the only mode that
  needs it.
- Esc / cancel → `cancelled`, exit 0, no file. This is intended, not an
  error.
- Unparseable slurp output → treated as cancel. Malformed geometry like
  `x,y WxH` violations is rejected by `Geometry::parse_slurp` (zero
  size, garbage) before reaching wf-recorder.

### `recorder child exited unexpectedly` and `stop failed: …` (exit 3)

`recorder child exited unexpectedly` is the standalone line the `start`
poll loop prints when the wf-recorder child dies mid-capture; the
recording is then finalized and reported as `capture ended early; saved
<path>` — the partial file is kept but the run is a failure (exit 3).

`recorder exited unexpectedly: <status>; kept recoverable partial at
<path>` inside `stop failed: <err>` means the child exited non-zero but
wrote data — the partial is finalized and kept, the run is a failure
(exit 3). A clean exit with an empty `.part` reports just the stderr
tail (the empty file is deleted; nothing is claimed saved). The tail
usually names the cause (bad output name, unsupported codec, protocol
error).

### `stop failed: …` / stop timeouts

`Recorder::stop` escalates SIGINT (≤5 s) → SIGTERM (≤3 s) → SIGKILL, then
always reaps. SIGKILL is unconditional at the end, so a stuck child can't
hang the CLI forever; an io error mid-stop additionally force-kills and
reaps before reporting. If `stop` reports failure, check for a leftover
`.mp4.part` file in the output directory — it is intentionally kept.

### `cannot write pidfile: …` (exit 3)

`$XDG_RUNTIME_DIR/niri-recorder` isn't writable. Check the runtime dir
exists (`echo $XDG_RUNTIME_DIR`); without one the code falls back to
`/tmp/niri-recorder-<uid>`.

### `cannot install signal handler` (exit 3)

The `ctrlc` handler couldn't be registered (already set, or platform
refusal) — the pidfile is cleaned up and no capture starts.

### `no free recording name in <dir>`

All 100 timestamped slots (`screen-recording-<ts>` plus `-1`…`-99`) exist
in the output directory — i.e. >100 starts in one second, or the
directory is full of stale `.part` files. Clean the directory or set a
different `output_directory`.

### Output filesystem problems (verified 2026-10-15)

- **Full at start**: wf-recorder exits with `Failed to write file
  header` / `No space left on device`; niri-recorder surfaces the
  stderr tail (`stop failed: recorder exited unexpectedly: …`),
  deletes the empty `.part`, exits 3. Free space and retry.
- **Fills mid-capture**: wf-recorder buffers muxer output and can exit
  0 anyway, leaving a truncated, still-somewhat-playable file reported
  as `saved`. If a file is suspiciously small for its duration, check
  `df -h` on the output directory — this is an upstream wf-recorder
  limitation the recorder cannot detect.
- **`ulimit -f` / per-file size limit**: wf-recorder preallocates a
  ~8.3 MB raw-frame buffer file (memfd). A limit under that fails at
  spawn with `creating a buffer file … File too large`, surfaced as
  `stop failed` / exit 3.

### Disabled monitor still listed / recorded blank

As of the disabled-connector fix, `niri msg output <NAME> off` removes
the output from `diagnostics` automatically (fresh query each run) and
from the GUI dropdown on next launch — a running GUI keeps its
already-populated list until restarted. On older builds a disabled
connector appeared as a selectable output; upgrade or re-enable it.

### Recording has ~1 frame / duration N/A

wf-recorder is damage-tracked: a completely static screen produces
almost no frames — a valid but near-empty file (single h264 frame,
`duration=N/A` in ffprobe). Move a window, run a clock, or switch
workspaces during capture to get real frame counts.

### Stop seems slow (~3–4 s)

Normal: SIGINT tells wf-recorder to finalize, and its encoder/muxer
flush takes a few seconds for mp4. The `.part` renames to the final
name once it exits.

### Daemon problems

- **`daemon already running on …`** — another daemon is listening on
  `$XDG_RUNTIME_DIR/niri-recorder/control.sock`. Use
  `niri-recorder diagnostics` to see its state; stop it with SIGINT/SIGTERM
  before starting a second one.
- **`invalid daemon response: …`** — something is listening on the
  socket but is not this daemon. The CLI refuses to fall back to a
  second recorder; remove the stale socket only after confirming no
  daemon is running.
- **`start` returns but `status` says idle** — the daemon may have
  exited; check `systemctl --user status niri-recorder-daemon` (unit in
  `data/niri-recorder-daemon.service`).
- **`start` fails with `already recording (pid N)`** — a GUI or
  foreground recorder owns the pidfile. Stop that session first; the
  daemon never steals a live pidfile.

### Encoder selection

`encoder = "hardware"` falls back to software when `ffmpeg -encoders`
lists no matching hardware encoder. If it *is* listed but the capture
still fails (e.g. `Failed to create processing pipeline config`), the
GPU/driver cannot run it — this is expected on CometLake iHD with VAAPI.
Set `encoder = "software"` or `"auto"` and record again. `diagnostics`
shows the probed encoder list in the backend line.

### No thumbnails in the recent list

Thumbnails require the `ffmpeg` binary (not just `ffprobe`) and the
recent list only shows finished `screen-recording-*.(mp4|webm)` files.
Generation errors are silent by design; delete
`$XDG_CACHE_HOME/niri-recorder/thumbnails/` to force regeneration, and
check `ffmpeg -version`.

### In-window shortcut does nothing

Shortcuts only fire while the recorder window has focus (Wayland has no
portable global hotkey). Verify the GTK accelerator syntax in
`config.toml` (e.g. `<Control>r`); an invalid value logs a warning via
`tracing` and is treated as disabled. For a global shortcut, bind
`niri-recorder start|stop` in niri instead.

### Config warnings

A missing `config.toml` is silent — defaults apply. A malformed file or
a `default_fps` other than 24/30/60 → logged warning (`RUST_LOG=warn` or
a `tracing` env filter makes it visible) and defaults are used; the app
never refuses to start over config. `niri-recorder diagnostics` prints
the config path being read.
