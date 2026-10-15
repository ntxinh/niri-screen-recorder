# niri-recorder Core + CLI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A working `niri-recorder` CLI that records the full display or a slurp-selected region to MP4 on Fedora 44 + Niri, with an explicit state machine, safe subprocess lifecycle, and pidfile-based stop — the same controller the GTK4 GUI (Plan B) will drive.

**Architecture:** Thin `main.rs` over a library. `capture/` owns a `CaptureBackend` trait with one implementation (`wf-recorder` + `slurp`) plus subprocess lifecycle primitives. `app/` owns the `AppState` state machine and the `Recorder` controller that sequences selection → spawn → stop/finalize, writes the pidfile, and renames `.part` → final name on confirmed success. `cli/` parses args and invokes the controller; `stop`/`status` operate on the pidfile only.

**Tech Stack:** Rust stable, clap, serde + toml, thiserror/anyhow, tracing + tracing-subscriber, directories (XDG), nix (signals), tempfile (dev-dep for tests).

**Spec:** `docs/superpowers/specs/2026-10-15-screen-recorder-design.md` — read §22/§23 (Phase 0 results + ratified decisions) first.

## Global Constraints

- Backend: **wf-recorder 0.6.0 only** (Fedora pkg `wf-recorder`); region selector **slurp**. Never pass `-a`/`--audio` — audio must never be captured.
- Codec: `libopenh264` → MP4. Fallback `libvpx-vp9` → WebM only when user-selected; extension must match muxer.
- Subprocesses: argv arrays via `std::process::Command`; NEVER build shell strings.
- Stop sequence: SIGINT → wait ≤5s → SIGTERM → wait ≤3s → SIGKILL. Wait and reap always.
- In-progress files: `*.part`, renamed to final timestamped name only after confirmed finalization. Never overwrite: append `-1`, `-2`, … on collision.
- Pidfile: `$XDG_RUNTIME_DIR/niri-recorder/recorder.pid`, mode 0600; stale pidfile (dead pid or argv mismatch) is ignored and removed.
- Config: `~/.config/niri-recorder/config.toml` via `directories`; malformed/missing config → defaults + `tracing::warn!`, never a startup failure.
- `~/` expansion is explicit in code; do not rely on shell expansion.
- No GTK/GUI code in this plan. `Cargo.toml` must not include gtk4/relm4 — Plan B adds them.
- Every task: `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test` before commit.
- Commit style: `feat:`/`test:`/`docs:`/`chore:` prefixes, imperative mood.

---

### Task 1: Project scaffold

**Files:**
- Create: `Cargo.toml`, `src/lib.rs`, `src/main.rs`, `mise.toml`, `.editorconfig`, `.gitignore`, `Makefile`

- [ ] **Step 1: Write `Cargo.toml`**

```toml
[package]
name = "niri-recorder"
version = "0.1.0"
edition = "2021"
description = "Local-first screen recorder for Wayland/Niri"

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
directories = "6"
nix = { version = "0.30", features = ["signal", "process"] }
serde = { version = "1", features = ["derive"] }
thiserror = "2"
toml = "0.8"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Write `src/lib.rs` and `src/main.rs`**

```rust
// src/lib.rs
pub mod app;
pub mod capture;
pub mod cli;
pub mod config;
pub mod files;
```

```rust
// src/main.rs
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    niri_recorder::cli::run(std::env::args_os())
}
```

Create empty `src/{app,capture,cli,config,files}/mod.rs` with only `//!` doc comments so the crate compiles.

- [ ] **Step 3: `mise.toml`, `.editorconfig`, `.gitignore`, `Makefile`**

```toml
# mise.toml
[tools]
rust = "stable"
```

```editorconfig
# .editorconfig
root = true
[*]
charset = utf-8
end_of_line = lf
insert_final_newline = true
trim_trailing_whitespace = true
[*.rs]
indent_style = space
indent_size = 4
[*.{toml,yml,yaml}]
indent_style = space
indent_size = 2
```

```
# .gitignore
/target
```

