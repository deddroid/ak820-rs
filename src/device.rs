//! HID transport: USB cable (feature reports on 0xFF13 + screen data on 0xFF68)
//! or the 2.4 GHz receiver (32-byte frames on 0xFF60).

use std::ffi::CString;
use std::thread::sleep;
use std::time::Duration;

use hidapi::{DeviceInfo, HidApi, HidDevice};

use crate::lighting::{Lighting, Mode};
use crate::protocol::{self as p, cmd, rf, ClockTime, Packet, SystemSettings, PACKET};
use crate::{Error, Result, Rgb};

/// The official driver waits 35 ms before and after every command; without
/// the pause *before*, the keyboard often ignores it.
const PAUSE: Duration = Duration::from_millis(35);
const RETRIES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    Wired,
    Receiver,
}

enum Link {
    Wired { config: HidDevice, data_path: Option<CString> },
    Receiver(HidDevice),
}

pub struct Keyboard {
    api: HidApi,
    link: Link,
    pid: u16,
    debug: bool,
}

/// One HID interface of the keyboard, for `ak820 list`.
#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub pid: u16,
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
    pub product: String,
    pub path: String,
}

pub fn list_interfaces() -> Result<Vec<InterfaceInfo>> {
    let api = HidApi::new()?;
    let mut out: Vec<InterfaceInfo> = api
        .device_list()
        .filter(|d| d.vendor_id() == p::VID)
        .map(|d| InterfaceInfo {
            pid: d.product_id(),
            interface: d.interface_number(),
            usage_page: d.usage_page(),
            usage: d.usage(),
            product: d.product_string().unwrap_or("").to_string(),
            path: d.path().to_string_lossy().into_owned(),
        })
        .collect();
    out.sort_by_key(|i| (i.pid, i.interface, i.usage_page));
    Ok(out)
}

fn find<'a>(api: &'a HidApi, pids: &[u16], usage_page: u16, interface: i32) -> Option<&'a DeviceInfo> {
    let candidates: Vec<&DeviceInfo> = api
        .device_list()
        .filter(|d| d.vendor_id() == p::VID && pids.contains(&d.product_id()))
        .collect();
    candidates
        .iter()
        .find(|d| d.usage_page() == usage_page)
        // Some backends (e.g. libusb) report no usage page: fall back to the interface number.
        .or_else(|| candidates.iter().find(|d| d.usage_page() == 0 && d.interface_number() == interface))
        .copied()
}

fn open_error(e: hidapi::HidError) -> Error {
    if cfg!(target_os = "linux") && e.to_string().contains("Permission denied") {
        Error::Permission(format!(
            "cannot open the keyboard ({e}).\n\
             On Linux install the udev rule once:  sudo cp udev/70-ajazz-ak820.rules /etc/udev/rules.d/ \
             && sudo udevadm control --reload && sudo udevadm trigger\n\
             then unplug and replug the keyboard."
        ))
    } else {
        Error::Hid(e)
    }
}

impl Keyboard {
    /// Open the keyboard: USB cable first, then the 2.4 GHz receiver.
    pub fn open() -> Result<Keyboard> {
        let api = HidApi::new()?;
        let debug = std::env::var_os("AK820_DEBUG").is_some();
        if let Some(info) = find(&api, p::WIRED_PIDS, p::CONFIG_USAGE_PAGE, p::CONFIG_INTERFACE) {
            let pid = info.product_id();
            let config = info.open_device(&api).map_err(open_error)?;
            let data_path = find(&api, p::WIRED_PIDS, p::DATA_USAGE_PAGE, p::DATA_INTERFACE)
                .map(|d| d.path().to_owned());
            return Ok(Keyboard { link: Link::Wired { config, data_path }, api, pid, debug });
        }
        if let Some(info) = find(&api, &[p::RECEIVER_PID], p::RECEIVER_USAGE_PAGE, p::RECEIVER_INTERFACE) {
            let pid = info.product_id();
            let dev = info.open_device(&api).map_err(open_error)?;
            return Ok(Keyboard { link: Link::Receiver(dev), api, pid, debug });
        }
        let bt = api
            .device_list()
            .any(|d| d.vendor_id() == p::VID && d.product_id() == p::BLUETOOTH_PID);
        Err(Error::NotFound(if bt {
            "the keyboard is connected via Bluetooth, which cannot be configured: use the USB cable or the 2.4 GHz receiver".into()
        } else {
            "AK820 Pro not found (USB cable or 2.4 GHz receiver). Try `ak820 list`.".into()
        }))
    }

