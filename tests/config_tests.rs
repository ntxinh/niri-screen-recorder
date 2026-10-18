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
fn phase6_defaults_and_old_configs_parse() {
    let s = Settings::from_toml_str("").unwrap();
    assert_eq!(s.start_shortcut, "<Control>r");
    assert_eq!(s.stop_shortcut, "<Control>s");
    assert_eq!(s.countdown_seconds, 0);
    assert!(!s.cleanup_enabled);
    assert_eq!(s.encoder, "auto");
    // A pre-Phase-6 config (unknown keys absent) still parses.
    let old = Settings::from_toml_str("output_directory = \"/tmp\"").unwrap();
    assert_eq!(old.countdown_seconds, 0);
    assert_eq!(old.encoder, "auto");
}

#[test]
fn countdown_must_be_0_3_5_or_10() {
    for ok in [0, 3, 5, 10] {
        assert!(Settings::from_toml_str(&format!("countdown_seconds = {ok}")).is_ok());
    }
    assert!(Settings::from_toml_str("countdown_seconds = 7").is_err());
}

#[test]
fn cleanup_rules_zero_means_no_limit() {
    let s = Settings::from_toml_str(
        "cleanup_enabled = true\ncleanup_max_files = 0\ncleanup_max_age_days = 0",
    )
    .unwrap();
    let r = s.cleanup_rules();
    assert!(r.enabled);
    assert_eq!(r.max_files, None);
    assert_eq!(r.max_age_days, None);

    let s = Settings::from_toml_str("cleanup_max_files = 5\ncleanup_max_age_days = 7").unwrap();
    let r = s.cleanup_rules();
    assert_eq!(r.max_files, Some(5));
    assert_eq!(r.max_age_days, Some(7));
}

#[test]
fn tilde_expands_to_home() {
    let p = expand_tilde("~/Videos/x");
    assert!(p.starts_with(directories::BaseDirs::new().unwrap().home_dir()));
}
