//! Application window: one `ApplicationWindow` whose content is the
//! control surface from spec §8 — target/display/fps/quality selectors,
//! a Start/Stop primary action, a state + elapsed status line and the
//! output path. All widgets map 1:1 onto `AppState`/`Settings`/
//! `CaptureCapabilities`; the blocking `Recorder` lives on the
//! `GuiController`'s worker thread.
//!
//! `adw::StyleManager` follows the system dark/light preference by
//! default — no manual theme code.

use super::controller::{GuiController, UiCommand, UiEvent};
use super::format::{format_elapsed, outputs_model, state_label};
use crate::app::pidfile::read_pidfile;
use crate::app::state::AppState;
use crate::cli::commands::check_preferred_backend;
use crate::config::settings::{self, Quality, Target};
use gtk4::{self as gtk, gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

const APP_ID: &str = "io.github.niri_recorder";
const FPS_CHOICES: [u32; 3] = [24, 30, 60];

/// Build the application and run the main loop; returns the exit code
/// passed to the process once the loop ends. `adw::Application::new`
/// initializes libadwaita; display availability is checked by the caller.
pub(super) fn run() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::FLAGS_NONE)
        .build();
    app.connect_activate(|app| {
        // Single-window app: a second activation (e.g. launcher re-run)
        // presents the existing window instead of building a duplicate
        // whose signal handler would collide with the first.
        if let Some(w) = app.active_window() {
            w.present();
            return;
        }
        let window = build_window(app);
        window.present();
    });
    app.run()
}

/// Widgets + view state shared by the GTK callbacks. Wrapped in
/// `Rc<RefCell>`; the event pump and tick hold weak refs so closing the
/// window drops everything and stops their sources.
struct Ui {
    state: AppState,
    started_at: Option<Instant>,
    /// `Some(pid)` while another session (usually a CLI `start`) owns the
    /// pidfile — Start is disabled and the status line says who controls.
    external_pid: Option<u32>,
    /// Backend probe completed on the worker (dropdowns populated).
    backend_ready: bool,
    /// Backend/preferred-backend problem — blocks Start.
    backend_error: Option<String>,
    /// Detail line under the status (error text or output path).
    detail: Option<String>,
    ctl: Rc<GuiController>,
    target_row: adw::ComboRow,
    output_row: adw::ComboRow,
    fps_row: adw::ComboRow,
    quality_row: adw::ComboRow,
    primary: gtk::Button,
    status: gtk::Label,
    output_label: gtk::Label,
}

fn combo(title: &str, items: &[&str]) -> adw::ComboRow {
    adw::ComboRow::builder()
        .title(title)
        .model(&gtk::StringList::new(items))
        .build()
}

impl Ui {
    fn on_event(&mut self, ev: UiEvent) {
        match ev {
            UiEvent::BackendReady(caps) => {
                self.backend_ready = true;
                self.populate_outputs(&outputs_model(&caps.outputs));
            }
            UiEvent::BackendError(msg) => {
                self.backend_error = Some(msg);
            }
            UiEvent::StateChanged(s) => {
                self.state = s;
                self.detail = None;
            }
            UiEvent::RecordingStarted { part_path } => {
                self.started_at = Some(Instant::now());
                self.state = AppState::Recording { pid: 0 };
                self.detail = part_path.map(|p| p.display().to_string());
            }
            UiEvent::Completed(path) => {
                self.started_at = None;
                self.state = AppState::Completed(path.clone());
                self.detail = Some(path.display().to_string());
            }
            UiEvent::Failed(msg) => {
                self.started_at = None;
                self.state = AppState::Failed(msg.clone());
                self.detail = Some(msg);
            }
        }
        self.refresh();
    }

