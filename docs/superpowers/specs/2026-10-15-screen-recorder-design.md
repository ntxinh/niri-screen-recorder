# Screen Recorder for Fedora 44 — Product & Technical Specification

**Status:** Draft for implementation  
**Project type:** Native Linux desktop application  
**Target environment:** Fedora Workstation 44, Wayland, Niri, DankMaterialShell (DMS), Fish  
**Primary language:** Rust  
**Working name:** `niri-recorder` (temporary; rename before release)

## 1. Product vision

Build a small, reliable, local-first screen recorder designed for a Wayland desktop. It should make it quick to record either a complete display or a user-selected rectangular region, without recording microphone or system audio.

The app should fit naturally into a Niri + DMS workflow and remain reliable when the user changes workspaces, switches focused applications, minimizes the control window, or uses multiple monitors.

The app is not a video editor, streaming studio, webcam recorder, or cloud service. The initial goal is dependable capture and easy access to resulting video files.

## 2. Goals and non-goals

### Goals

- Record a full display.
- Record a rectangular region selected interactively.
- Record video only; do not capture microphone or system audio.
- Continue recording when switching Niri workspaces or changing focused windows.
- Support multiple monitors, with explicit display selection for full-display recording and region selection across the supported desktop layout where the backend permits it.
- Provide Start, Stop, Cancel, and elapsed-time feedback.
- Save recordings locally in a configurable directory.
- Produce standard video files playable by common Linux media players.
- Handle missing dependencies, capture failures, disk errors, and interrupted recordings clearly.
- Integrate with the user's existing desktop without requiring GNOME Shell or KDE Plasma.
- Be testable, documented, and maintainable.

### Non-goals for MVP

- Audio capture of any kind.
- Webcam capture.
- Live streaming.
- Video editing, trimming, annotations, or effects.
- Cloud upload, account, or synchronization.
- Recording protected DRM content or bypassing compositor security restrictions.
- A custom low-level Wayland capture/encoder implementation unless external backends prove insufficient.
- Windows or macOS support.
- Mandatory tray icon or DMS-specific plugin.

## 3. User stories

1. As a user, I can launch the app and record my entire selected monitor.
2. As a user, I can select a rectangle and record only that area.
3. As a user, I can switch workspaces during recording without stopping capture.
4. As a user with two or more monitors, I can choose which display to record.
5. As a user, I can stop a recording and immediately locate the saved file.
6. As a user, I can cancel a region selection without creating a recording.
7. As a user, I can configure output directory, frame rate, and quality.
8. As a user, I can see whether recording is active even if the main window is hidden.
9. As a user, I can recover from a missing utility or unsupported capture protocol with an actionable error.
10. As a user, I can use the application without granting network access or creating an account.

## 4. Target environment and compatibility

Primary test environment:

- Fedora Workstation 44.
- Native Wayland session.
- Niri compositor.
- DankMaterialShell.
- Fish shell.
- One or more monitors; test mixed resolutions and scaling when available.

### Wayland constraints

Wayland does not provide unrestricted desktop pixel access to ordinary applications. Capture must use a compositor-supported capture protocol or an approved ScreenCast portal/PipeWire flow.

Do not implement screen capture using X11-only techniques such as `x11grab` as the primary path. Do not assume every Wayland compositor supports every protocol used by a given recorder.

### Backend compatibility gate

Before implementing the full UI, prove all of the following on the target system:

- Full-display capture works in Niri.
- Region capture works with the chosen selection and capture utilities.
- Capture continues when switching workspaces.
- The intended behavior on multiple monitors is documented and tested.
- Stopping the capture finalizes a playable video file.
- Video capture does not include audio.
- The capture process can be stopped cleanly by the app.

If the chosen backend cannot meet these requirements, select a compatible backend before building the rest of the app around it.

## 5. Technology decisions

### Recommended stack

