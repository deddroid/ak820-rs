//! Wire format of the AK820 Pro (`0x0C45`) and AK820 (`0x320F:0x505B`) stock
//! firmwares.
//!
//! Everything here is pure byte building, with no I/O, so it can be unit-tested
//! without a keyboard. See `docs/PROTOCOL.md` for the full description.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::keys::{self, Model};
use crate::lighting::{Axis, Direction, Lighting, Mode};
use crate::Rgb;

/// SONiX, the AK820 Pro MCU vendor.
pub const VID: u16 = 0x0C45;
/// USB cable. Both PIDs ship with the same protocol (`config.xml` of the official driver).
pub const WIRED_PIDS: &[u16] = &[0x8009, 0x800A];
/// 2.4 GHz USB receiver.
pub const RECEIVER_PID: u16 = 0xFDFD;
/// Bluetooth: no vendor interface, cannot be configured.
pub const BLUETOOTH_PID: u16 = 0xFEFE;

/// EVision, the AK820 (non-Pro) MCU vendor.
pub const EVISION_VID: u16 = 0x320F;
/// AK820 (non-Pro, wired RGB) USB PID.
pub const AK820_PID: u16 = 0x505B;
/// AK820 (non-Pro) vendor HID usage page and usage.
pub const EVISION_USAGE_PAGE: u16 = 0xFF1C;
pub const EVISION_USAGE: u16 = 0x0092;
pub const EVISION_INTERFACE: i32 = 1;
pub const EVISION_REPORT_ID: u8 = 0x04;
pub const EVISION_MAX_CHUNK: usize = 56;
pub const AK820_SLOTS: usize = 128;
pub const AK820_TABLE_LEN: usize = AK820_SLOTS * 3;
pub const AK820_CONFIG_LEN: usize = 29;
pub const AK820_MACRO_MAX: usize = 80 * 128;

/// Wired: 64-byte feature reports carrying commands.
pub const CONFIG_USAGE_PAGE: u16 = 0xFF13;
/// Wired: 4096-byte output reports carrying screen images.
pub const DATA_USAGE_PAGE: u16 = 0xFF68;
/// Receiver: 32-byte input/output reports.
pub const RECEIVER_USAGE_PAGE: u16 = 0xFF60;

/// Fallback interface numbers, for platforms where hidapi reports no usage page.
pub const CONFIG_INTERFACE: i32 = 3;
pub const DATA_INTERFACE: i32 = 2;
pub const RECEIVER_INTERFACE: i32 = 3;

pub const PACKET: usize = 64;
pub const SCREEN_BLOCK: usize = 4096;
pub const RF_FRAME: usize = 32;
pub const RF_CHUNK: usize = 28;

/// Key table / per-key color table: 144 slots × 4 bytes, ending in `AA 55`.
pub const TABLE_LEN: usize = 576;
pub const TABLE_SLOTS: usize = TABLE_LEN / 4;

/// Command byte (byte 1) of the `04 xx` control packets (AK820 Pro).
pub mod cmd {
    pub const BEGIN: u8 = 0x18;
    pub const COMMIT: u8 = 0x02;
    pub const SAVE: u8 = 0xF0;
    pub const LIGHTING: u8 = 0x13;
    pub const KEYMAP: u8 = 0x11;
    pub const KEYMAP_FN: u8 = 0x27;
    pub const MACRO_ERASE: u8 = 0x19;
    pub const MACRO_DATA: u8 = 0x15;
    pub const SYSTEM: u8 = 0x17;
    pub const KEY_COLORS: u8 = 0x23;
    pub const LIVE_COLORS: u8 = 0x20;
    pub const CLOCK: u8 = 0x28;
    pub const SCREEN: u8 = 0x72;
}

