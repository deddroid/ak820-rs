//! A complete keyboard configuration, stored as TOML.
//!
//! The keyboard cannot be read back, so the last applied configuration is kept
//! in a state file and every change is merged into it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::keys;
use crate::lighting::Lighting;
use crate::macros::{self, MacroDef};
use crate::protocol::{self, SystemSettings};
use crate::screen::Fit;
use crate::{Error, Result, Rgb};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Set the screen clock to the computer time when applying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_clock: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lighting: Option<Lighting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_colors: Option<KeyColors>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<System>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keymap: Option<Keymap>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub macros: BTreeMap<String, MacroDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<Screen>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyColors {
    /// Color of every key not listed in `keys`.
    #[serde(default)]
    pub base: Rgb,
    #[serde(default = "five")]
    pub brightness: u8,
    #[serde(default)]
    pub keys: BTreeMap<String, Rgb>,
}

impl Default for KeyColors {
    fn default() -> Self {
        KeyColors { base: Rgb::default(), brightness: 5, keys: BTreeMap::new() }
    }
}

fn five() -> u8 {
    5
}

impl KeyColors {
    pub fn resolve(&self) -> Result<Vec<(u8, Rgb)>> {
        for name in self.keys.keys() {
            keys::position(name).ok_or_else(|| Error::UnknownKey(name.clone()))?;
        }
        Ok(keys::all_lights()
            .map(|(_, idx)| {
                let color = self
                    .keys
                    .iter()
                    .find(|(k, _)| keys::position(k) == Some(idx))
                    .map(|(_, c)| *c)
                    .unwrap_or(self.base);
                (idx, color)
            })
            .collect())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum SleepTime {
    Never,
    #[serde(rename = "1m")]
    #[value(name = "1m")]
    Min1,
    #[default]
    #[serde(rename = "5m")]
    #[value(name = "5m")]
    Min5,
    #[serde(rename = "30m")]
    #[value(name = "30m")]
    Min30,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum FnMode {
    /// Fn layer while Fn is held.
    #[default]
    Hold,
    /// Fn toggles the Fn layer.
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct System {
    /// Only applies on battery.
    #[serde(default)]
    pub sleep: SleepTime,
    #[serde(default)]
    pub fn_mode: FnMode,
    /// Debounce: 1 (fast) ..= 5 (slow).
    #[serde(default = "two")]
    pub response: u8,
    #[serde(default)]
    pub block_win: bool,
    #[serde(default)]
    pub block_alt_f4: bool,
    #[serde(default)]
    pub block_alt_tab: bool,
}

fn two() -> u8 {
    2
}

impl Default for System {
    fn default() -> Self {
        System {
            sleep: SleepTime::default(),
            fn_mode: FnMode::default(),
            response: 2,
            block_win: false,
            block_alt_f4: false,
            block_alt_tab: false,
        }
    }
}

impl System {
    pub fn settings(&self) -> Result<SystemSettings> {
        if !(1..=5).contains(&self.response) {
            return Err(Error::Invalid("system.response must be 1..=5".into()));
        }
        Ok(SystemSettings {
            block_win: self.block_win,
            block_alt_f4: self.block_alt_f4,
            block_alt_tab: self.block_alt_tab,
            fn_toggle: self.fn_mode == FnMode::Toggle,
            sleep: self.sleep as u8,
            response: self.response,
        })
    }
}

/// `key = "target"` assignments; see [`keys::encode_target`] for targets.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keymap {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub normal: BTreeMap<String, String>,
    #[serde(default, rename = "fn", skip_serializing_if = "BTreeMap::is_empty")]
    pub fn_layer: BTreeMap<String, String>,
}

pub struct CompiledKeymap {
    pub normal: Vec<u8>,
    pub fn_layer: Vec<u8>,
    pub macros: Vec<Vec<[u8; 4]>>,
}

impl Keymap {
    pub fn compile(&self, named: &BTreeMap<String, MacroDef>) -> Result<CompiledKeymap> {
        let mut list = Vec::new();
        let mut layer = |map: &BTreeMap<String, String>, fn_layer: bool| -> Result<Vec<u8>> {
            let mut slots = Vec::new();
            for (key, target) in map {
                let pos = keys::check_remappable(key, fn_layer)?;
                let slot = keys::encode_target(target, named, &mut list)
                    .map_err(|e| Error::Invalid(format!("{key}: {e}")))?;
                slots.push((pos, slot));
            }
            Ok(protocol::table(slots))
        };
        let normal = layer(&self.normal, false)?;
        let fn_layer = layer(&self.fn_layer, true)?;
        let macros = list
            .iter()
            .map(|m| macros::events(&m.steps))
            .collect::<Result<Vec<_>>>()?;
        if macros::block(&macros, protocol::PACKET).is_none()
            || macros::block(&macros, protocol::RF_CHUNK).is_none()
        {
            return Err(Error::Invalid("the macros do not fit in the keyboard memory (3.5 KB)".into()));
        }
        Ok(CompiledKeymap { normal, fn_layer, macros })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Screen {
    /// PNG/JPEG/BMP/WebP or animated GIF; relative paths are relative to the profile file.
    pub image: PathBuf,
    #[serde(default)]
    pub fit: Fit,
}

impl Profile {
    pub fn load(path: &Path) -> Result<Profile> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Invalid(format!("cannot read {}: {e}", path.display())))?;
        let mut p: Profile = toml::from_str(&text)
            .map_err(|e| Error::Invalid(format!("{}: {e}", path.display())))?;
        if let (Some(s), Some(dir)) = (p.screen.as_mut(), path.parent()) {
            if s.image.is_relative() {
                s.image = dir.join(&s.image);
            }
        }
        Ok(p)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| Error::Invalid(e.to_string()))?;
        std::fs::write(path, text)?;
        Ok(())
    }

    /// Check everything that can be checked without the keyboard.
    pub fn validate(&self) -> Result<()> {
        if let Some(k) = &self.keymap {
            k.compile(&self.macros)?;
        }
        if let Some(c) = &self.key_colors {
            c.resolve()?;
        }
        if let Some(s) = &self.system {
            s.settings()?;
        }
        if let Some(s) = &self.screen {
            if !s.image.exists() {
                return Err(Error::Invalid(format!("screen image {} not found", s.image.display())));
            }
        }
        Ok(())
    }

    /// Overlay the sections set in `other` onto `self`.
    pub fn merge(&mut self, other: &Profile) {
        if other.lighting.is_some() {
            self.lighting = other.lighting.clone();
        }
        if other.key_colors.is_some() {
            self.key_colors = other.key_colors.clone();
        }
        if other.system.is_some() {
            self.system = other.system;
        }
        if other.keymap.is_some() {
            self.keymap = other.keymap.clone();
            self.macros = other.macros.clone();
        }
        if other.screen.is_some() {
            self.screen = other.screen.clone();
        }
    }
}

/// Where the last applied configuration is remembered.
pub fn state_path() -> PathBuf {
    if let Some(p) = std::env::var_os("AK820_STATE") {
        return PathBuf::from(p);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ak820")
        .join("state.toml")
}

pub fn load_state() -> Profile {
    let path = state_path();
    if !path.exists() {
        return Profile::default();
    }
    Profile::load(&path).unwrap_or_else(|e| {
        eprintln!("warning: ignoring unreadable state file: {e}");
        Profile::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../examples/profile.toml");

    #[test]
    fn example_profile_parses_and_compiles() {
        let p: Profile = toml::from_str(EXAMPLE).unwrap();
        let km = p.keymap.as_ref().unwrap().compile(&p.macros).unwrap();
        assert_eq!(km.normal.len(), 576);
        assert!(!km.macros.is_empty());
        p.key_colors.as_ref().unwrap().resolve().unwrap();
        p.system.unwrap().settings().unwrap();
        // round-trip through TOML
        let again: Profile = toml::from_str(&toml::to_string_pretty(&p).unwrap()).unwrap();
        assert_eq!(again, p);
    }

    #[test]
    fn key_colors_resolve() {
        let c = KeyColors {
            base: Rgb(0, 0, 32),
            brightness: 5,
            keys: BTreeMap::from([("w".to_string(), Rgb(255, 0, 0))]),
        };
        let r = c.resolve().unwrap();
        assert_eq!(r.len(), 81);
        assert!(r.contains(&(39, Rgb(255, 0, 0))));
        assert!(r.contains(&(1, Rgb(0, 0, 32))));
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(toml::from_str::<Profile>("[lighting]\nmode='static'\nfoo=1").is_err());
    }
}
