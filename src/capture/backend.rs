use crate::capture::capabilities::CaptureCapabilities;
use crate::capture::process::ManagedChild;
use crate::capture::request::CaptureRequest;
use std::path::PathBuf;

/// A running capture owned by the capture controller.
pub struct CaptureSession {
    pub child: ManagedChild,
    /// `.part` output path in use while recording.
    pub part_path: PathBuf,
    /// Destination path the recording is renamed to on clean stop.
    pub final_path: PathBuf,
}

/// A screen-recorder subprocess wrapper (wf-recorder, gpu-screen-recorder, …).
pub trait CaptureBackend {
    /// Probe tools, outputs, and codec support.
    fn capabilities(&self) -> Result<CaptureCapabilities, CaptureError>;
    /// Spawn the recorder for one capture.
    fn start(&self, req: CaptureRequest) -> Result<CaptureSession, CaptureError>;
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("missing dependency: {0} (install: {1})")]
    MissingTool(&'static str, &'static str),
    #[error("capture failed: {0}")]
    Spawn(std::io::Error),
    #[error("recorder exited unexpectedly: {0}")]
    UnexpectedExit(String),
    #[error("stop timed out; sent SIGKILL")]
    StopTimeout,
    #[error("backend unsupported: {0}")]
    Unsupported(String),
}
