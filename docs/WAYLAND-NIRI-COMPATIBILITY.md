# Wayland / Niri compatibility

Verified means: exercised on the target machine and observed to work —
Fedora 44, native Wayland (`wayland-1`), Niri, dual output: DP-3
(Dell P2419H 1920×1080@60) at (0,0) and eDP-1 (LG panel 1920×1080@60)
at (−1920,0) — a negative-coordinate layout. Intel CometLake-H UHD (iHD
26.2.4), wf-recorder 0.6.0. Spike evidence: spec §22 /
`docs/CAPTURE-BACKENDS.md`.

## Verified matrix

| Capability | Status | Evidence |
|---|---|---|
| Full-display capture (`-o <output>`) | ✅ | 1920×1080 H.264 MP4, playable, on both DP-3 and eDP-1 |
| Multi-monitor capture | ✅ | `--output eDP-1` → 1920×1080, 62 frames / 2.03 s stream in a ~4 s capture window; `--output DP-3` → 85 frames in ~3 s. Both finalized and ffprobe-clean |
| Negative-coordinate output layout | ✅ | eDP-1 at logical (−1920,0), DP-3 at (0,0); whole-display capture of either works. Region drags on the left monitor produce negative slurp origins — parsed (`tests/geometry_tests.rs`), but see pending row for live-drag verification |
| Multi-display without `--output` | ✅ (fallback + error path) | Uses `last_output` when it matches an enabled output (written by the GUI and by CLI starts with `--output`); otherwise `backend unsupported: multiple displays; --output required`, exit 3 |
| Disabled connector filtering | ✅ | eDP-1 `niri msg output off` → `diagnostics` lists only `DP-3` and `start --target display` records DP-3 without `--output`. Re-enabled → both listed, `--output` required again |
| Region capture (`-g "x,y WxH"`) | ✅ | Exact 800×600 from `-g "100,100 800x600"`; slurp geometry taken verbatim |
| Workspace switching during capture | ✅ | ~20 `focus-workspace` alternations across two workspaces during a 60 s DP-3 capture → 1817 frames, 60.53 s duration, ffprobe-clean, 137 KB |
| Long capture (60 s) | ✅ | 60.53 s duration / 1817 frames (~30 fps while damaged), playable |
| SIGINT stop → finalized file | ✅ | moov atom written; ffprobe parses output. wf-recorder takes ~3–4 s to flush after SIGINT before the `.part` renames |
| SIGTERM → SIGKILL stop escalation | ✅ (unit) | `tests/process_tests.rs` fake-child escalation; real-child SIGINT path verified |
| No audio in output | ✅ | ffprobe: single `video` stream; backend never emits `-a` |
| Output enumeration via `niri msg --json outputs` | ✅ | Both outputs detected; disabled connectors filtered (`logical: null`) |
| Collision-safe output names / `.part` staging | ✅ (unit + live) | `tests/filename_tests.rs`; every live capture staged `.part` then renamed |
| `stop` → SIGINT → `start` handler → graceful finalize | ✅ (live) | Verified repeatedly on target; automated coverage is unit-level — `tests/controller_tests.rs` exercises pidfile staleness via `kill(pid,0)`/`comm` |
| Output filesystem full at start | ✅ (error path) | 0-byte-free tmpfs output dir → wf-recorder `Failed to write file header / No space left on device` surfaced in stderr tail, empty `.part` deleted, exit 3 |
| GUI multi-output + record | ✅ | Dropdown enumerated eDP-1 (with DP-3 off); GUI start/stop produced a 12.9 s eDP-1 capture (388 frames), playable |
| wf-recorder `-r 30` real captures | ✅ | All live captures above ran `-r 30` |
| Encoder probe (`ffmpeg -encoders`) | ✅ | Parsed encoder list drives the settings dialog and `diagnostics`; probe unit-tested |
| Opt-in retention | ✅ (unit) | `tests/retention_tests.rs`: only finished recordings, count/age rules, never `.part` |
| Thumbnail generation | ✅ (unit) | `ffmpeg` extracts a frame from a synthetic clip; cache key follows mtime/size |
| Daemon control socket | ✅ (automated) | `tests/daemon_tests.rs`: 0600 socket, status/stop protocol, malformed requests, second-daemon refusal, shutdown |
| Daemon forwarded capture | ✅ (live) | `daemon` + `start --output DP-3` returned immediately; `status` = `recording pid=N`; `stop` saved H.264 2560×1440 / 1.97 s; second `stop` = `not recording` (exit 1) |
| GUI startup with Phase 6 UI | ✅ | 4 s launch under niri: no stderr, thumbnails written (160×90 PNG in the XDG cache) |
| GUI countdown | ✅ (AT-SPI) | Start → `Cancel countdown` + `Starting in 2s…`; Cancel returned to `Idle` with no file/pidfile. A second run let 3 s elapse → `Stop Recording` / `Recording · 00:01`, stop → `Completed`, H.264 2560×1440 / 1.47 s |
| Settings dialog (Phase 6 rows) | ✅ (AT-SPI) | Opened from the gear button; Countdown/Shortcuts/Encoder/Retention groups rendered with the saved values; no panic or stderr |
| DMS plugin load | ✅ (partial) | DMS 1.6: `plugins list` → `niriRecorder [disabled]`, reload → `PLUGIN_RELOAD_SUCCESS` / `[loaded]`, log `DankBar: Plugin loaded: niriRecorder`; bar rendering/click still manual |

