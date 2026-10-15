use niri_recorder::capture::backends::wf_recorder::WfRecorder;
use niri_recorder::capture::request::{CaptureRequest, CaptureTarget, Geometry};
use niri_recorder::config::settings::Quality;
use std::path::PathBuf;

fn req_for(target: CaptureTarget) -> CaptureRequest {
    CaptureRequest {
        target,
        fps: 30,
        quality: Quality::Standard,
        part_path: PathBuf::from("/tmp/rec-1.mp4.part"),
    }
}

fn flag_value<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    argv.iter()
        .position(|a| a == flag)
        .and_then(|i| argv.get(i + 1))
        .map(String::as_str)
}

#[test]
fn display_argv_has_output_and_no_audio() {
    let req = req_for(CaptureTarget::Display("DP-3".into()));
    let a = WfRecorder::argv(&req);
    assert_eq!(flag_value(&a, "-o"), Some("DP-3"));
    assert_eq!(a[0], "wf-recorder");
    assert!(!a.iter().any(|x| x == "-a" || x.starts_with("--audio")));
    assert_eq!(flag_value(&a, "-f"), Some(req.part_path.to_str().unwrap()));
}

#[test]
fn region_argv_passes_geometry() {
    let req = req_for(CaptureTarget::Region(Geometry {
        x: 1,
        y: 2,
        w: 3,
        h: 4,
    }));
    let a = WfRecorder::argv(&req);
    assert_eq!(flag_value(&a, "-g"), Some("1,2 3x4"));
    assert!(!a.iter().any(|x| x == "-o"));
}

#[test]
fn codec_is_openh264_and_fps_flag() {
    let a = WfRecorder::argv(&req_for(CaptureTarget::Display("DP-3".into())));
    assert_eq!(flag_value(&a, "-c"), Some("libopenh264"));
    assert_eq!(flag_value(&a, "-r"), Some("30"));
    // `.part` defeats muxer inference — `-m` must be explicit.
    assert_eq!(flag_value(&a, "-m"), Some("mp4"));
}

#[test]
fn webm_ext_selects_vp9_and_webm_muxer() {
    let mut req = req_for(CaptureTarget::Display("DP-3".into()));
    req.part_path = PathBuf::from("/tmp/rec-1.webm.part");
    let a = WfRecorder::argv(&req);
    assert_eq!(flag_value(&a, "-m"), Some("webm"));
    assert_eq!(flag_value(&a, "-c"), Some("libvpx-vp9"));
}

#[test]
fn standard_has_no_encoder_overrides_high_sets_bitrate() {
    let mut req = req_for(CaptureTarget::Display("DP-3".into()));
    let std_argv = WfRecorder::argv(&req);
    assert!(!std_argv.iter().any(|x| x == "-p"));
    req.quality = Quality::High;
    let high_argv = WfRecorder::argv(&req);
    let b_idx = high_argv.iter().position(|x| x == "b=10000000");
    assert!(b_idx.is_some() && high_argv[b_idx.unwrap() - 1] == "-p");
}

#[test]
fn never_overwrites_flag() {
    let a = WfRecorder::argv(&req_for(CaptureTarget::Display("DP-3".into())));
    assert!(!a
        .iter()
        .any(|x| { x == "-y" || x == "--overwrite" || x == "-n" || x.starts_with("--overwrite") }));
}