Use the Makefile from spec §12 verbatim (targets: `check fmt lint test build run clean`).

- [ ] **Step 4: Verify**

Run: `cargo build && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`
Expected: clean build, no warnings (allow `dead_code` on empty mods is NOT needed — empty mods produce none).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: project scaffold with mise toolchain and lint gates"
```

---

### Task 2: Settings (`config/`)

**Files:**
- Create: `src/config/mod.rs`, `src/config/settings.rs`
- Test: `tests/config_tests.rs`

**Interfaces:**
- Produces (used by Tasks 5, 7, 8):
```rust
pub enum Target { Display, Region }
pub enum Quality { Standard, High }
pub struct Settings { pub output_directory: PathBuf, pub default_target: Target,
    pub default_fps: u32, pub quality: Quality, pub preferred_backend: String }
pub fn load() -> Settings            // never fails; warn+defaults
pub fn save(&s: &Settings) -> anyhow::Result<()>
pub fn expand_tilde(p: &str) -> PathBuf
```

- [ ] **Step 1: Failing tests** — `tests/config_tests.rs`

```rust
use niri_recorder::config::settings::*;

#[test]
fn defaults_when_file_missing() {
    let s = Settings::from_toml_str("").unwrap();
    assert!(s.output_directory.ends_with("Screen Recordings"));
    assert_eq!(s.default_fps, 30);
    assert_eq!(s.preferred_backend, "auto");
}

#[test]
fn malformed_toml_falls_back_not_fails() {
    let s = load_from_str("{{{{"); // internal helper returning Settings, never Err
    assert_eq!(s.default_fps, 30);
}

#[test]
fn fps_must_be_24_30_or_60() {
    assert!(Settings::from_toml_str("default_fps = 30").is_ok());
    assert!(Settings::from_toml_str("default_fps = 15").is_err());
}

#[test]
fn tilde_expands_to_home() {
    let p = expand_tilde("~/Videos/x");
    assert!(p.starts_with(directories::BaseDirs::new().unwrap().home_dir()));
}
```

Expose `Settings::from_toml_str(&str) -> Result<Self, ConfigError>` and `load_from_str(&str) -> Settings` (warns and returns `Default` on parse error). `load()` reads the XDG config path, calls `load_from_str` on contents or defaults on missing.

- [ ] **Step 2: Run** — `cargo test --test config_tests` → FAIL (module doesn't exist).

- [ ] **Step 3: Implement** `src/config/settings.rs`

```rust
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Target { Display, Region }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality { Standard, High }

#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Settings {
    pub output_directory: String,   // serialized form; expand at use site
    pub default_target: Target,
    pub default_fps: u32,
    pub quality: Quality,
    pub preferred_backend: String,
}
```

`Default`: `output_directory = "~/Videos/Screen Recordings"`, `default_target = Display`, `default_fps = 30`, `quality = Standard`, `preferred_backend = "auto"`. `from_toml_str` parses then validates `default_fps ∈ {24,30,60}` else `ConfigError::InvalidFps` (thiserror). `expand_tilde`: replace leading `~` with `BaseDirs::home_dir()`; non-tilde paths pass through. `config_dir()`: `ProjectDirs::from("", "", "niri-recorder").config_dir()`. `load()` reads `<config_dir>/config.toml`; `save()` creates dirs, `toml::to_string_pretty`, writes.

Export `pub mod settings;` from `src/config/mod.rs`.

- [ ] **Step 4: Run** — `cargo test --test config_tests` → all PASS.

- [ ] **Step 5: Commit** — `git add -A && git commit -m "feat: settings with XDG paths, validation, tilde expansion"`

---

### Task 3: Output naming (`files/`)

**Files:**
- Create: `src/files/mod.rs`, `src/files/naming.rs`
- Test: `tests/filename_tests.rs`

**Interfaces:**
- Produces (used by Task 7):
```rust
pub struct OutputPlan { pub part_path: PathBuf, pub final_path: PathBuf }
pub fn plan_output(dir: &Path, ext: &str) -> std::io::Result<OutputPlan>
pub fn finalize(plan: OutputPlan) -> std::io::Result<PathBuf>  // rename .part → final
```

- [ ] **Step 1: Failing tests** — `tests/filename_tests.rs` (use `tempfile::tempdir`)

```rust
#[test]
fn name_is_timestamped_mp4_in_dir() {
    let d = tempfile::tempdir().unwrap();
    let p = plan_output(d.path(), "mp4").unwrap();
    assert_eq!(p.part_path.extension().unwrap(), "part");
    let n = p.final_path.file_name().unwrap().to_str().unwrap();
    assert!(n.starts_with("screen-recording-") && n.ends_with(".mp4"));
}

