# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.0] - 2026-11-08

### Added
- Support for the AJAZZ AK820 (non-Pro, wired RGB, `320f:505b`): lighting, per-key colors,
  live demo, remapping on the normal layer, macros, debounce, Win lock, polling rate, NKRO,
  WASD/arrows swap. The model is detected automatically and the GUI shows its layout.
  Contributed by koloved.
- `ak820 read` and **Read from keyboard**: read the configuration back from the AK820.

## [0.2.0] - 2026-11-08

### Added
- The window follows the system theme: a warm light palette and the violet dark one,
  same hierarchy in both. `AK820_GUI_THEME=light|dark` forces one.
- Embedded fonts: Inter for text and controls, Rajdhani for headings (SIL Open Font License).
- AUR packages `ak820` (from source) and `ak820-bin` (prebuilt), with install instructions.

### Changed
- On/off settings are switches instead of checkboxes; sleep time and Fn mode are drop-down menus.
- Rounded corners, spacing and hover/press states follow one consistent scale.
- Building from source needs Rust 1.89 or newer (now declared in `Cargo.toml`).

### Fixed
- The English README named the "Grant access" button in Italian.

## [0.1.1] - 2026-10-20

### Added
- MIT license.

### Changed
- Lighting: the real static effect (`01`) and per-effect parameters; the redundant
  "Lights off" button is gone.
- Profiles: applying a profile sends only what differs, and nothing when it is already in use.

## [0.1.0] - 2026-10-19

### Added
- Command line `ak820`: lighting, per-key colors, key remapping, macros, screen images and
  GIFs, clock, system settings, TOML profiles, 2.4 GHz receiver.
- Graphical app `ak820-gui` for Linux and Windows: tray icon, autostart, saved profiles,
  draggable screen crop, image drag & drop and paste.
- English by default, Italian selectable.
- Two directions per moving effect, app icon, `install.sh` installer.
- CI and release workflows with prebuilt Linux and Windows archives.

[Unreleased]: https://github.com/deddroid/ak820-rs/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/deddroid/ak820-rs/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/deddroid/ak820-rs/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/deddroid/ak820-rs/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/deddroid/ak820-rs/releases/tag/v0.1.0
