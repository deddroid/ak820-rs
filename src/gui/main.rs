//! Graphical interface for the AK820 Pro (egui), sharing the CLI state file.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod autostart;
mod i18n;
mod instance;
mod layout;
mod settings;
mod tray;
mod worker;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Button, Color32, ComboBox, CornerRadius, FontId, Pos2, Rect, RichText, Slider, Stroke, TextStyle,
    TextureHandle, TextureOptions, Ui, Vec2,
};

use ak820::lighting::{Direction, Lighting, Mode};
use ak820::macros::{MacroDef, MacroMode};
use ak820::profile::{self, FnMode, KeyColors, Keymap, Profile, Screen, SleepTime, System};
use ak820::screen::{self, Crop, Fit};
use ak820::{actions, keys, Rgb};

use i18n::{t, Lang};
use layout::{KeyLook, LAYOUT};
use settings::Settings;
use tray::{TrayCmd, Wake};
use worker::{Job, Msg, Op, Status};

const ACCENT: Color32 = Color32::from_rgb(124, 92, 255);
const ACCENT_SOFT: Color32 = Color32::from_rgb(70, 56, 140);
const OK: Color32 = Color32::from_rgb(80, 200, 120);
const WARN: Color32 = Color32::from_rgb(240, 180, 60);
const ERR: Color32 = Color32::from_rgb(235, 90, 90);
const KEYCAP: Color32 = Color32::from_rgb(48, 48, 60);

fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AK820 Pro")
            .with_app_id("ak820")
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([900.0, 620.0])
            .with_icon(app_icon()),
        ..Default::default()
    }
}

/// The window is closed and reopened while the app lives on in the tray, so the
/// app state outlives each window. (Wayland cannot hide a window: closing it is
/// the only portable way to go to the tray.)
struct Window(Arc<Mutex<App>>);

impl eframe::App for Window {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        eframe::App::update(&mut *self.0.lock().unwrap(), ctx, frame);
    }
}

fn main() -> eframe::Result {
    let start_in_tray = std::env::args().any(|a| a == autostart::TRAY_FLAG);
    let screenshots = std::env::var_os("AK820_GUI_SHOTS").is_some();
    let settings = Settings::load();
    i18n::init(settings.lang);

    // The current window's context, if a window is open: used to wake the UI.
    let ctx_slot: Arc<Mutex<Option<egui::Context>>> = Arc::default();
    let wake: Wake = {
        let slot = ctx_slot.clone();
        Arc::new(move || {
            if let Some(ctx) = slot.lock().unwrap().as_ref() {
                ctx.request_repaint();
            }
        })
    };
    let (tray_tx, tray_rx) = channel();
    if !screenshots {
        let (tx, w) = (tray_tx.clone(), wake.clone());
        let primary = instance::acquire(move || {
            let _ = tx.send(TrayCmd::Open);
            w();
        });
        if !primary {
            return Ok(()); // the running instance shows its window
        }
    }

    let app = Arc::new(Mutex::new(App::new(settings, wake, tray_tx, tray_rx, start_in_tray)));
    let mut show = !(start_in_tray && app.lock().unwrap().tray.is_some());
    loop {
        if show {
            let (a, slot) = (app.clone(), ctx_slot.clone());
            eframe::run_native(
                "AK820 Pro",
                native_options(),
                Box::new(move |cc| {
                    *slot.lock().unwrap() = Some(cc.egui_ctx.clone());
                    a.lock().unwrap().attach(&cc.egui_ctx);
                    Ok(Box::new(Window(a)))
                }),
            )?;
            *ctx_slot.lock().unwrap() = None;
            let a = app.lock().unwrap();
            if a.quit || a.tray.is_none() {
                break;
            }
            show = false;
        } else {
            // Window closed, icon in the tray: wait for the tray menu.
            let cmd = app.lock().unwrap().tray_rx.recv_timeout(Duration::from_millis(500));
            let mut a = app.lock().unwrap();
            match cmd {
                Ok(TrayCmd::Open) => show = true,
                Ok(TrayCmd::Apply(name)) => a.apply_named(&name),
                Ok(TrayCmd::Quit) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            a.handle_messages();
        }
    }
    Ok(())
}

fn app_icon() -> egui::IconData {
    let img = image::load_from_memory(include_bytes!("../../assets/icon-256.png"))
        .expect("embedded icon")
        .to_rgba8();
    let (width, height) = img.dimensions();
    egui::IconData { rgba: img.into_raw(), width, height }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
    Lighting,
    Colors,
    Keys,
    Macros,
    Screen,
    System,
    Profiles,
}

/// `(tab, icon, English, Italian)`; screenshot files are named after the English label.
const TABS: &[(Tab, &str, &str, &str)] = &[
    (Tab::Lighting, "💡", "Lighting", "Luci"),
    (Tab::Colors, "🎨", "Key colors", "Colori tasti"),
    (Tab::Keys, "⌨", "Keys", "Tasti"),
    (Tab::Macros, "📝", "Macros", "Macro"),
    (Tab::Screen, "🖼", "Screen", "Display"),
    (Tab::System, "⚙", "System", "Sistema"),
    (Tab::Profiles, "📁", "Profiles", "Profili"),
];

fn mode_label(m: Mode) -> &'static str {
    let (en, it) = match m {
        Mode::Off => ("Off", "Spento"),
        Mode::Static => ("Static", "Fisso"),
        Mode::SingleOn => ("Single on", "Tasto acceso"),
        Mode::SingleOff => ("Single off", "Tasto spento"),
        Mode::Glittering => ("Glittering", "Scintillio"),
        Mode::Falling => ("Falling", "Pioggia"),
        Mode::Colourful => ("Colourful", "Colorato"),
        Mode::Breath => ("Breath", "Respiro"),
        Mode::Spectrum => ("Spectrum", "Spettro"),
        Mode::Outward => ("Outward", "Onde"),
        Mode::Scrolling => ("Scrolling", "Scorrimento"),
        Mode::Rolling => ("Rolling", "Onda"),
        Mode::Rotating => ("Rotating", "Rotazione"),
        Mode::Explode => ("Explode", "Esplosione"),
        Mode::Launch => ("Launch", "Lancio"),
        Mode::Ripples => ("Ripples", "Increspature"),
        Mode::Flowing => ("Flowing", "Flusso"),
        Mode::Pulsating => ("Pulsating", "Pulsazione"),
        Mode::Tilt => ("Tilt", "Inclinazione"),
        Mode::Shuttle => ("Shuttle", "Navetta"),
        Mode::Custom => ("Per key", "Per tasto"),
    };
    t(en, it)
}

fn direction_label(d: Direction) -> &'static str {
    match d {
        Direction::Left => t("← Leftward", "← Verso sinistra"),
        Direction::Right => t("Rightward →", "Verso destra →"),
        Direction::Up => t("↑ Upward", "↑ Verso l'alto"),
        Direction::Down => t("↓ Downward", "↓ Verso il basso"),
    }
}

fn color32(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

// ---- remap target editor ------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Default,
    Key,
    Media,
    Mouse,
    Multi,
    Text,
    Macro,
}

const KINDS: &[(Kind, &str, &str)] = &[
    (Kind::Key, "Key / shortcut", "Tasto / scorciatoia"),
    (Kind::Media, "Media", "Multimediale"),
    (Kind::Mouse, "Mouse", "Mouse"),
    (Kind::Multi, "Several keys at once", "Più tasti insieme"),
    (Kind::Text, "Type text", "Scrivi testo"),
    (Kind::Macro, "Saved macro", "Macro salvata"),
    (Kind::Default, "Original", "Originale"),
];

const KEY_NAMES: &[&str] = &[
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u",
    "v", "w", "x", "y", "z", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "f1", "f2", "f3", "f4", "f5",
    "f6", "f7", "f8", "f9", "f10", "f11", "f12", "f13", "f14", "f15", "f16", "f17", "f18", "f19", "f20",
    "f21", "f22", "f23", "f24", "enter", "esc", "backspace", "tab", "space", "minus", "equal", "lbracket",
    "rbracket", "backslash", "semicolon", "quote", "grave", "comma", "period", "slash", "capslock",
    "printscreen", "scrolllock", "pause", "insert", "home", "end", "pgup", "pgdn", "delete", "up", "down",
    "left", "right", "numlock", "menu",
];

const MOUSE: &[(&str, &str, &str)] = &[
    ("mouse:left", "Left click", "Clic sinistro"),
    ("mouse:right", "Right click", "Clic destro"),
    ("mouse:middle", "Middle click", "Clic centrale"),
    ("wheel:up", "Wheel up", "Rotella su"),
    ("wheel:down", "Wheel down", "Rotella giù"),
];

const MODS: [&str; 4] = ["ctrl", "shift", "alt", "win"];

#[derive(Clone)]
struct TargetEditor {
    kind: Kind,
    mods: [bool; 4],
    key: String,
    media: String,
    mouse: String,
    multi: [String; 3],
    text: String,
    macro_name: String,
}

impl Default for TargetEditor {
    fn default() -> Self {
        TargetEditor {
            kind: Kind::Key,
            mods: [false; 4],
            key: "a".into(),
            media: "play".into(),
            mouse: "mouse:left".into(),
            multi: Default::default(),
            text: String::new(),
            macro_name: String::new(),
        }
    }
}

