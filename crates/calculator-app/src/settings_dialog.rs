use crate::widgets::{self, Page};
use crate::{prefs, window};
use gtk::prelude::*;
use nyx_ui::settings_card::{self, About, Spec, Standard};

const GROUPS: &[(&str, &str, &str)] = &[
    ("Calculations", "accessories-calculator-symbolic", "Precision, separators"),
    ("History", "document-open-recent-symbolic", "Retention, clear history"),
];

const SPEC: Spec = Spec {
    groups: GROUPS,
    search_text: None,
    standard: Some(Standard {
        app: "calculator",
        about: About {
            name: "Calculator",
            icon: "io.github.design_nexus.nyx.Calculator",
            version: env!("CARGO_PKG_VERSION"),
            description: "Standard, scientific and programmer modes, units, sheets and graphs.",
            licence: "MIT",
            website: "https://github.com/design-nexus/nyx-calculator",
            issues: "https://github.com/design-nexus/nyx-calculator/issues",
            credits: &[],
        },
        shortcuts: SHORTCUTS,
        reduce_motion: || prefs::get().reduce_motion,
        set_reduce_motion: |on| prefs::update(|p| p.reduce_motion = on),
    }),
};

pub fn open() {
    let (Some(overlay), Some(win)) = (window::overlay(), window::window()) else {
        return;
    };
    // The card opens over the calculator, from a tool panel too.
    window::show();
    settings_card::open(&overlay, &win, SPEC, || {
        let p = widgets::page("settings");
        build_content(&p);
        (p.root.upcast(), p.body)
    });
}

fn build_content(p: &Page) {
    calc_group(p);
    history_group(p);
}

fn calc_group(p: &Page) {
    let group = p.group("Calculations");
    let current = prefs::get();

    let (row, _) = widgets::choice_row(
        "Angles",
        "Units for trigonometric calculations.",
        vec![("degrees".into(), "Degrees".into()), ("radians".into(), "Radians".into())],
        if current.radians { "radians" } else { "degrees" },
        |id| {
            prefs::update(|p| p.radians = id == "radians");
            window::apply_angle_preference();
        },
    );
    group.add(&row);
    let (row, _) = widgets::choice_row(
        "Start in",
        "What opens next time. Switching keypads also remembers your choice; a tool opens beside the calculator.",
        vec![
            ("standard".into(), "Standard".into()),
            ("scientific".into(), "Scientific".into()),
            ("programmer".into(), "Programmer".into()),
            ("converter".into(), "Unit converter".into()),
            ("sheets".into(), "Sheets".into()),
            ("graph".into(), "Graph".into()),
        ],
        &current.calculator_mode,
        |id| prefs::update(|p| p.calculator_mode = id),
    );
    group.add(&row);
    let prec_options = vec![
        ("0".into(), "Automatic".into()),
        ("2".into(), "2 decimal places".into()),
        ("4".into(), "4 decimal places".into()),
        ("6".into(), "6 decimal places".into()),
        ("8".into(), "8 decimal places".into()),
        ("12".into(), "12 decimal places".into()),
    ];

    let (row, _) = widgets::choice_row(
        "Decimal precision",
        "Number of decimal places shown in results.",
        prec_options,
        &current.precision.to_string(),
        |val| {
            let p: usize = val.parse().unwrap_or(0);
            prefs::update(|pr| pr.precision = p);
            window::refresh_display();
        },
    );
    group.add(&row);

    let (row, _) = widgets::switch_row(
        "Thousands separators",
        "Group digits with commas (e.g. 1,234,567.89).",
        current.thousands_separator,
        |on| {
            prefs::update(|pr| pr.thousands_separator = on);
            window::refresh_display();
        },
    );
    group.add(&row);
}

fn history_group(p: &Page) {
    let group = p.group("History");
    let current = prefs::get();

    let (row, _) = widgets::switch_row("Record history", "Save completed calculations locally.", current.history_enabled, |on| {
        prefs::update(|pr| pr.history_enabled = on);
    });
    group.add(&row);

    let ret_options = vec![
        ("50".into(), "50 calculations".into()),
        ("100".into(), "100 calculations".into()),
        ("250".into(), "250 calculations".into()),
        ("1000".into(), "1,000 calculations".into()),
    ];

    let (row, _) = widgets::choice_row(
        "History retention",
        "Maximum number of unpinned calculations to keep.",
        ret_options,
        &current.history_retention.to_string(),
        |val| {
            let n: usize = val.parse().unwrap_or(100);
            prefs::update(|pr| pr.history_retention = n);
        },
    );
    group.add(&row);
}

/// Every shortcut, for the card's Keyboard group.
const SHORTCUTS: &[(&[&str], &str)] = &[
    (&["Alt", "1"], "Standard keypad"),
    (&["Alt", "2"], "Scientific keypad"),
    (&["Alt", "3"], "Programmer keypad"),
    (&["Alt", "4"], "Unit converter, beside the calculator"),
    (&["Alt", "5"], "Sheets, beside the calculator"),
    (&["Alt", "6"], "Graph, beside the calculator"),
    (&["Esc"], "Put the open tool away"),
    (&["0–9", "."], "Enter digits and decimal point"),
    (&["+", "−", "×", "÷"], "Basic arithmetic operations"),
    (&["Enter", "="], "Calculate result"),
    (&["Backspace"], "Delete last character"),
    (&["Esc", "C"], "Clear current expression"),
    (&["Ctrl", "H"], "Show or hide history"),
    (&["Ctrl", "C"], "Copy result to clipboard"),
    (&["Ctrl", "V"], "Paste expression"),
    (&["Ctrl", "M"], "Store current value in memory (MS)"),
    (&["Ctrl", "R"], "Recall memory value (MR)"),
    (&["Ctrl", "L"], "Clear memory register (MC)"),
    (&["Ctrl", ","], "Open settings"),
    (&["F1"], "Show shortcuts dialog"),
];