#[test]
fn never_overwrites_existing() {
    let d = tempfile::tempdir().unwrap();
    let a = plan_output(d.path(), "mp4").unwrap();
    std::fs::write(&a.final_path, b"x").unwrap();          // simulate existing
    let b = plan_output(d.path(), "mp4").unwrap();
    assert_ne!(a.final_path, b.final_path);
    assert!(b.final_path.to_str().unwrap().contains("-1."));
}

#[test]
fn finalize_renames_part_to_final() {
    let d = tempfile::tempdir().unwrap();
    let p = plan_output(d.path(), "mp4").unwrap();
    std::fs::write(&p.part_path, b"data").unwrap();
    let done = finalize(p).unwrap();
    assert!(done.exists());
}
```

- [ ] **Step 2: Run** — `cargo test --test filename_tests` → FAIL.

- [ ] **Step 3: Implement** `src/files/naming.rs`: create `dir` with `create_dir_all` (mode inherits umask); base name `screen-recording-%Y-%m-%d_%H-%M-%S.{ext}` via `std::time` → format manually (`chrono` is NOT a dependency — use `time` crate? No: keep zero-dep, format `SystemTime` via `chrono`? **Decision:** add `time = { version = "0.3", features = ["formatting", "local-offset"] }` to Cargo.toml — local-time formatting without it is unsafe). Collision loop: try base, then `base-{n}` n=1..=99. `finalize` = `fs::rename`. `part_path` = `final_path` with `.part` appended (e.g. `….mp4.part`).

Add `time` dep to Cargo.toml `[dependencies]`.

- [ ] **Step 4: Run** — `cargo test --test filename_tests` → PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat: collision-safe timestamped output naming with .part staging"`

---

### Task 4: Capture types + geometry (`capture/`)

**Files:**
- Create: `src/capture/mod.rs`, `src/capture/request.rs`, `src/capture/capabilities.rs`, `src/capture/backend.rs`
- Test: `tests/geometry_tests.rs`

**Interfaces:**
- Produces (Tasks 6–8 consume):
```rust
pub struct Geometry { pub x: i32, pub y: i32, pub w: u32, pub h: u32 }
impl Geometry { pub fn parse_slurp(s: &str) -> Result<Geometry, GeometryError>;
                pub fn to_wf_arg(&self) -> String }          // "x,y WxH"
pub enum CaptureTarget { Display(String), Region(Geometry) } // Display takes output name e.g. "DP-3"
pub struct CaptureRequest { pub target: CaptureTarget, pub fps: u32,
    pub quality: Quality, pub part_path: PathBuf }
pub struct CaptureCapabilities { pub outputs: Vec<String>, pub region: bool, pub codecs: Vec<String> }
pub struct CaptureSession { pub child: Child, pub part_path: PathBuf, pub final_path: PathBuf }
pub trait CaptureBackend {
    fn capabilities(&self) -> Result<CaptureCapabilities, CaptureError>;
    fn start(&self, req: CaptureRequest) -> Result<CaptureSession, CaptureError>;
}
```

- [ ] **Step 1: Failing tests** — `tests/geometry_tests.rs`

