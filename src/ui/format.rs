//! Pure display helpers for the control surface. Kept widget-free so the
//! unit tests run headless.

use crate::app::state::AppState;
use std::time::Duration;

/// `mm:ss` under an hour, `h:mm:ss` above — matches FR-10's elapsed timer.
pub fn format_elapsed(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

/// One label per `AppState` variant — states shown in the window must come
/// from the state machine, not scattered booleans.
pub fn state_label(state: &AppState) -> &'static str {
    match state {
        AppState::Idle => "Idle",
        AppState::SelectingRegion => "Selecting region…",
        AppState::Starting => "Starting…",
        AppState::Recording { .. } => "Recording",
        AppState::Stopping { .. } => "Stopping…",
        AppState::Completed(_) => "Completed",
        AppState::Failed(_) => "Failed",
    }
}

/// Sorted, deduplicated output names for the display dropdown —
/// `niri msg --json outputs` yields map keys in arbitrary order.
pub fn outputs_model(outputs: &[String]) -> Vec<String> {
    let mut v = outputs.to_vec();
    v.sort();
    v.dedup();
    v
}

/// FR-09: index of `last_output` in the (sorted, deduped) model, else 0.
/// A stale last_output (display gone) falls back to the first entry.
pub fn select_index(outputs: &[String], last_output: Option<&str>) -> u32 {
    last_output
        .and_then(|name| outputs.iter().position(|o| o == name))
        .unwrap_or(0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn elapsed_formats() {
        assert_eq!(format_elapsed(Duration::from_secs(0)), "00:00");
        assert_eq!(format_elapsed(Duration::from_secs(59)), "00:59");
        assert_eq!(format_elapsed(Duration::from_secs(61)), "01:01");
        assert_eq!(format_elapsed(Duration::from_secs(3599)), "59:59");
        assert_eq!(format_elapsed(Duration::from_secs(3600)), "1:00:00");
        assert_eq!(format_elapsed(Duration::from_secs(3723)), "1:02:03");
    }

    #[test]
    fn every_state_has_a_label() {
        let states = [
            AppState::Idle,
            AppState::SelectingRegion,
            AppState::Starting,
            AppState::Recording { pid: 1 },
            AppState::Stopping { pid: 1 },
            AppState::Completed(PathBuf::from("/tmp/x.mp4")),
            AppState::Failed("boom".into()),
        ];
        for s in &states {
            assert!(!state_label(s).is_empty());
        }
        assert_eq!(state_label(&AppState::Idle), "Idle");
    }

    #[test]
    fn outputs_sorted_and_deduped() {
        let input = vec!["DP-3".to_string(), "eDP-1".to_string(), "DP-3".to_string()];
        assert_eq!(outputs_model(&input), vec!["DP-3", "eDP-1"]);
        assert!(outputs_model(&[]).is_empty());
    }

    #[test]
    fn select_index_picks_last_output_or_zero() {
        let outputs = vec!["DP-3".to_string(), "eDP-1".to_string()];
        assert_eq!(select_index(&outputs, Some("eDP-1")), 1);
        assert_eq!(select_index(&outputs, Some("gone")), 0); // stale
        assert_eq!(select_index(&outputs, None), 0);
        assert_eq!(select_index(&[], Some("eDP-1")), 0);
    }
}
