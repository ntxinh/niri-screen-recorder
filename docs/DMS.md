# DankMaterialShell integration

A DMS bar widget + Control Center tile lives in
`data/dms-plugin/niriRecorder/`. It is a normal DMS plugin (`type:
"widget"`, capabilities `dankbar-widget` and `control-center`) that shells
out to the `niri-recorder` CLI — it does not embed capture code.

## Install

```sh
mkdir -p ~/.config/DankMaterialShell/plugins
cp -r data/dms-plugin/niriRecorder ~/.config/DankMaterialShell/plugins/
dms ipc call plugins reload niriRecorder   # or: dms restart
```

Then enable **Niri Recorder** in Settings → Plugins and add it to the
DankBar layout. Requires DMS ≥ 1.5.0 (manifest declares it); written
against the DMS 1.6 plugin docs.

## Behavior

- Polls `niri-recorder status` every 2 s (a pidfile/daemon-socket read,
  negligible cost).
- Bar pill: outline circle when idle, red filled circle + `REC` while
  recording. Click toggles.
- Control Center tile: secondary text says Idle/Recording/Starting.
- Start always uses `--target display`; the configured default display
  and encoder/quality settings still come from `config.toml`.
- With `niri-recorder daemon` running, `start` returns immediately and
  the recording is owned by the daemon. Without it, the foreground
  recorder process stays alive as a child of the Quickshell `Process`.
- Errors surface as DMS toasts; the CLI's stderr/exit codes are the
  source of truth.

## Verification status

Verified on the target machine (DMS 1.6, Quickshell `qs -p
/usr/share/quickshell/dms`) on 2026-10-18:

- `make install-dms-plugin` placed the files under
  `~/.config/DankMaterialShell/plugins/niriRecorder/`.
- `dms ipc call plugins list` listed `niriRecorder [disabled]`.
- `dms ipc call plugins reload niriRecorder` returned
  `PLUGIN_RELOAD_SUCCESS` and the plugin showed `[loaded]`.
- The DMS log recorded `DankBar: Plugin loaded: niriRecorder` with no QML
  errors.

The plugin was removed again after the check to leave the session's
plugin set untouched.

**Still manual-verification items:** adding the pill to a bar layout,
rendering the idle/recording states, click-to-record/stop, and the
Control Center tile. These need a live click and bar configuration.

If the pill does not receive clicks in your DMS build, compare with the
`MouseArea` pattern used by other bar widgets in
`DankMaterialShell/quickshell/PLUGINS`.

## Why not a DMS IPC command?

DMS IPC (`dms ipc call …`) controls DMS itself; it has no generic
"register a third-party target" API. A plugin is the supported extension
point.
