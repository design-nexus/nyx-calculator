use calculator_core::natural::{NaturalSession, SheetLineKind};
use calculator_storage::{CalculatorDb, SheetRecord};
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

/// Each line's result (its text, and whether it's an error), shared with the gutter.
type LineResults = Rc<RefCell<Vec<Option<(String, bool)>>>>;

const WELCOME: &str = r#"# Trip budget
flights = 2 × $318
hotel = 4 × $142
food = $55 × 5

flights + hotel + food
prev + 8% tax
prev split between 3

# Getting there
210 miles in km
72 fahrenheit to celsius"#;

pub struct SheetsView {
    /// The page: the sheet and the tip under it.
    pub root: gtk::Box,
    /// For the panel's bar after its name: the sheet picker (a name field while renaming).
    pub header_start: gtk::Stack,
    /// For the panel's bar before Close: Copy all, New sheet and the ⋯ menu.
    pub header_end: gtk::Box,
    copy_all: gtk::Button,
    state: Rc<RefCell<SheetsState>>,
}

struct SheetsState {
    db: CalculatorDb,
    current_sheet_id: Option<i64>,
    sheets: Vec<SheetRecord>,
    title: gtk::Label,
    picker: gtk::Button,
    title_entry: gtk::Entry,
    /// The picker, or the field to rename it in.
    name_stack: gtk::Stack,
    saved_at: Option<Instant>,
    text_view: gtk::TextView,
    results: LineResults,
    gutter: gtk::DrawingArea,
    numbers: gtk::DrawingArea,
    session: NaturalSession,
    save_timer: Option<glib::SourceId>,
    on_toast: Rc<dyn Fn(&str)>,
    is_updating: bool,
}

impl SheetsView {
    pub fn new(db: CalculatorDb, on_toast: impl Fn(&str) + 'static) -> Self {
        let toast_rc: Rc<dyn Fn(&str)> = Rc::new(on_toast);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("sheets-view");

        // The picker: the sheet's name and a chevron; a click lists every sheet
        let picker = gtk::Button::new();
        picker.add_css_class("calc-mode");
        picker.set_valign(gtk::Align::Center);
        let picker_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let title = gtk::Label::new(None);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(22);
        picker_content.append(&title);
        let chevron = gtk::Image::from_icon_name("pan-down-symbolic");
        chevron.add_css_class("calc-mode-chevron");
        picker_content.append(&chevron);
        picker.set_child(Some(&picker_content));
        let title_entry = gtk::Entry::new();
        title_entry.set_placeholder_text(Some("Sheet name"));
        title_entry.add_css_class("sheets-title-entry");
        title_entry.set_valign(gtk::Align::Center);
        let name_stack = gtk::Stack::new();
        name_stack.set_hhomogeneous(false);
        name_stack.set_vhomogeneous(false);
        name_stack.set_halign(gtk::Align::Start);
        name_stack.add_named(&picker, Some("name"));
        name_stack.add_named(&title_entry, Some("rename"));

        let header_end = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        let copy_all = nyx_ui::widgets::labeled_button("edit-copy-symbolic", "Copy all");
        copy_all.add_css_class("flat");
        copy_all.add_css_class("calc-ghost");
        copy_all.set_valign(gtk::Align::Center);
        copy_all.set_tooltip_text(Some("Copy every line with its result"));
        header_end.append(&copy_all);
        let new_btn = nyx_ui::widgets::bar_button("list-add-symbolic", "New sheet (Ctrl+N)");
        new_btn.update_property(&[gtk::accessible::Property::Label("New sheet")]);
        header_end.append(&new_btn);
        let more_btn = nyx_ui::widgets::bar_button("view-more-symbolic", "Rename, export or delete");
        more_btn.update_property(&[gtk::accessible::Property::Label("More")]);
        header_end.append(&more_btn);

        // The sheet: line numbers, the text, and each line's result in a column beside it
        let scrolled_editor = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::External)
            .hexpand(true)
            .vexpand(true)
            .build();
        scrolled_editor.add_css_class("sheets-scroller");
        let text_view = gtk::TextView::new();
        text_view.add_css_class("sheets-text-view");
        text_view.set_monospace(true);
        text_view.set_wrap_mode(gtk::WrapMode::WordChar);
        text_view.set_left_margin(4);
        text_view.set_right_margin(16);
        text_view.set_top_margin(10);
        text_view.set_bottom_margin(10);
        text_view.set_pixels_above_lines(4);
        text_view.set_pixels_below_lines(4);
        scrolled_editor.set_child(Some(&text_view));
        root.append(&scrolled_editor);
        install_tags(&text_view);

