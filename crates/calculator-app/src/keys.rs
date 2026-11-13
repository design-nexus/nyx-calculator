use crate::keypad::KeyAction;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;

pub fn install_calculator_keys(
    window: &gtk::ApplicationWindow,
    on_action: impl Fn(KeyAction) + 'static,
    on_copy: impl Fn() + 'static,
    on_paste: impl Fn() + 'static,
    on_toggle_history: impl Fn() + 'static,
    on_switch_mode: impl Fn(&str) + 'static,
) {
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);

    let on_action = std::rc::Rc::new(on_action);
    let on_copy = std::rc::Rc::new(on_copy);
    let on_paste = std::rc::Rc::new(on_paste);
    let on_toggle_history = std::rc::Rc::new(on_toggle_history);
    let on_switch_mode = std::rc::Rc::new(on_switch_mode);

    let win_weak = window.downgrade();
    controller.connect_key_pressed(move |_, key, _, mods| {
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        let alt = mods.contains(gdk::ModifierType::ALT_MASK);

        // Alt+1 to Alt+5 for mode switching
        if alt {
            match key {
                gdk::Key::_1 => {
                    on_switch_mode("standard");
                    return glib::Propagation::Stop;
                }
                gdk::Key::_2 => {
                    on_switch_mode("scientific");
                    return glib::Propagation::Stop;
                }
                gdk::Key::_3 => {
                    on_switch_mode("programmer");
                    return glib::Propagation::Stop;
                }
                gdk::Key::_4 => {
                    on_switch_mode("converter");
                    return glib::Propagation::Stop;
                }
                gdk::Key::_5 => {
                    on_switch_mode("sheets");
                    return glib::Propagation::Stop;
                }
                gdk::Key::_6 => {
                    on_switch_mode("graph");
                    return glib::Propagation::Stop;
                }
                _ => {}
            }
        }

        // Check if focus is in a text input (e.g. Converter or Sheets)
        if let Some(w) = win_weak.upgrade()
            && let Some(focus) = gtk::prelude::GtkWindowExt::focus(&w)
            && (focus.is::<gtk::Editable>() || focus.is::<gtk::TextView>())
        {
            if ctrl && (key == gdk::Key::h || key == gdk::Key::H) {
                on_toggle_history();
                return glib::Propagation::Stop;
            }
            return glib::Propagation::Proceed;
        }

        if ctrl {
            match key {
                gdk::Key::c | gdk::Key::C => {
                    on_copy();
                    return glib::Propagation::Stop;
                }
                gdk::Key::v | gdk::Key::V => {
                    on_paste();
                    return glib::Propagation::Stop;
                }
                gdk::Key::h | gdk::Key::H => {
                    on_toggle_history();
                    return glib::Propagation::Stop;
                }
                gdk::Key::m | gdk::Key::M => {
                    on_action(KeyAction::MemStore);
                    return glib::Propagation::Stop;
                }
                gdk::Key::r | gdk::Key::R => {
                    on_action(KeyAction::MemRecall);
                    return glib::Propagation::Stop;
                }
                gdk::Key::l | gdk::Key::L => {
                    on_action(KeyAction::MemClear);
                    return glib::Propagation::Stop;
                }
                _ => {}
            }
        }

        match key {
            gdk::Key::_0 | gdk::Key::KP_0 => {
                on_action(KeyAction::Digit('0'));
                glib::Propagation::Stop
            }
            gdk::Key::_1 | gdk::Key::KP_1 => {
                on_action(KeyAction::Digit('1'));
                glib::Propagation::Stop
            }
            gdk::Key::_2 | gdk::Key::KP_2 => {
                on_action(KeyAction::Digit('2'));
                glib::Propagation::Stop
            }
            gdk::Key::_3 | gdk::Key::KP_3 => {
                on_action(KeyAction::Digit('3'));
                glib::Propagation::Stop
            }
            gdk::Key::_4 | gdk::Key::KP_4 => {
                on_action(KeyAction::Digit('4'));
                glib::Propagation::Stop
            }
            gdk::Key::_5 | gdk::Key::KP_5 => {
                on_action(KeyAction::Digit('5'));
                glib::Propagation::Stop
            }
            gdk::Key::_6 | gdk::Key::KP_6 => {
                on_action(KeyAction::Digit('6'));
                glib::Propagation::Stop
            }
            gdk::Key::_7 | gdk::Key::KP_7 => {
                on_action(KeyAction::Digit('7'));
                glib::Propagation::Stop
            }
            gdk::Key::_8 | gdk::Key::KP_8 => {
                on_action(KeyAction::Digit('8'));
                glib::Propagation::Stop
            }
            gdk::Key::_9 | gdk::Key::KP_9 => {
                on_action(KeyAction::Digit('9'));
                glib::Propagation::Stop
            }
            // Hex digits A-F
            gdk::Key::a | gdk::Key::A if !ctrl => {
                on_action(KeyAction::Digit('A'));
                glib::Propagation::Stop
            }
            gdk::Key::b | gdk::Key::B if !ctrl => {
                on_action(KeyAction::Digit('B'));
                glib::Propagation::Stop
            }
            gdk::Key::d | gdk::Key::D if !ctrl => {
                on_action(KeyAction::Digit('D'));
                glib::Propagation::Stop
            }
            gdk::Key::e | gdk::Key::E if !ctrl => {
                on_action(KeyAction::Digit('E'));
                glib::Propagation::Stop
            }
            gdk::Key::f | gdk::Key::F if !ctrl => {
                on_action(KeyAction::Digit('F'));
                glib::Propagation::Stop
            }
            gdk::Key::period | gdk::Key::comma | gdk::Key::KP_Decimal => {
                on_action(KeyAction::Dot);
                glib::Propagation::Stop
            }
            gdk::Key::plus | gdk::Key::KP_Add => {
                on_action(KeyAction::OpAdd);
                glib::Propagation::Stop
            }
            gdk::Key::minus | gdk::Key::KP_Subtract => {
                on_action(KeyAction::OpSub);
                glib::Propagation::Stop
            }
            gdk::Key::asterisk | gdk::Key::KP_Multiply => {
                on_action(KeyAction::OpMul);
                glib::Propagation::Stop
            }
            gdk::Key::slash | gdk::Key::KP_Divide => {
                on_action(KeyAction::OpDiv);
                glib::Propagation::Stop
            }
            gdk::Key::asciicircum => {
                on_action(KeyAction::OpPow);
                glib::Propagation::Stop
            }
            gdk::Key::percent => {
                on_action(KeyAction::Percent);
                glib::Propagation::Stop
            }
            gdk::Key::exclam => {
                on_action(KeyAction::Sci(calculator_core::scientific::SciFunc::Fact));
                glib::Propagation::Stop
            }
            gdk::Key::parenleft => {
                on_action(KeyAction::LParen);
                glib::Propagation::Stop
            }
            gdk::Key::parenright => {
                on_action(KeyAction::RParen);
                glib::Propagation::Stop
            }
            gdk::Key::ampersand => {
                on_action(KeyAction::BitAnd);
                glib::Propagation::Stop
            }
            gdk::Key::bar => {
                on_action(KeyAction::BitOr);
                glib::Propagation::Stop
            }
            gdk::Key::asciitilde => {
                on_action(KeyAction::BitNot);
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::equal | gdk::Key::KP_Equal => {
                on_action(KeyAction::Equals);
                glib::Propagation::Stop
            }
            gdk::Key::BackSpace => {
                on_action(KeyAction::Backspace);
                glib::Propagation::Stop
            }
            gdk::Key::Escape if crate::tools::open_tool().is_some() => {
                crate::tools::close();
                glib::Propagation::Stop
            }
            gdk::Key::Escape | gdk::Key::Delete => {
                on_action(KeyAction::Clear);
                glib::Propagation::Stop
            }
            gdk::Key::c | gdk::Key::C if !ctrl => {
                on_action(KeyAction::Clear);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        }
    });

    window.add_controller(controller);
}
