use super::{KeyAction, backspace_btn, grid, make_btn};
use gtk::prelude::*;
use std::rc::Rc;

pub struct StandardKeypad {
    pub root: gtk::Grid,
}

impl StandardKeypad {
    pub fn new(on_action: impl Fn(KeyAction) + 'static) -> Self {
        let on_action: Rc<dyn Fn(KeyAction)> = Rc::new(on_action);
        let root = grid(6);
        root.add_css_class("calc-keypad-std");
        let key = |label: &str, class: &str, action: KeyAction| make_btn(label, class, action, on_action.clone());

        // Row 0: %, CE, C, ⌫
        root.attach(&key("%", "calc-btn-fn", KeyAction::Percent), 0, 0, 1, 1);
        root.attach(&key("CE", "calc-btn-fn", KeyAction::ClearEntry), 1, 0, 1, 1);
        root.attach(&key("C", "calc-btn-fn", KeyAction::Clear), 2, 0, 1, 1);
        root.attach(&backspace_btn(on_action.clone()), 3, 0, 1, 1);

        // Row 1: 1/x, x², √x, ÷
        root.attach(&key("1/x", "calc-btn-fn", KeyAction::Reciprocal), 0, 1, 1, 1);
        root.attach(&key("x²", "calc-btn-fn", KeyAction::Square), 1, 1, 1, 1);
        root.attach(&key("√x", "calc-btn-fn", KeyAction::Sqrt), 2, 1, 1, 1);
        root.attach(&key("÷", "calc-btn-op", KeyAction::OpDiv), 3, 1, 1, 1);

        // Rows 2–4: digits and the other operators
        let rows: [([char; 3], &str, KeyAction); 3] = [
            (['7', '8', '9'], "×", KeyAction::OpMul),
            (['4', '5', '6'], "−", KeyAction::OpSub),
            (['1', '2', '3'], "+", KeyAction::OpAdd),
        ];
        for (r, (digits, op, action)) in rows.into_iter().enumerate() {
            for (c, d) in digits.into_iter().enumerate() {
                root.attach(&key(&d.to_string(), "calc-btn-digit", KeyAction::Digit(d)), c as i32, r as i32 + 2, 1, 1);
            }
            root.attach(&key(op, "calc-btn-op", action), 3, r as i32 + 2, 1, 1);
        }

        // Row 5: ±, 0, ., =
        root.attach(&key("±", "calc-btn-digit", KeyAction::Negate), 0, 5, 1, 1);
        root.attach(&key("0", "calc-btn-digit", KeyAction::Digit('0')), 1, 5, 1, 1);
        root.attach(&key(".", "calc-btn-digit", KeyAction::Dot), 2, 5, 1, 1);
        root.attach(&key("=", "calc-btn-equals", KeyAction::Equals), 3, 5, 1, 1);

        Self { root }
    }
}
