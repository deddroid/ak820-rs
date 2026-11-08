//! The Shortcuts tab: the combinations printed in the AJAZZ manuals of the AK820 Pro
//! (tri-mode, screen) and of the AK820 (wired), to use the keyboard without the app.

use ak820::Model;
use eframe::egui::{self, Align, Layout, RichText, Ui, Vec2};

use crate::i18n::t;
use crate::macro_page::{chip, key_caps, key_label, keycap};
use crate::{card, title, App, WARN};

/// Which keyboards a combination works on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum On {
    Both,
    /// AK820 Pro: screen, Bluetooth and 2.4 GHz.
    Pro,
    /// AK820 wired.
    Wired,
}

impl On {
    fn matches(self, model: Model) -> bool {
        match self {
            On::Both => true,
            On::Pro => model == Model::Ak820Pro,
            On::Wired => model == Model::Ak820,
        }
    }
}

/// `keys` are layout names (`fn` first when it is a Fn combination); a name not in the
/// layout is printed as it is ("1/2/3").
struct Shortcut {
    on: On,
    keys: &'static [&'static str],
    en: &'static str,
    it: &'static str,
}

const fn s(on: On, keys: &'static [&'static str], en: &'static str, it: &'static str) -> Shortcut {
    Shortcut { on, keys, en, it }
}

const LIGHTS: &[Shortcut] = &[
    s(On::Both, &["fn", "backslash"], "Next lighting effect", "Effetto luce successivo"),
    s(On::Both, &["fn", "up"], "Brighter", "Luminosità +"),
    s(On::Both, &["fn", "down"], "Dimmer", "Luminosità −"),
    s(On::Both, &["fn", "equal"], "Faster effect", "Effetto più veloce"),
    s(On::Both, &["fn", "minus"], "Slower effect", "Effetto più lento"),
    s(On::Both, &["fn", "left"], "Effect direction", "Direzione dell'effetto"),
    s(On::Both, &["fn", "right"], "Light color (RGB effects)", "Colore della luce (effetti RGB)"),
    s(On::Both, &["fn", "x"], "Lights on or off", "Accendi o spegni le luci"),
    s(On::Both, &["fn", "1/2/3"], "Choose custom light group 1, 2 or 3", "Scegli il gruppo di luci personali 1, 2 o 3"),
    s(On::Both, &["fn", "grave"], "Save the current effect in that group", "Salva l'effetto attuale in quel gruppo"),
];

const SYSTEM: &[Shortcut] = &[
    s(On::Both, &["knob"], "Turn: volume. Press: mute", "Ruota: volume. Premi: muto"),
    s(On::Both, &["fn", "win"], "Lock or unlock the Win key", "Blocca o sblocca il tasto Win"),
    s(On::Both, &["fn", "pgup"], "Print screen", "Stamp (cattura schermo)"),
    s(On::Both, &["fn", "pgdn"], "Pause", "Pausa"),
    s(On::Wired, &["fn", "delete"], "Scroll lock", "Bloc Scorr"),
    s(On::Wired, &["fn", "home"], "Insert", "Ins"),
    s(On::Wired, &["fn", "a"], "Mac mode", "Modalità Mac"),
    s(On::Wired, &["fn", "s"], "Windows mode", "Modalità Windows"),
    s(On::Both, &["fn", "space"], "Hold 3–5 s: factory reset", "Tieni premuto 3–5 s: ripristino di fabbrica"),
];

const SCREEN: &[Shortcut] = &[
    s(
        On::Pro,
        &["fn", "knob"],
        "Settings menu: turn to move, press to confirm (Mac or Windows is under System)",
        "Menu impostazioni: ruota per spostarti, premi per confermare (Mac o Windows è in System)",
    ),
    s(On::Pro, &["fn", "home"], "Animation or main page", "Animazione o pagina principale"),
    s(On::Pro, &["fn", "delete"], "Screen on or off", "Accendi o spegni lo schermo"),
];

const CONNECTION: &[Shortcut] = &[
    s(
        On::Pro,
        &["fn", "Q/W/E"],
        "Bluetooth channel 1, 2 or 3. Hold 3 s: pair it",
        "Canale Bluetooth 1, 2 o 3. Tieni premuto 3 s: abbinalo",
    ),
    s(On::Pro, &["fn", "r"], "Hold 3 s: pair the 2.4 GHz receiver", "Tieni premuto 3 s: abbina il ricevitore 2.4 GHz"),
];

