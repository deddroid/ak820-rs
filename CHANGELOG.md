# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.5.1] - 2026-11-08

### Fixed
- Linux: the window no longer turns black or lags behind while it is resized. It now runs on
  native Wayland instead of XWayland, and dropping an image on it still works there.

## [0.5.0] - 2026-11-08

### Added
- **Shortcuts** tab: the keyboard's own Fn combinations from the AJAZZ manual (lighting, screen,
  connection, F keys on Windows and Mac), for the model in use. Combinations replaced by a Fn-layer
  remap are marked.
- Macros: how long each key or click stays pressed (`tap@80:ctrl+c`, also for text and clicks).
- Macros: the number of repetitions is typed and goes past 255 (`--count` up to 65535). The keyboard
  counts up to 253; past that the steps are copied inside the macro.

### Changed
- The Macros tab lists each macro with the key that plays it, a sentence saying what it does and
  whether it is on the keyboard, not sent yet or not assigned. Steps are edited one per row instead
  of as text, a macro is assigned to a key from the same page, and the memory it uses is shown.
- Color swatches are round, like the other controls.

### Fixed
- AK820: a macro repeated 254 or 255 times played as "start/stop" or "while held".
- AK820: a macro switched from "repeat N times" back to "once" kept repeating.

## [0.4.0] - 2026-11-08

### Added
- **Theme** in System → Application: same as the system, light or dark. The choice is saved.

### Fixed
- The selected lighting effect had square corners instead of rounded ones like every other button.

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

[Unreleased]: https://github.com/deddroid/ak820-rs/compare/v0.5.1...HEAD
[0.5.1]: https://github.com/deddroid/ak820-rs/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/deddroid/ak820-rs/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/deddroid/ak820-rs/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/deddroid/ak820-rs/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/deddroid/ak820-rs/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/deddroid/ak820-rs/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/deddroid/ak820-rs/releases/tag/v0.1.0
