use calculator_core::natural::{NaturalSession, SheetLineKind};
use calculator_storage::{CalculatorDb, SheetRecord};
use gtk::gdk;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Each line's result (its text, and whether it's an error), shared with the gutter.
type LineResults = Rc<RefCell<Vec<Option<(String, bool)>>>>;

pub struct SheetsView {
    pub root: gtk::Box,
    #[allow(dead_code)]
    state: Rc<RefCell<SheetsState>>,
}

struct SheetsState {
    db: CalculatorDb,
    current_sheet_id: Option<i64>,
    sheets: Vec<SheetRecord>,
    sheet_dropdown: gtk::DropDown,
    title_entry: gtk::Entry,
    /// The sheet picker, or the name field while renaming.
    name_stack: gtk::Stack,
    text_view: gtk::TextView,
    /// Drawn beside each line in `gutter`.
    results: LineResults,
    gutter: gtk::DrawingArea,
    session: NaturalSession,
    save_timer: Option<glib::SourceId>,
    on_toast: Rc<dyn Fn(&str)>,
    is_updating: bool,
}

impl SheetsView {
    pub fn new(db: CalculatorDb, on_toast: impl Fn(&str) + 'static) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.add_css_class("sheets-view");

        let toast_rc: Rc<dyn Fn(&str)> = Rc::new(on_toast);

        // 1. Top toolbar
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        toolbar.add_css_class("sheets-toolbar");

        let sheet_strings = gtk::StringList::new(&[]);
        let sheet_dropdown = gtk::DropDown::new(Some(sheet_strings), None::<gtk::Expression>);
        sheet_dropdown.add_css_class("converter-dropdown");
        sheet_dropdown.set_tooltip_text(Some("Switch sheet"));

        // The sheet's name shows once: in the picker, which turns into a field to rename it.
        let title_entry = gtk::Entry::new();
        title_entry.set_placeholder_text(Some("Sheet name"));
        title_entry.add_css_class("sheet-title-input");
        let name_stack = gtk::Stack::new();
        name_stack.set_hexpand(true);
        name_stack.set_hhomogeneous(true);
        name_stack.add_named(&sheet_dropdown, Some("pick"));
        name_stack.add_named(&title_entry, Some("rename"));
        toolbar.append(&name_stack);

        let rename_btn = nyx_ui::widgets::bar_button("document-edit-symbolic", "Rename this sheet");
        toolbar.append(&rename_btn);

        let new_btn = nyx_ui::widgets::bar_button("list-add-symbolic", "Create new calculation sheet");
        toolbar.append(&new_btn);

        let copy_all_btn = nyx_ui::widgets::bar_button("edit-copy-symbolic", "Copy all expressions and results to clipboard");
        toolbar.append(&copy_all_btn);

        let delete_btn = nyx_ui::widgets::bar_button("user-trash-symbolic", "Delete current sheet");
        toolbar.append(&delete_btn);

        root.append(&toolbar);

        // 2. The editor, with each line's result drawn beside it in a gutter
        let editor_pane = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        editor_pane.add_css_class("sheets-editor-pane");
        editor_pane.set_vexpand(true);
        editor_pane.set_overflow(gtk::Overflow::Hidden);

        // Left: Multi-line text view in scrolled window
        let scrolled_editor = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::External)
            .hexpand(true)
            .vexpand(true)
            .build();
        scrolled_editor.add_css_class("sheets-scrolled-editor");

        let text_view = gtk::TextView::new();
        text_view.add_css_class("sheets-text-view");
        text_view.set_monospace(true);
        text_view.set_wrap_mode(gtk::WrapMode::WordChar);
        text_view.set_left_margin(12);
        text_view.set_right_margin(12);
        text_view.set_top_margin(10);
        text_view.set_bottom_margin(10);
        scrolled_editor.set_child(Some(&text_view));
        editor_pane.append(&scrolled_editor);

        let results: LineResults = Rc::default();
        let gutter = results_gutter(&text_view, results.clone(), toast_rc.clone());
        scrolled_editor.vadjustment().connect_value_changed({
            let gutter = gutter.clone();
            move |_| gutter.queue_draw()
        });

        root.append(&editor_pane);

