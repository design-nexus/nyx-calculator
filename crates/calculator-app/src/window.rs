use crate::display::DisplayView;
use crate::history_sidebar::{self, HistorySidebar};
use crate::keypad::{KeyAction, ProgrammerKeypad, ScientificKeypad, StandardKeypad};
use crate::keys::install_calculator_keys;
use crate::tools::{self, Tool};
use crate::{hypr, icons, prefs, settings_dialog, theme};
use calculator_core::programmer::ProgrammerState;
use calculator_core::scientific::AngleMode;
use calculator_core::{CalcError, FormatOptions, MemoryRegister, evaluate_str_with_mode, format_decimal, preview_str_with_mode};
use calculator_storage::CalculatorDb;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use nyx_ui::chrome;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::cell::RefCell;
use std::rc::Rc;
use std::str::FromStr;

/// History's panel and the line before it.
const HISTORY_EXTRA: i32 = history_sidebar::WIDTH + 13;

/// Each keypad's window: Standard small, Scientific and Programmer with room for
/// six columns. History adds its width.
fn keypad_size(mode: &str) -> (i32, i32) {
    match mode {
        "scientific" => (500, 580),
        "programmer" => (580, 660),
        _ => (360, 600),
    }
}

/// The keypads that switch inside the calculator, with their menu icon and key.
const KEYPADS: &[(&str, &str, &str, &str)] = &[
    ("standard", "Standard", "accessories-calculator-symbolic", "Alt 1"),
    ("scientific", "Scientific", icons::FX, "Alt 2"),
    ("programmer", "Programmer", icons::BINARY, "Alt 3"),
];

pub(crate) struct Ui {
    window: gtk::ApplicationWindow,
    overlay: gtk::Overlay,
    frame_root: gtk::Box,
    display: DisplayView,
    keypad_stack: gtk::Stack,
    programmer_keypad: ProgrammerKeypad,
    mem: MemoryRow,
    sidebar: Rc<RefCell<HistorySidebar>>,
    history_box: gtk::Box,
    mode_btn: gtk::Button,
    mode_label: gtk::Label,
    tool_btn: gtk::Button,
    hist_btn: gtk::Button,
    db: CalculatorDb,
    memory: MemoryRegister,
    toast: gtk::Label,
    toast_timer: Option<glib::SourceId>,

    // Calculation state
    mode: String,
    angle_mode: AngleMode,
    programmer: ProgrammerState,
    input_buffer: String,
    tape_text: String,
    is_result_shown: bool,
    error_state: Option<String>,
}

thread_local! {
    static UI: RefCell<Option<Rc<RefCell<Ui>>>> = const { RefCell::new(None) };
}

pub fn ui() -> Option<Rc<RefCell<Ui>>> {
    UI.with(|u| u.borrow().clone())
}

pub fn window() -> Option<gtk::ApplicationWindow> {
    ui().map(|u| u.borrow().window.clone())
}

pub fn overlay() -> Option<gtk::Overlay> {
    ui().map(|u| u.borrow().overlay.clone())
}

/// Shows the calculator, and `section`'s tool beside it.
pub fn present(app: &gtk::Application, section: Option<&str>) {
    let tool = section.and_then(Tool::from_mode);
    if ui().is_some() {
        show();
        if let Some(tool) = tool {
            tools::open(tool);
        }
        return;
    }
    theme::install();
    build(app);

    // Headless snapshot test check
    if let Some(out) = std::env::var_os("CALCULATOR_SNAPSHOT") {
        snapshot_and_quit(app, std::path::PathBuf::from(out));
        return;
    }

    show();
    let start = std::env::var("CALCULATOR_MODE").unwrap_or_else(|_| prefs::get().calculator_mode);
    let tool = tool.or_else(|| Tool::from_mode(&start));
    // Beside the calculator once Hyprland has placed it.
    glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
        run_test_hooks();
        if let Some(tool) = tool {
            tools::open(tool);
        }
    });
}

/// Shows the calculator where it was, or floating in the middle the first time.
pub fn show() {
    let Some(w) = window() else { return };
    let first = !w.is_visible();
    w.present();
    if first {
        ensure_floating();
    }
}

/// The calculator's size as it asks for it now (its keypad's, and History's).
pub fn size() -> (i32, i32) {
    ui().map(|u| u.borrow().frame_root.size_request()).unwrap_or((360, 600))
}

/// The open tool's icon in the bar, highlighted; a click puts the tool away.
pub fn show_tool(tool: Option<Tool>) {
    let Some(ui) = ui() else { return };
    let ui = ui.borrow();
    ui.tool_btn.set_visible(tool.is_some());
    if let Some(tool) = tool {
        ui.tool_btn.set_icon_name(tool.icon());
        ui.tool_btn.update_property(&[gtk::accessible::Property::Label(&format!("Hide {}", tool.title()))]);
    }
}