        let results: LineResults = Rc::default();
        let gutter = results_gutter(&text_view, results.clone(), toast_rc.clone());
        let numbers = numbers_gutter(&text_view);
        scrolled_editor.vadjustment().connect_value_changed({
            let (gutter, numbers) = (gutter.clone(), numbers.clone());
            move |_| {
                gutter.queue_draw();
                numbers.queue_draw();
            }
        });

        // How to write a sheet, under it
        let tip = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        tip.add_css_class("sheets-tip");
        let tip_icon = gtk::Image::from_icon_name("dialog-information-symbolic");
        tip_icon.set_valign(gtk::Align::Start);
        tip.append(&tip_icon);
        let tip_text = gtk::Label::new(None);
        let code =
            |s: &str| format!("<span font_family=\"JetBrains Mono\" foreground=\"{}\">{s}</span>", nyx_theme::palette().text);
        tip_text.set_markup(&format!(
            "Write sums the way you'd say them: {}, {}. Name a line with {}, or use {} for the line above and {} for line 2.",
            code("15% of 249"),
            code("5 miles in km"),
            code("="),
            code("prev"),
            code("$2"),
        ));
        tip_text.set_wrap(true);
        // Its natural width would be one long line, and the panel takes that width.
        tip_text.set_max_width_chars(40);
        tip_text.set_xalign(0.0);
        tip_text.set_hexpand(true);
        tip.append(&tip_text);
        root.append(&tip);

        let state = Rc::new(RefCell::new(SheetsState {
            db,
            current_sheet_id: None,
            sheets: Vec::new(),
            title,
            picker: picker.clone(),
            title_entry: title_entry.clone(),
            name_stack: name_stack.clone(),
            saved_at: None,
            text_view: text_view.clone(),
            results,
            gutter,
            numbers,
            session: NaturalSession::new(),
            save_timer: None,
            on_toast: toast_rc,
            is_updating: false,
        }));

        reload_sheets_from_db(&state);

        let buffer = text_view.buffer();
        let st_buf = state.clone();
        buffer.connect_changed(move |_| {
            if st_buf.borrow().is_updating {
                return;
            }
            re_evaluate(&st_buf);
            schedule_save(&st_buf);
        });
        buffer.connect_cursor_position_notify(mark_current_line);

        let st_pick = state.clone();
        picker.connect_clicked(move |b| open_picker(&st_pick, b));

        let st_act = state.clone();
        title_entry.connect_activate(move |_| finish_rename(&st_act));
        let focus = gtk::EventControllerFocus::new();
        let st_leave = state.clone();
        focus.connect_leave(move |_| finish_rename(&st_leave));
        title_entry.add_controller(focus);
        let st_esc = state.clone();
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                let st = st_esc.borrow();
                st.title_entry.set_text(&st.title.text());
                drop(st);
                finish_rename(&st_esc);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        title_entry.add_controller(keys);

        let st_new = state.clone();
        new_btn.connect_clicked(move |_| create_new_sheet(&st_new, "Untitled sheet", ""));
        let st_copy = state.clone();
        copy_all.connect_clicked(move |_| copy_sheet_summary(&st_copy));
        let st_more = state.clone();
        more_btn.connect_clicked(move |b| {
            let (st_r, st_e, st_d) = (st_more.clone(), st_more.clone(), st_more.clone());
            let anchor = b.clone();
            nyx_ui::menu::popup_below(
                b,
                vec![
                    nyx_ui::menu::item("Rename", move || start_rename(&st_r)).icon("document-edit-symbolic"),
                    nyx_ui::menu::item("Export as text…", move || export(&st_e, &anchor)).icon("document-save-symbolic"),
                    nyx_ui::menu::separator(),
                    nyx_ui::menu::item("Delete this sheet", move || delete_current_sheet(&st_d))
                        .icon("user-trash-symbolic")
                        .danger(),
                ],
            );
        });

