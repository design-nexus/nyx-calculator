//! Nyx Calculator — A modern, fast desktop calculator for Omarchy and Nyx.

mod display;
mod history_sidebar;
mod hypr;
mod icon_svgs;
mod icons;
mod keypad;
mod keys;
mod prefs;
mod settings_dialog;
mod theme;
mod tools;
mod views;
mod widgets;
mod window;

use gtk::prelude::*;
use gtk::{gio, glib};

pub const APP_ID: &str = "io.github.design_nexus.nyx.Calculator";

/// The application ID, or `NYX_CALCULATOR_APP_ID` for a test copy beside the real one.
pub fn app_id() -> String {
    std::env::var("NYX_CALCULATOR_APP_ID").unwrap_or_else(|_| APP_ID.to_string())
}

const USAGE: &str = "Usage: nyx-calculator [OPTIONS]\n\
\n\
  --toggle        Close the window if it's open, otherwise open it\n\
  --section TOOL  Open a tool beside the calculator: converter, sheets or graph\n\
  --help, -h      Show this help message\n";

fn main() -> glib::ExitCode {
    nyx_core::app::init(nyx_core::app::App { slug: "nyx-calculator", name: "Calculator" });
    // A test copy's dialogs take their Wayland app ID from the program name.
    if std::env::var_os("NYX_CALCULATOR_APP_ID").is_some() {
        glib::set_prgname(Some(app_id().as_str()));
    }

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return glib::ExitCode::SUCCESS;
    }

    if std::env::var_os("GSK_RENDERER").is_none() {
        // SAFETY: single-threaded startup before GTK initializes
        unsafe { std::env::set_var("GSK_RENDERER", "ngl") };
    }

    let app = gtk::Application::builder().application_id(app_id()).flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE).build();

    app.connect_startup(|_| {
        theme::install();
        icons::install();
    });

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
