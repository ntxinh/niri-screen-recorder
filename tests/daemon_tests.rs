use niri_recorder::daemon::client;
use niri_recorder::daemon::protocol::{parse_request, Request, Response};
use niri_recorder::daemon::serve_at;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn wait_for_socket(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(Instant::now() < deadline, "daemon never bound {path:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn send_line(path: &std::path::Path, line: &str) -> Response {
    let mut stream = UnixStream::connect(path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    writeln!(stream, "{line}").unwrap();
    let mut buf = String::new();
    BufReader::new(stream).read_line(&mut buf).unwrap();
    serde_json::from_str(&buf).unwrap()
}

#[test]
fn status_answers_over_the_socket_and_shutdown_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("control.sock");
    let shutdown = Arc::new(AtomicBool::new(false));
    let server = std::thread::spawn({
        let socket = socket.clone();
        let shutdown = Arc::clone(&shutdown);
        move || serve_at(&socket, shutdown)
    });
    wait_for_socket(&socket);

    let resp = client::request_at(&socket, &Request::Status)
        .expect("daemon accepts connections")
        .expect("status succeeds");
    assert_eq!(resp, Response::idle());

    // The socket is user-private.
    let mode = std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    // A second daemon must not steal a live socket.
    assert!(serve_at(&socket, Arc::new(AtomicBool::new(false))).is_err());

    shutdown.store(true, Ordering::SeqCst);
    server.join().unwrap().unwrap();
    assert!(!socket.exists(), "socket removed on shutdown");
}

#[test]
fn malformed_and_unknown_requests_get_an_error_response() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("control.sock");
    let shutdown = Arc::new(AtomicBool::new(false));
    let server = std::thread::spawn({
        let socket = socket.clone();
        let shutdown = Arc::clone(&shutdown);
        move || serve_at(&socket, shutdown)
    });
    wait_for_socket(&socket);

    let resp = send_line(&socket, "not json");
    assert!(!resp.ok);
    assert_eq!(resp.code, 2);
    assert!(resp.error.unwrap().contains("invalid request"));

    let resp = send_line(&socket, "{\"cmd\":\"launch\"}");
    assert!(!resp.ok);
    assert_eq!(resp.code, 2);

    // Stop with no capture is a state conflict (CLI falls back to the
    // pidfile for a GUI/foreground recorder).
    let resp = send_line(&socket, "{\"cmd\":\"stop\"}");
    assert!(!resp.ok);
    assert_eq!(resp.code, 1);

    shutdown.store(true, Ordering::SeqCst);
    server.join().unwrap().unwrap();
}

#[test]
fn no_daemon_returns_none_so_cli_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    assert!(client::request_at(&dir.path().join("absent.sock"), &Request::Status).is_none());
}

#[test]
fn protocol_start_parses_defaults() {
    assert_eq!(
        parse_request("{\"cmd\":\"start\"}").unwrap(),
        Request::Start {
            target: None,
            output: None,
            fps: None,
            quality: None,
            notify: None,
        }
    );
}