        let state = Rc::new(RefCell::new(SheetsState {
            db,
            current_sheet_id: None,
            sheets: Vec::new(),
            sheet_dropdown: sheet_dropdown.clone(),
            title_entry: title_entry.clone(),
            name_stack: name_stack.clone(),
            text_view: text_view.clone(),
            results,
            gutter,
            session: NaturalSession::new(),
            save_timer: None,
            on_toast: toast_rc.clone(),
            is_updating: false,
        }));

        // Load existing sheets from DB or initialize default
        reload_sheets_from_db(&state);

        // Text change listener
        let buffer = text_view.buffer();
        let st_buf = state.clone();
        buffer.connect_changed(move |_| {
            if st_buf.borrow().is_updating {
                return;
            }
            re_evaluate_and_schedule_save(&st_buf);
        });

        // Title change listener
        let st_title = state.clone();
        title_entry.connect_changed(move |_| {
            if st_title.borrow().is_updating {
                return;
            }
            schedule_save(&st_title);
        });

        // Rename: the picker becomes the name field; Enter or leaving it saves the name.
        let st_ren = state.clone();
        rename_btn.connect_clicked(move |_| {
            let (stack, entry) = {
                let st = st_ren.borrow();
                (st.name_stack.clone(), st.title_entry.clone())
            };
            if stack.visible_child_name().as_deref() == Some("rename") {
                finish_rename(&st_ren);
            } else {
                stack.set_visible_child_name("rename");
                entry.grab_focus();
            }
        });
        let st_act = state.clone();
        title_entry.connect_activate(move |_| finish_rename(&st_act));
        let focus = gtk::EventControllerFocus::new();
        let st_leave = state.clone();
        focus.connect_leave(move |_| finish_rename(&st_leave));
        title_entry.add_controller(focus);

        // Sheet dropdown switch listener
        let st_drop = state.clone();
        sheet_dropdown.connect_selected_notify(move |dd| {
            if st_drop.borrow().is_updating {
                return;
            }
            let idx = dd.selected() as usize;
            let (target_id, cur_id) = {
                let st = st_drop.borrow();
                let tid = st.sheets.get(idx).map(|s| s.id);
                (tid, st.current_sheet_id)
            };
            if let Some(tid) = target_id
                && Some(tid) != cur_id
            {
                load_sheet_by_id(&st_drop, tid);
            }
        });

        // "+ New" sheet button
        let st_new = state.clone();
        new_btn.connect_clicked(move |_| {
            create_new_sheet(&st_new, "Untitled Sheet", "");
        });

        // Copy All button
        let st_copy = state.clone();
        copy_all_btn.connect_clicked(move |_| {
            copy_sheet_summary(&st_copy);
        });

        // Delete button
        let st_del = state.clone();
        delete_btn.connect_clicked(move |_| {
            delete_current_sheet(&st_del);
        });

        Self { root, state }
    }
}

fn reload_sheets_from_db(state: &Rc<RefCell<SheetsState>>) {
    let list = state.borrow().db.list_sheets().unwrap_or_default();
    if list.is_empty() {
        // Create initial default welcome sheet
        let default_content = r#"# Nyx Calculation Sheet
// Natural language math, units & variables

price = 100
tax = 8%
total = price + tax

$250 split between 4 people
5 miles in kilometers
32 fahrenheit to celsius
2 hours 30 minutes in seconds"#;

        // Bound first: a borrow in the `if let` condition would last through the block.
        let created = state.borrow().db.create_sheet("Default Sheet", default_content);
        if let Ok(id) = created {
            let list = state.borrow().db.list_sheets().unwrap_or_default();
            populate_sheet_dropdown(state, list, Some(id));
            load_sheet_by_id(state, id);
            return;
        }
    }

    let first_id = list.first().map(|s| s.id);
    populate_sheet_dropdown(state, list, first_id);
    if let Some(id) = first_id {
        load_sheet_by_id(state, id);
    }
}

