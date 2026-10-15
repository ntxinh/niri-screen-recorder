# niri-recorder

Local-first screen recorder for Wayland/Niri on Fedora. Captures a full
output or a selected region to MP4/H.264 using `wf-recorder` — no audio,
no network, no portal prompts.

**Status: GUI (Plan B).** A bare `niri-recorder` launches the
GTK4/libadwaita window; every feature is also available from the
command line below. See `docs/ROADMAP.md` for phase status.

## Supported environment

| Requirement | Status |
|---|---|
| Fedora 44, native Wayland session | Verified on target |
| Niri compositor (dual output DP-3 + eDP-1, 1920×1080@60) | Verified on target |
| Intel CometLake-H UHD (iHD) | Verified; software encode only |
| Multi-monitor | Verified (`--output` on DP-3 + eDP-1, per-display capture) |
| Other compositors | Untested; do not assume Sway/Hyprland results apply |

See `docs/WAYLAND-NIRI-COMPATIBILITY.md` for the full verified/pending
matrix and `docs/CAPTURE-BACKENDS.md` for why `wf-recorder` was chosen
over `wl-screenrec` and the XDG portal.

## Features

- Record one full display (`--target display`, `--output NAME`)
- Record a screen region via `slurp` (`--target region`)
- Graceful stop: `niri-recorder stop` or Ctrl-C → SIGINT → finalized MP4
- Single-instance guard via a user-private pidfile
- Collision-safe filenames; in-progress files staged as `.part`
- `status` and `diagnostics` commands
- No audio capture, ever — the backend never passes an audio flag

Known limitations: variable frame rate output (wlr-screencopy damage
tracking), no hardware encoding (VAAPI broken for this on CometLake iHD),
`--output NAME` is not validated against detected outputs (a bad name
surfaces later as a wf-recorder exit — `capture ended early` / `stop
failed`, exit 3); slurp region geometry across two outputs unverified.

## Install dependencies (Fedora 44, verified package names)

```sh
sudo dnf install wf-recorder slurp ffmpeg
```

`ffmpeg` supplies `ffprobe`, used by `diagnostics` and for checking
recordings. `niri` itself must be installed and running — output
enumeration uses `niri msg --json outputs`.

## Development setup

```sh
mise install        # stable Rust toolchain (mise.toml)
make check          # fmt --check + clippy -D warnings + tests + build
cargo run -- status # run without installing
```

Install binary + desktop entry + icon to `~/.local`:

```sh
make install      # cargo install --path . --locked + data/ files; PREFIX overridable
make uninstall    # removes all three
```

Binary only (no desktop integration):

```sh
cargo install --path .   # installs to ~/.cargo/bin
```

## Usage

### GUI

Run `niri-recorder` with no arguments (or launch **Niri Recorder** from
your app launcher) to open the window: pick display or region, set
fps/quality, start/stop, and act on finished recordings. Settings are
shared with the CLI via the config file below.

### CLI

```text
niri-recorder start                          # record sole/first display (foreground)
niri-recorder start --target display --output DP-3
niri-recorder start --target region          # slurp region picker
niri-recorder stop                           # SIGINT the recorder from the pidfile
niri-recorder status                         # "recording pid=N" or "idle"
niri-recorder diagnostics                    # tool versions, outputs, backend caps
```

`start` runs in the foreground of its own process — there is no daemon.

### Niri keybindings

Bind start/stop in `~/.config/niri/config.kdl`, inside the `binds {}`
block (`spawn` takes the command and its arguments):

```kdl
Mod+Shift+R { spawn "niri-recorder" "start" "--target" "display"; }
Mod+Shift+S { spawn "niri-recorder" "stop"; }
```

If `Mod+Shift+R` is already bound (niri defaults use it for
`switch-preset-column-width-back`), pick a free key or rebind the
default — two binds cannot share the same key.

Exit codes: `0` ok (incl. `status` and cancelled region select),
`1` not recording / already recording / cannot signal, `2` backend
missing or CLI parse error, `3` capture failure.

## Configuration and output

Config: `~/.config/niri-recorder/config.toml` (all keys optional):

```toml
output_directory = "~/Videos/Screen Recordings"
default_target = "display"    # display | region
default_fps = 30              # 24 | 30 | 60 only
quality = "standard"          # standard | high
preferred_backend = "auto"    # only wf-recorder exists; auto resolves to it
```

Output: `screen-recording-YYYY-MM-DD_HH-MM-SS.mp4` in the output
directory (`-1`…`-99` appended on collision). In-progress captures are
written as `<name>.mp4.part` and renamed once the recorder has exited and
the file is non-empty; a non-zero exit keeps the file but is reported as
a failure instead of a clean save.

Runtime state: `$XDG_RUNTIME_DIR/niri-recorder/recorder.pid` (mode 0600;
falls back to `/tmp/niri-recorder-<uid>` without a runtime dir).

## Troubleshooting

Run `niri-recorder diagnostics` first — it reports niri, wf-recorder,
slurp and ffprobe presence, detected outputs, and backend capabilities.
Error-by-error guidance is in `docs/TROUBLESHOOTING.md`.

## Security and privacy

- Recording is fully local; no telemetry, accounts, or upload paths.
- Screen recordings capture whatever is on screen — including private
  notifications, passwords managers, and documents. Treat output files
  accordingly.
- The pidfile lives in a user-private runtime dir (0600); no sockets, no
  network listener, no root required or wanted.
- No audio is captured; nothing requests microphone access.

## Project layout

`src/ui` (GTK4/libadwaita window — bare `niri-recorder`) → `src/cli`
(clap front-end) → `src/app` (`Recorder` controller, state
machine, pidfile) → `src/capture` (`CaptureBackend` trait, `wf-recorder`
backend, managed subprocess) → `src/files` (output naming/finalization),
`src/config` (settings). Full picture in `DESIGN.md`.

## Roadmap

Plan A (CLI) and Plan B (GTK4/libadwaita GUI, packaging, multi-monitor
validation) are implemented. See `docs/ROADMAP.md` and
`docs/REQUIREMENTS.md` for per-requirement status.