/// For screenshots of test copies: NYX_CALCULATOR_TEST_INPUT types its keys (`m` is
/// MS, `h` shows History), and NYX_CALCULATOR_TEST_MENU=1 opens the mode menu.
fn run_test_hooks() {
    for c in std::env::var("NYX_CALCULATOR_TEST_INPUT").unwrap_or_default().chars() {
        if c == 'h' {
            toggle_history();
            continue;
        }
        if c == 'x' {
            handle_action(KeyAction::SelectRadix(calculator_core::programmer::Radix::Hex));
            continue;
        }
        handle_action(match c {
            '0'..='9' => KeyAction::Digit(c),
            '.' => KeyAction::Dot,
            '+' => KeyAction::OpAdd,
            '-' => KeyAction::OpSub,
            '*' => KeyAction::OpMul,
            '/' => KeyAction::OpDiv,
            '=' => KeyAction::Equals,
            'm' => KeyAction::MemStore,
            'c' => KeyAction::Clear,
            '|' => KeyAction::BitOr,
            _ => continue,
        });
    }
    if std::env::var_os("NYX_CALCULATOR_TEST_MENU").is_some()
        && let Some(ui) = ui()
    {
        let button = ui.borrow().mode_btn.clone();
        open_mode_menu(&button);
    }
    // NYX_CALCULATOR_TEST_THEN: modes to switch to, `h` (History) or `close` (the tool), a second apart.
    let then = std::env::var("NYX_CALCULATOR_TEST_THEN").unwrap_or_default();
    for (i, step) in then.split(',').filter(|s| !s.is_empty()).map(String::from).enumerate() {
        glib::timeout_add_local_once(std::time::Duration::from_millis(1500 + 1000 * i as u64), move || match step.as_str() {
            "h" => toggle_history(),
            "close" => tools::close(),
            mode => switch_mode(mode),
        });
    }
}

/// Quits once the calculator is closed (its tool panel goes with it).
fn quit_if_idle() {
    glib::idle_add_local_once(|| {
        let calculator_open = window().is_some_and(|w| w.is_visible());
        if !calculator_open && let Some(app) = gio::Application::default() {
            app.quit();
        }
    });
}

fn build(app: &gtk::Application) {
    let window = gtk::ApplicationWindow::builder().application(app).title("Calculator").resizable(false).build();
    window.add_css_class("calculator-window");
    // Closing hides it first, so the open tool is put away (its sheet saved) before quitting.
    window.set_hide_on_close(true);
    window.connect_hide(|_| {
        tools::close();
        quit_if_idle();
    });

    window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    chrome::set_title(&window, "Calculator", "Standard");

    // The one bar: the app's icon and the mode menu, then History and Settings. Built
    // like nyx_ui::chrome::app_header but without window controls: the calculator
    // floats at its own size, and at 360 px they would crowd the bar.
    let bar = gtk::CenterBox::new();
    bar.add_css_class("app-header");
    bar.add_css_class("calc-bar");
    let bar_start = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let app_icon = gtk::Image::from_icon_name(icons::CALC);
    app_icon.add_css_class("app-icon");
    bar_start.append(&app_icon);
    let bar_end = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    bar.set_start_widget(Some(&bar_start));
    bar.set_end_widget(Some(&bar_end));
    let topbar = gtk::WindowHandle::new();
    topbar.set_child(Some(&bar));

    let mode_btn = gtk::Button::new();
    mode_btn.add_css_class("calc-mode");
    mode_btn.set_valign(gtk::Align::Center);
    mode_btn.set_tooltip_text(Some("Keypads and tools (Alt+1 to Alt+6)"));
    let mode_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let mode_label = gtk::Label::new(Some("Standard"));
    mode_content.append(&mode_label);
    let chevron = gtk::Image::from_icon_name("pan-down-symbolic");
    chevron.add_css_class("calc-mode-chevron");
    mode_content.append(&chevron);
    mode_btn.set_child(Some(&mode_content));
    mode_btn.connect_clicked(open_mode_menu);
    bar_start.append(&mode_btn);

    let tool_btn = nyx_ui::widgets::bar_button(icons::PLOT, "Hide the tool (Esc)");
    tool_btn.add_css_class("toggled");
    tool_btn.set_visible(false);
    tool_btn.connect_clicked(|_| tools::close());
    bar_end.append(&tool_btn);
    let hist_btn = nyx_ui::widgets::bar_button("document-open-recent-symbolic", "History (Ctrl+H)");
    hist_btn.update_property(&[gtk::accessible::Property::Label("History")]);
    bar_end.append(&hist_btn);
    bar_end.append(&chrome::settings_button(settings_dialog::open));

    let db = CalculatorDb::open_default().unwrap_or_else(|_| CalculatorDb::open_in_memory().unwrap());

    // The calculator: display, memory row and keypad
    let calc_view = gtk::Box::new(gtk::Orientation::Vertical, 6);
    calc_view.add_css_class("calc-view");

    let display = DisplayView::new();
    calc_view.append(&display.root);

    let mem = MemoryRow::new();
    calc_view.append(&mem.root);

    let keypad_stack = gtk::Stack::new();
    keypad_stack.set_hhomogeneous(false);
    keypad_stack.set_vhomogeneous(false);
    keypad_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    keypad_stack.add_named(&StandardKeypad::new(handle_action).root, Some("standard"));
    keypad_stack.add_named(&ScientificKeypad::new(handle_action).root, Some("scientific"));
    let programmer_keypad = ProgrammerKeypad::new(handle_action);
    keypad_stack.add_named(&programmer_keypad.root, Some("programmer"));
    calc_view.append(&keypad_stack);

    // Its size comes from the frame's size request, never from what it shows: the
    // scroller asks for only the least its keys need.
    let calc_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .hexpand(true)
        .vexpand(true)
        .child(&calc_view)
        .build();

    let sidebar = HistorySidebar::new(db.clone(), |formula| restore_calculation(&formula), toast);
    let history_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    history_box.append(&nyx_ui::frame::pane_line(gtk::Orientation::Vertical));
    history_box.append(&sidebar.borrow().root);
    let history_open = prefs::get().history_open;
    history_box.set_visible(history_open);
    if history_open {
        hist_btn.add_css_class("toggled");
    }

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    content.append(&calc_scroller);
    content.append(&history_box);

    let frame = nyx_ui::frame::AppFrame::new(&topbar);
    frame.set_flush(true);
    frame.set_page(&content);
    let overlay = frame.install(&window);

    let toast_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    toast_box.add_css_class("calc-toast-box");
    toast_box.set_halign(gtk::Align::Center);
    toast_box.set_valign(gtk::Align::End);
    toast_box.set_visible(false);
    let toast_lbl = gtk::Label::new(None);
    toast_lbl.add_css_class("calc-toast");
    toast_box.append(&toast_lbl);
    overlay.add_overlay(&toast_box);

    nyx_ui::keys::install(
        &window,
        nyx_ui::keys::Common {
            settings: Some(settings_dialog::open),
            shortcuts: Some(show_shortcuts),
            close: true,
            palette: None,
        },
    );
    install_calculator_keys(&window, handle_action, copy_result, paste_to_expression, toggle_history, switch_mode);

    hist_btn.connect_clicked(|_| toggle_history());

    let state = Rc::new(RefCell::new(Ui {
        window,
        overlay,
        frame_root: frame.root.clone(),
        display,
        keypad_stack,
        programmer_keypad,
        mem,
        sidebar,
        history_box,
        mode_btn,
        mode_label,
        tool_btn,
        hist_btn,
        db,
        memory: MemoryRegister::new(),
        toast: toast_lbl,
        toast_timer: None,
        mode: "standard".into(),
        angle_mode: if prefs::get().radians { AngleMode::Radians } else { AngleMode::Degrees },
        programmer: ProgrammerState::new(),
        input_buffer: String::new(),
        tape_text: String::new(),
        is_result_shown: false,
        error_state: None,
    }));

    UI.with(|u| *u.borrow_mut() = Some(state));

    apply_angle_preference();
    let start = std::env::var("CALCULATOR_MODE").unwrap_or_else(|_| prefs::get().calculator_mode);
    let keypad = if KEYPADS.iter().any(|k| k.0 == start) { start } else { "standard".into() };
    set_keypad(&keypad);
}

