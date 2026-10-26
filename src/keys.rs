//! Physical key positions and assignable targets (key remapping).
//!
//! A key's position (`key_index`, also its `light_index`) comes from the
//! official driver's `rgb-keyboard.xml` (ANSI layout).

use crate::macros::{self, MacroDef, MacroMode};
use crate::Error;

/// `(name, key_index)` for each physical key, row by row. The first name of a
/// position is its canonical name; later entries are aliases.
pub const KEYS: &[(&str, u8)] = &[
    ("esc", 1), ("f1", 2), ("f2", 3), ("f3", 4), ("f4", 5), ("f5", 6), ("f6", 7), ("f7", 8),
    ("f8", 9), ("f9", 10), ("f10", 11), ("f11", 12), ("f12", 13), ("delete", 119),
    ("grave", 19), ("1", 20), ("2", 21), ("3", 22), ("4", 23), ("5", 24), ("6", 25), ("7", 26),
    ("8", 27), ("9", 28), ("0", 29), ("minus", 30), ("equal", 31), ("backspace", 103), ("home", 117),
    ("tab", 37), ("q", 38), ("w", 39), ("e", 40), ("r", 41), ("t", 42), ("y", 43), ("u", 44),
    ("i", 45), ("o", 46), ("p", 47), ("lbracket", 48), ("rbracket", 49), ("backslash", 67),
    ("pgup", 118),
    ("capslock", 55), ("a", 56), ("s", 57), ("d", 58), ("f", 59), ("g", 60), ("h", 61), ("j", 62),
    ("k", 63), ("l", 64), ("semicolon", 65), ("quote", 66), ("enter", 85), ("pgdn", 121),
    ("lshift", 73), ("z", 74), ("x", 75), ("c", 76), ("v", 77), ("b", 78), ("n", 79), ("m", 80),
    ("comma", 81), ("period", 82), ("slash", 83), ("rshift", 84), ("up", 101),
    ("lctrl", 91), ("win", 92), ("lalt", 93), ("space", 94), ("ralt", 95), ("fn", 96),
    ("rctrl", 98), ("left", 99), ("down", 100), ("right", 102),
    // aliases
    ("del", 119), ("`", 19), ("-", 30), ("=", 31), ("bksp", 103), ("[", 48), ("]", 49),
    ("\\", 67), ("pageup", 118), ("caps", 55), (";", 65), ("'", 66), ("return", 85),
    ("pagedown", 121), ("shift", 73), (",", 81), (".", 82), ("/", 83), ("ctrl", 91),
    ("super", 92), ("gui", 92), ("alt", 93),
];

/// Keys whose Fn layer cannot be remapped (`fnlayer_disable` in the official
/// XML, plus Delete, which the firmware ignores).
const FN_LOCKED: &[u8] = &[1, 103, 117, 67, 92, 119];

/// The Fn key has a LED but cannot be remapped.
const FN_KEY: u8 = 96;

/// Supported keyboard hardware model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Model {
    #[default]
    Ak820Pro,
    Ak820,
}

impl Model {
    pub fn name(self) -> &'static str {
        match self {
            Model::Ak820Pro => "AK820 Pro",
            Model::Ak820 => "AK820",
        }
    }

    pub fn has_screen(self) -> bool {
        self == Model::Ak820Pro
    }
}

/// Logical key position used for the `End` key on the non-Pro AK820.
pub const END_POS: u8 = 120;

/// Mapping between logical `key_index` (`u8`) and the AK820 (`320F:505B`) hardware matrix/LED slot (`0..128`).
pub const AK820_HW_MAP: &[(u8, usize)] = &[
    // Row 0
    (1, 0), (2, 8), (3, 16), (4, 24), (5, 32), (6, 40), (7, 48), (8, 56),
    (9, 64), (10, 72), (11, 80), (12, 88), (13, 96), (119, 114),
    // Row 1
    (19, 1), (20, 9), (21, 17), (22, 25), (23, 33), (24, 41), (25, 49), (26, 57),
    (27, 65), (28, 73), (29, 81), (30, 89), (31, 97), (103, 105), (117, 121),
    // Row 2
    (37, 2), (38, 10), (39, 18), (40, 26), (41, 34), (42, 42), (43, 50), (44, 58),
    (45, 66), (46, 74), (47, 82), (48, 90), (49, 98), (67, 106), (118, 122),
    // Row 3
    (55, 3), (56, 11), (57, 19), (58, 27), (59, 35), (60, 43), (61, 51), (62, 59),
    (63, 67), (64, 75), (65, 83), (66, 91), (85, 107), (121, 115),
    // Row 4
    (73, 4), (74, 20), (75, 28), (76, 36), (77, 44), (78, 52), (79, 60), (80, 68),
    (81, 76), (82, 84), (83, 92), (84, 108), (101, 116), (END_POS, 123),
    // Row 5
    (91, 5), (92, 13), (93, 21), (94, 45), (95, 77), (96, 85),
    (98, 93), (99, 109), (100, 117), (102, 125),
];

