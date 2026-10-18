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

/// Settings-dialog encoder choices as `(stored value, label)`. The two
/// presets always exist; explicit encoders come from the backend probe
/// (already filtered to H.264/VP9/AV1) and are sorted for stability.
pub fn encoder_choices(codecs: &[String]) -> Vec<(String, String)> {
    let mut choices = vec![
        ("auto".to_string(), "Auto (software)".to_string()),
        ("hardware".to_string(), "Hardware (auto)".to_string()),
    ];
    let mut names = codecs.to_vec();
    names.sort();
    names.dedup();
    for name in names {
        choices.push((name.clone(), name));
    }
    choices
}

/// Row index for a stored encoder value; an unknown value (e.g. an
/// encoder this machine no longer lists) falls back to Auto.
pub fn encoder_index(choices: &[(String, String)], current: &str) -> u32 {
    choices
        .iter()
        .position(|(value, _)| value == current)
        .unwrap_or(0) as u32
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
    fn encoder_choices_and_index() {
        let choices = encoder_choices(&["h264_vaapi".into(), "libopenh264".into()]);
        assert_eq!(choices[0].0, "auto");
        assert_eq!(choices[1].0, "hardware");
        assert_eq!(choices[2].0, "h264_vaapi");
        assert_eq!(choices[3].0, "libopenh264");
        assert_eq!(encoder_index(&choices, "hardware"), 1);
        assert_eq!(encoder_index(&choices, "h264_vaapi"), 2);
        assert_eq!(encoder_index(&choices, "gone"), 0);
        assert_eq!(encoder_choices(&[]).len(), 2);
    }

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
