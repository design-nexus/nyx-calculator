use calculator_graph::calculus::{find_extrema, find_roots, integrate};
use calculator_graph::function::{DEFAULT_COLORS, PlottedFunction};
use calculator_graph::sampler::{sample_derivative, sample_function};
use calculator_graph::viewport::Viewport;
use gtk::cairo;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub struct GraphView {
    pub root: gtk::Box,
    #[allow(dead_code)]
    state: Rc<RefCell<GraphState>>,
}

struct GraphState {
    viewport: Viewport,
    functions: Vec<PlottedFunction>,
    next_id: usize,
    drawing_area: gtk::DrawingArea,
    func_list_box: gtk::Box,
    hover_coords: Option<(f64, f64)>, // Screen coordinates
    show_roots: bool,
    show_extrema: bool,
    integral_active: bool,
    integral_bounds: (f64, f64),
    integral_label: gtk::Label,
    on_toast: Rc<dyn Fn(&str)>,
}

impl GraphView {
    pub fn new(on_toast: impl Fn(&str) + 'static) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.add_css_class("graph-view");
        root.set_vexpand(true);
        root.set_hexpand(true);

        let toast_rc: Rc<dyn Fn(&str)> = Rc::new(on_toast);

        // 1. Left sidebar for functions and analysis tools
        // A fixed-width sidebar; only the plot takes spare width (STYLE.md's sidebar rule).
        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 8);
        sidebar.add_css_class("graph-sidebar");
        sidebar.set_size_request(280, -1);
        sidebar.set_hexpand(false);

        // Sidebar Header
        let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let title_lbl = gtk::Label::new(Some("Functions"));
        title_lbl.add_css_class("graph-sidebar-title");
        title_lbl.set_hexpand(true);
        title_lbl.set_halign(gtk::Align::Start);
        header_box.append(&title_lbl);

        let add_btn = nyx_ui::widgets::bar_button("list-add-symbolic", "Add function");
        header_box.append(&add_btn);
        let reset_btn = nyx_ui::widgets::bar_button("zoom-fit-best-symbolic", "Reset view");
        header_box.append(&reset_btn);
        sidebar.append(&header_box);