/// Command byte (byte 3) of the EVision 64-byte packets (AK820 `320F:505B`).
pub mod evision {
    pub const BEGIN: u8 = 0x01;
    pub const END: u8 = 0x02;
    pub const READ_CAPS: u8 = 0x03;
    pub const READ_CONFIG: u8 = 0x05;
    pub const WRITE_CONFIG: u8 = 0x06;
    pub const READ_DEFAULT_KEYMAP: u8 = 0x07;
    pub const READ_KEYMAP: u8 = 0x08;
    pub const WRITE_KEYMAP: u8 = 0x09;
    pub const READ_COLORS: u8 = 0x0A;
    pub const WRITE_COLORS: u8 = 0x0B;
    pub const LIVE_COLORS: u8 = 0x12;
    pub const WRITE_MACROS: u8 = 0x15;
    pub const READ_LEDS: u8 = 0x1B;
}

/// Order byte (byte 0) of the 2.4 GHz receiver frames.
pub mod rf {
    pub const LIGHTING: u8 = 0x05;
    pub const SYSTEM: u8 = 0x07;
    pub const MACROS: u8 = 0x09;
    pub const MUSIC: u8 = 0x0B;
    pub const CLOCK: u8 = 0x0C;
    pub const KEYMAP: u8 = 0x10;
    pub const KEYMAP_FN: u8 = 0x12;
    pub const BATTERY: u8 = 0x20;
}

pub type Packet = [u8; PACKET];

/// `[0]=04 [1]=cmd [2]=arg [8..10]=count (LE)`: count is the number of data
/// packets (or screen blocks) that follow, or a plain flag for BEGIN.
pub fn control(command: u8, arg: u8, count: u16) -> Packet {
    let mut p = [0u8; PACKET];
    p[0] = 0x04;
    p[1] = command;
    p[2] = arg;
    p[8..10].copy_from_slice(&count.to_le_bytes());
    p
}

/// The 16-bit sum the keyboard reports back on COMMIT for the data it received.
pub fn sum16(data: &[u8]) -> u16 {
    data.iter().fold(0u16, |acc, &b| acc.wrapping_add(b as u16))
}

/// Split data into zero-padded 64-byte packets.
pub fn packets(data: &[u8]) -> Vec<Packet> {
    data.chunks(PACKET)
        .map(|c| {
            let mut p = [0u8; PACKET];
            p[..c.len()].copy_from_slice(c);
            p
        })
        .collect()
}

/// Build a 576-byte table from `(slot, 4 bytes)` entries; unset slots stay zero.
pub fn table<I: IntoIterator<Item = (u8, [u8; 4])>>(entries: I) -> Vec<u8> {
    let mut t = vec![0u8; TABLE_LEN];
    for (slot, bytes) in entries {
        let i = slot as usize * 4;
        if i + 4 <= TABLE_LEN - 2 {
            t[i..i + 4].copy_from_slice(&bytes);
        }
    }
    t[TABLE_LEN - 2] = 0xAA;
    t[TABLE_LEN - 1] = 0x55;
    t
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    /// Sunday = 0.
    pub weekday: u8,
}

/// Wired clock data packet. Plain numbers, not BCD.
pub fn clock_packet(t: &ClockTime) -> Packet {
    let mut p = [0u8; PACKET];
    p[1] = 0x01;
    p[2] = 0x5A;
    p[3] = t.year.saturating_sub(2000) as u8;
    p[4] = t.month;
    p[5] = t.day;
    p[6] = t.hour;
    p[7] = t.minute;
    p[8] = t.second;
    p[10] = 0x04;
    p[62] = 0xAA;
    p[63] = 0x55;
    p
}

/// Receiver clock data: the first 14 wired bytes with the weekday in byte 10.
pub fn rf_clock_data(t: &ClockTime) -> [u8; 16] {
    let mut d = rf_short(&clock_packet(t));
    d[10] = t.weekday;
    d
}

/// The 16 bytes the receiver takes for lighting/system/clock: first 14 of the wired packet + `AA 55`.
pub fn rf_short(p: &Packet) -> [u8; 16] {
    let mut d = [0u8; 16];
    d[..14].copy_from_slice(&p[..14]);
    d[14] = 0xAA;
    d[15] = 0x55;
    d
}

