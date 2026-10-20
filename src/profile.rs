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
use crate::screen::{Crop, Fit};
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Screen {
    /// PNG/JPEG/BMP/WebP or animated GIF; relative paths are relative to the profile file.
    pub image: PathBuf,
    #[serde(default)]
    pub fit: Fit,
    /// Visible part of the image with `fit = "crop"`.
    #[serde(default, skip_serializing_if = "Crop::is_default")]
    pub crop: Crop,
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

impl Profile {
    /// The profile as a complete configuration to switch to: keys and macros
    /// are always rewritten (no remaps = factory layout) and the clock is synced.
    pub fn as_full_switch(&self) -> Profile {
        let mut p = self.clone();
        p.sync_clock = Some(true);
        if p.keymap.is_none() {
            p.keymap = Some(Keymap::default());
        }
        p
    }
}

impl Profile {
    /// What has to be sent to go from `current` (the last applied
    /// configuration) to this profile, as a full switch. `None` if nothing differs.
    ///
    /// Sections are compared as a whole: lighting and per-key colors together,
    /// key maps and macros together, system settings, and the screen (image,
    /// fit and crop). The clock is synced only when the screen changes, since
    /// setting it brings the clock face back over an image.
    pub fn switch_from(&self, current: &Profile) -> Option<Profile> {
        let mut out = Profile::default();
        let mut any = false;
        if (&self.lighting, &self.key_colors) != (&current.lighting, &current.key_colors)
            && (self.lighting.is_some() || self.key_colors.is_some())
        {
            out.lighting = self.lighting.clone();
            out.key_colors = self.key_colors.clone();
            any = true;
        }
        let keys = |p: &Profile| (p.keymap.clone().unwrap_or_default(), p.macros.clone());
        if keys(self) != keys(current) {
            out.keymap = Some(self.keymap.clone().unwrap_or_default());
            out.macros = self.macros.clone();
            any = true;
        }
        if self.system.is_some() && self.system != current.system {
            out.system = self.system;
            any = true;
        }
        let screen = |p: &Profile| {
            p.screen.as_ref().map(|s| (std::fs::canonicalize(&s.image).unwrap_or(s.image.clone()), s.fit, s.crop))
        };
        if screen(self) != screen(current) {
            out.screen = self.screen.clone();
            out.sync_clock = Some(true);
            any = true;
        }
        any.then_some(out)
    }
}

// ---- named profiles --------------------------------------------------------------------

/// Folder holding the named profiles (`NAME.toml` plus a copy of each screen image).
pub fn profiles_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("AK820_PROFILES") {
        return PathBuf::from(p);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ak820")
        .join("profiles")
}

fn check_name(name: &str) -> Result<()> {
    let bad = name.trim().is_empty()
        || name != name.trim()
        || name.starts_with('.')
        || name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control());
    if bad {
        return Err(Error::Invalid(format!("'{name}' is not a valid profile name")));
    }
    Ok(())
}

/// Names of the saved profiles, sorted.
pub fn list_profiles() -> Vec<String> {
    let Ok(dir) = std::fs::read_dir(profiles_dir()) else { return Vec::new() };
    let mut names: Vec<String> = dir
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

fn image_copies(name: &str) -> Vec<PathBuf> {
    let dir = profiles_dir();
    ["png", "jpg", "jpeg", "gif", "bmp", "webp"]
        .iter()
        .map(|e| dir.join(format!("{name}.{e}")))
        .filter(|p| p.exists())
        .collect()
}

/// Save `p` as the named profile. The screen image is copied next to it, so
/// the profile keeps working if the original file moves.
pub fn save_named(name: &str, p: &Profile) -> Result<()> {
    check_name(name)?;
    let dir = profiles_dir();
    std::fs::create_dir_all(&dir)?;
    let mut p = p.clone();
    p.sync_clock = None;
    if let Some(screen) = p.screen.as_mut() {
        let ext = screen.image.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "png".into());
        let file = format!("{name}.{ext}");
        let dest = dir.join(&file);
        let same = std::fs::canonicalize(&screen.image).ok() == std::fs::canonicalize(&dest).ok();
        if !same {
            for old in image_copies(name) {
                let _ = std::fs::remove_file(old);
            }
            std::fs::copy(&screen.image, &dest)
                .map_err(|e| Error::Invalid(format!("cannot copy {}: {e}", screen.image.display())))?;
        }
        screen.image = PathBuf::from(file);
    } else {
        for old in image_copies(name) {
            let _ = std::fs::remove_file(old);
        }
    }
    p.save(&dir.join(format!("{name}.toml")))
}

