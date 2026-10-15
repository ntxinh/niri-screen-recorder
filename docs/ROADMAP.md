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

## Next

- **Phase 4 — Reliability and multi-monitor validation**: run every
  pending item in `docs/WAYLAND-NIRI-COMPATIBILITY.md` on real
  multi-monitor hardware; disk-full and unexpected-exit hardening;
  no-audio validation; diagnostics polish.

## Remaining phases (spec §18)

- **Phase 6 — Optional enhancements** (post-MVP only): DMS integration,
  configurable shortcuts, background daemon with robust CLI control,
  recording history with thumbnails, opt-in cleanup rules, countdown and
  cursor controls if the backend supports them, selectable encoder
  presets / hardware-encoding preference.

## Deliberately not planned

Audio capture (hard non-goal — AGENTS.md forbids it), streaming or
upload, network IPC, merging multiple monitors into one canvas unless a
backend supports it natively.