/// Receiver frames: `[order, len, chunk#, ≤28 data bytes, sum of bytes 0..31]`.
/// `last_len` overrides the length byte of the final frame (used by macros,
/// matching the official driver).
pub fn rf_frames(order: u8, data: &[u8], last_len: Option<u8>) -> Vec<[u8; RF_FRAME]> {
    let chunks: Vec<&[u8]> = data.chunks(RF_CHUNK).collect();
    let n = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let mut f = [0u8; RF_FRAME];
            f[0] = order;
            f[1] = match last_len {
                Some(l) if i == n - 1 => l,
                _ => c.len() as u8,
            };
            f[2] = i as u8;
            f[3..3 + c.len()].copy_from_slice(c);
            f[RF_FRAME - 1] = f[..RF_FRAME - 1].iter().fold(0u8, |a, &b| a.wrapping_add(b));
            f
        })
        .collect()
}

/// USB polling rate on the AK820 (`320F:505B`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
pub enum PollingRate {
    #[serde(rename = "125hz")]
    #[value(name = "125hz", alias = "125")]
    Hz125 = 0,
    #[serde(rename = "250hz")]
    #[value(name = "250hz", alias = "250")]
    Hz250 = 1,
    #[serde(rename = "500hz")]
    #[value(name = "500hz", alias = "500")]
    Hz500 = 2,
    #[default]
    #[serde(rename = "1000hz")]
    #[value(name = "1000hz", alias = "1000")]
    Hz1000 = 3,
}

impl PollingRate {
    pub fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Hz125,
            1 => Self::Hz250,
            2 => Self::Hz500,
            _ => Self::Hz1000,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Hz125 => "125 Hz",
            Self::Hz250 => "250 Hz",
            Self::Hz500 => "500 Hz",
            Self::Hz1000 => "1000 Hz",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemSettings {
    pub block_win: bool,
    pub block_alt_f4: bool,
    pub block_alt_tab: bool,
    /// false = Fn layer while held, true = Fn toggles the layer (or Mac mode on AK820).
    pub fn_toggle: bool,
    /// 0 never, 1 = 1 min, 2 = 5 min, 3 = 30 min (battery only).
    pub sleep: u8,
    /// Debounce level 1 (fast) ..= 5 (slow).
    pub response: u8,
    /// USB polling rate (AK820).
    pub polling_rate: PollingRate,
    /// N-key rollover (AK820).
    pub nkro: bool,
    /// Swap WASD with arrow keys (AK820).
    pub swap_wasd: bool,
}

impl Default for SystemSettings {
    /// Factory values of the official driver.
    fn default() -> Self {
        Self {
            block_win: false,
            block_alt_f4: false,
            block_alt_tab: false,
            fn_toggle: false,
            sleep: 2,
            response: 2,
            polling_rate: PollingRate::Hz1000,
            nkro: true,
            swap_wasd: false,
        }
    }
}

pub fn system_packet(s: &SystemSettings) -> Packet {
    let mut p = [0u8; PACKET];
    p[1] = 0x01;
    p[2] = s.block_win as u8;
    p[3] = s.block_alt_f4 as u8;
    p[4] = s.block_alt_tab as u8;
    p[5] = s.fn_toggle as u8;
    p[6] = s.sleep.min(3);
    p[8] = s.response.clamp(1, 5);
    p[62] = 0xAA;
    p[63] = 0x55;
    p
}

/// Per-key color table: `[light_index, R, G, B]` in each key's slot.
pub fn color_table<I: IntoIterator<Item = (u8, [u8; 3])>>(colors: I) -> Vec<u8> {
    table(colors.into_iter().map(|(i, [r, g, b])| (i, [i, r, g, b])))
}

/// Live (unsaved) per-key frame: 512 bytes of consecutive `[index, R, G, B]`.
pub fn live_frame<I: IntoIterator<Item = (u8, [u8; 3])>>(colors: I) -> Vec<u8> {
    let mut d = Vec::with_capacity(512);
    for (i, [r, g, b]) in colors.into_iter().take(128) {
        d.extend_from_slice(&[i, r, g, b]);
    }
    d.resize(512, 0);
    d
}

// ---- EVision protocol (AK820 320F:505B) ------------------------------------

/// Build a 64-byte EVision packet with report ID `0x04` and 16-bit LE checksum at bytes `1..3`.
pub fn evision_packet(cmd: u8, size: u8, offset: u16, payload: &[u8]) -> Packet {
    let mut p = [0u8; PACKET];
    p[0] = EVISION_REPORT_ID;
    p[3] = cmd;
    p[4] = size;
    p[5..7].copy_from_slice(&offset.to_le_bytes());
    p[7] = 0;
    let n = payload.len().min(EVISION_MAX_CHUNK);
    p[8..8 + n].copy_from_slice(&payload[..n]);
    let sum = sum16(&p[3..]);
    p[1..3].copy_from_slice(&sum.to_le_bytes());
    p
}

/// Split `data` into 56-byte chunk write packets for EVision command `cmd`.
pub fn evision_write_packets(cmd: u8, base_offset: u16, data: &[u8]) -> Vec<Packet> {
    data.chunks(EVISION_MAX_CHUNK)
        .enumerate()
        .map(|(i, chunk)| {
            let offset = base_offset + (i * EVISION_MAX_CHUNK) as u16;
            evision_packet(cmd, chunk.len() as u8, offset, chunk)
        })
        .collect()
}

/// Build `(request_packet, chunk_len)` pairs to read `total_len` bytes in 56-byte chunks.
pub fn evision_read_requests(cmd: u8, base_offset: u16, total_len: usize) -> Vec<(Packet, usize)> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < total_len {
        let len = (total_len - pos).min(EVISION_MAX_CHUNK);
        let pkt = evision_packet(cmd, len as u8, base_offset + pos as u16, &[]);
        out.push((pkt, len));
        pos += len;
    }
    out
}

