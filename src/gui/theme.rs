//! Which theme the window uses: the one picked in the settings, or the system's.

use eframe::egui::{Theme, ThemePreference};
use serde::{Deserialize, Serialize};

/// The theme picked in System → Application.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    #[default]
    System,
    Light,
    Dark,
}

impl Choice {
    pub const ALL: [Choice; 3] = [Choice::System, Choice::Light, Choice::Dark];
}

/// `AK820_GUI_THEME=light|dark` for a single run, else `choice`; `System` asks the system.
pub fn preference(choice: Choice) -> ThemePreference {
    let choice = match std::env::var("AK820_GUI_THEME").as_deref() {
        Ok("light") => Choice::Light,
        Ok("dark") => Choice::Dark,
        _ => choice,
    };
    match choice {
        Choice::Light => Theme::Light.into(),
        Choice::Dark => Theme::Dark.into(),
        Choice::System => system().map_or(ThemePreference::System, Into::into),
    }
}

/// winit reports no system theme on X11 or Wayland, so ask the XDG settings portal
/// (the one the file picker already uses). `None` without a portal or a preference:
/// egui then falls back to dark.
#[cfg(target_os = "linux")]
fn system() -> Option<Theme> {
    use ashpd::desktop::settings::{ColorScheme, Settings};
    let scheme = pollster::block_on(async { Settings::new().await?.color_scheme().await }).ok()?;
    match scheme {
        ColorScheme::PreferDark => Some(Theme::Dark),
        ColorScheme::PreferLight => Some(Theme::Light),
        ColorScheme::NoPreference => None,
    }
}

/// Windows reports the theme through winit: egui's `System` preference handles it.
#[cfg(not(target_os = "linux"))]
fn system() -> Option<Theme> {
    None
}