pub fn ak820_hw_slot(pos: u8) -> Option<usize> {
    AK820_HW_MAP.iter().find(|&&(p, _)| p == pos).map(|&(_, hw)| hw)
}

pub fn ak820_pos_from_hw_slot(hw: usize) -> Option<u8> {
    AK820_HW_MAP.iter().find(|&&(_, h)| h == hw).map(|&(p, _)| p)
}

pub fn position(name: &str) -> Option<u8> {
    let n = name.to_ascii_lowercase();
    if n == "end" {
        return Some(END_POS);
    }
    KEYS.iter().find(|(k, _)| *k == n).map(|&(_, i)| i)
}

pub fn canonical_name(index: u8) -> Option<&'static str> {
    if index == END_POS {
        return Some("end");
    }
    KEYS.iter().find(|&&(_, i)| i == index).map(|&(k, _)| k)
}

/// Every key with a LED, canonical names only.
pub fn all_lights() -> impl Iterator<Item = (&'static str, u8)> {
    let mut seen = Vec::new();
    KEYS.iter().copied().filter(move |&(_, i)| {
        if seen.contains(&i) {
            false
        } else {
            seen.push(i);
            true
        }
    })
}

/// Every key with a LED for the given model, canonical names only.
pub fn all_lights_for(model: Model) -> Vec<(&'static str, u8)> {
    let mut out: Vec<(&'static str, u8)> = all_lights().collect();
    if model == Model::Ak820 {
        if let Some(up_pos) = out.iter().position(|&(n, _)| n == "up") {
            out.insert(up_pos + 1, ("end", END_POS));
        } else {
            out.push(("end", END_POS));
        }
    }
    out
}

pub fn check_remappable(name: &str, fn_layer: bool) -> Result<u8, Error> {
    let pos = position(name).ok_or_else(|| Error::UnknownKey(name.into()))?;
    if pos == FN_KEY || (fn_layer && FN_LOCKED.contains(&pos)) {
        return Err(Error::Invalid(format!(
            "'{name}' cannot be remapped{}",
            if fn_layer { " on the Fn layer" } else { "" }
        )));
    }
    Ok(pos)
}

/// HID keyboard usage codes for target keys (and macro events).
pub fn usage(name: &str) -> Option<u8> {
    let n = name.to_ascii_lowercase();
    let n = n.as_str();
    if n.len() == 1 {
        let c = n.as_bytes()[0];
        match c {
            b'a'..=b'z' => return Some(0x04 + c - b'a'),
            b'1'..=b'9' => return Some(0x1E + c - b'1'),
            b'0' => return Some(0x27),
            _ => {}
        }
    }
    if let Some(num) = n.strip_prefix('f').and_then(|s| s.parse::<u8>().ok()) {
        return match num {
            1..=12 => Some(0x39 + num),
            13..=24 => Some(0x5B + num),
            _ => None,
        };
    }
    Some(match n {
        "enter" | "return" => 0x28,
        "esc" | "escape" => 0x29,
        "backspace" | "bksp" => 0x2A,
        "tab" => 0x2B,
        "space" => 0x2C,
        "-" | "minus" => 0x2D,
        "=" | "equal" => 0x2E,
        "[" | "lbracket" => 0x2F,
        "]" | "rbracket" => 0x30,
        "\\" | "backslash" => 0x31,
        "nonus#" => 0x32,
        ";" | "semicolon" => 0x33,
        "'" | "quote" => 0x34,
        "`" | "grave" => 0x35,
        "," | "comma" => 0x36,
        "." | "period" => 0x37,
        "/" | "slash" => 0x38,
        "capslock" | "caps" => 0x39,
        "printscreen" | "prtsc" => 0x46,
        "scrolllock" => 0x47,
        "pause" => 0x48,
        "insert" | "ins" => 0x49,
        "home" => 0x4A,
        "pgup" | "pageup" => 0x4B,
        "delete" | "del" => 0x4C,
        "end" => 0x4D,
        "pgdn" | "pagedown" => 0x4E,
        "right" => 0x4F,
        "left" => 0x50,
        "down" => 0x51,
        "up" => 0x52,
        "numlock" => 0x53,
        "nonus\\" | "iso\\" => 0x64,
        "menu" | "app" => 0x65,
        "lctrl" | "ctrl" => 0xE0,
        "lshift" | "shift" => 0xE1,
        "lalt" | "alt" => 0xE2,
        "lwin" | "win" | "gui" | "super" | "cmd" => 0xE3,
        "rctrl" => 0xE4,
        "rshift" => 0xE5,
        "ralt" | "altgr" => 0xE6,
        "rwin" | "rgui" | "rsuper" | "rcmd" => 0xE7,
        _ => return None,
    })
}

/// Canonical key name from a HID keyboard usage code.
pub fn name_from_usage(u: u8) -> Option<&'static str> {
    Some(match u {
        0x04 => "a", 0x05 => "b", 0x06 => "c", 0x07 => "d", 0x08 => "e", 0x09 => "f",
        0x0A => "g", 0x0B => "h", 0x0C => "i", 0x0D => "j", 0x0E => "k", 0x0F => "l",
        0x10 => "m", 0x11 => "n", 0x12 => "o", 0x13 => "p", 0x14 => "q", 0x15 => "r",
        0x16 => "s", 0x17 => "t", 0x18 => "u", 0x19 => "v", 0x1A => "w", 0x1B => "x",
        0x1C => "y", 0x1D => "z",
        0x1E => "1", 0x1F => "2", 0x20 => "3", 0x21 => "4", 0x22 => "5",
        0x23 => "6", 0x24 => "7", 0x25 => "8", 0x26 => "9", 0x27 => "0",
        0x28 => "enter",
        0x29 => "esc",
        0x2A => "backspace",
        0x2B => "tab",
        0x2C => "space",
        0x2D => "minus",
        0x2E => "equal",
        0x2F => "lbracket",
        0x30 => "rbracket",
        0x31 => "backslash",
        0x32 => "nonus#",
        0x33 => "semicolon",
        0x34 => "quote",
        0x35 => "grave",
        0x36 => "comma",
        0x37 => "period",
        0x38 => "slash",
        0x39 => "capslock",
        0x3A => "f1", 0x3B => "f2", 0x3C => "f3", 0x3D => "f4", 0x3E => "f5", 0x3F => "f6",
        0x40 => "f7", 0x41 => "f8", 0x42 => "f9", 0x43 => "f10", 0x44 => "f11", 0x45 => "f12",
        0x46 => "printscreen",
        0x47 => "scrolllock",
        0x48 => "pause",
        0x49 => "insert",
        0x4A => "home",
        0x4B => "pgup",
        0x4C => "delete",
        0x4D => "end",
        0x4E => "pgdn",
        0x4F => "right",
        0x50 => "left",
        0x51 => "down",
        0x52 => "up",
        0x53 => "numlock",
        0x64 => "nonus\\",
        0x65 => "menu",
        0x68 => "f13", 0x69 => "f14", 0x6A => "f15", 0x6B => "f16", 0x6C => "f17", 0x6D => "f18",
        0x6E => "f19", 0x6F => "f20", 0x70 => "f21", 0x71 => "f22", 0x72 => "f23", 0x73 => "f24",
        0xE0 => "lctrl",
        0xE1 => "lshift",
        0xE2 => "lalt",
        0xE3 => "win",
        0xE4 => "rctrl",
        0xE5 => "rshift",
        0xE6 => "ralt",
        0xE7 => "rwin",
        _ => return None,
    })
}