/// Factory default 29-byte config block for AK820 (`320F:505B`).
pub fn evision_default_config() -> [u8; AK820_CONFIG_LEN] {
    let mut cfg = [0u8; AK820_CONFIG_LEN];
    cfg[0] = 0; // profile 0
    cfg[1] = Mode::Rolling.ak820_code();
    cfg[2] = 4; // max brightness (0..=4)
    cfg[3] = 1; // speed 3 (4 - 3 = 1)
    cfg[4] = 0; // Right
    cfg[5] = 1; // rainbow
    cfg[6..9].copy_from_slice(&[255, 255, 255]);
    cfg[21] = 1; // NKRO on
    cfg[22] = PollingRate::Hz1000 as u8;
    cfg[26] = 1; // debounce level 2 (index 1)
    cfg
}

/// Apply `Lighting` fields onto an AK820 29-byte config buffer.
pub fn evision_apply_lighting(cfg: &mut [u8; AK820_CONFIG_LEN], l: &Lighting) {
    cfg[1] = l.mode.ak820_code();
    cfg[2] = match l.brightness {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        _ => 4,
    };
    cfg[3] = 4u8.saturating_sub(l.speed.min(4));
    cfg[4] = l.direction.code(l.mode);
    cfg[5] = l.rainbow as u8;
    cfg[6..9].copy_from_slice(&[l.color.0, l.color.1, l.color.2]);
    cfg[25] = 0; // custom light group 0
}

/// Decode `Lighting` from an AK820 29-byte config buffer.
pub fn evision_parse_lighting(cfg: &[u8; AK820_CONFIG_LEN]) -> Lighting {
    let mode = Mode::from_ak820_code(cfg[1]).unwrap_or(Mode::Rolling);
    let brightness = match cfg[2] {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        _ => 5,
    };
    let speed = 4u8.saturating_sub(cfg[3].min(4));
    let direction = match (mode.direction_axis(), cfg[4]) {
        (Some(Axis::Vertical), 1) => Direction::Up,
        (Some(Axis::Vertical), _) => Direction::Down,
        (Some(Axis::Horizontal), 1) => Direction::Left,
        _ => Direction::Right,
    };
    let rainbow = cfg[5] != 0;
    let color = Rgb(cfg[6], cfg[7], cfg[8]);
    Lighting { mode, color, rainbow, brightness, speed, direction }
}