| Area | Recommendation | Rationale |
|---|---|---|
| Language | Rust, stable toolchain | Reliable process lifecycle, typed state, safe filesystem handling |
| GUI | GTK4 + libadwaita, Rust bindings | Native Linux desktop conventions and accessible controls |
| Capture abstraction | Internal Rust trait/interface | Keeps UI independent from capture implementation |
| Initial capture candidates | `wf-recorder`, `wl-screenrec`, XDG ScreenCast portal + PipeWire | Validate against Niri rather than assuming universal support |
| Region selector | `slurp` when compatible | Reuses a familiar Wayland selection utility |
| Video encoding | Delegate to a tested backend initially | Avoid reimplementing capture and encoding prematurely |
| Serialization | `serde` + TOML configuration | Simple, typed local settings |
| Errors | `thiserror` for domain errors; `anyhow` at app boundaries where appropriate | Clear error propagation |
| Logging | `tracing` + `tracing-subscriber` | Diagnosable process and capture lifecycle |
| Tests | Rust unit/integration tests; shell-level backend smoke tests | Test state transitions and real desktop integration |
| Version management | `mise.toml` | Reproducible Rust and tooling setup |
| Formatting/lint | `rustfmt`, `clippy` | Standard Rust quality gates |
| CI | GitHub Actions | Formatting, linting, tests, build validation |

### Why Rust instead of Go?

Rust is the preferred choice because this project is a native Linux desktop utility that needs careful process lifecycle management and may later integrate with Wayland, PipeWire, portals, and native GUI bindings. Go is viable for a CLI wrapper, but its desktop UI and native Wayland integration ecosystem is less compelling for this specific application.

### Dependency policy

- Prefer Fedora-packaged system dependencies for desktop integration.
- Document optional utilities separately from build-time dependencies.
- Do not bundle a second copy of a system recorder unless distribution requirements justify it.
- Detect dependency availability at startup and in a dedicated diagnostics screen.
- Do not shell-interpolate user-controlled paths or command arguments.
- Spawn subprocesses with argument arrays; do not build shell command strings.
- Record the tested utility versions and compatibility notes in `/docs`.

## 6. Capture architecture

Use a backend abstraction so that capture implementations can be swapped without changing UI logic.

Conceptual interface:

```rust
trait CaptureBackend {
    fn capabilities(&self) -> CaptureCapabilities;
    fn start(&self, request: CaptureRequest) -> Result<CaptureSession, CaptureError>;
    fn stop(&self, session: CaptureSession) -> Result<CaptureResult, CaptureError>;
    fn cancel(&self, session: CaptureSession) -> Result<(), CaptureError>;
}
```

The exact Rust signatures can be adjusted during implementation to fit asynchronous process handling and ownership. The interface is conceptual, not a requirement to force this exact API.

### Capture request

Represent the following explicitly:

- Capture target: full display or region.
- Display identifier for full-display mode, where supported.
- Region rectangle for region mode.
- Output file path.
- Frame rate.
- Quality/encoding preset.
- Audio disabled, with no audio options exposed in the UI.

### Candidate backend strategy

1. **Backend A: `wf-recorder` + `slurp`**
   - Prototype first if the installed versions support the target Niri session.
   - `slurp` returns a selected rectangle; validate the returned geometry before passing it to the recorder.
   - Test full display, selected output, region, workspace switching, and multiple displays.
   - Do not treat support on Sway or Hyprland as proof of support on Niri.

2. **Backend B: `wl-screenrec` + `slurp`**
   - Evaluate if its protocol requirements are met by the installed Niri version.
   - Test hardware encoding and software fallback where available.
   - Treat codec availability and GPU driver support as runtime capabilities, not universal guarantees.

3. **Backend C: XDG ScreenCast portal + PipeWire**
   - Consider for broader compositor compatibility and portal-mediated source selection.
   - Verify which portal implementation is active and whether the expected selection flow works on Niri.
   - Portal capture may present a compositor-controlled picker rather than use `slurp`; keep this behavior backend-specific.

The app should not silently claim support for a capture mode that a backend cannot provide. Surface capabilities to the UI and hide or disable unsupported options with a useful explanation.

## 7. Functional requirements

### FR-01: Full-display recording

- User can select a monitor when multiple displays are detected.
- When only one display is available, use it as the default.
- If the backend can record all displays as a single canvas, this may be an optional future mode; it is not required for MVP.
- Display names and dimensions should be shown where available.
- If a selected display disappears, stop safely and report the failure.
- Never assume that a “full screen” command captures every physical monitor; define whether it means one output or a combined desktop canvas.

### FR-02: Region recording