    pub fn connection(&self) -> Connection {
        match self.link {
            Link::Wired { .. } => Connection::Wired,
            Link::Receiver(_) => Connection::Receiver,
        }
    }

    pub fn pid(&self) -> u16 {
        self.pid
    }

    fn trace(&self, dir: &str, data: &[u8]) {
        if self.debug {
            let end = data.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1).min(24);
            let hex: Vec<String> = data[..end].iter().map(|b| format!("{b:02x}")).collect();
            eprintln!("  {dir} [{}] {}", data.len(), hex.join(" "));
        }
    }

    fn config(&self) -> Result<&HidDevice> {
        match &self.link {
            Link::Wired { config, .. } => Ok(config),
            Link::Receiver(_) => Err(Error::Unsupported(
                "this needs the USB cable (not supported through the 2.4 GHz receiver)".into(),
            )),
        }
    }

    // ---- wired primitives ------------------------------------------------

    fn feature(&self, packet: &Packet, pause: Duration) -> Result<()> {
        let dev = self.config()?;
        sleep(pause);
        let mut buf = [0u8; PACKET + 1]; // report ID 0 first
        buf[1..].copy_from_slice(packet);
        self.trace("->", packet);
        dev.send_feature_report(&buf)?;
        Ok(())
    }

    /// Command packet followed by the feature-report read the keyboard expects.
    fn command(&self, packet: &Packet) -> Result<Option<[u8; PACKET + 1]>> {
        self.feature(packet, PAUSE)?;
        sleep(PAUSE);
        let mut buf = [0u8; PACKET + 1];
        match self.config()?.get_feature_report(&mut buf) {
            Ok(_) => {
                self.trace("<-", &buf[1..]);
                Ok(Some(buf))
            }
            // The reply is mostly an acknowledgement; carry on without it.
            Err(_) => Ok(None),
        }
    }

    /// Data packets that follow a command are sent without reading a reply.
    fn data(&self, data: &[u8], pause: Duration) -> Result<()> {
        for pkt in p::packets(data) {
            self.feature(&pkt, pause)?;
        }
        Ok(())
    }

    /// COMMIT, checking the 16-bit sum of the received data the keyboard reports back.
    fn commit_checked(&self, data: &[u8]) -> Result<bool> {
        let reply = self.command(&p::control(cmd::COMMIT, 0, 0))?;
        Ok(match reply {
            Some(r) => u16::from_le_bytes([r[5], r[6]]) == p::sum16(data),
            None => false,
        })
    }

    /// BEGIN → `command` (count = packets) → table → COMMIT (checked) → SAVE.
    fn upload_table(&self, command: u8, table: &[u8], what: &str) -> Result<()> {
        for attempt in 1..=RETRIES {
            self.command(&p::control(cmd::BEGIN, 0, 0))?;
            self.command(&p::control(command, 0, (table.len() / PACKET) as u16))?;
            self.data(table, PAUSE)?;
            if self.commit_checked(table)? {
                self.command(&p::control(cmd::SAVE, 0, 0))?;
                return Ok(());
            }
            if attempt < RETRIES {
                eprintln!("  {what} not received correctly (attempt {attempt}), retrying...");
                sleep(Duration::from_millis(200));
            }
        }
        Err(Error::Transfer(format!("the keyboard did not accept the {what}")))
    }

    // ---- receiver primitives --------------------------------------------

    fn rf_send(&self, frames: &[[u8; p::RF_FRAME]]) -> Result<()> {
        let Link::Receiver(dev) = &self.link else { unreachable!() };
        'frames: for f in frames {
            for _ in 0..RETRIES {
                let mut buf = [0u8; p::RF_FRAME + 1];
                buf[1..].copy_from_slice(f);
                self.trace("->", f);
                dev.write(&buf)?;
                if self.rf_echo(dev, f)?.is_some() {
                    sleep(Duration::from_millis(2));
                    continue 'frames;
                }
            }
            return Err(Error::Transfer(
                "the 2.4 GHz receiver did not answer: is the keyboard on and in 2.4G mode? \
                 If it was idle, press a key to wake it up."
                    .into(),
            ));
        }
        Ok(())
    }

    /// The receiver confirms a frame by echoing its first 3 bytes.
    fn rf_echo(&self, dev: &HidDevice, frame: &[u8]) -> Result<Option<Vec<u8>>> {
        sleep(Duration::from_millis(5));
        let mut buf = [0u8; 64];
        for _ in 0..20 {
            let n = dev.read_timeout(&mut buf, 10)?;
            if n >= 3 && buf[..3] == frame[..3] {
                self.trace("<-", &buf[..n]);
                return Ok(Some(buf[..n].to_vec()));
            }
        }
        Ok(None)
    }

    // ---- features ---------------------------------------------------------

    /// Apply a built-in lighting effect (saved in the keyboard).
    pub fn set_lighting(&self, l: &Lighting) -> Result<()> {
        let data = l.packet();
        match self.connection() {
            Connection::Receiver => self.rf_send(&p::rf_frames(rf::LIGHTING, &p::rf_short(&data), None)),
            Connection::Wired => {
                self.command(&p::control(cmd::BEGIN, 0, 1))?;
                self.command(&p::control(cmd::LIGHTING, 0, 1))?;
                self.command(&data)?;
                self.command(&p::control(cmd::COMMIT, 0, 0))?;
                self.command(&p::control(cmd::SAVE, 0, 0))?;
                Ok(())
            }
        }
    }

    /// Static per-key colors (switches the effect to `custom`). Wired only.
    pub fn set_key_colors(&self, brightness: u8, colors: &[(u8, Rgb)]) -> Result<()> {
        self.config()?;
        let effect = Lighting { brightness, ..Lighting::new(Mode::Custom) };
        self.set_lighting(&effect)?;
        let table = p::color_table(colors.iter().map(|&(i, c)| (i, c.bytes())));
        self.upload_table(cmd::KEY_COLORS, &table, "key colors")
    }

    /// One real-time per-key frame, not saved. The keyboard returns to its
    /// saved effect shortly after frames stop. Wired only.
    pub fn live_frame(&self, colors: &[(u8, Rgb)]) -> Result<()> {
        let data = p::live_frame(colors.iter().map(|&(i, c)| (i, c.bytes())));
        self.feature(&p::control(cmd::LIVE_COLORS, 0, 8), Duration::from_millis(5))?;
        self.data(&data, Duration::from_millis(2))?;
        self.feature(&p::control(cmd::COMMIT, 0, 0), Duration::from_millis(5))?;
        sleep(Duration::from_millis(5));
        let mut buf = [0u8; PACKET + 1];
        let _ = self.config()?.get_feature_report(&mut buf);
        Ok(())
    }

    /// Set the screen clock. This also switches the screen back to the clock.
    pub fn set_clock(&self, t: &ClockTime) -> Result<()> {
        match self.connection() {
            Connection::Receiver => self.rf_send(&p::rf_frames(rf::CLOCK, &p::rf_clock_data(t), None)),
            Connection::Wired => {
                self.command(&p::control(cmd::BEGIN, 0, 1))?;
                self.command(&p::control(cmd::CLOCK, 0, 1))?;
                self.command(&p::clock_packet(t))?;
                self.command(&p::control(cmd::COMMIT, 0, 0))?;
                sleep(Duration::from_millis(100));
                Ok(())
            }
        }
    }

    /// Sleep timer, Fn mode, debounce and key blocking. All in one packet.
    pub fn set_system(&self, s: &SystemSettings) -> Result<()> {
        let data = p::system_packet(s);
        if self.connection() == Connection::Receiver {
            return self.rf_send(&p::rf_frames(rf::SYSTEM, &p::rf_short(&data), None));
        }
        for _ in 0..RETRIES {
            self.command(&p::control(cmd::BEGIN, 0, 0))?;
            self.command(&p::control(cmd::SYSTEM, 1, 1))?;
            sleep(PAUSE);
            self.data(&data, PAUSE)?;
            sleep(PAUSE);
            if self.commit_checked(&data)? {
                return Ok(());
            }
            sleep(Duration::from_millis(200));
        }
        Err(Error::Transfer("the keyboard did not accept the system settings".into()))
    }

    /// Upload the macro block and both key layers. The macro block is shared
    /// by both layers, so everything is always sent together.
    pub fn set_keymaps(&self, normal: &[u8], fn_layer: &[u8], macros: &[Vec<[u8; 4]>]) -> Result<()> {
        use crate::macros::{block, data_len};
        let too_big = || Error::Invalid("the macros do not fit in the keyboard memory (3.5 KB)".into());
        match self.connection() {
            Connection::Receiver => {
                let blk = block(macros, p::RF_CHUNK).ok_or_else(too_big)?;
                let last = (data_len(macros) % p::RF_CHUNK) as u8;
                let mut frames = p::rf_frames(rf::MACROS, &blk, Some(last));
                frames.extend(p::rf_frames(rf::KEYMAP, normal, None));
                frames.extend(p::rf_frames(rf::KEYMAP_FN, fn_layer, None));
                self.rf_send(&frames)
            }
            Connection::Wired => {
                let blk = block(macros, PACKET).ok_or_else(too_big)?;
                self.upload_macros(&blk)?;
                self.upload_table(cmd::KEYMAP, normal, "key table")?;
                self.upload_table(cmd::KEYMAP_FN, fn_layer, "Fn key table")
            }
        }
    }

    fn upload_macros(&self, blk: &[u8]) -> Result<()> {
        for attempt in 1..=RETRIES {
            // No BEGIN for macros.
            self.command(&p::control(cmd::MACRO_ERASE, 0, 0))?;
            self.command(&p::control(cmd::MACRO_DATA, 0, (blk.len() / PACKET) as u16))?;
            sleep(Duration::from_millis(30));
            self.data(blk, PAUSE)?;
            sleep(Duration::from_millis(30));
            // The first upload after a while is sometimes lost (flash erase).
            if self.commit_checked(blk)? {
                return Ok(());
            }
            if attempt < RETRIES {
                eprintln!("  macros not received correctly (attempt {attempt}), retrying...");
            }
        }
        Err(Error::Transfer("the keyboard did not accept the macros".into()))
    }

    /// Upload screen blocks (see [`crate::screen::blocks`]). Wired only.
    pub fn upload_screen(&self, blocks: &[Vec<u8>], mut progress: impl FnMut(usize, usize)) -> Result<()> {
        let Link::Wired { data_path, .. } = &self.link else {
            self.config()?;
            unreachable!()
        };
        let path = data_path
            .as_ref()
            .ok_or_else(|| Error::NotFound("screen data interface (0xFF68) not found".into()))?;
        let data = self.api.open_path(path).map_err(open_error)?;
        let n = blocks.len();
        self.command(&p::control(cmd::BEGIN, 0, 0))?;
        self.command(&p::control(cmd::SCREEN, 1, n as u16))?;
        let mut ack = [0u8; PACKET + 1];
        for (i, blk) in blocks.iter().enumerate() {
            let mut buf = Vec::with_capacity(blk.len() + 1);
            buf.push(0);
            buf.extend_from_slice(blk);
            data.write(&buf)?;
            // Wait for the `01 5A 02` acknowledgement before the next block.
            if data.read_timeout(&mut ack, 3000)? == 0 {
                return Err(Error::Transfer(format!("no acknowledgement for screen block {}/{n}", i + 1)));
            }
            progress(i + 1, n);
        }
        // COMMIT only: SAVE would corrupt the image.
        self.command(&p::control(cmd::COMMIT, 0, 0))?;
        sleep(Duration::from_millis(100));
        Ok(())
    }

    /// Battery level in %, only available through the 2.4 GHz receiver.
    pub fn battery(&self) -> Result<Option<u8>> {
        let Link::Receiver(dev) = &self.link else {
            return Err(Error::Unsupported(
                "the battery level can only be read through the 2.4 GHz receiver".into(),
            ));
        };
        let frame = p::rf_frames(rf::BATTERY, &[0], None)[0];
        for _ in 0..2 {
            let mut buf = [0u8; p::RF_FRAME + 1];
            buf[1..].copy_from_slice(&frame);
            dev.write(&buf)?;
            if let Some(r) = self.rf_echo(dev, &frame)? {
                return Ok(r.get(3).copied().filter(|v| (1..=100).contains(v)));
            }
        }
        Ok(None)
    }
}