/// Apply `SystemSettings` onto an AK820 29-byte config buffer.
pub fn evision_apply_system(cfg: &mut [u8; AK820_CONFIG_LEN], s: &SystemSettings) {
    cfg[19] = s.swap_wasd as u8;
    cfg[20] = s.block_win as u8;
    cfg[21] = s.nkro as u8;
    cfg[22] = s.polling_rate as u8;
    cfg[26] = s.response.saturating_sub(1).min(3);
    cfg[27] = s.fn_toggle as u8;
}

/// Decode `SystemSettings` from an AK820 29-byte config buffer.
pub fn evision_parse_system(cfg: &[u8; AK820_CONFIG_LEN]) -> SystemSettings {
    SystemSettings {
        block_win: cfg[20] != 0,
        block_alt_f4: false,
        block_alt_tab: false,
        fn_toggle: cfg[27] != 0,
        sleep: 2,
        response: cfg[26].min(3) + 1,
        polling_rate: PollingRate::from_code(cfg[22]),
        nkro: cfg[21] != 0,
        swap_wasd: cfg[19] != 0,
    }
}

/// Build the 384-byte (`128 * 3`) RGB buffer for AK820 custom per-key colors (`0x0B`) or live frame (`0x12`).
pub fn evision_color_table<I: IntoIterator<Item = (u8, [u8; 3])>>(colors: I) -> [u8; AK820_TABLE_LEN] {
    let mut buf = [0u8; AK820_TABLE_LEN];
    for (pos, [r, g, b]) in colors {
        if let Some(hw) = keys::ak820_hw_slot(pos) {
            if hw < AK820_SLOTS {
                buf[hw * 3..hw * 3 + 3].copy_from_slice(&[r, g, b]);
            }
        }
    }
    buf
}

/// Decode a 384-byte AK820 RGB buffer (`0x0A`) into `(base_color, per_key_map)`.
pub fn evision_parse_colors(buf: &[u8; AK820_TABLE_LEN]) -> (Rgb, BTreeMap<String, Rgb>) {
    let mut counts: BTreeMap<[u8; 3], usize> = BTreeMap::new();
    let mut key_rgb = Vec::new();
    for (name, pos) in keys::all_lights_for(Model::Ak820) {
        if let Some(hw) = keys::ak820_hw_slot(pos) {
            let rgb = [buf[hw * 3], buf[hw * 3 + 1], buf[hw * 3 + 2]];
            *counts.entry(rgb).or_insert(0) += 1;
            key_rgb.push((name, rgb));
        }
    }
    let base_arr = counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(rgb, _)| rgb)
        .unwrap_or([0, 0, 0]);
    let base = Rgb(base_arr[0], base_arr[1], base_arr[2]);
    let mut map = BTreeMap::new();
    for (name, rgb) in key_rgb {
        let c = Rgb(rgb[0], rgb[1], rgb[2]);
        if c != base {
            map.insert(name.to_string(), c);
        }
    }
    (base, map)
}

/// Factory default 384-byte keymap for AK820 (`320F:505B`), matching hardware command `0x07`.
pub fn evision_default_keymap() -> [u8; AK820_TABLE_LEN] {
    let mut buf = [0u8; AK820_TABLE_LEN];
    for (name, pos) in keys::all_lights_for(Model::Ak820) {
        if let Some(hw) = keys::ak820_hw_slot(pos) {
            let entry = if name == "fn" {
                [0xA0, 0x01, 0x00]
            } else if let Some(u) = keys::usage(name) {
                if u >= 0xE0 {
                    [0x20, 1 << (u - 0xE0), 0x00]
                } else {
                    [0x20, 0x00, u]
                }
            } else {
                [0, 0, 0]
            };
            buf[hw * 3..hw * 3 + 3].copy_from_slice(&entry);
        }
    }
    // Volume rotary encoder slots (CW = vol+, Press = mute, CCW = vol-).
    buf[87 * 3..87 * 3 + 3].copy_from_slice(&[0x30, 0xE9, 0x00]);
    buf[103 * 3..103 * 3 + 3].copy_from_slice(&[0x30, 0xE2, 0x00]);
    buf[111 * 3..111 * 3 + 3].copy_from_slice(&[0x30, 0xEA, 0x00]);
    buf
}