- User starts region selection from the app.
- Launch the supported region selector as a subprocess.
- User can cancel selection without starting capture.
- Validate selection output: parseable geometry, positive width and height, and valid coordinates.
- Reject malformed output and report a clear error.
- Support regions on the target monitor layout as far as the chosen selector/backend allows.
- Document limitations involving negative coordinates, mixed scaling, rotated displays, and selections spanning output boundaries.
- If the selection utility is missing, explain which package is required.

### FR-03: Workspace independence

- Recording must run independently of the main GUI window and current focus.
- Changing workspaces must not send a stop signal or restart capture.
- Moving the app window or hiding it must not affect capture.
- Do not bind recording lifecycle to a window's visibility, focus, or active workspace.
- Use a long-lived capture service/controller owned by the application process.
- Closing the GUI should prompt while recording is active: **Stop recording and exit** or **Keep recording in background**. If background mode is not implemented in MVP, offer only a clear stop-and-exit confirmation.
- Do not claim recording continues after application exit unless a dedicated background service has actually been implemented and tested.

### FR-04: Multi-monitor support

- Detect outputs through the selected backend or supported compositor interfaces.
- Show available display choices when the backend can enumerate outputs.
- Record one selected output in MVP.
- Region recording must be tested against the desktop's global coordinate layout.
- Support monitor hot-plugging gracefully: if the target becomes unavailable, stop cleanly and preserve the finalized file if possible.
- Do not merge multiple monitors into one video unless explicitly supported by the backend and implemented as a defined feature.
- Test mixed resolutions and scale factors where possible.

### FR-05: Audio disabled

- Do not expose microphone or system-audio controls.
- Do not request microphone permission.
- Do not pass audio-capture flags or audio sources to subprocesses.
- Verify output files contain no audio stream in integration tests where `ffprobe` is available.
- If a backend defaults to audio capture, explicitly disable it.

### FR-06: Recording controls

- Start recording.
- Stop recording gracefully and finalize the file.
- Cancel before recording begins.
- Display a visible recording state and elapsed duration.
- Prevent duplicate Start operations while recording is active.
- Prevent accidental second capture process from being started by repeated clicks or shortcuts.
- Report failures to start or stop capture.
- Do not report success until the backend confirms that the process has stopped and the output has been finalized or its state has been assessed.

### FR-07: Output files

- Default directory: `~/Videos/Screen Recordings`.
- Create the directory if needed, with appropriate permissions.
- Use a collision-safe filename, for example `screen-recording-YYYY-MM-DD_HH-MM-SS.mp4`.
- Never silently overwrite an existing file.
- Allow the user to choose an output directory in settings.
- Show the final output path after a successful recording.
- Provide **Open Folder** and **Play Recording** actions using desktop file/URI handlers.
- Use a temporary/in-progress filename or explicit recording state so incomplete files are not mistaken for completed recordings.
- Preserve a recoverable partial file when safe, and clearly mark it as incomplete if finalization fails.

### FR-08: Recording quality

MVP settings:

- Frame rate: 30 FPS default; optionally 24 and 60 FPS if supported.
- Quality preset: Standard and High.
- Codec/container defaults selected according to backend support and playback compatibility.
- Display the selected codec/preset where useful.
- No promise of hardware encoding; detect and expose only capabilities actually available.
- Keep advanced encoder parameters out of the main UI until the basics are reliable.

Suggested compatibility target: MP4/H.264 where the backend and installed encoder support it. A WebM fallback is acceptable if it is explicitly shown to the user and documented. The extension must match the actual container.

### FR-09: Settings

Persist user preferences locally:

- Default output directory.
- Default capture target/mode.
- Last selected display, if still available.
- Frame rate.
- Quality preset.
- Filename timestamp format, if configurable.
- Start/stop keyboard shortcut, only if reliable and conflict-safe.

Settings must have defaults and validation. Missing or malformed configuration must not prevent application startup; fall back to defaults and log a useful warning.

### FR-10: Status and diagnostics

- Show Idle, Selecting Region, Starting, Recording, Stopping, Completed, and Error states.
- Show elapsed time while recording.
- Show the selected display or region.
- Provide a diagnostics view or command that reports compositor/session information, dependency versions, detected backends, and available capabilities.
- Avoid collecting or uploading diagnostics automatically.
- Redact environment secrets from logs.

## 8. UX and visual design

### Main window

Keep the interface compact, dark-mode friendly, keyboard accessible, and usable at small window sizes.

Suggested layout:

1. Header: application name and Settings.
2. Capture target selector:
   - Full display
   - Region
3. Display selector when relevant.
4. Quality and frame-rate selectors.
5. Primary action:
   - **Start Recording** while idle
   - **Stop Recording** while recording
6. Recording status, elapsed timer, and output destination.
7. Recent recordings list with file name, timestamp, and actions.

### UX behavior

- Starting region mode launches the selector immediately or after a concise confirmation, not a complicated wizard.
- Cancelled selection returns to Idle.
- Disable conflicting controls during transitions.
- Show concise errors with a technical detail expander or copyable diagnostic detail.
- Avoid modal dialogs for routine success.
- Confirm destructive actions and exit while recording.
- Follow system dark/light appearance where supported by GTK/libadwaita.
- Do not require a custom title bar or DMS integration for MVP.

### DMS and Niri integration

- App must work as a normal desktop application launched from an app launcher.
- Provide documented CLI commands so the user can create Niri keybindings later.
- Optional later integration: DMS quick action, status indicator, or a small recording control surface.
- Do not depend on undocumented DMS APIs or Niri internals for core recording.
- Do not assume that a global shortcut can be registered portably. Prefer a configurable shortcut or a documented Niri keybinding that invokes the app's CLI/control interface.

## 9. CLI interface

A CLI is valuable for Niri keybindings and automation, even if the main product is graphical.

Proposed commands (final names can change):

```text
niri-recorder
niri-recorder start --target display
niri-recorder start --target region
niri-recorder stop
niri-recorder status
niri-recorder diagnostics
```

Requirements:

- CLI operations must use the same capture controller and configuration as the GUI.
- If start/stop/status are intended to control an existing process, implement a reliable local IPC mechanism with ownership and stale-state handling. Do not pretend that separate CLI invocations can control an unrelated GUI process without IPC.
- Bind any local control socket to a user-private runtime directory and enforce appropriate permissions.
- Validate all CLI arguments.
- Return meaningful exit codes and machine-readable output as an optional later enhancement.
- Do not expose a network listener.

## 10. Process lifecycle and state machine

Suggested state machine:

```text
Idle
  -> SelectingRegion (region mode only)
  -> Starting
  -> Recording
  -> Stopping
  -> Completed
  -> Idle

SelectingRegion -> Idle          (user cancels)
Starting -> Error                (backend fails)
Recording -> Stopping            (user stops)
Recording -> Error/Recovery      (backend exits unexpectedly)
Stopping -> Error/Recovery       (finalization fails)
```

Implementation requirements:

- Model transitions explicitly; avoid scattered booleans such as `is_recording`, `is_stopping`, and `is_starting`.
- Keep subprocess handles and process IDs owned by the capture controller.
- Consume stdout/stderr without allowing pipe buffers to block the child process.
- Handle SIGINT/SIGTERM and application shutdown.
- Use graceful termination first; use a bounded timeout and stronger termination only as a fallback.
- Wait for the child process to exit and inspect its exit status.
- Do not treat a signal request as proof that a file has finalized.
- Clean up temporary state and stale lock/IPC files safely.
- Guard against duplicate starts and stale process IDs.
- Log state transitions with session identifiers.

## 11. Proposed project structure

```text
niri-recorder/
├── Cargo.toml
├── Cargo.lock
├── mise.toml
├── Makefile
├── .editorconfig
├── .gitignore
├── AGENTS.md
├── README.md
├── DESIGN.md
├── LICENSE
├── src/
│   ├── main.rs
│   ├── cli/
│   │   ├── mod.rs
│   │   └── commands.rs
│   ├── app/
│   │   ├── mod.rs
│   │   ├── state.rs
│   │   └── controller.rs
│   ├── capture/
│   │   ├── mod.rs
│   │   ├── backend.rs
│   │   ├── capabilities.rs
│   │   ├── request.rs
│   │   ├── process.rs
│   │   └── backends/
│   │       ├── mod.rs
│   │       ├── wf_recorder.rs
│   │       ├── wl_screenrec.rs
│   │       └── portal.rs
│   ├── config/
│   │   ├── mod.rs
│   │   └── settings.rs
│   ├── files/
│   │   ├── mod.rs
│   │   └── naming.rs
│   ├── diagnostics/
│   │   └── mod.rs
│   └── ui/
│       ├── mod.rs
│       ├── window.rs
│       ├── settings.rs
│       └── recordings.rs
├── tests/
│   ├── config_tests.rs
│   ├── filename_tests.rs
│   ├── state_machine_tests.rs
│   ├── process_tests.rs
│   └── backend_smoke_tests.rs
└── docs/
    ├── REQUIREMENTS.md
    ├── ARCHITECTURE.md
    ├── CAPTURE-BACKENDS.md
    ├── WAYLAND-NIRI-COMPATIBILITY.md
    ├── UX.md
    ├── TESTING.md
    ├── TROUBLESHOOTING.md
    └── ROADMAP.md
```

