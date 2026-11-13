use crate::prefs;
use calculator_core::units::{UnitCategory, UnitDefinition, convert, find_unit, units_for_category};
use gtk::glib;
use gtk::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::cell::RefCell;
use std::rc::Rc;
use std::str::FromStr;

/// The kinds with a chip of their own; the rest are under More.
const MAIN_KINDS: [UnitCategory; 3] = [UnitCategory::Length, UnitCategory::Mass, UnitCategory::Temperature];
/// How many recent pairs show under the cards.
const RECENT: usize = 3;

pub struct ConverterView {
    pub root: gtk::Box,
    #[allow(dead_code)]
    state: Rc<RefCell<ConverterState>>,
}

struct ConverterState {
    category: UnitCategory,
    units: Vec<&'static UnitDefinition>,
    from_idx: usize,
    to_idx: usize,
    kind_chips: Vec<(UnitCategory, gtk::Button)>,
    more_btn: gtk::Button,
    more_label: gtk::Label,
    from_dropdown: gtk::DropDown,
    to_dropdown: gtk::DropDown,
    from_entry: gtk::Entry,
    result: gtk::Label,
    result_unit: gtk::Label,
    /// The result in full, for Copy.
    exact: String,
    formula_label: gtk::Label,
    chips_title: gtk::Label,
    chips_box: gtk::Box,
    record_timer: Option<glib::SourceId>,
    /// While the dropdowns are being set from here, not by the user.
    is_updating: bool,
}

fn kind_name(c: UnitCategory) -> &'static str {
    match c {
        UnitCategory::Mass => "Weight",
        UnitCategory::Data => "Data",
        other => other.display_name(),
    }
}

/// The pair a kind opens on: Miles to Kilometers and the like.
fn default_pair(c: UnitCategory) -> (usize, usize) {
    match c {
        UnitCategory::Length => (7, 3),
        UnitCategory::Mass => (5, 2),
        UnitCategory::Temperature => (1, 0),
        _ => (0, 1),
    }
}

impl ConverterView {
    pub fn new(on_toast: impl Fn(&str) + 'static) -> Self {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 10);
        column.add_css_class("converter-view");

        // The kinds: the three most used, then More for the rest
        let kinds = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        kinds.add_css_class("converter-kinds");
        let mut kind_chips = Vec::new();
        for c in MAIN_KINDS {
            let b = gtk::Button::with_label(kind_name(c));
            b.add_css_class("converter-kind");
            kinds.append(&b);
            kind_chips.push((c, b));
        }
        let more_btn = gtk::Button::new();
        more_btn.add_css_class("converter-kind");
        let more_content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let more_label = gtk::Label::new(Some("More"));
        more_content.append(&more_label);
        let chevron = gtk::Image::from_icon_name("pan-down-symbolic");
        chevron.add_css_class("calc-mode-chevron");
        more_content.append(&chevron);
        more_btn.set_child(Some(&more_content));
        more_btn.set_tooltip_text(Some("Area, volume, speed, time, data and more"));
        kinds.append(&more_btn);
        column.append(&kinds);

        // From: the unit, then the number to convert
        let (from_card, from_dropdown) = card("From");
        let from_entry = gtk::Entry::new();
        from_entry.set_text("1");
        from_entry.add_css_class("converter-input");
        from_entry.set_has_frame(false);
        from_card.attach(&from_entry, 1, 1, 1, 1);
        column.append(&from_card);

        let swap_btn = nyx_ui::widgets::icon_button(crate::icons::CONVERT, "Swap (Ctrl+S)");
        swap_btn.add_css_class("converter-swap-btn");
        swap_btn.set_halign(gtk::Align::Center);
        column.append(&swap_btn);

        // To: the unit, then the result in the accent with its copy button
        let (to_card, to_dropdown) = card("To");
        to_card.add_css_class("to");
        let value_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        value_row.add_css_class("converter-value-row");
        let result = gtk::Label::new(None);
        result.add_css_class("converter-output");
        result.set_selectable(true);
        result.set_xalign(0.0);
        result.set_ellipsize(gtk::pango::EllipsizeMode::End);
        value_row.append(&result);
        let result_unit = gtk::Label::new(None);
        result_unit.add_css_class("converter-output-unit");
        result_unit.set_valign(gtk::Align::Baseline);
        result_unit.set_hexpand(true);
        result_unit.set_xalign(0.0);
        value_row.append(&result_unit);
        let copy_btn = nyx_ui::widgets::icon_button("edit-copy-symbolic", "Copy the result");
        copy_btn.set_valign(gtk::Align::Center);
        value_row.append(&copy_btn);
        to_card.attach(&value_row, 1, 1, 1, 1);
        column.append(&to_card);

        let formula_label = gtk::Label::new(None);
        formula_label.add_css_class("converter-formula");
        formula_label.set_halign(gtk::Align::Center);
        column.append(&formula_label);

        // Pairs used lately (or common ones for this kind, until there are some)
        let chips_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        chips_row.add_css_class("converter-chips");
        chips_row.set_halign(gtk::Align::Center);
        let chips_title = gtk::Label::new(None);
        chips_title.add_css_class("converter-chips-title");
        chips_row.append(&chips_title);
        let chips_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        chips_row.append(&chips_box);
        column.append(&chips_row);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::External)
            .vexpand(true)
            .child(&column)
            .build();
        root.append(&scroller);