/// Convert a 4-byte AK820 Pro target slot (`keys::encode_target`) into a 3-byte EVision slot.
pub fn evision_encode_slot(slot4: [u8; 4], default_slot: [u8; 3]) -> [u8; 3] {
    match slot4 {
        [0, 0, 0, 0] => default_slot,
        [2, mods, usage, _] => [0x20, mods, usage],
        [3, lo, hi, _] => [0x30, lo, hi],
        [1, 1, btn, _] => [0x40, btn, 0x00],
        [1, 3, wheel, _] => [0x40, 0x00, wheel],
        [6, id, mode, count] => {
            let ev_mode = match mode {
                0 | 1 => count.max(1),
                2 => 0xFF, // Toggle
                3 => 0xFE, // Hold
                _ => 1,
            };
            [0x70, id, ev_mode]
        }
        [7, u1, _, _] => [0x20, 0x00, u1],
        _ => default_slot,
    }
}

/// Convert a 576-byte AK820 Pro keymap table (`protocol::table`) into a 384-byte EVision keymap table.
pub fn evision_from_pro_keymap(
    pro_table: &[u8],
    base_keymap: &[u8; AK820_TABLE_LEN],
) -> [u8; AK820_TABLE_LEN] {
    let mut out = *base_keymap;
    for &(pos, hw) in keys::AK820_HW_MAP {
        let i = pos as usize * 4;
        if i + 4 <= pro_table.len() && hw < AK820_SLOTS {
            let slot4 = [pro_table[i], pro_table[i + 1], pro_table[i + 2], pro_table[i + 3]];
            let def = [base_keymap[hw * 3], base_keymap[hw * 3 + 1], base_keymap[hw * 3 + 2]];
            let entry = evision_encode_slot(slot4, def);
            out[hw * 3..hw * 3 + 3].copy_from_slice(&entry);
        }
    }
    out
}

/// Decode a 3-byte EVision keymap slot into a remap target string if it differs from `default_slot`.
pub fn evision_decode_slot(slot: [u8; 3], default_slot: [u8; 3]) -> Option<String> {
    if slot == default_slot {
        return None;
    }
    match slot[0] {
        0x20 => {
            let mods = slot[1];
            let u = slot[2];
            let mut parts: Vec<&str> = Vec::new();
            for (bit, name) in [
                (0x01, "ctrl"),
                (0x02, "shift"),
                (0x04, "alt"),
                (0x08, "win"),
                (0x10, "rctrl"),
                (0x20, "rshift"),
                (0x40, "ralt"),
                (0x80, "rwin"),
            ] {
                if mods & bit != 0 {
                    parts.push(name);
                }
            }
            if u != 0 {
                if let Some(k) = keys::name_from_usage(u) {
                    parts.push(k);
                }
            }
            (!parts.is_empty()).then(|| parts.join("+"))
        }
        0x30 => {
            let usage = u16::from_le_bytes([slot[1], slot[2]]);
            keys::CONSUMER
                .iter()
                .find(|&&(_, v)| v == usage)
                .map(|&(name, _)| name.to_string())
        }
        0x40 => {
            if slot[1] != 0 {
                let b = match slot[1] {
                    0x01 => "mouse:left",
                    0x02 => "mouse:right",
                    0x04 => "mouse:middle",
                    _ => return None,
                };
                Some(b.into())
            } else {
                match slot[2] {
                    0x01 => Some("wheel:up".into()),
                    0xFF => Some("wheel:down".into()),
                    _ => None,
                }
            }
        }
        0x70 => Some(format!("macro:macro{}", slot[1] as usize + 1)),
        _ => None,
    }
}

/// Decode a 384-byte EVision keymap (`0x08`) against the factory keymap (`0x07`) into a map of remapped keys.
pub fn evision_parse_keymap(
    active: &[u8; AK820_TABLE_LEN],
    default_km: &[u8; AK820_TABLE_LEN],
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for &(pos, hw) in keys::AK820_HW_MAP {
        if hw < AK820_SLOTS {
            if let Some(name) = keys::canonical_name(pos) {
                let slot = [active[hw * 3], active[hw * 3 + 1], active[hw * 3 + 2]];
                let def = [default_km[hw * 3], default_km[hw * 3 + 1], default_km[hw * 3 + 2]];
                if let Some(target) = evision_decode_slot(slot, def) {
                    out.insert(name.to_string(), target);
                }
            }
        }
    }
    out
}