This structure is a starting point. Keep modules small, but do not create empty modules merely to match the tree. Backend files should exist only when those backends are implemented or actively being evaluated.

## 12. Required repository files

### `mise.toml`

- Pin or constrain the stable Rust toolchain.
- Include required development tools only when they are actually used by the project.
- Avoid pinning Fedora system libraries in mise; document those in `README.md`.
- Provide reproducible commands for installing the toolchain and running checks.
- Do not commit local machine paths or secrets.

### `AGENTS.md`

Include agent-facing development rules:

- Read `README.md`, `DESIGN.md`, and relevant `/docs` files before making architectural changes.
- Confirm actual Niri/backend compatibility; do not infer it.
- Prefer existing tested capture utilities over custom low-level capture code.
- Keep GUI logic separate from capture process management.
- Never add audio capture.
- Never execute shell command strings assembled from user input.
- Run formatting, Clippy, tests, and build checks before declaring a task complete.
- Update documentation whenever behavior, commands, configuration, or architecture changes.
- Do not claim tests passed unless they were run and their results were inspected.
- Record known platform limitations honestly.
- Avoid unrelated refactors and unnecessary dependencies.

### `README.md`

Include:

- Project purpose and screenshots when available.
- Supported environment and compatibility status.
- Feature list and known limitations.
- Fedora dependency installation instructions, verified against Fedora 44 package names.
- Development setup with mise.
- Build, run, test, lint, and install instructions.
- Usage examples for GUI and CLI.
- Configuration path and output directory.
- Troubleshooting and diagnostics.
- Security/privacy notes.
- Current project status and roadmap.

### `.editorconfig`

At minimum:

- UTF-8.
- LF line endings.
- Final newline.
- Trim trailing whitespace.
- Four-space indentation for Rust unless a repository-wide convention is explicitly chosen otherwise.
- Two-space indentation for YAML/TOML-adjacent or Markdown examples only where appropriate; avoid settings that conflict with standard formatters.

### `DESIGN.md`

Document:

- Architecture and component boundaries.
- Capture backend decision and evidence.
- Process lifecycle and state machine.
- Output finalization and crash handling.
- Configuration and IPC design.
- Security boundaries.
- Known compatibility limitations.
- Alternatives considered and reasons for rejecting them.

### `/docs`

Keep focused, maintained documents for requirements, architecture, backend compatibility, UX, testing, troubleshooting, and roadmap. Update them with implementation changes rather than allowing them to diverge from the code.

### `Makefile`

Provide a consistent command interface, such as:

```makefile
.PHONY: check fmt lint test build run clean

check: fmt lint test build

fmt:
	cargo fmt --all -- --check

lint:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test --all-targets

build:
	cargo build

run:
	cargo run

clean:
	cargo clean
```

Adjust targets to match actual features. Add integration or backend smoke-test targets separately so that ordinary unit tests do not require a live desktop session.

## 13. Configuration and storage

Suggested configuration path:

```text
~/.config/niri-recorder/config.toml
```

Use the platform's standard user directories where practical; prefer XDG directory conventions rather than hard-coding a home path.

Suggested configuration:

```toml
output_directory = "~/Videos/Screen Recordings"
default_target = "display"
default_fps = 30
quality = "standard"
preferred_backend = "auto"
```

The example is illustrative. Implement path expansion explicitly and safely; do not assume every library expands `~`.

Runtime state should use a user-private runtime directory when available, for example under `$XDG_RUNTIME_DIR`. Do not store session sockets, lock files, or temporary control state in a world-readable directory.

## 14. Security and privacy