fn populate_sheet_dropdown(state: &Rc<RefCell<SheetsState>>, sheets: Vec<SheetRecord>, sel_id: Option<i64>) {
    let titles: Vec<String> = sheets.iter().map(|s| s.title.clone()).collect();
    let title_refs: Vec<&str> = titles.iter().map(|s| s.as_str()).collect();
    let model = gtk::StringList::new(&title_refs);

    let sel_idx = if let Some(id) = sel_id { sheets.iter().position(|s| s.id == id).unwrap_or(0) } else { 0 };

    let dd = {
        let mut st = state.borrow_mut();
        st.is_updating = true;
        st.sheets = sheets;
        st.sheet_dropdown.clone()
    };

    dd.set_model(Some(&model));
    dd.set_selected(sel_idx as u32);

    state.borrow_mut().is_updating = false;
}

fn load_sheet_by_id(state: &Rc<RefCell<SheetsState>>, id: i64) {
    let maybe_record = state.borrow().db.get_sheet(id).ok().flatten();
    if let Some(record) = maybe_record {
        let (title_entry, text_view) = {
            let mut st = state.borrow_mut();
            st.current_sheet_id = Some(id);
            st.is_updating = true;
            (st.title_entry.clone(), st.text_view.clone())
        };

        title_entry.set_text(&record.title);
        text_view.buffer().set_text(&record.content);

        state.borrow_mut().is_updating = false;

        re_evaluate(state);
    }
}

fn create_new_sheet(state: &Rc<RefCell<SheetsState>>, title: &str, content: &str) {
    let created = state.borrow().db.create_sheet(title, content);
    if let Ok(id) = created {
        let list = state.borrow().db.list_sheets().unwrap_or_default();
        populate_sheet_dropdown(state, list, Some(id));
        load_sheet_by_id(state, id);
        (state.borrow().on_toast)(&format!("Created new sheet: {title}"));
    }
}

fn delete_current_sheet(state: &Rc<RefCell<SheetsState>>) {
    let cur_id = state.borrow().current_sheet_id;
    if let Some(id) = cur_id {
        let _ = state.borrow().db.delete_sheet(id);
        reload_sheets_from_db(state);
        (state.borrow().on_toast)("Deleted sheet");
    }
}

fn re_evaluate_and_schedule_save(state: &Rc<RefCell<SheetsState>>) {
    re_evaluate(state);
    schedule_save(state);
}

fn re_evaluate(state: &Rc<RefCell<SheetsState>>) {
    let text = {
        let buf = state.borrow().text_view.buffer();
        let (start, end) = buf.bounds();
        buf.text(&start, &end, false).to_string()
    };

    let outputs = state.borrow_mut().session.evaluate_sheet(&text);

    let mut per_line = Vec::new();
    for out in outputs {
        let item = match out.kind {
            SheetLineKind::Result(res) => Some((res.formatted, false)),
            SheetLineKind::Error(err) => Some((err, true)),
            SheetLineKind::Empty | SheetLineKind::Comment(_) | SheetLineKind::Heading(_) => None,
        };
        if per_line.len() <= out.line_index {
            per_line.resize(out.line_index + 1, None);
        }
        per_line[out.line_index] = item;
    }
    let (results, gutter) = {
        let st = state.borrow();
        (st.results.clone(), st.gutter.clone())
    };
    *results.borrow_mut() = per_line;
    gutter.queue_draw();
}

/// Back from renaming: save the name now and show it in the picker.
fn finish_rename(state: &Rc<RefCell<SheetsState>>) {
    let (stack, cur_id, title) = {
        let st = state.borrow();
        (st.name_stack.clone(), st.current_sheet_id, st.title_entry.text().trim().to_string())
    };
    if stack.visible_child_name().as_deref() != Some("rename") {
        return;
    }
    stack.set_visible_child_name("pick");
    if let Some(id) = cur_id {
        let title = if title.is_empty() { "Untitled Sheet".to_string() } else { title };
        let content = {
            let buf = state.borrow().text_view.buffer();
            let (start, end) = buf.bounds();
            buf.text(&start, &end, false).to_string()
        };
        let _ = state.borrow().db.update_sheet(id, &title, &content);
        let list = state.borrow().db.list_sheets().unwrap_or_default();
        populate_sheet_dropdown(state, list, Some(id));
    }
}