        // "Saved 3 min ago" keeps up with the clock.
        let weak = Rc::downgrade(&state);
        glib::timeout_add_seconds_local(30, move || {
            let Some(st) = weak.upgrade() else { return glib::ControlFlow::Break };
            show_saved(&st.borrow());
            glib::ControlFlow::Continue
        });

        Self { root, header_start: name_stack, header_end, copy_all, state }
    }

    /// Writes a change still waiting to be saved (the panel is closing).
    pub fn save_now(&self) {
        save_now_state(&self.state);
    }

    pub fn new_sheet(&self) {
        create_new_sheet(&self.state, "Untitled sheet", "");
    }

    /// In a narrow panel Copy all is just its icon.
    pub fn set_narrow(&self, narrow: bool) {
        if let Some(label) = self.copy_all.child().and_then(|c| c.last_child()) {
            label.set_visible(!narrow);
        }
    }
}

fn install_tags(text_view: &gtk::TextView) {
    let rgba = |c: &str, alpha: f32| gdk::RGBA::parse(c).map(|c| c.with_alpha(alpha)).ok();
    let swatch = |name: &str| nyx_theme::swatches().into_iter().find(|(n, _)| n == name).map(|(_, c)| c);
    let p = nyx_theme::palette();
    let table = text_view.buffer().tag_table();
    let tag = |name: &str| {
        let t = gtk::TextTag::new(Some(name));
        table.add(&t);
        t
    };
    let var = tag("var");
    var.set_foreground_rgba(swatch("violet").and_then(|c| rgba(&c, 1.0)).as_ref());
    let reference = tag("ref");
    reference.set_foreground_rgba(swatch("teal").and_then(|c| rgba(&c, 1.0)).as_ref());
    let heading = tag("heading");
    heading.set_foreground_rgba(rgba(&p.dim_text, 0.8).as_ref());
    heading.set_weight(600);
    let comment = tag("comment");
    comment.set_foreground_rgba(rgba(&p.dim_text, 0.8).as_ref());
    let current = tag("current");
    current.set_paragraph_background_rgba(rgba(&p.accent, 0.06).as_ref());
}

fn mark_current_line(buffer: &gtk::TextBuffer) {
    let (start, end) = buffer.bounds();
    buffer.remove_tag_by_name("current", &start, &end);
    let cursor = buffer.iter_at_mark(&buffer.get_insert());
    let mut line_start = cursor;
    line_start.set_line_offset(0);
    let mut line_end = cursor;
    if !line_end.ends_line() {
        line_end.forward_to_line_end();
    }
    buffer.apply_tag_by_name("current", &line_start, &line_end);
}

