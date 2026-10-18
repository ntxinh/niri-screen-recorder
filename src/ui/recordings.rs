//! Completion actions + the recent-recordings list.
//!
//! `list_recent`/`is_recording`/`display_time` are pure and headless-tested;
//! [`RecentRecordings`] is the GTK section (`adw::PreferencesGroup` wrapping a
//! `boxed-list` `gtk::ListBox` — one row per file: name, timestamp, Play).

use super::thumbnails::Thumbnailer;
use gtk4::{self as gtk, gio};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

/// Filename stem prefix produced by [`crate::files::naming::plan_output`].
const PREFIX: &str = crate::files::naming::RECORDING_PREFIX;
/// Rows shown in the window's recent list.
const RECENT_LIMIT: usize = 5;

/// Is `p` a finished recording file? In-progress `….mp4.part` siblings are
/// excluded by the extension check (their extension is `part`).
pub fn is_recording(p: &Path) -> bool {
    crate::files::naming::is_recording_path(p)
}

/// Newest `limit` recordings in `dir`, mtime descending. A missing or
/// unreadable dir yields an empty vec — the GUI renders "no recent
/// recordings", not an error.
pub fn list_recent(dir: &Path, limit: usize) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(PathBuf, Option<fs::Metadata>)> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_recording(p))
        .map(|p| {
            let meta = fs::metadata(&p).ok();
            (p, meta)
        })
        .collect();
    files.sort_by(|a, b| {
        b.1.as_ref()
            .and_then(|m| m.modified().ok())
            .cmp(&a.1.as_ref().and_then(|m| m.modified().ok()))
    });
    files.truncate(limit);
    files.into_iter().map(|(p, _)| p).collect()
}

/// Does `s`'s first 19 bytes match `YYYY-MM-DD_HH-MM-SS`? All-ASCII shape
/// check — guarantees the byte slices below sit on char boundaries.
fn is_timestamp(s: &str) -> bool {
    const DIGITS: [usize; 14] = [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18];
    let b = s.as_bytes();
    if b.len() < 19 {
        return false;
    }
    DIGITS.iter().all(|&i| b[i].is_ascii_digit())
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'_'
        && b[13] == b'-'
        && b[16] == b'-'
}

/// Row timestamp: parse `screen-recording-YYYY-MM-DD_HH-MM-SS[…]` back into
/// `YYYY-MM-DD HH:MM:SS`; renamed files fall back to mtime, unreadable
/// metadata to `None`.
fn display_time(p: &Path, meta: Option<&fs::Metadata>) -> Option<String> {
    if let Some(rest) = p.file_name()?.to_str()?.strip_prefix(PREFIX) {
        if is_timestamp(rest) {
            return Some(format!(
                "{} {}:{}:{}",
                &rest[..10],
                &rest[11..13],
                &rest[14..16],
                &rest[17..19]
            ));
        }
    }
    let fd = time::format_description::parse_borrowed::<2>(
        "[year]-[month]-[day] [hour]:[minute]:[second]",
    )
    .ok()?;
    let local = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    meta?.modified().ok().and_then(|t| {
        time::OffsetDateTime::from(t)
            .to_offset(local)
            .format(&fd)
            .ok()
    })
}

/// Open `path`'s parent folder in the file manager. `FileLauncher` (GTK
/// 4.10) activates the dir's default handler with a window parent.
pub(super) fn open_folder(path: &Path, btn: &gtk::Button) {
    let Some(dir) = path.parent().map(gio::File::for_path) else {
        return;
    };
    let window = btn.root().and_downcast::<gtk::Window>();
    gtk::FileLauncher::new(Some(&dir)).launch(window.as_ref(), gio::Cancellable::NONE, |res| {
        if let Err(e) = res {
            eprintln!("niri-recorder: cannot open folder: {e}");
        }
    });
}

/// Open `path` in the default video player. `File::uri` handles escaping
/// (incl. non-UTF8 names); a missing handler is reported on stderr only.
pub(super) fn play_file(path: &Path) {
    let uri = gio::File::for_path(path).uri();
    if let Err(e) = gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE) {
        eprintln!("niri-recorder: cannot play {}: {e}", path.display());
    }
}

/// One row in the recent list: thumbnail + filename (ellipsized) +
/// timestamp + Play. The thumbnail is generated off-thread; until it
/// arrives the picture is simply empty.
fn row(path: &Path, thumbs: &Thumbnailer) -> gtk::Box {
    let meta = fs::metadata(path).ok();
    let preview = gtk::Picture::builder()
        .width_request(96)
        .height_request(54)
        .content_fit(gtk::ContentFit::Cover)
        .css_classes(["thumbnail"])
        .build();
    thumbs.request(path, &preview);
    let name = gtk::Label::builder()
        .label(&*path.file_name().unwrap_or_default().to_string_lossy())
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();
    let when = gtk::Label::builder()
        .label(display_time(path, meta.as_ref()).unwrap_or_default())
        .css_classes(["dim-label", "caption"])
        .valign(gtk::Align::Center)
        .build();
    let play = gtk::Button::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text("Play")
        .valign(gtk::Align::Center)
        .build();
    let owned = path.to_path_buf();
    play.connect_clicked(move |_| play_file(&owned));
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(10)
        .margin_end(10)
        .build();
    row.append(&preview);
    row.append(&name);
    row.append(&when);
    row.append(&play);
    row
}

