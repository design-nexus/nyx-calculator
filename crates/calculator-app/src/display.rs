use gtk::pango;
use gtk::prelude::*;

pub struct DisplayView {
    pub root: gtk::Box,
    pub tape_label: gtk::Label,
    pub preview_label: gtk::Label,
    pub readout_label: gtk::Label,
    pub error_badge: gtk::Label,
}

impl DisplayView {
    pub fn new() -> Self {
        // Bottom-aligned in whatever height the keypad leaves.
        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        root.add_css_class("calc-display");
        root.set_vexpand(true);
        root.set_valign(gtk::Align::End);

        let line = |class: &str| {
            let l = gtk::Label::new(None);
            l.add_css_class(class);
            l.set_xalign(1.0);
            l.set_ellipsize(pango::EllipsizeMode::Start);
            l
        };

        // The last sum, then its live result while typing, then the number itself
        let tape_label = line("calc-tape");
        root.append(&tape_label);

        let preview_label = line("calc-preview");
        preview_label.set_visible(false);
        root.append(&preview_label);

        let readout_label = line("calc-readout");
        readout_label.set_text("0");
        readout_label.set_selectable(true);
        root.append(&readout_label);

        let error_badge = gtk::Label::new(None);
        error_badge.add_css_class("calc-error-badge");
        error_badge.set_halign(gtk::Align::End);
        error_badge.set_visible(false);
        root.append(&error_badge);

        Self { root, tape_label, preview_label, readout_label, error_badge }
    }

    pub fn set_tape(&self, text: &str) {
        self.tape_label.set_text(text);
    }

    pub fn set_preview(&self, text: Option<&str>) {
        if let Some(t) = text {
            self.preview_label.set_text(&format!("= {t}"));
            self.preview_label.set_visible(true);
        } else {
            self.preview_label.set_text("");
            self.preview_label.set_visible(false);
        }
    }

    pub fn set_readout(&self, text: &str) {
        self.readout_label.set_text(text);
        // Long numbers step down a size before they're cut off at the start.
        if text.chars().count() > 11 {
            self.readout_label.add_css_class("long");
        } else {
            self.readout_label.remove_css_class("long");
        }
    }

    pub fn set_error(&self, error: Option<&str>) {
        if let Some(msg) = error {
            self.error_badge.set_text(msg);
            self.error_badge.set_visible(true);
        } else {
            self.error_badge.set_visible(false);
        }
    }
}
