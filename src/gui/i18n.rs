//! English/Italian strings for the GUI, plus the per-user language preference.
//!
//! The preference lives in `gui.toml`, separate from the keyboard state file.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    En,
    It,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::It];

    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::It => "Italiano",
        }
    }

    fn parse(s: &str) -> Option<Lang> {
        let s = s.trim().to_ascii_lowercase();
        if s.starts_with("it") {
            Some(Lang::It)
        } else if s.starts_with("en") {
            Some(Lang::En)
        } else {
            None
        }
    }
}

static ITALIAN: AtomicBool = AtomicBool::new(false);

pub fn set(lang: Lang) {
    ITALIAN.store(lang == Lang::It, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if ITALIAN.load(Ordering::Relaxed) {
        Lang::It
    } else {
        Lang::En
    }
}

/// Pick the string for the current language.
pub fn t<'a>(en: &'a str, it: &'a str) -> &'a str {
    match current() {
        Lang::En => en,
        Lang::It => it,
    }
}

#[derive(Default, Serialize, Deserialize)]
struct GuiSettings {
    lang: Option<Lang>,
}

fn settings_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("ak820").join("gui.toml"))
}

/// Choose the startup language: `AK820_GUI_LANG`, then the saved choice, then the system locale.
pub fn init() -> Lang {
    let forced = std::env::var("AK820_GUI_LANG").ok().and_then(|v| Lang::parse(&v));
    let saved = || {
        let text = std::fs::read_to_string(settings_path()?).ok()?;
        toml::from_str::<GuiSettings>(&text).ok()?.lang
    };
    let lang = forced.or_else(saved).unwrap_or_else(system_lang);
    set(lang);
    lang
}

/// Remember the language chosen in the sidebar.
pub fn save(lang: Lang) -> std::io::Result<()> {
    let Some(path) = settings_path() else { return Ok(()) };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = toml::to_string(&GuiSettings { lang: Some(lang) }).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

fn system_lang() -> Lang {
    // POSIX precedence: the first non-empty variable wins.
    let env = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty());
    #[cfg(windows)]
    let env = env.or_else(windows_locale);
    match env.as_deref() {
        Some(l) if l.starts_with("it") => Lang::It,
        _ => Lang::En,
    }
}

/// The user's locale name, e.g. `it-IT`.
#[cfg(windows)]
fn windows_locale() -> Option<String> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultLocaleName(name: *mut u16, len: i32) -> i32;
    }
    let mut buf = [0u16; 85];
    // SAFETY: the buffer is valid for `len` UTF-16 units; the call writes at most that many.
    let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    (n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_locale_names() {
        assert_eq!(Lang::parse("it_IT.UTF-8"), Some(Lang::It));
        assert_eq!(Lang::parse("EN"), Some(Lang::En));
        assert_eq!(Lang::parse("de_DE"), None);
    }

    #[test]
    fn settings_round_trip() {
        let text = toml::to_string(&GuiSettings { lang: Some(Lang::It) }).unwrap();
        assert_eq!(text.trim(), r#"lang = "it""#);
        assert_eq!(toml::from_str::<GuiSettings>(&text).unwrap().lang, Some(Lang::It));
    }
}
