use super::{KeyAction, backspace_btn, grid as key_grid, make_btn};
use calculator_core::programmer::{ProgrammerState, Radix, Signedness, WordWidth};
use gtk::prelude::*;
use std::rc::Rc;

const WIDTHS: [(WordWidth, &str); 4] =
    [(WordWidth::Qword, "64-bit"), (WordWidth::Dword, "32-bit"), (WordWidth::Word, "16-bit"), (WordWidth::Byte, "8-bit")];
const SIGNS: [(Signedness, &str); 2] = [(Signedness::Signed, "Signed"), (Signedness::Unsigned, "Unsigned")];

pub struct ProgrammerKeypad {
    pub root: gtk::Box,
    rows: Vec<(Radix, gtk::Box, gtk::Label)>,
    word_label: gtk::Label,
    sign_label: gtk::Label,
    bit_buttons: Vec<gtk::Button>,
    digit_buttons: Vec<(char, gtk::Button)>,
}

impl ProgrammerKeypad {
    pub fn new(on_action: impl Fn(KeyAction) + 'static) -> Self {
        let on_action: Rc<dyn Fn(KeyAction)> = Rc::new(on_action);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);

        // Every base, the one keys type in large; a click picks it.
        let radix_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        radix_box.add_css_class("calc-radix-box");
        let mut rows = Vec::new();
        for (name, radix) in [("HEX", Radix::Hex), ("DEC", Radix::Dec), ("OCT", Radix::Oct), ("BIN", Radix::Bin)] {
            let (row, value) = make_radix_row(name, radix, on_action.clone());
            radix_box.append(&row);
            rows.push((radix, row, value));
        }
        root.append(&radix_box);

        // The word size and signedness, then how to use the bits
        let ctrl_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        ctrl_row.add_css_class("calc-ctrl-row");
        let (word_btn, word_label) = picker("Word size");
        let (cb, shown) = (on_action.clone(), word_label.clone());
        word_btn.connect_clicked(move |b| {
            let current = shown.text();
            let entries = WIDTHS
                .iter()
                .map(|&(w, label)| {
                    let cb = cb.clone();
                    nyx_ui::menu::check(label, current == label, move |_| cb(KeyAction::SetWordWidth(w)))
                })
                .collect();
            nyx_ui::menu::popup_below(b, entries);
        });
        ctrl_row.append(&word_btn);
        let (sign_btn, sign_label) = picker("Signed or unsigned");
        let (cb, shown) = (on_action.clone(), sign_label.clone());
        sign_btn.connect_clicked(move |b| {
            let current = shown.text();
            let entries = SIGNS
                .iter()
                .map(|&(s, label)| {
                    let cb = cb.clone();
                    nyx_ui::menu::check(label, current == label, move |_| cb(KeyAction::SetSignedness(s)))
                })
                .collect();
            nyx_ui::menu::popup_below(b, entries);
        });
        ctrl_row.append(&sign_btn);
        let hint = gtk::Label::new(Some("Click a bit to flip it"));
        hint.add_css_class("calc-bit-hint");
        hint.set_hexpand(true);
        hint.set_xalign(1.0);
        ctrl_row.append(&hint);
        root.append(&ctrl_row);

        // The 64 bits: four rows of 16, a gap between nibbles, each row's lowest bit
        // number at its end.
        let bit_grid = gtk::Grid::builder().column_spacing(2).row_spacing(0).build();
        bit_grid.add_css_class("calc-bit-grid");
        let mut bit_buttons = Vec::with_capacity(64);
        for row in 0..4 {
            for col in 0..16 {
                let bit_idx = 63 - (row * 16 + col);
                let btn = gtk::Button::with_label("0");
                btn.add_css_class("bit-btn");
                btn.set_hexpand(true);
                if col % 4 == 0 && col > 0 {
                    btn.set_margin_start(6);
                }
                btn.set_tooltip_text(Some(&format!("Bit {bit_idx}")));
                btn.set_focusable(false);
                let cb = on_action.clone();
                btn.connect_clicked(move |_| cb(KeyAction::ToggleBit(bit_idx)));
                bit_grid.attach(&btn, col as i32, row as i32, 1, 1);
                bit_buttons.push(btn);
            }
            let pos = gtk::Label::new(Some(&(48 - row * 16).to_string()));
            pos.add_css_class("calc-bit-pos");
            pos.set_xalign(1.0);
            pos.set_width_chars(3);
            bit_grid.attach(&pos, 16, row as i32, 1, 1);
        }
        root.append(&bit_grid);

