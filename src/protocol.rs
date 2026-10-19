//! Wire format of the AK820 Pro stock firmware (the protocol spoken by the
//! official Windows `DeviceDriver.exe` v1.0.0.5).
//!
//! Everything here is pure byte building, with no I/O, so it can be unit-tested
//! without a keyboard. See `docs/PROTOCOL.md` for the full description.

/// SONiX, the keyboard MCU vendor.
pub const VID: u16 = 0x0C45;
/// USB cable. Both PIDs ship with the same protocol (`config.xml` of the official driver).
pub const WIRED_PIDS: &[u16] = &[0x8009, 0x800A];
/// 2.4 GHz USB receiver.
pub const RECEIVER_PID: u16 = 0xFDFD;
/// Bluetooth: no vendor interface, cannot be configured.
pub const BLUETOOTH_PID: u16 = 0xFEFE;

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

/// Command byte (byte 1) of the `04 xx` control packets.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemSettings {
    pub block_win: bool,
    pub block_alt_f4: bool,
    pub block_alt_tab: bool,
    /// false = Fn layer while held, true = Fn toggles the layer.
    pub fn_toggle: bool,
    /// 0 never, 1 = 1 min, 2 = 5 min, 3 = 30 min (battery only).
    pub sleep: u8,
    /// Debounce level 1 (fast) ..= 5 (slow).
    pub response: u8,
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
}