/// The memory row: what's stored (or that nothing is), MR and MC only when there's
/// something to recall or clear, then M+, M− and MS. In Scientific the Deg/Rad
/// switch leads it and "Memory is empty" moves beside M+.
struct MemoryRow {
    root: gtk::Box,
    angle: gtk::Box,
    deg: gtk::ToggleButton,
    rad: gtk::ToggleButton,
    tag: gtk::Box,
    tag_value: gtk::Label,
    empty: gtk::Label,
    recall: gtk::Button,
    clear: gtk::Button,
    spacer: gtk::Box,
}

impl MemoryRow {
    fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        root.add_css_class("calc-mem");

        let angle = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        angle.add_css_class("segmented");
        angle.add_css_class("calc-angle");
        angle.set_valign(gtk::Align::Center);
        angle.set_margin_end(8);
        let deg = gtk::ToggleButton::with_label("Deg");
        let rad = gtk::ToggleButton::with_label("Rad");
        rad.set_group(Some(&deg));
        for (b, mode, tip) in [(&deg, AngleMode::Degrees, "Angles in degrees"), (&rad, AngleMode::Radians, "Angles in radians")] {
            b.add_css_class("segment");
            b.set_focusable(false);
            b.set_tooltip_text(Some(tip));
            b.connect_toggled(move |b| {
                // Not while the calculator itself is showing the unit it set.
                let free = ui().is_some_and(|u| u.try_borrow_mut().is_ok());
                if b.is_active() && free {
                    handle_action(KeyAction::SetAngle(mode));
                }
            });
            angle.append(b);
        }
        root.append(&angle);

        let tag = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        tag.add_css_class("calc-mem-tag");
        tag.set_valign(gtk::Align::Center);
        tag.append(&gtk::Label::new(Some("M")));
        let tag_value = gtk::Label::new(None);
        tag_value.add_css_class("calc-mem-value");
        tag_value.set_ellipsize(gtk::pango::EllipsizeMode::End);
        tag.append(&tag_value);
        root.append(&tag);

        let empty = gtk::Label::new(Some("Memory is empty"));
        empty.add_css_class("calc-mem-empty");
        empty.set_xalign(0.0);
        empty.set_margin_start(4);
        root.append(&empty);

        let button = |label: &str, tip: &str, action: KeyAction| {
            let b = gtk::Button::with_label(label);
            b.add_css_class("calc-mem-btn");
            b.set_tooltip_text(Some(tip));
            b.set_focusable(false);
            b.connect_clicked(move |_| handle_action(action));
            b
        };
        let recall = button("MR", "Recall memory (Ctrl+R)", KeyAction::MemRecall);
        let clear = button("MC", "Clear memory (Ctrl+L)", KeyAction::MemClear);
        root.append(&recall);
        root.append(&clear);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        root.append(&spacer);
        root.append(&button("M+", "Add to memory", KeyAction::MemAdd));
        root.append(&button("M−", "Subtract from memory", KeyAction::MemSub));
        root.append(&button("MS", "Store in memory (Ctrl+M)", KeyAction::MemStore));

