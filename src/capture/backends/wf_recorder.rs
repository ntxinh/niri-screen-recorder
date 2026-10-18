//! `wf-recorder` backend: captures a niri output or a slurp-picked region.

use crate::capture::backend::{CaptureBackend, CaptureError, CaptureSession};
use crate::capture::capabilities::CaptureCapabilities;
use crate::capture::encoders;
use crate::capture::process::spawn;
use crate::capture::request::{CaptureRequest, CaptureTarget};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Video-only backend wrapping the `wf-recorder` CLI.
pub struct WfRecorder {
    slurp_available: bool,
}

impl WfRecorder {
    /// Probe for `wf-recorder` (required) and `slurp` (region capture).
    pub fn detect() -> Result<Self, CaptureError> {
        Self::probe("wf-recorder").map_err(|_| {
            CaptureError::MissingTool("wf-recorder", "sudo dnf install wf-recorder")
        })?;
        Ok(Self {
            slurp_available: Self::probe("slurp").is_ok(),
        })
    }

    fn probe(binary: &str) -> Result<(), CaptureError> {
        Command::new(binary)
            .arg("--help")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|_| ())
            .map_err(CaptureError::Spawn)
    }

    /// Output names from `niri msg --json outputs`; empty on any failure.
    /// Enabled outputs only — `niri msg` also reports disabled connectors
    /// (`logical: null`), and a disabled output cannot be captured.
    pub fn list_outputs() -> Vec<String> {
        let Ok(out) = Command::new("niri")
            .args(["msg", "--json", "outputs"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            return Vec::new();
        };
        Self::parse_enabled_outputs(&out.stdout)
    }

    /// Pure parser for the `outputs` map so the enabled filter is testable
    /// without a compositor. Keys whose `logical` is null are disabled.
    fn parse_enabled_outputs(json: &[u8]) -> Vec<String> {
        let map: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(json).unwrap_or_default();
        map.iter()
            .filter(|(_, v)| !v.get("logical").is_none_or(serde_json::Value::is_null))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Full argument vector for one capture; pure and unit-testable.
    /// Never includes `-a`/`--audio` — this backend is video-only.
    pub fn argv(req: &CaptureRequest) -> Vec<String> {
        let codec = req
            .encoder
            .clone()
            .unwrap_or_else(|| Self::codec(req.part_path.as_path()).to_string());
        let mut a = vec![
            "wf-recorder".to_string(),
            "-f".to_string(),
            req.part_path.to_string_lossy().into_owned(),
            // `.part` suffix breaks format inference → name the muxer.
            "-m".to_string(),
            Self::muxer(req.part_path.as_path()).to_string(),
            "-c".to_string(),
            codec.clone(),
            "-r".to_string(),
            req.fps.to_string(),
        ];
        // openh264 keeps its verified AVOptions; other codecs get only
        // the generic bitrate option (see `encoders::quality_options`).
        a.extend(encoders::quality_options(&codec, req.quality));
        match &req.target {
            CaptureTarget::Display(name) => a.extend(["-o".into(), name.clone()]),
            CaptureTarget::Region(g) => a.extend(["-g".into(), g.to_wf_arg()]),
        }
        a
    }

    /// Codec matching the output container: mp4 → libopenh264, webm → libvpx-vp9.
    /// `part_path` is `name.<ext>.part`, so the inner extension selects the codec.
    fn codec(part_path: &std::path::Path) -> &'static str {
        match std::path::Path::new(part_path.file_stem().unwrap_or_default())
            .extension()
            .and_then(|e| e.to_str())
        {
            Some("webm") => "libvpx-vp9",
            _ => "libopenh264",
        }
    }

    /// Muxer matching the output container: mp4 → mp4, webm → webm.
    /// Same inner-extension rule as [`Self::codec`].
    fn muxer(part_path: &std::path::Path) -> &'static str {
        match std::path::Path::new(part_path.file_stem().unwrap_or_default())
            .extension()
            .and_then(|e| e.to_str())
        {
            Some("webm") => "webm",
            _ => "mp4",
        }
    }
}

impl CaptureBackend for WfRecorder {
    fn capabilities(&self) -> Result<CaptureCapabilities, CaptureError> {
        // Probe ffmpeg once per process; an unavailable ffmpeg yields an
        // empty list and encoder `auto` still works via the constants.
        // Only H.264 encoders are offered: the app always writes MP4, and
        // VP8/VP9/Theora cannot be trusted in an MP4 muxer.
        let codecs = encoders::available()
            .iter()
            .filter(|name| name.contains("264"))
            .cloned()
            .collect();
        Ok(CaptureCapabilities {
            outputs: Self::list_outputs(),
            region: self.slurp_available,
            codecs,
        })
    }

    fn start(&self, req: CaptureRequest) -> Result<CaptureSession, CaptureError> {
        let argv = Self::argv(&req);
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        let child = spawn(&refs, &[])?;
        // `.part` is `final + ".part"` by naming convention → strip to get final.
        let final_path = req
            .part_path
            .to_str()
            .and_then(|s| s.strip_suffix(".part"))
            .map_or_else(|| req.part_path.clone(), PathBuf::from);
        Ok(CaptureSession {
            child,
            part_path: req.part_path,
            final_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::WfRecorder;

    #[test]
    fn disabled_outputs_are_filtered() {
        // Mirrors real `niri msg --json outputs` on this machine: a
        // disabled connector has `"logical": null` and must not be
        // offered as a capture target.
        let json = br#"{
            "DP-3": {"logical": {"x": 0, "y": 0, "width": 1920, "height": 1080}},
            "eDP-1": {"logical": null}
        }"#;
        assert_eq!(WfRecorder::parse_enabled_outputs(json), vec!["DP-3"]);

        let all_off = br#"{"eDP-1": {"logical": null}}"#;
        assert!(WfRecorder::parse_enabled_outputs(all_off).is_empty());
    }
}