/// Modifier bit for combos (`02 mods key 00`).
fn modifier_bit(name: &str) -> Option<u8> {
    Some(match name {
        "ctrl" | "lctrl" => 0x01,
        "shift" | "lshift" => 0x02,
        "alt" | "lalt" | "option" => 0x04,
        "win" | "lwin" | "gui" | "super" | "cmd" => 0x08,
        "rctrl" => 0x10,
        "rshift" => 0x20,
        "ralt" | "altgr" => 0x40,
        "rwin" | "rgui" | "rsuper" | "rcmd" => 0x80,
        _ => return None,
    })
}

/// HID consumer usages.
pub const CONSUMER: &[(&str, u16)] = &[
    ("play", 0xCD),
    ("next", 0xB5),
    ("prev", 0xB6),
    ("stop", 0xB7),
    ("mute", 0xE2),
    ("vol+", 0xE9),
    ("vol-", 0xEA),
    ("bright+", 0x6F),
    ("bright-", 0x70),
    ("calc", 0x192),
    ("mail", 0x18A),
    ("browser", 0x196),
    ("mycomputer", 0x194),
    ("search", 0x221),
    ("www-home", 0x223),
    ("www-back", 0x224),
    ("www-forward", 0x225),
    ("www-refresh", 0x227),
];

