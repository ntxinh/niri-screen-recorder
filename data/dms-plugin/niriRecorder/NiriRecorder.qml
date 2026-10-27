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
