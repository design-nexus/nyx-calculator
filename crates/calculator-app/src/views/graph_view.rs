use calculator_core::evaluate_str_with_mode;
use calculator_core::scientific::AngleMode;
use calculator_graph::calculus::{find_extrema, find_roots, integrate};
use calculator_graph::function::{DEFAULT_COLORS, PlottedFunction};
use calculator_graph::sampler::{sample_derivative, sample_function};
use calculator_graph::viewport::Viewport;
use gtk::cairo;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use rust_decimal::prelude::ToPrimitive;
use std::cell::RefCell;
use std::rc::Rc;

/// Functions are named in this order, as on paper.
const NAMES: [char; 10] = ['f', 'g', 'h', 'p', 'q', 'r', 's', 'u', 'v', 'w'];
/// Theme swatches for the curves, in order; the graph crate's colours stand in.
const SWATCHES: [&str; 6] = ["blue", "coral", "green", "amber", "violet", "teal"];
/// The home view spans this many units across; up and down keep the same scale.
const HOME_WIDTH: f64 = 11.0;
const SIDE_WIDTH: i32 = 280;

pub struct GraphView {
    pub root: gtk::Box,
    /// "Copy image", for the panel's header (just its icon in a narrow panel).
    pub copy_button: gtk::Button,
    parts: Parts,
    state: Rc<RefCell<GraphState>>,
}

/// What moves between the wide layout (the functions beside the plot) and the
/// narrow one (above it, the results in a line under it).
struct Parts {
    side_scroll: gtk::ScrolledWindow,
    side: gtk::Box,
    line: gtk::Separator,
    plot: gtk::Overlay,
    headings: [gtk::Label; 2],
    chips_a: gtk::Box,
    chips_b: gtk::Box,
    area_toggle: gtk::ToggleButton,
    copy_label: gtk::Widget,
    narrow: std::cell::Cell<Option<bool>>,
}

struct GraphState {
    viewport: Viewport,
    /// The plot's size when the view was last fitted to it.
    sized: (f64, f64),
    functions: Vec<PlottedFunction>,
    next_id: usize,
    /// The function the area and the results are for.
    selected: usize,
    drawing_area: gtk::DrawingArea,
    func_list_box: gtk::Box,
    hover_coords: Option<(f64, f64)>,
    show_roots: bool,
    show_extrema: bool,
    integral_active: bool,
    integral_bounds: (f64, f64),
    area_label: gtk::Label,
    bounds_row: gtk::Box,
    results: gtk::Grid,
    /// Results in one line under the plot, not a card beside it.
    narrow: bool,
    on_toast: Rc<dyn Fn(&str)>,
}

impl GraphView {
    pub fn new(on_toast: impl Fn(&str) + 'static) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.add_css_class("graph-view");
        root.set_vexpand(true);
        root.set_hexpand(true);

        // The functions and what to show, beside the plot
        let side = gtk::Box::new(gtk::Orientation::Vertical, 6);
        side.add_css_class("graph-side");
        let side_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::External)
            .hexpand(false)
            .build();
        let line = nyx_ui::frame::pane_line(gtk::Orientation::Vertical);

        let functions_heading = heading("Functions");
        side.append(&functions_heading);
        let func_list_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        side.append(&func_list_box);

        let add_btn = gtk::Button::new();
        add_btn.add_css_class("flat");
        add_btn.add_css_class("graph-add");
        let add_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        add_content.append(&gtk::Image::from_icon_name("list-add-symbolic"));
        add_content.append(&gtk::Label::new(Some("Add a function")));
        add_btn.set_child(Some(&add_content));
        add_btn.set_halign(gtk::Align::Start);
        side.append(&add_btn);

        let show_heading = heading("Show");
        side.append(&show_heading);
        let (roots_toggle, _) = chip("Roots", "Mark where each curve crosses zero");
        let (extrema_toggle, _) = chip("Highs and lows", "Mark each curve's highs and lows");
        let (area_toggle, area_label) = chip("Area under f", "Shade and measure the area under the selected function");
        let chips_a = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        chips_a.append(&roots_toggle);
        chips_a.append(&extrema_toggle);
        side.append(&chips_a);
        let chips_b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        chips_b.append(&area_toggle);
        side.append(&chips_b);

        let bounds_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        bounds_row.add_css_class("graph-bounds");
        bounds_row.append(&gtk::Label::new(Some("Area from")));
        let from_entry = bound_entry("0", "Where the area starts");
        bounds_row.append(&from_entry);
        bounds_row.append(&gtk::Label::new(Some("to")));
        let to_entry = bound_entry("π", "Where the area ends");
        bounds_row.append(&to_entry);
        side.append(&bounds_row);