        let (from_idx, to_idx) = default_pair(UnitCategory::Length);
        let state = Rc::new(RefCell::new(ConverterState {
            category: UnitCategory::Length,
            units: Vec::new(),
            from_idx,
            to_idx,
            kind_chips,
            more_btn: more_btn.clone(),
            more_label,
            from_dropdown: from_dropdown.clone(),
            to_dropdown: to_dropdown.clone(),
            from_entry: from_entry.clone(),
            result,
            result_unit,
            exact: String::new(),
            formula_label,
            chips_title,
            chips_box,
            record_timer: None,
            is_updating: false,
        }));

        set_category(&state, UnitCategory::Length, None);

        for (c, b) in state.borrow().kind_chips.iter() {
            let (st, c) = (state.clone(), *c);
            b.connect_clicked(move |_| set_category(&st, c, None));
        }
        let st_more = state.clone();
        more_btn.connect_clicked(move |b| {
            let current = st_more.borrow().category;
            let entries = UnitCategory::ALL
                .into_iter()
                .filter(|c| !MAIN_KINDS.contains(c))
                .map(|c| {
                    let st = st_more.clone();
                    nyx_ui::menu::check(kind_name(c), c == current, move |_| set_category(&st, c, None))
                })
                .collect();
            nyx_ui::menu::popup_below(b, entries);
        });

        let st_from = state.clone();
        from_dropdown.connect_selected_notify(move |dd| {
            if st_from.borrow().is_updating {
                return;
            }
            st_from.borrow_mut().from_idx = dd.selected() as usize;
            recalculate(&st_from);
            schedule_record(&st_from);
        });
        let st_to = state.clone();
        to_dropdown.connect_selected_notify(move |dd| {
            if st_to.borrow().is_updating {
                return;
            }
            st_to.borrow_mut().to_idx = dd.selected() as usize;
            recalculate(&st_to);
            schedule_record(&st_to);
        });
        let st_entry = state.clone();
        from_entry.connect_changed(move |_| recalculate(&st_entry));

