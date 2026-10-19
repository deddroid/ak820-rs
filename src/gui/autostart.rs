//! Start the app (in the tray) when the user logs in.
//!
//! Linux: an XDG autostart entry. Windows: the `Run` registry key of the user.

use std::io;

/// Command-line flag the autostart entry passes: start hidden in the tray.
pub const TRAY_FLAG: &str = "--tray";

#[cfg(target_os = "linux")]
fn entry() -> Option<std::path::PathBuf> {
    Some(dirs::config_dir()?.join("autostart").join("ak820.desktop"))
}

#[cfg(target_os = "linux")]
pub fn is_enabled() -> bool {
    entry().is_some_and(|p| p.exists())
}

#[cfg(target_os = "linux")]
pub fn set(enabled: bool) -> io::Result<()> {
    let path = entry().ok_or_else(|| io::Error::other("no config directory"))?;
    if !enabled {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=AK820 Pro\nComment=AJAZZ AK820 Pro configurator\n\
         Exec=\"{}\" {TRAY_FLAG}\nIcon=ak820\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exe.display()
    );
    std::fs::write(path, text)
}

#[cfg(windows)]
mod win {
    use std::io;
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    };

    const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const VALUE: &str = "AK820 Pro";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn is_enabled() -> bool {
        let (key, value) = (wide(KEY), wide(VALUE));
        let mut size = 0u32;
        // SAFETY: valid NUL-terminated strings; a null data pointer only queries the size.
        let r = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        r == 0
    }

    pub fn set(enabled: bool) -> io::Result<()> {
        let (key, value) = (wide(KEY), wide(VALUE));
        let r = if enabled {
            let exe = std::env::current_exe()?;
            let data = wide(&format!("\"{}\" {}", exe.display(), super::TRAY_FLAG));
            // SAFETY: `data` is a NUL-terminated UTF-16 string of the given byte length.
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            }
        } else {
            // SAFETY: valid NUL-terminated strings.
            let r = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) };
            if r == 2 { 0 } else { r } // ERROR_FILE_NOT_FOUND: already off
        };
        if r == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(r as i32))
        }
    }
}

#[cfg(windows)]
pub use win::{is_enabled, set};

#[cfg(not(any(target_os = "linux", windows)))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn set(_enabled: bool) -> io::Result<()> {
    Err(io::Error::other("autostart is not supported on this system"))
}
