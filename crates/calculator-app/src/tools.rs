//! The bigger tools (Unit converter, Sheets, Graph), one at a time in a panel beside
//! the calculator, lined up with its top edge. The panel is a window of its own,
//! transient for the calculator (so Hyprland floats it), with the calculator's
//! Wayland app ID plus the tool (`….Calculator.Graph`). Under Hyprland the panel is
//! placed beside the calculator on the side with room, and follows it when the
//! calculator changes size; elsewhere the compositor places it.

use crate::hypr::{self, Rect};
use crate::views::{ConverterView, GraphView, SheetsView};
use crate::{icons, window};
use calculator_storage::CalculatorDb;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::ffi::{CString, c_char, c_void};
use std::rc::Rc;
use std::time::Duration;

/// Between the calculator and the panel, and kept from the screen's edges.
const GAP: i32 = 10;
const MARGIN: i32 = 20;
/// Narrower than this, Graph puts its functions above the plot and Sheets' Copy all
/// is just its icon.
const WIDE: i32 = 600;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Converter,
    Sheets,
    Graph,
}

impl Tool {
    pub const ALL: [Tool; 3] = [Tool::Converter, Tool::Sheets, Tool::Graph];

    pub fn from_mode(mode: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.mode() == mode)
    }

    pub fn mode(self) -> &'static str {
        match self {
            Tool::Converter => "converter",
            Tool::Sheets => "sheets",
            Tool::Graph => "graph",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Tool::Converter => "Unit converter",
            Tool::Sheets => "Sheets",
            Tool::Graph => "Graph",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tool::Converter => icons::CONVERT,
            Tool::Sheets => icons::SHEET,
            Tool::Graph => icons::PLOT,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Tool::Converter => "Alt 4",
            Tool::Sheets => "Alt 5",
            Tool::Graph => "Alt 6",
        }
    }

    fn class_suffix(self) -> &'static str {
        match self {
            Tool::Converter => "Converter",
            Tool::Sheets => "Sheets",
            Tool::Graph => "Graph",
        }
    }

    /// The width the tool wants and the least it works in.
    fn widths(self) -> (i32, i32) {
        match self {
            Tool::Converter => (400, 400),
            Tool::Sheets => (700, 380),
            Tool::Graph => (740, 390),
        }
    }

    /// Its own height, or `None` to be as tall as the calculator.
    fn height(self) -> Option<i32> {
        match self {
            Tool::Converter => Some(444),
            Tool::Sheets | Tool::Graph => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
}

/// Where a panel goes: its side, its box, and the calculator's x if it has to move
/// over to make room.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub side: Side,
    pub panel: Rect,
    pub calc_x: Option<i32>,
}

/// Beside `calc`, inside `area`: on `prefer`'s side (else the right) if the panel fits
/// there at its `want`ed width, else on the other side. With room on neither, on the
/// side with more, as wide as fits down to `min`, the calculator moving over if even
/// that doesn't fit.
pub fn plan(calc: Rect, area: Rect, want: i32, min: i32, height: i32, prefer: Option<Side>) -> Plan {
    let room = |side| match side {
        Side::Right => area.right() - MARGIN - (calc.right() + GAP),
        Side::Left => calc.x - GAP - (area.x + MARGIN),
    };
    let y = calc.y.min(area.bottom() - MARGIN - height).max(area.y);
    let at = |side, width: i32, calc_x: i32| match side {
        Side::Right => calc_x + calc.w + GAP,
        Side::Left => calc_x - GAP - width,
    };
    let first = prefer.unwrap_or(Side::Right);
    let other = if first == Side::Right { Side::Left } else { Side::Right };
    for side in [first, other] {
        if room(side) >= want {
            return Plan { side, panel: Rect { x: at(side, want, calc.x), y, w: want, h: height }, calc_x: None };
        }
    }
    let side = if room(Side::Left) > room(Side::Right) { Side::Left } else { Side::Right };
    let width = want.min(room(side).max(area.w - 2 * MARGIN - GAP - calc.w)).max(min);
    let calc_x = (room(side) < width).then(|| {
        let x = match side {
            Side::Left => area.x + MARGIN + width + GAP,
            Side::Right => area.right() - MARGIN - width - GAP - calc.w,
        };
        x.min(area.right() - calc.w).max(area.x)
    });
    // Where even that can't fit, it overlaps the calculator rather than leave the screen.
    let x = at(side, width, calc_x.unwrap_or(calc.x)).min(area.right() - width).max(area.x);
    Plan { side, panel: Rect { x, y, w: width, h: height }, calc_x }
}

