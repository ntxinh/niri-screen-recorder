//! Encoder probing and preference resolution.
//!
//! `wf-recorder -c <name>` accepts any ffmpeg encoder. We probe
//! `ffmpeg -encoders` once per process and resolve the user's
//! `encoder` setting against it:
//!
//! - `auto` / `software` → the container's software codec (unchanged
//!   pre-Phase-6 behavior),
//! - `hardware` → the first probed hardware H.264/VP9 encoder, falling
//!   back to software when none is listed,
//! - any other name → that encoder when available, else software.
//!
//! Availability is *listed support only*: appearing in `ffmpeg
//! -encoders` does not prove this GPU/driver can run the encoder. A
//! failed start surfaces wf-recorder's stderr and the user can pin
//! `encoder = "software"`.

use crate::config::settings::Quality;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// Software codec for the MP4 container.
pub const SOFTWARE_MP4: &str = "libopenh264";
/// Software codec for the WebM container.
pub const SOFTWARE_WEBM: &str = "libvpx-vp9";

/// Hardware H.264 encoders in preference order (VAAPI first: it is the
/// one Fedora's ffmpeg ships enabled for Intel/AMD GPUs).
const HW_H264: [&str; 4] = ["h264_vaapi", "h264_nvenc", "h264_qsv", "h264_v4l2m2m"];
/// Hardware VP9 encoders in preference order.
const HW_VP9: [&str; 2] = ["vp9_vaapi", "vp9_qsv"];

/// Resolved codec for one capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncoderChoice {
    /// ffmpeg encoder name passed to `-c`.
    pub codec: String,
    /// Whether the chosen codec is a hardware encoder.
    pub hardware: bool,
    /// Set when the preference could not be honored (explained to the
    /// user via tracing and the diagnostics command).
    pub warning: Option<String>,
}

/// Parse `ffmpeg -hide_banner -encoders` output into encoder names.
/// Only video encoders (flag column starts with `V`) are returned.
pub fn parse_encoders(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let flags = fields.next()?;
            let name = fields.next()?;
            // Flag column is 6 chars: `V.....`; text lines like
            // "Encoders:", the legend (`V..... = Video`), and the dashed
            // rule are skipped.
            (flags.len() == 6 && flags.starts_with('V') && name != "=").then(|| name.to_string())
        })
        .collect()
}

/// Video encoders this system's ffmpeg lists. Probed once per process;
/// an unavailable ffmpeg yields an empty list (software still works).
pub fn available() -> &'static [String] {
    static AVAILABLE: OnceLock<Vec<String>> = OnceLock::new();
    AVAILABLE.get_or_init(|| {
        Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .map(|out| parse_encoders(&String::from_utf8_lossy(&out.stdout)))
            .unwrap_or_default()
    })
}

/// Is `name` an encoder this ffmpeg lists?
pub fn is_available(name: &str, available: &[String]) -> bool {
    available.iter().any(|e| e == name)
}

/// Does resolving `preference` require probing ffmpeg?
pub fn needs_probe(preference: &str) -> bool {
    !matches!(preference, "" | "auto" | "software")
}

/// Software codec for a file extension (`mp4`/anything → H.264, `webm`
/// → VP9). Matches the pre-Phase-6 container rule.
pub fn software_codec(container: &str) -> &'static str {
    if container == "webm" {
        SOFTWARE_WEBM
    } else {
        SOFTWARE_MP4
    }
}

/// Resolve `preference` against `available` for `container`.
pub fn resolve(preference: &str, available: &[String], container: &str) -> EncoderChoice {
    let software = EncoderChoice {
        codec: software_codec(container).to_string(),
        hardware: false,
        warning: None,
    };
    match preference.trim() {
        "" | "auto" | "software" => software,
        "hardware" => {
            let candidates: &[&str] = if container == "webm" {
                &HW_VP9
            } else {
                &HW_H264
            };
            match candidates.iter().find(|name| is_available(name, available)) {
                Some(name) => EncoderChoice {
                    codec: (*name).to_string(),
                    hardware: true,
                    warning: None,
                },
                None => EncoderChoice {
                    warning: Some("no hardware encoder listed by ffmpeg; using software".into()),
                    ..software
                },
            }
        }
        explicit if is_available(explicit, available) => EncoderChoice {
            codec: explicit.to_string(),
            hardware: !matches!(
                explicit,
                SOFTWARE_MP4 | SOFTWARE_WEBM | "libx264" | "libvpx" | "libaom-av1"
            ),
            warning: None,
        },
        explicit => EncoderChoice {
            warning: Some(format!(
                "encoder {explicit:?} is not listed by ffmpeg; using {}",
                software.codec
            )),
            ..software
        },
    }
}

