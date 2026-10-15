# Agent rules for niri-recorder

These rules apply to every contributor, human or automated. They are
normative; the spec (`docs/superpowers/specs/2026-10-15-screen-recorder-design.md`)
§12 is the source.

## Before changing anything

- Read `README.md`, `DESIGN.md`, and the relevant `docs/` file before
  making architectural changes.
- Confirm actual Niri/backend compatibility; do not infer it. Support on
  Sway or Hyprland is not evidence of support on Niri. Backend claims in
  docs must cite observed results (`docs/CAPTURE-BACKENDS.md`).

## Non-negotiables

- Prefer existing tested capture utilities (`wf-recorder`, `slurp`) over
  custom low-level capture code.
- Never add audio capture: no `-a`/audio flags, no mic permission
  requests, no audio controls. `tests/backend_tests.rs` asserts this.
- Never execute shell command strings assembled from user input. Spawn
  subprocesses with argv arrays only (see `src/capture/process.rs::spawn`).
- Keep UI logic separate from capture process management. The CLI must
  drive the same `app::controller::Recorder` a future GUI uses — no
  parallel capture path in front-end code.

## Before declaring a task complete

- Run `make check` (fmt --check, clippy `-D warnings`, all tests, build).
  Do not claim checks passed unless they ran and results were inspected.
- Update documentation whenever behavior, commands, configuration, or
  architecture changes — docs in this repo describe what is built, not
  what is planned.
- Record known platform limitations honestly in
  `docs/WAYLAND-NIRI-COMPATIBILITY.md`; mark unverified scenarios as
  manual-verification items, never as passing.
- Avoid unrelated refactors and unnecessary dependencies.