fn consumer(name: &str) -> Option<u16> {
    let n = match name {
        "volup" | "volume+" => "vol+",
        "voldown" | "volume-" => "vol-",
        "previous" => "prev",
        "playpause" => "play",
        other => other,
    };
    CONSUMER.iter().find(|(k, _)| *k == n).map(|&(_, v)| v)
}

/// Mouse buttons (bitmask). The keyboard's mouse descriptor only declares 3 buttons.
pub fn mouse_button(name: &str) -> Option<u8> {
    match name {
        "left" => Some(0x01),
        "right" => Some(0x02),
        "middle" => Some(0x04),
        _ => None,
    }
}

/// Parse a key combo such as `ctrl+shift+a`, `a` or `ctrl`. Returns `(mods, usage)`.
fn combo(s: &str) -> Option<(u8, u8)> {
    let parts: Vec<&str> = s.split('+').collect();
    let (last, mods) = parts.split_last()?;
    let mut bits = 0;
    for m in mods {
        bits |= modifier_bit(m)?;
    }
    if let Some(bit) = modifier_bit(last) {
        return Some((bits | bit, 0));
    }
    let u = usage(last)?;
    (u < 0xE0).then_some((bits, u))
}

/// Encode a remap target into its 4-byte slot. Macros referenced by `text:` or
/// `macro:` are appended to `macro_list` (their id is their position there).
///
/// Grammar:
/// - `default`                 factory behaviour
/// - `b`, `ctrl+c`, `shift`    key / shortcut / modifier
/// - `play`, `vol+`, …         media (see [`CONSUMER`])
/// - `mouse:left|right|middle`, `wheel:up|down`
/// - `multi:a+s+d`             up to 3 keys at once
/// - `text:Hello`              macro typing a text (US layout)
/// - `macro:NAME`              macro defined in the profile
pub fn encode_target(
    target: &str,
    named_macros: &std::collections::BTreeMap<String, MacroDef>,
    macro_list: &mut Vec<MacroDef>,
) -> Result<[u8; 4], Error> {
    let bad = || Error::Invalid(format!("unknown target '{target}' (see `ak820 keys`)"));
    let t = target.trim();
    let lower = t.to_ascii_lowercase();

    if lower == "default" || lower.is_empty() {
        return Ok([0; 4]);
    }
    if let Some(text) = t.strip_prefix("text:") {
        let def = MacroDef { steps: vec![format!("text:{text}")], mode: MacroMode::Once, count: 1 };
        return push_macro(def, macro_list);
    }
    if let Some(name) = t.strip_prefix("macro:") {
        let def = named_macros
            .get(name)
            .ok_or_else(|| Error::Invalid(format!("macro '{name}' is not defined")))?;
        return push_macro(def.clone(), macro_list);
    }
    if let Some(keys) = lower.strip_prefix("multi:") {
        let names: Vec<&str> = keys.split('+').collect();
        // Modifiers inside a Multi-Key are sent as plain keys and ignored by the OS:
        // turn `multi:shift+a` into the combo `shift+a`.
        if names.iter().any(|n| modifier_bit(n).is_some()) {
            return combo(keys).map(|(m, u)| [2, m, u, 0]).ok_or_else(bad);
        }
        if names.is_empty() || names.len() > 3 {
            return Err(bad());
        }
        let mut slot = [7, 0, 0, 0];
        for (i, n) in names.iter().enumerate() {
            slot[i + 1] = usage(n).ok_or_else(bad)?;
        }
        return Ok(slot);
    }
    if let Some(b) = lower.strip_prefix("mouse:") {
        return mouse_button(b).map(|m| [1, 1, m, 0]).ok_or_else(bad);
    }
    if let Some(w) = lower.strip_prefix("wheel:") {
        return match w {
            "up" => Ok([1, 3, 0x01, 0]),
            "down" => Ok([1, 3, 0xFF, 0]),
            _ => Err(bad()),
        };
    }
    if let Some(v) = consumer(&lower) {
        return Ok([3, (v & 0xFF) as u8, (v >> 8) as u8, 0]);
    }
    combo(&lower).map(|(m, u)| [2, m, u, 0]).ok_or_else(bad)
}