        let st_swap = state.clone();
        swap_btn.connect_clicked(move |_| swap(&st_swap));
        let keys = gtk::EventControllerKey::new();
        let st_key = state.clone();
        keys.connect_key_pressed(move |_, key, _, mods| {
            if mods.contains(gtk::gdk::ModifierType::CONTROL_MASK) && matches!(key, gtk::gdk::Key::s | gtk::gdk::Key::S) {
                swap(&st_key);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        root.add_controller(keys);

        let st_copy = state.clone();
        let toast = Rc::new(on_toast);
        copy_btn.connect_clicked(move |b| {
            let (exact, unit) = {
                let st = st_copy.borrow();
                (st.exact.clone(), st.result_unit.text().to_string())
            };
            if !exact.is_empty() {
                b.clipboard().set_text(&exact);
                toast(&format!("Copied {exact} {unit}"));
            }
        });

        Self { root, state }
    }
}

/// A card: its label and unit dropdown, the number under the dropdown.
fn card(title: &str) -> (gtk::Grid, gtk::DropDown) {
    let card = gtk::Grid::builder().column_spacing(10).row_spacing(6).build();
    card.add_css_class("converter-card");
    let label = gtk::Label::new(Some(title));
    label.add_css_class("converter-card-title");
    label.set_xalign(0.0);
    label.set_width_chars(4);
    card.attach(&label, 0, 0, 1, 1);
    let dropdown = gtk::DropDown::new(Some(gtk::StringList::new(&[])), None::<gtk::Expression>);
    dropdown.add_css_class("converter-dropdown");
    dropdown.set_hexpand(true);
    card.attach(&dropdown, 1, 0, 1, 1);
    (card, dropdown)
}

/// Switches to `category`, on `pair` (unit indices) or the kind's usual pair.
fn set_category(state: &Rc<RefCell<ConverterState>>, category: UnitCategory, pair: Option<(usize, usize)>) {
    let units = units_for_category(category);
    let (mut from, mut to) = pair.unwrap_or_else(|| default_pair(category));
    if from >= units.len() {
        from = 0;
    }
    if to >= units.len() {
        to = usize::from(units.len() > 1);
    }
    let names: Vec<&str> = units.iter().map(|u| u.name).collect();
    {
        let mut st = state.borrow_mut();
        st.category = category;
        st.units = units;
        st.from_idx = from;
        st.to_idx = to;
        st.is_updating = true;
    }
    let (from_dd, to_dd) = {
        let st = state.borrow();
        for (c, b) in &st.kind_chips {
            if *c == category {
                b.add_css_class("on");
            } else {
                b.remove_css_class("on");
            }
        }
        let more = !MAIN_KINDS.contains(&category);
        st.more_label.set_text(if more { kind_name(category) } else { "More" });
        if more {
            st.more_btn.add_css_class("on");
        } else {
            st.more_btn.remove_css_class("on");
        }
        (st.from_dropdown.clone(), st.to_dropdown.clone())
    };
    from_dd.set_model(Some(&gtk::StringList::new(&names)));
    to_dd.set_model(Some(&gtk::StringList::new(&names)));
    from_dd.set_selected(from as u32);
    to_dd.set_selected(to as u32);
    state.borrow_mut().is_updating = false;
    recalculate(state);
    show_chips(state);
}

fn swap(state: &Rc<RefCell<ConverterState>>) {
    let (category, from, to) = {
        let st = state.borrow();
        (st.category, st.from_idx, st.to_idx)
    };
    set_category(state, category, Some((to, from)));
    schedule_record(state);
}

/// Remembers the pair once it has stayed a moment, for Recent.
fn schedule_record(state: &Rc<RefCell<ConverterState>>) {
    if let Some(t) = state.borrow_mut().record_timer.take() {
        t.remove();
    }
    let st = state.clone();
    let id = glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
        st.borrow_mut().record_timer = None;
        let pair = {
            let s = st.borrow();
            match (s.units.get(s.from_idx), s.units.get(s.to_idx)) {
                (Some(f), Some(t)) if f.id != t.id => Some((f.id.to_string(), t.id.to_string())),
                _ => None,
            }
        };
        if let Some(pair) = pair {
            prefs::update(|p| {
                p.recent_units.retain(|r| *r != pair);
                p.recent_units.insert(0, pair);
                p.recent_units.truncate(8);
            });
            show_chips(&st);
        }
    });
    state.borrow_mut().record_timer = Some(id);
}

