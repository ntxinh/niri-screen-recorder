# Roadmap

Spec §18 phases mapped to reality. Plan A (core recorder + CLI) covers
Phases 0–2; Plan B is the GUI.

## Done

- **Phase 0 — Capture feasibility spike** ✅ wf-recorder + slurp chosen
  on target hardware; gate scenarios passed; evidence in spec §22 and
  `docs/CAPTURE-BACKENDS.md`.
- **Phase 1 — Repository foundation** ✅ Rust project, `mise.toml`,
  `Makefile`, `AGENTS.md`, `README.md`, `DESIGN.md`, `/docs`, fmt/clippy
  gates.
- **Phase 2 — Capture controller and CLI** ✅ state machine, backend
  abstraction, managed subprocess, display + region capture, stop/cancel,
  output naming + finalization, `status`/`diagnostics`, fake-child
  testing.
- **Phase 3 — Minimal GUI** ✅ GTK4/libadwaita window over the `Recorder`
  controller: capture mode, display selector, fps/quality settings,
  start/stop, state + elapsed time, final-file actions,
  exit-while-recording prompt, settings dialog, recent-recordings list.
- **Phase 5 — Packaging and desktop integration** ✅ `make install` /
  `make uninstall` (binary + desktop entry + icon to `~/.local`),
  launcher verified, Niri keybinding examples in README. RPM skipped —
  paths still stabilizing.
- **Phase 6 — Optional enhancements** ✅ (post-MVP scope):
  - configurable in-window shortcuts (`start_shortcut`/`stop_shortcut`),
  - countdown before capture (0/3/5/10 s, cancellable),
  - recording-history thumbnails (cached `ffmpeg` extraction),
  - opt-in cleanup rules (`cleanup_enabled`, count/age),
  - encoder presets and hardware preference with software fallback,
  - foreground daemon with Unix-socket CLI control
    (`niri-recorder daemon`; `docs/DAEMON.md`),
  - DankMaterialShell bar/Control Center plugin in `data/dms-plugin/`
    (manifest discovery and QML load verified in DMS 1.6; bar rendering
    and clicks remain manual checks — `docs/DMS.md`).

  Cursor visibility is **not implemented**: wf-recorder 0.6.0 has no
  cursor flag, so the spec's "if supported by the backend" condition is
  not met. Hardware encoding is selectable but unverified on CometLake
  iHD; the default stays software.

## Next

- **Phase 4 — Reliability and multi-monitor validation**: run every
  pending item in `docs/WAYLAND-NIRI-COMPATIBILITY.md` on real
  multi-monitor hardware; disk-full and unexpected-exit hardening;
  no-audio validation; diagnostics polish.

## Remaining phases (spec §18)

None — Phases 0–6 are implemented. Phase 4's manual-verification items
remain open (see `docs/WAYLAND-NIRI-COMPATIBILITY.md`).

## Deliberately not planned

Audio capture (hard non-goal — AGENTS.md forbids it), streaming or
upload, network IPC, merging multiple monitors into one canvas unless a
backend supports it natively.
