//! Per-user GUI preferences, stored in `gui.toml` next to the state file
//! (never in the keyboard profile).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub lang: Option<Lang>,
    /// Closing the window keeps the app running in the system tray.
    pub close_to_tray: bool,
    /// Sync the screen clock when the app starts.
    pub sync_clock_on_start: bool,
    /// Last profile applied from the profile list.
    pub active_profile: Option<String>,
}

fn path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("AK820_GUI_SETTINGS") {
        return Some(PathBuf::from(p));
    }
    Some(dirs::config_dir()?.join("ak820").join("gui.toml"))
}

impl Settings {
    pub fn load() -> Settings {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = path() else { return Ok(()) };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_old_files() {
        let s = Settings { lang: Some(Lang::It), close_to_tray: true, ..Settings::default() };
        let text = toml::to_string(&s).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), s);
        // gui.toml written by older versions only had the language
        let old: Settings = toml::from_str(r#"lang = "en""#).unwrap();
        assert_eq!(old.lang, Some(Lang::En));
        assert!(!old.close_to_tray);
    }
}
