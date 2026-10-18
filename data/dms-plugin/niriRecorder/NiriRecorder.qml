// Niri Recorder — DankMaterialShell bar widget + Control Center tile.
//
// The widget only talks to the niri-recorder CLI: `status` is polled,
// and start/stop use argv arrays (never a shell string). With the
// `niri-recorder daemon` running, `start` forwards into the daemon and
// returns immediately; without it, the foreground recorder keeps this
// Process alive until the recording stops.
//
// UNVERIFIED: written against the DMS 1.6 plugin docs; not exercised in
// a live DankMaterialShell session (see docs/DMS.md).

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

    function refresh() {
        if (!statusProc.running)
            statusProc.running = true;
    }

    function toggle() {
        if (recording || pending) {
            stopProc.running = true;
        } else {
            pending = true;
            startProc.running = true;
        }
        refreshSoon.restart();
    }

    Process {
        id: statusProc
        command: ["niri-recorder", "status"]
        stdout: SplitParser {
            onRead: line => root.recording = line.trim().startsWith("recording")
        }
    }

    Process {
        id: startProc
        command: ["niri-recorder", "start", "--target", "display"]
        onExited: (exitCode) => {
            root.pending = false;
            if (exitCode !== 0)
                ToastService.showError("Niri Recorder", "start failed (exit " + exitCode + ")");
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
        interval: 400
        repeat: false
        onTriggered: root.refresh()
    }

    // Cheap poll: `status` is a pidfile read, not capture work.
    Timer {
        interval: 2000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.refresh()
    }

    // --- Control Center tile --------------------------------------------
    ccWidgetIcon: root.recording ? "stop_circle" : "radio_button_checked"
    ccWidgetPrimaryText: "Niri Recorder"
    ccWidgetSecondaryText: root.recording
        ? "Recording — click to stop"
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
                    name: root.recording ? "radio_button_checked" : "radio_button_unchecked"
                    size: Theme.iconSize
                    color: root.recording ? Theme.error : Theme.surfaceVariantText
                    anchors.verticalCenter: parent.verticalCenter
                }

                StyledText {
                    visible: root.recording
                    text: "REC"
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
                    name: root.recording ? "radio_button_checked" : "radio_button_unchecked"
                    size: Theme.iconSize
                    color: root.recording ? Theme.error : Theme.surfaceVariantText
                    anchors.horizontalCenter: parent.horizontalCenter
                }

                StyledText {
                    visible: root.recording
                    text: "REC"
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
