//! `wf-recorder` backend: captures a niri output or a slurp-picked region.

use crate::capture::backend::{CaptureBackend, CaptureError, CaptureSession};
use crate::capture::capabilities::CaptureCapabilities;
use crate::capture::process::spawn;
use crate::capture::request::{CaptureRequest, CaptureTarget};
use crate::config::settings::Quality;
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
    pub fn list_outputs() -> Vec<String> {
        let Ok(out) = Command::new("niri")
            .args(["msg", "--json", "outputs"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            return Vec::new();
        };
        serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&out.stdout)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Full argument vector for one capture; pure and unit-testable.
    /// Never includes `-a`/`--audio` — this backend is video-only.
    pub fn argv(req: &CaptureRequest) -> Vec<String> {
        let mut a = vec![
            "wf-recorder".to_string(),
            "-f".to_string(),
            req.part_path.to_string_lossy().into_owned(),
            // `.part` suffix breaks format inference → name the muxer.
            "-m".to_string(),
            Self::muxer(req.part_path.as_path()).to_string(),
            "-c".to_string(),
            Self::codec(req.part_path.as_path()).to_string(),
            "-r".to_string(),
            req.fps.to_string(),
        ];
        if let Quality::High = req.quality {
            // openh264 has no bitrate priv-opt; `b` is the AVCodecContext
            // bitrate AVOption (verified: encoder logs 10000 kb/s target).
            a.extend([
                "-p".into(),
                "b=10000000".into(),
                "-p".into(),
                "rc_mode=bitrate".into(),
                "-p".into(),
                "allow_skip_frames=true".into(),
            ]);
        }
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
        Ok(CaptureCapabilities {
            outputs: Self::list_outputs(),
            region: self.slurp_available,
            codecs: vec!["libopenh264".into(), "libvpx-vp9".into()],
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