        // Scrolled functions container: as tall as its functions, the tools right under it
        let scrolled_funcs = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::External)
            .hexpand(false)
            .propagate_natural_height(true)
            .max_content_height(320)
            .build();

        let func_list_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        func_list_box.add_css_class("graph-func-list");
        scrolled_funcs.set_child(Some(&func_list_box));
        sidebar.append(&scrolled_funcs);

        // Analysis tools section
        let tools_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        tools_box.add_css_class("graph-tools-box");

        let tools_title = gtk::Label::new(Some("Analysis Tools"));
        tools_title.add_css_class("converter-card-title");
        tools_title.set_halign(gtk::Align::Start);
        tools_box.append(&tools_title);

        let tools_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);

        let roots_toggle = gtk::ToggleButton::with_label("Roots");
        roots_toggle.add_css_class("graph-tool-toggle");
        roots_toggle.set_tooltip_text(Some("Detect and highlight roots f(x) = 0"));
        tools_row.append(&roots_toggle);

        let extrema_toggle = gtk::ToggleButton::with_label("Extrema");
        extrema_toggle.add_css_class("graph-tool-toggle");
        extrema_toggle.set_tooltip_text(Some("Highlight local minima and maxima"));
        tools_row.append(&extrema_toggle);

        let int_toggle = gtk::ToggleButton::with_label("∫ Area");
        int_toggle.add_css_class("graph-tool-toggle");
        int_toggle.set_tooltip_text(Some("Numerical integration under curve"));
        tools_row.append(&int_toggle);

        tools_box.append(&tools_row);

        let integral_label = gtk::Label::new(None);
        integral_label.add_css_class("graph-integral-label");
        integral_label.set_halign(gtk::Align::Start);
        tools_box.append(&integral_label);

        sidebar.append(&tools_box);
        root.append(&sidebar);

        // 2. Right Canvas Area
        let canvas_frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        canvas_frame.add_css_class("graph-canvas-frame");
        // The plot is a rounded card beside the sidebar, inset like the other modes' content.
        canvas_frame.set_overflow(gtk::Overflow::Hidden);
        canvas_frame.set_hexpand(true);
        canvas_frame.set_vexpand(true);

        let drawing_area = gtk::DrawingArea::new();
        drawing_area.set_hexpand(true);
        drawing_area.set_vexpand(true);
        canvas_frame.append(&drawing_area);
        root.append(&canvas_frame);

        // Initial default functions
        let initial_funcs = vec![
            PlottedFunction::new(0, "sin(x)", DEFAULT_COLORS[0]),
            PlottedFunction::new(1, "0.1 * x^2 - 3", DEFAULT_COLORS[1]),
        ];

        let state = Rc::new(RefCell::new(GraphState {
            viewport: Viewport::default(),
            functions: initial_funcs,
            next_id: 2,
            drawing_area: drawing_area.clone(),
            func_list_box: func_list_box.clone(),
            hover_coords: None,
            show_roots: false,
            show_extrema: false,
            integral_active: false,
            integral_bounds: (0.0, std::f64::consts::PI),
            integral_label: integral_label.clone(),
            on_toast: toast_rc,
        }));

        // Populate sidebar UI
        refresh_function_rows(&state);

        // Cairo Draw function
        let st_draw = state.clone();
        drawing_area.set_draw_func(move |_, cr, width, height| {
            draw_graph(&st_draw, cr, width as f64, height as f64);
        });

        // 3. Gestures & Controllers
        // Drag gesture for real-time panning
        let drag = gtk::GestureDrag::new();
        let start_vp = Rc::new(RefCell::new(Viewport::default()));

        let s_vp = start_vp.clone();
        let st_d_start = state.clone();
        drag.connect_drag_begin(move |_, _, _| {
            *s_vp.borrow_mut() = st_d_start.borrow().viewport;
        });

        let s_vp2 = start_vp.clone();
        let st_d_upd = state.clone();
        drag.connect_drag_update(move |_gesture, offset_x, offset_y| {
            let width = st_d_upd.borrow().drawing_area.width() as f64;
            let height = st_d_upd.borrow().drawing_area.height() as f64;
            if width <= 0.0 || height <= 0.0 {
                return;
            }

            let mut base_vp = *s_vp2.borrow();
            base_vp.pan(offset_x, offset_y, width, height);
            st_d_upd.borrow_mut().viewport = base_vp;
            st_d_upd.borrow().drawing_area.queue_draw();
        });
        drawing_area.add_controller(drag);

        // Scroll controller for zooming anchored at mouse cursor
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
            st_scroll.borrow().drawing_area.queue_draw();
            glib::Propagation::Stop
        });
        drawing_area.add_controller(scroll);

        // Motion controller for inspection hover probe
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

        // 4. UI Actions
        let st_add = state.clone();
        add_btn.connect_clicked(move |_| {
            {
                let mut st = st_add.borrow_mut();
                let id = st.next_id;
                st.next_id += 1;
                let col = DEFAULT_COLORS[id % DEFAULT_COLORS.len()];
                st.functions.push(PlottedFunction::new(id, "cos(x)", col));
            };
            refresh_function_rows(&st_add);
            st_add.borrow().drawing_area.queue_draw();
        });

        let st_reset = state.clone();
        reset_btn.connect_clicked(move |_| {
            st_reset.borrow_mut().viewport.reset();
            st_reset.borrow().drawing_area.queue_draw();
            (st_reset.borrow().on_toast)("Reset view to default");
        });

        let st_roots = state.clone();
        roots_toggle.connect_toggled(move |btn| {
            st_roots.borrow_mut().show_roots = btn.is_active();
            st_roots.borrow().drawing_area.queue_draw();
        });

        let st_extrema = state.clone();
        extrema_toggle.connect_toggled(move |btn| {
            st_extrema.borrow_mut().show_extrema = btn.is_active();
            st_extrema.borrow().drawing_area.queue_draw();
        });

        let st_int = state.clone();
        int_toggle.connect_toggled(move |btn| {
            let active = btn.is_active();
            st_int.borrow_mut().integral_active = active;
            if active {
                update_integral_calculation(&st_int);
            } else {
                st_int.borrow().integral_label.set_text("");
            }
            st_int.borrow().drawing_area.queue_draw();
        });

        Self { root, state }
    }
}