/// Build the EVision macro buffer for `0x15` (`WRITE_MACROS`) from `macros::events` lists.
pub fn evision_macro_block(macros: &[Vec<[u8; 4]>]) -> Option<Vec<u8>> {
    if macros.is_empty() {
        let mut empty = vec![0u8; 16];
        empty[0..2].copy_from_slice(&0x55AAu16.to_le_bytes());
        empty[2..4].copy_from_slice(&16u16.to_le_bytes());
        return Some(empty);
    }
    if macros.len() > 64 {
        return None;
    }
    let mut converted: Vec<Vec<[u8; 4]>> = Vec::with_capacity(macros.len());
    for ev_list in macros {
        let mut ev_out: Vec<[u8; 4]> = Vec::new();
        let mut i = 0;
        while i < ev_list.len() {
            let e = ev_list[i];
            match e[3] {
                0xB0 | 0x30 => {
                    let is_down = e[3] == 0xB0;
                    let hid = e[2];
                    let mut delay_ms: u32 = 0;
                    while i + 1 < ev_list.len() && ev_list[i + 1][3] == 0x50 {
                        let d = u16::from_le_bytes([ev_list[i + 1][0], ev_list[i + 1][1]]) as u32;
                        delay_ms = delay_ms.saturating_add(d);
                        i += 1;
                    }
                    let ms = (delay_ms.max(5).min(0x7FFF)) as u16;
                    let b0 = ((ms >> 8) as u8 & 0x7F) | if is_down { 0x80 } else { 0x00 };
                    let b1 = (ms & 0xFF) as u8;
                    ev_out.push([b0, b1, hid, 0]);
                }
                _ => {}
            }
            i += 1;
        }
        converted.push(ev_out);
    }

    let count = converted.len();
    let header_len = 16 + count * 2;
    let body_len: usize = converted.iter().map(|m| 4 + m.len() * 4).sum();
    let total_len = header_len + body_len;
    if total_len > AK820_MACRO_MAX {
        return None;
    }
    let mut buf = vec![0u8; total_len];
    buf[0..2].copy_from_slice(&0x55AAu16.to_le_bytes());
    buf[2..4].copy_from_slice(&(total_len as u16).to_le_bytes());
    buf[4..6].copy_from_slice(&(count as u16).to_le_bytes());
    let mut pos = header_len;
    for (idx, m) in converted.iter().enumerate() {
        buf[16 + idx * 2..18 + idx * 2].copy_from_slice(&(pos as u16).to_le_bytes());
        buf[pos..pos + 2].copy_from_slice(&(m.len() as u16).to_le_bytes());
        pos += 4;
        for ev in m {
            buf[pos..pos + 4].copy_from_slice(ev);
            pos += 4;
        }
    }
    Some(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_layout() {
        let p = control(cmd::SCREEN, 1, 0x0123);
        assert_eq!(&p[..3], &[0x04, 0x72, 0x01]);
        assert_eq!(&p[8..10], &[0x23, 0x01]);
        assert!(p[10..].iter().all(|&b| b == 0));
    }

    #[test]
    fn clock_layout() {
        let t = ClockTime { year: 2026, month: 10, day: 14, hour: 21, minute: 5, second: 9, weekday: 3 };
        let p = clock_packet(&t);
        assert_eq!(&p[1..9], &[1, 0x5A, 26, 10, 14, 21, 5, 9]);
        assert_eq!(p[10], 4);
        assert_eq!(&p[62..], &[0xAA, 0x55]);
        let rf = rf_clock_data(&t);
        assert_eq!(rf[10], 3);
        assert_eq!(&rf[14..], &[0xAA, 0x55]);
    }

    #[test]
    fn tables() {
        let t = color_table([(1, [255, 0, 0]), (121, [0, 0, 255])]);
        assert_eq!(t.len(), 576);
        assert_eq!(&t[4..8], &[1, 255, 0, 0]);
        assert_eq!(&t[484..488], &[121, 0, 0, 255]);
        assert_eq!(&t[574..], &[0xAA, 0x55]);
        assert_eq!(packets(&t).len(), 9);
    }

    #[test]
    fn live_layout() {
        let d = live_frame([(1, [1, 2, 3]), (2, [4, 5, 6])]);
        assert_eq!(d.len(), 512);
        assert_eq!(&d[..8], &[1, 1, 2, 3, 2, 4, 5, 6]);
    }

    #[test]
    fn rf_framing() {
        let f = rf_frames(rf::LIGHTING, &[1u8; 16], None);
        assert_eq!(f.len(), 1);
        assert_eq!(&f[0][..3], &[0x05, 16, 0]);
        assert_eq!(f[0][31], (0x05u32 + 16 + 16) as u8);
        let t = rf_frames(rf::KEYMAP, &table([]), None);
        assert_eq!(t.len(), 21);
        assert_eq!(t[20][1], 16);
        let m = rf_frames(rf::MACROS, &[0u8; 56], Some(5));
        assert_eq!((m[0][1], m[1][1]), (28, 5));
    }

    #[test]
    fn system_layout() {
        let p = system_packet(&SystemSettings::default());
        assert_eq!(&p[..9], &[0, 1, 0, 0, 0, 0, 2, 0, 2]);
        assert_eq!(&p[62..], &[0xAA, 0x55]);
    }

    #[test]
    fn checksum() {
        assert_eq!(sum16(&[0xFF; 300]), (0xFFu32 * 300) as u16);
    }

    #[test]
    fn evision_framing() {
        let p = evision_packet(evision::BEGIN, 0, 0, &[]);
        assert_eq!(&p[..8], &[0x04, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]);
        let reqs = evision_read_requests(evision::READ_KEYMAP, 0, AK820_TABLE_LEN);
        assert_eq!(reqs.len(), 7);
        assert_eq!(reqs[0].1, 56);
        assert_eq!(reqs[6].1, 48);
    }

    #[test]
    fn evision_config_roundtrip() {
        let mut cfg = evision_default_config();
        let l = Lighting {
            mode: Mode::Static,
            color: Rgb(0x12, 0x34, 0x56),
            rainbow: false,
            brightness: 5,
            speed: 3,
            direction: Direction::Right,
        };
        evision_apply_lighting(&mut cfg, &l);
        assert_eq!(evision_parse_lighting(&cfg), l);

        let sys = SystemSettings {
            block_win: true,
            fn_toggle: true,
            response: 3,
            polling_rate: PollingRate::Hz500,
            nkro: false,
            swap_wasd: true,
            ..SystemSettings::default()
        };
        evision_apply_system(&mut cfg, &sys);
        assert_eq!(evision_parse_system(&cfg), sys);
    }

    #[test]
    fn evision_keymap_roundtrip() {
        let def = evision_default_keymap();
        // Esc -> hw 0 = [0x20, 0x00, 0x29], LShift -> hw 4 = [0x20, 0x02, 0x00], Fn -> hw 85 = [0xA0, 0x01, 0x00]
        assert_eq!(&def[0..3], &[0x20, 0x00, 0x29]);
        assert_eq!(&def[4 * 3..4 * 3 + 3], &[0x20, 0x02, 0x00]);
        assert_eq!(&def[85 * 3..85 * 3 + 3], &[0xA0, 0x01, 0x00]);

        // Remap pgdn (121) -> end (0x4D), capslock (55) -> ctrl (0x01)
        let pro = table([(121, [2, 0, 0x4D, 0]), (55, [2, 0x01, 0, 0])]);
        let ev = evision_from_pro_keymap(&pro, &def);
        let parsed = evision_parse_keymap(&ev, &def);
        assert_eq!(parsed.get("pgdn").map(String::as_str), Some("end"));
        assert_eq!(parsed.get("capslock").map(String::as_str), Some("ctrl"));
        assert_eq!(parsed.len(), 2);
    }
}
