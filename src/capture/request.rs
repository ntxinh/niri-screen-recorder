use crate::config::settings::Quality;
use std::path::PathBuf;

/// A capture region in output-layout coordinates. `x`/`y` may be negative
/// for monitors left of / above the primary output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Geometry {
    /// Parse `slurp`-style geometry: `x,y WxH` (trailing whitespace tolerated).
    /// `w` and `h` must be non-zero.
    pub fn parse_slurp(s: &str) -> Result<Geometry, GeometryError> {
        let err = || GeometryError::Malformed(s.trim().to_owned());
        let (origin, size) = s.trim().split_once(' ').ok_or_else(err)?;
        let (x, y) = origin.split_once(',').ok_or_else(err)?;
        let (w, h) = size.split_once('x').ok_or_else(err)?;
        let g = Geometry {
            x: x.parse().map_err(|_| err())?,
            y: y.parse().map_err(|_| err())?,
            w: w.parse().map_err(|_| err())?,
            h: h.parse().map_err(|_| err())?,
        };
        if g.w == 0 || g.h == 0 {
            return Err(GeometryError::ZeroSize);
        }
        Ok(g)
    }

    /// Geometry argument for `wf-recorder -g`, in `x,y WxH` form.
    pub fn to_wf_arg(&self) -> String {
        format!("{},{} {}x{}", self.x, self.y, self.w, self.h)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GeometryError {
    #[error("malformed geometry: {0:?} (expected `x,y WxH`)")]
    Malformed(String),
    #[error("geometry has zero width or height")]
    ZeroSize,
}

/// What to capture: a whole output by name, or a screen region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureTarget {
    /// Output name as reported by niri/wlr-randr, e.g. `"DP-3"`.
    Display(String),
    Region(Geometry),
}

/// Parameters for starting one capture.
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    pub target: CaptureTarget,
    pub fps: u32,
    pub quality: Quality,
    pub part_path: PathBuf,
    /// Resolved ffmpeg encoder name (see `capture::encoders`). `None`
    /// keeps the container's software default.
    pub encoder: Option<String>,
}
