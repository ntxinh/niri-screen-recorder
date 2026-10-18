# Daemon — robust CLI control

`niri-recorder daemon` is an optional foreground service that owns the
capture process. Without it, `start` is itself the recorder (see
`DESIGN.md`); with it, `start`/`stop`/`status` are control commands.

```sh
niri-recorder daemon          # foreground; systemd user unit recommended
niri-recorder start           # forwarded when the daemon is running
niri-recorder stop            # waits for finalize, prints the saved path
niri-recorder status          # daemon first, then pidfile
```

If no daemon is listening, all three fall back to the pre-Phase-6
pidfile/SIGINT behavior, so the CLI works either way.

## Systemd user unit

`data/niri-recorder-daemon.service` is provided but not installed
automatically:

```sh
mkdir -p ~/.config/systemd/user
cp data/niri-recorder-daemon.service ~/.config/systemd/user/
systemctl --user import-environment WAYLAND_DISPLAY
systemctl --user enable --now niri-recorder-daemon
```

Import `WAYLAND_DISPLAY` whenever the session starts (niri does this in
its own startup config/portal setup); the daemon needs it only when a
capture actually starts. `Restart=on-failure` restarts the service after
a crash; a finalizing recording is not resumed.

## Socket and security

- Path: `$XDG_RUNTIME_DIR/niri-recorder/control.sock`
  (`/tmp/niri-recorder-<uid>` when no runtime dir exists).
- The directory is forced to `0700` and refused when foreign-owned; the
  socket is `0600`. Any process running as your user can control it —
  that is the same trust boundary as the pidfile.
- No network listener and no root.

## Protocol

One newline-delimited JSON request per connection, one response line:

```json
{"cmd":"start","target":"display","output":"DP-3","fps":30,"quality":"standard"}
{"cmd":"stop"}
{"cmd":"status"}
```

```json
{"ok":true,"state":"recording","code":0,"pid":1234,"part":"/path/x.mp4.part"}
{"ok":true,"state":"idle","code":0,"saved":"/path/x.mp4"}
{"ok":false,"state":"failed","code":1,"error":"already recording (pid 1234)"}
```

`code` is the CLI exit code (0 ok, 1 state conflict, 2 backend/parse,
3 capture failure). `state` is one of `idle`, `starting`, `recording`,
`stopping`, `completed`, `failed`.

Requests are handled on their own connection threads; capture runs on a
single worker thread. `status` is answered from a shared snapshot and
stays responsive while a region selection is blocking on `slurp`;
`stop` also sets the recorder's cancel flag so it aborts a pending slurp.

## Single-instance and pidfile

The daemon claims the same pidfile as a foreground recorder or the GUI
when it starts a capture. Consequences:

- A GUI/foreground recording blocks a daemon `start` (`already
  recording (pid N)`), and vice versa.
- `stop` from the daemon works only for its own capture; an idle daemon
  answers code 1 and the CLI falls back to the pidfile, so a GUI
  recording can still be stopped.
- If the daemon dies mid-recording, its pidfile is stale and the
  foreground `stop` path cleans it up as before.

Retention (`files::retention`) runs after a successful daemon save,
identically to the CLI and GUI.

## Verified

- Socket lifecycle, permissions, protocol errors, idle `status`, second
  daemon refusal, shutdown cleanup: automated
  (`tests/daemon_tests.rs`).
- Forwarded `start`/`stop` against a live capture (2026-10-18, target
  machine, DP-3): `start --output DP-3` returned immediately,
  `status` reported `recording pid=N`, `stop` printed
  `saved → …mp4` and ffprobe reported H.264 2560×1440, 1.97 s. A second
  `stop` answered `not recording` (exit 1), and SIGTERM shut the daemon
  down cleanly with the socket removed.
- `status`/`stop` while a region selection is blocking on `slurp` is a
  manual-verification item (no input injection available here).