        let results = gtk::Grid::builder().row_spacing(6).column_spacing(10).build();
        results.add_css_class("graph-results");

        // The plot, with zoom controls in its corner
        let drawing_area = gtk::DrawingArea::new();
        drawing_area.set_hexpand(true);
        drawing_area.set_vexpand(true);
        let plot = gtk::Overlay::new();
        plot.add_css_class("graph-plot");
        plot.set_child(Some(&drawing_area));
        let zoom = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        zoom.add_css_class("graph-zoom");
        zoom.set_halign(gtk::Align::End);
        zoom.set_valign(gtk::Align::End);
        let zoom_in = nyx_ui::widgets::icon_button("list-add-symbolic", "Zoom in");
        let reset_btn = nyx_ui::widgets::icon_button("view-refresh-symbolic", "Reset the view");
        zoom.append(&zoom_in);
        zoom.append(&reset_btn);
        plot.add_overlay(&zoom);

        let copy_button = nyx_ui::widgets::labeled_button("edit-copy-symbolic", "Copy image");
        copy_button.add_css_class("flat");
        copy_button.add_css_class("calc-ghost");
        copy_button.set_valign(gtk::Align::Center);
        copy_button.set_tooltip_text(Some("Copy the graph as a picture"));

        let mut f = PlottedFunction::new(0, "sin(x)", DEFAULT_COLORS[0]);
        f.show_derivative = true;
        let state = Rc::new(RefCell::new(GraphState {
            viewport: Viewport::default(),
            sized: (0.0, 0.0),
            functions: vec![f, PlottedFunction::new(1, "x^2/4 - 1", DEFAULT_COLORS[1])],
            next_id: 2,
            selected: 0,
            drawing_area: drawing_area.clone(),
            func_list_box: func_list_box.clone(),
            hover_coords: None,
            show_roots: true,
            show_extrema: true,
            integral_active: true,
            integral_bounds: (0.0, std::f64::consts::PI),
            area_label,
            bounds_row,
            results,
            narrow: false,
            on_toast: Rc::new(on_toast),
        }));
        roots_toggle.set_active(true);
        extrema_toggle.set_active(true);
        area_toggle.set_active(true);

        refresh_function_rows(&state);
        update_results(&state);

        let st_draw = state.clone();
        drawing_area.set_draw_func(move |_, cr, width, height| {
            fit_view(&st_draw, width as f64, height as f64);
            draw_graph(&st_draw, cr, width as f64, height as f64, true);
        });

        // Drag to pan
        let drag = gtk::GestureDrag::new();
        let start_vp = Rc::new(RefCell::new(Viewport::default()));
        let s_vp = start_vp.clone();
        let st_d_start = state.clone();
        drag.connect_drag_begin(move |_, _, _| {
            *s_vp.borrow_mut() = st_d_start.borrow().viewport;
        });
        let st_d_upd = state.clone();
        drag.connect_drag_update(move |_gesture, offset_x, offset_y| {
            let width = st_d_upd.borrow().drawing_area.width() as f64;
            let height = st_d_upd.borrow().drawing_area.height() as f64;
            if width <= 0.0 || height <= 0.0 {
                return;
            }
            let mut base_vp = *start_vp.borrow();
            base_vp.pan(offset_x, offset_y, width, height);
            st_d_upd.borrow_mut().viewport = base_vp;
            redraw(&st_d_upd);
        });
        drawing_area.add_controller(drag);

        // Scroll to zoom about the pointer
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let st_scroll = state.clone();
        scroll.connect_scroll(move |controller, _, dy| {
            let (width, height) = {
                let da = &st_scroll.borrow().drawing_area;
                (da.width() as f64, da.height() as f64)
            };
            if width <= 0.0 || height <= 0.0 {
                return glib::Propagation::Proceed;
            }
            let factor = if dy > 0.0 { 1.15 } else { 0.85 };
            let mouse_pos = controller.current_event().and_then(|ev| ev.position());
            st_scroll.borrow_mut().viewport.zoom(factor, mouse_pos, width, height);
            redraw(&st_scroll);
            glib::Propagation::Stop
        });
        drawing_area.add_controller(scroll);

        let motion = gtk::EventControllerMotion::new();
        let st_motion = state.clone();
        motion.connect_motion(move |_, x, y| {
            st_motion.borrow_mut().hover_coords = Some((x, y));
            st_motion.borrow().drawing_area.queue_draw();
        });
        let st_leave = state.clone();
        motion.connect_leave(move |_| {
            st_leave.borrow_mut().hover_coords = None;
            st_leave.borrow().drawing_area.queue_draw();
        });
        drawing_area.add_controller(motion);

