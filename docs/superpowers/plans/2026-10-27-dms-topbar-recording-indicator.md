# DankMaterialShell Top Bar Recording Indicator & Desktop Notifications Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide an auto-hiding DankMaterialShell top bar indicator with live elapsed timer for screen recording, and system `notify-send` notifications on start/stop/save.

**Architecture:** DankMaterialShell's `PluginComponent` controls bar visibility via `_visibilityOverrideValue` bound to recording state so the widget collapses to 0 width when idle and animates into view with a live timer when recording. The `niri-recorder` core binary sends `notify-send` desktop notifications on start, clean save, and failures across all triggers (Niri hotkeys, DMS bar click, CLI).

**Tech Stack:** QML / Quickshell / DankMaterialShell (DMS), Rust (`wf-recorder`, `notify-send`, `nix`, `clap`), Wayland/Niri.

## Global Constraints

- Never add audio capture or audio flags.
- Never execute shell command strings assembled from user input; spawn subprocesses with argv arrays only.
- Keep UI logic separate from capture process management; front-end code talks to `niri-recorder` CLI/daemon.
- Run `make check` (fmt --check, clippy `-D warnings`, all tests, build) before declaring completion.

---

### Task 1: Rebuild and Update `niri-recorder` Binary with Notification Support

**Files:**
- Modify: `~/.cargo/bin/niri-recorder`
- Test: `tests/backend_tests.rs`, `tests/daemon_tests.rs`

**Interfaces:**
- Consumes: `src/app/notify.rs::notify(bool, &str, &str)`
- Produces: Installed binary at `~/.cargo/bin/niri-recorder` with `--notify` and `--no-notify` CLI flags and notification dispatch.

- [ ] **Step 1: Run test suite to verify current codebase passes**

Run: `cargo test`
Expected: PASS (all unit and integration tests pass)

- [ ] **Step 2: Build release binary and verify CLI flags**

Run:
```bash
cargo build --release
target/release/niri-recorder start --help | grep -E "notify|no-notify"
```
Expected: Output showing `--notify` and `--no-notify` options.

- [ ] **Step 3: Install updated binary to `~/.cargo/bin/niri-recorder`**

Run:
```bash
cargo install --path . --force
which niri-recorder
niri-recorder --version
```
Expected: Installed to `/home/exodia/.cargo/bin/niri-recorder` matching current package version.

- [ ] **Step 4: Verify installed binary sends a notification when invoked**

Run:
```bash
notify-send --app-name niri-recorder --icon media-record "Test Notification" "Testing notify-send from plan"
```
Expected: Desktop notification appears cleanly without error.

---

### Task 2: Update DankMaterialShell Plugin with Auto-Hide & Live Elapsed Timer

**Files:**
- Modify: `data/dms-plugin/niriRecorder/NiriRecorder.qml`

**Interfaces:**
- Consumes: `niri-recorder status` (returns `recording pid=N` or `idle`), `niri-recorder stop`, `niri-recorder start --target display`
- Produces: `PluginComponent` with `_visibilityOverrideValue` bound to active state, live timer formatting, and click-to-stop pill.

- [ ] **Step 1: Update `NiriRecorder.qml` implementation**

Update `data/dms-plugin/niriRecorder/NiriRecorder.qml` with:
1. `_visibilityOverride: true` and `_visibilityOverrideValue: root.recording || root.pending`
2. Elapsed seconds counter driven by a 1000ms timer
3. Time formatting helper `elapsedDisplay` (`mm:ss` or `hh:mm:ss`)
4. 1000ms polling interval on `niri-recorder status`
5. Bar pill displaying red `radio_button_checked` + `REC mm:ss`, with `MouseArea` click calling `root.toggle()`
6. State reset when recording stops