        let row = Self { root, angle, deg, rad, tag, tag_value, empty, recall, clear, spacer };
        row.set_value(None);
        row
    }

    fn set_value(&self, value: Option<&str>) {
        self.tag.set_visible(value.is_some());
        self.tag_value.set_text(value.unwrap_or(""));
        self.recall.set_visible(value.is_some());
        self.clear.set_visible(value.is_some());
        self.empty.set_visible(value.is_none());
    }

    fn set_angle_visible(&self, visible: bool) {
        self.angle.set_visible(visible);
        let after = if visible { &self.spacer } else { &self.tag };
        self.root.reorder_child_after(&self.empty, Some(after));
        self.empty.set_margin_end(if visible { 8 } else { 0 });
    }

    /// Shows `mode` in the switch without switching again.
    fn show_angle(&self, mode: AngleMode) {
        let b = if mode == AngleMode::Degrees { &self.deg } else { &self.rad };
        if !b.is_active() {
            b.set_active(true);
        }
    }
}

/// The mode menu: the keypads, which switch here, then the tools, which open apart.
fn open_mode_menu(anchor: &gtk::Button) {
    let current = ui().map(|u| u.borrow().mode.clone()).unwrap_or_default();
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    popover.add_css_class("menu-popover");
    popover.add_css_class("calc-mode-menu");
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_halign(gtk::Align::Start);
    popover.set_parent(anchor);
    // A test copy never has the keyboard, so a grabbing popup would close at once.
    if std::env::var_os("NYX_CALCULATOR_TEST_MENU").is_some() {
        popover.set_autohide(false);
    }

    let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
    list.add_css_class("menu-list");
    list.set_size_request(236, -1);
    let heading = |text: &str| {
        let l = gtk::Label::new(Some(text));
        l.set_xalign(0.0);
        l.add_css_class("menu-heading");
        l
    };
    let row = |icon: &str, label: &str, key: &str, checked: bool, mode: &'static str| {
        let button = gtk::Button::new();
        button.add_css_class("flat");
        button.add_css_class("menu-item");
        if checked {
            button.add_css_class("checked");
        }
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let image = gtk::Image::from_icon_name(if checked { "object-select-symbolic" } else { icon });
        image.add_css_class("menu-icon");
        content.append(&image);
        let text = gtk::Label::new(Some(label));
        text.set_xalign(0.0);
        text.set_hexpand(true);
        content.append(&text);
        let k = gtk::Label::new(Some(key));
        k.add_css_class("menu-key");
        content.append(&k);
        button.set_child(Some(&content));
        let p = popover.clone();
        button.connect_clicked(move |_| {
            p.popdown();
            switch_mode(mode);
        });
        button
    };

    list.append(&heading("Keypad"));
    for (mode, label, icon, key) in KEYPADS {
        list.append(&row(icon, label, key, *mode == current, mode));
    }
    list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    list.append(&heading("Opens beside the calculator"));
    let open = tools::open_tool();
    for tool in Tool::ALL {
        list.append(&row(tool.icon(), tool.title(), tool.key(), open == Some(tool), tool.mode()));
    }
    popover.set_child(Some(&list));

    anchor.add_css_class("open");
    let a = anchor.downgrade();
    popover.connect_closed(move |pop| {
        if let Some(a) = a.upgrade() {
            a.remove_css_class("open");
        }
        let pop = pop.clone();
        glib::idle_add_local_once(move || {
            if pop.parent().is_some() {
                pop.unparent();
            }
        });
    });
    popover.popup();
}

/// Alt+1 to Alt+3 and the menu's keypads switch the calculator; the tools open beside it.
pub fn switch_mode(mode: &str) {
    if let Some(tool) = Tool::from_mode(mode) {
        tools::open(tool);
        return;
    }
    show();
    set_keypad(mode);
    prefs::update(|p| p.calculator_mode = mode.to_string());
    tools::follow();
}

/// The window's size for its keypad, and History beside it if it's open.
fn apply_size(ui: &Ui) {
    let (w, h) = keypad_size(&ui.mode);
    let extra = if ui.history_box.is_visible() { HISTORY_EXTRA } else { 0 };
    let (w, h) = (w + extra, h);
    if ui.frame_root.size_request() == (w, h) {
        return;
    }
    keep_corner(w, h);
    ui.frame_root.set_size_request(w, h);
}

/// Hyprland resizes a floating window about its middle, which would move the keys
/// under the pointer (and can push the window off screen). Moved ahead by half the
/// change, it ends with its top-left corner where it was, kept on screen.
fn keep_corner(w: i32, h: i32) {
    if !hypr::available() {
        return;
    }
    let Some(calc) = hypr::client(&crate::app_id()) else { return };
    let Some(mon) = hypr::monitor(calc.monitor) else { return };
    let area = mon.work;
    let x = calc.rect.x.min(area.right() - w).max(area.x);
    let y = calc.rect.y.min(area.bottom() - h).max(area.y);
    let (dw, dh) = (w - calc.rect.w, h - calc.rect.h);
    hypr::move_to(&calc.address, x + dw / 2, y + dh / 2);
}

