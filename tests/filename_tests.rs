use niri_recorder::files::naming::{finalize, plan_output};

#[test]
fn name_is_timestamped_mp4_in_dir() {
    let d = tempfile::tempdir().unwrap();
    let p = plan_output(d.path(), "mp4").unwrap();
    assert_eq!(p.part_path.extension().unwrap(), "part");
    let n = p.final_path.file_name().unwrap().to_str().unwrap();
    assert!(n.starts_with("screen-recording-") && n.ends_with(".mp4"));
}

#[test]
fn never_overwrites_existing() {
    let d = tempfile::tempdir().unwrap();
    let a = plan_output(d.path(), "mp4").unwrap();
    std::fs::write(&a.final_path, b"x").unwrap(); // simulate existing
    let b = plan_output(d.path(), "mp4").unwrap();
    assert_ne!(a.final_path, b.final_path);
    assert!(b.final_path.to_str().unwrap().contains("-1."));
}

#[test]
fn finalize_renames_part_to_final() {
    let d = tempfile::tempdir().unwrap();
    let p = plan_output(d.path(), "mp4").unwrap();
    std::fs::write(&p.part_path, b"data").unwrap();
    let done = finalize(p).unwrap();
    assert!(done.exists());
}
