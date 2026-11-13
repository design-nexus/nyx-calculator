use super::{KeyAction, backspace_btn, grid, make_btn};
use calculator_core::scientific::SciFunc;
use gtk::prelude::*;
use std::rc::Rc;

pub struct ScientificKeypad {
    pub root: gtk::Grid,
}

impl ScientificKeypad {
    pub fn new(on_action: impl Fn(KeyAction) + 'static) -> Self {
        let on_action: Rc<dyn Fn(KeyAction)> = Rc::new(on_action);
        let root = grid(5);
        root.add_css_class("calc-keypad-sci");
        let at = |label: &str, class: &str, action: KeyAction, c: i32, r: i32| {
            root.attach(&make_btn(label, class, action, on_action.clone()), c, r, 1, 1);
        };
        let fun = "calc-btn-fn";
        let dig = "calc-btn-digit";
        let op = "calc-btn-op";

        // Row 0: sin, cos, tan, π, C, ⌫
        at("sin", fun, KeyAction::Sci(SciFunc::Sin), 0, 0);
        at("cos", fun, KeyAction::Sci(SciFunc::Cos), 1, 0);
        at("tan", fun, KeyAction::Sci(SciFunc::Tan), 2, 0);
        at("π", fun, KeyAction::ConstPi, 3, 0);
        at("C", fun, KeyAction::Clear, 4, 0);
        root.attach(&backspace_btn(on_action.clone()), 5, 0, 1, 1);

        // Row 1: asin, acos, atan, e, (, )
        at("asin", fun, KeyAction::Sci(SciFunc::Asin), 0, 1);
        at("acos", fun, KeyAction::Sci(SciFunc::Acos), 1, 1);
        at("atan", fun, KeyAction::Sci(SciFunc::Atan), 2, 1);
        at("e", fun, KeyAction::ConstE, 3, 1);
        at("(", fun, KeyAction::LParen, 4, 1);
        at(")", fun, KeyAction::RParen, 5, 1);

        // Row 2: sinh, cosh, tanh, τ, xʸ, ÷
        at("sinh", fun, KeyAction::Sci(SciFunc::Sinh), 0, 2);
        at("cosh", fun, KeyAction::Sci(SciFunc::Cosh), 1, 2);
        at("tanh", fun, KeyAction::Sci(SciFunc::Tanh), 2, 2);
        at("τ", fun, KeyAction::ConstTau, 3, 2);
        at("xʸ", fun, KeyAction::OpPow, 4, 2);
        at("÷", op, KeyAction::OpDiv, 5, 2);

        // Row 3: ln, log, 7, 8, 9, ×
        at("ln", fun, KeyAction::Sci(SciFunc::Ln), 0, 3);
        at("log", fun, KeyAction::Sci(SciFunc::Log10), 1, 3);
        at("7", dig, KeyAction::Digit('7'), 2, 3);
        at("8", dig, KeyAction::Digit('8'), 3, 3);
        at("9", dig, KeyAction::Digit('9'), 4, 3);
        at("×", op, KeyAction::OpMul, 5, 3);

        // Row 4: √x, ∛x, 4, 5, 6, −
        at("√x", fun, KeyAction::Sqrt, 0, 4);
        at("∛x", fun, KeyAction::Sci(SciFunc::Cbrt), 1, 4);
        at("4", dig, KeyAction::Digit('4'), 2, 4);
        at("5", dig, KeyAction::Digit('5'), 3, 4);
        at("6", dig, KeyAction::Digit('6'), 4, 4);
        at("−", op, KeyAction::OpSub, 5, 4);

        // Row 5: n!, 1/x, 1, 2, 3, +
        at("n!", fun, KeyAction::Sci(SciFunc::Fact), 0, 5);
        at("1/x", fun, KeyAction::Reciprocal, 1, 5);
        at("1", dig, KeyAction::Digit('1'), 2, 5);
        at("2", dig, KeyAction::Digit('2'), 3, 5);
        at("3", dig, KeyAction::Digit('3'), 4, 5);
        at("+", op, KeyAction::OpAdd, 5, 5);

        // Row 6: abs, EXP, ±, 0, ., =
        at("abs", fun, KeyAction::Sci(SciFunc::Abs), 0, 6);
        at("EXP", fun, KeyAction::ExpNotation, 1, 6);
        at("±", dig, KeyAction::Negate, 2, 6);
        at("0", dig, KeyAction::Digit('0'), 3, 6);
        at(".", dig, KeyAction::Dot, 4, 6);
        at("=", "calc-btn-equals", KeyAction::Equals, 5, 6);

        Self { root }
    }
}