        let st_add = state.clone();
        add_btn.connect_clicked(move |_| {
            {
                let mut st = st_add.borrow_mut();
                let id = st.next_id;
                st.next_id += 1;
                st.functions.push(PlottedFunction::new(id, "cos(x)", DEFAULT_COLORS[id % DEFAULT_COLORS.len()]));
                st.selected = id;
            }
            refresh_function_rows(&st_add);
            redraw(&st_add);
        });

        let st_zoom = state.clone();
        zoom_in.connect_clicked(move |_| {
            let (width, height) = {
                let da = &st_zoom.borrow().drawing_area;
                (da.width() as f64, da.height() as f64)
            };
            st_zoom.borrow_mut().viewport.zoom(0.8, None, width, height);
            redraw(&st_zoom);
        });

        let st_reset = state.clone();
        reset_btn.connect_clicked(move |_| {
            st_reset.borrow_mut().sized = (0.0, 0.0);
            redraw(&st_reset);
        });

        let st_roots = state.clone();
        roots_toggle.connect_toggled(move |btn| {
            st_roots.borrow_mut().show_roots = btn.is_active();
            redraw(&st_roots);
        });
        let st_extrema = state.clone();
        extrema_toggle.connect_toggled(move |btn| {
            st_extrema.borrow_mut().show_extrema = btn.is_active();
            redraw(&st_extrema);
        });
        let st_int = state.clone();
        area_toggle.connect_toggled(move |btn| {
            st_int.borrow_mut().integral_active = btn.is_active();
            redraw(&st_int);
        });

        for (entry, is_end) in [(from_entry, false), (to_entry, true)] {
            let st = state.clone();
            entry.connect_changed(move |e| {
                let Some(v) = evaluate_str_with_mode(e.text().trim(), AngleMode::Radians).ok().and_then(|d| d.to_f64()) else {
                    e.add_css_class("error");
                    return;
                };
                e.remove_css_class("error");
                {
                    let mut s = st.borrow_mut();
                    if is_end {
                        s.integral_bounds.1 = v;
                    } else {
                        s.integral_bounds.0 = v;
                    }
                }
                redraw(&st);
            });
        }

        let st_copy = state.clone();
        copy_button.connect_clicked(move |_| copy_image(&st_copy));

        let copy_label = copy_button.child().and_then(|c| c.last_child()).expect("labelled button");
        let parts = Parts {
            side_scroll,
            side,
            line,
            plot,
            headings: [functions_heading, show_heading],
            chips_a,
            chips_b,
            area_toggle,
            copy_label,
            narrow: std::cell::Cell::new(None),
        };
        let view = Self { root, copy_button, parts, state };
        view.set_narrow(false);
        view
    }

    /// Wide: the functions in a column beside the plot. Narrow: above it, with the
    /// results in a line under it and Copy image as just its icon.
    pub fn set_narrow(&self, narrow: bool) {
        let p = &self.parts;
        if p.narrow.replace(Some(narrow)) == Some(narrow) {
            return;
        }
        let results = self.state.borrow().results.clone();
        for w in [
            p.side_scroll.upcast_ref::<gtk::Widget>(),
            p.side.upcast_ref(),
            p.line.upcast_ref(),
            p.plot.upcast_ref(),
            results.upcast_ref(),
        ] {
            if let Some(parent) = w.parent() {
                if let Some(sw) = parent.downcast_ref::<gtk::ScrolledWindow>() {
                    sw.set_child(None::<&gtk::Widget>);
                } else if let Some(vp) = parent.downcast_ref::<gtk::Viewport>() {
                    vp.set_child(None::<&gtk::Widget>);
                } else if let Some(bx) = parent.downcast_ref::<gtk::Box>() {
                    bx.remove(w);
                }
            }
        }
        if let Some(parent) = p.area_toggle.parent().and_downcast::<gtk::Box>() {
            parent.remove(&p.area_toggle);
        }
        for h in &p.headings {
            h.set_visible(!narrow);
        }
        p.copy_label.set_visible(!narrow);
        p.chips_b.set_visible(!narrow);
        if narrow {
            p.chips_a.append(&p.area_toggle);
            self.root.set_orientation(gtk::Orientation::Vertical);
            p.side.set_size_request(-1, -1);
            p.side.add_css_class("narrow");
            self.root.append(&p.side);
            p.plot.add_css_class("narrow");
            self.root.append(&p.plot);
            results.add_css_class("narrow");
            self.root.append(&results);
        } else {
            p.chips_b.append(&p.area_toggle);
            self.root.set_orientation(gtk::Orientation::Horizontal);
            p.side.set_size_request(SIDE_WIDTH, -1);
            p.side.remove_css_class("narrow");
            p.side.append(&results);
            results.remove_css_class("narrow");
            p.side_scroll.set_child(Some(&p.side));
            self.root.append(&p.side_scroll);
            self.root.append(&p.line);
            p.plot.remove_css_class("narrow");
            self.root.append(&p.plot);
        }
        self.state.borrow_mut().narrow = narrow;
        update_results(&self.state);
    }
}