## Pending manual verification

These could not be driven headlessly — slurp needs a real pointer drag and
this session has no injectable input (no ydotool/wtype, no uinput access,
no passwordless sudo, AT-SPI key events rejected under niri). Run each on
the machine before claiming support:

| Item | Manual steps |
|---|---|
| Interactive `slurp` region pick | `niri-recorder start --target region` → drag on screen → wait 3 s → `niri-recorder stop`. Expect `saved → <file>`, ffprobe shows the dragged dimensions |
| Region on the negative-coordinate monitor | Same drag but on eDP-1 (at −1920,0): slurp prints e.g. `-1500,100 640x480`. wf-recorder accepts it — confirm the file dimensions match |
| Region spanning both outputs | Drag from eDP-1 across the boundary into DP-3 — slurp may report e.g. `-200,300 900x400`; confirm wf-recorder captures the span or fails honestly |
| Esc-cancel region selection | `start --target region`, press Esc → `cancelled`, exit 0, no file |
| Mixed resolutions / scale factors | Untestable on target (both outputs 1920×1080@1.0) |
| Monitor disconnect during recording | Physically disconnect DP-3 mid-capture → expect clean stop, partial preserved |
| `wf-recorder -r 24` / `-r 60` real captures | Config validates these values; only 30 was recorded on target |
| VFR vs CFR playback on picky players | Output is damage-tracked VFR (~30 fps avg); `-D` not used. A static eDP-1 screen produced a valid 1-frame file — check your player handles near-empty frame counts |
| In-window shortcuts | Focus the window, press `Ctrl+R` / `Ctrl+S` (keyboard input is not injectable on niri); rebind via `start_shortcut`/`stop_shortcut` and confirm invalid values fall back to disabled with a warning. Esc-cancel of a countdown is wired to the same path |
| Thumbnail rendering in a row | Automated generation is verified; check that a fresh recording's preview appears in the list after it is saved |
| DMS bar rendering and clicks | Add the widget to the bar and toggle recording from it (Control Center tile too); loading is verified |
| Daemon forwarded capture | `niri-recorder daemon` in one terminal, `start --output DP-3` in another → returns immediately; `stop` prints `saved → …` and ffprobe parses the file |
| Hardware encoding | Set `encoder = "hardware"` (or `h264_vaapi`) and record. VAAPI has no H.264 encode profile on this CometLake iHD machine, so expect a start failure reported via stderr; verify the software fallback by unsetting it |


## Known caveats

- **Cursor visibility is not controllable.** wf-recorder 0.6.0 exposes no
  cursor flag; the compositor's screencopy decides. The spec's
  "if supported by the backend" condition is not met, so no control is
  offered.
- **Hardware encoding is unverified.** `ffmpeg -encoders` lists VAAPI/QSV
  encoders on this machine, but the Phase 0 spike showed no usable H.264
  encode profile on CometLake iHD. `encoder = "auto"` therefore stays on
  `libopenh264`; selecting `hardware` may fail at start and falls back
  only when ffmpeg lists nothing, not when the encoder fails at runtime.

- **Damage-tracked VFR on a static screen is sparse.** An idle eDP-1
  captured 1 frame in 3 s — a valid but near-empty file. Any on-screen
  change (mouse, clock, window focus) produces ~30 fps.
- **wf-recorder buffers muxer output.** `.part` growth is lumpy; the mp4
  flushes at finalize. If the output filesystem fills mid-capture,
  wf-recorder may still exit 0 and the file is truncated — the recorder
  can only report what the child reports. Check free space before long
  captures; a suspiciously small file means the disk filled.
- **wf-recorder needs ~8.3 MB headroom for its raw-frame buffer** — a
  `ulimit -f` under that fails at spawn with `File too large` (surfaced,
  exit 3).
- wf-recorder keeps running ~3–4 s after SIGINT while it flushes; this
  is normal, not a hang.

## Session requirements

- Native Wayland session under Niri — `wf-recorder` uses the
  wlr-screencopy protocol; no XWayland capture path exists.
- Do not run under X11 or as root.
- Other wlroots compositors (Sway, Hyprland) may work via the same
  protocol but are **untested** — per AGENTS.md, support elsewhere is not
  evidence of support on Niri, and the converse applies too.
