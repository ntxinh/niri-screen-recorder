# niri-recorder GUI + Packaging — Implementation Plan (Plan B)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A GTK4/libadwaita window driving the existing `Recorder` controller — target/display/fps/quality selectors, start/stop with elapsed timer, post-record actions, recent recordings — plus desktop packaging (.desktop file, icon) and the remaining reliability items.

**Architecture:** `niri-recorder` with no subcommand launches the GUI (when a Wayland display is present); subcommands remain the CLI. The GUI runs on the GTK main thread; `Recorder` calls that block (slurp selection, child wait) run on a worker thread communicating over `glib` channels — the recorder's lifetime stays independent of window visibility/focus per spec FR-03. Closing while recording prompts Stop-and-exit or Cancel (approved: no detached-recording mode in MVP).

**Tech Stack:** gtk4-rs + libadwaita-rs (versions compatible with installed system libs — implementer resolves via `cargo add` after `pkg-config --modversion gtk4 libadwaita-1` confirms versions), existing crate internals unchanged.

**Spec:** `docs/superpowers/specs/2026-10-15-screen-recorder-design.md` — §8 UX layout, FR-01/03/06/07/08/09.

## Global Constraints

- Plan A invariants carry forward: argv-only spawns, no audio flags ever, `.part` staging, pidfile model, state machine is the single source of truth.
- GUI must not break `cargo test` in headless environments: **all GUI tests must be `#[cfg]`-gated or live in the binary and run only under Wayland**; unit-testable logic stays in the lib.
- GUI widgets map 1:1 onto `Recorder`/`Settings`/`CaptureCapabilities` — no duplicated logic in the UI layer.
- States shown must come from `AppState` (Idle/SelectingRegion/Starting/Recording/Stopping/Completed/Failed) — a label per state, not scattered booleans.
- Exit-while-recording: `adw::AlertDialog` → "Stop recording and exit" (stop → finalize → exit) or "Cancel" (keep window). No "keep recording" option.
- `wf-recorder` quality presets surface as Standard/High only; fps 24/30/60 (validated set); outputs from `capabilities().outputs`.
- Commit style `feat:`/`fix:`/`docs:`; every task runs fmt+clippy+test gates.

---

### Task 1: GUI dependencies + skeleton window

**Files:**
- Modify: `Cargo.toml` (add `gtk4`, `libadwaita` deps)
- Modify: `src/main.rs` (bare invocation → GUI when `WAYLAND_DISPLAY`/`DISPLAY` set; else help)
- Create: `src/ui/mod.rs`, `src/ui/app.rs` (Application + ApplicationWindow shell)
- Add `pub mod ui;` to `src/lib.rs` — but gate: `#[cfg(feature = "gui")] pub mod ui;` with `gui` default feature, OR keep ui in lib unconditionally (simpler; gtk4 is a hard dep anyway). **Decision: unconditional** — keep it simple, GUI is core to the product.