pub fn load_named(name: &str) -> Result<Profile> {
    check_name(name)?;
    let path = profiles_dir().join(format!("{name}.toml"));
    if !path.exists() {
        return Err(Error::Invalid(format!("no profile named '{name}'")));
    }
    Profile::load(&path)
}

pub fn delete_named(name: &str) -> Result<()> {
    check_name(name)?;
    let dir = profiles_dir();
    std::fs::remove_file(dir.join(format!("{name}.toml")))?;
    for img in image_copies(name) {
        let _ = std::fs::remove_file(img);
    }
    Ok(())
}

pub fn rename_named(old: &str, new: &str) -> Result<()> {
    check_name(new)?;
    if list_profiles().iter().any(|n| n == new) {
        return Err(Error::Invalid(format!("a profile named '{new}' already exists")));
    }
    let p = load_named(old)?;
    save_named(new, &p)?;
    delete_named(old)
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
    fn named_profiles() {
        let dir = std::env::temp_dir().join(format!("ak820-test-{}", std::process::id()));
        std::env::set_var("AK820_PROFILES", &dir);
        let img = dir.join("src.png");
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbImage::new(4, 4).save(&img).unwrap();
        let p = Profile {
            lighting: Some(Lighting::factory()),
            screen: Some(Screen { image: img.clone(), fit: Fit::Crop, crop: Crop { zoom: 2.0, x: 0.3, y: 0.5 } }),
            ..Profile::default()
        };
        save_named("Gaming", &p).unwrap();
        assert!(dir.join("Gaming.png").exists());
        let back = load_named("Gaming").unwrap();
        assert_eq!(back.screen.as_ref().unwrap().image, dir.join("Gaming.png"));
        assert_eq!(back.screen.as_ref().unwrap().crop.zoom, 2.0);
        // saving it again from its own copy keeps the image
        save_named("Gaming", &back).unwrap();
        assert!(dir.join("Gaming.png").exists());
        rename_named("Gaming", "Work").unwrap();
        assert_eq!(list_profiles(), vec!["Work".to_string()]);
        assert!(dir.join("Work.png").exists() && !dir.join("Gaming.png").exists());
        assert!(save_named("../x", &p).is_err());
        let full = back.as_full_switch();
        assert_eq!(full.keymap, Some(Keymap::default()));
        assert_eq!(full.sync_clock, Some(true));
        delete_named("Work").unwrap();
        assert!(list_profiles().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn switch_sends_only_differences() {
        let base = Profile {
            lighting: Some(Lighting::factory()),
            keymap: Some(Keymap::default()),
            system: Some(System::default()),
            ..Profile::default()
        };
        assert!(base.switch_from(&base).is_none());
        // a missing keymap means the factory layout
        let mut no_keys = base.clone();
        no_keys.keymap = None;
        assert!(no_keys.switch_from(&base).is_none());
        // only the lighting changed
        let mut red = base.clone();
        red.lighting = Some(Lighting::new(crate::lighting::Mode::Static));
        let d = red.switch_from(&base).unwrap();
        assert!(d.lighting.is_some() && d.keymap.is_none() && d.system.is_none() && d.screen.is_none());
        assert_eq!(d.sync_clock, None);
        // a remap
        let mut remap = base.clone();
        remap.keymap.as_mut().unwrap().normal.insert("pgdn".into(), "end".into());
        let d = remap.switch_from(&base).unwrap();
        assert!(d.keymap.is_some() && d.lighting.is_none());
        // removing the screen image brings the clock back
        let mut with_screen = base.clone();
        with_screen.screen = Some(Screen { image: "x.png".into(), fit: Fit::Crop, crop: Crop::default() });
        let d = base.switch_from(&with_screen).unwrap();
        assert!(d.screen.is_none() && d.sync_clock == Some(true));
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(toml::from_str::<Profile>("[lighting]\nmode='static'\nfoo=1").is_err());
    }
}