    /// Derive every sensitivity/label from `self.state` — no scattered
    /// per-callback widget mutation.
    fn refresh(&mut self) {
        let idleish = matches!(
            self.state,
            AppState::Idle | AppState::Completed(_) | AppState::Failed(_)
        );
        let recording = matches!(self.state, AppState::Recording { .. });
        let is_region = self.target_row.selected() == 1;
        let can_start = idleish
            && self.backend_ready
            && self.backend_error.is_none()
            && self.external_pid.is_none();

        // All selectors insensitive during transitions (SelectingRegion /
        // Starting / Recording / Stopping); display additionally only
        // meaningful for Full display.
        for row in [
            &self.target_row,
            &self.output_row,
            &self.fps_row,
            &self.quality_row,
        ] {
            row.set_sensitive(idleish);
        }
        self.output_row.set_sensitive(idleish && !is_region);
        self.primary.set_label(if recording {
            "Stop Recording"
        } else {
            "Start Recording"
        });
        self.primary.set_sensitive(can_start || recording);

        let status = if let Some(err) = &self.backend_error {
            err.clone()
        } else if let Some(pid) = self.external_pid {
            format!("recording (pid {pid}) — controlled by another session")
        } else if !self.backend_ready {
            "Checking backend…".to_string()
        } else {
            match (&self.state, self.started_at) {
                (AppState::Recording { .. }, Some(t)) => {
                    format!(
                        "{} · {}",
                        state_label(&self.state),
                        format_elapsed(t.elapsed())
                    )
                }
                _ => state_label(&self.state).to_string(),
            }
        };
        self.status.set_label(&status);
        self.output_label
            .set_label(self.detail.as_deref().unwrap_or(""));
    }

    /// Swap the display dropdown's model after the worker's backend probe.
    fn populate_outputs(&mut self, names: &[String]) {
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        self.output_row
            .set_model(Some(&gtk::StringList::new(&refs)));
        self.output_row.set_selected(0);
    }

    /// The dropdown selections, resolved to command arguments.
    fn selections(&self) -> (Target, Option<String>, u32, Quality) {
        let target = if self.target_row.selected() == 1 {
            Target::Region
        } else {
            Target::Display
        };
        let output = (!matches!(target, Target::Region))
            .then(|| self.output_row.selected_item())
            .flatten()
            .and_downcast::<gtk::StringObject>()
            .map(|o| o.string().to_string());
        let fps = FPS_CHOICES
            .get(self.fps_row.selected() as usize)
            .copied()
            .unwrap_or(30);
        let quality = match self.quality_row.selected() {
            1 => Quality::High,
            _ => Quality::Standard,
        };
        (target, output, fps, quality)
    }
}

