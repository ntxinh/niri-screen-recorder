//! Client side of the daemon control socket.
//!
//! Every helper returns `None` when no daemon is listening, so the CLI
//! can fall back to its pre-Phase-6 pidfile/SIGINT path. A daemon that
//! answers but speaks garbage is `Some(Err(..))` — callers must report it
//! instead of falling back, otherwise a second recorder could start.

use super::protocol::{Request, Response};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `$XDG_RUNTIME_DIR/niri-recorder/control.sock`.
pub fn socket_path() -> Option<PathBuf> {
    Some(crate::app::pidfile::runtime_dir().join("control.sock"))
}

/// Ask the daemon at `path`. `None` = not running (connect refused);
/// `Some(Err)` = connected but the exchange failed.
pub fn request_at(path: &Path, req: &Request) -> Option<Result<Response, String>> {
    let mut stream = UnixStream::connect(path).ok()?;
    // Stop can block while wf-recorder flushes (≤ ~10 s); starts can
    // block on an interactive slurp for as long as the user takes.
    let _ = stream.set_read_timeout(Some(Duration::from_secs(120)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let mut line = serde_json::to_string(req)
        .map_err(|e| format!("cannot encode request: {e}"))
        .ok()?;
    line.push('\n');
    if let Err(e) = stream.write_all(line.as_bytes()) {
        return Some(Err(format!("cannot write to daemon: {e}")));
    }
    let mut reader = BufReader::new(stream);
    let mut buf = String::new();
    match reader.read_line(&mut buf) {
        Ok(0) => Some(Err("daemon closed the connection without a response".into())),
        Ok(_) => Some(parse_response(&buf).map_err(|e| format!("invalid daemon response: {e}"))),
        Err(e) => Some(Err(format!("cannot read from daemon: {e}"))),
    }
}

fn parse_response(line: &str) -> Result<Response, serde_json::Error> {
    // Also validates that the line is a control response, not an echoed
    // request (both are JSON objects; the fields differ).
    serde_json::from_str(line.trim())
}

/// `Some(response)` when a daemon handled the request.
pub fn request(req: &Request) -> Option<Result<Response, String>> {
    socket_path().and_then(|path| request_at(&path, req))
}

/// Status as seen by a running daemon; `None` when none is running.
pub fn try_status() -> Option<Result<Response, String>> {
    request(&Request::Status)
}

/// Forward a start to the daemon (settings-resolved arguments).
pub fn try_start(
    target: crate::config::settings::Target,
    output: Option<String>,
    fps: u32,
    quality: crate::config::settings::Quality,
) -> Option<Result<Response, String>> {
    request(&Request::Start {
        target: Some(target),
        output,
        fps: Some(fps),
        quality: Some(quality),
    })
}

pub fn try_stop() -> Option<Result<Response, String>> {
    request(&Request::Stop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::{Quality, Target};
    use crate::daemon::protocol::parse_request;
    use std::os::unix::net::UnixListener;

    #[test]
    fn no_daemon_means_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(request_at(&dir.path().join("absent.sock"), &Request::Status).is_none());
    }

    #[test]
    fn start_request_serializes_arguments() {
        let line = serde_json::to_string(&Request::Start {
            target: Some(Target::Display),
            output: Some("DP-3".into()),
            fps: Some(30),
            quality: Some(Quality::High),
        })
        .unwrap();
        assert_eq!(
            parse_request(&line).unwrap(),
            Request::Start {
                target: Some(Target::Display),
                output: Some("DP-3".into()),
                fps: Some(30),
                quality: Some(Quality::High),
            }
        );
    }

    #[test]
    fn round_trips_over_a_unix_socket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            assert_eq!(parse_request(&line).unwrap(), Request::Status);
            let resp = super::super::protocol::encode_response(&Response::idle());
            writeln!(stream, "{resp}").unwrap();
        });

        let got = request_at(&path, &Request::Status).unwrap().unwrap();
        assert_eq!(got, Response::idle());
        server.join().unwrap();
    }

    #[test]
    fn silent_socket_reports_error_not_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            // Drop without answering.
        });
        let got = request_at(&path, &Request::Status);
        assert!(matches!(got, Some(Err(_))), "{got:?}");
        server.join().unwrap();
    }
}