impl TargetEditor {
    fn from_target(t: &str) -> Self {
        let mut e = TargetEditor::default();
        let lower = t.to_ascii_lowercase();
        if let Some(s) = t.strip_prefix("text:") {
            e.kind = Kind::Text;
            e.text = s.into();
        } else if let Some(s) = t.strip_prefix("macro:") {
            e.kind = Kind::Macro;
            e.macro_name = s.into();
        } else if let Some(s) = lower.strip_prefix("multi:") {
            e.kind = Kind::Multi;
            for (i, k) in s.split('+').take(3).enumerate() {
                e.multi[i] = k.into();
            }
        } else if MOUSE.iter().any(|(v, _, _)| *v == lower) {
            e.kind = Kind::Mouse;
            e.mouse = lower;
        } else if keys::CONSUMER.iter().any(|(n, _)| *n == lower) {
            e.kind = Kind::Media;
            e.media = lower;
        } else if lower == "default" {
            e.kind = Kind::Default;
        } else {
            e.kind = Kind::Key;
            e.key.clear();
            for part in lower.split('+') {
                match MODS.iter().position(|m| *m == part) {
                    Some(i) => e.mods[i] = true,
                    None => e.key = part.into(),
                }
            }
        }
        e
    }

    fn target(&self) -> String {
        match self.kind {
            Kind::Default => "default".into(),
            Kind::Key => {
                let mut parts: Vec<&str> =
                    MODS.iter().zip(self.mods).filter(|(_, on)| *on).map(|(m, _)| *m).collect();
                if !self.key.is_empty() {
                    parts.push(&self.key);
                }
                parts.join("+")
            }
            Kind::Media => self.media.clone(),
            Kind::Mouse => self.mouse.clone(),
            Kind::Multi => {
                let k: Vec<&str> = self.multi.iter().filter(|s| !s.is_empty()).map(String::as_str).collect();
                format!("multi:{}", k.join("+"))
            }
            Kind::Text => format!("text:{}", self.text),
            Kind::Macro => format!("macro:{}", self.macro_name),
        }
    }
}

// ---- screenshots for documentation / visual checks ---------------------------------------

struct Shots {
    dir: PathBuf,
    index: usize,
    wait: u32,
    requested: bool,
}

// ---- app ---------------------------------------------------------------------------------

struct App {
    tab: Tab,
    settings: Settings,
    autostart: bool,
    tray: Option<tray::Handle>,
    tray_tx: Sender<TrayCmd>,
    tray_rx: Receiver<TrayCmd>,
    wake: Wake,
    quit: bool,
    clock_synced: bool,
    state: Profile,
    busy: bool,
    resync_after_job: bool,
    progress: f32,
    status: Status,
    log: Vec<(String, bool)>,
    show_log: bool,
    jobs: Sender<Job>,
    msgs: Receiver<Msg>,

    light: Lighting,
    colors: KeyColors,
    brush: Rgb,

    fn_layer: bool,
    keymap: Keymap,
    macros: BTreeMap<String, MacroDef>,
    selected: Option<&'static str>,
    editor: TargetEditor,
    key_error: Option<String>,

    macro_sel: Option<String>,
    macro_name: String,
    macro_steps: String,
    macro_mode: MacroMode,
    macro_count: u8,
    macro_error: Option<String>,

    screen_path: Option<PathBuf>,
    fit: Fit,
    crop: Crop,
    /// Decoded frames (downscaled for the preview) with their durations.
    source: Vec<(image::RgbaImage, u32)>,
    /// Size of the original image, which the crop refers to.
    source_size: (u32, u32),
    textures: Option<Vec<TextureHandle>>,
    preview_start: Instant,
    screen_error: Option<String>,

    system: System,
    battery: Option<Option<u8>>,
    confirm_reset: bool,
    profile_error: Option<String>,
    profiles: Vec<(String, Profile)>,
    new_profile: String,
    renaming: Option<(String, String)>,
    confirm_delete: Option<String>,

    shots: Option<Shots>,
}

impl App {
    fn new(settings: Settings, wake: Wake, tray_tx: Sender<TrayCmd>, tray_rx: Receiver<TrayCmd>, start_in_tray: bool) -> Self {
        let (job_tx, job_rx) = channel();
        let (msg_tx, msg_rx) = channel();
        worker::spawn(job_rx, msg_tx, wake.clone());

        let state = profile::load_state();
        let mut app = App {
            tab: Tab::Lighting,
            autostart: autostart::is_enabled(),
            tray: None,
            tray_tx,
            tray_rx,
            wake,
            quit: false,
            clock_synced: false,
            state: Profile::default(),
            busy: false,
            resync_after_job: false,
            progress: 0.0,
            status: Status::Searching,
            log: Vec::new(),
            show_log: false,
            jobs: job_tx,
            msgs: msg_rx,
            light: Lighting::factory(),
            colors: KeyColors::default(),
            brush: Rgb(255, 0, 0),
            fn_layer: false,
            keymap: Keymap::default(),
            macros: BTreeMap::new(),
            selected: None,
            editor: TargetEditor::default(),
            key_error: None,
            macro_sel: None,
            macro_name: String::new(),
            macro_steps: String::new(),
            macro_mode: MacroMode::Once,
            macro_count: 1,
            macro_error: None,
            screen_path: None,
            fit: Fit::Crop,
            crop: Crop::default(),
            source: Vec::new(),
            source_size: (1, 1),
            textures: None,
            preview_start: Instant::now(),
            screen_error: None,
            system: System::default(),
            battery: None,
            confirm_reset: false,
            profile_error: None,
            profiles: Vec::new(),
            new_profile: String::new(),
            renaming: None,
            confirm_delete: None,
            shots: std::env::var_os("AK820_GUI_SHOTS")
                .map(|d| Shots { dir: d.into(), index: 0, wait: 0, requested: false }),
            settings,
        };
        app.state = state;
        app.load_drafts();
        app.refresh_profiles();
        if app.settings.close_to_tray || start_in_tray {
            app.set_tray(true);
        }
        if app.shots.is_some() {
            app.selected = Some("pgdn");
            app.editor = TargetEditor::from_target("ctrl+end");
            let demo = Path::new("examples/test-anim.gif");
            if demo.exists() {
                app.load_screen(demo.to_path_buf(), Crop { zoom: 1.6, x: 0.6, y: 0.45 });
            }
        }
        app
    }

    /// A new window was opened: style it and drop textures of the previous one.
    fn attach(&mut self, ctx: &egui::Context) {
        if std::env::var_os("AK820_DEBUG").is_some() {
            eprintln!("window opened");
        }
        setup_style(ctx);
        self.textures = None;
    }

