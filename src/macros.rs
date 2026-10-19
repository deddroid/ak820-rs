//! Macros stored in the keyboard and played by a remapped key (`06 id mode count`).
//!
//! Storage block (max 3584 bytes, shared by both layers):
//! - bytes 0..400: 100 index entries × 4 bytes, bytes 0..2 = LE offset of macro *i*
//! - from 400: per macro, event count (LE u16) + 6 zero bytes + 4-byte events
//! - padded to whole packets, `AA 55` in the last two bytes.

use serde::{Deserialize, Serialize};

use crate::keys;
use crate::Error;

pub const MAX_MACROS: usize = 100;
pub const BLOCK_MAX: usize = 0xE00;
const DATA_START: usize = 400;
/// Pause after every action; the official driver puts ≥ 10 ms.
const STEP_DELAY_MS: u16 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MacroMode {
    /// Play once per press.
    #[default]
    Once,
    /// Play `count` times.
    Repeat,
    /// Start on a press, stop on the next.
    Toggle,
    /// Repeat while held.
    Hold,
    /// Each press sends the next event of the macro.
    Step,
}

impl MacroMode {
    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "once" => Some(Self::Once),
            "repeat" => Some(Self::Repeat),
            "toggle" => Some(Self::Toggle),
            "hold" => Some(Self::Hold),
            "step" => Some(Self::Step),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroDef {
    /// `text:Hello`, `tap:ctrl+c`, `down:shift`, `up:shift`, `delay:100`,
    /// `click:left`, `mouse-down:left`, `mouse-up:left`.
    pub steps: Vec<String>,
    #[serde(default)]
    pub mode: MacroMode,
    /// Repetitions for `repeat`.
    #[serde(default = "one")]
    pub count: u8,
}

fn one() -> u8 {
    1
}

fn key_down(u: u8) -> [u8; 4] {
    [0, 0, u, 0xB0]
}
fn key_up(u: u8) -> [u8; 4] {
    [0, 0, u, 0x30]
}
fn mouse_down(b: u8) -> [u8; 4] {
    [0, 0, b, 0x90]
}
fn mouse_up(b: u8) -> [u8; 4] {
    [0, 0, b, 0x10]
}
fn delay(ms: u16) -> [u8; 4] {
    let [lo, hi] = ms.to_le_bytes();
    [lo, hi, 0, 0x50]
}

/// US-layout keys for a character: `(needs shift, key name)`.
fn char_key(c: char) -> Option<(bool, String)> {
    const SHIFTED: &[(char, char)] = &[
        ('!', '1'), ('@', '2'), ('#', '3'), ('$', '4'), ('%', '5'), ('^', '6'), ('&', '7'),
        ('*', '8'), ('(', '9'), (')', '0'), ('_', '-'), ('+', '='), ('{', '['), ('}', ']'),
        ('|', '\\'), (':', ';'), ('"', '\''), ('~', '`'), ('<', ','), ('>', '.'), ('?', '/'),
    ];
    match c {
        ' ' => Some((false, "space".into())),
        '\n' => Some((false, "enter".into())),
        '\t' => Some((false, "tab".into())),
        c if c.is_ascii_uppercase() => Some((true, c.to_ascii_lowercase().to_string())),
        c => match SHIFTED.iter().find(|(s, _)| *s == c) {
            Some(&(_, base)) => Some((true, base.to_string())),
            None if keys::usage(&c.to_string()).is_some() => Some((false, c.to_string())),
            None => None,
        },
    }
}

fn press_combo(usages: &[u8], out: &mut Vec<[u8; 4]>) {
    for &u in usages {
        out.push(key_down(u));
        out.push(delay(STEP_DELAY_MS));
    }
    for &u in usages.iter().rev() {
        out.push(key_up(u));
        out.push(delay(STEP_DELAY_MS));
    }
}

fn key_usage(name: &str, step: &str) -> Result<u8, Error> {
    keys::usage(name).ok_or_else(|| Error::Invalid(format!("unknown key '{name}' in macro step '{step}'")))
}

/// Encode macro steps into 4-byte events.
pub fn events(steps: &[String]) -> Result<Vec<[u8; 4]>, Error> {
    let mut out = Vec::new();
    for step in steps {
        let (kind, arg) = step
            .split_once(':')
            .ok_or_else(|| Error::Invalid(format!("macro step '{step}' must look like kind:value")))?;
        match kind {
            "text" => {
                for c in arg.chars() {
                    let (shift, name) = char_key(c).ok_or_else(|| {
                        Error::Invalid(format!("character '{c}' cannot be typed by a macro (US layout only)"))
                    })?;
                    let u = key_usage(&name, step)?;
                    let combo = if shift { vec![0xE1, u] } else { vec![u] };
                    press_combo(&combo, &mut out);
                }
            }
            "tap" => {
                let usages = arg.split('+').map(|k| key_usage(k, step)).collect::<Result<Vec<_>, _>>()?;
                press_combo(&usages, &mut out);
            }
            "down" => {
                out.push(key_down(key_usage(arg, step)?));
                out.push(delay(STEP_DELAY_MS));
            }
            "up" => {
                out.push(key_up(key_usage(arg, step)?));
                out.push(delay(STEP_DELAY_MS));
            }
            "delay" => {
                let ms: u16 = arg
                    .parse()
                    .map_err(|_| Error::Invalid(format!("bad delay in '{step}' (0..65535 ms)")))?;
                out.push(delay(ms));
            }
            "click" | "mouse-down" | "mouse-up" => {
                let b = keys::mouse_button(arg)
                    .ok_or_else(|| Error::Invalid(format!("unknown mouse button in '{step}'")))?;
                if kind != "mouse-up" {
                    out.push(mouse_down(b));
                    out.push(delay(STEP_DELAY_MS));
                }
                if kind != "mouse-down" {
                    out.push(mouse_up(b));
                    out.push(delay(STEP_DELAY_MS));
                }
            }
            _ => return Err(Error::Invalid(format!("unknown macro step kind '{kind}'"))),
        }
    }
    if out.is_empty() {
        return Err(Error::Invalid("empty macro".into()));
    }
    Ok(out)
}

/// Bytes used by index + bodies, before padding.
pub fn data_len(macros: &[Vec<[u8; 4]>]) -> usize {
    DATA_START + macros.iter().map(|e| 8 + 4 * e.len()).sum::<usize>()
}

/// Build the storage block, padded to a multiple of `unit` (64 wired, 28 over
/// the 2.4 GHz receiver). `None` if it does not fit.
pub fn block(macros: &[Vec<[u8; 4]>], unit: usize) -> Option<Vec<u8>> {
    if macros.len() > MAX_MACROS {
        return None;
    }
    let mut b = vec![0u8; BLOCK_MAX];
    let mut pos = DATA_START;
    for (i, ev) in macros.iter().enumerate() {
        let body = 8 + 4 * ev.len();
        if pos + body > BLOCK_MAX {
            return None;
        }
        b[i * 4..i * 4 + 2].copy_from_slice(&(pos as u16).to_le_bytes());
        b[pos..pos + 2].copy_from_slice(&(ev.len() as u16).to_le_bytes());
        for (j, e) in ev.iter().enumerate() {
            b[pos + 8 + 4 * j..pos + 12 + 4 * j].copy_from_slice(e);
        }
        pos += body;
    }
    // Same padding rule as the official driver: always leaves room for `AA 55`.
    let units = pos / unit + if pos.is_multiple_of(unit) { 1 } else { 2 };
    let len = units * unit;
    if len > BLOCK_MAX {
        return None;
    }
    b.truncate(len);
    b[len - 2] = 0xAA;
    b[len - 1] = 0x55;
    Some(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_events() {
        let e = events(&["text:Hi".into()]).unwrap();
        // H = shift down, h down, h up, shift up; i = down, up; each followed by a delay
        assert_eq!(e.len(), 12);
        assert_eq!(e[0], [0, 0, 0xE1, 0xB0]);
        assert_eq!(e[1], [15, 0, 0, 0x50]);
        assert_eq!(e[2], [0, 0, 0x0B, 0xB0]);
        assert_eq!(e[4], [0, 0, 0x0B, 0x30]);
        assert_eq!(e[6], [0, 0, 0xE1, 0x30]);
        assert!(events(&["text:è".into()]).is_err());
    }

    #[test]
    fn other_steps() {
        let e = events(&["delay:300".into(), "click:left".into(), "tap:ctrl+c".into()]).unwrap();
        assert_eq!(e[0], [0x2C, 0x01, 0, 0x50]);
        assert_eq!(e[1], [0, 0, 1, 0x90]);
        assert_eq!(e[3], [0, 0, 1, 0x10]);
        assert_eq!(e[5], [0, 0, 0xE0, 0xB0]);
    }

    #[test]
    fn block_layout() {
        let m = vec![events(&["text:a".into()]).unwrap()];
        let b = block(&m, 64).unwrap();
        assert_eq!(&b[0..2], &400u16.to_le_bytes());
        assert_eq!(&b[400..402], &4u16.to_le_bytes());
        assert_eq!(&b[408..412], &[0, 0, 0x04, 0xB0]);
        assert_eq!(b.len() % 64, 0);
        assert_eq!(&b[b.len() - 2..], &[0xAA, 0x55]);
        assert_eq!(data_len(&m), 424);
        assert_eq!(b.len(), 512);
        // empty block = just the index
        assert_eq!(block(&[], 64).unwrap().len(), 512);
        assert_eq!(block(&m, 28).unwrap().len() % 28, 0);
    }

    #[test]
    fn too_big() {
        let big = vec![events(&[format!("text:{}", "a".repeat(250))]).unwrap()];
        assert!(block(&big, 64).is_none());
    }
}
