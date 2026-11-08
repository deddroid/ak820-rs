//! Which theme the window starts with.

use eframe::egui::{Theme, ThemePreference};

/// `AK820_GUI_THEME=light|dark`, else the system's preference.
pub fn preference() -> ThemePreference {
    match std::env::var("AK820_GUI_THEME").as_deref() {
        Ok("light") => Theme::Light.into(),
        Ok("dark") => Theme::Dark.into(),
        _ => system().map_or(ThemePreference::System, Into::into),
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
