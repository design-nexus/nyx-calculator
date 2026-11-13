use calculator_storage::{CalculatorDb, HistoryEntry};
use gtk::gdk;
use gtk::glib;
use gtk::pango;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub struct HistorySidebar {
    pub root: gtk::Box,
    list_box: gtk::Box,
    search_entry: gtk::SearchEntry,
    db: CalculatorDb,
    on_restore: Rc<dyn Fn(String)>,
    on_toast: Rc<dyn Fn(&str)>,
    this: Weak<RefCell<HistorySidebar>>,
}

pub const WIDTH: i32 = 280;

impl HistorySidebar {
    pub fn new(db: CalculatorDb, on_restore: impl Fn(String) + 'static, on_toast: impl Fn(&str) + 'static) -> Rc<RefCell<Self>> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("calc-history");
        root.set_size_request(WIDTH, -1);
        root.set_hexpand(false);

        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        head.add_css_class("calc-history-head");
        let title = gtk::Label::new(Some("History"));
        title.add_css_class("calc-history-title");
        title.set_hexpand(true);
        title.set_xalign(0.0);
        head.append(&title);
        let clear_btn = gtk::Button::with_label("Clear");
        clear_btn.add_css_class("flat");
        clear_btn.add_css_class("calc-ghost");
        clear_btn.set_valign(gtk::Align::Center);
        clear_btn.set_tooltip_text(Some("Clear every calculation, pinned ones too"));
        head.append(&clear_btn);
        root.append(&head);

        let search_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        search_box.add_css_class("calc-history-search");
        let search_entry = gtk::SearchEntry::new();
        search_entry.set_hexpand(true);
        search_entry.add_css_class("sidebar-search");
        search_entry.set_placeholder_text(Some("Search calculations"));
        search_box.append(&search_entry);
        root.append(&search_box);

