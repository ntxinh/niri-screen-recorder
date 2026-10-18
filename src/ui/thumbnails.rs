//! Thumbnails for the recent-recordings list.
//!
//! `ffmpeg` extracts one frame into the XDG cache directory; the PNG is
//! keyed by the recording's path + mtime + size, so a cached thumbnail is
//! reused until the file changes. Generation happens on a worker thread
//! (subprocess + decode) and the finished path is delivered back to the
//! GTK main context over an `async_channel`; the main thread only loads
//! the texture. A missing ffmpeg simply means no thumbnails.

use gtk4 as gtk;
use gtk4::{gdk, glib};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Rendered thumbnail width; height follows the source aspect ratio.
const THUMB_WIDTH: u32 = 160;
/// Seconds into the recording to sample — avoids black first frames.
const SEEK_SECS: &str = "1";

/// `$XDG_CACHE_HOME/niri-recorder/thumbnails` (created on demand).
pub fn cache_dir() -> Option<PathBuf> {
    let dir = directories::ProjectDirs::from("", "", "niri-recorder")?
        .cache_dir()
        .join("thumbnails");
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Cache path for `video`: a hash of canonical path, mtime and size.
/// Returns `None` when the file can't be stat'd.
pub fn cache_path(video: &Path, cache: &Path) -> Option<PathBuf> {
    let meta = fs::metadata(video).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    fs::canonicalize(video)
        .unwrap_or_else(|_| video.to_path_buf())
        .hash(&mut hasher);
    modified.hash(&mut hasher);
    meta.len().hash(&mut hasher);
    Some(cache.join(format!("{:016x}.png", hasher.finish())))
}

/// ffmpeg arguments for one extraction. `-ss 0` is the fallback used when
/// seeking past the end of a very short recording produces no frame.
pub fn argv(video: &Path, out: &Path, seek: &str) -> Vec<String> {
    vec![
        "ffmpeg".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-ss".into(),
        seek.into(),
        "-i".into(),
        video.to_string_lossy().into_owned(),
        "-frames:v".into(),
        "1".into(),
        "-vf".into(),
        format!("scale={THUMB_WIDTH}:-1"),
        out.to_string_lossy().into_owned(),
    ]
}

/// Generate `out` from `video`, trying `-ss 1` then `-ss 0`. Returns
/// whether a non-empty PNG exists afterwards. Never shells out: argv only.
pub fn generate(video: &Path, out: &Path) -> bool {
    // A zero-byte PNG is a leftover from an interrupted run, not a hit.
    if out.is_file() && fs::metadata(out).is_ok_and(|m| m.len() > 0) {
        return true;
    }
    for seek in [SEEK_SECS, "0"] {
        let args = argv(video, out, seek);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let _ = std::process::Command::new("ffmpeg")
            .args(&refs[1..])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if out.is_file() && fs::metadata(out).is_ok_and(|m| m.len() > 0) {
            return true;
        }
    }
    false
}

/// Main-thread registry: video path → the `Picture` widget currently
/// showing it, plus the channel workers report back on.
pub(super) struct Thumbnailer {
    pictures: Rc<RefCell<HashMap<PathBuf, gtk::Picture>>>,
    tx: async_channel::Sender<(PathBuf, PathBuf)>,
}

impl Thumbnailer {
    pub(super) fn new() -> Self {
        let (tx, rx) = async_channel::unbounded::<(PathBuf, PathBuf)>();
        let pictures = Rc::new(RefCell::new(HashMap::<PathBuf, gtk::Picture>::new()));
        let registry = Rc::clone(&pictures);
        // Drained on the GTK main context; a row refreshed away simply
        // isn't in the map and its late thumbnail is dropped.
        glib::spawn_future_local(async move {
            while let Ok((video, thumb)) = rx.recv().await {
                let Some(picture) = registry.borrow().get(&video).cloned() else {
                    continue;
                };
                let Some(path) = thumb.to_str() else { continue };
                if let Ok(texture) = gdk::Texture::from_filename(path) {
                    picture.set_paintable(Some(&texture));
                }
            }
        });
        Self { pictures, tx }
    }

    /// Start a new row set: drop references to widgets being rebuilt.
    pub(super) fn reset(&self) {
        self.pictures.borrow_mut().clear();
    }

    /// Register a row's picture and kick off thumbnail generation when
    /// the cache is cold. Returns immediately.
    pub(super) fn request(&self, video: &Path, picture: &gtk::Picture) {
        self.pictures
            .borrow_mut()
            .insert(video.to_path_buf(), picture.clone());
        let Some(cache) = cache_dir() else { return };
        let Some(thumb) = cache_path(video, &cache) else {
            return;
        };
        if thumb.is_file() {
            // Already cached: post it through the same channel so the
            // update still happens on the main context.
            let _ = self.tx.send_blocking((video.to_path_buf(), thumb));
            return;
        }
        let tx = self.tx.clone();
        let video = video.to_path_buf();
        std::thread::spawn(move || {
            if generate(&video, &thumb) {
                let _ = tx.send_blocking((video, thumb));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_is_stable_and_changes_with_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("screen-recording-x.mp4");
        fs::write(&video, b"frames").unwrap();
        let cache = dir.path().join("cache");
        fs::create_dir_all(&cache).unwrap();

        let a = cache_path(&video, &cache).unwrap();
        let b = cache_path(&video, &cache).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.parent(), Some(cache.as_path()));

        // Rewriting changes size/mtime → a different key.
        fs::write(&video, b"more frames").unwrap();
        let c = cache_path(&video, &cache).unwrap();
        assert_ne!(a, c);
    }

    #[test]
    fn argv_is_shell_free_and_video_only() {
        let args = argv(Path::new("/tmp/a b.mp4"), Path::new("/tmp/t.png"), "1");
        // Paths with spaces stay one argv element (no shell involved).
        assert!(args.contains(&"/tmp/a b.mp4".to_string()));
        assert!(!args.iter().any(|a| a == "-a" || a.starts_with("--audio")));
        assert!(args.iter().any(|a| a == "-frames:v"));
        assert!(args.iter().any(|a| a == "scale=160:-1"));
    }

    /// Runs only where ffmpeg is installed (the project's capture
    /// dependency); skipped otherwise rather than failing the suite.
    #[test]
    fn generates_a_png_from_a_synthetic_video() {
        if std::process::Command::new("ffmpeg")
            .arg("-version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_err()
        {
            eprintln!("ffmpeg missing — skipping thumbnail generation test");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("clip.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:s=320x180:d=2",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&video)
            .status()
            .unwrap();
        assert!(made.success());

        let thumb = dir.path().join("thumb.png");
        assert!(generate(&video, &thumb));
        assert!(fs::metadata(&thumb).unwrap().len() > 0);
        // Second call is a cache hit.
        assert!(generate(&video, &thumb));
    }
}
