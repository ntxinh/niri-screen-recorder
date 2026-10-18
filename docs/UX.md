# UX — CLI and window contract

The `niri-recorder` binary is both CLI and GUI: bare invocation opens
the GTK4/libadwaita window, subcommands drive the same
`app::controller::Recorder`. An optional `daemon` subcommand adds a
long-lived control service; `start`/`stop`/`status` use it automatically
when it is running (`docs/DAEMON.md`).

## Commands

```text
niri-recorder                                        bare → GUI
niri-recorder start [--target display|region] [--output NAME]
niri-recorder stop
niri-recorder status
niri-recorder diagnostics
niri-recorder daemon                                 foreground service
```

Without a daemon, `start` blocks in the foreground for the whole
recording (spec §23.2). With a daemon, `start` forwards the request and
returns once capture has begun (region starts still wait for the drag);
`stop` waits for finalize and prints the saved path.

## Observable states via `status`

`status` asks the daemon first, then the pidfile:

- `recording pid=N` — either the daemon or a foreground/GUI recorder is
  live
- `idle` — no recording (stale pidfiles are removed on read)

The internal machine (`AppState`: Idle, SelectingRegion, Starting,
Recording, Stopping, Completed, Failed) is finer-grained; the daemon
also exposes `starting`/`stopping` in its socket response, but `status`
prints only the binary form.

## Message contract (stdout / stderr)

| Condition | Stream | Text | Exit |
|---|---|---|---|
| start begins (foreground) | stdout | `recording → <path>.mp4.part` | — |
| start forwarded to daemon | stdout | `recording → <path>.mp4.part` / `recording pid=N` | 0 |
| region pick cancelled | stdout | `cancelled` | 0 |
| clean stop (Ctrl-C, `stop`, daemon stop) | stdout | `saved → <path>.mp4` | 0 |
| daemon stop with no capture | stdout | `not recording` (pidfile fallback) | 1 |
| child died mid-capture (poll loop) | stderr | `recorder child exited unexpectedly` then `capture ended early; saved <path>` | 3 |
| stop/finalize failed (incl. child wrote empty `.part` → `recorder exited unexpectedly: <stderr tail>`) | stderr | `stop failed: <err>` (+ failed reason) | 3 |
| second `start` while recording | stderr | `already recording (pid N)` | 1 |
| `stop` with nothing running | stdout | `not recording` | 1 |
| `stop` signal sent (no daemon) | stdout | `stop requested (pid N)` | 0 |
| backend missing | stderr | `missing dependency: wf-recorder (install: sudo dnf install wf-recorder)` | 2 |
| daemon protocol/data error | stderr | `invalid daemon response: …` / `cannot … daemon` | 3 |
| clap parse error / `--help` | stderr/stdout | usage text | 2 / 0 |
| `status` | stdout | `recording pid=N` / `idle` | 0 |
| `diagnostics` | stdout | tool versions, outputs, backend caps, daemon state, config path | 0 |

Exit codes: **0** ok · **1** not recording / already recording / cannot
signal · **2** backend missing or CLI parse error · **3** capture failure.

## Window behavior

- Start/Stop primary button; selectors disabled during transitions.
- Configurable countdown (0/3/5/10 s): status shows `Starting in Ns…`,
  the primary button becomes **Cancel countdown**, and Esc or the stop
  shortcut cancels. Selections are frozen when the countdown starts.
- In-window shortcuts (`<Control>r` / `<Control>s` by default, GTK
  syntax in `config.toml`, empty disables). They fire only while the
  window has focus — Wayland exposes no portable global hotkey. Use niri
  binds (`README.md`) for a global shortcut.
- Recent recordings show `ffmpeg`-extracted thumbnails; a cold cache
  shows an empty preview until the worker finishes. A missing `ffmpeg`
  only disables thumbnails.
- Retention is off by default; when enabled it runs after a clean save
  and only touches finished `screen-recording-*.(mp4|webm)` files.

## Behavior notes

- Output paths print the configured `output_directory` — `~` is
  expanded, but a relative directory is printed relative, not resolved.
- `start` takes 100 ms worst case to notice a signal (poll loop).
- Without a daemon, `stop` is fire-and-forget: it confirms the signal was
  delivered, not that the file finalized — finalization is reported by
  the `start` process that owns the capture. With a daemon, `stop` waits
  and reports the saved file (or the failure), never just "requested".
- Logging goes to stderr via `tracing`; `RUST_LOG` controls verbosity.
- Cleanup and hardware-encoder fallbacks log via `tracing` (stderr), not
  stdout, so scripts parsing the contract above are unaffected.
