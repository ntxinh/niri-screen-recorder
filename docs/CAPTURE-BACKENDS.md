# Capture backends — evaluation and evidence

Phase 0 spike results, verbatim from the spec §22 (verified on target,
2026-10-15). Target machine: Fedora 44, native Wayland (`wayland-1`),
Niri, single output DP-3 (Dell P2419H 1920×1080@60), Intel CometLake-H
UHD (iHD 26.2.4).

## Backend verdicts

| Backend | Verdict | Evidence |
|---|---|---|
| `wf-recorder` 0.6.0 (dnf: `wf-recorder-0.6.0-2.fc44`) | **Chosen** | All gate scenarios pass; see below |
| `wl-screenrec` 0.2.0 (max buildable on F44 ffmpeg) | **Rejected on this hardware** | Unconditionally inserts a `scale_vaapi` filter; iHD exposes zero H.264 encode profiles on CometLake → `Failed to create processing pipeline config: 12 (the requested VAProfile is not supported)` and panic at `filter.rs`. `--no-hw` and bogus `--dri-device` do not bypass it. Revisit if a newer version builds or VAAPI drivers change. |
| XDG ScreenCast portal + PipeWire | **Rejected as primary** | `niri-portals.conf` declares no ScreenCast impl; installed backends are `gnome`/`gtk` (gnome portal's ScreenCast expects mutter). Fedora ffmpeg ships no `pipewire` demuxer. |

## Gate results (wf-recorder + libopenh264 → MP4/H.264)

- Full display: `wf-recorder -f out.mp4 -c libopenh264 -r 30` → 1920×1080
  H.264 MP4, video-only stream, finalized on SIGINT.
- Region: `-g "100,100 800x600"` → exact 800×600 output. `-g` accepts
  slurp's `x,y WxH` output verbatim.
- Workspace survival: 4 `niri msg action focus-workspace` switches during
  a 16.4s capture → 494 frames, uninterrupted, playable.
- Stop: SIGINT finalizes cleanly (moov atom written; ffprobe parses).
  Fallback: `SIGTERM`, bounded timeout, then `SIGKILL`.
- Audio: absent (single `video` stream per ffprobe); `-a` is opt-in and
  will never be passed.

## Caveats from the spike

- Output is **variable frame rate** (wlr-screencopy damage tracking;
  `fps=30` filter averages ~30). `-D`/`--no-damage` forces CFR at higher
  cost. `avg_frame_rate` ~30 confirmed.
- `libopenh264` is Fedora's only packaged H.264 software encoder (no
  `libx264` in Fedora ffmpeg). Quality presets map to bitrate via `-p`.
- Interactive `slurp` could not be driven headlessly; only the geometry
  contract it emits was verified. First interactive run is a
  manual-verification item.
- Multi-monitor behaviour untestable on this machine (single output).

## Command lines the code actually builds

`WfRecorder::argv` (`src/capture/backends/wf_recorder.rs`) produces,
in order:

```text
wf-recorder -f <name>.mp4.part -m mp4 -c libopenh264 -r <fps>
    [ -p b=10000000 -p rc_mode=bitrate -p allow_skip_frames=true ]   # quality=high only
    -o <output>        # --target display
    -g "<x>,<y> <w>x<h>"   # --target region, slurp geometry verbatim
```

- `-m mp4` names the muxer explicitly because the `.part` staging
  extension defeats wf-recorder's format inference.
- `-c`/`libopenh264` → MP4 is the default. If the part path's inner
  extension is `.webm`, the backend selects `-m webm -c libvpx-vp9`
  instead (codec/muxer always match the container).
- **Standard** quality passes no `-p` encoder options (encoder default
  bitrate). **High** passes the AVOptions above — `b` is the
  AVCodecContext bitrate option (openh264 has no bitrate priv-opt);
  verified to log a 10000 kb/s target.
- No audio flag (`-a`/`--audio`) is ever emitted; asserted by
  `tests/backend_tests.rs`.
- `encoder` (Phase 6) replaces the container default: `-c <name>` where
  `name` comes from `capture::encoders::resolve`. `auto`/`software` keep
  `libopenh264` (MP4) / `libvpx-vp9` (WebM); `hardware` picks the first
  encoder listed by `ffmpeg -encoders` in the order
  `h264_vaapi` → `h264_nvenc` → `h264_qsv` → `h264_v4l2m2m` (VP9 VAAPI/QSV
  for WebM). An unlisted explicit name falls back to software with a
  warning. **Availability is not verification**: this CometLake iHD
  machine lists VAAPI encoders but the Phase 0 spike found no usable
  H.264 encode profile, so hardware encoding is unverified.
- Cursor control is not offered: wf-recorder 0.6.0 (`--help`) exposes no
  cursor-visibility flag, so the compositor decides.

## Backend abstraction

`CaptureBackend` (`src/capture/backend.rs`) keeps the trait surface so
`wl-screenrec` or a portal backend can be added later without changing
`app::controller::Recorder`. Plan A implements only `WfRecorder`; do not
add backend files until they are implemented or actively evaluated.