/// Colours a line's name (`food =`), its references (`prev`, `$2`), headings and comments.
fn highlight(buffer: &gtk::TextBuffer) {
    let (start, end) = buffer.bounds();
    for name in ["var", "ref", "heading", "comment"] {
        buffer.remove_tag_by_name(name, &start, &end);
    }
    for line in 0..buffer.line_count() {
        let Some(line_start) = buffer.iter_at_line(line) else { continue };
        let mut line_end = line_start;
        if !line_end.ends_line() {
            line_end.forward_to_line_end();
        }
        let text = buffer.text(&line_start, &line_end, false).to_string();
        let trimmed = text.trim_start();
        let at = |from: usize, to: usize| {
            let char_at = |byte: usize| text[..byte].chars().count() as i32;
            (buffer.iter_at_line_offset(line, char_at(from)), buffer.iter_at_line_offset(line, char_at(to)))
        };
        if trimmed.starts_with('#') {
            buffer.apply_tag_by_name("heading", &line_start, &line_end);
            continue;
        }
        if trimmed.starts_with("//") {
            buffer.apply_tag_by_name("comment", &line_start, &line_end);
            continue;
        }
        if let Some(eq) = text.find('=') {
            let name = text[..eq].trim_end();
            let lead = text.len() - text.trim_start().len();
            if !name.trim().is_empty()
                && name.trim().chars().all(|c| c.is_alphanumeric() || c == '_')
                && !text[eq..].starts_with("==")
                && let (Some(a), Some(b)) = at(lead, name.len())
            {
                buffer.apply_tag_by_name("var", &a, &b);
            }
        }
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < text.len() {
            let word_start = i == 0 || !(bytes[i - 1] as char).is_alphanumeric();
            let len = if word_start && text[i..].starts_with("prev") && !text[i + 4..].starts_with(|c: char| c.is_alphanumeric())
            {
                4
            } else if bytes[i] == b'$' && text[i + 1..].starts_with(|c: char| c.is_ascii_digit()) {
                // `$2` names an earlier line; `$318` is money.
                let digits: String = text[i + 1..].chars().take_while(|c| c.is_ascii_digit()).collect();
                let earlier = digits.parse::<i32>().is_ok_and(|n| n >= 1 && n <= line);
                let decimal = text[i + 1 + digits.len()..].starts_with(['.', ',']);
                if earlier && !decimal { 1 + digits.len() } else { 0 }
            } else {
                0
            };
            if len > 0 {
                if let (Some(a), Some(b)) = at(i, i + len) {
                    buffer.apply_tag_by_name("ref", &a, &b);
                }
                i += len;
            } else {
                i += text[i..].chars().next().map_or(1, char::len_utf8);
            }
        }
    }
}

fn reload_sheets_from_db(state: &Rc<RefCell<SheetsState>>) {
    let list = state.borrow().db.list_sheets().unwrap_or_default();
    if list.is_empty() {
        // Bound first: a borrow in the `if let` condition would last through the block.
        let created = state.borrow().db.create_sheet("Trip budget", WELCOME);
        if let Ok(id) = created {
            refresh_sheets(state);
            load_sheet_by_id(state, id);
            return;
        }
    }
    refresh_sheets(state);
    if let Some(id) = list.first().map(|s| s.id) {
        load_sheet_by_id(state, id);
    }
}

fn refresh_sheets(state: &Rc<RefCell<SheetsState>>) {
    let sheets = state.borrow().db.list_sheets().unwrap_or_default();
    state.borrow_mut().sheets = sheets;
}

/// Every sheet under the picker, the open one checked, then New sheet.
fn open_picker(state: &Rc<RefCell<SheetsState>>, anchor: &gtk::Button) {
    let (sheets, current) = {
        let st = state.borrow();
        (st.sheets.clone(), st.current_sheet_id)
    };
    let mut entries = vec![nyx_ui::menu::heading("Sheets")];
    for sheet in sheets {
        let (st, id) = (state.clone(), sheet.id);
        entries.push(nyx_ui::menu::check(&sheet.title, Some(id) == current, move |_| {
            if st.borrow().current_sheet_id != Some(id) {
                save_now_state(&st);
                load_sheet_by_id(&st, id);
            }
        }));
    }
    entries.push(nyx_ui::menu::separator());
    let st_new = state.clone();
    entries.push(
        nyx_ui::menu::item("New sheet", move || create_new_sheet(&st_new, "Untitled sheet", ""))
            .icon("list-add-symbolic")
            .key("Ctrl+N"),
    );
    nyx_ui::menu::popup_below(anchor, entries);
}

fn load_sheet_by_id(state: &Rc<RefCell<SheetsState>>, id: i64) {
    let maybe_record = state.borrow().db.get_sheet(id).ok().flatten();
    let Some(record) = maybe_record else { return };
    let (title, title_entry, text_view) = {
        let mut st = state.borrow_mut();
        st.current_sheet_id = Some(id);
        st.is_updating = true;
        st.saved_at = None;
        (st.title.clone(), st.title_entry.clone(), st.text_view.clone())
    };
    title.set_text(&record.title);
    title_entry.set_text(&record.title);
    text_view.buffer().set_text(&record.content);
    state.borrow_mut().is_updating = false;
    show_saved(&state.borrow());
    re_evaluate(state);
}

