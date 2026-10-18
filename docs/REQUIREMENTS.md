# Requirements status

Functional requirements from spec §7 mapped to the implemented system
(CLI + GTK GUI + optional daemon).
Status: ✅ implemented+tested · ⚠️ implemented, manual verification
pending · 🔜 not implemented.

| FR | Requirement | Status | Notes |
|---|---|---|---|
| FR-01 | Full-display recording | ✅ | `start --target display [--output NAME]`; the sole enabled output is auto-selected, several outputs fall back to `last_output` when it matches, otherwise an explicit `--output` is required. `--output` is passed through unvalidated — a bad name surfaces as a wf-recorder exit (`capture ended early` / `stop failed`, exit 3). GUI has a display dropdown. |
| FR-02 | Region recording | ⚠️ | `start --target region` runs `slurp`, parses `x,y WxH` (negative coords ok, zero-size/garbage rejected), cancel → clean `Idle`; missing `slurp` → actionable error; Ctrl-C/stop during `slurp` cancels without spawning. Interactive slurp itself remains a manual-verification item. |
| FR-03 | Workspace independence | ✅ | Capture lives in the `start` process, the GUI worker, or the daemon worker — never tied to a window; workspace switching during capture verified uninterrupted. GUI close-while-recording prompt implemented. |
| FR-04 | Multi-monitor support | ⚠️ | Outputs enumerated via `niri msg --json outputs`; one output at a time. GUI selects from the enabled list and remembers `last_output`; the CLI/daemon reuse it for bare starts. Hot-plug/disconnect untested. Combined-canvas capture is intentionally out of scope. |
| FR-05 | Audio disabled | ✅ | Backend never emits `-a`/`--audio` (asserted in `tests/backend_tests.rs`); every probed output has a single video stream; no audio controls exist anywhere. |
| FR-06 | Recording controls | ✅ | start/stop/cancel; stop finalizes and only then reports `saved → <path>`; single-instance guard is an atomic `O_EXCL` pidfile claim — duplicate starts exit 1 (`already recording`), stale pidfiles are replaced. GUI: primary button, elapsed timer, in-window shortcuts; optional `niri-recorder daemon` for forwarded control. |
| FR-07 | Output files | ✅ | Configurable directory, `screen-recording-YYYY-MM-DD_HH-MM-SS.mp4`, `-1`…`-99` collision suffixes, `.part` staging renamed after the recorder exits + non-empty check (a non-zero exit keeps the file but reports the failure). GUI: recent list with thumbnails, Open Folder / Play. |
| FR-08 | Recording quality | ✅ | `default_fps` 24/30/60 (validated); `quality = standard` / `high`. `encoder` selects `auto`/`software`, `hardware` (probed preference, software fallback), or an explicit H.264 encoder. Hardware encoding is **unverified** on CometLake iHD; the default stays software. |
| FR-09 | Settings | ✅ | `~/.config/niri-recorder/config.toml`: `output_directory`, `default_target`, `default_fps`, `quality`, `preferred_backend`, `last_output` (auto-written by GUI and CLI/daemon starts with `--output`), `encoder`, `countdown_seconds`, `start_shortcut`, `stop_shortcut`, `cleanup_enabled`, `cleanup_max_files`, `cleanup_max_age_days`. Missing → defaults silently; unreadable/malformed → defaults + warning; `~` expanded at use site. |
| FR-10 | Status and diagnostics | ✅ | `status` (recording pid / idle, daemon-aware), `diagnostics` (niri + tool versions, outputs, backend capabilities, daemon socket/state, config path). GUI shows the internal `AppState` label and elapsed time. No telemetry. |

## Phase 6 additions (post-MVP)

- Configurable in-window shortcuts (Wayland has no portable global
  hotkey; niri binds remain the global mechanism).
- Countdown before GUI capture (0/3/5/10 s, cancellable).
- Thumbnails for the recent-recordings list (cached ffmpeg extraction).
- Opt-in retention by file count and/or age.
- Encoder presets / hardware preference with software fallback.
- Foreground daemon + Unix-socket CLI control.
- DankMaterialShell bar/Control Center plugin (loads in DMS 1.6; bar
  click behavior still manual).
- Cursor-visibility control is **not implemented** — wf-recorder 0.6.0
  has no cursor flag.

## Non-goals confirmed out of scope (spec §2 / plan)

Audio in any form, streaming/upload, screen sharing integration,
compositor changes, multi-monitor canvas merging, network IPC.