```rust
use niri_recorder::capture::request::Geometry;

#[test] fn parses_slurp_output() { let g = Geometry::parse_slurp("100,200 800x600\n").unwrap();
    assert_eq!((g.x,g.y,g.w,g.h),(100,200,800,600)); }
#[test] fn accepts_negative_origin() { assert!(Geometry::parse_slurp("-1920,0 1920x1080").is_ok()); }
#[test] fn rejects_zero_size() { assert!(Geometry::parse_slurp("0,0 0x600").is_err()); }
#[test] fn rejects_garbage() { assert!(Geometry::parse_slurp("hello").is_err()); }
#[test] fn wf_arg_roundtrip() { assert_eq!(Geometry::parse_slurp("1,2 3x4").unwrap().to_wf_arg(), "1,2 3x4"); }
```

- [ ] **Step 2: Run** — `cargo test --test geometry_tests` → FAIL.

- [ ] **Step 3: Implement**

`request.rs`: `Geometry` with manual parse — split on space → `"x,y"` and `"WxH"`; commas/x as delimiters; `w,h > 0`; negatives allowed on x,y (multi-monitor left-of-primary layouts). `CaptureTarget`, `CaptureRequest` as above. `capabilities.rs`: plain struct. `backend.rs`: `pub struct CaptureSession { pub child: std::process::Child, pub part_path: PathBuf, pub final_path: PathBuf }`, `pub trait CaptureBackend` as above, and

```rust
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("missing dependency: {0} (install: {1})")] MissingTool(&'static str, &'static str),
    #[error("capture failed: {0}")] Spawn(std::io::Error),
    #[error("recorder exited unexpectedly: {0}")] UnexpectedExit(String),
    #[error("stop timed out; sent SIGKILL")] StopTimeout,
    #[error("backend unsupported: {0}")] Unsupported(String),
}
```

`mod.rs`: `pub mod backend; pub mod capabilities; pub mod request;` (+ `process`, `backends` added by later tasks). Re-export `CaptureError`.

- [ ] **Step 4: Run** — `cargo test --test geometry_tests` → PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat: capture types, slurp geometry parsing, backend trait"`

---

### Task 5: Subprocess lifecycle (`capture/process.rs`)

**Files:**
- Create: `src/capture/process.rs`, `tests/fake_child.sh` (executable test fixture)
- Test: `tests/process_tests.rs`

**Interfaces:**
- Produces (Task 7 consumes):
```rust
pub struct ManagedChild { pub child: std::process::Child, stderr_tail: Arc<Mutex<VecDeque<String>>> }
pub fn spawn(argv: &[&str]) -> Result<ManagedChild, CaptureError>
impl ManagedChild {
    pub fn stop_graceful(&mut self) -> Result<std::process::ExitStatus, CaptureError>;
    pub fn pid(&self) -> u32;
    pub fn stderr_tail(&self) -> String;
}
```

`spawn` drains stderr on a thread into a bounded ring (last ~50 lines) so the child never blocks on a full pipe. `stop_graceful`: SIGINT → poll `try_wait` 50ms ticks ≤5s → SIGTERM ≤3s → SIGKILL → `wait()`.

- [ ] **Step 1: Test fixture** — `tests/fake_child.sh`

```bash
#!/usr/bin/env bash
# Fake recorder: writes bytes until SIGINT, then "finalizes" and exits 0.
trap 'echo "finalized" >> "$1"; exit 0' INT
trap 'echo "killed-term" >> "$1"; exit 0' TERM
printf 'fake-ready\n'
while true; do echo data >> "$1"; sleep 0.2; done
```

Note: trap can't take $1 — write it as `trap 'echo finalized >> "$TARGET"; exit 0' INT` reading `TARGET` env var. Final form:

```bash
#!/usr/bin/env bash
trap 'echo finalized >> "$TARGET"; exit 0' INT
trap 'exit 143' TERM
: "${TARGET:?}"
printf 'fake-ready\n'
while true; do echo data >> "$TARGET"; sleep 0.2; done
```

`chmod +x` it. Tests invoke it via `spawn(&["bash", "tests/fake_child.sh"])` with `TARGET=<tempdir>/out` set — so `spawn` must accept env: change signature to `spawn(argv: &[&str], env: &[(&str,&str)])`.

