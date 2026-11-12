//! Nyx Calculator — A modern, fast desktop calculator for Omarchy and Nyx.

mod display;
mod history_sidebar;
mod keypad;
mod keys;
mod prefs;
mod settings_dialog;
mod theme;
mod views;
mod widgets;
mod window;

use gtk::prelude::*;
use gtk::{gio, glib};

pub const APP_ID: &str = "io.github.design_nexus.nyx.Calculator";

const USAGE: &str = "Usage: nyx-calculator [OPTIONS]\n\
\n\
  --toggle        Close the window if it's open, otherwise open it\n\
  --help, -h      Show this help message\n";

fn main() -> glib::ExitCode {
    nyx_core::app::init(nyx_core::app::App { slug: "nyx-calculator", name: "Calculator" });

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return glib::ExitCode::SUCCESS;
    }

    if std::env::var_os("GSK_RENDERER").is_none() {
        // SAFETY: single-threaded startup before GTK initializes
        unsafe { std::env::set_var("GSK_RENDERER", "ngl") };
    }

    let app = gtk::Application::builder().application_id(APP_ID).flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE).build();

    app.connect_startup(|_| theme::install());

    app.connect_command_line(|app, cl| {
        let argv: Vec<String> = cl.arguments().iter().map(|a| a.to_string_lossy().to_string()).collect();
        let has = |flag: &str| argv.iter().any(|a| a == flag);
        let value = |flag: &str| argv.iter().position(|a| a == flag).and_then(|i| argv.get(i + 1)).cloned();

        if has("--toggle")
            && let Some(w) = window::window()
            && w.is_visible()
        {
            w.close();
            return glib::ExitCode::SUCCESS;
        }

        window::present(app, value("--section").as_deref());
        glib::ExitCode::SUCCESS
    });

    app.connect_shutdown(|_| prefs::flush());
    app.run()
}
