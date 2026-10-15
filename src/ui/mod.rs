//! GTK4/libadwaita application shell. `run` is invoked for a bare
//! `niri-recorder` (no subcommand); it exits the process itself on a
//! missing/broken display.

mod app;
pub(crate) mod controller;
mod format;
mod recordings;
mod settings;

/// Run the GUI event loop. Returns the GTK exit code once the last
/// window closes. With no usable display, prints an actionable error to
/// stderr and exits 1 (a GUI app is expected to open a window, not to
/// return silently).
pub fn run() -> gtk4::glib::ExitCode {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_none() {
        no_display();
    }
    if gtk4::init().is_err() || gtk4::gdk::Display::default().is_none() {
        no_display();
    }
    app::run()
}

fn no_display() -> ! {
    eprintln!(
        "niri-recorder: no display available — use a subcommand (start/stop/status/diagnostics)"
    );
    std::process::exit(1);
}