- All recording is local by default.
- No telemetry, account, or upload functionality in MVP.
- No audio capture or audio permission request.
- Use compositor-mediated capture paths that respect Wayland security boundaries.
- Never bypass portal permissions or compositor security checks.
- Do not run the app as root.
- Use safe subprocess APIs and explicit arguments.
- Validate and normalize output paths.
- Do not overwrite existing recordings silently.
- Ensure local IPC is user-private and rejects unauthorized peers where supported.
- Avoid logging full environment variables, tokens, or unrelated private paths.
- Explain that screen recordings may contain confidential information visible on the desktop.

## 15. Error handling

Every expected failure should produce an actionable message.

Examples:

- **Recorder not found:** name the missing dependency and show the documented Fedora installation command.
- **Region selector cancelled:** return to Idle without treating it as an error.
- **Unsupported protocol:** explain that the current backend is not compatible with the compositor/session.
- **No display available:** refresh capability detection and allow retry.
- **Output directory unwritable:** show the path and offer settings to choose another directory.
- **Disk full:** stop safely if possible, preserve the partial file, and explain that the recording may be incomplete.
- **Encoder unavailable:** suggest a supported fallback only if one is actually detected.
- **Recorder exited unexpectedly:** inspect the exit status, retain diagnostic output, and assess whether the file is playable.
- **Stop timed out:** report the problem and avoid claiming finalization succeeded.

## 16. Testing strategy

### Unit tests

- State transitions and invalid transitions.
- Filename generation and collision avoidance.
- Configuration defaults, parsing, and validation.
- Geometry parsing and validation.
- Backend capability selection.
- Exit-status interpretation.
- Error-to-user-message mapping.

### Integration tests

- Spawn a fake child process to test start, stop, timeout, stderr handling, and unexpected exit.
- Test graceful termination and child reaping.
- Test configuration and output path handling.
- Test CLI-to-controller communication if IPC is implemented.
- Verify that no audio option is passed to the capture process.

### Manual acceptance tests on Fedora 44 + Niri

- Full-display capture on a single monitor.
- Region capture on a single monitor.
- Start recording, switch across several workspaces, return, and stop.
- Start recording and focus other applications.
- Minimize or hide the main window during capture.
- Two monitors with different resolutions.
- Region selection on each monitor.
- Mixed scaling and negative-coordinate layouts if present.
- Monitor disconnect during recording.
- Cancel region selection.
- Remove or rename a required utility and confirm actionable error handling.
- Fill or simulate a full output filesystem in a safe test environment.
- Stop a long recording and verify the file is playable.
- Confirm that the output has no audio stream when `ffprobe` is available.
- Verify CPU usage and output size at each supported quality preset.

Tests that need an actual compositor or display should be marked as integration/manual tests, not silently skipped and reported as full validation.

## 17. Acceptance criteria

MVP is acceptable when all of the following are true:

1. The app builds reproducibly using documented mise and Fedora dependencies.
2. The app starts in a native Wayland Niri session.
3. A user can record one selected display.
4. A user can select a rectangle and record it.
5. Workspace switching does not interrupt the recording.
6. The chosen multi-monitor behavior is implemented and documented.
7. Audio is not captured, and no audio controls are exposed.
8. Stop finalizes a playable file under normal conditions.
9. Existing files are never overwritten silently.
10. Cancelled selection creates no recording.
11. Missing dependencies and unsupported capture capabilities produce actionable errors.
12. Repeated clicks cannot create multiple simultaneous recording sessions through the same controller.
13. Unit tests, formatting, Clippy, and build checks pass.
14. Backend smoke tests have been run on the target environment, and compatibility claims match observed results.
15. README, DESIGN, AGENTS, and relevant `/docs` files reflect the implemented behavior.

## 18. Implementation phases

### Phase 0 — Capture feasibility spike

Deliverables:

- Install/test candidate capture utilities on Fedora 44.
- Verify Niri protocol compatibility for the installed versions.
- Record a full display and a selected region.
- Test workspace switching and multiple monitors.
- Verify output finalization and no audio.
- Document findings in `docs/CAPTURE-BACKENDS.md` and `docs/WAYLAND-NIRI-COMPATIBILITY.md`.

**Exit gate:** Choose a backend only after all critical capture scenarios have been demonstrated. Do not start full UI development before this gate.

### Phase 1 — Repository foundation

- Create Rust project and `mise.toml`.
- Add `AGENTS.md`, `README.md`, `.editorconfig`, `DESIGN.md`, `Makefile`, and `/docs`.
- Configure formatting, Clippy, tests, and CI.
- Establish logging and error conventions.

