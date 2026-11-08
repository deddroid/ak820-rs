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
/// Default press length and pause after every action; the official driver puts ≥ 10 ms.
pub const DEFAULT_HOLD_MS: u16 = 15;
/// Most repetitions the keyboard counts by itself. The AK820 reads 254 and 255 as
/// "start/stop" and "while held", so both models stop at 253.
pub const REPEAT_MAX: u16 = 253;

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
    pub count: u16,
}

fn one() -> u16 {
    1
}

impl MacroDef {
    /// Steps and repeat count as stored in the keyboard. Past [`REPEAT_MAX`] the steps
    /// are copied inside the macro: `copies × count` = the requested repetitions, with
    /// the fewest copies. Only `repeat` uses the count.
    pub fn stored(&self) -> (Vec<String>, u8) {
        if self.mode != MacroMode::Repeat {
            return (self.steps.clone(), 1);
        }
        let n = self.count.max(1);
        let copies = (1..=n).find(|&c| n.is_multiple_of(c) && n / c <= REPEAT_MAX).unwrap_or(n);
        let count = u8::try_from(n / copies).expect("n / copies <= REPEAT_MAX < 256");
        (self.steps.iter().cloned().cycle().take(self.steps.len() * usize::from(copies)).collect(), count)
    }
}

/// One macro step, written `kind:value` in profiles. Steps that press something also
/// take how long it stays pressed: `kind@MS:value` (default [`DEFAULT_HOLD_MS`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Keys pressed together, then released: `tap:ctrl+c`.
    Tap { keys: String, hold_ms: u16 },
    /// Text typed with the US layout, each key held `hold_ms`: `text:Hello`.
    Text { text: String, hold_ms: u16 },
    /// A key pressed and left down: `down:shift`.
    Down(String),
    /// A key released: `up:shift`.
    Up(String),
    /// A pause in milliseconds: `delay:100`.
    Delay(u16),
    /// A mouse button clicked: `click:left`.
    Click { button: String, hold_ms: u16 },
    /// A mouse button pressed and left down: `mouse-down:left`.
    MouseDown(String),
    /// A mouse button released: `mouse-up:left`.
    MouseUp(String),
}

impl std::str::FromStr for Step {
    type Err = Error;

    fn from_str(step: &str) -> Result<Self, Error> {
        let (head, arg) = step
            .split_once(':')
            .ok_or_else(|| Error::Invalid(format!("macro step '{step}' must look like kind:value")))?;
        let (kind, hold) = match head.split_once('@') {
            Some((kind, ms)) => {
                let ms = ms.parse::<u16>().ok().filter(|&ms| ms > 0);
                (kind, Some(ms.ok_or_else(|| Error::Invalid(format!("bad duration in '{step}' (1..65535 ms)")))?))
            }
            None => (head, None),
        };
        let hold_ms = hold.unwrap_or(DEFAULT_HOLD_MS);
        let arg = arg.to_string();
        Ok(match kind {
            "tap" => Step::Tap { keys: arg, hold_ms },
            "text" => Step::Text { text: arg, hold_ms },
            "click" => Step::Click { button: arg, hold_ms },
            _ if hold.is_some() => return Err(Error::Invalid(format!("'{kind}' takes no duration in '{step}'"))),
            "down" => Step::Down(arg),
            "up" => Step::Up(arg),
            "delay" => {
                Step::Delay(arg.parse().map_err(|_| Error::Invalid(format!("bad delay in '{step}' (0..65535 ms)")))?)
            }
            "mouse-down" => Step::MouseDown(arg),
            "mouse-up" => Step::MouseUp(arg),
            _ => return Err(Error::Invalid(format!("unknown macro step kind '{kind}'"))),
        })
    }
}

impl std::fmt::Display for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hold = |ms: u16| if ms == DEFAULT_HOLD_MS { String::new() } else { format!("@{ms}") };
        match self {
            Step::Tap { keys, hold_ms } => write!(f, "tap{}:{keys}", hold(*hold_ms)),
            Step::Text { text, hold_ms } => write!(f, "text{}:{text}", hold(*hold_ms)),
            Step::Down(key) => write!(f, "down:{key}"),
            Step::Up(key) => write!(f, "up:{key}"),
            Step::Delay(ms) => write!(f, "delay:{ms}"),
            Step::Click { button, hold_ms } => write!(f, "click{}:{button}", hold(*hold_ms)),
            Step::MouseDown(button) => write!(f, "mouse-down:{button}"),
            Step::MouseUp(button) => write!(f, "mouse-up:{button}"),
        }
    }
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
        ('!', '1'),
        ('@', '2'),
        ('#', '3'),
        ('$', '4'),
        ('%', '5'),
        ('^', '6'),
        ('&', '7'),
        ('*', '8'),
        ('(', '9'),
        (')', '0'),
        ('_', '-'),
        ('+', '='),
        ('{', '['),
        ('}', ']'),
        ('|', '\\'),
        (':', ';'),
        ('"', '\''),
        ('~', '`'),
        ('<', ','),
        ('>', '.'),
        ('?', '/'),
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