- [ ] **Step 2: Failing tests** — `tests/process_tests.rs`

```rust
#[test] fn graceful_stop_reaps_and_reports_exit() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("out"); 
    let mut c = spawn(&["bash","tests/fake_child.sh"], &[("TARGET", target.to_str().unwrap())]).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let status = c.stop_graceful().unwrap();
    assert!(status.success());
    assert!(std::fs::read_to_string(&target).unwrap().contains("finalized"));
}

#[test] fn missing_binary_maps_to_missing_tool() {
    let e = spawn(&["definitely-not-a-real-binary-xyz"], &[]).unwrap_err();
    assert!(matches!(e, CaptureError::Spawn(_))); // io ErrorKind::NotFound
}
```

- [ ] **Step 3: Run** — FAIL. **Step 4: Implement** `process.rs` per the interface; use `nix::sys::signal::kill(Pid::from_raw(pid as i32), Signal::SIGINT)` etc.; pipe stderr (`Stdio::piped`), stdout to `Stdio::null()` (wf-recorder progress goes to stderr). **Step 5: Run** — PASS. **Step 6: Commit** — `git commit -m "feat: managed subprocess with drained stderr and bounded SIGINT→TERM→KILL stop"`

---

### Task 6: State machine (`app/state.rs`)

**Files:** Create `src/app/mod.rs`, `src/app/state.rs`. Test `tests/state_machine_tests.rs`.

**Interfaces:**
```rust
pub enum AppState { Idle, SelectingRegion, Starting, Recording { pid: u32 },
    Stopping { pid: u32 }, Completed(PathBuf), Failed(String) }
pub enum Event { StartDisplay, StartRegion, SelectionDone(Geometry), SelectionCancelled,
    Spawned(u32), SpawnFailed(String), StopRequested, Exited { ok: bool, path: PathBuf },
    Died(String) }
pub fn transition(s: AppState, e: Event) -> AppState   // invalid → unchanged (log at call site)
```

- [ ] **Step 1: Failing tests** covering: Idle→Recording happy path via StartDisplay→Spawned; StartRegion→SelectingRegion→SelectionCancelled→Idle; Recording→StopRequested→Stopping→Exited→Completed; Recording→Died→Failed; **duplicate-start rejected**: `transition(Recording{pid:1}, StartDisplay)` returns `Recording{pid:1}` unchanged. ~8 tests, one per row of spec §10's diagram.

- [ ] **Step 2–5:** Standard TDD cycle; `transition` is a pure function — a `match` on `(state, event)`. Commit: `feat: explicit recorder state machine`.

---

### Task 7: wf-recorder backend (`capture/backends/wf_recorder.rs`)

**Files:** Create `src/capture/backends/mod.rs`, `src/capture/backends/wf_recorder.rs`. Test `tests/backend_tests.rs`.

**Interfaces:**
```rust
pub struct WfRecorder;
impl WfRecorder {
    pub fn detect() -> Result<Self, CaptureError>;          // checks binary + slurp exist
    pub fn list_outputs() -> Vec<String>;                    // `niri msg --json outputs` → names
    fn argv(req: &CaptureRequest) -> Vec<String>;            // pure, testable
}
impl CaptureBackend for WfRecorder { /* capabilities(), start() */ }
```

- [ ] **Step 1: Failing tests** — `backend_tests.rs` on the pure `argv()`:

```rust
#[test] fn display_argv_has_output_and_no_audio() {
    let req = req_for(CaptureTarget::Display("DP-3".into()));
    let a = WfRecorder::argv(&req);
    assert!(a.contains(&"-o".to_string()) && a.contains(&"DP-3".to_string()));
    assert!(!a.iter().any(|x| x.contains('a') && x == "-a"));
    assert_eq!(a.iter().position(|x| x=="-f").map(|i| &a[i+1]), Some(&req.part_path.to_str().unwrap().to_string()));
}
#[test] fn region_argv_passes_geometry() { /* -g "1,2 3x4" */ }
#[test] fn codec_is_openh264_and_fps_flag() { /* contains "-c","libopenh264" and "-r","30" */ }
#[test] fn never_overwrites_flag() { /* assert "-y"-style overwrite flags absent; wf-recorder has none, assert no "--overwrite" */ }
```