    fn set_tray(&mut self, on: bool) {
        if on && self.tray.is_none() {
            let names = self.profiles.iter().map(|(n, _)| n.clone()).collect();
            self.tray = tray::spawn(self.tray_tx.clone(), self.wake.clone(), names, self.settings.active_profile.clone());
            if self.tray.is_none() {
                self.log.push((t("The system tray is not available", "La tray di sistema non è disponibile").into(), true));
            }
        } else if !on {
            self.tray = None;
        }
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            let msg = t("Could not save the settings", "Impossibile salvare le impostazioni");
            self.log.push((format!("{msg}: {e}"), true));
        }
    }

    fn refresh_profiles(&mut self) {
        self.profiles = profile::list_profiles()
            .into_iter()
            .map(|n| {
                let p = profile::load_named(&n).unwrap_or_default();
                (n, p)
            })
            .collect();
        if let Some(tray) = &self.tray {
            tray.set_profiles(self.profiles.iter().map(|(n, _)| n.clone()).collect(), self.settings.active_profile.clone());
        }
    }

    /// Switch the keyboard to a saved profile.
    fn apply_named(&mut self, name: &str) {
        match profile::load_named(name).and_then(|p| p.validate().map(|_| p)) {
            Ok(p) => {
                self.resync_after_job = true;
                let title = format!("{} {name}", t("Profile", "Profilo"));
                self.run(&title, Op::Apply(p.as_full_switch()), Some(p));
                self.settings.active_profile = Some(name.to_string());
                self.save_settings();
                self.refresh_profiles();
            }
            Err(e) => self.log.push((format!("{name}: {e}"), true)),
        }
    }

    /// Reset every editor to the remembered configuration.
    fn load_drafts(&mut self) {
        let s = &self.state;
        if let Some(l) = s.lighting.as_ref().filter(|l| l.mode != Mode::Custom) {
            self.light = l.clone();
        }
        self.colors = s.key_colors.clone().unwrap_or_default();
        self.keymap = s.keymap.clone().unwrap_or_default();
        self.macros = s.macros.clone();
        self.system = s.system.unwrap_or_default();
        if let Some(sc) = self.state.screen.clone() {
            if self.screen_path.as_ref() != Some(&sc.image) {
                self.fit = sc.fit;
                self.load_screen(sc.image, sc.crop);
            } else {
                self.fit = sc.fit;
                self.crop = sc.crop;
            }
        }
    }

    fn connected(&self) -> bool {
        matches!(self.status, Status::Wired(_) | Status::Receiver)
    }

    fn wired(&self) -> bool {
        matches!(self.status, Status::Wired(_))
    }

    fn run(&mut self, title: &str, op: Op, new_state: Option<Profile>) {
        self.busy = true;
        self.progress = 0.0;
        let _ = self.jobs.send(Job { title: title.into(), op, new_state });
    }

    fn handle_messages(&mut self) {
        while let Ok(m) = self.msgs.try_recv() {
            match m {
                Msg::Status(s) => {
                    if s == Status::Receiver {
                        self.battery = None;
                    }
                    self.status = s;
                    if self.settings.sync_clock_on_start && !self.clock_synced && self.connected() {
                        self.clock_synced = true;
                        self.run(t("Clock", "Orologio"), Op::Clock, None);
                    }
                }
                Msg::Log { text, error } => {
                    if error {
                        self.show_log = true;
                    }
                    self.log.push((text, error));
                    let excess = self.log.len().saturating_sub(300);
                    self.log.drain(..excess);
                }
                Msg::Progress(p) => self.progress = p,
                Msg::Battery(b) => self.battery = Some(b),
                Msg::Done { ok, new_state } => {
                    self.busy = false;
                    if let (true, Some(s)) = (ok, new_state) {
                        self.state = s;
                        if self.resync_after_job {
                            self.load_drafts();
                        }
                    }
                    self.resync_after_job = false;
                }
            }
        }
    }

    /// A primary action button, disabled while busy or without a keyboard.
    fn action(&self, ui: &mut Ui, text: &str, needs_wire: bool) -> bool {
        let enabled = !self.busy && if needs_wire { self.wired() } else { self.connected() };
        let button = Button::new(RichText::new(text).strong().color(Color32::WHITE))
            .fill(ACCENT)
            .min_size(Vec2::new(150.0, 34.0));
        let resp = ui.add_enabled(enabled, button);
        let resp = if needs_wire && self.status == Status::Receiver {
            resp.on_disabled_hover_text(t("Works only with the USB cable", "Funziona solo con il cavo USB"))
        } else {
            resp
        };
        resp.clicked()
    }

    // ---- tabs --------------------------------------------------------------------------

    fn tab_lighting(&mut self, ui: &mut Ui) {
        title(
            ui,
            t("Lighting effects", "Effetti luce"),
            t("The keyboard's built-in effects, saved in its memory.", "Gli effetti integrati della tastiera, salvati nella sua memoria."),
        );
        card(ui, t("Effect", "Effetto"), |ui| {
            let modes: Vec<Mode> = Mode::ALL.iter().copied().filter(|m| *m != Mode::Custom).collect();
            egui::Grid::new("modes").spacing([8.0, 8.0]).show(ui, |ui| {
                for (i, m) in modes.iter().enumerate() {
                    let selected = self.light.mode == *m;
                    let b = Button::new(mode_label(*m))
                        .selected(selected)
                        .min_size(Vec2::new(130.0, 32.0));
                    if ui.add(b).clicked() {
                        self.light.mode = *m;
                    }
                    if i % 5 == 4 {
                        ui.end_row();
                    }
                }
            });
        });
        ui.add_space(8.0);
        let off = self.light.mode == Mode::Off;
        card(ui, t("Parameters", "Parametri"), |ui| {
            ui.add_enabled_ui(!off, |ui| {
                egui::Grid::new("light-params").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                    ui.label(t("Color", "Colore"));
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!self.light.rainbow, |ui| color_button(ui, &mut self.light.color));
                        ui.checkbox(&mut self.light.rainbow, t("Rainbow", "Arcobaleno"));
                    });
                    ui.end_row();
                    ui.label(t("Brightness", "Luminosità"));
                    ui.add(Slider::new(&mut self.light.brightness, 0..=5));
                    ui.end_row();
                    ui.label(t("Speed", "Velocità"));
                    ui.add_enabled(self.light.mode != Mode::Static, Slider::new(&mut self.light.speed, 0..=5));
                    ui.end_row();
                    ui.label(t("Direction", "Direzione"));
                    match self.light.mode.direction_axis() {
                        Some(axis) => {
                            self.light.direction = self.light.direction.on_axis(axis);
                            ui.horizontal(|ui| {
                                for d in axis.directions() {
                                    ui.selectable_value(&mut self.light.direction, d, direction_label(d));
                                }
                            });
                        }
                        None => {
                            ui.label(RichText::new(t("this effect has no direction", "questo effetto non ha una direzione")).weak());
                        }
                    }
                    ui.end_row();
                });
            });
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if self.action(ui, t("✔ Apply", "✔ Applica"), false) {
                let l = self.light.clone();
                let mut s = self.state.clone();
                s.lighting = Some(l.clone());
                self.run(&format!("{}: {}", t("Lighting", "Luci"), mode_label(l.mode)), Op::Lighting(l), Some(s));
            }
            if ui.add_enabled(!self.busy && self.connected(), Button::new(t("Lights off", "Spegni luci"))).clicked() {
                let l = Lighting::new(Mode::Off);
                let mut s = self.state.clone();
                s.lighting = Some(l.clone());
                self.run(t("Lights off", "Luci spente"), Op::Lighting(l), Some(s));
            }
        });
    }

    fn tab_colors(&mut self, ui: &mut Ui) {
        title(
            ui,
            t("Per-key colors", "Colori per tasto"),
            t(
                "Click: paint with the brush · right-click: back to the base color. Cable only.",
                "Clic: colora con il pennello · clic destro: torna al colore di base. Solo via cavo.",
            ),
        );
        let clicked = layout::keyboard(ui, |name| {
            let color = self
                .colors
                .keys
                .get(name)
                .copied()
                .unwrap_or(self.colors.base);
            KeyLook { fill: color32(color), sub: None, dim: false, selected: false }
        });
        if let Some(c) = clicked {
            if c.secondary {
                self.colors.keys.remove(c.key);
            } else {
                self.colors.keys.insert(c.key.to_string(), self.brush);
            }
        }
        ui.add_space(12.0);
        ui.columns(2, |cols| {
            card(&mut cols[0], t("Tools", "Strumenti"), |ui| {
                egui::Grid::new("color-tools").num_columns(2).spacing([20.0, 10.0]).show(ui, |ui| {
                    ui.label(t("Brush", "Pennello"));
                    color_button(ui, &mut self.brush);
                    ui.end_row();
                    ui.label(t("Base color", "Colore di base"));
                    color_button(ui, &mut self.colors.base);
                    ui.end_row();
                    ui.label(t("Brightness", "Luminosità"));
                    ui.add(Slider::new(&mut self.colors.brightness, 1..=5));
                    ui.end_row();
                });
            });
            card(&mut cols[1], t("Quick fills", "Riempimenti rapidi"), |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button(t("All with the brush", "Tutto col pennello")).clicked() {
                        self.colors.base = self.brush;
                        self.colors.keys.clear();
                    }
                    if ui.button(t("Clear painted keys", "Cancella tasti colorati")).clicked() {
                        self.colors.keys.clear();
                    }
                    if ui.button(t("Rainbow", "Arcobaleno")).clicked() {
                        for (idx, x, y) in actions::key_grid() {
                            if let Some(name) = keys::canonical_name(idx) {
                                self.colors.keys.insert(name.into(), actions::hsv(x * 0.8 + y * 0.1));
                            }
                        }
                    }
                    if ui.button(t("WASD + arrows", "WASD + frecce")).clicked() {
                        for k in ["w", "a", "s", "d", "up", "down", "left", "right"] {
                            self.colors.keys.insert(k.into(), self.brush);
                        }
                    }
                });
            });
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if self.action(ui, t("✔ Apply colors", "✔ Applica colori"), true) {
                let c = self.colors.clone();
                let mut s = self.state.clone();
                s.key_colors = Some(c.clone());
                s.lighting = Some(Lighting::new(Mode::Custom));
                self.run(t("Per-key colors", "Colori per tasto"), Op::KeyColors(c), Some(s));
            }
            if ui
                .add_enabled(!self.busy && self.wired(), Button::new(t("▶ Live rainbow demo (10 s)", "▶ Demo arcobaleno live (10 s)")))
                .on_hover_text(t("Real-time animation, not saved", "Animazione in tempo reale, non salvata"))
                .clicked()
            {
                self.run(t("Live demo", "Demo live"), Op::Demo(10), None);
            }
        });
    }

    fn tab_keys(&mut self, ui: &mut Ui) {
        title(
            ui,
            t("Key remapping", "Rimappatura tasti"),
            t(
                "Changes are saved in the keyboard and work on every computer.",
                "Le modifiche sono salvate nella tastiera e valgono su ogni computer.",
            ),
        );
        ui.horizontal(|ui| {
            ui.label("Layer:");
            if ui.selectable_label(!self.fn_layer, t("Normal", "Normale")).clicked() {
                self.fn_layer = false;
            }
            if ui.selectable_label(self.fn_layer, "Fn +").clicked() {
                self.fn_layer = true;
            }
        });
        ui.add_space(6.0);
        let fn_layer = self.fn_layer;
        let layer = if fn_layer { &self.keymap.fn_layer } else { &self.keymap.normal };
        let clicked = layout::keyboard(ui, |name| {
            let target = layer.get(name);
            KeyLook {
                fill: if target.is_some() { ACCENT_SOFT } else { KEYCAP },
                sub: target.cloned(),
                dim: keys::check_remappable(name, fn_layer).is_err(),
                selected: self.selected == Some(name),
            }
        });
        if let Some(c) = clicked {
            if keys::check_remappable(c.key, fn_layer).is_ok() {
                self.selected = Some(c.key);
                self.key_error = None;
                let layer = if fn_layer { &self.keymap.fn_layer } else { &self.keymap.normal };
                self.editor = layer.get(c.key).map(|t| TargetEditor::from_target(t)).unwrap_or_default();
            }
        }
        ui.add_space(10.0);
        ui.columns(2, |cols| {
            self.key_editor(&mut cols[0]);
            self.remap_list(&mut cols[1]);
        });
        ui.add_space(10.0);
        self.send_keymap_bar(ui, true);
    }

    fn key_editor(&mut self, ui: &mut Ui) {
        let Some(key) = self.selected else {
            card(ui, t("Key", "Tasto"), |ui| {
                ui.label(t("Click a key on the keyboard above.", "Clicca un tasto sulla tastiera qui sopra."));
            });
            return;
        };
        let label = LAYOUT.iter().find(|k| k.0 == key).map_or(key, |k| k.1);
        let heading = format!("{}{}", if self.fn_layer { "Fn + " } else { "" }, if label.is_empty() { t("Space", "Spazio") } else { label });
        card(ui, &heading, |ui| {
            ComboBox::from_id_salt("kind")
                .selected_text(KINDS.iter().find(|k| k.0 == self.editor.kind).map_or("", |k| t(k.1, k.2)))
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (k, en, it) in KINDS {
                        ui.selectable_value(&mut self.editor.kind, *k, t(en, it));
                    }
                });
            ui.add_space(6.0);
            let e = &mut self.editor;
            match e.kind {
                Kind::Default => {
                    ui.label(t("The key goes back to its factory function.", "Il tasto torna alla sua funzione di fabbrica."));
                }
                Kind::Key => {
                    ui.horizontal(|ui| {
                        for (i, m) in ["Ctrl", "Shift", "Alt", "Win"].iter().enumerate() {
                            ui.toggle_value(&mut e.mods[i], *m);
                        }
                        ui.label("+");
                        name_combo(ui, "key", &mut e.key, KEY_NAMES, true);
                    });
                }
                Kind::Media => {
                    let names: Vec<&str> = keys::CONSUMER.iter().map(|(n, _)| *n).collect();
                    name_combo(ui, "media", &mut e.media, &names, false);
                }
                Kind::Mouse => {
                    ui.horizontal_wrapped(|ui| {
                        for (v, en, it) in MOUSE {
                            ui.selectable_value(&mut e.mouse, v.to_string(), t(en, it));
                        }
                    });
                }
                Kind::Multi => {
                    ui.horizontal(|ui| {
                        for i in 0..3 {
                            name_combo(ui, &format!("multi{i}"), &mut e.multi[i], KEY_NAMES, true);
                        }
                    });
                }
                Kind::Text => {
                    ui.add(
                        egui::TextEdit::multiline(&mut e.text)
                            .hint_text(t("Text to type (US layout)", "Testo da scrivere (layout USA)"))
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                }
                Kind::Macro => {
                    if self.macros.is_empty() {
                        ui.label(t("No macros: create one in the Macros tab.", "Nessuna macro: creane una nella scheda Macro."));
                    }
                    let names: Vec<&str> = self.macros.keys().map(String::as_str).collect();
                    name_combo(ui, "macro", &mut e.macro_name, &names, false);
                }
            }
            ui.add_space(6.0);
            let target = self.editor.target();
            ui.label(RichText::new(format!("→ {target}")).monospace().weak());
            if let Some(err) = &self.key_error {
                ui.colored_label(ERR, err);
            }
            ui.horizontal(|ui| {
                if ui.button(t("Assign", "Assegna")).clicked() {
                    self.assign(key, target);
                }
                if ui.button(t("Reset key", "Ripristina tasto")).clicked() {
                    self.assign(key, "default".into());
                }
            });
        });
    }

    fn assign(&mut self, key: &'static str, target: String) {
        let check = keys::encode_target(&target, &self.macros, &mut Vec::new());
        match check {
            Err(e) => self.key_error = Some(e.to_string()),
            Ok(_) => {
                self.key_error = None;
                let layer = if self.fn_layer { &mut self.keymap.fn_layer } else { &mut self.keymap.normal };
                if target == "default" {
                    layer.remove(key);
                } else {
                    layer.insert(key.to_string(), target);
                }
            }
        }
    }

    fn remap_list(&mut self, ui: &mut Ui) {
        let fn_layer = self.fn_layer;
        let heading = if fn_layer {
            t("Fn layer remaps", "Rimappature layer Fn")
        } else {
            t("Normal layer remaps", "Rimappature layer normale")
        };
        card(ui, heading, |ui| {
            let layer = if fn_layer { &mut self.keymap.fn_layer } else { &mut self.keymap.normal };
            if layer.is_empty() {
                ui.label(RichText::new(t("None: factory layout.", "Nessuna: layout di fabbrica.")).weak());
            }
            let mut remove = None;
            egui::ScrollArea::vertical().max_height(170.0).show(ui, |ui| {
                for (k, target) in layer.iter() {
                    ui.horizontal(|ui| {
                        if ui.small_button("✖").on_hover_text(t("Remove", "Rimuovi")).clicked() {
                            remove = Some(k.clone());
                        }
                        ui.label(RichText::new(k).strong());
                        ui.label("→");
                        ui.label(RichText::new(target).monospace());
                    });
                }
            });
            if let Some(k) = remove {
                layer.remove(&k);
            }
        });
    }

    fn keymap_draft_changed(&self) -> bool {
        Some(&self.keymap) != self.state.keymap.as_ref().or(Some(&Keymap::default()))
            || self.macros != self.state.macros
    }

    fn send_keymap_bar(&mut self, ui: &mut Ui, show_clear: bool) {
        let compiled = self.keymap.compile(&self.macros);
        ui.horizontal(|ui| {
            let changed = self.keymap_draft_changed();
            if let Err(e) = &compiled {
                ui.colored_label(ERR, e.to_string());
            }
            ui.add_enabled_ui(compiled.is_ok(), |ui| {
                if self.action(ui, t("⬆ Send keys and macros", "⬆ Invia tasti e macro"), false) {
                    let mut s = self.state.clone();
                    s.keymap = Some(self.keymap.clone());
                    s.macros = self.macros.clone();
                    self.run(t("Remapping and macros", "Rimappatura e macro"), Op::Keymap(s.clone()), Some(s));
                }
            });
            if ui.add_enabled(changed, Button::new(t("Discard changes", "Annulla modifiche"))).clicked() {
                self.keymap = self.state.keymap.clone().unwrap_or_default();
                self.macros = self.state.macros.clone();
            }
            if show_clear && ui.button(t("Clear this layer", "Svuota questo layer")).clicked() {
                if self.fn_layer {
                    self.keymap.fn_layer.clear();
                } else {
                    self.keymap.normal.clear();
                }
            }
            if changed {
                ui.colored_label(WARN, t("● changes not sent", "● modifiche non inviate"));
            }
        });
    }

    fn tab_macros(&mut self, ui: &mut Ui) {
        title(
            ui,
            t("Macros", "Macro"),
            t(
                "Sequences saved in the keyboard, to assign to a key in the Keys tab.",
                "Sequenze salvate nella tastiera, da assegnare a un tasto nella scheda Tasti.",
            ),
        );
        ui.columns(2, |cols| {
            card(&mut cols[0], t("Saved macros", "Macro salvate"), |ui| {
                if self.macros.is_empty() {
                    ui.label(RichText::new(t("No macros.", "Nessuna macro.")).weak());
                }
                let mut pick = None;
                for (name, m) in &self.macros {
                    let sel = self.macro_sel.as_deref() == Some(name.as_str());
                    let used = self
                        .keymap
                        .normal
                        .values()
                        .chain(self.keymap.fn_layer.values())
                        .any(|t| *t == format!("macro:{name}"));
                    let text = format!(
                        "{name}  ·  {} {}{}",
                        m.steps.len(),
                        t("steps", "passi"),
                        if used { t("  · in use", "  · in uso") } else { "" }
                    );
                    if ui.selectable_label(sel, text).clicked() {
                        pick = Some(name.clone());
                    }
                }
                if let Some(name) = pick {
                    let m = &self.macros[&name];
                    self.macro_name = name.clone();
                    self.macro_steps = m.steps.join("\n");
                    self.macro_mode = m.mode;
                    self.macro_count = m.count;
                    self.macro_sel = Some(name);
                    self.macro_error = None;
                }
                ui.add_space(8.0);
                if ui.button(t("➕ New macro", "➕ Nuova macro")).clicked() {
                    self.macro_sel = None;
                    self.macro_name = format!("macro{}", self.macros.len() + 1);
                    self.macro_steps = t("text:Hello!", "text:Ciao!").into();
                    self.macro_mode = MacroMode::Once;
                    self.macro_count = 1;
                }
            });
            let ui = &mut cols[1];
            card(ui, t("Editor", "Editor"), |ui| {
                egui::Grid::new("macro-edit").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label(t("Name", "Nome"));
                    ui.text_edit_singleline(&mut self.macro_name);
                    ui.end_row();
                    ui.label(t("Mode", "Modalità"));
                    ComboBox::from_id_salt("macro-mode")
                        .selected_text(macro_mode_label(self.macro_mode))
                        .show_ui(ui, |ui| {
                            for m in [MacroMode::Once, MacroMode::Repeat, MacroMode::Toggle, MacroMode::Hold, MacroMode::Step] {
                                ui.selectable_value(&mut self.macro_mode, m, macro_mode_label(m));
                            }
                        });
                    ui.end_row();
                    if self.macro_mode == MacroMode::Repeat {
                        ui.label(t("Repetitions", "Ripetizioni"));
                        ui.add(Slider::new(&mut self.macro_count, 1..=255));
                        ui.end_row();
                    }
                });
                ui.label(t("Steps (one per line):", "Passi (uno per riga):"));
                ui.add(
                    egui::TextEdit::multiline(&mut self.macro_steps)
                        .code_editor()
                        .desired_rows(7)
                        .desired_width(f32::INFINITY),
                );
                ui.label(
                    RichText::new(t(
                        "text:Hello · tap:ctrl+c · down:shift · up:shift · delay:100 · click:left",
                        "text:Ciao · tap:ctrl+c · down:shift · up:shift · delay:100 · click:left",
                    ))
                        .small()
                        .weak(),
                );
                let steps: Vec<String> =
                    self.macro_steps.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
                let valid = ak820::macros::events(&steps);
                match &valid {
                    Ok(ev) => {
                        ui.label(RichText::new(format!("✔ {} {}", ev.len(), t("events", "eventi"))).color(OK));
                    }
                    Err(e) if !steps.is_empty() => {
                        ui.colored_label(ERR, e.to_string());
                    }
                    Err(_) => {}
                }
                if let Some(e) = &self.macro_error {
                    ui.colored_label(ERR, e);
                }
                ui.horizontal(|ui| {
                    let name = self.macro_name.trim().to_string();
                    if ui.add_enabled(valid.is_ok() && !name.is_empty(), Button::new(t("💾 Save macro", "💾 Salva macro"))).clicked() {
                        if let Some(old) = self.macro_sel.take().filter(|o| *o != name) {
                            self.macros.remove(&old);
                            rename_macro_refs(&mut self.keymap, &old, &name);
                        }
                        self.macros.insert(name.clone(), MacroDef { steps, mode: self.macro_mode, count: self.macro_count });
                        self.macro_sel = Some(name);
                        self.macro_error = None;
                    }
                    if let Some(sel) = self.macro_sel.clone() {
                        if ui.button(t("🗑 Delete", "🗑 Elimina")).clicked() {
                            let target = format!("macro:{sel}");
                            let used = self.keymap.normal.values().chain(self.keymap.fn_layer.values()).any(|t| *t == target);
                            if used {
                                self.macro_error = Some(
                                    t(
                                        "The macro is assigned to a key: remove the assignment first.",
                                        "La macro è assegnata a un tasto: rimuovi prima l'assegnazione.",
                                    )
                                    .into(),
                                );
                            } else {
                                self.macros.remove(&sel);
                                self.macro_sel = None;
                            }
                        }
                    }
                });
            });
        });
        ui.add_space(10.0);
        self.send_keymap_bar(ui, false);
    }

    fn load_screen(&mut self, path: PathBuf, crop: Crop) {
        self.screen_error = None;
        self.textures = None;
        match screen::load_source(&path) {
            Ok(frames) => {
                self.source_size = frames.first().map_or((1, 1), |f| f.image.dimensions());
                // Keep the preview light: at most 512 px on the long side.
                self.source = frames
                    .into_iter()
                    .map(|f| {
                        let (w, h) = f.image.dimensions();
                        let img = if w.max(h) > 512 {
                            let k = 512.0 / w.max(h) as f32;
                            image::imageops::thumbnail(&f.image, (w as f32 * k) as u32, (h as f32 * k) as u32)
                        } else {
                            f.image
                        };
                        (img, f.duration_ms.max(30))
                    })
                    .collect();
                let (w, h) = self.source_size;
                self.crop = crop.clamped(w, h);
                self.preview_start = Instant::now();
            }
            Err(e) => {
                self.source.clear();
                self.screen_error = Some(e.to_string());
            }
        }
        self.screen_path = Some(path);
    }

    /// Draw the 128×128 screen preview; in crop mode it can be dragged and zoomed.
    fn screen_preview(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let size = Vec2::splat(288.0);
        let sense = if self.fit == Fit::Crop { egui::Sense::drag() } else { egui::Sense::hover() };
        let (rect, resp) = ui.allocate_exact_size(size, sense);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(8), Color32::BLACK);
        if self.source.is_empty() {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, t("no image", "nessuna immagine"), FontId::proportional(14.0), Color32::GRAY);
            return;
        }
        let textures = self.textures.get_or_insert_with(|| {
            self.source
                .iter()
                .enumerate()
                .map(|(i, (img, _))| {
                    let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                    ctx.load_texture(format!("screen{i}"), ci, TextureOptions::LINEAR)
                })
                .collect()
        });
        // Current animation frame.
        let total: u32 = self.source.iter().map(|f| f.1).sum();
        let mut ms = (self.preview_start.elapsed().as_millis() as u32) % total.max(1);
        let mut idx = 0;
        for (i, (_, d)) in self.source.iter().enumerate() {
            if ms < *d {
                idx = i;
                break;
            }
            ms -= d;
        }
        let tex = textures[idx].id();
        let (w, h) = (self.source_size.0 as f32, self.source_size.1 as f32);
        let full = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        match self.fit {
            Fit::Crop => {
                let (x0, y0, side) = self.crop.rect(self.source_size.0, self.source_size.1);
                let uv = Rect::from_min_max(Pos2::new(x0 / w, y0 / h), Pos2::new((x0 + side) / w, (y0 + side) / h));
                painter.image(tex, rect, uv, Color32::WHITE);
                if resp.dragged() {
                    let d = resp.drag_delta();
                    self.crop.x -= d.x / rect.width() * side / w;
                    self.crop.y -= d.y / rect.height() * side / h;
                    self.crop = self.crop.clamped(self.source_size.0, self.source_size.1);
                }
                if resp.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.0 {
                        self.crop.zoom *= (scroll * 0.003).exp();
                        self.crop = self.crop.clamped(self.source_size.0, self.source_size.1);
                        // Don't scroll the page while zooming.
                        ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
                    }
                    ctx.set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
                }
            }
            Fit::Pad => {
                let k = size.x / w.max(h);
                let inner = Rect::from_center_size(rect.center(), Vec2::new(w * k, h * k));
                painter.image(tex, inner, full, Color32::WHITE);
            }
            Fit::Stretch => {
                painter.image(tex, rect, full, Color32::WHITE);
            }
        }
        painter.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0_f32, Color32::from_gray(90)), egui::StrokeKind::Inside);
        if self.source.len() > 1 {
            ctx.request_repaint_after(Duration::from_millis(15));
        }
    }

    fn tab_screen(&mut self, ui: &mut Ui) {
        title(
            ui,
            t("Screen", "Display"),
            t(
                "Image or animated GIF on the 128×128 screen. Drop a file here or pick one.",
                "Immagine o GIF animata sul display 128×128. Trascina un file qui o sceglilo.",
            ),
        );
        let dropped = ui.ctx().input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()));
        if let Some(p) = dropped {
            self.load_screen(p, Crop::default());
        }
        ui.columns(2, |cols| {
            card(&mut cols[0], t("Preview", "Anteprima"), |ui| {
                self.screen_preview(ui);
                if self.fit == Fit::Crop && !self.source.is_empty() {
                    ui.label(
                        RichText::new(t(
                            "Drag to move the image · scroll to zoom",
                            "Trascina per spostare l'immagine · rotella per lo zoom",
                        ))
                        .small()
                        .weak(),
                    );
                }
            });
            let ui = &mut cols[1];
            card(ui, t("Image", "Immagine"), |ui| {
                if ui.button(t("📂 Choose image…", "📂 Scegli immagine…")).clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter(t("Images", "Immagini"), &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
                        .pick_file()
                    {
                        self.load_screen(p, Crop::default());
                    }
                }
                if let Some(p) = &self.screen_path {
                    ui.label(RichText::new(p.file_name().unwrap_or_default().to_string_lossy()).monospace());
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(t("Fit:", "Adattamento:"));
                    ui.selectable_value(&mut self.fit, Fit::Crop, t("Crop", "Ritaglia"));
                    ui.selectable_value(&mut self.fit, Fit::Pad, t("Black bars", "Bande nere"));
                    ui.selectable_value(&mut self.fit, Fit::Stretch, t("Stretch", "Allunga"));
                });
                if self.fit == Fit::Crop && !self.source.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label("Zoom");
                        let r = ui.add(Slider::new(&mut self.crop.zoom, 1.0..=screen::MAX_ZOOM).logarithmic(true).fixed_decimals(1).suffix("×"));
                        if r.changed() {
                            self.crop = self.crop.clamped(self.source_size.0, self.source_size.1);
                        }
                        if ui.button(t("Reset", "Centra")).clicked() {
                            self.crop = Crop::default();
                        }
                    });
                }
                if !self.source.is_empty() {
                    let n = self.source.len();
                    let blocks = screen::block_count(n);
                    let secs = blocks * 50 / 1000 + 1;
                    let (w, h) = self.source_size;
                    ui.label(match (i18n::current(), n) {
                        (Lang::En, 1) => format!("{w}×{h} · 1 frame · {blocks} blocks · ~{secs} s to send"),
                        (Lang::En, n) => format!("{w}×{h} · {n} frames · {blocks} blocks · ~{secs} s to send"),
                        (Lang::It, 1) => format!("{w}×{h} · 1 fotogramma · {blocks} blocchi · ~{secs} s di invio"),
                        (Lang::It, n) => format!("{w}×{h} · {n} fotogrammi · {blocks} blocchi · ~{secs} s di invio"),
                    });
                    if n >= screen::MAX_FRAMES {
                        let n = screen::MAX_FRAMES;
                        let text = match i18n::current() {
                            Lang::En => format!("Limited to the first {n} frames"),
                            Lang::It => format!("Limitata ai primi {n} fotogrammi"),
                        };
                        ui.colored_label(WARN, text);
                    }
                }
                if let Some(e) = &self.screen_error {
                    ui.colored_label(ERR, e);
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ready = !self.source.is_empty();
                ui.add_enabled_ui(ready, |ui| {
                    if self.action(ui, t("⬆ Send to screen", "⬆ Invia al display"), true) {
                        if let Some(p) = &self.screen_path {
                            let image = std::fs::canonicalize(p).unwrap_or(p.clone());
                            let sc = Screen { image, fit: self.fit, crop: self.crop };
                            let mut s = self.state.clone();
                            s.screen = Some(sc.clone());
                            self.run(t("Sending to screen", "Invio al display"), Op::Screen(sc), Some(s));
                        }
                    }
                });
                if ui.add_enabled(!self.busy && self.connected(), Button::new(t("🕒 Show clock", "🕒 Mostra orologio"))).clicked() {
                    self.run(t("Clock", "Orologio"), Op::Clock, None);
                }
            });
        });
    }

    fn app_settings_card(&mut self, ui: &mut Ui) {
        card(ui, t("Application", "Applicazione"), |ui| {
            egui::Grid::new("app-settings").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                ui.label(t("Language", "Lingua"));
                let mut lang = i18n::current();
                ComboBox::from_id_salt("lang")
                    .selected_text(format!("🌐  {}", lang.name()))
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for l in Lang::ALL {
                            ui.selectable_value(&mut lang, l, l.name());
                        }
                    });
                if lang != i18n::current() {
                    i18n::set(lang);
                    self.settings.lang = Some(lang);
                    self.save_settings();
                    self.refresh_profiles(); // tray menu labels
                }
                ui.end_row();

                ui.label(t("Startup", "Avvio"));
                let mut autostart = self.autostart;
                ui.checkbox(&mut autostart, t("Start with the computer (in the tray)", "Avvia all'accensione del PC (nella tray)"));
                if autostart != self.autostart {
                    match autostart::set(autostart) {
                        Ok(()) => self.autostart = autostart,
                        Err(e) => {
                            let msg = t("Could not change the startup setting", "Impossibile cambiare l'avvio automatico");
                            self.log.push((format!("{msg}: {e}"), true));
                        }
                    }
                }
                ui.end_row();

                ui.label(t("Closing the window", "Chiusura finestra"));
                let mut to_tray = self.settings.close_to_tray;
                ui.checkbox(&mut to_tray, t("Keep running in the tray", "Riduci nella tray"))
                    .on_hover_text(t(
                        "The icon in the system tray reopens the window and switches profile. On GNOME it needs the AppIndicator extension.",
                        "L'icona nella tray riapre la finestra e cambia profilo. Su GNOME serve l'estensione AppIndicator.",
                    ));
                if to_tray != self.settings.close_to_tray {
                    self.settings.close_to_tray = to_tray;
                    self.save_settings();
                    self.set_tray(to_tray);
                }
                ui.end_row();

                ui.label(t("Clock", "Orologio"));
                let mut sync = self.settings.sync_clock_on_start;
                ui.checkbox(&mut sync, t("Sync the screen clock when the app starts", "Sincronizza l'orologio del display all'avvio"));
                if sync != self.settings.sync_clock_on_start {
                    self.settings.sync_clock_on_start = sync;
                    self.save_settings();
                }
                ui.end_row();
            });
        });
    }

    fn tab_system(&mut self, ui: &mut Ui) {
        title(
            ui,
            t("System", "Sistema"),
            t("Clock, power saving and key behavior.", "Orologio, risparmio energetico e comportamento dei tasti."),
        );
        self.app_settings_card(ui);
        ui.add_space(8.0);
        card(ui, t("Screen clock", "Orologio del display"), |ui| {
            ui.horizontal(|ui| {
                let format = t("%Y-%m-%d  %H:%M:%S", "%d/%m/%Y  %H:%M:%S");
                ui.label(RichText::new(chrono::Local::now().format(format).to_string()).monospace().size(18.0));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
                if ui.add_enabled(!self.busy && self.connected(), Button::new(t("🕒 Sync with the PC", "🕒 Sincronizza con il PC"))).clicked() {
                    self.run("Orologio", Op::Clock, None);
                }
            });
        });
        ui.add_space(8.0);
        card(ui, t("Settings", "Impostazioni"), |ui| {
            egui::Grid::new("system").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                ui.label(t("Sleep (on battery)", "Sospensione (a batteria)"));
                ui.horizontal(|ui| {
                    for (v, l) in [(SleepTime::Never, t("Never", "Mai")), (SleepTime::Min1, "1 min"), (SleepTime::Min5, "5 min"), (SleepTime::Min30, "30 min")] {
                        ui.selectable_value(&mut self.system.sleep, v, l);
                    }
                });
                ui.end_row();
                ui.label(t("Fn key", "Tasto Fn"));
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.system.fn_mode, FnMode::Hold, t("Hold", "Tenuto premuto"));
                    ui.selectable_value(&mut self.system.fn_mode, FnMode::Toggle, t("Toggle", "Attiva/disattiva"));
                });
                ui.end_row();
                ui.label(t("Debounce", "Antirimbalzo"));
                ui.add(Slider::new(&mut self.system.response, 1..=5).text(t("1 fast · 5 slow", "1 veloce · 5 lento")));
                ui.end_row();
                ui.label(t("Block", "Blocca"));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.system.block_win, "Win");
                    ui.checkbox(&mut self.system.block_alt_f4, "Alt+F4");
                    ui.checkbox(&mut self.system.block_alt_tab, "Alt+Tab");
                });
                ui.end_row();
            });
            ui.add_space(8.0);
            if self.action(ui, t("✔ Apply settings", "✔ Applica impostazioni"), false) {
                if let Ok(settings) = self.system.settings() {
                    let mut s = self.state.clone();
                    s.system = Some(self.system);
                    self.run(t("System settings", "Impostazioni di sistema"), Op::System(settings), Some(s));
                }
            }
        });
        ui.add_space(8.0);
        card(ui, t("Battery", "Batteria"), |ui| {
            ui.horizontal(|ui| {
                let enabled = !self.busy && self.status == Status::Receiver;
                if ui
                    .add_enabled(enabled, Button::new(t("🔋 Read battery", "🔋 Leggi batteria")))
                    .on_disabled_hover_text(t("Available only with the 2.4 GHz receiver", "Disponibile solo con il ricevitore 2.4 GHz"))
                    .clicked()
                {
                    self.run(t("Battery", "Batteria"), Op::Battery, None);
                }
                match self.battery {
                    Some(Some(v)) => {
                        ui.add(egui::ProgressBar::new(v as f32 / 100.0).text(format!("{v}%")).desired_width(200.0));
                    }
                    Some(None) => {
                        ui.label(t("Level not available (keyboard asleep?)", "Livello non disponibile (tastiera in standby?)"));
                    }
                    None => {}
                }
            });
        });
    }

    fn saved_profiles_card(&mut self, ui: &mut Ui) {
        enum Action {
            Apply(String),
            Overwrite(String),
            StartRename(String),
            Rename(String, String),
            CancelRename,
            AskDelete(String),
            Delete(String),
            CancelDelete,
        }
        let mut action = None;
        card(ui, t("Saved profiles", "Profili salvati"), |ui| {
            ui.label(
                RichText::new(t(
                    "A profile holds everything: lighting, per-key colors, remaps, macros, screen and settings. Apply switches the keyboard to it in one click (also from the tray menu).",
                    "Un profilo contiene tutto: luci, colori per tasto, rimappature, macro, display e impostazioni. Applica porta la tastiera a quel profilo con un clic (anche dal menu della tray).",
                ))
                .weak(),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_profile)
                        .hint_text(t("New profile name", "Nome del nuovo profilo"))
                        .desired_width(220.0),
                );
                let name = self.new_profile.trim().to_string();
                let exists = self.profiles.iter().any(|(n, _)| *n == name);
                let label = if exists { t("💾 Overwrite", "💾 Sovrascrivi") } else { t("💾 Save current configuration", "💾 Salva la configurazione attuale") };
                if ui
                    .add_enabled(!name.is_empty(), Button::new(label))
                    .on_hover_text(t(
                        "Saves what was last sent to the keyboard (unsent edits are not included)",
                        "Salva quello che è stato inviato alla tastiera (le modifiche non inviate sono escluse)",
                    ))
                    .clicked()
                {
                    action = Some(Action::Overwrite(name));
                }
            });
            ui.add_space(6.0);
            if self.profiles.is_empty() {
                ui.label(RichText::new(t("No saved profiles yet.", "Ancora nessun profilo salvato.")).weak());
            }
            let can_apply = !self.busy && self.connected();
            let confirm_delete = self.confirm_delete.clone();
            let active_profile = self.settings.active_profile.clone();
            let renaming = &mut self.renaming;
            for (name, p) in &self.profiles {
                let active = active_profile.as_deref() == Some(name.as_str());
                egui::Frame::new()
                    .fill(if active { ACCENT_SOFT.gamma_multiply(0.5) } else { ui.visuals().extreme_bg_color })
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            if let Some((old, buf)) = renaming.as_mut().filter(|(o, _)| o == name) {
                                ui.add(egui::TextEdit::singleline(buf).desired_width(200.0));
                                if ui.button("✔").clicked() {
                                    action = Some(Action::Rename(old.clone(), buf.trim().to_string()));
                                }
                                if ui.button("✖").clicked() {
                                    action = Some(Action::CancelRename);
                                }
                            } else {
                                ui.vertical(|ui| {
                                    let mut title = RichText::new(name).strong().size(16.0);
                                    if active {
                                        title = title.color(Color32::WHITE);
                                    }
                                    ui.horizontal(|ui| {
                                        ui.label(title);
                                        if active {
                                            ui.label(RichText::new(t("● active", "● attivo")).color(OK).small());
                                        }
                                    });
                                    ui.label(RichText::new(profile_summary(p)).weak().small());
                                });
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if confirm_delete.as_deref() == Some(name.as_str()) {
                                    if ui.button(t("Cancel", "Annulla")).clicked() {
                                        action = Some(Action::CancelDelete);
                                    }
                                    if ui.add(Button::new(RichText::new(t("Delete", "Elimina")).color(Color32::WHITE)).fill(ERR)).clicked() {
                                        action = Some(Action::Delete(name.clone()));
                                    }
                                    return;
                                }
                                if ui.small_button("🗑").on_hover_text(t("Delete", "Elimina")).clicked() {
                                    action = Some(Action::AskDelete(name.clone()));
                                }
                                if ui.small_button("✏").on_hover_text(t("Rename", "Rinomina")).clicked() {
                                    action = Some(Action::StartRename(name.clone()));
                                }
                                if ui
                                    .small_button("⟳")
                                    .on_hover_text(t("Replace with the current configuration", "Sostituisci con la configurazione attuale"))
                                    .clicked()
                                {
                                    action = Some(Action::Overwrite(name.clone()));
                                }
                                let b = Button::new(RichText::new(t("▶ Apply", "▶ Applica")).strong().color(Color32::WHITE))
                                    .fill(ACCENT)
                                    .min_size(Vec2::new(100.0, 30.0));
                                if ui.add_enabled(can_apply, b).clicked() {
                                    action = Some(Action::Apply(name.clone()));
                                }
                            });
                        });
                    });
                ui.add_space(4.0);
            }
        });
        let result = match action {
            None => return,
            Some(Action::Apply(n)) => {
                self.apply_named(&n);
                Ok(())
            }
            Some(Action::Overwrite(n)) => profile::save_named(&n, &self.state).map(|_| {
                self.new_profile.clear();
                self.settings.active_profile = Some(n);
                self.save_settings();
            }),
            Some(Action::StartRename(n)) => {
                self.renaming = Some((n.clone(), n));
                Ok(())
            }
            Some(Action::Rename(old, new)) => {
                self.renaming = None;
                if old == new {
                    Ok(())
                } else {
                    profile::rename_named(&old, &new).map(|_| {
                        if self.settings.active_profile.as_deref() == Some(old.as_str()) {
                            self.settings.active_profile = Some(new);
                            self.save_settings();
                        }
                    })
                }
            }
            Some(Action::CancelRename) => {
                self.renaming = None;
                Ok(())
            }
            Some(Action::AskDelete(n)) => {
                self.confirm_delete = Some(n);
                Ok(())
            }
            Some(Action::Delete(n)) => {
                self.confirm_delete = None;
                profile::delete_named(&n).map(|_| {
                    if self.settings.active_profile.as_deref() == Some(n.as_str()) {
                        self.settings.active_profile = None;
                        self.save_settings();
                    }
                })
            }
            Some(Action::CancelDelete) => {
                self.confirm_delete = None;
                Ok(())
            }
        };
        if let Err(e) = result {
            self.profile_error = Some(e.to_string());
        }
        self.refresh_profiles();
    }

    fn tab_profiles(&mut self, ui: &mut Ui) {
        title(
            ui,
            t("Profiles", "Profili"),
            t(
                "Save, load and resend the whole configuration (TOML files, compatible with the CLI).",
                "Salva, carica e reinvia l'intera configurazione (file TOML, compatibili con la CLI).",
            ),
        );
        self.saved_profiles_card(ui);
        ui.add_space(8.0);
        card(ui, t("Files", "File"), |ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.busy && self.connected(), Button::new(t("📂 Import and apply…", "📂 Importa e applica…"))).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter(t("Profile", "Profilo"), &["toml"]).pick_file() {
                        match Profile::load(&p).and_then(|prof| prof.validate().map(|_| prof)) {
                            Ok(prof) => {
                                let mut merged = self.state.clone();
                                merged.merge(&prof);
                                self.profile_error = None;
                                self.resync_after_job = true;
                                self.run(t("Apply profile", "Applica profilo"), Op::Apply(prof), Some(merged));
                            }
                            Err(e) => self.profile_error = Some(e.to_string()),
                        }
                    }
                }
                if ui.button(t("💾 Export…", "💾 Esporta…")).clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter(t("Profile", "Profilo"), &["toml"])
                        .set_file_name(t("ak820-profile.toml", "ak820-profilo.toml"))
                        .save_file()
                    {
                        match self.state.save(&p) {
                            Ok(()) => {
                                let msg = t("Profile exported to", "Profilo esportato in");
                                self.log.push((format!("{msg} {}", p.display()), false));
                            }
                            Err(e) => self.profile_error = Some(e.to_string()),
                        }
                    }
                }
                if ui
                    .add_enabled(!self.busy && self.connected(), Button::new(t("🔁 Resend everything", "🔁 Reinvia tutto")))
                    .on_hover_text(t(
                        "Send the last saved configuration to the keyboard again",
                        "Rimanda alla tastiera l'ultima configurazione salvata",
                    ))
                    .clicked()
                {
                    let mut p = self.state.clone();
                    p.sync_clock = Some(true);
                    if p.keymap.is_none() && !p.macros.is_empty() {
                        p.keymap = Some(Keymap::default());
                    }
                    self.run(t("Resending configuration", "Reinvio configurazione"), Op::Apply(p), None);
                }
            });
            if let Some(e) = &self.profile_error {
                ui.colored_label(ERR, e);
            }
        });
        ui.add_space(8.0);
        card(ui, t("Factory reset", "Ripristino di fabbrica"), |ui| {
            ui.label(t(
                "Removes remaps and macros, restores the rainbow wave and the default settings.",
                "Rimuove rimappature e macro, rimette l'onda arcobaleno e le impostazioni di default.",
            ));
            ui.horizontal(|ui| {
                if !self.confirm_reset {
                    if ui.add_enabled(!self.busy && self.connected(), Button::new(t("Reset…", "Ripristina…"))).clicked() {
                        self.confirm_reset = true;
                    }
                } else {
                    ui.colored_label(WARN, t("Are you sure?", "Sicuro?"));
                    if ui.add(Button::new(RichText::new(t("Yes, reset", "Sì, ripristina")).color(Color32::WHITE)).fill(ERR)).clicked() {
                        self.confirm_reset = false;
                        let p = Profile {
                            sync_clock: Some(true),
                            lighting: Some(Lighting::factory()),
                            system: Some(System::default()),
                            keymap: Some(Keymap::default()),
                            ..Profile::default()
                        };
                        let _ = self.state.save(&profile::state_path().with_extension("before-reset.toml"));
                        self.resync_after_job = true;
                        self.run(t("Factory reset", "Ripristino di fabbrica"), Op::Apply(p.clone()), Some(p));
                    }
                    if ui.button(t("Cancel", "Annulla")).clicked() {
                        self.confirm_reset = false;
                    }
                }
            });
        });
        ui.add_space(8.0);
        card(ui, t("Saved configuration", "Configurazione salvata"), |ui| {
            ui.label(RichText::new(profile::state_path().display().to_string()).monospace().weak());
            let text = toml::to_string_pretty(&self.state).unwrap_or_default();
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(if text.is_empty() { t("(empty)", "(vuota)").into() } else { text }).monospace()));
            });
        });
    }

    // ---- chrome ------------------------------------------------------------------------

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").exact_height(38.0).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                let (color, text) = match &self.status {
                    Status::Searching => (Color32::GRAY, t("Looking for the keyboard…", "Ricerca tastiera…").to_string()),
                    Status::Wired(pid) => {
                        (OK, format!("{} (PID {pid:04x})", t("Connected via USB cable", "Collegata via cavo USB")))
                    }
                    Status::Receiver => (OK, t("Connected via 2.4 GHz receiver", "Collegata via ricevitore 2.4 GHz").into()),
                    Status::Bluetooth => (
                        WARN,
                        t("On Bluetooth: use the cable or the receiver", "In Bluetooth: usa il cavo o il ricevitore").into(),
                    ),
                    Status::NotFound => (ERR, t("Keyboard not found", "Tastiera non trovata").into()),
                    Status::Permission => (
                        WARN,
                        t("Missing permissions to access the keyboard", "Permessi mancanti per accedere alla tastiera").into(),
                    ),
                    Status::Error(e) => (ERR, format!("{}: {e}", t("Error", "Errore"))),
                };
                ui.label(RichText::new("●").color(color));
                ui.label(text);
                #[cfg(target_os = "linux")]
                if self.status == Status::Permission
                    && ui
                        .add_enabled(!self.busy, Button::new(t("🔑 Grant access", "🔑 Concedi accesso")))
                        .on_hover_text(t(
                            "Install the udev rule (asks for the administrator password)",
                            "Installa la regola udev (richiede la password di amministratore)",
                        ))
                        .clicked()
                {
                    self.run(t("Installing udev rule", "Installazione regola udev"), Op::InstallUdev, None);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.selectable_label(self.show_log, t("📜 Log", "📜 Registro")).clicked() {
                        self.show_log = !self.show_log;
                    }
                    if self.busy {
                        ui.add(egui::ProgressBar::new(self.progress).desired_width(160.0).animate(self.progress == 0.0));
                        ui.spinner();
                    }
                    if let Some((msg, err)) = self.log.last() {
                        let t = RichText::new(msg);
                        ui.label(if *err { t.color(ERR) } else { t.weak() });
                    }
                });
            });
        });
    }

    fn log_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_log;
        egui::Window::new(t("Log", "Registro"))
            .open(&mut open)
            .default_size([520.0, 260.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for (m, err) in &self.log {
                        let t = RichText::new(m).monospace();
                        ui.label(if *err { t.color(ERR) } else { t });
                    }
                });
            });
        self.show_log = open;
    }

    fn nav(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("nav").exact_width(190.0).resizable(false).show(ctx, |ui| {
            ui.add_space(16.0);
            ui.label(RichText::new("AK820 Pro").size(22.0).strong().color(ACCENT));
            ui.label(RichText::new(t("configurator", "configuratore")).weak());
            ui.add_space(18.0);
            for &(tab, icon, en, it) in TABS {
                let selected = self.tab == tab;
                let text = RichText::new(format!("{icon}   {}", t(en, it))).size(16.0);
                let b = Button::new(if selected { text.color(Color32::WHITE) } else { text })
                    .fill(if selected { ACCENT } else { Color32::TRANSPARENT })
                    .min_size(Vec2::new(170.0, 38.0));
                if ui.add(b).clicked() {
                    self.tab = tab;
                }
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.add_space(10.0);
                ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).small().weak());
            });
        });
    }

    fn screenshots(&mut self, ctx: &egui::Context) {
        let Some(shots) = self.shots.as_mut().filter(|s| s.index < TABS.len()) else { return };
        let saved = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = saved {
            let (_, _, label, _) = TABS[shots.index];
            let _ = std::fs::create_dir_all(&shots.dir);
            let path = shots.dir.join(format!("{}-{}.png", shots.index + 1, label.replace(' ', "-").to_lowercase()));
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, bytes) {
                let _ = buf.save(&path);
            }
            shots.index += 1;
            shots.wait = 0;
            shots.requested = false;
            if shots.index >= TABS.len() {
                self.quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        self.tab = TABS[shots.index].0;
        shots.wait += 1;
        if shots.wait > 20 && !shots.requested {
            shots.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        ctx.request_repaint();
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(cmd) = self.tray_rx.try_recv() {
            match cmd {
                TrayCmd::Open => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayCmd::Apply(name) => self.apply_named(&name),
                TrayCmd::Quit => {
                    self.quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        self.handle_messages();
        self.screenshots(ctx);
        self.nav(ctx);
        self.status_bar(ctx);
        self.log_window(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                ui.add_space(8.0);
                match self.tab {
                    Tab::Lighting => self.tab_lighting(ui),
                    Tab::Colors => self.tab_colors(ui),
                    Tab::Keys => self.tab_keys(ui),
                    Tab::Macros => self.tab_macros(ui),
                    Tab::Screen => self.tab_screen(ui),
                    Tab::System => self.tab_system(ui),
                    Tab::Profiles => self.tab_profiles(ui),
                }
                ui.add_space(16.0);
            });
        });
    }
}

// ---- helpers ---------------------------------------------------------------------------

/// System fonts tried as fallback for glyphs egui's built-in fonts lack (arrows, ●).
const FALLBACK_FONTS: &[&str] = &[
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    "C:\\Windows\\Fonts\\segoeui.ttf",
    "C:\\Windows\\Fonts\\seguisym.ttf",
];

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (i, path) in FALLBACK_FONTS.iter().enumerate() {
        if let Ok(bytes) = std::fs::read(path) {
            let name = format!("fallback{i}");
            fonts.font_data.insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes)));
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(name.clone());
            }
        }
    }
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    setup_fonts(ctx);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(24, 24, 32);
    visuals.window_fill = Color32::from_rgb(30, 30, 40);
    visuals.extreme_bg_color = Color32::from_rgb(16, 16, 22);
    visuals.faint_bg_color = Color32::from_rgb(32, 32, 44);
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1.5_f32, Color32::from_rgb(200, 185, 255));
    visuals.hyperlink_color = ACCENT;
    for w in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
        &mut visuals.widgets.noninteractive,
    ] {
        w.corner_radius = CornerRadius::same(7);
    }
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(44, 44, 58);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(58, 56, 80);
    ctx.set_visuals(visuals);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(10.0, 8.0);
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
        s.spacing.slider_width = 220.0;
        s.text_styles.insert(TextStyle::Body, FontId::proportional(15.0));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(15.0));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(24.0));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(14.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(12.5));
    });
}

