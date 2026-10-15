use niri_recorder::config::settings::*;

#[test]
fn defaults_when_file_missing() {
    let s = Settings::from_toml_str("").unwrap();
    assert!(s.output_directory.ends_with("Screen Recordings"));
    assert_eq!(s.default_fps, 30);
    assert_eq!(s.preferred_backend, "auto");
}

#[test]
fn malformed_toml_falls_back_not_fails() {
    let s = load_from_str("{{{{"); // internal helper returning Settings, never Err
    assert_eq!(s.default_fps, 30);
}

#[test]
fn fps_must_be_24_30_or_60() {
    assert!(Settings::from_toml_str("default_fps = 30").is_ok());
    assert!(Settings::from_toml_str("default_fps = 15").is_err());
}

#[test]
fn tilde_expands_to_home() {
    let p = expand_tilde("~/Videos/x");
    assert!(p.starts_with(directories::BaseDirs::new().unwrap().home_dir()));
}