fn refresh_function_rows(state: &Rc<RefCell<GraphState>>) {
    let list_box = state.borrow().func_list_box.clone();
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    let funcs = state.borrow().functions.clone();
    for func in funcs {
        let fid = func.id;
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        row.add_css_class("graph-func-row");

        // Color indicator pill
        let (r, g, b) = func.color;
        let color_dot = gtk::Label::new(Some("●"));
        color_dot.add_css_class("graph-color-dot");
        let style = format!("color: rgb({}, {}, {});", (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
        let provider = gtk::CssProvider::new();
        provider.load_from_string(&format!(".graph-color-dot-{} {{ {} }}", fid, style));
        color_dot.add_css_class(&format!("graph-color-dot-{}", fid));
        gtk::style_context_add_provider_for_display(
            &gdk::Display::default().unwrap(),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        row.append(&color_dot);

        // Formula entry
        let entry = gtk::Entry::new();
        entry.set_text(&func.formula);
        entry.add_css_class("graph-func-entry");
        entry.set_hexpand(true);

        let st_e = state.clone();
        entry.connect_changed(move |ent| {
            let new_text = ent.text().to_string();
            let mut st = st_e.borrow_mut();
            if let Some(f) = st.functions.iter_mut().find(|f| f.id == fid) {
                f.set_formula(&new_text);
            }
            st.drawing_area.queue_draw();
        });
        row.append(&entry);

        // Visibility toggle button
        let vis_btn = gtk::Button::builder()
            .icon_name(if func.visible { "view-reveal-symbolic" } else { "view-conceal-symbolic" })
            .tooltip_text("Toggle curve visibility")
            .has_frame(false)
            .build();
        vis_btn.add_css_class("bar-button");

        let st_v = state.clone();
        let vb = vis_btn.clone();
        vis_btn.connect_clicked(move |_| {
            let mut st = st_v.borrow_mut();
            if let Some(f) = st.functions.iter_mut().find(|f| f.id == fid) {
                f.visible = !f.visible;
                vb.set_icon_name(if f.visible { "view-reveal-symbolic" } else { "view-conceal-symbolic" });
            }
            st.drawing_area.queue_draw();
        });
        row.append(&vis_btn);

        // Derivative toggle button
        let deriv_btn = gtk::Button::with_label("f'");
        deriv_btn.set_tooltip_text(Some("Toggle derivative curve"));
        deriv_btn.add_css_class("graph-tool-toggle");
        if func.show_derivative {
            deriv_btn.add_css_class("active");
        }

        let st_d = state.clone();
        let db = deriv_btn.clone();
        deriv_btn.connect_clicked(move |_| {
            let mut st = st_d.borrow_mut();
            if let Some(f) = st.functions.iter_mut().find(|f| f.id == fid) {
                f.show_derivative = !f.show_derivative;
                if f.show_derivative {
                    db.add_css_class("active");
                } else {
                    db.remove_css_class("active");
                }
            }
            st.drawing_area.queue_draw();
        });
        row.append(&deriv_btn);

        // Remove button
        let del_btn =
            gtk::Button::builder().icon_name("user-trash-symbolic").tooltip_text("Remove function").has_frame(false).build();
        del_btn.add_css_class("bar-button");

        let st_del = state.clone();
        del_btn.connect_clicked(move |_| {
            st_del.borrow_mut().functions.retain(|f| f.id != fid);
            refresh_function_rows(&st_del);
            st_del.borrow().drawing_area.queue_draw();
        });
        row.append(&del_btn);

        list_box.append(&row);
    }
}

fn update_integral_calculation(state: &Rc<RefCell<GraphState>>) {
    let (funcs, bounds) = {
        let st = state.borrow();
        (st.functions.clone(), st.integral_bounds)
    };

    if let Some(f) = funcs.iter().find(|f| f.visible && f.parsed.is_some()) {
        let (a, b) = bounds;
        let eval_fn = |x: f64| f.eval(x);
        if let Ok(res) = integrate(&eval_fn, a, b, 200) {
            state.borrow().integral_label.set_text(&format!("∫ [{a:.2}, {b:.2}] = {res:.4}"));
            return;
        }
    }
    state.borrow().integral_label.set_text("");
}

fn draw_graph(state: &Rc<RefCell<GraphState>>, cr: &cairo::Context, width: f64, height: f64) {
    let (vp, funcs, hover, show_roots, show_extrema, int_active, int_bounds) = {
        let st = state.borrow();
        (
            st.viewport,
            st.functions.clone(),
            st.hover_coords,
            st.show_roots,
            st.show_extrema,
            st.integral_active,
            st.integral_bounds,
        )
    };

    // The theme's colours: the page's background, its text for the grid, axes and
    // labels, and its surface for the coordinate badge.
    let p = nyx_theme::palette();
    let rgb = |c: &str| {
        gtk::gdk::RGBA::parse(c).map_or((0.5, 0.5, 0.5), |c| (f64::from(c.red()), f64::from(c.green()), f64::from(c.blue())))
    };
    let (bg_r, bg_g, bg_b) = rgb(&p.bg);
    let (tx_r, tx_g, tx_b) = rgb(&p.text);
    let (sf_r, sf_g, sf_b) = rgb(&p.surface);

    // 1. Clear Background
    cr.set_source_rgb(bg_r, bg_g, bg_b);
    let _ = cr.paint();

    // 2. Draw Dynamic Grid Lines
    let (step_x, step_y) = vp.grid_steps(width, height);

    // Minor/Secondary grid lines
    cr.set_line_width(0.6);
    cr.set_source_rgba(tx_r, tx_g, tx_b, 0.06);

    let start_x = (vp.x_min / (step_x / 2.0)).floor() as i64;
    let end_x = (vp.x_max / (step_x / 2.0)).ceil() as i64;
    for i in start_x..=end_x {
        let wx = i as f64 * (step_x / 2.0);
        let (sx, _) = vp.world_to_screen(wx, 0.0, width, height);
        cr.move_to(sx, 0.0);
        cr.line_to(sx, height);
    }

    let start_y = (vp.y_min / (step_y / 2.0)).floor() as i64;
    let end_y = (vp.y_max / (step_y / 2.0)).ceil() as i64;
    for i in start_y..=end_y {
        let wy = i as f64 * (step_y / 2.0);
        let (_, sy) = vp.world_to_screen(0.0, wy, width, height);
        cr.move_to(0.0, sy);
        cr.line_to(width, sy);
    }
    let _ = cr.stroke();

    // Major grid lines & Axis Labels
    cr.set_line_width(0.9);
    cr.set_source_rgba(tx_r, tx_g, tx_b, 0.14);

    let major_start_x = (vp.x_min / step_x).floor() as i64;
    let major_end_x = (vp.x_max / step_x).ceil() as i64;
    for i in major_start_x..=major_end_x {
        let wx = i as f64 * step_x;
        let (sx, _) = vp.world_to_screen(wx, 0.0, width, height);
        cr.move_to(sx, 0.0);
        cr.line_to(sx, height);
    }

    let major_start_y = (vp.y_min / step_y).floor() as i64;
    let major_end_y = (vp.y_max / step_y).ceil() as i64;
    for i in major_start_y..=major_end_y {
        let wy = i as f64 * step_y;
        let (_, sy) = vp.world_to_screen(0.0, wy, width, height);
        cr.move_to(0.0, sy);
        cr.line_to(width, sy);
    }
    let _ = cr.stroke();

    // 3. Coordinate Axes (X and Y crosshair)
    let (origin_x, origin_y) = vp.world_to_screen(0.0, 0.0, width, height);
    cr.set_line_width(1.6);
    cr.set_source_rgba(tx_r, tx_g, tx_b, 0.45);

    // X axis
    if (0.0..=height).contains(&origin_y) {
        cr.move_to(0.0, origin_y);
        cr.line_to(width, origin_y);
    }
    // Y axis
    if (0.0..=width).contains(&origin_x) {
        cr.move_to(origin_x, 0.0);
        cr.line_to(origin_x, height);
    }
    let _ = cr.stroke();

    // Labels on axes
    cr.set_source_rgba(tx_r, tx_g, tx_b, 0.6);
    cr.set_font_size(10.0);
    for i in major_start_x..=major_end_x {
        if i == 0 {
            continue;
        }
        let wx = i as f64 * step_x;
        let (sx, _) = vp.world_to_screen(wx, 0.0, width, height);
        let clamped_y = origin_y.clamp(14.0, height - 6.0);
        let text = format!("{wx:.4}");
        let text = text.trim_end_matches('0').trim_end_matches('.');
        // Labels that would be cut off at the plot's edges are left out.
        let w = cr.text_extents(text).map(|e| e.x_advance()).unwrap_or(0.0);
        let x = sx - w / 2.0;
        if x < 2.0 || x + w > width - 2.0 {
            continue;
        }
        cr.move_to(x, clamped_y + 12.0);
        let _ = cr.show_text(text);
    }
    for i in major_start_y..=major_end_y {
        if i == 0 {
            continue;
        }
        let wy = i as f64 * step_y;
        let (_, sy) = vp.world_to_screen(0.0, wy, width, height);
        let clamped_x = origin_x.clamp(6.0, width - 36.0);
        if sy - 8.0 < 2.0 || sy + 3.0 > height - 2.0 {
            continue;
        }
        cr.move_to(clamped_x + 6.0, sy + 3.0);
        let _ = cr.show_text(format!("{wy:.4}").trim_end_matches('0').trim_end_matches('.'));
    }

    // 4. Shaded Integral Area (if active)
    if int_active && let Some(f) = funcs.iter().find(|f| f.visible && f.parsed.is_some()) {
        let (a, b) = int_bounds;
        let (sa, _) = vp.world_to_screen(a, 0.0, width, height);
        let (sb, _) = vp.world_to_screen(b, 0.0, width, height);
        let (_, sy_zero) = vp.world_to_screen(0.0, 0.0, width, height);

        cr.set_source_rgba(0.35, 0.55, 0.95, 0.18);
        cr.move_to(sa, sy_zero);

        let steps = 100;
        let dx = (b - a) / steps as f64;
        for i in 0..=steps {
            let x = a + i as f64 * dx;
            if let Ok(y) = f.eval(x) {
                let (sx, sy) = vp.world_to_screen(x, y, width, height);
                cr.line_to(sx, sy);
            }
        }
        cr.line_to(sb, sy_zero);
        cr.close_path();
        let _ = cr.fill();
    }

    // 5. Draw Function Curves
    for f in &funcs {
        if !f.visible || f.parsed.is_none() {
            continue;
        }

        let (r, g, b) = f.color;

        // Derivative curve (dashed)
        if f.show_derivative {
            let deriv_segments = sample_derivative(f, &vp, width, height);
            cr.set_source_rgba(r, g, b, 0.55);
            cr.set_line_width(1.4);
            cr.set_dash(&[6.0, 4.0], 0.0);
            for seg in deriv_segments {
                if let Some(&(first_x, first_y)) = seg.first() {
                    cr.move_to(first_x, first_y);
                    for &(px, py) in &seg[1..] {
                        cr.line_to(px, py);
                    }
                }
            }
            let _ = cr.stroke();
            cr.set_dash(&[], 0.0); // Reset dash
        }

        // Primary curve
        let segments = sample_function(f, &vp, width, height);
        cr.set_source_rgb(r, g, b);
        cr.set_line_width(2.4);

        for seg in segments {
            if let Some(&(first_x, first_y)) = seg.first() {
                cr.move_to(first_x, first_y);
                for &(px, py) in &seg[1..] {
                    cr.line_to(px, py);
                }
            }
        }
        let _ = cr.stroke();

        // 6. Roots (if enabled)
        if show_roots {
            let eval_fn = |x: f64| f.eval(x);
            let roots = find_roots(&eval_fn, vp.x_min, vp.x_max, 150);
            cr.set_source_rgb(0.95, 0.85, 0.20); // Gold points
            for root_x in roots {
                let (sx, sy) = vp.world_to_screen(root_x, 0.0, width, height);
                cr.arc(sx, sy, 4.5, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();

                // Small root label
                cr.set_source_rgba(tx_r, tx_g, tx_b, 0.9);
                cr.set_font_size(9.0);
                cr.move_to(sx - 10.0, sy - 7.0);
                let _ = cr.show_text(&format!("{root_x:.2}"));
                cr.set_source_rgb(0.95, 0.85, 0.20);
            }
        }

        // 7. Extrema (if enabled)
        if show_extrema {
            let eval_fn = |x: f64| f.eval(x);
            let extrema = find_extrema(&eval_fn, vp.x_min, vp.x_max, 150);
            for (ex_x, ex_y, is_max) in extrema {
                let (sx, sy) = vp.world_to_screen(ex_x, ex_y, width, height);
                if is_max {
                    cr.set_source_rgb(0.3, 0.85, 0.6); // Emerald for max
                } else {
                    cr.set_source_rgb(0.95, 0.4, 0.4); // Coral for min
                }
                cr.arc(sx, sy, 4.5, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();

                cr.set_source_rgba(tx_r, tx_g, tx_b, 0.9);
                cr.set_font_size(9.0);
                cr.move_to(sx - 12.0, sy - 7.0);
                let _ = cr.show_text(&format!("({ex_x:.2}, {ex_y:.2})"));
            }
        }
    }

    // 8. Hover Coordinate Probe & Crosshair
    if let Some((hx, hy)) = hover {
        let (world_hx, _) = vp.screen_to_world(hx, hy, width, height);

        // Find closest curve to hover x
        let mut closest_point = None;
        let mut min_dist = f64::MAX;

        for f in &funcs {
            if f.visible
                && f.parsed.is_some()
                && let Ok(y) = f.eval(world_hx)
            {
                let (sx, sy) = vp.world_to_screen(world_hx, y, width, height);
                let dist = (sy - hy).abs();
                if dist < min_dist && dist < 80.0 {
                    min_dist = dist;
                    closest_point = Some((sx, sy, world_hx, y, f.color));
                }
            }
        }

        if let Some((sx, sy, wx, wy, col)) = closest_point {
            // Dotted crosshairs to axes
            cr.set_source_rgba(tx_r, tx_g, tx_b, 0.35);
            cr.set_line_width(1.0);
            cr.set_dash(&[4.0, 3.0], 0.0);
            cr.move_to(sx, 0.0);
            cr.line_to(sx, height);
            cr.move_to(0.0, sy);
            cr.line_to(width, sy);
            let _ = cr.stroke();
            cr.set_dash(&[], 0.0);

            // Glowing dot on curve
            let (r, g, b) = col;
            cr.set_source_rgba(r, g, b, 0.3);
            cr.arc(sx, sy, 9.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();

            cr.set_source_rgb(r, g, b);
            cr.arc(sx, sy, 4.5, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();

            cr.set_source_rgb(bg_r, bg_g, bg_b);
            cr.arc(sx, sy, 2.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();

            // Coordinate Pill Badge
            let badge_text = format!("({:.3}, {:.3})", wx, wy);
            let badge_x = (sx + 12.0).min(width - 110.0);
            let badge_y = (sy - 12.0).max(24.0);

            cr.set_source_rgba(sf_r, sf_g, sf_b, 0.92);
            draw_rounded_rect(cr, badge_x - 6.0, badge_y - 14.0, 100.0, 22.0, 6.0);
            let _ = cr.fill();

            cr.set_source_rgba(tx_r, tx_g, tx_b, 0.25);
            cr.set_line_width(1.0);
            draw_rounded_rect(cr, badge_x - 6.0, badge_y - 14.0, 100.0, 22.0, 6.0);
            let _ = cr.stroke();

            cr.set_source_rgb(tx_r, tx_g, tx_b);
            cr.set_font_size(10.5);
            cr.move_to(badge_x, badge_y + 1.0);
            let _ = cr.show_text(&badge_text);
        }
    }
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
