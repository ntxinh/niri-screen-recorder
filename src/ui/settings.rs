//! Settings dialog: an `adw::PreferencesDialog` (v1_5) with one group —
//! output directory (folder picker), capture target, frame rate and
//! quality. Every change is persisted to `config.toml` immediately via
//! `settings::save` and mirrored into the main window's selectors /
//! recent list via [`Ui::apply_settings`].

use super::app::{combo, Ui, FPS_CHOICES};
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

    let group = adw::PreferencesGroup::new();
    group.add(&dir_row);
    group.add(&target_row);
    group.add(&fps_row);
    group.add(&quality_row);
    let page = adw::PreferencesPage::builder().title("General").build();
    page.add(&group);
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
