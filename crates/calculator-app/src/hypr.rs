//! Where this process's windows are under Hyprland, and moving them. A Wayland
//! client can't place its own windows, so the calculator and the tool panel beside
//! it are placed through hyprctl, only ever by their own address or class.

use std::process::Command;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
}

/// A window of this process, in layout coordinates.
pub struct Client {
    pub address: String,
    pub rect: Rect,
    pub monitor: i64,
}

/// A monitor's whole box and the part of it windows may use (bars left out).
pub struct Monitor {
    pub rect: Rect,
    pub work: Rect,
}

pub fn available() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
}

fn json(what: &str) -> Option<serde_json::Value> {
    let output = Command::new("hyprctl").args([what, "-j"]).output().ok()?;
    serde_json::from_slice(&output.stdout).ok()
}

fn pair(v: &serde_json::Value, key: &str) -> Option<(i32, i32)> {
    let a = v.get(key)?.as_array()?;
    Some((a.first()?.as_i64()? as i32, a.get(1)?.as_i64()? as i32))
}

/// This process's mapped window with `class`.
pub fn client(class: &str) -> Option<Client> {
    let pid = u64::from(std::process::id());
    json("clients")?.as_array()?.iter().find_map(|c| {
        let mine = c.get("pid")?.as_u64()? == pid && c.get("class")?.as_str()? == class && c.get("mapped")?.as_bool()?;
        if !mine {
            return None;
        }
        let (x, y) = pair(c, "at")?;
        let (w, h) = pair(c, "size")?;
        Some(Client {
            address: format!("address:{}", c.get("address")?.as_str()?),
            rect: Rect { x, y, w, h },
            monitor: c.get("monitor")?.as_i64()?,
        })
    })
}

pub fn monitor(id: i64) -> Option<Monitor> {
    json("monitors")?.as_array()?.iter().find_map(|m| {
        if m.get("id")?.as_i64()? != id {
            return None;
        }
        let scale = m.get("scale")?.as_f64()?.max(0.1);
        let (mut w, mut h) = (m.get("width")?.as_f64()? / scale, m.get("height")?.as_f64()? / scale);
        if m.get("transform")?.as_i64()? % 2 == 1 {
            std::mem::swap(&mut w, &mut h);
        }
        let rect = Rect { x: m.get("x")?.as_i64()? as i32, y: m.get("y")?.as_i64()? as i32, w: w as i32, h: h as i32 };
        let r: Vec<i32> = m.get("reserved")?.as_array()?.iter().filter_map(|v| v.as_i64().map(|v| v as i32)).collect();
        let [left, top, right, bottom] = r[..] else { return None };
        let work = Rect { x: rect.x + left, y: rect.y + top, w: rect.w - left - right, h: rect.h - top - bottom };
        Some(Monitor { rect, work })
    })
}

/// Runs one dispatcher (`hl.dsp.…`).
pub fn dispatch(lua: &str) {
    let _ = Command::new("hyprctl").args(["dispatch", lua]).output();
}

/// Evaluates Lua in Hyprland's config; true when Hyprland took it.
pub fn eval(lua: &str) -> bool {
    Command::new("hyprctl").args(["eval", lua]).output().is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "ok")
}

pub fn move_to(address: &str, x: i32, y: i32) {
    dispatch(&format!(r#"hl.dsp.window.move({{ window = "{address}", x = {x}, y = {y} }})"#));
}

/// `class` as a Hyprland class pattern, without backslashes (they'd need escaping in Lua).
pub fn class_pattern(class: &str) -> String {
    class.replace('.', "[.]")
}
