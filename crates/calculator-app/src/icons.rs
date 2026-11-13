use crate::icon_svgs::ICONS;

pub const FX: &str = "nyx-calc-fx-symbolic";
pub const BINARY: &str = "nyx-calc-binary-symbolic";
pub const CONVERT: &str = "nyx-calc-convert-symbolic";
pub const SHEET: &str = "nyx-calc-sheet-symbolic";
pub const PLOT: &str = "nyx-calc-plot-symbolic";
pub const BACKSPACE: &str = "nyx-calc-backspace-symbolic";
pub const CALC: &str = "nyx-calc-calc-symbolic";
pub const PIN: &str = "nyx-calc-pin-symbolic";

/// Writes the icons to the app's cache once and adds that folder to the icon theme.
pub fn install() {
    let dir = nyx_core::paths::cache_dir().join("calc-icons");
    for (name, svg) in ICONS {
        let path = dir.join(format!("{name}.svg"));
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*svg) {
            let _ = nyx_core::cmd::atomic_write(&path, svg);
        }
    }
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(dir);
    }
}