Replace contents of `data/dms-plugin/niriRecorder/NiriRecorder.qml`:
```qml
// Niri Recorder — DankMaterialShell bar widget + Control Center tile.
//
// The widget talks to the niri-recorder CLI: `status` is polled every 1s,
// and start/stop use argv arrays. When idle, the bar pill collapses
// completely so it takes up zero space. While recording, it animates onto
// the bar with a red indicator and live elapsed timer. Clicking stops it.

import QtQuick
import Quickshell
import Quickshell.Io
import qs.Common
import qs.Services
import qs.Modules.Plugins
import qs.Widgets

PluginComponent {
    id: root

    layerNamespacePlugin: "niri-recorder"

    property bool recording: false
    property bool pending: false
    property int elapsedSeconds: 0
    property var recordingStartTime: null

    // Auto-hide: pill collapses to width: 0 when idle.
    _visibilityOverride: true
    _visibilityOverrideValue: root.recording || root.pending

    function pad2(n) { return n < 10 ? "0" + n : "" + n; }

    readonly property string elapsedDisplay: {
        var h = Math.floor(elapsedSeconds / 3600);
        var m = Math.floor((elapsedSeconds % 3600) / 60);
        var s = elapsedSeconds % 60;
        return h > 0 ? pad2(h) + ":" + pad2(m) + ":" + pad2(s)
                     : pad2(m) + ":" + pad2(s);
    }

    function refresh() {
        if (!statusProc.running)
            statusProc.running = true;
    }

    function toggle() {
        if (recording || pending) {
            stopProc.running = true;
        } else {
            pending = true;
            recordingStartTime = Date.now();
            elapsedSeconds = 0;
            startProc.running = true;
        }
        refreshSoon.restart();
    }

    Process {
        id: statusProc
        command: ["niri-recorder", "status"]
        stdout: SplitParser {
            onRead: line => {
                var wasRecording = root.recording;
                root.recording = line.trim().startsWith("recording");
                if (root.recording && !wasRecording && !root.recordingStartTime) {
                    root.recordingStartTime = Date.now();
                    root.elapsedSeconds = 0;
                } else if (!root.recording && wasRecording) {
                    root.recordingStartTime = null;
                    root.elapsedSeconds = 0;
                }
            }
        }
    }

    Process {
        id: startProc
        command: ["niri-recorder", "start", "--target", "display"]
        onExited: (exitCode) => {
            root.pending = false;
            if (exitCode !== 0) {
                root.recordingStartTime = null;
                root.elapsedSeconds = 0;
                ToastService.showError("Niri Recorder", "start failed (exit " + exitCode + ")");
            }
            refreshSoon.restart();
        }
    }

    Process {
        id: stopProc
        command: ["niri-recorder", "stop"]
        onExited: (exitCode) => {
            if (exitCode !== 0)
                ToastService.showError("Niri Recorder", "stop failed (exit " + exitCode + ")");
            refreshSoon.restart();
        }
    }

    Timer {
        id: refreshSoon
        interval: 300
        repeat: false
        onTriggered: root.refresh()
    }

    // Status polling: 1000ms
    Timer {
        interval: 1000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.refresh()
    }

    // Elapsed timer: ticks every 1s while recording
    Timer {
        interval: 1000
        repeat: true
        running: root.recording
        onTriggered: {
            if (root.recording && root.recordingStartTime) {
                root.elapsedSeconds = Math.max(0, Math.floor((Date.now() - root.recordingStartTime) / 1000));
            }
        }
    }

    // --- Control Center tile --------------------------------------------
    ccWidgetIcon: root.recording ? "stop_circle" : "radio_button_checked"
    ccWidgetPrimaryText: "Niri Recorder"
    ccWidgetSecondaryText: root.recording
        ? "Recording (" + root.elapsedDisplay + ") — click to stop"
        : (root.pending ? "Starting…" : "Idle — click to record")
    ccWidgetIsActive: root.recording || root.pending
    onCcWidgetToggled: root.toggle()

    // --- DankBar pills ---------------------------------------------------
    horizontalBarPill: Component {
        Item {
            implicitWidth: pillRow.implicitWidth
            implicitHeight: pillRow.implicitHeight

            Row {
                id: pillRow
                spacing: Theme.spacingS

                DankIcon {
                    name: root.pending ? "hourglass_top" : "radio_button_checked"
                    size: Theme.iconSize
                    color: root.pending ? Theme.primary : Theme.error
                    anchors.verticalCenter: parent.verticalCenter
                }

                StyledText {
                    text: root.pending ? "REC …" : "REC " + root.elapsedDisplay
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.error
                    anchors.verticalCenter: parent.verticalCenter
                }
            }

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.toggle()
            }
        }
    }

    verticalBarPill: Component {
        Item {
            implicitWidth: pillColumn.implicitWidth
            implicitHeight: pillColumn.implicitHeight

            Column {
                id: pillColumn
                spacing: Theme.spacingXS

                DankIcon {
                    name: root.pending ? "hourglass_top" : "radio_button_checked"
                    size: Theme.iconSize
                    color: root.pending ? Theme.primary : Theme.error
                    anchors.horizontalCenter: parent.horizontalCenter
                }

                StyledText {
                    text: root.elapsedDisplay
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.error
                    anchors.horizontalCenter: parent.horizontalCenter
                }
            }

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.toggle()
            }
        }
    }
}
```

- [ ] **Step 2: Commit plugin changes to git**

Run:
```bash
git add data/dms-plugin/niriRecorder/NiriRecorder.qml
git commit -m "feat(dms): add auto-hide and live elapsed timer to niriRecorder bar pill"
```
Expected: Clean git commit.

---

