//! Daemon control protocol: one JSON request per line, one JSON response
//! per line, over a Unix stream socket. No shell, no network.

use crate::config::settings::{Quality, Target};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum Request {
    /// Begin a capture; omitted fields fall back to config.toml.
    Start {
        #[serde(default)]
        target: Option<Target>,
        #[serde(default)]
        output: Option<String>,
        #[serde(default)]
        fps: Option<u32>,
        #[serde(default)]
        quality: Option<Quality>,
    },
    Stop,
    Status,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Response {
    pub ok: bool,
    /// `idle` | `starting` | `recording` | `stopping` | `completed` | `failed`
    pub state: String,
    /// Process exit code the CLI should use (0 ok, 1 state conflict,
    /// 2 backend, 3 capture failure).
    pub code: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn idle() -> Self {
        Self {
            ok: true,
            state: "idle".into(),
            code: 0,
            pid: None,
            part: None,
            saved: None,
            error: None,
        }
    }

    pub fn recording(pid: u32, part: Option<String>) -> Self {
        Self {
            ok: true,
            state: "recording".into(),
            code: 0,
            pid: Some(pid),
            part,
            saved: None,
            error: None,
        }
    }

    pub fn starting() -> Self {
        Self {
            ok: true,
            state: "starting".into(),
            code: 0,
            ..Self::idle()
        }
    }

    pub fn saved(path: &std::path::Path) -> Self {
        Self {
            ok: true,
            state: "idle".into(),
            code: 0,
            saved: Some(path.display().to_string()),
            ..Self::idle()
        }
    }

    pub fn error(code: u8, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            state: "failed".into(),
            code,
            error: Some(message.into()),
            ..Self::idle()
        }
    }
}

/// Encode one response as a single line (newline appended by caller).
pub fn encode_response(resp: &Response) -> String {
    serde_json::to_string(resp).unwrap_or_else(|e| {
        format!(
            "{{\"ok\":false,\"state\":\"failed\",\"code\":3,\"error\":\"encode: {}\"}}",
            e.to_string().replace('"', "'")
        )
    })
}

/// Parse one request line.
pub fn parse_request(line: &str) -> Result<Request, String> {
    serde_json::from_str(line.trim()).map_err(|e| format!("invalid request: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip() {
        let start = Request::Start {
            target: Some(Target::Region),
            output: None,
            fps: Some(60),
            quality: Some(Quality::High),
        };
        let line = serde_json::to_string(&start).unwrap();
        assert_eq!(parse_request(&line).unwrap(), start);

        assert_eq!(parse_request("{\"cmd\":\"stop\"}").unwrap(), Request::Stop);
        assert_eq!(
            parse_request("{\"cmd\":\"status\"}").unwrap(),
            Request::Status
        );
        // Omitted fields are defaults (settings apply them).
        assert_eq!(
            parse_request("{\"cmd\":\"start\"}").unwrap(),
            Request::Start {
                target: None,
                output: None,
                fps: None,
                quality: None
            }
        );
    }

    #[test]
    fn malformed_requests_are_rejected() {
        assert!(parse_request("not json").is_err());
        assert!(parse_request("{\"cmd\":\"launch\"}").is_err());
        assert!(parse_request("").is_err());
    }

    #[test]
    fn responses_round_trip_and_omit_empty_fields() {
        let r = Response::recording(42, Some("/tmp/a.mp4.part".into()));
        let line = encode_response(&r);
        assert!(!line.contains("saved"));
        assert!(!line.contains('\n'));
        let back: Response = serde_json::from_str(&line).unwrap();
        assert_eq!(back, r);

        let err = Response::error(3, "boom");
        assert_eq!(err.code, 3);
        assert!(!err.ok);
    }
}