### Phase 2 — Capture controller and CLI

- Implement state machine and backend abstraction.
- Implement safe subprocess lifecycle.
- Add full-display and region capture.
- Implement stop, cancellation, output naming, and finalization checks.
- Add status and diagnostics commands.
- Test against a fake child process and then the real selected backend.

### Phase 3 — Minimal GUI

- Create GTK4/libadwaita window.
- Add capture mode, display selector, frame-rate and quality settings.
- Add start/stop actions, state feedback, elapsed time, and final file actions.
- Ensure UI closure and workspace changes cannot accidentally stop capture.

### Phase 4 — Reliability and multi-monitor validation

- Test display changes, mixed resolutions/scaling, cancellation, disk errors, and unexpected process exit.
- Improve partial-file recovery and error reporting.
- Validate no-audio output.
- Add compatibility diagnostics and documented limitations.

### Phase 5 — Packaging and desktop integration

- Document installation and removal.
- Add desktop entry and application icon.
- Test launch from app launcher.
- Document Niri keybinding examples.
- Optionally package as an RPM after dependencies and install paths are stable.

### Phase 6 — Optional enhancements

Only after MVP acceptance:

- DMS quick action or status integration.
- Global or configurable keyboard shortcuts.
- Background service/daemon with robust CLI control.
- Recording history with thumbnails.
- Automatic output cleanup rules (opt-in only).
- Optional countdown and cursor-visibility controls if supported by the backend.
- Optional selectable encoder presets and hardware-encoding preference.

## 19. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Capture backend does not work on the installed Niri version | Make compatibility testing Phase 0 and keep backend abstraction |
| Region selector and recorder use incompatible coordinate systems | Validate geometry and test mixed scaling, negative coordinates, and cross-monitor selection |
| Workspace change interrupts capture | Keep recorder process independent of GUI focus and workspace |
| Multiple monitors behave differently across backends | Define MVP as one selected output plus region mode; test explicitly |
| Hardware encoder is unavailable | Detect capabilities and provide a verified software fallback if practical |
| App exits and leaves an incomplete file | Graceful stop, child reaping, finalization checks, recovery diagnostics |
| CLI cannot control an existing GUI process | Implement local IPC intentionally or limit CLI to standalone commands |
| GTK/Wayland packaging differs between environments | Use Fedora packages initially and document runtime dependencies |

## 20. Definition of done for each change

A coding agent or contributor must:

1. Read relevant project instructions and documentation.
2. Implement the smallest coherent change.
3. Add or update tests.
4. Run `cargo fmt --all -- --check`.
5. Run `cargo clippy --all-targets --all-features -- -D warnings`.
6. Run relevant `cargo test` targets.
7. Run a build.
8. Run desktop/backend integration tests when the change affects capture behavior.
9. Update README or `/docs` if behavior or compatibility changes.
10. Report exactly what was tested and list any unverified conditions.

## 21. Reference projects and documentation

Use these as implementation research references, not as proof that every feature works in the target environment:

- `slurp`: https://github.com/emersion/slurp
- `wf-recorder`: https://github.com/ammen99/wf-recorder
- `wl-screenrec`: https://github.com/russelltg/wl-screenrec
- Wayland screen capture protocol overview: https://wayland.app/protocols/
- XDG Desktop Portal: https://flatpak.github.io/xdg-desktop-portal/
- PipeWire: https://pipewire.org/
- GTK4 Rust bindings: https://gtk-rs.org/
- libadwaita Rust bindings: https://world.pages.gitlab.gnome.org/Rust/libadwaita/

Check current upstream repository URLs, release status, licenses, and Fedora package availability before adopting dependencies.

---

## Final recommendation

Build this as a **Rust + GTK4 application with a capture-backend abstraction**. Start with a short compatibility spike comparing `wf-recorder`, `wl-screenrec`, and portal/PipeWire capture on the actual Fedora 44 + Niri setup. Use `slurp` for region selection only when the selected capture path supports its geometry and protocol assumptions.

The most important architectural rule is that the capture session must be managed independently from the visible GUI and active workspace. This is essential to the core use case, not a later optimization.

---

## 22. Phase 0 findings — capture feasibility spike (2026-10-15, verified on target)

