/// What a capture backend can do on this system right now.
#[derive(Debug, Clone, Default)]
pub struct CaptureCapabilities {
    /// Detectable output names, e.g. `"DP-3"`.
    pub outputs: Vec<String>,
    /// Whether region capture (slurp geometry) is supported.
    pub region: bool,
    /// Supported encoder names understood by the backend.
    pub codecs: Vec<String>,
}