/// What a panel holds on to.
enum View {
    Converter(#[allow(dead_code)] ConverterView),
    Sheets(SheetsView),
    Graph(GraphView),
}

struct Panel {
    tool: Tool,
    window: gtk::Window,
    frame_root: gtk::Box,
    view: View,
    size: Cell<(i32, i32)>,
}

impl Panel {
    fn class(&self) -> String {
        format!("{}.{}", crate::app_id(), self.tool.class_suffix())
    }

    fn set_size(&self, w: i32, h: i32) {
        self.size.set((w, h));
        self.frame_root.set_size_request(w, h);
        match &self.view {
            View::Graph(g) => g.set_narrow(w < WIDE),
            View::Sheets(s) => s.set_narrow(w < WIDE),
            View::Converter(_) => {}
        }
    }
}

thread_local! {
    static PANELS: RefCell<Vec<Rc<Panel>>> = const { RefCell::new(Vec::new()) };
    static OPEN: Cell<Option<Tool>> = const { Cell::new(None) };
    static SIDE: Cell<Option<Side>> = const { Cell::new(None) };
    static DB: RefCell<Option<CalculatorDb>> = const { RefCell::new(None) };
}

pub fn open_tool() -> Option<Tool> {
    OPEN.with(Cell::get)
}

fn panel(tool: Tool) -> Option<Rc<Panel>> {
    PANELS.with(|p| p.borrow().iter().find(|p| p.tool == tool).cloned())
}

/// Shows `tool` beside the calculator, in place of the one open.
pub fn open(tool: Tool) {
    let Some(calc) = window::window() else { return };
    if !calc.is_visible() {
        window::show();
        // The calculator is placed once it's mapped; the panel goes beside it after.
        glib::timeout_add_local_once(Duration::from_millis(300), move || open(tool));
        return;
    }
    if open_tool() == Some(tool) {
        if let Some(p) = panel(tool) {
            p.window.present();
        }
        return;
    }
    hide_open();
    let p = panel(tool).unwrap_or_else(|| {
        let p = Rc::new(build(tool, &calc));
        PANELS.with(|ps| ps.borrow_mut().push(p.clone()));
        p
    });

    let (want, min) = tool.widths();
    let calc_h = window::size().1;
    let height = tool.height().unwrap_or(calc_h);
    match place(&p, want, min, height, None) {
        Some(plan) => {
            p.set_size(plan.panel.w, plan.panel.h);
            SIDE.with(|s| s.set(Some(plan.side)));
        }
        None => p.set_size(want, height),
    }
    p.window.present();
    OPEN.with(|o| o.set(Some(tool)));
    window::show_tool(Some(tool));
    if hypr::available() {
        settle(p, 0);
    }
}

/// Once Hyprland has mapped the panel: drop the first-map rule and check where it landed.
fn settle(p: Rc<Panel>, tries: u32) {
    glib::timeout_add_local_once(Duration::from_millis(60), move || {
        if open_tool() != Some(p.tool) {
            return;
        }
        // Mapped, and at its own size: it can first map a little larger, and Hyprland
        // then shrinks it about its middle, off the spot the rule gave it.
        let (w, h) = p.size.get();
        let settled = hypr::client(&p.class()).is_some_and(|c| (c.rect.w, c.rect.h) == (w, h));
        if !settled && tries < 30 {
            settle(p, tries + 1);
            return;
        }
        hypr::eval(&format!(r#"hl.window_rule({{ name = "{}", enabled = false }})"#, rule_name()));
        follow_now();
    });
}

/// Puts away the open tool's panel.
pub fn close() {
    if hide_open() {
        recenter();
    }
}

/// Hides the open panel, if any; true when one was open.
fn hide_open() -> bool {
    let Some(tool) = OPEN.with(|o| o.take()) else { return false };
    if let Some(p) = panel(tool) {
        if let View::Sheets(s) = &p.view {
            s.save_now();
        }
        p.window.set_visible(false);
    }
    window::show_tool(None);
    true
}

/// With the panel put away, the calculator goes back to the middle of its screen.
fn recenter() {
    if !hypr::available() {
        return;
    }
    if let Some(calc) = hypr::client(&crate::app_id()) {
        hypr::dispatch(&format!(r#"hl.dsp.window.center({{ window = "{}" }})"#, calc.address));
    }
}

fn rule_name() -> String {
    format!("{}-tool-panel", crate::app_id().replace('.', "-"))
}

/// Works out where `p` goes beside the calculator and, before it maps, gives
/// Hyprland a rule that opens it there; moves the calculator over if it must.
fn place(p: &Panel, want: i32, min: i32, height: i32, prefer: Option<Side>) -> Option<Plan> {
    if !hypr::available() {
        return None;
    }
    let calc = hypr::client(&crate::app_id())?;
    let mon = hypr::monitor(calc.monitor)?;
    let plan = plan(calc.rect, mon.work, want, min, height, prefer);
    if let Some(x) = plan.calc_x {
        hypr::move_to(&calc.address, x, calc.rect.y);
    }
    let class = format!("^{}$", hypr::class_pattern(&p.class()));
    let (x, y) = (plan.panel.x - mon.rect.x, plan.panel.y - mon.rect.y);
    let (w, h) = (plan.panel.w, plan.panel.h);
    hypr::eval(&format!(
        r#"hl.window_rule({{ name = "{}", enabled = true, match = {{ class = "{class}" }}, move = {{ {x}, {y} }}, size = {{ {w}, {h} }} }})"#,
        rule_name()
    ));
    Some(plan)
}

/// After the calculator changes size, the panel follows: same side if it still fits,
/// as tall as the calculator, lined up with its top edge.
pub fn follow() {
    if open_tool().is_none() {
        return;
    }
    // Once GTK has committed the calculator's new size and Hyprland has resized it.
    glib::timeout_add_local_once(Duration::from_millis(200), follow_now);
}

fn follow_now() {
    let Some(tool) = open_tool() else { return };
    let Some(p) = panel(tool) else { return };
    if !hypr::available() {
        return;
    }
    let (Some(calc), Some(me)) = (hypr::client(&crate::app_id()), hypr::client(&p.class())) else { return };
    let Some(mon) = hypr::monitor(calc.monitor) else { return };
    let (want, min) = tool.widths();
    let height = tool.height().unwrap_or(calc.rect.h);
    let plan = plan(calc.rect, mon.work, want, min, height, SIDE.with(Cell::get));
    SIDE.with(|s| s.set(Some(plan.side)));
    if let Some(x) = plan.calc_x {
        hypr::move_to(&calc.address, x, calc.rect.y);
    }
    let resized = p.size.get() != (plan.panel.w, plan.panel.h);
    if resized {
        p.set_size(plan.panel.w, plan.panel.h);
    }
    let target = plan.panel;
    if !resized && (me.rect.x, me.rect.y) == (target.x, target.y) {
        return;
    }
    // Hyprland resizes a floating window about its middle; move it once that's done.
    let address = me.address;
    let delay = if resized { 150 } else { 0 };
    glib::timeout_add_local_once(Duration::from_millis(delay), move || hypr::move_to(&address, target.x, target.y));
}

fn db() -> CalculatorDb {
    DB.with(|d| {
        d.borrow_mut()
            .get_or_insert_with(|| CalculatorDb::open_default().unwrap_or_else(|_| CalculatorDb::open_in_memory().unwrap()))
            .clone()
    })
}

fn build(tool: Tool, calc: &gtk::ApplicationWindow) -> Panel {
    // A plain window: a GtkApplicationWindow would take the calculator's app ID.
    let window = gtk::Window::builder().resizable(false).transient_for(calc).build();
    window.add_css_class("calculator-window");
    window.add_css_class("calc-panel");
    window.add_css_class(&format!("calc-tool-{}", tool.mode()));
    window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    window.set_hide_on_close(true);
    nyx_ui::chrome::set_title(&window, "Calculator", tool.title());
    // Again on map: GTK creates the Wayland toplevel only then, and an ID set before
    // it may be dropped. Both come before the first frame, when the class counts.
    let app_id = format!("{}.{}", crate::app_id(), tool.class_suffix());
    let id = app_id.clone();
    window.connect_realize(move |w| set_wayland_app_id(w, &id));
    window.connect_map(move |w| set_wayland_app_id(w, &app_id));

    // The panel's bar: the tool's icon and name, what it picks from, its actions, Close.
    let bar = gtk::CenterBox::new();
    bar.add_css_class("app-header");
    bar.add_css_class("calc-bar");
    let start = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let image = gtk::Image::from_icon_name(tool.icon());
    image.add_css_class("app-icon");
    start.append(&image);
    let name = gtk::Label::new(Some(tool.title()));
    name.add_css_class("app-title");
    start.append(&name);
    let end = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    bar.set_start_widget(Some(&start));
    bar.set_end_widget(Some(&end));
    let topbar = gtk::WindowHandle::new();
    topbar.set_child(Some(&bar));

    let frame = nyx_ui::frame::AppFrame::new(&topbar);
    let overlay = frame.install(&window);
    // A window that can't be resized takes its content's natural size; this one takes
    // the size it's given (or the least its content needs), so long text ellipsizes.
    let fit = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_width(false)
        .propagate_natural_height(false)
        .build();
    overlay.set_child(None::<&gtk::Widget>);
    fit.set_child(Some(&frame.root));
    overlay.set_child(Some(&fit));
    let toast = {
        let overlay = overlay.downgrade();
        move |msg: &str| {
            if let Some(o) = overlay.upgrade() {
                nyx_ui::toast::show(&o, msg);
            }
        }
    };

    let view = match tool {
        Tool::Converter => {
            let v = ConverterView::new(toast);
            frame.set_page(&v.root);
            View::Converter(v)
        }
        Tool::Sheets => {
            let v = SheetsView::new(db(), toast);
            v.header_start.set_margin_start(12);
            start.append(&v.header_start);
            end.append(&v.header_end);
            frame.set_flush(true);
            frame.set_page(&v.root);
            View::Sheets(v)
        }
        Tool::Graph => {
            let v = GraphView::new(toast);
            end.append(&v.copy_button);
            frame.set_flush(true);
            frame.set_page(&v.root);
            View::Graph(v)
        }
    };
    let close_btn = nyx_ui::widgets::bar_button("window-close-symbolic", "Close (Esc)");
    close_btn.update_property(&[gtk::accessible::Property::Label(&format!("Close {}", tool.title()))]);
    close_btn.connect_clicked(|_| close());
    end.append(&close_btn);

    nyx_ui::keys::install(
        &window,
        nyx_ui::keys::Common {
            settings: Some(crate::settings_dialog::open),
            shortcuts: Some(window::show_shortcuts),
            close: false,
            palette: None,
        },
    );
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, mods| {
        if key == gdk::Key::Escape && mods.is_empty() {
            close();
            return glib::Propagation::Stop;
        }
        if mods.contains(gdk::ModifierType::CONTROL_MASK) && matches!(key, gdk::Key::n | gdk::Key::N) && tool == Tool::Sheets {
            if let Some(p) = panel(Tool::Sheets)
                && let View::Sheets(s) = &p.view
            {
                s.new_sheet();
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);
    let alt = gtk::EventControllerKey::new();
    alt.set_propagation_phase(gtk::PropagationPhase::Capture);
    alt.connect_key_pressed(|_, key, _, mods| {
        if !mods.contains(gdk::ModifierType::ALT_MASK) {
            return glib::Propagation::Proceed;
        }
        let mode = match key {
            gdk::Key::_1 => "standard",
            gdk::Key::_2 => "scientific",
            gdk::Key::_3 => "programmer",
            gdk::Key::_4 => "converter",
            gdk::Key::_5 => "sheets",
            gdk::Key::_6 => "graph",
            _ => return glib::Propagation::Proceed,
        };
        window::switch_mode(mode);
        glib::Propagation::Stop
    });
    window.add_controller(alt);

    window.connect_close_request(move |_| {
        if open_tool() == Some(tool) {
            close();
        }
        glib::Propagation::Stop
    });

    Panel { tool, window, frame_root: frame.root.clone(), view, size: Cell::new((0, 0)) }
}

unsafe extern "C" {
    fn gdk_wayland_toplevel_set_application_id(toplevel: *mut c_void, application_id: *const c_char);
}

fn set_wayland_app_id(window: &impl IsA<gtk::Native>, id: &str) {
    let Some(surface) = window.surface() else { return };
    if !surface.type_().name().contains("Wayland") {
        return;
    }
    let Ok(c_id) = CString::new(id) else { return };
    use glib::translate::ToGlibPtr;
    let raw: *mut gdk::ffi::GdkSurface = surface.to_glib_none().0;
    // SAFETY: a realized Wayland toplevel's surface and a NUL-terminated string.
    unsafe {
        gdk_wayland_toplevel_set_application_id(raw as *mut c_void, c_id.as_ptr());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: Rect = Rect { x: 0, y: 0, w: 1280, h: 800 };
    const NARROW: Rect = Rect { x: 0, y: 0, w: 800, h: 800 };

    #[test]
    fn opens_on_the_right_when_it_fits() {
        let calc = Rect { x: 85, y: 100, w: 360, h: 600 };
        let p = plan(calc, WIDE, 740, 390, 600, None);
        assert_eq!(p, Plan { side: Side::Right, panel: Rect { x: 455, y: 100, w: 740, h: 600 }, calc_x: None });
    }

    #[test]
    fn opens_on_the_left_when_only_the_left_has_room() {
        let calc = Rect { x: 820, y: 100, w: 360, h: 600 };
        let p = plan(calc, WIDE, 740, 390, 600, None);
        assert_eq!((p.side, p.panel.x, p.calc_x), (Side::Left, 70, None));
    }

    #[test]
    fn narrow_screen_uses_the_room_on_the_left() {
        let calc = Rect { x: 420, y: 100, w: 360, h: 600 };
        let p = plan(calc, NARROW, 740, 390, 600, None);
        assert_eq!(p, Plan { side: Side::Left, panel: Rect { x: 20, y: 100, w: 390, h: 600 }, calc_x: None });
    }

    #[test]
    fn calculator_moves_over_when_neither_side_has_room() {
        let calc = Rect { x: 460, y: 100, w: 360, h: 600 };
        let p = plan(calc, WIDE, 740, 390, 600, None);
        assert_eq!(p, Plan { side: Side::Right, panel: Rect { x: 520, y: 100, w: 740, h: 600 }, calc_x: Some(150) });
    }

    #[test]
    fn keeps_its_side_while_it_fits() {
        let calc = Rect { x: 460, y: 100, w: 360, h: 600 };
        let p = plan(calc, WIDE, 400, 360, 420, Some(Side::Left));
        assert_eq!((p.side, p.panel.x), (Side::Left, 50));
    }

    #[test]
    fn overlaps_rather_than_leaving_the_screen() {
        let calc = Rect { x: 150, y: 110, w: 500, h: 580 };
        let p = plan(calc, NARROW, 400, 400, 444, None);
        assert_eq!((p.panel.x, p.calc_x), (400, Some(0)));
    }

    #[test]
    fn stays_on_screen_when_the_calculator_sits_low() {
        let calc = Rect { x: 85, y: 500, w: 360, h: 280 };
        let p = plan(calc, WIDE, 400, 360, 420, None);
        assert_eq!(p.panel.y, 360);
    }
}