fn create_new_sheet(state: &Rc<RefCell<SheetsState>>, title: &str, content: &str) {
    save_now_state(state);
    let created = state.borrow().db.create_sheet(title, content);
    if let Ok(id) = created {
        refresh_sheets(state);
        load_sheet_by_id(state, id);
        start_rename(state);
    }
}

fn delete_current_sheet(state: &Rc<RefCell<SheetsState>>) {
    let (cur_id, title) = {
        let st = state.borrow();
        (st.current_sheet_id, st.title.text().to_string())
    };
    if let Some(id) = cur_id {
        if let Some(t) = state.borrow_mut().save_timer.take() {
            t.remove();
        }
        let _ = state.borrow().db.delete_sheet(id);
        reload_sheets_from_db(state);
        (state.borrow().on_toast)(&format!("Deleted “{title}”"));
    }
}

fn re_evaluate(state: &Rc<RefCell<SheetsState>>) {
    let buffer = state.borrow().text_view.buffer();
    let text = {
        let (start, end) = buffer.bounds();
        buffer.text(&start, &end, false).to_string()
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
    let (results, gutter, numbers) = {
        let st = state.borrow();
        (st.results.clone(), st.gutter.clone(), st.numbers.clone())
    };
    *results.borrow_mut() = per_line;
    highlight(&buffer);
    mark_current_line(&buffer);
    gutter.queue_draw();
    numbers.queue_draw();
}

fn start_rename(state: &Rc<RefCell<SheetsState>>) {
    let st = state.borrow();
    st.title_entry.set_text(&st.title.text());
    st.name_stack.set_visible_child_name("rename");
    st.title_entry.grab_focus();
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
    stack.set_visible_child_name("name");
    if cur_id.is_none() {
        return;
    }
    let title = if title.is_empty() { "Untitled sheet".to_string() } else { title };
    state.borrow().title.set_text(&title);
    state.borrow().title_entry.set_text(&title);
    save(state);
    refresh_sheets(state);
}

/// When the sheet was last saved, in the picker's tooltip.
fn show_saved(st: &SheetsState) {
    let saved = match st.saved_at.map(|t| t.elapsed().as_secs() / 60) {
        None => None,
        Some(0) => Some("Saved just now".to_string()),
        Some(1) => Some("Saved a minute ago".to_string()),
        Some(m) if m < 60 => Some(format!("Saved {m} min ago")),
        Some(_) => Some("Saved over an hour ago".to_string()),
    };
    let tip = match saved {
        Some(s) => format!("{s}. Pick another sheet"),
        None => "Pick another sheet".into(),
    };
    st.picker.set_tooltip_text(Some(&tip));
}

/// Saves the sheet, each line with its result, as a text file.
fn export(state: &Rc<RefCell<SheetsState>>, anchor: &gtk::Button) {
    let text = summary(state);
    let title = state.borrow().title.text().to_string();
    let dialog = gtk::FileDialog::builder().title("Export the sheet").initial_name(format!("{title}.txt")).modal(true).build();
    let parent = anchor.root().and_downcast::<gtk::Window>();
    let toast = state.borrow().on_toast.clone();
    dialog.save(parent.as_ref(), None::<&gio::Cancellable>, move |res| {
        let Ok(file) = res else { return };
        let Some(path) = file.path() else { return };
        match std::fs::write(&path, text.as_bytes()) {
            Ok(()) => toast(&format!("Exported to {}", path.display())),
            Err(e) => toast(&format!("Couldn't export: {e}")),
        }
    });
}

/// The gutter left of the sheet: each line's number at the height of that line.
fn numbers_gutter(text_view: &gtk::TextView) -> gtk::DrawingArea {
    let gutter = gtk::DrawingArea::new();
    gutter.add_css_class("sheet-numbers");
    gutter.set_content_width(40);
    let tv = text_view.clone();
    gutter.set_draw_func(move |area, cr, width, _| {
        let buffer = tv.buffer();
        let c = area.color();
        cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64);
        let (_, top) = tv.window_to_buffer_coords(gtk::TextWindowType::Left, 0, 0);
        let (_, bottom) = tv.window_to_buffer_coords(gtk::TextWindowType::Left, 0, area.height());
        for line in 0..buffer.line_count() {
            let Some(iter) = buffer.iter_at_line(line) else { continue };
            let row = tv.iter_location(&iter);
            if row.y() + row.height() < top || row.y() > bottom {
                continue;
            }
            let (_, y) = tv.buffer_to_window_coords(gtk::TextWindowType::Left, 0, row.y());
            let layout = area.create_pango_layout(Some(&(line + 1).to_string()));
            let (_, logical) = layout.pixel_extents();
            cr.move_to((width - 14 - logical.width()) as f64, (y + (row.height() - logical.height()) / 2) as f64);
            pangocairo::functions::show_layout(cr, &layout);
        }
    });
    text_view.set_gutter(gtk::TextWindowType::Left, Some(&gutter));
    gutter
}