        // The keys: bit operations on top, A–F at the left, the digits in the middle,
        // the operators at the right, 0 wide along the bottom.
        let grid = key_grid(6);
        grid.add_css_class("calc-keypad-prog");
        grid.set_vexpand(true);
        let fun = "calc-btn-fn";
        let op = "calc-btn-op";
        let key = |label: &str, class: &str, action: KeyAction, c: i32, r: i32, w: i32| {
            let b = make_btn(label, class, action, on_action.clone());
            grid.attach(&b, c, r, w, 1);
            b
        };
        key("AND", fun, KeyAction::BitAnd, 0, 0, 1);
        key("OR", fun, KeyAction::BitOr, 1, 0, 1);
        key("NOT", fun, KeyAction::BitNot, 2, 0, 1);
        key("XOR", fun, KeyAction::BitXor, 3, 0, 1);
        key("C", fun, KeyAction::Clear, 4, 0, 1);
        grid.attach(&backspace_btn(on_action.clone()), 5, 0, 1, 1);
        key("LSH", fun, KeyAction::Shl, 0, 1, 1);
        key("RSH", fun, KeyAction::Shr, 1, 1, 1);
        key("mod", fun, KeyAction::OpMod, 2, 1, 1);
        key("(", fun, KeyAction::LParen, 3, 1, 1);
        key(")", fun, KeyAction::RParen, 4, 1, 1);
        key("÷", op, KeyAction::OpDiv, 5, 1, 1);

        let mut digit_buttons = Vec::new();
        let rows_of_digits: [([char; 2], [char; 3], &str, KeyAction); 3] = [
            (['A', 'B'], ['7', '8', '9'], "×", KeyAction::OpMul),
            (['C', 'D'], ['4', '5', '6'], "−", KeyAction::OpSub),
            (['E', 'F'], ['1', '2', '3'], "+", KeyAction::OpAdd),
        ];
        for (r, (hex, digits, sym, action)) in rows_of_digits.into_iter().enumerate() {
            let r = r as i32 + 2;
            for (c, ch) in hex.into_iter().chain(digits).enumerate() {
                let b = key(&ch.to_string(), "calc-btn-digit", KeyAction::Digit(ch), c as i32, r, 1);
                if ch.is_ascii_alphabetic() {
                    b.add_css_class("calc-btn-hex");
                }
                digit_buttons.push((ch, b));
            }
            key(sym, op, action, 5, r, 1);
        }
        key("CE", fun, KeyAction::ClearEntry, 0, 5, 1);
        key("±", "calc-btn-digit", KeyAction::Negate, 1, 5, 1);
        let zero = key("0", "calc-btn-digit", KeyAction::Digit('0'), 2, 5, 3);
        digit_buttons.push(('0', zero));
        key("=", "calc-btn-equals", KeyAction::Equals, 5, 5, 1);
        root.append(&grid);

