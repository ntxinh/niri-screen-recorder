//! Collision-safe output file naming with `.part` staging.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use time::OffsetDateTime;

/// Maximum collision suffix (`-1` … `-99`) tried after the base name.
const MAX_SUFFIX: u32 = 99;

/// Recording output plan: write to `part_path`, then [`finalize`].
pub struct OutputPlan {
    pub part_path: PathBuf,
    pub final_path: PathBuf,
}

fn timestamp() -> String {
    let t = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

/// Pick a non-existing `screen-recording-<local ts>.<ext>` under `dir`
/// (created if missing), appending `-1` … `-99` on collision.
/// `part_path` = `final_path` with `.part` appended (`….mp4.part`).
pub fn plan_output(dir: &Path, ext: &str) -> io::Result<OutputPlan> {
    fs::create_dir_all(dir)?;
    let ts = timestamp();
    for n in 0..=MAX_SUFFIX {
        let stem = if n == 0 {
            format!("screen-recording-{ts}")
        } else {
            format!("screen-recording-{ts}-{n}")
        };
        let final_path = dir.join(format!("{stem}.{ext}"));
        let mut part = final_path.clone().into_os_string();
        part.push(".part");
        let part_path = PathBuf::from(part);
        if !final_path.exists() && !part_path.exists() {
            return Ok(OutputPlan {
                part_path,
                final_path,
            });
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("no free recording name in {}", dir.display()),
    ))
}

/// Rename `part_path` to `final_path`, returning the final path.
pub fn finalize(plan: OutputPlan) -> io::Result<PathBuf> {
    fs::rename(&plan.part_path, &plan.final_path)?;
    Ok(plan.final_path)
}
