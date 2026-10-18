//! In-window start/stop accelerators.
//!
//! Wayland offers no portable way for an app to register a global hotkey
//! (niri users bind `niri-recorder start|stop` in `config.kdl` instead), so
//! these are ordinary window accelerators: they fire only while the
//! recorder window has focus. Spec text is GTK accelerator syntax, e.g.
//! `<Control>r`; an empty string disables that binding.

use gtk4 as gtk;
use gtk4::gdk;

/// A parsed accelerator: key sym + modifiers.
pub type Accel = (gdk::Key, gdk::ModifierType);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
}

/// What the window should do for this key press.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Shortcuts {
    start: Option<Accel>,
    stop: Option<Accel>,
}

/// Parse one accelerator spec. Invalid or empty input yields `None`
/// (the binding is simply inactive).
pub fn parse(spec: &str) -> Option<Accel> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }
    let Some((key, mods)) = gtk::accelerator_parse(spec) else {
        tracing::warn!(shortcut = spec, "ignoring invalid shortcut");
        return None;
    };
    if key == gdk::Key::VoidSymbol || !gtk::accelerator_valid(key, mods) {
        tracing::warn!(shortcut = spec, "ignoring invalid shortcut");
        return None;
    }
    Some((key, mods))
}

impl Shortcuts {
    pub fn new(start: &str, stop: &str) -> Self {
        Self {
            start: parse(start),
            stop: parse(stop),
        }
    }

    /// Replace both bindings (settings dialog wrote new values).
    pub fn set(&mut self, start: &str, stop: &str) {
        self.start = parse(start);
        self.stop = parse(stop);
    }

    /// Match a key press. Lock modifiers (Caps/Num) are masked out so a
    /// `<Control>r` binding still fires with Caps Lock on.
    pub fn action(&self, key: gdk::Key, mods: gdk::ModifierType) -> Option<Action> {
        // Avoid a GTK call (which needs `gtk::init`) when nothing is bound.
        if self.start.is_none() && self.stop.is_none() {
            return None;
        }
        let mods = mods & gtk::accelerator_get_default_mod_mask();
        if self.start == Some((key, mods)) {
            return Some(Action::Start);
        }
        if self.stop == Some((key, mods)) {
            return Some(Action::Stop);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `gtk::accelerator_parse` needs a display, so only the empty/disable
    // path and the pure matching logic are covered headlessly.
    #[test]
    fn empty_disables_bindings() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("   "), None);
        let s = Shortcuts::new("", "");
        assert_eq!(s.action(gdk::Key::r, gdk::ModifierType::CONTROL_MASK), None);
    }
}