        Self { root, rows, word_label, sign_label, bit_buttons, digit_buttons }
    }

    pub fn update_state(&self, state: &ProgrammerState) {
        for (radix, row, value) in &self.rows {
            value.set_text(&match radix {
                Radix::Hex => trim_zeros(&state.format_hex()),
                Radix::Dec => thousands(&state.format_dec()),
                Radix::Oct => group(&state.format_oct(), 3),
                Radix::Bin => trim_nibbles(&state.format_bin()),
            });
            if *radix == state.active_radix {
                row.add_css_class("active-radix");
            } else {
                row.remove_css_class("active-radix");
            }
        }

        self.word_label.set_text(WIDTHS.iter().find(|w| w.0 == state.width).map_or("64-bit", |w| w.1));
        self.sign_label.set_text(SIGNS.iter().find(|s| s.0 == state.signedness).map_or("Signed", |s| s.1));

        let max_bits = state.width.bits();
        for (i, btn) in self.bit_buttons.iter().enumerate() {
            let bit_idx = 63 - i;
            let on = state.get_bit(bit_idx);
            btn.set_label(if on { "1" } else { "0" });
            btn.set_sensitive(bit_idx < max_bits);
            if on && bit_idx < max_bits {
                btn.add_css_class("active");
            } else {
                btn.remove_css_class("active");
            }
        }

        for (ch, btn) in &self.digit_buttons {
            btn.set_sensitive(state.active_radix.is_digit_valid(*ch));
        }
    }
}

/// A dropdown-looking button: its choice and a chevron.
fn picker(tip: &str) -> (gtk::Button, gtk::Label) {
    let button = gtk::Button::new();
    button.add_css_class("calc-picker");
    button.set_focusable(false);
    button.set_tooltip_text(Some(tip));
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let label = gtk::Label::new(None);
    content.append(&label);
    let chevron = gtk::Image::from_icon_name("pan-down-symbolic");
    chevron.add_css_class("calc-mode-chevron");
    content.append(&chevron);
    button.set_child(Some(&content));
    (button, label)
}

fn trim_zeros(s: &str) -> String {
    let t = s.trim_start_matches('0');
    if t.is_empty() { "0".into() } else { t.into() }
}

/// `-12345` as `-12,345`.
fn thousands(s: &str) -> String {
    let (sign, digits) = s.strip_prefix('-').map_or(("", s), |d| ("−", d));
    format!("{sign}{}", group(digits, 3).replace(' ', ","))
}

/// Digits in groups of `n` from the right, a space between.
fn group(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(n) {
            out.push(' ');
        }
        out.push(*c);
    }
    out
}

/// The bits from the highest nibble that has one set.
fn trim_nibbles(s: &str) -> String {
    let groups: Vec<&str> = s.split(' ').collect();
    let first = groups.iter().position(|g| g.contains('1')).unwrap_or(groups.len().saturating_sub(1));
    groups[first..].join(" ")
}

fn make_radix_row(name: &str, radix: Radix, cb: Rc<dyn Fn(KeyAction)>) -> (gtk::Box, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("calc-radix-row");

    let tag = gtk::Label::new(Some(name));
    tag.add_css_class("calc-radix-tag");
    tag.set_xalign(0.0);
    row.append(&tag);

    let val = gtk::Label::new(Some("0"));
    val.add_css_class("calc-radix-val");
    val.set_hexpand(true);
    val.set_xalign(1.0);
    val.set_ellipsize(gtk::pango::EllipsizeMode::Start);
    val.set_selectable(true);
    row.append(&val);

    let gesture = gtk::GestureClick::new();
    gesture.connect_pressed(move |_, _, _, _| {
        cb(KeyAction::SelectRadix(radix));
    });
    row.add_controller(gesture);

    (row, val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bases_read_as_drawn() {
        assert_eq!(trim_zeros("000000000000BEEF"), "BEEF");
        assert_eq!(trim_zeros("0000"), "0");
        assert_eq!(thousands("48879"), "48,879");
        assert_eq!(thousands("-1234567"), "−1,234,567");
        assert_eq!(group("137357", 3), "137 357");
        assert_eq!(group("1357", 3), "1 357");
        assert_eq!(trim_nibbles("0000 0000 1011 1110 1110 1111"), "1011 1110 1110 1111");
        assert_eq!(trim_nibbles("0000 0000"), "0000");
    }
}