- [ ] **Step 2–3: Implement** — `argv` builds: `["wf-recorder", "-f", part_path, "-c", codec(req.quality), "-r", fps]` + `-o name` or `-g geom`. Quality → encoder params: `Standard` → `-p crf=`-equivalent for openh264 is bitrate: pass `-b 5M`? wf-recorder `-p name=value` are ffmpeg encoder opts: `Standard` → `-p bitrate=5M`? Keep simple: `Standard` → no extra opts (2 Mbps default observed), `High` → `-p bitrate=10000000`. `detect()`: `which`-style check via `Command::new("wf-recorder").arg("--help")` spawn-success probe, same for `slurp`; missing → `MissingTool("wf-recorder", "sudo dnf install wf-recorder")`. `list_outputs()`: run `niri msg --json outputs`, parse keys with `serde_json` (add `serde_json = "1"` dep). `capabilities()`: outputs from `list_outputs`, `region: slurp present`, `codecs: ["libopenh264","libvpx-vp9"]`. `start()`: `spawn(&argv, &[])` → `CaptureSession`.

- [ ] **Step 4:** `cargo test --test backend_tests` PASS. **Step 5: Commit** — `feat: wf-recorder backend with capability detection and no-audio argv`.

---

### Task 8: Controller + pidfile (`app/controller.rs`)

**Files:** Create `src/app/controller.rs`. Test `tests/controller_tests.rs` (uses fake child via a `TestBackend` implementing `CaptureBackend` — trait pays off here).

**Interfaces:**
```rust
pub struct Recorder<B: CaptureBackend> { state: AppState, backend: B, settings: Settings, session: Option<CaptureSession> }
impl<B: CaptureBackend> Recorder<B> {
    pub fn start(&mut self, req_kind: TargetKind) -> Result<(), CaptureError>;
    pub fn stop(&mut self) -> Result<PathBuf, CaptureError>;   // returns finalized path
    pub fn state(&self) -> &AppState;
}
pub fn write_pidfile() -> io::Result<PathBuf>;       // XDG_RUNTIME_DIR/niri-recorder/recorder.pid, mode 0600, holds our pid+argv[0]
pub fn read_pidfile() -> Option<PidEntry>;           // None if stale/dead
pub fn clear_pidfile();
pub struct PidEntry { pub pid: u32 }
```

`start(Display)`: plan_output → `backend.start` → state Recording, session stored. `start(Region)`: spawn `slurp` (managed, no trailing args), read its stdout on completion → `Geometry::parse_slurp` → proceed; slurp exit≠0 or empty → `SelectionCancelled` → Idle. `stop()`: `session.child.stop_graceful()` → check `part_path` non-empty → `finalize()` → Completed.

- [ ] **Step 1: Failing tests** — a `FakeBackend` in the test file returning a `CaptureSession` wrapping the `fake_child.sh` spawn writing into tempdir. Tests: start→stop yields finalized file and Completed state; double-start is a no-op error; stop-from-Idle errors; pidfile write/read/clear roundtrip; stale pidfile (write pid 999999) → `read_pidfile()==None` and file removed.

- [ ] **Step 2–5:** TDD; implement. `TargetKind = Target` reused from settings. Commit: `feat: recorder controller, state machine integration, XDG pidfile`.

---

### Task 9: CLI (`cli/`)

**Files:** Create `src/cli/mod.rs`, `src/cli/commands.rs`.

**Interfaces:** `pub fn run<I: IntoIterator<Item = OsString>>(args: I) -> anyhow::Result<()>` — called by `main.rs`.

- [ ] **Step 1: Implement** (thin — mostly declarative clap):

