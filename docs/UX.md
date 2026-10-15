# UX — CLI contract (Plan A)

The user interface is the `niri-recorder` binary. A GTK4/libadwaita GUI
(spec §8) is planned as Plan B and will drive the same `Recorder`
controller — none exists yet.

## Commands

```text
niri-recorder                                        bare → full help, exit 0
niri-recorder start [--target display|region] [--output NAME]
niri-recorder stop
niri-recorder status
niri-recorder diagnostics
```

`start` blocks in the foreground for the whole recording — this is the
control model (spec §23.2), not a missing feature. Ctrl-C it or run
`niri-recorder stop` from anywhere.

## Observable states via `status`

`status` is pidfile-driven, so externally visible state is binary:

- `recording pid=N` — a `start` process owns the pidfile and is alive
- `idle` — no recording (stale pidfiles are removed on read)

The internal machine (`AppState`: Idle, SelectingRegion, Starting,
Recording, Stopping, Completed, Failed) is finer-grained but only
observable through `start`'s own stdout/stderr while it runs.

## Message contract (stdout / stderr)

| Condition | Stream | Text | Exit |
|---|---|---|---|
| start begins | stdout | `recording → <path>.mp4.part` | — |
| region pick cancelled | stdout | `cancelled` | 0 |
| clean stop (Ctrl-C or `stop`) | stdout | `saved → <path>.mp4` | 0 |
| child died mid-capture (poll loop) | stderr | `recorder child exited unexpectedly` then `capture ended early; saved <path>` | 3 |
| stop/finalize failed (incl. child wrote empty `.part` → `recorder exited unexpectedly: <stderr tail>`) | stderr | `stop failed: <err>` (+ failed reason) | 3 |
| second `start` while recording | stderr | `already recording (pid N)` | 1 |
| `stop` with nothing running | stdout | `not recording` | 1 |
| `stop` signal sent | stdout | `stop requested (pid N)` | 0 |
| backend missing | stderr | `missing dependency: wf-recorder (install: sudo dnf install wf-recorder)` | 2 |
| clap parse error / `--help` | stderr/stdout | usage text | 2 / 0 |
| `status` | stdout | `recording pid=N` / `idle` | 0 |
| `diagnostics` | stdout | tool versions, outputs, backend caps, config path | 0 |

Exit codes: **0** ok · **1** not recording / already recording / cannot
signal · **2** backend missing or CLI parse error · **3** capture failure.

## Behavior notes

- Output paths print the configured `output_directory` — `~` is
  expanded, but a relative directory is printed relative, not resolved.
- `start` takes 100 ms worst case to notice a signal (poll loop).
- `stop` is fire-and-forget: it confirms the signal was delivered, not
  that the file finalized — finalization is reported by the `start`
  process that owns the capture.
- Keyboard-driven UX comes from Niri binds invoking `start`/`stop`
  directly (example in README); no global-shortcut machinery is
  registered by the app.
- Logging goes to stderr via `tracing`; `RUST_LOG` controls verbosity.