/// Recent-recordings section; `widget` is appended to the window's content
/// column. Hidden while the dir has no finished recordings.
pub(super) struct RecentRecordings {
    pub widget: adw::PreferencesGroup,
    list: gtk::ListBox,
    dir: PathBuf,
    thumbs: Thumbnailer,
}

impl RecentRecordings {
    /// Builds the section and fills it once — the initial scan is a single
    /// `read_dir` + `stat` per file, cheap enough for the main thread.
    pub(super) fn new(dir: PathBuf) -> Self {
        let list = gtk::ListBox::builder()
            .css_classes(["boxed-list"])
            .selection_mode(gtk::SelectionMode::None)
            .build();
        let widget = adw::PreferencesGroup::builder()
            .title("Recent recordings")
            .build();
        widget.add(&list);
        let r = Self {
            widget,
            list,
            dir,
            thumbs: Thumbnailer::new(),
        };
        r.refresh();
        r
    }

    /// Rebuild the rows (newest [`RECENT_LIMIT`]) — called on `Completed`
    /// so the just-finished file appears immediately.
    pub(super) fn refresh(&self) {
        while let Some(row) = self.list.first_child() {
            self.list.remove(&row);
        }
        self.thumbs.reset();
        let paths = list_recent(&self.dir, RECENT_LIMIT);
        for p in &paths {
            self.list.append(&row(p, &self.thumbs));
        }
        self.widget.set_visible(!paths.is_empty());
    }

    /// Point the list at a new output directory (settings dialog changed
    /// it) and rescan immediately. No-op when the dir didn't change —
    /// apply_settings calls this on every persisted settings write.
    pub(super) fn set_dir(&mut self, dir: PathBuf) {
        if self.dir == dir {
            return;
        }
        self.dir = dir;
        self.refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn touch(dir: &Path, name: &str, age_secs: u64) -> PathBuf {
        let p = dir.join(name);
        let f = fs::File::create(&p).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(age_secs))
            .unwrap();
        p
    }

    #[test]
    fn filters_to_final_recording_names() {
        let dir = tempfile::tempdir().unwrap();
        let keep_mp4 = touch(dir.path(), "screen-recording-2026-10-15_21-34-24.mp4", 10);
        let keep_webm = touch(dir.path(), "screen-recording-2026-10-15_21-35-17.webm", 20);
        touch(
            dir.path(),
            "screen-recording-2026-10-15_21-36-00.mp4.part",
            5,
        );
        touch(dir.path(), "notes.txt", 5);
        touch(dir.path(), "other-2026-10-15.mp4", 5);
        // A directory with a matching name must not be listed.
        fs::create_dir(dir.path().join("screen-recording-2026-10-15_21-37-00.mp4")).unwrap();

        let got = list_recent(dir.path(), 10);
        assert_eq!(got, vec![keep_mp4, keep_webm]);
    }

    #[test]
    fn newest_first_and_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let old = touch(dir.path(), "screen-recording-2026-10-14_10-00-00.mp4", 300);
        let mid = touch(dir.path(), "screen-recording-2026-10-15_10-00-00.mp4", 200);
        let new = touch(dir.path(), "screen-recording-2026-10-16_10-00-00.mp4", 100);

        assert_eq!(
            list_recent(dir.path(), 3),
            vec![new.clone(), mid.clone(), old]
        );
        assert_eq!(list_recent(dir.path(), 2), vec![new, mid]);
        assert_eq!(list_recent(dir.path(), 0), Vec::<PathBuf>::new());
    }

    #[test]
    fn missing_or_empty_dir_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_recent(dir.path(), 5).is_empty());
        assert!(list_recent(&dir.path().join("nope"), 5).is_empty());
    }

    #[test]
    fn timestamp_parsed_from_name() {
        let dir = tempfile::tempdir().unwrap();
        let p = touch(dir.path(), "screen-recording-2026-10-15_21-34-24.mp4", 10);
        let meta = fs::metadata(&p).unwrap();
        assert_eq!(
            display_time(&p, Some(&meta)).as_deref(),
            Some("2026-10-15 21:34:24")
        );
        // Suffix collisions still parse (`-1` after the timestamp).
        let p = touch(
            dir.path(),
            "screen-recording-2026-10-15_21-34-24-1.webm",
            10,
        );
        assert_eq!(
            display_time(&p, None).as_deref(),
            Some("2026-10-15 21:34:24")
        );
    }

    #[test]
    fn timestamp_falls_back_to_mtime() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "screen-recording-renamed.mp4",
            // Timestamp-shaped but garbage — must not render as text.
            "screen-recording-abcdefghij_kl-mn-op.mp4",
        ] {
            let p = touch(dir.path(), name, 10);
            let meta = fs::metadata(&p).unwrap();
            assert!(
                display_time(&p, Some(&meta)).is_some(),
                "{name} should fall back to mtime"
            );
            assert_eq!(display_time(&p, None), None);
        }
    }

    #[test]
    fn non_ascii_stem_falls_back_without_panic() {
        let dir = tempfile::tempdir().unwrap();
        // Multi-byte char inside the stem: byte-slicing `[11..13]` used to
        // panic off char boundaries.
        let p = touch(
            dir.path(),
            "screen-recording-2026-10-15_2\u{20ac}34567.mp4",
            10,
        );
        let meta = fs::metadata(&p).unwrap();
        assert!(display_time(&p, Some(&meta)).is_some());
    }
}
