# Requirements status

Functional requirements from spec §7 mapped to Plan A (CLI) reality.
Status: ✅ implemented+tested · ⚠️ implemented, manual verification
pending · 🔜 deferred to Plan B (GUI) or later phase.

| FR | Requirement | Status | Notes |
|---|---|---|---|
| FR-01 | Full-display recording | ⚠️ | `start --target display [--output NAME]` works; single output auto-selected, multiple outputs → explicit `--output` required (actionable error). `--output` is passed through unvalidated — a bad name doesn't fail the spawn; wf-recorder exits non-zero later and the run reports `capture ended early; saved <path>` or `stop failed: recorder exited unexpectedly: <tail>` (exit 3). Display picker UI and all-displays canvas deferred. Verified on single-output target only. |
| FR-02 | Region recording | ⚠️ | `start --target region` runs `slurp`, parses `x,y WxH` (negative coords ok, zero-size/garbage rejected), cancel → clean `Idle`; missing `slurp` → `missing dependency: slurp (install: sudo dnf install slurp)`; Ctrl-C during `slurp` cancels without spawning. Interactive slurp itself unverified on target (headless spike). |
| FR-03 | Workspace independence | ✅ | Capture lives in the foreground `start` process, not tied to any window; workspace switching during capture verified uninterrupted (494 frames / 16.4 s). Exit-while-recording prompt deferred (no GUI). |
| FR-04 | Multi-monitor support | ⚠️ | Outputs enumerated via `niri msg --json outputs`; `--output` selects one. Multi-monitor hot-plug/disconnect untested — single-output hardware. Combined-canvas capture intentionally out of scope. |
| FR-05 | Audio disabled | ✅ | Backend never emits `-a`/`--audio` (asserted in `tests/backend_tests.rs`); spike confirmed output has a single video stream; no audio controls exist anywhere. |
| FR-06 | Recording controls | ✅ | start/stop/cancel; stop finalizes and only then reports `saved → <path>`; single-instance guard is an atomic `O_EXCL` pidfile claim — duplicate starts exit 1 (`already recording`), stale pidfiles are replaced; elapsed-time display deferred to GUI. |
| FR-07 | Output files | ✅ | `~/Videos/Screen Recordings` default (configurable), `screen-recording-YYYY-MM-DD_HH-MM-SS.mp4`, `-1`…`-99` collision suffixes, `.part` staging renamed after the recorder exits + non-empty check (a non-zero exit keeps the file but reports the failure); final path printed. "Open folder / play" actions deferred to GUI. |
| FR-08 | Recording quality | ✅ | `default_fps` 24/30/60 (validated); `quality = standard` (encoder defaults) / `high` (`-p b=10000000 -p rc_mode=bitrate -p allow_skip_frames=true`); codec/container auto-matched (mp4→libopenh264, webm→libvpx-vp9). No hardware encoding on target (VAAPI broken on CometLake iHD). |
| FR-09 | Settings | ✅ | `~/.config/niri-recorder/config.toml`: `output_directory`, `default_target` (honored when `--target` is omitted), `default_fps`, `quality`, `preferred_backend` (`auto`/`wf-recorder` only — other values exit 2, spec §23.6); missing → defaults silently; unreadable/malformed → defaults + warning; `~` expanded at use site. "Last selected display" and filename-format options not implemented (GUI settings scope). |
| FR-10 | Status and diagnostics | ✅ | `status` (recording pid / idle), `diagnostics` (niri + tool versions, outputs, backend capabilities, config path). State display is `status`'s pid/idle line; full state names (SelectingRegion, Stopping…) exist internally (`AppState`) but aren't surfaced per-invocation — the pidfile model only distinguishes recording/not. No telemetry. |

## Non-goals confirmed out of scope (spec §2 / plan)

GUI (Plan B), audio in any form, streaming/upload, screen sharing
integration, compositor changes, multi-monitor canvas merging.
