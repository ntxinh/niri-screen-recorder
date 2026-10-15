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
- `multiple displays; --output required` — pass
  `niri-recorder start --output DP-3` with one of the names from
  `diagnostics`.
- `cannot start while …` / `cannot stop while …` — internal state guard;
  a second start on the same process, or stop with nothing running.

### `niri-recorder: already recording (pid N)`

A live pidfile exists — the pidfile is the single-instance lock. Check
`niri-recorder status`; use `niri-recorder stop`, or Ctrl-C the `start`
process. If the file is stale it is removed automatically on next read,
so persisting output here means a real recorder is running. Exit 1.

### `not recording` (from `stop`, exit 1)

No pidfile or it was stale (dead pid / mismatched argv0 — cleaned up
automatically). Nothing to stop.

### `cannot signal pid N: …` (from `stop`, exit 1)

The pidfile pointed at a live process but SIGINT failed — typically a
permissions mismatch (recorder started as another user/root). Check `ps
-p N -o pid,user,comm`.

### `slurp` region problems

- Region mode but `slurp` missing → spawn error `capture failed: …`
  (exit 3). Fix: `sudo dnf install slurp`. Note `detect()` treats slurp
  as optional — region is the only mode that needs it.
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

`recorder exited unexpectedly: <stderr tail>` is not a standalone line —
it appears as the `<err>` inside `stop failed: <err>` when the child
exited cleanly but wrote an empty `.part` (the empty file is deleted;
nothing is claimed saved). The captured stderr tail usually names the
cause (bad output name, unsupported codec, protocol error).

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

### Config warnings

A missing `config.toml` is silent — defaults apply. A malformed file or
a `default_fps` other than 24/30/60 → logged warning (`RUST_LOG=warn` or
a `tracing` env filter makes it visible) and defaults are used; the app
never refuses to start over config. `niri-recorder diagnostics` prints
the config path being read.