/// Recent pairs other than the one showing; until there are some, this kind's common ones.
fn show_chips(state: &Rc<RefCell<ConverterState>>) {
    let (chips_box, title, current) = {
        let st = state.borrow();
        let current = st.units.get(st.from_idx).zip(st.units.get(st.to_idx)).map(|(f, t)| (f.id, t.id));
        (st.chips_box.clone(), st.chips_title.clone(), current)
    };
    while let Some(child) = chips_box.first_child() {
        chips_box.remove(&child);
    }
    let recent: Vec<(&'static UnitDefinition, &'static UnitDefinition)> = prefs::get()
        .recent_units
        .iter()
        .filter_map(|(f, t)| Some((find_unit(f)?, find_unit(t)?)))
        .filter(|(f, t)| Some((f.id, t.id)) != current)
        .take(RECENT)
        .collect();
    let pairs: Vec<(&'static UnitDefinition, &'static UnitDefinition)> = if recent.is_empty() {
        title.set_text("Common");
        let category = state.borrow().category;
        let units = units_for_category(category);
        common_pairs(category).iter().filter_map(|&(f, t)| Some((*units.get(f)?, *units.get(t)?))).collect()
    } else {
        title.set_text("Recent");
        recent
    };
    for (from, to) in pairs {
        let b = gtk::Button::with_label(&format!("{} to {}", from.symbol, to.symbol));
        b.add_css_class("converter-chip");
        let st = state.clone();
        b.connect_clicked(move |_| {
            let units = units_for_category(from.category);
            let f = units.iter().position(|u| u.id == from.id).unwrap_or(0);
            let t = units.iter().position(|u| u.id == to.id).unwrap_or(1);
            set_category(&st, from.category, Some((f, t)));
            schedule_record(&st);
        });
        chips_box.append(&b);
    }
    title.set_visible(chips_box.first_child().is_some());
}

/// Unit indices worth a chip in each kind.
fn common_pairs(category: UnitCategory) -> &'static [(usize, usize)] {
    match category {
        UnitCategory::Length => &[(3, 7), (2, 5), (1, 4)],
        UnitCategory::Mass => &[(2, 5), (1, 4)],
        UnitCategory::Temperature => &[(0, 1), (0, 2)],
        UnitCategory::Data => &[(3, 2), (4, 3), (9, 8)],
        UnitCategory::Time => &[(5, 4), (6, 5), (4, 3)],
        UnitCategory::Speed => &[(1, 2), (0, 1)],
        UnitCategory::Volume => &[(1, 9), (0, 5)],
        _ => &[],
    }
}

/// A result as it reads: two decimals, or four significant digits below 1.
fn tidy(v: Decimal, decimals: u32) -> String {
    let r = if v.abs() >= dec!(1) || v.is_zero() { v.round_dp(decimals) } else { v.round_sf(4).unwrap_or(v) };
    r.normalize().to_string().replace('-', "−")
}

fn recalculate(state: &Rc<RefCell<ConverterState>>) {
    let (input_text, from_idx, to_idx, units) = {
        let st = state.borrow();
        (st.from_entry.text().to_string(), st.from_idx, st.to_idx, st.units.clone())
    };
    let (Some(from_unit), Some(to_unit)) = (units.get(from_idx), units.get(to_idx)) else { return };
    let st = state.borrow();
    st.result_unit.set_text(to_unit.symbol);

    let Ok(val) = Decimal::from_str(input_text.trim()) else {
        st.result.set_text("—");
        st.formula_label.set_text("Type a number to convert");
        drop(st);
        state.borrow_mut().exact.clear();
        return;
    };
    match convert(val, from_unit.id, to_unit.id) {
        Ok(result) => {
            st.result.set_text(&tidy(result, 2));
            let exact = result.round_dp(10).normalize().to_string();
            st.result.set_tooltip_text(Some(&exact));
            match convert(dec!(1), from_unit.id, to_unit.id) {
                Ok(ratio) => {
                    st.formula_label.set_text(&format!("1 {} = {} {}", from_unit.symbol, tidy(ratio, 3), to_unit.symbol))
                }
                Err(_) => st.formula_label.set_text(""),
            }
            drop(st);
            state.borrow_mut().exact = exact;
        }
        Err(e) => {
            st.result.set_text("—");
            st.formula_label.set_text(&e.to_string());
            drop(st);
            state.borrow_mut().exact.clear();
        }
    }
}