Target machine: Fedora 44, native Wayland (`wayland-1`), Niri, single output DP-3 (Dell P2419H 1920×1080@60), Intel CometLake-H UHD (iHD 26.2.4). Spike artifacts were recorded to `/tmp` and discarded.

### Backend verdicts

| Backend | Verdict | Evidence |
|---|---|---|
| `wf-recorder` 0.6.0 (dnf: `wf-recorder-0.6.0-2.fc44`) | **Chosen** | All gate scenarios pass; see below |
| `wl-screenrec` 0.2.0 (max buildable on F44 ffmpeg) | **Rejected on this hardware** | Unconditionally inserts a `scale_vaapi` filter; iHD exposes zero H.264 encode profiles on CometLake → `Failed to create processing pipeline config: 12 (the requested VAProfile is not supported)` and panic at `filter.rs`. `--no-hw` and bogus `--dri-device` do not bypass it. Revisit if a newer version builds or VAAPI drivers change. |
| XDG ScreenCast portal + PipeWire | **Rejected as primary** | `niri-portals.conf` declares no ScreenCast impl; installed backends are `gnome`/`gtk` (gnome portal's ScreenCast expects mutter). Fedora ffmpeg ships no `pipewire` demuxer. |

### Gate results (wf-recorder + libopenh264 → MP4/H.264)

- Full display: `wf-recorder -f out.mp4 -c libopenh264 -r 30` → 1920×1080 H.264 MP4, video-only stream, finalized on SIGINT.
- Region: `-g "100,100 800x600"` → exact 800×600 output. `-g` accepts slurp's `x,y WxH` output verbatim.
- Workspace survival: 4 `niri msg action focus-workspace` switches during a 16.4s capture → 494 frames, uninterrupted, playable.
- Stop: SIGINT finalizes cleanly (moov atom written; ffprobe parses). Fallback: `SIGTERM`, bounded timeout, then `SIGKILL`.
- Audio: absent (single `video` stream per ffprobe); `-a` is opt-in and will never be passed.

### Caveats from the spike

- Output is **variable frame rate** (wlr-screencopy damage tracking; `fps=30` filter averages ~30). `-D`/`--no-damage` forces CFR at higher cost. `avg_frame_rate` ~30 confirmed.
- `libopenh264` is Fedora's only packaged H.264 software encoder (no `libx264` in Fedora ffmpeg). Quality presets map to bitrate via `-p`.
- Interactive `slurp` could not be driven headlessly; only the geometry contract it emits was verified. First interactive run is a manual-verification item.
- Multi-monitor behaviour untestable on this machine (single output). Documented as manual-verification item per §16.

## 23. Ratified design decisions

1. **MVP backend:** `wf-recorder` + `slurp` only. The `CaptureBackend` trait (§6) remains so `wl-screenrec`/portal can slot in later; no second backend is implemented in MVP.
2. **CLI control model (§9):** no IPC daemon. `niri-recorder start` runs the capture in the foreground of that process; it writes a pidfile under `$XDG_RUNTIME_DIR/niri-recorder/` (mode 0600, user-private dir). `niri-recorder stop` reads the pidfile and sends SIGINT; `status` reports pidfile state. `diagnostics` is standalone. Stale pidfiles (dead pid, mismatched argv) are detected and ignored. This gives Niri keybindings honest start/stop without pretending to control an unrelated process.
3. **Codec/container default:** `libopenh264` in MP4 (`screen-recording-*.mp4`). Fallback `libvpx-vp9` in WebM, surfaced in settings/diagnostics when selected. Extension always matches the muxer.
4. **Spec location:** this file (`docs/superpowers/specs/2026-10-15-screen-recorder-design.md`) is the canonical spec. `DESIGN.md` at repo root will summarize architecture for humans; `/docs` files carry backend compatibility evidence.
5. **Runtime hardening confirmed:** spawn `wf-recorder`/`slurp` with argv arrays only; pidfile + lock in `$XDG_RUNTIME_DIR`; in-progress files use a `.part` suffix renamed on confirmed finalization; never overwrite — timestamp names with collision suffix.
6. **VAAPI/hw-encode:** not attempted by the MVP backend (driver unusable on this hardware). `preferred_backend = "auto"` resolves to wf-recorder; a `wl-screenrec` preference produces an actionable unsupported-backend error listing the evidence above.
