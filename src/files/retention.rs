//! Opt-in retention for finished recordings.
//!
//! Cleanup is deliberately conservative: it only ever considers files
//! that [`crate::files::naming::is_recording_path`] accepts — our own
//! `screen-recording-<timestamp>.(mp4|webm)` outputs. In-progress
//! `.part` files, renamed files, and anything else in the directory are
//! never deleted. Both rules are optional and apply to the union:
//! `max_files` keeps the newest N, `max_age_days` drops old entries.

use crate::files::naming::is_recording_path;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Resolved cleanup settings. `enabled == false` makes [`apply`] a no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CleanupRules {
    pub enabled: bool,
    /// Keep at most this many recordings (newest first). `None` = no limit.
    pub max_files: Option<usize>,
    /// Drop recordings older than this. `None` = no age limit.
    pub max_age_days: Option<u32>,
}

impl CleanupRules {
    fn is_noop(&self) -> bool {
        !self.enabled || (self.max_files.is_none() && self.max_age_days.is_none())
    }
}

/// Paths [`apply`] would remove, computed without touching the disk.
/// Sorted newest-first so callers can report in a stable order.
pub fn victims(dir: &Path, rules: &CleanupRules, now: SystemTime) -> Vec<PathBuf> {
    if rules.is_noop() {
        return Vec::new();
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    // (path, mtime); files without usable metadata are left alone.
    let mut files: Vec<(PathBuf, SystemTime)> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| is_recording_path(p))
        .filter_map(|p| {
            let modified = fs::metadata(&p).ok()?.modified().ok()?;
            Some((p, modified))
        })
        .collect();
    if files.is_empty() {
        return Vec::new();
    }
    files.sort_by_key(|(_, mtime)| std::cmp::Reverse(*mtime));

    let too_old = |mtime: SystemTime| {
        rules.max_age_days.is_some_and(|days| {
            let age = now.duration_since(mtime).unwrap_or(Duration::ZERO);
            age > Duration::from_secs(u64::from(days) * 86_400)
        })
    };
    files
        .into_iter()
        .enumerate()
        .filter(|(i, (_, mtime))| rules.max_files.is_some_and(|keep| *i >= keep) || too_old(*mtime))
        .map(|(_, (p, _))| p)
        .collect()
}

/// Apply `settings`' resolved rules to its output directory. Shared by
/// the CLI, the GUI worker and the daemon so retention behaves the same
/// on every path.
pub fn apply_for_settings(settings: &crate::config::settings::Settings) -> Vec<PathBuf> {
    let dir = crate::config::settings::expand_tilde(&settings.output_directory);
    let removed = apply(&dir, &settings.cleanup_rules());
    if !removed.is_empty() {
        tracing::info!(count = removed.len(), dir = %dir.display(), "cleanup removed old recordings");
    }
    removed
}

/// Remove [`victims`]; returns the paths that were actually deleted.
/// Individual delete failures are ignored (a file may vanish between
/// the scan and the unlink) — retention is best-effort, never fatal.
pub fn apply(dir: &Path, rules: &CleanupRules) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    for path in victims(dir, rules, SystemTime::now()) {
        if fs::remove_file(&path).is_ok() {
            removed.push(path);
        }
    }
    removed
}
