# Wayland / Niri compatibility

Verified means: exercised on the target machine and observed to work —
Fedora 44, native Wayland (`wayland-1`), Niri, single output DP-3
(Dell P2419H 1920×1080@60), Intel CometLake-H UHD (iHD 26.2.4), wf-recorder
0.6.0. Spike evidence: spec §22 / `docs/CAPTURE-BACKENDS.md`.

## Verified matrix

| Capability | Status | Evidence |
|---|---|---|
| Full-display capture (`-o <output>`) | ✅ | 1920×1080 H.264 MP4, playable |
| Region capture (`-g "x,y WxH"`) | ✅ | Exact 800×600 from `-g "100,100 800x600"`; slurp geometry taken verbatim |
| Workspace switching during capture | ✅ | 4 `niri msg action focus-workspace` switches in a 16.4s capture → 494 frames, uninterrupted |
| SIGINT stop → finalized file | ✅ | moov atom written; ffprobe parses output |
| SIGTERM → SIGKILL stop escalation | ✅ (unit) | `tests/process_tests.rs` fake-child escalation; real-child SIGINT path verified |
| No audio in output | ✅ | ffprobe: single `video` stream; backend never emits `-a` |
| Output enumeration via `niri msg --json outputs` | ✅ (unit + spike) | DP-3 detected; empty-list handling unit-tested |
| Collision-safe output names / `.part` staging | ✅ (unit) | `tests/filename_tests.rs` |
| SIGINT capture-survival when child is a different process | ✅ (unit) | pidfile+SIGINT control model, `tests/controller_tests.rs` |

## Pending manual verification

These could not be exercised on the single-output spike machine or
headlessly. Each must be run on a real Niri session before the matching
claim is made (see `docs/TESTING.md` for the full acceptance list).

| Item | Why pending |
|---|---|
| Interactive `slurp` region pick | Could not be driven headlessly; only its `x,y WxH` output contract was verified against wf-recorder `-g` |
| Multi-monitor capture | Spike machine has one output; `--output NAME` implemented but untested on hardware |
| Region selection on each monitor / spanning outputs | Same; negative-coordinate layouts are parsed (`tests/geometry_tests.rs`) but untested live |
| Mixed resolutions / scale factors | Untestable on target |
| Monitor disconnect during recording | Untestable on target |
| `wf-recorder -r 24` / `-r 60` real captures | Config validates these values; only 30 was recorded on target |
| VFR vs CFR playback on picky players | Output is damage-tracked VFR (~30 fps avg); `-D` not used |

## Session requirements

- Native Wayland session under Niri — `wf-recorder` uses the
  wlr-screencopy protocol; no XWayland capture path exists.
- Do not run under X11 or as root.
- Other wlroots compositors (Sway, Hyprland) may work via the same
  protocol but are **untested** — per AGENTS.md, support elsewhere is not
  evidence of support on Niri, and the converse applies too.