fn heading(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("graph-heading");
    l.set_xalign(0.0);
    l
}

/// A toggle chip: a check mark while it's on, then its label.
fn chip(label: &str, tip: &str) -> (gtk::ToggleButton, gtk::Label) {
    let toggle = gtk::ToggleButton::new();
    toggle.add_css_class("calc-chip");
    toggle.set_tooltip_text(Some(tip));
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let check = gtk::Image::from_icon_name("object-select-symbolic");
    check.set_visible(false);
    content.append(&check);
    let text = gtk::Label::new(Some(label));
    content.append(&text);
    toggle.set_child(Some(&content));
    toggle.connect_toggled(move |t| check.set_visible(t.is_active()));
    (toggle, text)
}

fn bound_entry(text: &str, tip: &str) -> gtk::Entry {
    let e = gtk::Entry::new();
    e.set_text(text);
    e.set_width_chars(3);
    e.set_max_width_chars(5);
    EntryExt::set_alignment(&e, 0.5);
    e.set_tooltip_text(Some(tip));
    e.add_css_class("graph-bound");
    e
}

fn name(index: usize) -> char {
    NAMES[index % NAMES.len()]
}

/// The curve colour for the function at `index`: a theme swatch.
fn colour(index: usize) -> (f64, f64, f64) {
    let swatches = nyx_theme::swatches();
    let want = SWATCHES[index % SWATCHES.len()];
    swatches
        .iter()
        .find(|(n, _)| n == want)
        .and_then(|(_, c)| gdk::RGBA::parse(c.as_str()).ok())
        .map(|c| (f64::from(c.red()), f64::from(c.green()), f64::from(c.blue())))
        .unwrap_or(DEFAULT_COLORS[index % DEFAULT_COLORS.len()])
}