fn push_macro(def: MacroDef, list: &mut Vec<MacroDef>) -> Result<[u8; 4], Error> {
    // Validate now so errors point at the key that uses it.
    macros::events(&def.steps)?;
    if list.len() >= macros::MAX_MACROS {
        return Err(Error::Invalid("too many macros (max 100)".into()));
    }
    let id = list.len() as u8;
    let (mode, count) = (def.mode.code(), def.count.max(1));
    list.push(def);
    Ok([6, id, mode, count])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn enc(t: &str) -> [u8; 4] {
        encode_target(t, &BTreeMap::new(), &mut Vec::new()).unwrap()
    }

    #[test]
    fn positions() {
        assert_eq!(position("esc"), Some(1));
        assert_eq!(position("PgDn"), Some(121));
        assert_eq!(position("end"), Some(END_POS));
        assert_eq!(position("\\"), Some(67));
        assert_eq!(canonical_name(121), Some("pgdn"));
        assert_eq!(canonical_name(END_POS), Some("end"));
        assert_eq!(all_lights().count(), 81);
        assert_eq!(all_lights_for(Model::Ak820Pro).len(), 81);
        assert_eq!(all_lights_for(Model::Ak820).len(), 82);
        assert_eq!(AK820_HW_MAP.len(), 82);
        assert_eq!(ak820_hw_slot(1), Some(0));
        assert_eq!(ak820_hw_slot(END_POS), Some(123));
        assert_eq!(ak820_pos_from_hw_slot(123), Some(END_POS));
    }

    #[test]
    fn usages() {
        assert_eq!(usage("a"), Some(0x04));
        assert_eq!(usage("1"), Some(0x1E));
        assert_eq!(usage("0"), Some(0x27));
        assert_eq!(usage("f12"), Some(0x45));
        assert_eq!(usage("f13"), Some(0x68));
        assert_eq!(usage("shift"), Some(0xE1));
        assert_eq!(name_from_usage(0x04), Some("a"));
        assert_eq!(name_from_usage(0x4D), Some("end"));
    }

    #[test]
    fn targets() {
        assert_eq!(enc("default"), [0, 0, 0, 0]);
        assert_eq!(enc("b"), [2, 0, 0x05, 0]);
        assert_eq!(enc("ctrl+a"), [2, 0x01, 0x04, 0]);
        assert_eq!(enc("ctrl+shift"), [2, 0x03, 0, 0]);
        assert_eq!(enc("vol+"), [3, 0xE9, 0, 0]);
        assert_eq!(enc("calc"), [3, 0x92, 0x01, 0]);
        assert_eq!(enc("mouse:right"), [1, 1, 2, 0]);
        assert_eq!(enc("wheel:down"), [1, 3, 0xFF, 0]);
        assert_eq!(enc("multi:a+s"), [7, 0x04, 0x16, 0]);
        assert_eq!(enc("multi:shift+a"), [2, 0x02, 0x04, 0]);
        assert!(encode_target("nope", &BTreeMap::new(), &mut Vec::new()).is_err());
    }

    #[test]
    fn macro_targets() {
        let mut list = Vec::new();
        let named = BTreeMap::from([(
            "copy".to_string(),
            MacroDef { steps: vec!["tap:ctrl+c".into()], mode: MacroMode::Repeat, count: 3 },
        )]);
        assert_eq!(encode_target("text:hi", &named, &mut list).unwrap(), [6, 0, 0, 1]);
        assert_eq!(encode_target("macro:copy", &named, &mut list).unwrap(), [6, 1, 1, 3]);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn fn_layer_restrictions() {
        assert!(check_remappable("esc", false).is_ok());
        assert!(check_remappable("esc", true).is_err());
        assert!(check_remappable("fn", false).is_err());
    }
}