fn title(ui: &mut Ui, heading: &str, sub: &str) {
    ui.heading(RichText::new(heading).strong());
    ui.label(RichText::new(sub).weak());
    ui.add_space(10.0);
}

fn card<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(50, 50, 66)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).strong().size(16.0));
            ui.add_space(6.0);
            add(ui)
        })
        .inner
}

fn color_button(ui: &mut Ui, c: &mut Rgb) {
    let mut rgb = c.bytes();
    if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
        *c = Rgb(rgb[0], rgb[1], rgb[2]);
    }
    ui.label(RichText::new(format!("#{c}")).monospace().weak());
}

fn name_combo(ui: &mut Ui, id: &str, value: &mut String, names: &[&str], allow_empty: bool) {
    ComboBox::from_id_salt(id)
        .selected_text(if value.is_empty() { "—".to_string() } else { value.clone() })
        .height(320.0)
        .show_ui(ui, |ui| {
            if allow_empty {
                ui.selectable_value(value, String::new(), "—");
            }
            for n in names {
                ui.selectable_value(value, n.to_string(), *n);
            }
        });
}

fn profile_summary(p: &Profile) -> String {
    let mut parts: Vec<String> = Vec::new();
    match (&p.lighting, &p.key_colors) {
        (Some(l), _) if l.mode != Mode::Custom => parts.push(format!("💡 {}", mode_label(l.mode))),
        (_, Some(_)) => parts.push(format!("🎨 {}", t("per-key colors", "colori per tasto"))),
        _ => {}
    }
    let remaps = p.keymap.as_ref().map_or(0, |k| k.normal.len() + k.fn_layer.len());
    if remaps > 0 {
        parts.push(format!("⌨ {remaps} {}", t("remaps", "rimappature")));
    }
    if !p.macros.is_empty() {
        parts.push(format!("📝 {} macro", p.macros.len()));
    }
    if p.screen.is_some() {
        parts.push(format!("🖼 {}", t("screen image", "immagine display")));
    }
    if parts.is_empty() {
        return t("empty", "vuoto").into();
    }
    parts.join("  ·  ")
}

fn macro_mode_label(m: MacroMode) -> &'static str {
    match m {
        MacroMode::Once => t("Once", "Una volta"),
        MacroMode::Repeat => t("Repeat N times", "Ripeti N volte"),
        MacroMode::Toggle => t("Start/stop", "Avvia/ferma"),
        MacroMode::Hold => t("While held", "Finché tenuto"),
        MacroMode::Step => t("One event per press", "Un evento per pressione"),
    }
}

fn rename_macro_refs(km: &mut Keymap, old: &str, new: &str) {
    let (from, to) = (format!("macro:{old}"), format!("macro:{new}"));
    for t in km.normal.values_mut().chain(km.fn_layer.values_mut()) {
        if *t == from {
            *t = to.clone();
        }
    }
}