/// `(F key, Windows en/it, Mac en/it)`: the same on both keyboards.
const F_ROW: &[(&str, &str, &str, &str, &str)] = &[
    ("f1", "Media player", "Lettore multimediale", "Screen brightness −", "Luminosità schermo −"),
    ("f2", "Volume −", "Volume −", "Screen brightness +", "Luminosità schermo +"),
    ("f3", "Volume +", "Volume +", "Mission Control", "Mission Control"),
    ("f4", "Mute", "Muto", "Launchpad", "Launchpad"),
    ("f5", "Stop", "Stop", "Keyboard backlight −", "Retroilluminazione −"),
    ("f6", "Previous track", "Brano precedente", "Keyboard backlight +", "Retroilluminazione +"),
    ("f7", "Play/pause", "Play/pausa", "Previous track", "Brano precedente"),
    ("f8", "Next track", "Brano successivo", "Play/pause", "Play/pausa"),
    ("f9", "Mail", "Posta", "Next track", "Brano successivo"),
    ("f10", "Browser", "Browser", "Mute", "Muto"),
    ("f11", "This PC", "Questo PC", "Volume −", "Volume −"),
    ("f12", "Calculator", "Calcolatrice", "Volume +", "Volume +"),
];

/// Room for the keys column, so descriptions line up.
const KEYS_WIDTH: f32 = 160.0;

impl App {
    pub(crate) fn tab_shortcuts(&mut self, ui: &mut Ui) {
        title(
            ui,
            t("Keyboard shortcuts", "Scorciatoie della tastiera"),
            t(
                "Combinations that work on the keyboard by itself, from the AJAZZ manual. A key remapped on the Fn layer replaces its combination.",
                "Combinazioni che funzionano sulla tastiera da sola, dal manuale AJAZZ. Un tasto rimappato nel layer Fn sostituisce la sua combinazione.",
            ),
        );
        let model = self.model;
        ui.columns(2, |cols| {
            self.shortcut_card(&mut cols[0], t("Lighting", "Luci"), LIGHTS);
            if model.has_screen() {
                self.shortcut_card(&mut cols[1], t("Screen", "Schermo"), SCREEN);
                cols[1].add_space(8.0);
                self.shortcut_card(&mut cols[1], t("Connection", "Connessione"), CONNECTION);
                cols[1].add_space(8.0);
            }
            self.shortcut_card(&mut cols[1], t("Keys and system", "Tasti e sistema"), SYSTEM);
        });
        ui.add_space(8.0);
        card(ui, t("F keys with Fn", "Tasti F con Fn"), |ui| {
            egui::Grid::new("f-row").num_columns(3).spacing([24.0, 6.0]).striped(false).show(ui, |ui| {
                ui.label("");
                ui.label(RichText::new("Windows").strong());
                ui.label(RichText::new("Mac").strong());
                ui.end_row();
                for (key, win_en, win_it, mac_en, mac_it) in F_ROW {
                    ui.horizontal(|ui| {
                        key_caps(ui, model, true, key);
                        self.remapped_chip(ui, key);
                    });
                    ui.label(t(win_en, win_it));
                    ui.label(t(mac_en, mac_it));
                    ui.end_row();
                }
            });
        });
    }

    fn shortcut_card(&self, ui: &mut Ui, heading: &str, list: &[Shortcut]) {
        let model = self.model;
        card(ui, heading, |ui| {
            for sc in list.iter().filter(|sc| sc.on.matches(model)) {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(KEYS_WIDTH, 26.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.set_min_width(KEYS_WIDTH);
                            match sc.keys {
                                ["fn", key] => {
                                    key_caps(ui, model, true, key);
                                    self.remapped_chip(ui, key);
                                }
                                keys => {
                                    for key in keys {
                                        keycap(ui, &key_label(model, key));
                                    }
                                }
                            }
                        },
                    );
                    // Wraps at the end of the row, centered on the keys when it fits on one line.
                    ui.add(egui::Label::new(t(sc.en, sc.it)).wrap());
                });
                ui.add_space(2.0);
            }
        });
    }

    /// Marks a Fn combination the user has replaced with a remap.
    fn remapped_chip(&self, ui: &mut Ui, key: &str) {
        if self.keymap.fn_layer.contains_key(key) {
            chip(ui, t("remapped", "rimappato"), WARN);
        }
    }
}