- [ ] **Step 1:** `cargo add gtk4 libadwaita` (resolver picks compatible versions; verify `pkg-config --modversion libadwaita-1` ≥ the crate's minimum first — Fedora 44 ships libadwaita 1.8.x, compatible with libadwaita-rs 0.8).
- [ ] **Step 2:** Skeleton: `ui::run() -> glib::ExitCode` builds `adw::Application` (id `io.github.niri-recorder` or similar), `ApplicationWindow` + `AdwHeaderBar`, placeholder `gtk::Box` content, dark/light follows system (`adw::StyleManager` default). ~120 lines.
- [ ] **Step 3:** `cli::run` change: bare args → `ui::run()`; if `ui::run` fails to init GTK (no display), print error + exit 1. Subcommands unchanged.
- [ ] **Step 4:** Smoke: `./target/debug/niri-recorder` opens a window on this Wayland session; close cleanly. `cargo test` still green (no GUI in tests).
- [ ] **Step 5:** Commit `feat: GTK4 skeleton window, bare invocation launches GUI`.

---

### Task 2: Control surface — selectors + start/stop + status

**Files:**
- Create: `src/ui/window.rs` (builds the content widgets, binds to a `GuiController`)
- Create: `src/ui/controller.rs` (owns `Recorder<WfRecorder>` + `Settings`; runs blocking calls on a worker thread; pushes state to widgets via `glib::Sender<UiEvent>`)
- Test: none unit-testable here beyond compile; smoke per task end. (Logic-heavy pieces — e.g. output-list→dropdown mapping, elapsed-time formatting — extract into pure fns and unit test them in `src/ui/controller.rs` `mod tests`.)

**Interfaces:**
- `enum UiEvent { StateChanged(AppState), Tick /* elapsed refresh */ , Failed(String), Completed(PathBuf) }` sent worker→main via `glib::MainContext::channel` or `async-channel`.
- `GuiController::request_start(target, output, fps, quality)`, `request_stop()`, `request_region()` — all non-blocking; worker thread drives `Recorder` and emits UiEvents.

- [ ] **Step 1:** Widgets per spec §8: header (app name + Settings button wired in T4), target dropdown (Full display / Region), display dropdown (from `capabilities().outputs`; hidden/disabled for Region), fps dropdown (24/30/60), quality dropdown (Standard/High), primary button (Start Recording ↔ Stop Recording), status row (state label + elapsed `mm:ss` via `glib::timeout_add_local(500ms)`), output-path label.
- [ ] **Step 2:** Worker thread owns `Recorder`; commands via `std::sync::mpsc`; events back via glib channel → widgets update. Region flow: request_region → worker runs `Recorder::start(Region,…)` which blocks inside slurp — correct, it's on the worker; cancel selection → SelectionCancelled → Idle event.
- [ ] **Step 3:** Elapsed timer ticks only in Recording state; state label maps every AppState variant.
- [ ] **Step 4:** Smoke: record DP-3 from GUI, stop, verify finalized mp4; record region (manual slurp drag — first real interactive verification; report result).
- [ ] **Step 5:** Commit `feat: GUI control surface wired to recorder controller`.

---

### Task 3: Completion actions + recent recordings

**Files:** Modify `src/ui/window.rs`, `src/ui/controller.rs`; create `src/ui/recordings.rs`.

- [ ] **Step 1:** After Completed(path): show output path + two actions — **Open Folder** (`gtk::FileLauncher::new(&File::for_path(dir)).launch()` or `gio::AppInfo::launch_default_for_uri("file://…")`) and **Play Recording** (`launch_default_for_uri` on the file).
- [ ] **Step 2:** Recent recordings list: scan `output_directory` for `screen-recording-*.mp4|webm`, newest 5, rows with filename + timestamp + per-row play button. Pure list builder in `recordings.rs` is unit-testable (`list_recent(dir, n)` → sorted Vec<PathBuf>) — test it with tempdir.
- [ ] **Step 3:** Smoke: complete a recording, Open Folder opens Files at the dir, Play opens the video.
- [ ] **Step 4:** Commit `feat: completion actions and recent recordings list`.

---

### Task 4: Settings dialog + close-while-recording

**Files:** Create `src/ui/settings.rs`; modify `src/ui/window.rs`, `src/ui/app.rs`.

- [ ] **Step 1:** `adw::PreferencesDialog` (or `PreferencesWindow`): output directory row (`gtk::FileDialog` folder picker → persist `Settings.save()`), default target, fps, quality. Changes persist to config.toml immediately.
- [ ] **Step 2:** Close-while-recording: `close-request` handler → if state is Recording/Starting/SelectingRegion → `adw::AlertDialog` "Recording in progress" with "Stop recording and exit" / "Cancel". Stop path: `request_stop()` → on Completed/Failed event → `app.quit()`.
- [ ] **Step 3:** Smoke: start recording, close window → dialog appears → Stop-and-exit finalizes file + exits; Cancel keeps recording.
- [ ] **Step 4:** Commit `feat: settings dialog and close-while-recording prompt`.

---

### Task 5: Packaging — desktop entry + icon + install docs

**Files:** Create `data/io.github.niri-recorder.desktop`, `data/icons/` (simple SVG icon — a circle-in-square record motif; hand-write minimal SVG), modify `README.md`, `Makefile` (add `install`/`uninstall` targets), `docs/ROADMAP.md`.

- [ ] **Step 1:** `.desktop` file: Name, Exec=`niri-recorder`, Icon, Categories=AudioVideo;Keywords=screen;record;, `StartupNotify=true`, `DBusActivatable=false`. Validate with `desktop-file-validate` if present.
- [ ] **Step 2:** Icon SVG → `~/.local/share/icons/hicolor/scalable/apps/niri-recorder.svg` via install target.
- [ ] **Step 3:** Makefile `install`: `cargo install --path .` + `install -Dm644` desktop file + icon to `~/.local/share/applications` + icon dir. `uninstall` reverses.
- [ ] **Step 4:** Niri keybinding examples into README: `niri-recorder start --target display` / `stop` bindings in `~/.config/niri/config.kdl` (spawn-sh macros).
- [ ] **Step 5:** Smoke: `make install`, launch from DMS/launcher or `gtk4-launch`, record once, `make uninstall` removes cleanly.
- [ ] **Step 6:** Commit `feat: desktop entry, icon, install targets, niri keybinding docs`.

---

### Task 6: Reliability pass

**Files:** Modify as needed; update `docs/WAYLAND-NIRI-COMPATIBILITY.md`, `docs/TESTING.md`, `docs/TROUBLESHOOTING.md`.

- [ ] **Step 1:** Multi-monitor reality check — eDP-1 (laptop panel) + DP-3 now present: record eDP-1 via GUI display dropdown AND via CLI `--output eDP-1`; region-select on each output; region spanning both (slurp allows cross-output drag) → record result. Document findings (esp. negative-coordinate layouts).
- [ ] **Step 2:** Disk-full/partial-file behavior: point `output_directory` at a small tmpfs or `ulimit -f`-constrained shell; confirm stop reports failure honestly and `.part` handling is sane.
- [ ] **Step 3:** Long-recording sanity (~60s), workspace switches mid-GUI-recording.
- [ ] **Step 4:** Update compatibility/testing docs with results; commit `docs: reliability results — multi-monitor, disk pressure, long capture`.
- [ ] **Step 5:** Any bugs found → fix commits `fix:` + tests.

## Self-review notes

- **Spec coverage:** FR-01 display selector (T2), FR-03 prompt-to-stop (T4 — user-approved scope), FR-06 controls+timer (T2), FR-07 actions (T3), FR-08 fps/quality (T2/T4), FR-09 settings UI (T4), §8 layout (T2–T4), Phase 5 packaging (T5), Phase 4 reliability (T6).
- **Deferred:** DMS integration, global shortcuts beyond documented niri bindings, thumbnails, hardware encode — all spec Phase 6.
- **Risk:** GTK4+async glue is the largest unknown; T2 carries the complexity. slurp blocking lands on the worker thread by design.