### Task 3: Deploy Plugin and Configure DankMaterialShell Top Bar

**Files:**
- Deploy to: `~/.config/DankMaterialShell/plugins/niriRecorder/`
- Modify: `~/.config/DankMaterialShell/plugin_settings.json`
- Modify: `~/.config/DankMaterialShell/settings.json`

**Interfaces:**
- Consumes: `dms ipc call plugins reload niriRecorder`
- Produces: Loaded `niriRecorder` plugin in DankBar `leftWidgets` layout.

- [ ] **Step 1: Install plugin files into DankMaterialShell plugins directory**

Run:
```bash
mkdir -p ~/.config/DankMaterialShell/plugins/niriRecorder
cp -r data/dms-plugin/niriRecorder/* ~/.config/DankMaterialShell/plugins/niriRecorder/
ls -la ~/.config/DankMaterialShell/plugins/niriRecorder/
```
Expected: `NiriRecorder.qml` and `plugin.json` present in target directory.

- [ ] **Step 2: Ensure `niriRecorder` is enabled in `plugin_settings.json`**

Ensure `~/.config/DankMaterialShell/plugin_settings.json` has `"niriRecorder": { "enabled": true }`.
Run:
```bash
python3 -c '
import json
path = "/home/exodia/.config/DankMaterialShell/plugin_settings.json"
try:
    with open(path, "r") as f:
        data = json.load(f)
except Exception:
    data = {}
data["niriRecorder"] = {"enabled": True}
with open(path, "w") as f:
    json.dump(data, f, indent=2)
print("plugin_settings.json updated successfully")
'
```
Expected: `plugin_settings.json updated successfully`

- [ ] **Step 3: Update `settings.json` to place `niriRecorder` into `leftWidgets`**

Replace `wlScreenrecWrapper` with `niriRecorder` in `barConfigs[0].leftWidgets`:
Run:
```bash
python3 -c '
import json
path = "/home/exodia/.config/DankMaterialShell/settings.json"
with open(path, "r") as f:
    data = json.load(f)

for bar in data.get("barConfigs", []):
    widgets = bar.get("leftWidgets", [])
    new_widgets = []
    found = False
    for w in widgets:
        w_id = w.get("id") if isinstance(w, dict) else w
        if w_id in ("wlScreenrecWrapper", "niriRecorder"):
            if not found:
                new_widgets.append({"id": "niriRecorder", "enabled": True})
                found = True
        else:
            new_widgets.append(w)
    if not found:
        new_widgets.append({"id": "niriRecorder", "enabled": True})
    bar["leftWidgets"] = new_widgets

with open(path, "w") as f:
    json.dump(data, f, indent=2)
print("settings.json updated successfully with niriRecorder in leftWidgets")
'
```
Expected: `settings.json updated successfully with niriRecorder in leftWidgets`

- [ ] **Step 4: Reload plugin in DMS**

Run:
```bash
dms ipc call plugins reload niriRecorder
dms ipc call plugins list
```
Expected: `niriRecorder [loaded]` appears in plugin list.

---

### Task 4: Full Verification and Checks

**Files:**
- Test: Full integration via `make check` and manual recording test

- [ ] **Step 1: Run `make check`**

Run: `make check`
Expected: Formatting check passes, clippy with `-D warnings` passes, all unit/backend/daemon tests pass.

- [ ] **Step 2: Functional verification of idle state**

Run:
```bash
niri-recorder status
```
Expected: `idle`. Top bar shows NO `niriRecorder` icon (completely collapsed).

- [ ] **Step 3: Functional verification of recording start and indicator**

Start a 3-second display recording:
Run:
```bash
niri-recorder start --target display &
REC_PID=$!
sleep 2
niri-recorder status
```
Expected:
1. Desktop notification pops up: `"Recording started: Capturing <output>"`.
2. DMS top bar displays red recording icon and `"REC 00:0x"`.
3. `niri-recorder status` returns `recording pid=...`.

- [ ] **Step 4: Functional verification of stop and auto-hide**

Run:
```bash
niri-recorder stop
wait $REC_PID 2>/dev/null || true
sleep 1
niri-recorder status
```
Expected:
1. Desktop notification pops up: `"Recording saved: /home/exodia/Videos/rec_...mp4"`.
2. DMS top bar icon collapses and disappears completely.
3. `niri-recorder status` returns `idle`.

- [ ] **Step 5: Verify recorded MP4 file exists and is valid**

Run:
```bash
LATEST=$(ls -t ~/Videos/rec_*.mp4 2>/dev/null | head -n 1)
echo "Latest recording: $LATEST"
test -f "$LATEST" && ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 "$LATEST"
```
Expected: Outputs a positive non-zero duration float (e.g. `2.5`). Clean up the 2-second test file if desired.