/// The right-hand gutter of the editor: each line's result at the height of that line
/// (so it stays beside it when lines wrap or the sheet scrolls); a click copies it.
fn results_gutter(
    text_view: &gtk::TextView,
    results: LineResults,
    on_toast: Rc<dyn Fn(&str)>,
) -> gtk::DrawingArea {
    let gutter = gtk::DrawingArea::new();
    gutter.add_css_class("sheet-gutter");
    gutter.set_content_width(170);
    let tv = text_view.clone();
    let res = results.clone();
    gutter.set_draw_func(move |area, cr, width, _| {
        let buffer = tv.buffer();
        let accent = area.color();
        let danger = gdk::RGBA::parse(nyx_theme::palette().danger.as_str()).unwrap_or(accent);
        for (line, item) in res.borrow().iter().enumerate() {
            let Some((text, error)) = item else { continue };
            let Some(iter) = buffer.iter_at_line(line as i32) else { continue };
            let row = tv.iter_location(&iter);
            let (_, y) = tv.buffer_to_window_coords(gtk::TextWindowType::Right, 0, row.y());
            let layout = area.create_pango_layout(Some(text));
            layout.set_width((width - 12) * gtk::pango::SCALE);
            layout.set_ellipsize(gtk::pango::EllipsizeMode::End);
            layout.set_alignment(gtk::pango::Alignment::Right);
            let (_, logical) = layout.pixel_extents();
            let c = if *error { danger } else { accent };
            cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64);
            cr.move_to(0.0, (y + (row.height() - logical.height()) / 2) as f64);
            pangocairo::functions::show_layout(cr, &layout);
        }
    });
    let click = gtk::GestureClick::new();
    let tv = text_view.clone();
    click.connect_released(move |_, _, _, y| {
        let (_, by) = tv.window_to_buffer_coords(gtk::TextWindowType::Right, 0, y as i32);
        let (iter, _) = tv.line_at_y(by);
        if let Some(Some((text, false))) = results.borrow().get(iter.line() as usize)
            && let Some(display) = gdk::Display::default()
        {
            display.clipboard().set_text(text);
            on_toast(&format!("Copied: {text}"));
        }
    });
    gutter.add_controller(click);
    text_view.set_gutter(gtk::TextWindowType::Right, Some(&gutter));
    gutter
}

fn schedule_save(state: &Rc<RefCell<SheetsState>>) {
    if let Some(id) = state.borrow_mut().save_timer.take() {
        id.remove();
    }

    let st = state.clone();
    let source_id = glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
        let (cur_id, title, content) = {
            let s = st.borrow();
            let buf = s.text_view.buffer();
            let (start, end) = buf.bounds();
            (s.current_sheet_id, s.title_entry.text().to_string(), buf.text(&start, &end, false).to_string())
        };

        if let Some(id) = cur_id {
            let final_title = if title.trim().is_empty() { "Untitled Sheet" } else { title.trim() };
            let _ = st.borrow().db.update_sheet(id, final_title, &content);
        }
    });

    state.borrow_mut().save_timer = Some(source_id);
}

fn copy_sheet_summary(state: &Rc<RefCell<SheetsState>>) {
    let (title, content) = {
        let s = state.borrow();
        let buf = s.text_view.buffer();
        let (start, end) = buf.bounds();
        (s.title_entry.text().to_string(), buf.text(&start, &end, false).to_string())
    };

    let mut session = NaturalSession::new();
    let outputs = session.evaluate_sheet(&content);

    let mut full_text = format!("# {title}\n\n");
    for out in outputs {
        match out.kind {
            SheetLineKind::Empty => full_text.push('\n'),
            SheetLineKind::Heading(h) => full_text.push_str(&format!("# {h}\n")),
            SheetLineKind::Comment(c) => full_text.push_str(&format!("{c}\n")),
            SheetLineKind::Result(res) => {
                full_text.push_str(&format!("{:<30} => {}\n", out.original_text, res.formatted));
            }
            SheetLineKind::Error(err) => {
                full_text.push_str(&format!("{:<30} => [Error: {}]\n", out.original_text, err));
            }
        }
    }

    if let Some(display) = gdk::Display::default() {
        display.clipboard().set_text(&full_text);
        (state.borrow().on_toast)("Copied full sheet to clipboard");
    }
}