        let list_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        list_box.add_css_class("calc-history-list");
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::External)
            .vexpand(true)
            .child(&list_box)
            .build();
        root.append(&scroll);

        let sidebar = Rc::new_cyclic(|this| {
            RefCell::new(Self {
                root,
                list_box,
                search_entry: search_entry.clone(),
                db: db.clone(),
                on_restore: Rc::new(on_restore),
                on_toast: Rc::new(on_toast),
                this: this.clone(),
            })
        });

        let weak = Rc::downgrade(&sidebar);
        search_entry.connect_search_changed(move |_| {
            if let Some(s) = weak.upgrade() {
                s.borrow().refresh();
            }
        });

        let weak_clear = Rc::downgrade(&sidebar);
        clear_btn.connect_clicked(move |_| {
            if let Some(s) = weak_clear.upgrade() {
                let _ = s.borrow().db.clear_history();
                s.borrow().refresh();
                (s.borrow().on_toast)("History cleared");
            }
        });

        sidebar.borrow().refresh();
        sidebar
    }

    pub fn refresh(&self) {
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        let query = self.search_entry.text().to_string();
        let query_opt = if query.trim().is_empty() { None } else { Some(query.as_str()) };
        let Ok(entries) = self.db.list_history(query_opt, 50, 0) else { return };

        if entries.is_empty() {
            let empty = if query_opt.is_some() {
                nyx_ui::widgets::empty_state("system-search-symbolic", "No matches", "No calculation has that in it.", None)
            } else {
                nyx_ui::widgets::empty_state(
                    "document-open-recent-symbolic",
                    "No calculations yet",
                    "Sums you work out show here, newest first.",
                    None,
                )
            };
            empty.add_css_class("calc-history-empty");
            self.list_box.append(&empty);
            return;
        }

        let mut heading: Option<&'static str> = None;
        for entry in &entries {
            let section = if entry.is_pinned { "Pinned" } else { day_heading(entry.timestamp) };
            if heading != Some(section) {
                heading = Some(section);
                let lab = gtk::Label::new(Some(section));
                lab.add_css_class("calc-history-lab");
                lab.set_xalign(0.0);
                self.list_box.append(&lab);
            }
            self.list_box.append(&self.create_row(entry));
        }
    }

    fn create_row(&self, entry: &HistoryEntry) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.add_css_class("calc-history-row");
        if entry.is_pinned {
            row.add_css_class("pinned");
        }

        // Shown while the pointer is on the row (style.css), in the room the
        // right-aligned sum leaves at its start.
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        actions.add_css_class("calc-history-actions");
        actions.set_valign(gtk::Align::Center);
        let action = |icon: &str, tip: &str| {
            let b = nyx_ui::widgets::icon_button(icon, tip);
            b.add_css_class("history-action-btn");
            b.set_focusable(false);
            actions.append(&b);
            b
        };
        let pin_btn = action(crate::icons::PIN, if entry.is_pinned { "Unpin" } else { "Pin to top" });
        let copy_btn = action("edit-copy-symbolic", "Copy result");
        let del_btn = action("user-trash-symbolic", "Delete calculation");
        row.append(&actions);

        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        let expr = gtk::Label::new(Some(&entry.expression));
        expr.add_css_class("calc-history-expr");
        expr.set_xalign(1.0);
        expr.set_ellipsize(pango::EllipsizeMode::Start);
        text.append(&expr);
        let res = gtk::Label::new(Some(&format!("= {}", entry.result)));
        res.add_css_class("calc-history-res");
        res.set_xalign(1.0);
        res.set_ellipsize(pango::EllipsizeMode::Start);
        text.append(&res);
        row.append(&text);
        if entry.is_pinned {
            let pin = gtk::Image::from_icon_name(crate::icons::PIN);
            pin.add_css_class("calc-history-pin");
            pin.set_valign(gtk::Align::Start);
            row.append(&pin);
        }

        let (db, id, this) = (self.db.clone(), entry.id, self.this.clone());
        pin_btn.connect_clicked(move |_| {
            let _ = db.toggle_pin(id);
            if let Some(s) = this.upgrade() {
                // After the click: the list it belongs to is rebuilt.
                glib::idle_add_local_once(move || s.borrow().refresh());
            }
        });
        let (result, toast) = (entry.result.clone(), self.on_toast.clone());
        copy_btn.connect_clicked(move |_| {
            if let Some(display) = gdk::Display::default() {
                display.clipboard().set_text(&result);
                toast(&format!("Copied {result}"));
            }
        });
        let (db, this) = (self.db.clone(), self.this.clone());
        del_btn.connect_clicked(move |_| {
            let _ = db.delete_history(id);
            if let Some(s) = this.upgrade() {
                glib::idle_add_local_once(move || s.borrow().refresh());
            }
        });

        let gesture = gtk::GestureClick::new();
        let on_restore = self.on_restore.clone();
        let formula = entry.raw_result.clone();
        gesture.connect_released(move |g, _, x, y| {
            // A click on one of the row's buttons isn't a click on the row.
            if let Some(w) = g.widget()
                && w.pick(x, y, gtk::PickFlags::DEFAULT).is_some_and(|p| p.ancestor(gtk::Button::static_type()).is_some())
            {
                return;
            }
            on_restore(formula.clone());
        });
        row.add_controller(gesture);
        row.set_tooltip_text(Some("Use this result"));
        row
    }
}

/// "Today", "Yesterday" or "Earlier" for a Unix time, in local time.
fn day_heading(timestamp: i64) -> &'static str {
    let (Ok(when), Ok(now)) = (glib::DateTime::from_unix_local(timestamp), glib::DateTime::now_local()) else { return "Earlier" };
    let day = |d: &glib::DateTime| (d.year(), d.day_of_year());
    if day(&when) == day(&now) {
        "Today"
    } else if now.add_days(-1).is_ok_and(|y| day(&y) == day(&when)) {
        "Yesterday"
    } else {
        "Earlier"
    }
}
