//! Graphical interface for the AK820 Pro (egui), sharing the CLI state file.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod layout;
mod worker;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

use eframe::egui::{
    self, Button, Color32, ComboBox, CornerRadius, FontId, RichText, Slider, Stroke, TextStyle, TextureHandle,
    TextureOptions, Ui, Vec2,
};

use ak820::lighting::{Direction, Lighting, Mode};
use ak820::macros::{MacroDef, MacroMode};
use ak820::profile::{self, FnMode, KeyColors, Keymap, Profile, Screen, SleepTime, System};
use ak820::screen::{self, Fit, Frame};
use ak820::{actions, keys, Rgb};

use layout::{KeyLook, LAYOUT};
use worker::{Job, Msg, Op, Status};

const ACCENT: Color32 = Color32::from_rgb(124, 92, 255);
const ACCENT_SOFT: Color32 = Color32::from_rgb(70, 56, 140);
const OK: Color32 = Color32::from_rgb(80, 200, 120);
const WARN: Color32 = Color32::from_rgb(240, 180, 60);
const ERR: Color32 = Color32::from_rgb(235, 90, 90);
const KEYCAP: Color32 = Color32::from_rgb(48, 48, 60);

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AK820 Pro")
            .with_app_id("ak820")
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([900.0, 620.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native("AK820 Pro", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
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

const TABS: &[(Tab, &str, &str)] = &[
    (Tab::Lighting, "💡", "Luci"),
    (Tab::Colors, "🎨", "Colori tasti"),
    (Tab::Keys, "⌨", "Tasti"),
    (Tab::Macros, "📝", "Macro"),
    (Tab::Screen, "🖼", "Display"),
    (Tab::System, "⚙", "Sistema"),
    (Tab::Profiles, "📁", "Profili"),
];

fn mode_label(m: Mode) -> &'static str {
    match m {
        Mode::Off => "Spento",
        Mode::Static => "Fisso",
        Mode::SingleOn => "Tasto acceso",
        Mode::SingleOff => "Tasto spento",
        Mode::Glittering => "Scintillio",
        Mode::Falling => "Pioggia",
        Mode::Colourful => "Colorato",
        Mode::Breath => "Respiro",
        Mode::Spectrum => "Spettro",
        Mode::Outward => "Onde",
        Mode::Scrolling => "Scorrimento",
        Mode::Rolling => "Onda",
        Mode::Rotating => "Rotazione",
        Mode::Explode => "Esplosione",
        Mode::Launch => "Lancio",
        Mode::Ripples => "Increspature",
        Mode::Flowing => "Flusso",
        Mode::Pulsating => "Pulsazione",
        Mode::Tilt => "Inclinazione",
        Mode::Shuttle => "Navetta",
        Mode::Custom => "Per tasto",
    }
}

fn direction_label(d: Direction) -> &'static str {
    match d {
        Direction::Left => "← Verso sinistra",
        Direction::Right => "Verso destra →",
        Direction::Up => "↑ Verso l'alto",
        Direction::Down => "↓ Verso il basso",
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

const KINDS: &[(Kind, &str)] = &[
    (Kind::Key, "Tasto / scorciatoia"),
    (Kind::Media, "Multimediale"),
    (Kind::Mouse, "Mouse"),
    (Kind::Multi, "Più tasti insieme"),
    (Kind::Text, "Scrivi testo"),
    (Kind::Macro, "Macro salvata"),
    (Kind::Default, "Originale"),
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

const MOUSE: &[(&str, &str)] = &[
    ("mouse:left", "Clic sinistro"),
    ("mouse:right", "Clic destro"),
    ("mouse:middle", "Clic centrale"),
    ("wheel:up", "Rotella su"),
    ("wheel:down", "Rotella giù"),
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
        } else if MOUSE.iter().any(|(v, _)| *v == lower) {
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
    frames: Option<Arc<Vec<Frame>>>,
    preview: Vec<(TextureHandle, u32)>,
    preview_start: Instant,
    screen_error: Option<String>,

    system: System,
    battery: Option<Option<u8>>,
    confirm_reset: bool,
    profile_error: Option<String>,

    shots: Option<Shots>,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        setup_style(&cc.egui_ctx);
        let (job_tx, job_rx) = channel();
        let (msg_tx, msg_rx) = channel();
        worker::spawn(job_rx, msg_tx, cc.egui_ctx.clone());

        let state = profile::load_state();
        let mut app = App {
            tab: Tab::Lighting,
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
            frames: None,
            preview: Vec::new(),
            preview_start: Instant::now(),
            screen_error: None,
            system: System::default(),
            battery: None,
            confirm_reset: false,
            profile_error: None,
            shots: std::env::var_os("AK820_GUI_SHOTS")
                .map(|d| Shots { dir: d.into(), index: 0, wait: 0, requested: false }),
        };
        app.state = state;
        app.load_drafts();
        if app.shots.is_some() {
            app.selected = Some("pgdn");
            app.editor = TargetEditor::from_target("ctrl+end");
            let demo = Path::new("examples/test-anim.gif");
            if demo.exists() {
                app.load_screen(&cc.egui_ctx, demo.to_path_buf());
            }
        }
        app
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
            resp.on_disabled_hover_text("Funziona solo con il cavo USB")
        } else {
            resp
        };
        resp.clicked()
    }

    // ---- tabs --------------------------------------------------------------------------

    fn tab_lighting(&mut self, ui: &mut Ui) {
        title(ui, "Effetti luce", "Gli effetti integrati della tastiera, salvati nella sua memoria.");
        card(ui, "Effetto", |ui| {
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
        card(ui, "Parametri", |ui| {
            ui.add_enabled_ui(!off, |ui| {
                egui::Grid::new("light-params").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                    ui.label("Colore");
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!self.light.rainbow, |ui| color_button(ui, &mut self.light.color));
                        ui.checkbox(&mut self.light.rainbow, "Arcobaleno");
                    });
                    ui.end_row();
                    ui.label("Luminosità");
                    ui.add(Slider::new(&mut self.light.brightness, 0..=5));
                    ui.end_row();
                    ui.label("Velocità");
                    ui.add_enabled(self.light.mode != Mode::Static, Slider::new(&mut self.light.speed, 0..=5));
                    ui.end_row();
                    ui.label("Direzione");
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
                            ui.label(RichText::new("questo effetto non ha una direzione").weak());
                        }
                    }
                    ui.end_row();
                });
            });
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if self.action(ui, "✔ Applica", false) {
                let l = self.light.clone();
                let mut s = self.state.clone();
                s.lighting = Some(l.clone());
                self.run(&format!("Luci: {}", mode_label(l.mode)), Op::Lighting(l), Some(s));
            }
            if ui.add_enabled(!self.busy && self.connected(), Button::new("Spegni luci")).clicked() {
                let l = Lighting::new(Mode::Off);
                let mut s = self.state.clone();
                s.lighting = Some(l.clone());
                self.run("Luci spente", Op::Lighting(l), Some(s));
            }
        });
    }

    fn tab_colors(&mut self, ui: &mut Ui) {
        title(
            ui,
            "Colori per tasto",
            "Clic: colora con il pennello · clic destro: torna al colore di base. Solo via cavo.",
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
            card(&mut cols[0], "Strumenti", |ui| {
                egui::Grid::new("color-tools").num_columns(2).spacing([20.0, 10.0]).show(ui, |ui| {
                    ui.label("Pennello");
                    color_button(ui, &mut self.brush);
                    ui.end_row();
                    ui.label("Colore di base");
                    color_button(ui, &mut self.colors.base);
                    ui.end_row();
                    ui.label("Luminosità");
                    ui.add(Slider::new(&mut self.colors.brightness, 1..=5));
                    ui.end_row();
                });
            });
            card(&mut cols[1], "Riempimenti rapidi", |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Tutto col pennello").clicked() {
                        self.colors.base = self.brush;
                        self.colors.keys.clear();
                    }
                    if ui.button("Cancella tasti colorati").clicked() {
                        self.colors.keys.clear();
                    }
                    if ui.button("Arcobaleno").clicked() {
                        for (idx, x, y) in actions::key_grid() {
                            if let Some(name) = keys::canonical_name(idx) {
                                self.colors.keys.insert(name.into(), actions::hsv(x * 0.8 + y * 0.1));
                            }
                        }
                    }
                    if ui.button("WASD + frecce").clicked() {
                        for k in ["w", "a", "s", "d", "up", "down", "left", "right"] {
                            self.colors.keys.insert(k.into(), self.brush);
                        }
                    }
                });
            });
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if self.action(ui, "✔ Applica colori", true) {
                let c = self.colors.clone();
                let mut s = self.state.clone();
                s.key_colors = Some(c.clone());
                s.lighting = Some(Lighting::new(Mode::Custom));
                self.run("Colori per tasto", Op::KeyColors(c), Some(s));
            }
            if ui
                .add_enabled(!self.busy && self.wired(), Button::new("▶ Demo arcobaleno live (10 s)"))
                .on_hover_text("Animazione in tempo reale, non salvata")
                .clicked()
            {
                self.run("Demo live", Op::Demo(10), None);
            }
        });
    }

    fn tab_keys(&mut self, ui: &mut Ui) {
        title(
            ui,
            "Rimappatura tasti",
            "Le modifiche sono salvate nella tastiera e valgono su ogni computer.",
        );
        ui.horizontal(|ui| {
            ui.label("Layer:");
            if ui.selectable_label(!self.fn_layer, "Normale").clicked() {
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
            card(ui, "Tasto", |ui| {
                ui.label("Clicca un tasto sulla tastiera qui sopra.");
            });
            return;
        };
        let label = LAYOUT.iter().find(|k| k.0 == key).map_or(key, |k| k.1);
        let heading = format!("{}{}", if self.fn_layer { "Fn + " } else { "" }, if label.is_empty() { "Spazio" } else { label });
        card(ui, &heading, |ui| {
            ComboBox::from_id_salt("kind")
                .selected_text(KINDS.iter().find(|k| k.0 == self.editor.kind).unwrap().1)
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (k, l) in KINDS {
                        ui.selectable_value(&mut self.editor.kind, *k, *l);
                    }
                });
            ui.add_space(6.0);
            let e = &mut self.editor;
            match e.kind {
                Kind::Default => {
                    ui.label("Il tasto torna alla sua funzione di fabbrica.");
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
                        for (v, l) in MOUSE {
                            ui.selectable_value(&mut e.mouse, v.to_string(), *l);
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
                            .hint_text("Testo da scrivere (layout USA)")
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                }
                Kind::Macro => {
                    if self.macros.is_empty() {
                        ui.label("Nessuna macro: creane una nella scheda Macro.");
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
                if ui.button("Assegna").clicked() {
                    self.assign(key, target);
                }
                if ui.button("Ripristina tasto").clicked() {
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
        card(ui, if fn_layer { "Rimappature layer Fn" } else { "Rimappature layer normale" }, |ui| {
            let layer = if fn_layer { &mut self.keymap.fn_layer } else { &mut self.keymap.normal };
            if layer.is_empty() {
                ui.label(RichText::new("Nessuna: layout di fabbrica.").weak());
            }
            let mut remove = None;
            egui::ScrollArea::vertical().max_height(170.0).show(ui, |ui| {
                for (k, t) in layer.iter() {
                    ui.horizontal(|ui| {
                        if ui.small_button("✖").on_hover_text("Rimuovi").clicked() {
                            remove = Some(k.clone());
                        }
                        ui.label(RichText::new(k).strong());
                        ui.label("→");
                        ui.label(RichText::new(t).monospace());
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
                if self.action(ui, "⬆ Invia tasti e macro", false) {
                    let mut s = self.state.clone();
                    s.keymap = Some(self.keymap.clone());
                    s.macros = self.macros.clone();
                    self.run("Rimappatura e macro", Op::Keymap(s.clone()), Some(s));
                }
            });
            if ui.add_enabled(changed, Button::new("Annulla modifiche")).clicked() {
                self.keymap = self.state.keymap.clone().unwrap_or_default();
                self.macros = self.state.macros.clone();
            }
            if show_clear && ui.button("Svuota questo layer").clicked() {
                if self.fn_layer {
                    self.keymap.fn_layer.clear();
                } else {
                    self.keymap.normal.clear();
                }
            }
            if changed {
                ui.colored_label(WARN, "● modifiche non inviate");
            }
        });
    }

    fn tab_macros(&mut self, ui: &mut Ui) {
        title(ui, "Macro", "Sequenze salvate nella tastiera, da assegnare a un tasto nella scheda Tasti.");
        ui.columns(2, |cols| {
            card(&mut cols[0], "Macro salvate", |ui| {
                if self.macros.is_empty() {
                    ui.label(RichText::new("Nessuna macro.").weak());
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
                    let text = format!("{name}  ·  {} passi{}", m.steps.len(), if used { "  · in uso" } else { "" });
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
                if ui.button("➕ Nuova macro").clicked() {
                    self.macro_sel = None;
                    self.macro_name = format!("macro{}", self.macros.len() + 1);
                    self.macro_steps = "text:Ciao!".into();
                    self.macro_mode = MacroMode::Once;
                    self.macro_count = 1;
                }
            });
            let ui = &mut cols[1];
            card(ui, "Editor", |ui| {
                egui::Grid::new("macro-edit").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label("Nome");
                    ui.text_edit_singleline(&mut self.macro_name);
                    ui.end_row();
                    ui.label("Modalità");
                    ComboBox::from_id_salt("macro-mode")
                        .selected_text(macro_mode_label(self.macro_mode))
                        .show_ui(ui, |ui| {
                            for m in [MacroMode::Once, MacroMode::Repeat, MacroMode::Toggle, MacroMode::Hold, MacroMode::Step] {
                                ui.selectable_value(&mut self.macro_mode, m, macro_mode_label(m));
                            }
                        });
                    ui.end_row();
                    if self.macro_mode == MacroMode::Repeat {
                        ui.label("Ripetizioni");
                        ui.add(Slider::new(&mut self.macro_count, 1..=255));
                        ui.end_row();
                    }
                });
                ui.label("Passi (uno per riga):");
                ui.add(
                    egui::TextEdit::multiline(&mut self.macro_steps)
                        .code_editor()
                        .desired_rows(7)
                        .desired_width(f32::INFINITY),
                );
                ui.label(
                    RichText::new("text:Ciao · tap:ctrl+c · down:shift · up:shift · delay:100 · click:left")
                        .small()
                        .weak(),
                );
                let steps: Vec<String> =
                    self.macro_steps.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
                let valid = ak820::macros::events(&steps);
                match &valid {
                    Ok(ev) => {
                        ui.label(RichText::new(format!("✔ {} eventi", ev.len())).color(OK));
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
                    if ui.add_enabled(valid.is_ok() && !name.is_empty(), Button::new("💾 Salva macro")).clicked() {
                        if let Some(old) = self.macro_sel.take().filter(|o| *o != name) {
                            self.macros.remove(&old);
                            rename_macro_refs(&mut self.keymap, &old, &name);
                        }
                        self.macros.insert(name.clone(), MacroDef { steps, mode: self.macro_mode, count: self.macro_count });
                        self.macro_sel = Some(name);
                        self.macro_error = None;
                    }
                    if let Some(sel) = self.macro_sel.clone() {
                        if ui.button("🗑 Elimina").clicked() {
                            let target = format!("macro:{sel}");
                            let used = self.keymap.normal.values().chain(self.keymap.fn_layer.values()).any(|t| *t == target);
                            if used {
                                self.macro_error = Some("La macro è assegnata a un tasto: rimuovi prima l'assegnazione.".into());
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

    fn load_screen(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.screen_error = None;
        match actions::load_frames(&path, self.fit) {
            Ok(frames) => {
                self.preview = frames
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let img = egui::ColorImage::from_rgb([screen::SIZE as usize; 2], &rgb565_to_rgb(&f.rgb565));
                        (ctx.load_texture(format!("frame{i}"), img, TextureOptions::NEAREST), f.duration_ms.max(30))
                    })
                    .collect();
                self.preview_start = Instant::now();
                self.frames = Some(Arc::new(frames));
            }
            Err(e) => {
                self.frames = None;
                self.preview.clear();
                self.screen_error = Some(e.to_string());
            }
        }
        self.screen_path = Some(path);
    }

    fn tab_screen(&mut self, ui: &mut Ui) {
        title(ui, "Display", "Immagine o GIF animata sul display 128×128. Trascina un file qui o sceglilo.");
        let ctx = ui.ctx().clone();
        let dropped = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()));
        if let Some(p) = dropped {
            self.load_screen(&ctx, p);
        }
        ui.columns(2, |cols| {
            card(&mut cols[0], "Anteprima (colori come sul display)", |ui| {
                let size = Vec2::splat(256.0);
                if self.preview.is_empty() {
                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    ui.painter().rect_filled(rect, CornerRadius::same(8), Color32::BLACK);
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "nessuna immagine", FontId::proportional(14.0), Color32::GRAY);
                } else {
                    let total: u32 = self.preview.iter().map(|p| p.1).sum();
                    let mut t = (self.preview_start.elapsed().as_millis() as u32) % total.max(1);
                    let mut idx = 0;
                    for (i, (_, d)) in self.preview.iter().enumerate() {
                        if t < *d {
                            idx = i;
                            break;
                        }
                        t -= d;
                    }
                    let tex = &self.preview[idx].0;
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), size)).corner_radius(8));
                    if self.preview.len() > 1 {
                        ctx.request_repaint_after(std::time::Duration::from_millis(15));
                    }
                }
            });
            let ui = &mut cols[1];
            card(ui, "Immagine", |ui| {
                if ui.button("📂 Scegli immagine…").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("Immagini", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
                        .pick_file()
                    {
                        self.load_screen(&ctx, p);
                    }
                }
                if let Some(p) = &self.screen_path {
                    ui.label(RichText::new(p.file_name().unwrap_or_default().to_string_lossy()).monospace());
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Adattamento:");
                    let before = self.fit;
                    ui.selectable_value(&mut self.fit, Fit::Crop, "Ritaglia");
                    ui.selectable_value(&mut self.fit, Fit::Pad, "Bande nere");
                    ui.selectable_value(&mut self.fit, Fit::Stretch, "Allunga");
                    if before != self.fit {
                        if let Some(p) = self.screen_path.clone() {
                            self.load_screen(&ctx, p);
                        }
                    }
                });
                if let Some(f) = &self.frames {
                    let blocks = screen::blocks(f).len();
                    ui.label(format!(
                        "{} fotogramm{} · {} blocchi · ~{} s di invio",
                        f.len(),
                        if f.len() == 1 { "a" } else { "i" },
                        blocks,
                        blocks * 50 / 1000 + 1
                    ));
                    if f.len() >= screen::MAX_FRAMES {
                        ui.colored_label(WARN, format!("Limitata ai primi {} fotogrammi", screen::MAX_FRAMES));
                    }
                }
                if let Some(e) = &self.screen_error {
                    ui.colored_label(ERR, e);
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ready = self.frames.is_some();
                ui.add_enabled_ui(ready, |ui| {
                    if self.action(ui, "⬆ Invia al display", true) {
                        let frames = self.frames.clone().unwrap();
                        let mut s = self.state.clone();
                        if let Some(p) = &self.screen_path {
                            let image = std::fs::canonicalize(p).unwrap_or(p.clone());
                            s.screen = Some(Screen { image, fit: self.fit });
                        }
                        self.run("Invio al display", Op::Screen(frames), Some(s));
                    }
                });
                if ui.add_enabled(!self.busy && self.connected(), Button::new("🕒 Mostra orologio")).clicked() {
                    self.run("Orologio", Op::Clock, None);
                }
            });
        });
    }

    fn tab_system(&mut self, ui: &mut Ui) {
        title(ui, "Sistema", "Orologio, risparmio energetico e comportamento dei tasti.");
        card(ui, "Orologio del display", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(chrono::Local::now().format("%d/%m/%Y  %H:%M:%S").to_string()).monospace().size(18.0));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
                if ui.add_enabled(!self.busy && self.connected(), Button::new("🕒 Sincronizza con il PC")).clicked() {
                    self.run("Orologio", Op::Clock, None);
                }
            });
        });
        ui.add_space(8.0);
        card(ui, "Impostazioni", |ui| {
            egui::Grid::new("system").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                ui.label("Sospensione (a batteria)");
                ui.horizontal(|ui| {
                    for (v, l) in [(SleepTime::Never, "Mai"), (SleepTime::Min1, "1 min"), (SleepTime::Min5, "5 min"), (SleepTime::Min30, "30 min")] {
                        ui.selectable_value(&mut self.system.sleep, v, l);
                    }
                });
                ui.end_row();
                ui.label("Tasto Fn");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.system.fn_mode, FnMode::Hold, "Tenuto premuto");
                    ui.selectable_value(&mut self.system.fn_mode, FnMode::Toggle, "Attiva/disattiva");
                });
                ui.end_row();
                ui.label("Antirimbalzo");
                ui.add(Slider::new(&mut self.system.response, 1..=5).text("1 veloce · 5 lento"));
                ui.end_row();
                ui.label("Blocca");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.system.block_win, "Win");
                    ui.checkbox(&mut self.system.block_alt_f4, "Alt+F4");
                    ui.checkbox(&mut self.system.block_alt_tab, "Alt+Tab");
                });
                ui.end_row();
            });
            ui.add_space(8.0);
            if self.action(ui, "✔ Applica impostazioni", false) {
                if let Ok(settings) = self.system.settings() {
                    let mut s = self.state.clone();
                    s.system = Some(self.system);
                    self.run("Impostazioni di sistema", Op::System(settings), Some(s));
                }
            }
        });
        ui.add_space(8.0);
        card(ui, "Batteria", |ui| {
            ui.horizontal(|ui| {
                let enabled = !self.busy && self.status == Status::Receiver;
                if ui
                    .add_enabled(enabled, Button::new("🔋 Leggi batteria"))
                    .on_disabled_hover_text("Disponibile solo con il ricevitore 2.4 GHz")
                    .clicked()
                {
                    self.run("Batteria", Op::Battery, None);
                }
                match self.battery {
                    Some(Some(v)) => {
                        ui.add(egui::ProgressBar::new(v as f32 / 100.0).text(format!("{v}%")).desired_width(200.0));
                    }
                    Some(None) => {
                        ui.label("Livello non disponibile (tastiera in standby?)");
                    }
                    None => {}
                }
            });
        });
    }

    fn tab_profiles(&mut self, ui: &mut Ui) {
        title(ui, "Profili", "Salva, carica e reinvia l'intera configurazione (file TOML, compatibili con la CLI).");
        card(ui, "File", |ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.busy && self.connected(), Button::new("📂 Importa e applica…")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Profilo", &["toml"]).pick_file() {
                        match Profile::load(&p).and_then(|prof| prof.validate().map(|_| prof)) {
                            Ok(prof) => {
                                let mut merged = self.state.clone();
                                merged.merge(&prof);
                                self.profile_error = None;
                                self.resync_after_job = true;
                                self.run("Applica profilo", Op::Apply(prof), Some(merged));
                            }
                            Err(e) => self.profile_error = Some(e.to_string()),
                        }
                    }
                }
                if ui.button("💾 Esporta…").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("Profilo", &["toml"])
                        .set_file_name("ak820-profilo.toml")
                        .save_file()
                    {
                        match self.state.save(&p) {
                            Ok(()) => self.log.push((format!("Profilo esportato in {}", p.display()), false)),
                            Err(e) => self.profile_error = Some(e.to_string()),
                        }
                    }
                }
                if ui
                    .add_enabled(!self.busy && self.connected(), Button::new("🔁 Reinvia tutto"))
                    .on_hover_text("Rimanda alla tastiera l'ultima configurazione salvata")
                    .clicked()
                {
                    let mut p = self.state.clone();
                    p.sync_clock = Some(true);
                    if p.keymap.is_none() && !p.macros.is_empty() {
                        p.keymap = Some(Keymap::default());
                    }
                    self.run("Reinvio configurazione", Op::Apply(p), None);
                }
            });
            if let Some(e) = &self.profile_error {
                ui.colored_label(ERR, e);
            }
        });
        ui.add_space(8.0);
        card(ui, "Ripristino di fabbrica", |ui| {
            ui.label("Rimuove rimappature e macro, rimette l'onda arcobaleno e le impostazioni di default.");
            ui.horizontal(|ui| {
                if !self.confirm_reset {
                    if ui.add_enabled(!self.busy && self.connected(), Button::new("Ripristina…")).clicked() {
                        self.confirm_reset = true;
                    }
                } else {
                    ui.colored_label(WARN, "Sicuro?");
                    if ui.add(Button::new(RichText::new("Sì, ripristina").color(Color32::WHITE)).fill(ERR)).clicked() {
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
                        self.run("Ripristino di fabbrica", Op::Apply(p.clone()), Some(p));
                    }
                    if ui.button("Annulla").clicked() {
                        self.confirm_reset = false;
                    }
                }
            });
        });
        ui.add_space(8.0);
        card(ui, "Configurazione salvata", |ui| {
            ui.label(RichText::new(profile::state_path().display().to_string()).monospace().weak());
            let text = toml::to_string_pretty(&self.state).unwrap_or_default();
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(if text.is_empty() { "(vuota)".into() } else { text }).monospace()));
            });
        });
    }

    // ---- chrome ------------------------------------------------------------------------

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").exact_height(38.0).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                let (color, text) = match &self.status {
                    Status::Searching => (Color32::GRAY, "Ricerca tastiera…".to_string()),
                    Status::Wired(pid) => (OK, format!("Collegata via cavo USB (PID {pid:04x})")),
                    Status::Receiver => (OK, "Collegata via ricevitore 2.4 GHz".into()),
                    Status::Bluetooth => (WARN, "In Bluetooth: usa il cavo o il ricevitore".into()),
                    Status::NotFound => (ERR, "Tastiera non trovata".into()),
                    Status::Permission => (WARN, "Permessi mancanti per accedere alla tastiera".into()),
                    Status::Error(e) => (ERR, format!("Errore: {e}")),
                };
                ui.label(RichText::new("●").color(color));
                ui.label(text);
                #[cfg(target_os = "linux")]
                if self.status == Status::Permission
                    && ui
                        .add_enabled(!self.busy, Button::new("🔑 Concedi accesso"))
                        .on_hover_text("Installa la regola udev (richiede la password di amministratore)")
                        .clicked()
                {
                    self.run("Installazione regola udev", Op::InstallUdev, None);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.selectable_label(self.show_log, "📜 Registro").clicked() {
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
        egui::Window::new("Registro")
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
            ui.label(RichText::new("configuratore").weak());
            ui.add_space(18.0);
            for &(tab, icon, label) in TABS {
                let selected = self.tab == tab;
                let text = RichText::new(format!("{icon}   {label}")).size(16.0);
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
            let (tab, _, label) = TABS[shots.index];
            let _ = std::fs::create_dir_all(&shots.dir);
            let path = shots.dir.join(format!("{}-{}.png", shots.index + 1, label.replace(' ', "-").to_lowercase()));
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, bytes) {
                let _ = buf.save(&path);
            }
            let _ = tab;
            shots.index += 1;
            shots.wait = 0;
            shots.requested = false;
            if shots.index >= TABS.len() {
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

fn macro_mode_label(m: MacroMode) -> &'static str {
    match m {
        MacroMode::Once => "Una volta",
        MacroMode::Repeat => "Ripeti N volte",
        MacroMode::Toggle => "Avvia/ferma",
        MacroMode::Hold => "Finché tenuto",
        MacroMode::Step => "Un evento per pressione",
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

/// Decode the RGB565 frame back to RGB, so the preview shows the display's real colors.
fn rgb565_to_rgb(data: &[u8]) -> Vec<u8> {
    data.as_chunks::<2>()
        .0
        .iter()
        .flat_map(|&c| {
            let v = u16::from_le_bytes(c);
            let r = ((v >> 11) & 0x1F) as u8;
            let g = ((v >> 5) & 0x3F) as u8;
            let b = (v & 0x1F) as u8;
            [(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)]
        })
        .collect()
}