/// Extra `-p` AVOption pairs for the selected codec/quality.
///
/// Standard passes nothing. `libopenh264` keeps the verified pre-Phase-6
/// options; other codecs get only the generic `b` (AVCodecContext
/// bitrate) option, which openh264 does not need but every ffmpeg
/// encoder context accepts.
pub fn quality_options(codec: &str, quality: Quality) -> Vec<String> {
    if quality != Quality::High {
        return Vec::new();
    }
    let mut opts = vec!["-p".to_string(), "b=10000000".to_string()];
    if codec == SOFTWARE_MP4 {
        opts.extend([
            "-p".to_string(),
            "rc_mode=bitrate".to_string(),
            "-p".to_string(),
            "allow_skip_frames=true".to_string(),
        ]);
    }
    opts
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Encoders:
 V..... = Video
 A..... = Audio
 ------
 V....D libopenh264          OpenH264 H.264 / AVC
 V..... h264_vaapi           H.264/AVC (VAAPI)
 V..... libvpx-vp9           libvpx VP9
 A....D aac                  AAC
 S..... srt                  SubRip subtitle
";

    #[test]
    fn parses_only_video_encoders() {
        let names = parse_encoders(SAMPLE);
        assert_eq!(names, vec!["libopenh264", "h264_vaapi", "libvpx-vp9"]);
        assert!(!names.contains(&"aac".to_string()));
        assert!(!names.contains(&"srt".to_string()));
    }

    #[test]
    fn auto_and_software_stay_on_container_default() {
        let avail = vec!["h264_vaapi".to_string()];
        assert_eq!(resolve("auto", &avail, "mp4").codec, SOFTWARE_MP4);
        assert_eq!(resolve("software", &avail, "webm").codec, SOFTWARE_WEBM);
        assert_eq!(resolve("", &avail, "mp4").codec, SOFTWARE_MP4);
    }

    #[test]
    fn hardware_picks_first_listed_candidate() {
        let avail = vec!["h264_nvenc".into(), "h264_vaapi".into()];
        let c = resolve("hardware", &avail, "mp4");
        assert_eq!(c.codec, "h264_vaapi"); // preference order, not list order
        assert!(c.hardware);
        assert!(c.warning.is_none());

        let c = resolve("hardware", &avail, "webm");
        assert_eq!(c.codec, SOFTWARE_WEBM);
        assert!(!c.hardware);
        assert!(c.warning.unwrap().contains("software"));
    }

    #[test]
    fn explicit_encoder_is_used_when_listed() {
        let avail = vec!["libopenh264".into(), "h264_qsv".into()];
        let c = resolve("h264_qsv", &avail, "mp4");
        assert_eq!(c.codec, "h264_qsv");
        assert!(c.hardware);
        assert!(c.warning.is_none());

        let c = resolve("h264_vaapi", &avail, "mp4");
        assert_eq!(c.codec, SOFTWARE_MP4, "unlisted → software fallback");
        assert!(!c.hardware);
        assert!(c.warning.unwrap().contains("h264_vaapi"));
    }

    #[test]
    fn quality_options_keep_verified_openh264_set() {
        let o = quality_options(SOFTWARE_MP4, Quality::High);
        assert!(o.iter().any(|s| s == "rc_mode=bitrate"));
        assert!(quality_options(SOFTWARE_MP4, Quality::Standard).is_empty());
        let hw = quality_options("h264_vaapi", Quality::High);
        assert!(hw.contains(&"b=10000000".to_string()));
        assert!(!hw.iter().any(|s| s == "rc_mode=bitrate"));
    }

    #[test]
    fn probe_only_needed_for_non_default_preferences() {
        assert!(!needs_probe("auto"));
        assert!(!needs_probe("software"));
        assert!(!needs_probe(""));
        assert!(needs_probe("hardware"));
        assert!(needs_probe("h264_vaapi"));
    }
}