/// The right-hand gutter of the editor: each line's result at the height of that line
/// (so it stays beside it when lines wrap or the sheet scrolls); a click copies it.
fn results_gutter(text_view: &gtk::TextView, results: LineResults, on_toast: Rc<dyn Fn(&str)>) -> gtk::DrawingArea {
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
            layout.set_width((width - 28) * gtk::pango::SCALE);
            layout.set_ellipsize(gtk::pango::EllipsizeMode::End);
            layout.set_alignment(gtk::pango::Alignment::Right);
            let (_, logical) = layout.pixel_extents();
            let c = if *error { danger } else { accent };
            cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64);
            cr.move_to(14.0, (y + (row.height() - logical.height()) / 2) as f64);
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
            on_toast(&format!("Copied {text}"));
        }
    });
    gutter.add_controller(click);
    gutter.set_tooltip_text(Some("Click a result to copy it"));
    text_view.set_gutter(gtk::TextWindowType::Right, Some(&gutter));
    gutter
}

fn schedule_save(state: &Rc<RefCell<SheetsState>>) {
    if let Some(id) = state.borrow_mut().save_timer.take() {
        id.remove();
    }
    let st = state.clone();
    let source_id = glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
        st.borrow_mut().save_timer = None;
        save(&st);
    });
    state.borrow_mut().save_timer = Some(source_id);
}

fn save_now_state(state: &Rc<RefCell<SheetsState>>) {
    let pending = state.borrow_mut().save_timer.take();
    if let Some(id) = pending {
        id.remove();
        save(state);
    }
}

fn save(state: &Rc<RefCell<SheetsState>>) {
    let (cur_id, title, content) = {
        let s = state.borrow();
        let buf = s.text_view.buffer();
        let (start, end) = buf.bounds();
        (s.current_sheet_id, s.title.text().to_string(), buf.text(&start, &end, false).to_string())
    };
    if let Some(id) = cur_id {
        let final_title = if title.trim().is_empty() { "Untitled sheet" } else { title.trim() };
        if state.borrow().db.update_sheet(id, final_title, &content).is_ok() {
            state.borrow_mut().saved_at = Some(Instant::now());
            show_saved(&state.borrow());
        }
    }
}

fn copy_sheet_summary(state: &Rc<RefCell<SheetsState>>) {
    let text = summary(state);
    if let Some(display) = gdk::Display::default() {
        display.clipboard().set_text(&text);
        (state.borrow().on_toast)("Copied the whole sheet");
    }
}

/// The sheet as text: its name, then each line with its result.
fn summary(state: &Rc<RefCell<SheetsState>>) -> String {
    let (title, content) = {
        let s = state.borrow();
        let buf = s.text_view.buffer();
        let (start, end) = buf.bounds();
        (s.title.text().to_string(), buf.text(&start, &end, false).to_string())
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
    full_text
}
