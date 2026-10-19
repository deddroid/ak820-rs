//! Higher-level operations shared by the CLI and the GUI.

use std::path::Path;

use chrono::{Datelike, Local, NaiveDateTime, Timelike};

use crate::device::{Connection, Keyboard};
use crate::keys;
use crate::lighting::Mode;
use crate::profile::{KeyColors, Profile};
use crate::protocol::ClockTime;
use crate::screen::{self, Crop, Fit, Frame};
use crate::{Error, Result, Rgb};

pub fn now() -> ClockTime {
    to_clock(Local::now().naive_local())
}

pub fn to_clock(t: NaiveDateTime) -> ClockTime {
    ClockTime {
        year: t.year() as u16,
        month: t.month() as u8,
        day: t.day() as u8,
        hour: t.hour() as u8,
        minute: t.minute() as u8,
        second: t.second() as u8,
        weekday: t.weekday().num_days_from_sunday() as u8,
    }
}

pub fn send_keymap(kb: &Keyboard, p: &Profile) -> Result<()> {
    let km = p.keymap.clone().unwrap_or_default().compile(&p.macros)?;
    kb.set_keymaps(&km.normal, &km.fn_layer, &km.macros)
}

pub fn send_key_colors(kb: &Keyboard, c: &KeyColors) -> Result<()> {
    kb.set_key_colors(c.brightness, &c.resolve()?)
}

pub fn load_frames(file: &Path, fit: Fit, crop: Crop) -> Result<Vec<Frame>> {
    if !file.is_file() {
        return Err(Error::Invalid(format!("image {} not found", file.display())));
    }
    screen::load(file, fit, crop).map_err(|e| Error::Invalid(format!("cannot load {}: {e}", file.display())))
}

pub fn upload_frames(kb: &Keyboard, frames: &[Frame], progress: impl FnMut(usize, usize)) -> Result<()> {
    kb.upload_screen(&screen::blocks(frames), progress)
}

/// Send every section of `p` that is set, reporting each step through `log`.
pub fn apply(
    kb: &Keyboard,
    p: &Profile,
    log: &mut dyn FnMut(String),
    progress: &mut dyn FnMut(usize, usize),
) -> Result<()> {
    if let Some(s) = &p.system {
        log("system settings".into());
        kb.set_system(&s.settings()?)?;
    }
    if p.keymap.is_some() {
        log("key remapping and macros".into());
        send_keymap(kb, p)?;
    }
    let custom = p.lighting.as_ref().is_none_or(|l| l.mode == Mode::Custom);
    match (&p.key_colors, &p.lighting) {
        (Some(c), _) if custom => {
            if kb.connection() == Connection::Wired {
                log("per-key colors".into());
                send_key_colors(kb, c)?;
            } else {
                log("per-key colors skipped: they need the USB cable".into());
            }
        }
        (_, Some(l)) => {
            log(format!("lighting: {}", l.mode.name()));
            kb.set_lighting(l)?;
        }
        _ => {}
    }
    // The clock switches the screen back to the clock face, so it goes before the image.
    if p.sync_clock == Some(true) {
        log("clock".into());
        kb.set_clock(&now())?;
    }
    if let Some(s) = &p.screen {
        if kb.connection() == Connection::Wired {
            log(format!("screen: {}", s.image.display()));
            upload_frames(kb, &load_frames(&s.image, s.fit, s.crop)?, &mut *progress)?;
        } else {
            log("screen skipped: it needs the USB cable".into());
        }
    }
    Ok(())
}

/// `(light index, x 0..1, row 0..1)` of every key, from its rank in its row.
pub fn key_grid() -> Vec<(u8, f32, f32)> {
    const ROW_ENDS: &[&str] = &["delete", "home", "pgup", "pgdn", "up", "right"];
    let mut rows: Vec<Vec<u8>> = vec![vec![]];
    for (name, idx) in keys::all_lights() {
        rows.last_mut().unwrap().push(idx);
        if ROW_ENDS.contains(&name) {
            rows.push(vec![]);
        }
    }
    rows.retain(|r| !r.is_empty());
    let nrows = rows.len();
    rows.iter()
        .enumerate()
        .flat_map(|(y, r)| {
            let n = r.len();
            r.iter()
                .enumerate()
                .map(move |(x, &i)| (i, x as f32 / (n - 1) as f32, y as f32 / (nrows - 1) as f32))
        })
        .collect()
}

pub fn hsv(h: f32) -> Rgb {
    let h = h.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    Rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// One frame of the rainbow demo at time `t` seconds.
pub fn rainbow_frame(grid: &[(u8, f32, f32)], t: f32) -> Vec<(u8, Rgb)> {
    grid.iter().map(|&(i, x, y)| (i, hsv(x * 0.8 + y * 0.2 - t * 0.3))).collect()
}
