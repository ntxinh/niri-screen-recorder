//! Desktop notifications via `notify-send`.

use std::process::Command;

/// Send a desktop notification if `enabled` is true.
///
/// Spawns `notify-send` in the background (non-blocking). Failures
/// (such as missing `notify-send`) are silently ignored so recording
/// is never disrupted.
pub fn notify(enabled: bool, summary: &str, body: &str) {
    if !enabled {
        return;
    }
    let mut cmd = Command::new("notify-send");
    cmd.args([
        "--app-name",
        "niri-recorder",
        "--icon",
        "media-record",
        "--hint",
        "string:desktop-entry:io.github.niri_recorder",
        summary,
    ]);
    if !body.is_empty() {
        cmd.arg(body);
    }
    let _ = cmd.spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_disabled_is_noop() {
        // Disabled notifications do not spawn anything.
        notify(false, "Test", "Disabled");
    }
}
