//! Settings dialog: an `adw::PreferencesDialog` (v1_5) with groups for
//! output directory, capture defaults, shortcuts/countdown, encoder and
//! retention. Every change is persisted to `config.toml` immediately via
//! `settings::save` and mirrored into the main window via
//! [`Ui::apply_settings`].

use super::app::{combo, Ui, COUNTDOWN_CHOICES, FPS_CHOICES};
use super::format::{encoder_choices, encoder_index};
use crate::config::settings::{self, Quality, Settings, Target};
use gtk4::{self as gtk, gio};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Build and present the preferences dialog over `parent`; returns it so
/// the caller can hold the open instance (single-instance guard in
/// `Ui::settings_dialog`). Loads `config.toml` fresh so the rows always
/// show on-disk state, not the launch snapshot.
pub(super) fn present(
    parent: &adw::ApplicationWindow,
    ui: &Rc<RefCell<Ui>>,
    codecs: &[String],
) -> adw::PreferencesDialog {
    // One shared copy; every row mutates it then applies.
    let settings = Rc::new(RefCell::new(settings::load()));

    // mutate → save → mirror; a failed save warns on stderr and the UI
    // still applies the change in-memory.
    let apply: Rc<dyn Fn(&Settings)> = {
        let ui = Rc::clone(ui);
        Rc::new(move |s: &Settings| {
            if let Err(e) = settings::save(s) {
                eprintln!("niri-recorder: cannot save settings: {e}");
            }
            ui.borrow_mut().apply_settings(s);
        })
    };

    // --- output directory ---------------------------------------------
    let dir_row = adw::ActionRow::builder()
        .title("Output directory")
        .subtitle(&settings.borrow().output_directory)
        .build();
    let pick = gtk::Button::builder()
        .icon_name("folder-open-symbolic")
        .valign(gtk::Align::Center)
        .build();
    dir_row.add_suffix(&pick);
    dir_row.set_activatable_widget(Some(&pick));
    {
        let dir_row = dir_row.clone();
        let parent = parent.clone();
        let settings = Rc::clone(&settings);
        let apply = Rc::clone(&apply);
        pick.connect_clicked(move |_| {
            let fd = gtk::FileDialog::builder()
                .title("Choose output directory")
                .build();
            let dir_row = dir_row.clone();
            let settings = Rc::clone(&settings);
            let apply = Rc::clone(&apply);
            fd.select_folder(Some(&parent), gio::Cancellable::NONE, move |res| {
                let Ok(file) = res else { return };
                let Some(path) = file.path() else { return };
                let stored = settings::collapse_tilde(&path);
                dir_row.set_subtitle(&stored);
                let mut s = settings.borrow_mut();
                s.output_directory = stored;
                apply(&s);
            });
        });
    }

    // --- selectors mirroring main-window rows --------------------------
    let target_row = combo("Capture target", &["Full display", "Region"]);
    target_row.set_selected(match settings.borrow().default_target {
        Target::Display => 0,
        Target::Region => 1,
    });
    let fps_names: Vec<String> = FPS_CHOICES.iter().map(u32::to_string).collect();
    let fps_refs: Vec<&str> = fps_names.iter().map(String::as_str).collect();
    let fps_row = combo("Frame rate", &fps_refs);
    fps_row.set_selected(
        FPS_CHOICES
            .iter()
            .position(|f| *f == settings.borrow().default_fps)
            .unwrap_or(1) as u32,
    );
    let quality_row = combo("Quality", &["Standard", "High"]);
    quality_row.set_selected(match settings.borrow().quality {
        Quality::Standard => 0,
        Quality::High => 1,
    });

    // --- countdown ------------------------------------------------------
    let countdown_names = ["Off", "3 seconds", "5 seconds", "10 seconds"];
    let countdown_row = combo("Countdown before capture", &countdown_names);
    countdown_row.set_selected(
        COUNTDOWN_CHOICES
            .iter()
            .position(|s| *s == settings.borrow().countdown_seconds)
            .unwrap_or(0) as u32,
    );

    // --- notifications --------------------------------------------------
    let notify_row = adw::SwitchRow::builder()
        .title("Desktop notifications")
        .subtitle("Show notifications when recording starts, stops, or fails")
        .active(settings.borrow().notifications)
        .build();
    // --- shortcuts ------------------------------------------------------
    let start_row = adw::EntryRow::builder()
        .title("Start shortcut")
        .text(&settings.borrow().start_shortcut)
        .show_apply_button(true)
        .build();
    start_row.set_tooltip_text(Some(
        "GTK accelerator, e.g. <Control>r. Empty disables. Only active while \
         this window is focused; use a niri bind for a global shortcut.",
    ));
    let stop_row = adw::EntryRow::builder()
        .title("Stop shortcut")
        .text(&settings.borrow().stop_shortcut)
        .show_apply_button(true)
        .build();
    stop_row.set_tooltip_text(Some("GTK accelerator, e.g. <Control>s. Empty disables."));

    // --- encoder --------------------------------------------------------
    let codec_choices = encoder_choices(codecs);
    let encoder_labels: Vec<String> = codec_choices.iter().map(|(_, l)| l.clone()).collect();
    let encoder_refs: Vec<&str> = encoder_labels.iter().map(String::as_str).collect();
    let encoder_row = combo("Encoder", &encoder_refs);
    encoder_row.set_selected(encoder_index(&codec_choices, &settings.borrow().encoder));

    // --- retention ------------------------------------------------------
    let cleanup_row = adw::SwitchRow::builder()
        .title("Automatic cleanup")
        .subtitle("Delete old finished recordings after each save; off by default")
        .active(settings.borrow().cleanup_enabled)
        .build();
    let cleanup_files_row = adw::SpinRow::builder()
        .title("Keep at most")
        .subtitle("0 = no file-count limit")
        .adjustment(&gtk::Adjustment::new(
            f64::from(settings.borrow().cleanup_max_files),
            0.0,
            9_999.0,
            1.0,
            10.0,
            0.0,
        ))
        .digits(0)
        .climb_rate(1.0)
        .build();
    let cleanup_age_row = adw::SpinRow::builder()
        .title("Delete older than (days)")
        .subtitle("0 = no age limit")
        .adjustment(&gtk::Adjustment::new(
            f64::from(settings.borrow().cleanup_max_age_days),
            0.0,
            3_650.0,
            1.0,
            10.0,
            0.0,
        ))
        .digits(0)
        .climb_rate(1.0)
        .build();
    let set_cleanup_sensitive = {
        let files = cleanup_files_row.clone();
        let age = cleanup_age_row.clone();
        move |on: bool| {
            files.set_sensitive(on);
            age.set_sensitive(on);
        }
    };
    set_cleanup_sensitive(settings.borrow().cleanup_enabled);

    {
        let settings = Rc::clone(&settings);
        let apply = Rc::clone(&apply);
        target_row.connect_selected_notify(move |row| {
            let mut s = settings.borrow_mut();
            s.default_target = if row.selected() == 1 {
                Target::Region
            } else {
                Target::Display
            };
            apply(&s);
        });
    }
    {
        let settings = Rc::clone(&settings);
        let apply = Rc::clone(&apply);
        fps_row.connect_selected_notify(move |row| {
            let mut s = settings.borrow_mut();
            s.default_fps = FPS_CHOICES
                .get(row.selected() as usize)
                .copied()
                .unwrap_or(30);
            apply(&s);
        });
    }
    {
        let settings = Rc::clone(&settings);
        let apply = Rc::clone(&apply);
        quality_row.connect_selected_notify(move |row| {
            let mut s = settings.borrow_mut();
            s.quality = if row.selected() == 1 {
                Quality::High
            } else {
                Quality::Standard
            };
            apply(&s);
        });
    }
    {
        let settings = Rc::clone(&settings);
        let apply = Rc::clone(&apply);
        countdown_row.connect_selected_notify(move |row| {
            let mut s = settings.borrow_mut();
            s.countdown_seconds = COUNTDOWN_CHOICES
                .get(row.selected() as usize)
                .copied()
                .unwrap_or(0);
            apply(&s);
        });
    }
    {
        let settings = Rc::clone(&settings);
        let apply = Rc::clone(&apply);
        notify_row.connect_active_notify(move |row| {
            let mut s = settings.borrow_mut();
            s.notifications = row.is_active();
            apply(&s);
        });
    }
    // EntryRow.apply fires on Enter or the apply button, so partial input
    // never lands in config.toml.
    {
        let settings = Rc::clone(&settings);
        let apply = Rc::clone(&apply);
        start_row.connect_apply(move |row| {
            let mut s = settings.borrow_mut();
            s.start_shortcut = row.text().to_string();
            apply(&s);
        });
    }
    {
        let settings = Rc::clone(&settings);
        let apply = Rc::clone(&apply);
        stop_row.connect_apply(move |row| {
            let mut s = settings.borrow_mut();
            s.stop_shortcut = row.text().to_string();
            apply(&s);
        });
    }
    {
        let settings = Rc::clone(&settings);
        let apply = Rc::clone(&apply);
        encoder_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            let Some((value, _)) = codec_choices.get(idx) else {
                return;
            };
            let mut s = settings.borrow_mut();
            s.encoder = value.clone();
            apply(&s);
        });
    }
    {
        let settings = Rc::clone(&settings);
        let apply = Rc::clone(&apply);
        let set_sensitive = set_cleanup_sensitive.clone();
        cleanup_row.connect_active_notify(move |row| {
            set_sensitive(row.is_active());
            let mut s = settings.borrow_mut();
            s.cleanup_enabled = row.is_active();
            apply(&s);
        });
    }
    {
        let settings = Rc::clone(&settings);
        let apply = Rc::clone(&apply);
        cleanup_files_row.connect_value_notify(move |row| {
            let mut s = settings.borrow_mut();
            s.cleanup_max_files = row.value() as u32;
            apply(&s);
        });
    }
    {
        let settings = Rc::clone(&settings);
        let apply = Rc::clone(&apply);
        cleanup_age_row.connect_value_notify(move |row| {
            let mut s = settings.borrow_mut();
            s.cleanup_max_age_days = row.value() as u32;
            apply(&s);
        });
    }

    let group = adw::PreferencesGroup::new();
    group.add(&dir_row);
    group.add(&target_row);
    group.add(&fps_row);
    group.add(&quality_row);
    group.add(&countdown_row);

    group.add(&notify_row);
    let shortcut_group = adw::PreferencesGroup::builder()
        .title("Shortcuts")
        .description(
            "Window shortcuts only — bind `niri-recorder start|stop` in niri for global keys",
        )
        .build();
    shortcut_group.add(&start_row);
    shortcut_group.add(&stop_row);

    let encoder_group = adw::PreferencesGroup::builder()
        .title("Encoder")
        .description("Hardware encoders fall back to software when unavailable")
        .build();
    encoder_group.add(&encoder_row);

    let cleanup_group = adw::PreferencesGroup::builder().title("Retention").build();
    cleanup_group.add(&cleanup_row);
    cleanup_group.add(&cleanup_files_row);
    cleanup_group.add(&cleanup_age_row);

    let page = adw::PreferencesPage::builder().title("General").build();
    page.add(&group);
    page.add(&shortcut_group);
    page.add(&encoder_group);
    page.add(&cleanup_group);
    let dialog = adw::PreferencesDialog::builder().title("Settings").build();
    dialog.add(&page);
    dialog.present(Some(parent));
    // Clear the single-instance slot so the next gear-click rebuilds.
    dialog.connect_closed({
        let ui = Rc::clone(ui);
        move |_| {
            if let Ok(mut u) = ui.try_borrow_mut() {
                u.settings_dialog = None;
            }
        }
    });
    dialog
}
