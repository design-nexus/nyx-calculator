pub mod programmer;
pub mod scientific;
pub mod standard;

pub use programmer::ProgrammerKeypad;
pub use scientific::ScientificKeypad;
pub use standard::StandardKeypad;

use calculator_core::programmer::{Radix, Signedness, WordWidth};
use calculator_core::scientific::{AngleMode, SciFunc};
use gtk::prelude::*;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Digit(char),
    Dot,
    OpAdd,
    OpSub,
    OpMul,
    OpDiv,
    OpPow,
    OpMod,
    Percent,
    Negate,
    Reciprocal,
    Square,
    Sqrt,
    LParen,
    RParen,
    Clear,
    ClearEntry,
    Backspace,
    Equals,
    MemClear,
    MemRecall,
    MemAdd,
    MemSub,
    MemStore,
    // Scientific
    Sci(SciFunc),
    ConstPi,
    ConstE,
    ConstTau,
    SetAngle(AngleMode),
    ExpNotation,
    // Programmer
    BitAnd,
    BitOr,
    BitXor,
    BitNot,
    Shl,
    Shr,
    ToggleBit(usize),
    SelectRadix(Radix),
    SetWordWidth(WordWidth),
    SetSignedness(Signedness),
}

pub fn make_btn(label: &str, extra_class: &str, action: KeyAction, cb: Rc<dyn Fn(KeyAction)>) -> gtk::Button {
    let btn = gtk::Button::with_label(label);
    btn.add_css_class("calc-btn");
    if !extra_class.is_empty() {
        btn.add_css_class(extra_class);
    }
    btn.set_focusable(false);
    btn.connect_clicked(move |_| cb(action));
    btn
}

/// The ⌫ key: the backspace icon, named for screen readers.
pub fn backspace_btn(cb: Rc<dyn Fn(KeyAction)>) -> gtk::Button {
    let btn = make_btn("", "calc-btn-fn", KeyAction::Backspace, cb);
    btn.set_icon_name(crate::icons::BACKSPACE);
    btn.add_css_class("calc-btn-bs");
    btn.set_tooltip_text(Some("Delete the last character (Backspace)"));
    btn.update_property(&[gtk::accessible::Property::Label("Backspace")]);
    btn
}

/// A key grid: equal columns and rows, `gap` px apart.
pub fn grid(gap: i32) -> gtk::Grid {
    let grid = gtk::Grid::builder().column_spacing(gap).row_spacing(gap).column_homogeneous(true).row_homogeneous(true).build();
    grid.add_css_class("calc-keypad");
    grid
}