fn set_keypad(mode: &str) {
    let Some(ui_rc) = ui() else { return };
    let mut ui = ui_rc.borrow_mut();
    let Some((mode, title, _, _)) = KEYPADS.iter().find(|k| k.0 == mode).or(KEYPADS.first()) else { return };
    ui.mode = mode.to_string();
    ui.keypad_stack.set_visible_child_name(mode);
    apply_size(&ui);
    // The programmer pad's rows are the value; above them, only the sum being typed.
    let programmer = *mode == "programmer";
    ui.display.readout_label.set_visible(!programmer);
    ui.display.root.set_vexpand(!programmer);
    ui.mem.root.set_visible(!programmer);
    ui.mem.set_angle_visible(*mode == "scientific");
    show_sum(&ui);
    ui.mode_label.set_text(title);
    ui.mode_btn.update_property(&[gtk::accessible::Property::Label(&format!("Keypad: {title}"))]);
    chrome::set_title(&ui.window, "Calculator", title);

    if *mode == "programmer" {
        if let Ok(num) = get_current_number(&ui)
            && let Some(u) = num.to_u64()
        {
            ui.programmer.set_value(u);
        }
        ui.programmer_keypad.update_state(&ui.programmer);
    }
}

/// Floats and centres the calculator under Hyprland (its size is its own).
/// NYX_CALCULATOR_TEST_AT=x,y puts a test copy there on its monitor instead.
fn ensure_floating() {
    if !hypr::available() {
        return;
    }
    let class = crate::app_id();
    std::thread::spawn(move || {
        let mut calc = None;
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(40));
            calc = hypr::client(&class);
            if calc.is_some() {
                break;
            }
        }
        let Some(calc) = calc else { return };
        let addr = calc.address;
        hypr::dispatch(&format!(r#"hl.dsp.window.float({{ window = "{addr}", action = "enable" }})"#));
        let at = std::env::var("NYX_CALCULATOR_TEST_AT").ok().and_then(|s| {
            let (x, y) = s.split_once(',')?;
            Some((x.trim().parse::<i32>().ok()?, y.trim().parse::<i32>().ok()?))
        });
        match at.zip(hypr::monitor(calc.monitor)) {
            Some(((x, y), mon)) => hypr::move_to(&addr, mon.rect.x + x, mon.rect.y + y),
            None => hypr::dispatch(&format!(r#"hl.dsp.window.center({{ window = "{addr}" }})"#)),
        }
    });
}

pub fn handle_action(action: KeyAction) {
    let Some(ui_rc) = ui() else { return };
    let mut ui = ui_rc.borrow_mut();

    if ui.error_state.is_some() && action != KeyAction::Clear {
        ui.error_state = None;
        ui.display.set_error(None);
    }

    match action {
        KeyAction::Digit(c) => {
            if ui.is_result_shown {
                ui.input_buffer.clear();
                ui.is_result_shown = false;
            }
            ui.input_buffer.push(c);
            update_readout(&mut ui);
        }
        KeyAction::Dot => {
            if ui.is_result_shown {
                ui.input_buffer.clear();
                ui.is_result_shown = false;
            }
            if ui.input_buffer.is_empty() {
                ui.input_buffer.push_str("0.");
            } else {
                let last_segment = ui
                    .input_buffer
                    .rsplit(|ch| ['+', '-', '*', '/', '^', '%', '(', ' ', '×', '÷', '−'].contains(&ch))
                    .next()
                    .unwrap_or("");
                if !last_segment.contains('.') {
                    ui.input_buffer.push('.');
                }
            }
            update_readout(&mut ui);
        }
        KeyAction::OpAdd => append_binary_op(&mut ui, " + "),
        KeyAction::OpSub => append_binary_op(&mut ui, " − "),
        KeyAction::OpMul => append_binary_op(&mut ui, " × "),
        KeyAction::OpDiv => append_binary_op(&mut ui, " ÷ "),
        KeyAction::OpPow => append_binary_op(&mut ui, " ^ "),
        KeyAction::OpMod => append_binary_op(&mut ui, " % "),
        KeyAction::BitAnd => append_binary_op(&mut ui, " & "),
        KeyAction::BitOr => append_binary_op(&mut ui, " | "),
        KeyAction::BitXor => append_binary_op(&mut ui, " ^ "),
        KeyAction::Shl => append_binary_op(&mut ui, " << "),
        KeyAction::Shr => append_binary_op(&mut ui, " >> "),
        KeyAction::BitNot => {
            if ui.input_buffer.is_empty() {
                ui.input_buffer.push_str("~0");
            } else {
                ui.input_buffer = format!("~({})", ui.input_buffer);
            }
            execute_calculation(&mut ui);
        }
        KeyAction::Percent => {
            if !ui.input_buffer.is_empty() {
                ui.input_buffer.push('%');
                ui.is_result_shown = false;
                update_readout(&mut ui);
            }
        }
        KeyAction::LParen => {
            if ui.is_result_shown {
                ui.input_buffer.clear();
                ui.is_result_shown = false;
            }
            ui.input_buffer.push('(');
            update_readout(&mut ui);
        }
        KeyAction::RParen => {
            ui.input_buffer.push(')');
            ui.is_result_shown = false;
            update_readout(&mut ui);
        }
        KeyAction::Negate => {
            if let Ok(val) = evaluate_str_with_mode(&ui.input_buffer, ui.angle_mode) {
                let neg = -val;
                ui.input_buffer = neg.to_string();
                update_readout(&mut ui);
            }
        }
        KeyAction::Square => {
            wrap_current_or_evaluate(&mut ui, "sqr");
        }
        KeyAction::Sqrt => {
            wrap_current_or_evaluate(&mut ui, "sqrt");
        }
        KeyAction::Reciprocal => {
            wrap_current_or_evaluate(&mut ui, "recip");
        }
        KeyAction::Sci(func) => {
            let name = match func {
                calculator_core::scientific::SciFunc::Sin => "sin",
                calculator_core::scientific::SciFunc::Cos => "cos",
                calculator_core::scientific::SciFunc::Tan => "tan",
                calculator_core::scientific::SciFunc::Asin => "asin",
                calculator_core::scientific::SciFunc::Acos => "acos",
                calculator_core::scientific::SciFunc::Atan => "atan",
                calculator_core::scientific::SciFunc::Sinh => "sinh",
                calculator_core::scientific::SciFunc::Cosh => "cosh",
                calculator_core::scientific::SciFunc::Tanh => "tanh",
                calculator_core::scientific::SciFunc::Ln => "ln",
                calculator_core::scientific::SciFunc::Log10 => "log",
                calculator_core::scientific::SciFunc::Log2 => "log2",
                calculator_core::scientific::SciFunc::Exp => "exp",
                calculator_core::scientific::SciFunc::Sqrt => "sqrt",
                calculator_core::scientific::SciFunc::Cbrt => "cbrt",
                calculator_core::scientific::SciFunc::Sqr => "sqr",
                calculator_core::scientific::SciFunc::Fact => "fact",
                calculator_core::scientific::SciFunc::Abs => "abs",
            };
            wrap_current_or_evaluate(&mut ui, name);
        }
        KeyAction::ConstPi => {
            if ui.is_result_shown {
                ui.input_buffer.clear();
                ui.is_result_shown = false;
            }
            ui.input_buffer.push('π');
            update_readout(&mut ui);
        }
        KeyAction::ConstE => {
            if ui.is_result_shown {
                ui.input_buffer.clear();
                ui.is_result_shown = false;
            }
            ui.input_buffer.push('e');
            update_readout(&mut ui);
        }
        KeyAction::ConstTau => {
            if ui.is_result_shown {
                ui.input_buffer.clear();
                ui.is_result_shown = false;
            }
            ui.input_buffer.push('τ');
            update_readout(&mut ui);
        }
        KeyAction::SetAngle(mode) => {
            if ui.angle_mode != mode {
                ui.angle_mode = mode;
                prefs::update(|p| p.radians = mode == AngleMode::Radians);
                update_readout(&mut ui);
            }
        }
        KeyAction::ExpNotation => {
            ui.input_buffer.push('e');
            ui.is_result_shown = false;
            update_readout(&mut ui);
        }
        // Programmer Mode Actions
        KeyAction::ToggleBit(bit_idx) => {
            ui.programmer.toggle_bit(bit_idx);
            ui.input_buffer = ui.programmer.value.to_string();
            ui.programmer_keypad.update_state(&ui.programmer);
            update_readout(&mut ui);
        }
        KeyAction::SelectRadix(radix) => {
            ui.programmer.active_radix = radix;
            ui.programmer_keypad.update_state(&ui.programmer);
        }
        KeyAction::SetWordWidth(width) => {
            ui.programmer.set_width(width);
            ui.programmer_keypad.update_state(&ui.programmer);
            update_readout(&mut ui);
        }
        KeyAction::SetSignedness(signedness) => {
            ui.programmer.signedness = signedness;
            ui.programmer_keypad.update_state(&ui.programmer);
        }
        KeyAction::Backspace => {
            if ui.is_result_shown {
                ui.input_buffer.clear();
                ui.is_result_shown = false;
            } else {
                let trimmed = ui.input_buffer.trim_end();
                if trimmed.len() < ui.input_buffer.len() {
                    ui.input_buffer = trimmed.to_string();
                } else {
                    ui.input_buffer.pop();
                }
            }
            update_readout(&mut ui);
        }
        KeyAction::Clear => {
            ui.input_buffer.clear();
            ui.tape_text.clear();
            ui.is_result_shown = false;
            ui.error_state = None;
            ui.programmer.set_value(0);
            ui.programmer_keypad.update_state(&ui.programmer);
            ui.display.set_tape("");
            ui.display.set_preview(None);
            ui.display.set_readout("0");
            ui.display.set_error(None);
        }
        KeyAction::ClearEntry => {
            if let Some(pos) = ui.input_buffer.rfind(|ch| ['+', '-', '*', '/', '^', '×', '÷', '−', '&', '|'].contains(&ch)) {
                ui.input_buffer.truncate(pos + 1);
            } else {
                ui.input_buffer.clear();
            }
            ui.is_result_shown = false;
            update_readout(&mut ui);
        }
        KeyAction::Equals => {
            execute_calculation(&mut ui);
        }
        KeyAction::MemClear => {
            ui.memory.clear();
            show_memory(&ui);
            toast("Memory cleared");
        }
        KeyAction::MemRecall => {
            if ui.memory.is_active() {
                let val = ui.memory.recall();
                if ui.is_result_shown {
                    ui.input_buffer.clear();
                    ui.is_result_shown = false;
                }
                ui.input_buffer.push_str(&val.to_string());
                update_readout(&mut ui);
            }
        }
        KeyAction::MemStore => {
            if let Ok(val) = get_current_number(&ui) {
                ui.memory.store(val);
                show_memory(&ui);
            }
        }
        KeyAction::MemAdd => {
            if let Ok(val) = get_current_number(&ui)
                && ui.memory.add(val).is_ok()
            {
                show_memory(&ui);
            }
        }
        KeyAction::MemSub => {
            if let Ok(val) = get_current_number(&ui)
                && ui.memory.sub(val).is_ok()
            {
                show_memory(&ui);
            }
        }
    }
    show_sum(&ui);
}

fn show_memory(ui: &Ui) {
    let value = ui.memory.is_active().then(|| format_decimal(ui.memory.recall(), &get_format_options()));
    ui.mem.set_value(value.as_deref());
}

/// The line over the readout: the last sum, or in Programmer (which has no readout)
/// the sum being typed, its operators spelled as on the keys.
fn show_sum(ui: &Ui) {
    if ui.mode != "programmer" {
        ui.display.set_tape(&ui.tape_text);
        return;
    }
    let sum = if ui.is_result_shown || ui.input_buffer.is_empty() { &ui.tape_text } else { &ui.input_buffer };
    let mut text = sum.clone();
    for (op, name) in
        [(" & ", " AND "), (" | ", " OR "), (" ^ ", " XOR "), (" << ", " LSH "), (" >> ", " RSH "), (" % ", " mod ")]
    {
        text = text.replace(op, name);
    }
    ui.display.set_tape(&text.replace("~(", "NOT ("));
    ui.display.set_preview(None);
}

fn append_binary_op(ui: &mut Ui, op: &str) {
    if ui.input_buffer.is_empty() {
        ui.input_buffer.push('0');
    }
    ui.is_result_shown = false;

    let trimmed = ui.input_buffer.trim_end();
    if trimmed.ends_with(['+', '-', '*', '/', '^', '×', '÷', '−', '&', '|']) {
        let mut s = trimmed.to_string();
        s.pop();
        ui.input_buffer = s;
    }
    ui.input_buffer.push_str(op);
    update_readout(ui);
}

fn wrap_current_or_evaluate(ui: &mut Ui, func_name: &str) {
    if ui.input_buffer.is_empty() {
        ui.input_buffer.push('0');
    }
    ui.input_buffer = format!("{func_name}({})", ui.input_buffer);
    execute_calculation(ui);
}

fn update_readout(ui: &mut Ui) {
    let opts = get_format_options();
    if ui.input_buffer.is_empty() {
        ui.display.set_readout("0");
        ui.display.set_preview(None);
        if ui.mode == "programmer" {
            ui.programmer.set_value(0);
            ui.programmer_keypad.update_state(&ui.programmer);
        }
        return;
    }

    if let Some(preview_val) = preview_str_with_mode(&ui.input_buffer, ui.angle_mode) {
        let formatted_preview = format_decimal(preview_val, &opts);
        ui.display.set_preview(Some(&formatted_preview));
        if ui.mode == "programmer"
            && let Some(u) = preview_val.to_u64()
        {
            ui.programmer.set_value(u);
            ui.programmer_keypad.update_state(&ui.programmer);
        }
    } else {
        ui.display.set_preview(None);
    }

    ui.display.set_readout(&ui.input_buffer);
}

fn execute_calculation(ui: &mut Ui) {
    if ui.input_buffer.trim().is_empty() {
        return;
    }
    let opts = get_format_options();
    match evaluate_str_with_mode(&ui.input_buffer, ui.angle_mode) {
        Ok(res) => {
            let formatted = format_decimal(res, &opts);
            let raw_res = res.to_string();
            let expr = ui.input_buffer.clone();

            ui.tape_text = expr.clone();
            ui.display.set_tape(&ui.tape_text);
            ui.display.set_readout(&formatted);
            ui.display.set_preview(None);
            ui.display.set_error(None);

            if ui.mode == "programmer"
                && let Some(u) = res.to_u64()
            {
                ui.programmer.set_value(u);
                ui.programmer_keypad.update_state(&ui.programmer);
            }

            if prefs::get().history_enabled {
                let _ = ui.db.save_calculation(&expr, &formatted, &raw_res, &ui.mode);
                ui.sidebar.borrow().refresh();
            }

            ui.input_buffer = raw_res;
            ui.is_result_shown = true;
        }
        Err(err) => {
            let msg = match err {
                CalcError::DivisionByZero => "Cannot divide by zero".to_string(),
                CalcError::InvalidDomain(m) => m,
                CalcError::Overflow => "Numerical overflow".to_string(),
                CalcError::Underflow => "Numerical underflow".to_string(),
                CalcError::UnbalancedParentheses => "Unbalanced parentheses".to_string(),
                CalcError::SyntaxError(m) => m,
                CalcError::EmptyExpression => "Empty expression".to_string(),
            };
            ui.error_state = Some(msg.clone());
            ui.display.set_error(Some(&msg));
        }
    }
}

fn get_current_number(ui: &Ui) -> Result<Decimal, CalcError> {
    if let Ok(res) = evaluate_str_with_mode(&ui.input_buffer, ui.angle_mode) {
        Ok(res)
    } else {
        Decimal::from_str(&ui.input_buffer).map_err(|_| CalcError::SyntaxError("Invalid number".into()))
    }
}

pub fn restore_calculation(val: &str) {
    let Some(ui_rc) = ui() else { return };
    let mut ui = ui_rc.borrow_mut();
    ui.input_buffer = val.to_string();
    ui.is_result_shown = false;
    update_readout(&mut ui);
}

/// History slides out beside the keypad: the window grows by its width, the
/// calculator itself stays as it is.
pub fn toggle_history() {
    let Some(ui_rc) = ui() else { return };
    let ui = ui_rc.borrow();
    let open = !ui.history_box.is_visible();
    ui.history_box.set_visible(open);
    apply_size(&ui);
    if open {
        ui.hist_btn.add_css_class("toggled");
        ui.sidebar.borrow().refresh();
    } else {
        ui.hist_btn.remove_css_class("toggled");
    }
    prefs::update(|p| p.history_open = open);
    tools::follow();
}

pub fn copy_result() {
    let Some(ui_rc) = ui() else { return };
    let ui = ui_rc.borrow();
    let text = ui.display.readout_label.text().to_string();
    if let Some(display) = gdk::Display::default() {
        display.clipboard().set_text(&text);
        toast("Copied to clipboard");
    }
}

pub fn paste_to_expression() {
    let Some(display) = gdk::Display::default() else { return };
    display.clipboard().read_text_async(None::<&gio::Cancellable>, move |res| {
        if let Ok(Some(text)) = res {
            let clean = text.trim();
            if !clean.is_empty()
                && let Some(ui_rc) = ui()
            {
                let mut ui = ui_rc.borrow_mut();
                if ui.is_result_shown {
                    ui.input_buffer.clear();
                    ui.is_result_shown = false;
                }
                ui.input_buffer.push_str(clean);
                update_readout(&mut ui);
            }
        }
    });
}

pub fn apply_angle_preference() {
    if let Some(state) = ui() {
        let mut ui = state.borrow_mut();
        ui.angle_mode = if prefs::get().radians { AngleMode::Radians } else { AngleMode::Degrees };
        ui.mem.show_angle(ui.angle_mode);
        update_readout(&mut ui);
    }
}

pub fn refresh_display() {
    let Some(ui_rc) = ui() else { return };
    let mut ui = ui_rc.borrow_mut();
    update_readout(&mut ui);
    show_memory(&ui);
}

pub fn toast(msg: &str) {
    let Some(ui_rc) = ui() else { return };
    let mut ui = ui_rc.borrow_mut();
    if let Some(t) = ui.toast_timer.take() {
        t.remove();
    }
    ui.toast.set_text(msg);
    if let Some(parent) = ui.toast.parent() {
        parent.set_visible(true);
    }
    let weak = Rc::downgrade(&ui_rc);
    ui.toast_timer = Some(glib::timeout_add_local_once(std::time::Duration::from_millis(2500), move || {
        if let Some(u) = weak.upgrade() {
            let mut u = u.borrow_mut();
            u.toast_timer = None;
            if let Some(parent) = u.toast.parent() {
                parent.set_visible(false);
            }
        }
    }));
}

fn get_format_options() -> FormatOptions {
    let p = prefs::get();
    FormatOptions {
        thousands_separator: p.thousands_separator,
        precision: if p.precision == 0 { None } else { Some(p.precision) },
    }
}

pub fn show_shortcuts() {
    let (dialog, card) = nyx_ui::widgets::dialog("Keyboard Shortcuts", 480);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.add_css_class("group-list");

    let shortcuts: &[(&[&str], &str)] = &[
        (&["Alt", "1"], "Standard keypad"),
        (&["Alt", "2"], "Scientific keypad"),
        (&["Alt", "3"], "Programmer keypad"),
        (&["Alt", "4"], "Unit converter, beside the calculator"),
        (&["Alt", "5"], "Sheets, beside the calculator"),
        (&["Alt", "6"], "Graph, beside the calculator"),
        (&["Esc"], "Put the open tool away"),
        (&["0–9", "A–F", "."], "Enter numbers / hex digits"),
        (&["+", "−", "×", "÷"], "Basic arithmetic"),
        (&["&", "|", "~", "^"], "Bitwise AND, OR, NOT, XOR"),
        (&["Enter", "="], "Calculate result"),
        (&["Backspace"], "Delete character"),
        (&["Esc", "C"], "Clear expression (Esc: when no tool is open)"),
        (&["Ctrl", "H"], "Show or hide history"),
        (&["Ctrl", "C"], "Copy result"),
        (&["Ctrl", "V"], "Paste formula"),
        (&["Ctrl", "M"], "Memory store (MS)"),
        (&["Ctrl", "R"], "Memory recall (MR)"),
        (&["Ctrl", "L"], "Memory clear (MC)"),
        (&["Ctrl", ","], "Open settings"),
        (&["F1"], "Show shortcuts"),
    ];

    for (keys, what) in shortcuts {
        list.append(&nyx_ui::widgets::row(what, "", Some(nyx_ui::widgets::key_caps(keys).upcast_ref())));
    }

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .child(&list)
        .max_content_height(420)
        .propagate_natural_height(true)
        .build();
    card.append(&scroll);

    let close = gtk::Button::with_label("Close");
    close.set_halign(gtk::Align::End);
    let d = dialog.clone();
    close.connect_clicked(move |_| d.close());
    card.append(&close);
    dialog.present();
}

fn snapshot_and_quit(app: &gtk::Application, out: std::path::PathBuf) {
    let Some(ui) = ui() else { return };
    let window = ui.borrow().window.clone();
    window.set_opacity(0.01);
    window.present();

    let app = app.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
        if let Some(child) = window.child() {
            let paintable = gtk::WidgetPaintable::new(Some(&child));
            let (w, h) = (child.width(), child.height());
            let snapshot = gtk::Snapshot::new();
            snapshot.append_color(&gdk::RGBA::BLACK, &gtk::graphene::Rect::new(0.0, 0.0, w as f32, h as f32));
            paintable.snapshot(&snapshot, w as f64, h as f64);
            if let (Some(node), Some(renderer)) = (snapshot.to_node(), window.renderer()) {
                let texture = renderer.render_texture(node, None);
                if texture.save_to_png(&out).is_ok() {
                    println!("Rendered headless snapshot: {}", out.display());
                }
            }
        }
        app.quit();
    });
}
