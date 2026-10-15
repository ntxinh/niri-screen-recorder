//! Application skeleton: one `ApplicationWindow` with a header bar and
//! placeholder content. `adw::StyleManager` follows the system
//! dark/light preference by default — no manual theme code.

use gtk4::{self as gtk, gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;

const APP_ID: &str = "io.github.niri_recorder";

/// Build the application and run the main loop; returns the exit code
/// passed to the process once the loop ends. `adw::Application::new`
/// initializes libadwaita; display availability is checked by the caller.
pub(super) fn run() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::FLAGS_NONE)
        .build();
    app.connect_activate(|app| {
        let window = build_window(app);
        window.present();
    });
    app.run()
}

fn build_window(app: &adw::Application) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("niri-recorder")
        .default_width(420)
        .default_height(300)
        .build();

    let label = gtk::Label::builder()
        .label("niri-recorder")
        .css_classes(["title-1"])
        .build();

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    content.append(&label);

    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&content));

    window.set_content(Some(&view));
    window
}