```rust
#[derive(Parser)] #[command(name="niri-recorder", about="Screen recorder for Wayland/Niri")]
struct Cli { #[command(subcommand)] cmd: Option<Cmd> }
#[derive(Subcommand)]
enum Cmd {
    Start { #[arg(long, value_enum, default_value_t=TargetArg::Display)] target: TargetArg,
            #[arg(long)] output: Option<String> },          // display name; default = sole/first output
    Stop, Status, Diagnostics,
}
```

- `start`: `Settings::load()` → `WfRecorder::detect()` → `write_pidfile()` → `Recorder::start(target)` → `println!("recording → {}", part)` → wait on child; on SIGINT (ctrlc handler sets flag → or simpler: the recorder IS the foreground process, so install a signal hook with `nix`/minimal handler that calls `stop`) — simplest correct: spawn-signal-handler via `ctrlc` crate? **Decision: add `ctrlc = "3"` dep** — on SIGINT call `recorder.stop()`, print final path, `clear_pidfile()`, exit 0. Exit codes: 0 ok, 2 backend-missing, 3 capture failure.
- `stop`: `read_pidfile()` → `kill(pid, SIGINT)` → print "stop requested (pid N)"; stale → "not recording", exit 1.
- `status`: pidfile+state → print `recording pid=N` or `idle`.
- `diagnostics`: print niri version (`niri --version`), outputs (`niri msg outputs`), tool presence+versions (`wf-recorder --version`, `slurp -h` first line, `ffprobe -version` first line), active backend, config path.

- [ ] **Step 2: Test** — CLI arg-parse unit tests in `commands.rs` (`Cli::try_parse_from`) covering: `start` default target=display; `start --target region`; invalid `--target` rejected; `stop`/`status` parse. Run `cargo test`.

- [ ] **Step 3: Manual smoke** — on this machine: `cargo run -- diagnostics`, `cargo run -- start --target display`, `cargo run -- status`, `cargo run -- stop` in another shell; ffprobe the output: video-only, finalized. Record results.

- [ ] **Step 4: Commit** — `feat: CLI with start/stop/status/diagnostics over pidfile control`.

---

### Task 10: Docs + repo files

**Files:** Create `README.md`, `AGENTS.md`, `DESIGN.md`, `docs/{CAPTURE-BACKENDS,WAYLAND-NIRI-COMPATIBILITY,TESTING,TROUBLESHOOTING,ROADMAP,REQUIREMENTS,UX}.md`.

- [ ] **Step 1:** Write them — `AGENTS.md` and `README.md` content per spec §12 verbatim requirements; `CAPTURE-BACKENDS.md` = spec §22 findings verbatim + wf-recorder command lines; `WAYLAND-NIRI-COMPATIBILITY.md` = verified matrix + manual-verification list (interactive slurp, multi-monitor); `DESIGN.md` = condensed architecture + state machine + lifecycle from spec §§6,10,13; remaining docs as focused one-screen files from their spec sections. `docs/` stubs must contain real content from the spec, not placeholders.
- [ ] **Step 2:** `make check` green. **Step 3: Commit** — `docs: repo docs, AGENTS rules, compatibility evidence`.

---

## Self-review notes

- **Spec coverage (Plan A scope):** §6 backend abstraction ✓T4/T7, §7 FR-02 geometry ✓T4/T8 (interactive slurp deferred to smoke), FR-05 no-audio ✓T7 test, FR-06 controls ✓T6/T8/T9, FR-07 output naming ✓T3, FR-09 settings ✓T2, §9 CLI ✓T9 (pidfile model per §23.2), §10 lifecycle ✓T5/T6, §13 config ✓T2. **Deferred to Plan B:** GUI (§8), FR-01/FR-04 display-picker UI, exit-while-recording prompt, recent-recordings list. **Manual-only:** interactive slurp, multi-monitor — flagged in spec §22, listed in compatibility doc.
- **Type consistency:** `Target`, `Quality`, `Geometry`, `CaptureRequest`, `CaptureSession`, `CaptureError`, `AppState`, `Event`, `Recorder`, `PidEntry` — names identical across tasks.
- **Deps added mid-plan:** `time` (T3), `serde_json` (T7), `ctrlc` (T9) — add each when the task that needs it runs.