/// Press `usages` in order, keep them down `hold_ms`, release them in reverse order.
fn press_combo(usages: &[u8], hold_ms: u16, out: &mut Vec<[u8; 4]>) {
    for (i, &u) in usages.iter().enumerate() {
        out.push(key_down(u));
        out.push(delay(if i + 1 == usages.len() { hold_ms } else { DEFAULT_HOLD_MS }));
    }
    for &u in usages.iter().rev() {
        out.push(key_up(u));
        out.push(delay(DEFAULT_HOLD_MS));
    }
}

fn key_usage(name: &str, step: &str) -> Result<u8, Error> {
    keys::usage(name).ok_or_else(|| Error::Invalid(format!("unknown key '{name}' in macro step '{step}'")))
}

/// Encode macro steps into 4-byte events.
pub fn events(steps: &[String]) -> Result<Vec<[u8; 4]>, Error> {
    let mut out = Vec::new();
    for step in steps {
        let mouse =
            |b: &str| keys::mouse_button(b).ok_or_else(|| Error::Invalid(format!("unknown mouse button in '{step}'")));
        match step.parse::<Step>()? {
            Step::Text { text, hold_ms } => {
                for c in text.chars() {
                    let (shift, name) = char_key(c).ok_or_else(|| {
                        Error::Invalid(format!("character '{c}' cannot be typed by a macro (US layout only)"))
                    })?;
                    let u = key_usage(&name, step)?;
                    let combo = if shift { vec![0xE1, u] } else { vec![u] };
                    press_combo(&combo, hold_ms, &mut out);
                }
            }
            Step::Tap { keys, hold_ms } => {
                let usages = keys.split('+').map(|k| key_usage(k, step)).collect::<Result<Vec<_>, _>>()?;
                press_combo(&usages, hold_ms, &mut out);
            }
            Step::Down(key) => {
                out.push(key_down(key_usage(&key, step)?));
                out.push(delay(DEFAULT_HOLD_MS));
            }
            Step::Up(key) => {
                out.push(key_up(key_usage(&key, step)?));
                out.push(delay(DEFAULT_HOLD_MS));
            }
            Step::Delay(ms) => out.push(delay(ms)),
            Step::Click { button, hold_ms } => {
                let b = mouse(&button)?;
                out.extend([mouse_down(b), delay(hold_ms), mouse_up(b), delay(DEFAULT_HOLD_MS)]);
            }
            Step::MouseDown(button) => out.extend([mouse_down(mouse(&button)?), delay(DEFAULT_HOLD_MS)]),
            Step::MouseUp(button) => out.extend([mouse_up(mouse(&button)?), delay(DEFAULT_HOLD_MS)]),
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
    fn hold_durations() {
        // the default duration is not written, so old profiles keep their bytes
        assert_eq!(events(&["tap@15:ctrl+c".into()]).unwrap(), events(&["tap:ctrl+c".into()]).unwrap());
        let e = events(&["tap@80:ctrl+c".into()]).unwrap();
        assert_eq!(e[1], delay(DEFAULT_HOLD_MS)); // between ctrl and c
        assert_eq!(e[3], delay(80)); // c held
        let e = events(&["click@200:left".into()]).unwrap();
        assert_eq!(e[1], delay(200));
        assert!(events(&["delay@5:100".into()]).is_err());
        assert!(events(&["tap@0:a".into()]).is_err());
        for text in ["tap@80:ctrl+c", "text:a:b@c", "delay:100", "click:left", "mouse-up:right", "text@40:Hi"] {
            assert_eq!(text.parse::<Step>().unwrap().to_string(), text);
        }
    }

    #[test]
    fn repeat_past_the_keyboard_counter() {
        let def = |count| MacroDef { steps: vec!["click:left".into()], mode: MacroMode::Repeat, count };
        assert_eq!(def(10).stored(), (vec!["click:left".to_string()], 10));
        assert_eq!(def(253).stored().1, 253);
        let (steps, count) = def(1000).stored();
        assert_eq!((steps.len(), count), (4, 250));
        let (steps, count) = def(257).stored(); // prime: every repetition is a copy
        assert_eq!((steps.len(), count), (257, 1));
        let once = MacroDef { mode: MacroMode::Once, ..def(40) };
        assert_eq!(once.stored().1, 1);
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