fn build_window(app: &adw::Application) -> adw::ApplicationWindow {
    let settings = settings::load();
    // Pure config check on the main thread; the subprocess probe
    // (wf-recorder/slurp/niri) runs on the worker and arrives as
    // BackendReady/BackendError — the window never blocks on niri IPC.
    let backend_error = check_preferred_backend(&settings).err();

    let (tx_events, rx_events) = async_channel::unbounded::<UiEvent>();
    let ctl = Rc::new(GuiController::spawn(settings.clone(), tx_events));
    // `niri-recorder stop` SIGINTs this process (it owns the pidfile);
    // route the signal to the worker's Stop path, same as the CLI's flag.
    let cmd_tx = ctl.command_sender();
    let quit_flag = ctl.quit_flag();
    if let Err(e) = ctrlc::set_handler(move || {
        quit_flag.store(true, Ordering::SeqCst);
        let _ = cmd_tx.send(UiCommand::Stop);
    }) {
        eprintln!("niri-recorder: cannot install signal handler: {e}");
    }

    // --- selectors ------------------------------------------------------
    let target_row = combo("Capture target", &["Full display", "Region"]);
    target_row.set_selected(match settings.default_target {
        Target::Display => 0,
        Target::Region => 1,
    });
    // Empty until the worker's BackendReady event populates it.
    let output_row = combo("Display", &[]);
    let fps_names: Vec<String> = FPS_CHOICES.iter().map(u32::to_string).collect();
    let fps_refs: Vec<&str> = fps_names.iter().map(String::as_str).collect();
    let fps_row = combo("Frame rate", &fps_refs);
    fps_row.set_selected(
        FPS_CHOICES
            .iter()
            .position(|f| *f == settings.default_fps)
            .unwrap_or(1) as u32,
    );
    let quality_row = combo("Quality", &["Standard", "High"]);
    quality_row.set_selected(match settings.quality {
        Quality::Standard => 0,
        Quality::High => 1,
    });

    // ComboRow is a ListBoxRow — it must live in a list container
    // (PreferencesGroup wraps a ListBox); appending to a plain Box
    // triggers gtk_list_box_row_grab_focus assertions.
    let selectors = adw::PreferencesGroup::new();
    for row in [&target_row, &output_row, &fps_row, &quality_row] {
        selectors.add(row);
    }

    let primary = gtk::Button::builder()
        .label("Start Recording")
        .css_classes(["suggested-action", "pill"])
        .margin_top(6)
        .build();
    let status = gtk::Label::builder()
        .label("Idle")
        .css_classes(["heading"])
        .margin_top(6)
        .build();
    let output_label = gtk::Label::builder()
        .label("")
        .css_classes(["dim-label", "caption"])
        .selectable(true)
        .wrap(true)
        .build();

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    content.append(&selectors);
    content.append(&primary);
    content.append(&status);
    content.append(&output_label);

    let header = adw::HeaderBar::new();
    // Settings dialog lands in Task 4; the button is a placeholder until then.
    let settings_btn = gtk::Button::builder()
        .icon_name("emblem-system-symbolic")
        .tooltip_text("Settings")
        .sensitive(false)
        .build();
    header.pack_end(&settings_btn);

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&content));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("niri-recorder")
        .default_width(420)
        .default_height(340)
        .content(&view)
        .build();

    let ui = Rc::new(RefCell::new(Ui {
        state: AppState::Idle,
        started_at: None,
        external_pid: read_pidfile().map(|e| e.pid),
        backend_ready: false,
        backend_error,
        detail: None,
        ctl,
        target_row,
        output_row,
        fps_row,
        quality_row,
        primary,
        status,
        output_label,
    }));

    // --- wiring ---------------------------------------------------------
    // Changing target flips the display row's sensitivity via refresh.
    // Clone widget handles first so the Ref temp is dropped before the
    // closure moves `ui` (E0505).
    let target_row = ui.borrow().target_row.clone();
    {
        let ui = Rc::clone(&ui);
        target_row.connect_selected_notify(move |_| {
            ui.borrow_mut().refresh();
        });
    }
    let primary = ui.borrow().primary.clone();
    {
        let ui = Rc::clone(&ui);
        primary.connect_clicked(move |_| {
            let mut ui = ui.borrow_mut();
            if matches!(ui.state, AppState::Recording { .. }) {
                ui.ctl.request_stop();
            } else {
                let (target, output, fps, quality) = ui.selections();
                ui.detail = None;
                ui.ctl.request_start(target, output, fps, quality);
            }
        });
    }

    // Elapsed timer + external-pidfile watch. Tick only does work while
    // Recording or while idle-checking the pidfile; it dies with `ui`.
    glib::timeout_add_local(
        Duration::from_millis(500),
        glib::clone!(
            #[weak]
            ui,
            #[upgrade_or]
            glib::ControlFlow::Break,
            move || {
                let mut ui = ui.borrow_mut();
                // Another session may claim the pidfile in any startable
                // state (Idle/Completed/Failed), not just Idle.
                ui.external_pid = if matches!(
                    ui.state,
                    AppState::Idle | AppState::Completed(_) | AppState::Failed(_)
                ) {
                    read_pidfile().map(|e| e.pid)
                } else {
                    None
                };
                ui.refresh();
                glib::ControlFlow::Continue
            }
        ),
    );

    // Worker → main event pump. The async_channel receiver is drained
    // inside the default main context (`glib::MainContext::channel` was
    // removed in glib-rs 0.22). A manual Weak upgrade inside the loop —
    // not clone!'s capture upgrade — keeps the future from holding `ui`
    // alive past window close, which would break GuiController's Drop.
    let weak_ui = Rc::downgrade(&ui);
    glib::spawn_future_local(async move {
        while let Ok(ev) = rx_events.recv().await {
            let Some(ui) = weak_ui.upgrade() else { break };
            ui.borrow_mut().on_event(ev);
        }
    });
    ui.borrow_mut().refresh();
    window
}