fn css_rgb((r, g, b): (f64, f64, f64), alpha: f64) -> String {
    format!("rgba({}, {}, {}, {alpha})", (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// Style one widget (not its children) with `css`.
fn style(widget: &impl IsA<gtk::Widget>, css: &str) {
    add_css(widget, &format!("* {{ {css} }}"));
}

/// Style one widget with `css` while it has the `on` class.
fn style_on(widget: &impl IsA<gtk::Widget>, css: &str) {
    add_css(widget, &format!("*.on {{ {css} }}"));
}

fn add_css(widget: &impl IsA<gtk::Widget>, css: &str) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(css);
    #[allow(deprecated)]
    widget.style_context().add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
}

fn redraw(state: &Rc<RefCell<GraphState>>) {
    update_results(state);
    state.borrow().drawing_area.queue_draw();
}

/// Keeps a unit as long across as it is up, whatever shape the plot is, and puts
/// the view home on the first draw and after Reset.
fn fit_view(state: &Rc<RefCell<GraphState>>, width: f64, height: f64) {
    let mut st = state.borrow_mut();
    if st.sized == (width, height) || width <= 0.0 || height <= 0.0 {
        return;
    }
    if st.sized == (0.0, 0.0) {
        st.viewport.x_min = -HOME_WIDTH / 2.0;
        st.viewport.x_max = HOME_WIDTH / 2.0;
        st.viewport.y_min = 0.0;
        st.viewport.y_max = 0.0;
    }
    let units_per_px = st.viewport.width_world() / width;
    let mid = (st.viewport.y_min + st.viewport.y_max) / 2.0;
    st.viewport.y_min = mid - height * units_per_px / 2.0;
    st.viewport.y_max = mid + height * units_per_px / 2.0;
    st.sized = (width, height);
    drop(st);
    // Not while drawing: the results card changes the side's layout.
    let st = state.clone();
    glib::idle_add_local_once(move || update_results(&st));
}

fn refresh_function_rows(state: &Rc<RefCell<GraphState>>) {
    let list_box = state.borrow().func_list_box.clone();
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    let (funcs, selected) = {
        let st = state.borrow();
        (st.functions.clone(), st.selected)
    };
    for (index, func) in funcs.iter().enumerate() {
        let fid = func.id;
        let c = colour(index);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.add_css_class("graph-fn");
        row.set_widget_name(&fid.to_string());
        style_on(&row, &format!("box-shadow: inset 0 0 0 1.5px {};", css_rgb(c, 0.55)));
        if fid == selected {
            row.add_css_class("on");
        }

        // The swatch shows and hides the curve.
        let dot = gtk::Button::new();
        dot.add_css_class("graph-dot");
        dot.set_valign(gtk::Align::Center);
        dot.set_tooltip_text(Some(if func.visible { "Hide this curve" } else { "Show this curve" }));
        style(
            &dot,
            &if func.visible {
                format!("background: {};", css_rgb(c, 1.0))
            } else {
                format!("background: transparent; box-shadow: inset 0 0 0 1.5px {};", css_rgb(c, 1.0))
            },
        );
        let st_v = state.clone();
        dot.connect_clicked(move |_| {
            if let Some(f) = st_v.borrow_mut().functions.iter_mut().find(|f| f.id == fid) {
                f.visible = !f.visible;
            }
            refresh_function_rows(&st_v);
            redraw(&st_v);
        });
        row.append(&dot);

        let n = name(index);
        let prefix = gtk::Label::new(Some(&format!("{n}(x) =")));
        prefix.add_css_class("graph-fn-name");
        row.append(&prefix);

        let entry = gtk::Entry::new();
        entry.set_text(&func.formula);
        entry.add_css_class("graph-fn-entry");
        entry.set_hexpand(true);
        entry.set_width_chars(4);
        if func.error.is_some() {
            entry.add_css_class("error");
        }
        let st_e = state.clone();
        entry.connect_changed(move |ent| {
            let ok = {
                let mut st = st_e.borrow_mut();
                st.functions.iter_mut().find(|f| f.id == fid).map(|f| {
                    f.set_formula(&ent.text());
                    f.error.is_none()
                })
            };
            if ok == Some(false) {
                ent.add_css_class("error");
            } else {
                ent.remove_css_class("error");
            }
            redraw(&st_e);
        });
        let focus = gtk::EventControllerFocus::new();
        let st_f = state.clone();
        focus.connect_enter(move |_| select(&st_f, fid));
        entry.add_controller(focus);
        row.append(&entry);

        let deriv = gtk::Button::with_label(&format!("{n}′"));
        deriv.add_css_class("graph-dvb");
        deriv.set_valign(gtk::Align::Center);
        deriv.set_tooltip_text(Some(&format!("Show the slope of {n} as a dashed curve")));
        style_on(&deriv, &format!("color: {}; background: {};", css_rgb(c, 1.0), css_rgb(c, 0.14)));
        if func.show_derivative {
            deriv.add_css_class("on");
        }
        let st_d = state.clone();
        deriv.connect_clicked(move |_| {
            if let Some(f) = st_d.borrow_mut().functions.iter_mut().find(|f| f.id == fid) {
                f.show_derivative = !f.show_derivative;
            }
            refresh_function_rows(&st_d);
            redraw(&st_d);
        });
        row.append(&deriv);

        // Shown while the pointer is on the card (style.css).
        let del = nyx_ui::widgets::icon_button("window-close-symbolic", &format!("Remove {n}"));
        del.add_css_class("graph-del");
        let st_del = state.clone();
        del.connect_clicked(move |_| {
            {
                let mut st = st_del.borrow_mut();
                st.functions.retain(|f| f.id != fid);
                if st.selected == fid {
                    st.selected = st.functions.first().map_or(0, |f| f.id);
                }
            }
            refresh_function_rows(&st_del);
            redraw(&st_del);
        });
        row.append(&del);

        let click = gtk::GestureClick::new();
        let st_c = state.clone();
        click.connect_pressed(move |_, _, _, _| select(&st_c, fid));
        row.add_controller(click);

        list_box.append(&row);
    }
}

fn select(state: &Rc<RefCell<GraphState>>, fid: usize) {
    if state.borrow().selected == fid {
        return;
    }
    state.borrow_mut().selected = fid;
    // In place: rebuilding the cards would take the focus from the field just chosen.
    let list_box = state.borrow().func_list_box.clone();
    let mut child = list_box.first_child();
    while let Some(row) = child {
        if row.widget_name() == fid.to_string() {
            row.add_css_class("on");
        } else {
            row.remove_css_class("on");
        }
        child = row.next_sibling();
    }
    redraw(state);
}

/// The selected function and its name, if it can be drawn.
fn selected(st: &GraphState) -> Option<(usize, char, PlottedFunction)> {
    let index = st.functions.iter().position(|f| f.id == st.selected)?;
    let f = &st.functions[index];
    (f.visible && f.parsed.is_some()).then(|| (index, name(index), f.clone()))
}

fn number(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    let s = if s == "-0" { "0".to_string() } else { s };
    s.replace('-', "−")
}

/// The Area label's function, the bounds row, and the card of what was found.
fn update_results(state: &Rc<RefCell<GraphState>>) {
    let st = state.borrow();
    let sel = selected(&st);
    let n = sel.as_ref().map_or('f', |s| s.1);
    st.area_label.set_text(&format!("Area under {n}"));
    st.bounds_row.set_sensitive(st.integral_active);

    while let Some(child) = st.results.first_child() {
        st.results.remove(&child);
    }
    let mut rows = 0;
    let narrow = st.narrow;
    let mut add = |label: &str, value: &str| {
        let l = gtk::Label::new(Some(label));
        l.set_xalign(0.0);
        l.set_hexpand(!narrow);
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        l.add_css_class("graph-results-key");
        let v = gtk::Label::new(Some(value));
        v.set_xalign(1.0);
        v.set_ellipsize(gtk::pango::EllipsizeMode::End);
        v.set_max_width_chars(18);
        v.add_css_class("graph-results-value");
        if narrow {
            if rows > 0 {
                let dot = gtk::Label::new(Some("·"));
                dot.add_css_class("graph-results-key");
                st.results.attach(&dot, rows * 3 - 1, 0, 1, 1);
            }
            st.results.attach(&l, rows * 3, 0, 1, 1);
            st.results.attach(&v, rows * 3 + 1, 0, 1, 1);
        } else {
            st.results.attach(&l, 0, rows, 1, 1);
            st.results.attach(&v, 1, rows, 1, 1);
        }
        rows += 1;
    };
    if let Some((_, n, f)) = &sel {
        if st.integral_active {
            let (a, b) = st.integral_bounds;
            let value = integrate(&|x| f.eval(x), a, b, 400).map_or("—".to_string(), |v| format!("{v:.4}").replace('-', "−"));
            let label = if narrow {
                let bounds = |e: &gtk::Widget| e.downcast_ref::<gtk::Entry>().map(|e| e.text().to_string()).unwrap_or_default();
                let mut entries =
                    std::iter::successors(st.bounds_row.first_child(), |w| w.next_sibling()).filter(|w| w.is::<gtk::Entry>());
                let from = entries.next().map(|e| bounds(&e)).unwrap_or_default();
                let to = entries.next().map(|e| bounds(&e)).unwrap_or_default();
                format!("Area under {n}, {from} to {to}")
            } else {
                format!("Area under {n}")
            };
            add(&label, &value);
        }
        if st.show_roots {
            let vp = st.viewport;
            let roots = find_roots(&|x| f.eval(x), vp.x_min, vp.x_max, 300);
            let short = |v: f64| number((v * 100.0).round() / 100.0);
            let mut text = roots.iter().take(4).map(|r| short(*r)).collect::<Vec<_>>().join(", ");
            if roots.len() > 4 {
                text.push_str(", …");
            }
            add(&format!("Roots of {n}"), if roots.is_empty() { "None in view" } else { &text });
        }
    }
    st.results.set_visible(rows > 0);
}

fn draw_graph(state: &Rc<RefCell<GraphState>>, cr: &cairo::Context, width: f64, height: f64, with_hover: bool) {
    let st = state.borrow();
    let vp = st.viewport;
    let hover = if with_hover { st.hover_coords } else { None };

    // The theme's colours: the page's background, its text for the grid, axes and
    // labels, and its popover colour for the readout.
    let p = nyx_theme::palette();
    let rgb = |c: &str| {
        gtk::gdk::RGBA::parse(c).map_or((0.5, 0.5, 0.5), |c| (f64::from(c.red()), f64::from(c.green()), f64::from(c.blue())))
    };
    let (bg_r, bg_g, bg_b) = rgb(&p.bg);
    let (tx_r, tx_g, tx_b) = rgb(&p.text);
    let (pp_r, pp_g, pp_b) = rgb(&nyx_theme::look().popover);

    cr.set_source_rgb(bg_r, bg_g, bg_b);
    let _ = cr.paint();
    cr.select_font_face("Inter", cairo::FontSlant::Normal, cairo::FontWeight::Normal);

    let (step_x, step_y) = vp.grid_steps(width, height);
    let (major_start_x, major_end_x) = ((vp.x_min / step_x).floor() as i64, (vp.x_max / step_x).ceil() as i64);
    let (major_start_y, major_end_y) = ((vp.y_min / step_y).floor() as i64, (vp.y_max / step_y).ceil() as i64);

    // Grid
    cr.set_line_width(1.0);
    cr.set_source_rgba(tx_r, tx_g, tx_b, 0.07);
    for i in major_start_x..=major_end_x {
        let (sx, _) = vp.world_to_screen(i as f64 * step_x, 0.0, width, height);
        cr.move_to(sx.round() + 0.5, 0.0);
        cr.line_to(sx.round() + 0.5, height);
    }
    for i in major_start_y..=major_end_y {
        let (_, sy) = vp.world_to_screen(0.0, i as f64 * step_y, width, height);
        cr.move_to(0.0, sy.round() + 0.5);
        cr.line_to(width, sy.round() + 0.5);
    }
    let _ = cr.stroke();

    // Axes
    let (origin_x, origin_y) = vp.world_to_screen(0.0, 0.0, width, height);
    cr.set_line_width(1.2);
    cr.set_source_rgba(tx_r, tx_g, tx_b, 0.3);
    if (0.0..=height).contains(&origin_y) {
        cr.move_to(0.0, origin_y);
        cr.line_to(width, origin_y);
    }
    if (0.0..=width).contains(&origin_x) {
        cr.move_to(origin_x, 0.0);
        cr.line_to(origin_x, height);
    }
    let _ = cr.stroke();

    // Labels on the axes
    cr.set_source_rgba(tx_r, tx_g, tx_b, 0.5);
    cr.set_font_size(10.0);
    for i in major_start_x..=major_end_x {
        if i == 0 {
            continue;
        }
        let wx = i as f64 * step_x;
        let (sx, _) = vp.world_to_screen(wx, 0.0, width, height);
        let text = number(wx);
        // Labels that would be cut off at the plot's edges are left out.
        let w = cr.text_extents(&text).map(|e| e.x_advance()).unwrap_or(0.0);
        let x = sx - w / 2.0;
        if x < 2.0 || x + w > width - 2.0 {
            continue;
        }
        cr.move_to(x, origin_y.clamp(4.0, height - 18.0) + 16.0);
        let _ = cr.show_text(&text);
    }
    for i in major_start_y..=major_end_y {
        if i == 0 {
            continue;
        }
        let wy = i as f64 * step_y;
        let (_, sy) = vp.world_to_screen(0.0, wy, width, height);
        if sy - 8.0 < 2.0 || sy + 3.0 > height - 2.0 {
            continue;
        }
        let text = number(wy);
        let w = cr.text_extents(&text).map(|e| e.x_advance()).unwrap_or(0.0);
        cr.move_to(origin_x.clamp(w + 12.0, width - 4.0) - w - 8.0, sy + 4.0);
        let _ = cr.show_text(&text);
    }

    let sel = selected(&st);

    // Area under the selected function
    if st.integral_active
        && let Some((index, _, f)) = &sel
    {
        let (a, b) = st.integral_bounds;
        let (r, g, bl) = colour(*index);
        cr.set_source_rgba(r, g, bl, 0.22);
        cr.move_to(vp.world_to_screen(a, 0.0, width, height).0, origin_y);
        let steps = 120;
        for i in 0..=steps {
            let x = a + (b - a) * i as f64 / steps as f64;
            if let Ok(y) = f.eval(x) {
                let (sx, sy) = vp.world_to_screen(x, y, width, height);
                cr.line_to(sx, sy);
            }
        }
        cr.line_to(vp.world_to_screen(b, 0.0, width, height).0, origin_y);
        cr.close_path();
        let _ = cr.fill();
    }

    let stroke_segments = |segments: Vec<Vec<(f64, f64)>>| {
        for seg in segments {
            if let Some(&(x0, y0)) = seg.first() {
                cr.move_to(x0, y0);
                for &(px, py) in &seg[1..] {
                    cr.line_to(px, py);
                }
            }
        }
        let _ = cr.stroke();
    };

    // Curves, their slopes dashed underneath
    for (index, f) in st.functions.iter().enumerate() {
        if !f.visible || f.parsed.is_none() {
            continue;
        }
        let (r, g, b) = colour(index);
        if f.show_derivative {
            cr.set_source_rgba(r, g, b, 0.6);
            cr.set_line_width(1.5);
            cr.set_dash(&[5.0, 4.0], 0.0);
            stroke_segments(sample_derivative(f, &vp, width, height));
            cr.set_dash(&[], 0.0);
        }
        cr.set_source_rgb(r, g, b);
        cr.set_line_width(2.4);
        stroke_segments(sample_function(f, &vp, width, height));
    }

    // Roots as open rings on the axis, highs and lows as solid dots
    for (index, f) in st.functions.iter().enumerate() {
        if !f.visible || f.parsed.is_none() {
            continue;
        }
        let (r, g, b) = colour(index);
        let eval_fn = |x: f64| f.eval(x);
        cr.set_line_width(2.0);
        if st.show_roots {
            for root_x in find_roots(&eval_fn, vp.x_min, vp.x_max, 300) {
                let (sx, sy) = vp.world_to_screen(root_x, 0.0, width, height);
                cr.arc(sx, sy, 4.0, 0.0, std::f64::consts::TAU);
                cr.set_source_rgb(bg_r, bg_g, bg_b);
                let _ = cr.fill_preserve();
                cr.set_source_rgb(r, g, b);
                let _ = cr.stroke();
            }
        }
        if st.show_extrema {
            for (ex_x, ex_y, _) in find_extrema(&eval_fn, vp.x_min, vp.x_max, 300) {
                let (sx, sy) = vp.world_to_screen(ex_x, ex_y, width, height);
                cr.arc(sx, sy, 4.0, 0.0, std::f64::consts::TAU);
                cr.set_source_rgb(r, g, b);
                let _ = cr.fill_preserve();
                cr.set_source_rgb(bg_r, bg_g, bg_b);
                let _ = cr.stroke();
            }
        }
    }

    // The point under the pointer: a dashed drop to the axis and its coordinates,
    // named when it's a high or low.
    let Some((hx, hy)) = hover else { return };
    let (world_hx, _) = vp.screen_to_world(hx, hy, width, height);
    let mut closest: Option<(f64, f64, f64, f64, usize)> = None;
    let mut min_dist = 60.0;
    for (index, f) in st.functions.iter().enumerate() {
        if f.visible
            && let Ok(y) = f.eval(world_hx)
        {
            let (sx, sy) = vp.world_to_screen(world_hx, y, width, height);
            if (sy - hy).abs() < min_dist {
                min_dist = (sy - hy).abs();
                closest = Some((sx, sy, world_hx, y, index));
            }
        }
    }
    let Some((mut sx, mut sy, mut wx, mut wy, index)) = closest else { return };
    let mut kind = "";
    if st.show_extrema {
        let f = &st.functions[index];
        for (ex_x, ex_y, is_max) in find_extrema(&|x: f64| f.eval(x), vp.x_min, vp.x_max, 300) {
            let (ex, ey) = vp.world_to_screen(ex_x, ex_y, width, height);
            if (ex - sx).abs() < 8.0 {
                (sx, sy, wx, wy) = (ex, ey, ex_x, ex_y);
                kind = if is_max { "max " } else { "min " };
            }
        }
    }
    let (r, g, b) = colour(index);
    cr.set_source_rgba(tx_r, tx_g, tx_b, 0.4);
    cr.set_line_width(1.0);
    cr.set_dash(&[3.0, 3.0], 0.0);
    cr.move_to(sx, sy);
    cr.line_to(sx, origin_y.clamp(0.0, height));
    let _ = cr.stroke();
    cr.set_dash(&[], 0.0);
    cr.arc(sx, sy, 4.5, 0.0, std::f64::consts::TAU);
    cr.set_source_rgb(r, g, b);
    let _ = cr.fill_preserve();
    cr.set_source_rgb(bg_r, bg_g, bg_b);
    cr.set_line_width(2.0);
    let _ = cr.stroke();

    cr.select_font_face("Inter", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    cr.set_font_size(11.0);
    let text = format!("{kind}({}, {})", number(wx), number(wy));
    let tw = cr.text_extents(&text).map(|e| e.x_advance()).unwrap_or(80.0);
    let (bw, bh) = (tw + 20.0, 26.0);
    let bx = (sx + 10.0).min(width - bw - 4.0).max(4.0);
    let by = (sy - bh - 8.0).max(4.0);
    draw_rounded_rect(cr, bx, by, bw, bh, 6.0);
    cr.set_source_rgb(pp_r, pp_g, pp_b);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(tx_r, tx_g, tx_b, 0.12);
    cr.set_line_width(1.0);
    let _ = cr.stroke();
    cr.set_source_rgb(tx_r, tx_g, tx_b);
    cr.move_to(bx + 10.0, by + 17.0);
    let _ = cr.show_text(&text);
}

/// Copies the plot as it is now, without the pointer's readout, as a picture.
fn copy_image(state: &Rc<RefCell<GraphState>>) {
    let da = state.borrow().drawing_area.clone();
    let scale = da.scale_factor().max(1);
    let (w, h) = (da.width(), da.height());
    if w <= 0 || h <= 0 {
        return;
    }
    let Ok(mut surface) = cairo::ImageSurface::create(cairo::Format::ARgb32, w * scale, h * scale) else { return };
    if let Ok(cr) = cairo::Context::new(&surface) {
        cr.scale(f64::from(scale), f64::from(scale));
        draw_graph(state, &cr, f64::from(w), f64::from(h), false);
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let Ok(data) = surface.data() else { return };
    let bytes = glib::Bytes::from(&data[..]);
    // Cairo's ARGB32 is premultiplied BGRA in memory on little-endian machines.
    let texture = gdk::MemoryTexture::new(w * scale, h * scale, gdk::MemoryFormat::B8g8r8a8Premultiplied, &bytes, stride);
    da.clipboard().set_texture(&texture);
    (state.borrow().on_toast)("Graph copied as a picture");
}

fn draw_rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let pi = std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -pi / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, pi / 2.0);
    cr.arc(x + r, y + h - r, r, pi / 2.0, pi);
    cr.arc(x + r, y + r, r, pi, 3.0 * pi / 2.0);
    cr.close_path();
}
