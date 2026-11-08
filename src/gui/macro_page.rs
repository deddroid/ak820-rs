//! The Macros tab: what each macro does, which key plays it and whether the keyboard
//! already has it. Steps are edited as typed rows instead of the `kind:value` text.

use ak820::macros::{self, MacroDef, MacroMode, Step, DEFAULT_HOLD_MS, REPEAT_MAX};
use ak820::profile::Keymap;
use ak820::{keys, Model};
use eframe::egui::{
    self, Align, Button, Color32, ComboBox, CornerRadius, DragValue, FontId, Layout, Margin, RichText, Sense, Ui, Vec2,
};

use crate::i18n::t;
use crate::{
    accent, card, layout, macro_mode_label, mix, name_combo, rename_macro_refs, title, App, ERR, KEY_NAMES, MODS, OK,
    WARN,
};

/// Where a macro stands, as shown next to it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MacroStatus {
    /// Assigned to a key and sent as it is now.
    OnKeyboard,
    /// Assigned, but the keyboard has an older version or not this key yet.
    Pending,
    /// No key plays it, so it is not sent at all.
    Unassigned,
}

impl MacroStatus {
    fn label(self) -> &'static str {
        match self {
            MacroStatus::OnKeyboard => t("✔ On the keyboard", "✔ Sulla tastiera"),
            MacroStatus::Pending => t("⬆ Not sent yet", "⬆ Da inviare"),
            MacroStatus::Unassigned => t("No key", "Non assegnata"),
        }
    }

    fn color(self, ui: &Ui) -> Color32 {
        match self {
            MacroStatus::OnKeyboard => OK,
            MacroStatus::Pending => WARN,
            MacroStatus::Unassigned => ui.visuals().weak_text_color(),
        }
    }
}

/// One line of the macro list.
struct MacroRow {
    name: String,
    keys: Vec<(bool, String)>,
    summary: String,
    status: MacroStatus,
}

enum StepMove {
    Up,
    Down,
    Remove,
}

impl App {
    pub(crate) fn tab_macros(&mut self, ui: &mut Ui) {
        title(
            ui,
            t("Macros", "Macro"),
            t(
                "Keys, text and clicks played by a key. Assign each macro to a key here or in the Keys tab.",
                "Tasti, testo e clic eseguiti da un tasto. Assegna ogni macro a un tasto qui o nella scheda Tasti.",
            ),
        );
        if self.macro_sel.as_ref().is_some_and(|n| !self.macros.contains_key(n)) {
            self.macro_sel = None; // dropped by "Discard changes"
        }
        self.macro_list(ui);
        if let Some(name) = self.macro_sel.clone() {
            ui.add_space(8.0);
            self.macro_editor(ui, &name);
        }
        ui.add_space(10.0);
        self.send_keymap_bar(ui, false);
    }

    /// Keys playing `name`: `(Fn layer, key name)`.
    fn macro_keys(&self, name: &str) -> Vec<(bool, String)> {
        assigned_keys(&self.keymap, name)
    }

    fn macro_status(&self, name: &str) -> MacroStatus {
        let keys = self.macro_keys(name);
        if keys.is_empty() {
            return MacroStatus::Unassigned;
        }
        let empty = Keymap::default();
        let sent = self.state.keymap.as_ref().unwrap_or(&empty);
        let sent_keys = assigned_keys(sent, name);
        if keys.iter().all(|k| sent_keys.contains(k)) && self.state.macros.get(name) == self.macros.get(name) {
            MacroStatus::OnKeyboard
        } else {
            MacroStatus::Pending
        }
    }

    /// Three macros in the three states, for the screenshots (`AK820_GUI_SHOTS`).
    pub(crate) fn demo_macros(&mut self) {
        let def = |steps: &[&str], mode, count| MacroDef {
            steps: steps.iter().map(|s| s.to_string()).collect(),
            mode,
            count,
        };
        let copy = t("copy-paste", "copia-incolla").to_string();
        let sign = t("signature", "firma").to_string();
        let click = t("fast click", "clic rapido").to_string();
        self.macros.insert(copy.clone(), def(&["tap:ctrl+c", "delay:100", "tap@40:ctrl+v"], MacroMode::Once, 1));
        self.macros.insert(sign.clone(), def(&[t("text:Best regards", "text:Cordiali saluti")], MacroMode::Once, 1));
        self.macros.insert(click, def(&["click:left", "delay:50"], MacroMode::Repeat, 500));
        self.keymap.fn_layer.insert("f1".into(), format!("macro:{sign}"));
        self.state.macros = self.macros.clone();
        self.state.keymap = Some(self.keymap.clone());
        self.keymap.fn_layer.insert("c".into(), format!("macro:{copy}"));
        self.select_macro(copy);
    }

    fn select_macro(&mut self, name: String) {
        self.macro_name = name.clone();
        self.macro_sel = Some(name);
        self.macro_error = None;
    }

    fn macro_list(&mut self, ui: &mut Ui) {
        let model = self.model;
        let rows: Vec<MacroRow> = self
            .macros
            .iter()
            .map(|(name, def)| MacroRow {
                name: name.clone(),
                keys: self.macro_keys(name),
                summary: summary(def),
                status: self.macro_status(name),
            })
            .collect();
        let mut pick = None;
        let mut create = false;
        card(ui, t("Your macros", "Le tue macro"), |ui| {
            if rows.is_empty() {
                ui.label(
                    RichText::new(t(
                        "No macros yet. A macro plays keys, text and clicks when you press the key it's assigned to.",
                        "Ancora nessuna macro. Una macro esegue tasti, testo e clic quando premi il tasto a cui è assegnata.",
                    ))
                    .weak(),
                );
                ui.add_space(4.0);
            }
            for MacroRow { name, keys, summary: text, status } in &rows {
                let selected = self.macro_sel.as_deref() == Some(name.as_str());
                let bg = ui.painter().add(egui::Shape::Noop);
                let frame = egui::Frame::new().inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(130.0, 26.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.set_min_width(130.0);
                                match keys.first() {
                                    Some((fn_layer, key)) => {
                                        key_caps(ui, model, *fn_layer, key);
                                        if keys.len() > 1 {
                                            ui.label(RichText::new(format!("+{}", keys.len() - 1)).weak());
                                        }
                                    }
                                    None => {
                                        ui.label(RichText::new(t("no key", "nessun tasto")).weak());
                                    }
                                }
                            },
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new("›").size(18.0).weak());
                            chip(ui, status.label(), status.color(ui));
                            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                                ui.label(RichText::new(name).strong());
                                ui.label(RichText::new(text).weak().small());
                            });
                        });
                    });
                });
                let rect = frame.response.rect;
                let resp = ui.interact(rect, ui.id().with(("macro-row", name)), Sense::click());
                let fill = if selected {
                    mix(ui.visuals().faint_bg_color, accent(ui), 0.16)
                } else if resp.hovered() {
                    ui.visuals().widgets.hovered.weak_bg_fill
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().set(bg, egui::epaint::RectShape::filled(rect, CornerRadius::same(12), fill));
                if resp.clicked() {
                    pick = Some(name.clone());
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                create = ui.button(t("➕ New macro", "➕ Nuova macro")).clicked();
                if let Ok(c) = self.keymap.compile(&self.macros) {
                    if !c.macros.is_empty() {
                        let used = macros::data_len(&c.macros) * 100 / macros::BLOCK_MAX;
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let text = t("Keyboard memory used by macros", "Memoria della tastiera usata dalle macro");
                            ui.label(RichText::new(format!("{text}: {used}%")).weak().small());
                        });
                    }
                }
            });
        });
        if let Some(name) = pick {
            self.select_macro(name);
        }
        if create {
            let name = (1..)
                .map(|i| format!("{} {i}", t("macro", "macro")))
                .find(|n| !self.macros.contains_key(n))
                .expect("there is always a free name");
            let steps = vec![Step::Text { text: t("Hello!", "Ciao!").into(), hold_ms: DEFAULT_HOLD_MS }.to_string()];
            self.macros.insert(name.clone(), MacroDef { steps, mode: MacroMode::Once, count: 1 });
            self.select_macro(name);
        }
    }

    fn macro_editor(&mut self, ui: &mut Ui, name: &str) {
        let model = self.model;
        let Some(original) = self.macros.get(name).cloned() else { return };
        let mut def = original.clone();
        let assigned = self.macro_keys(name);
        let target = format!("macro:{name}");
        let (mut rename, mut delete, mut unassign, mut assign) = (None, false, None, None);
        let mut moved = None;
        card(ui, t("Edit macro", "Modifica macro"), |ui| {
            egui::Grid::new("macro-head").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
                ui.label(t("Name", "Nome"));
                ui.horizontal(|ui| {
                    let edit = egui::TextEdit::singleline(&mut self.macro_name).desired_width(220.0);
                    if ui.add(edit).changed() {
                        rename = Some(self.macro_name.trim().to_string());
                    }
                    if let Some(e) = &self.macro_error {
                        ui.colored_label(ERR, e);
                    }
                });
                ui.end_row();

                ui.label(t("Played by", "Avviata da"));
                ui.horizontal_wrapped(|ui| {
                    if assigned.is_empty() {
                        ui.label(
                            RichText::new(t(
                                "no key yet: assign one so the keyboard receives it",
                                "ancora nessun tasto: assegnane uno perché la tastiera la riceva",
                            ))
                            .weak(),
                        );
                    }
                    for (fn_layer, key) in &assigned {
                        key_caps(ui, model, *fn_layer, key);
                        if ui
                            .small_button("✖")
                            .on_hover_text(t("Remove from this key", "Togli da questo tasto"))
                            .clicked()
                        {
                            unassign = Some((*fn_layer, key.clone()));
                        }
                        ui.add_space(8.0);
                    }
                });
                ui.end_row();

                ui.label("");
                ui.horizontal(|ui| {
                    let (fn_layer, key) = &mut self.macro_assign;
                    if model.has_fn_layer() {
                        let layer_label = |f: bool| if f { "Fn +" } else { t("Normal layer", "Layer normale") };
                        ComboBox::from_id_salt("assign-layer").selected_text(layer_label(*fn_layer)).show_ui(
                            ui,
                            |ui| {
                                ui.selectable_value(fn_layer, false, layer_label(false));
                                ui.selectable_value(fn_layer, true, layer_label(true));
                            },
                        );
                    } else {
                        *fn_layer = false;
                    }
                    let names: Vec<(&str, String)> = layout::layout_for(model)
                        .iter()
                        .filter(|k| keys::check_remappable(k.0, *fn_layer).is_ok())
                        .map(|k| (k.0, key_label(model, k.0)))
                        .collect();
                    let shown = names.iter().find(|n| n.0 == key.as_str()).map(|n| n.1.clone());
                    ComboBox::from_id_salt("assign-key")
                        .selected_text(shown.clone().unwrap_or_else(|| t("choose a key", "scegli un tasto").into()))
                        .height(320.0)
                        .show_ui(ui, |ui| {
                            for (n, l) in &names {
                                ui.selectable_value(key, n.to_string(), l);
                            }
                        });
                    if ui
                        .add_enabled(shown.is_some(), Button::new(t("Assign to this key", "Assegna a questo tasto")))
                        .clicked()
                    {
                        assign = Some((*fn_layer, key.clone()));
                    }
                    let layer = if *fn_layer { &self.keymap.fn_layer } else { &self.keymap.normal };
                    if let Some(old) = layer.get(key.as_str()).filter(|old| **old != target) {
                        ui.label(RichText::new(format!("{} {old}", t("replaces", "sostituisce"))).weak().small());
                    }
                });
                ui.end_row();

                ui.label(t("Mode", "Modalità"));
                ComboBox::from_id_salt("macro-mode").selected_text(macro_mode_label(def.mode)).show_ui(ui, |ui| {
                    for m in [MacroMode::Once, MacroMode::Repeat, MacroMode::Toggle, MacroMode::Hold, MacroMode::Step] {
                        ui.selectable_value(&mut def.mode, m, macro_mode_label(m));
                    }
                });
                ui.end_row();

                if def.mode == MacroMode::Repeat {
                    ui.label(t("Repetitions", "Ripetizioni"));
                    ui.horizontal(|ui| {
                        ui.add(DragValue::new(&mut def.count).range(1..=u16::MAX).speed(1.0));
                        if def.count > REPEAT_MAX && !def.steps.is_empty() {
                            let copies = def.stored().0.len() / def.steps.len();
                            let note = format!(
                                "{} {REPEAT_MAX} {}: {} {copies} {}",
                                t("The keyboard counts up to", "La tastiera conta fino a"),
                                t("times", "volte"),
                                t("the steps are copied", "i passi vengono copiati"),
                                t("times in its memory", "volte nella sua memoria"),
                            );
                            ui.label(RichText::new(note).weak().small());
                        }
                    });
                    ui.end_row();
                }
            });

            ui.add_space(10.0);
            ui.label(RichText::new(t("Steps", "Passi")).strong());
            ui.add_space(2.0);
            let count = def.steps.len();
            for i in 0..count {
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{}", i + 1)).weak().monospace());
                            match def.steps[i].parse::<Step>() {
                                Ok(mut step) => {
                                    if step_editor(ui, i, &mut step) {
                                        def.steps[i] = step.to_string();
                                    }
                                }
                                Err(_) => {
                                    ui.add(egui::TextEdit::singleline(&mut def.steps[i]).desired_width(260.0));
                                }
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("✖").on_hover_text(t("Remove step", "Togli il passo")).clicked() {
                                    moved = Some((i, StepMove::Remove));
                                }
                                if ui.add_enabled(i + 1 < count, Button::new("⬇").small()).clicked() {
                                    moved = Some((i, StepMove::Down));
                                }
                                if ui.add_enabled(i > 0, Button::new("⬆").small()).clicked() {
                                    moved = Some((i, StepMove::Up));
                                }
                            });
                        });
                        if let Some((msg, warning)) = step_problem(model, &def.steps[i]) {
                            ui.colored_label(if warning { WARN } else { ERR }, msg);
                        }
                    });
                ui.add_space(4.0);
            }
            if count == 0 {
                ui.label(RichText::new(t("Add at least one step.", "Aggiungi almeno un passo.")).color(ERR));
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.menu_button(t("➕ Add step", "➕ Aggiungi passo"), |ui| {
                    for (label, step) in new_steps() {
                        if ui.button(label).clicked() {
                            def.steps.push(step.to_string());
                            ui.close_menu();
                        }
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    delete = ui.button(t("🗑 Delete macro", "🗑 Elimina macro")).clicked();
                });
            });
        });

        match moved {
            Some((i, StepMove::Up)) => def.steps.swap(i, i - 1),
            Some((i, StepMove::Down)) => def.steps.swap(i, i + 1),
            Some((i, StepMove::Remove)) => {
                def.steps.remove(i);
            }
            None => {}
        }
        if def != original {
            self.macros.insert(name.to_string(), def);
        }
        if let Some((fn_layer, key)) = unassign {
            let layer = if fn_layer { &mut self.keymap.fn_layer } else { &mut self.keymap.normal };
            layer.remove(&key);
        }
        if let Some((fn_layer, key)) = assign {
            let layer = if fn_layer { &mut self.keymap.fn_layer } else { &mut self.keymap.normal };
            layer.insert(key, target.clone());
        }
        if delete {
            self.macros.remove(name);
            for layer in [&mut self.keymap.normal, &mut self.keymap.fn_layer] {
                layer.retain(|_, t| *t != target);
            }
            self.macro_sel = None;
            return;
        }
        if let Some(new) = rename {
            if new.is_empty() {
                self.macro_error = Some(t("Enter a name", "Scrivi un nome").into());
            } else if new != name && self.macros.contains_key(&new) {
                self.macro_error = Some(t("Another macro has this name", "Un'altra macro ha già questo nome").into());
            } else if new != name {
                if let Some(d) = self.macros.remove(name) {
                    self.macros.insert(new.clone(), d);
                }
                rename_macro_refs(&mut self.keymap, name, &new);
                self.macro_sel = Some(new);
                self.macro_error = None;
            } else {
                self.macro_error = None;
            }
        }
    }
}

fn assigned_keys(keymap: &Keymap, name: &str) -> Vec<(bool, String)> {
    let target = format!("macro:{name}");
    let on = |fn_layer: bool, layer: &std::collections::BTreeMap<String, String>| {
        layer.iter().filter(|(_, t)| **t == target).map(|(k, _)| (fn_layer, k.clone())).collect::<Vec<_>>()
    };
    let mut keys = on(false, &keymap.normal);
    keys.extend(on(true, &keymap.fn_layer));
    keys
}

/// The label printed on a key, or a readable name for keys used in steps.
pub(crate) fn key_label(model: Model, name: &str) -> String {
    match name {
        "ctrl" | "lctrl" | "rctrl" => return "Ctrl".into(),
        "shift" | "lshift" | "rshift" => return "Shift".into(),
        "alt" | "lalt" | "ralt" => return "Alt".into(),
        "win" | "lwin" | "rwin" => return "Win".into(),
        "space" => return t("Space", "Spazio").into(),
        "knob" => return t("Knob", "Manopola").into(),
        _ => {}
    }
    if let Some(k) = layout::layout_for(model).iter().find(|k| k.0 == name && !k.1.is_empty()) {
        return k.1.into();
    }
    let mut chars = name.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// A key drawn as a small keycap.
pub(crate) fn keycap(ui: &mut Ui, label: &str) {
    let color = ui.visuals().text_color();
    let galley = ui.painter().layout_no_wrap(label.to_owned(), FontId::proportional(12.5), color);
    let size = Vec2::new((galley.size().x + 14.0).max(26.0), 24.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let fill = ui.visuals().widgets.inactive.bg_fill;
    let top = rect.with_max_y(rect.bottom() - 2.0);
    ui.painter().rect_filled(rect, CornerRadius::same(6), mix(fill, Color32::BLACK, 0.3));
    ui.painter().rect_filled(top, CornerRadius::same(6), fill);
    ui.painter().galley(top.center() - galley.size() / 2.0, galley, color);
}

/// The keys of a combination, `Fn` first when it is on the Fn layer.
pub(crate) fn key_caps(ui: &mut Ui, model: Model, fn_layer: bool, key: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if fn_layer {
            keycap(ui, "Fn");
            ui.label(RichText::new("+").weak());
        }
        keycap(ui, &key_label(model, key));
    });
}

/// A capsule with a status text in its color.
pub(crate) fn chip(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(12.5), color);
    let size = galley.size() + Vec2::new(18.0, 8.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same((size.y / 2.0) as u8), color.gamma_multiply(0.16));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
}

fn combo_text(keys: &str) -> String {
    keys.split('+')
        .map(|k| match k {
            "ctrl" => "Ctrl".to_string(),
            "shift" => "Shift".into(),
            "alt" => "Alt".into(),
            "win" => "Win".into(),
            k => {
                let mut c = k.chars();
                c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn mouse_label(button: &str) -> &'static str {
    match button {
        "left" => t("left", "sinistro"),
        "right" => t("right", "destro"),
        "middle" => t("middle", "centrale"),
        _ => "?",
    }
}

/// One step in words, lower case: "press Ctrl+C", "wait 100 ms".
fn describe(step: &Step) -> String {
    let held = |ms: u16| if ms == DEFAULT_HOLD_MS { String::new() } else { format!(" {} {ms} ms", t("for", "per")) };
    match step {
        Step::Tap { keys, hold_ms } => format!("{} {}{}", t("press", "premi"), combo_text(keys), held(*hold_ms)),
        Step::Text { text, .. } => format!("{}{text}{}", t("type “", "scrivi «"), t("”", "»")),
        Step::Down(k) => format!("{} {}", t("hold", "tieni premuto"), combo_text(k)),
        Step::Up(k) => format!("{} {}", t("release", "rilascia"), combo_text(k)),
        Step::Delay(ms) => format!("{} {ms} ms", t("wait", "attendi")),
        Step::Click { button, hold_ms } => {
            let click = match crate::i18n::current() {
                crate::i18n::Lang::It => format!("clic {}", mouse_label(button)),
                crate::i18n::Lang::En => format!("{} click", mouse_label(button)),
            };
            format!("{click}{}", held(*hold_ms))
        }
        Step::MouseDown(b) => {
            format!("{} {} {}", t("hold the", "tieni premuto il tasto"), mouse_label(b), t("mouse button", "del mouse"))
        }
        Step::MouseUp(b) => {
            format!("{} {} {}", t("release the", "rilascia il tasto"), mouse_label(b), t("mouse button", "del mouse"))
        }
    }
}

/// What a macro does, as a sentence: its first steps and how it plays.
fn summary(def: &MacroDef) -> String {
    let steps: Vec<Step> = def.steps.iter().filter_map(|s| s.parse().ok()).collect();
    let mut parts: Vec<String> = steps.iter().take(3).map(describe).collect();
    if steps.len() > 3 {
        parts.push(format!("{} {} {}", t("and", "e altri"), steps.len() - 3, t("more steps", "passi")));
    }
    let mut text = parts.join(", ");
    if let Some(first) = text.chars().next() {
        text = first.to_uppercase().chain(text.chars().skip(1)).collect();
    }
    let how = match def.mode {
        MacroMode::Once => t("Once per press.", "Una volta per pressione.").to_string(),
        MacroMode::Repeat => {
            format!("{} {} {}", t("Repeated", "Ripetuta"), def.count, t("times per press.", "volte per pressione."))
        }
        MacroMode::Toggle => t("Starts and stops with each press.", "Parte e si ferma a ogni pressione.").into(),
        MacroMode::Hold => t("Repeats while the key is held.", "Si ripete finché tieni premuto il tasto.").into(),
        MacroMode::Step => t("One event per press.", "Un evento per pressione.").into(),
    };
    if text.is_empty() {
        how
    } else {
        format!("{text}. {how}")
    }
}

/// The steps offered by "Add step".
fn new_steps() -> Vec<(&'static str, Step)> {
    let hold_ms = DEFAULT_HOLD_MS;
    vec![
        (t("Press keys", "Premi una combinazione"), Step::Tap { keys: "ctrl+c".into(), hold_ms }),
        (t("Type text", "Scrivi un testo"), Step::Text { text: t("Hello!", "Ciao!").into(), hold_ms }),
        (t("Wait", "Attendi"), Step::Delay(100)),
        (t("Mouse click", "Clic del mouse"), Step::Click { button: "left".into(), hold_ms }),
        (t("Hold a key down", "Tieni premuto un tasto"), Step::Down("shift".into())),
        (t("Release a key", "Rilascia un tasto"), Step::Up("shift".into())),
        (t("Hold a mouse button", "Tieni premuto un tasto del mouse"), Step::MouseDown("left".into())),
        (t("Release a mouse button", "Rilascia un tasto del mouse"), Step::MouseUp("left".into())),
    ]
}

/// How long a key or button stays pressed, in ms.
fn hold_value(ui: &mut Ui, ms: &mut u16, prefix: &str) -> bool {
    ui.add(DragValue::new(ms).range(1..=u16::MAX).prefix(prefix).suffix(" ms"))
        .on_hover_text(t("How long it stays pressed", "Quanto resta premuto"))
        .changed()
}

/// The controls of one step; `true` when it changed.
fn step_editor(ui: &mut Ui, i: usize, step: &mut Step) -> bool {
    let mut changed = false;
    match step {
        Step::Tap { keys, hold_ms } => {
            ui.label(RichText::new("⌨").color(accent(ui)));
            ui.label(t("Press", "Premi"));
            changed |= combo_editor(ui, i, keys);
            changed |= hold_value(ui, hold_ms, t("held ", "premuto "));
        }
        Step::Text { text, hold_ms } => {
            ui.label(RichText::new("⌨").color(accent(ui)));
            ui.label(t("Type", "Scrivi"));
            let edit = egui::TextEdit::singleline(text)
                .desired_width(220.0)
                .hint_text(t("Text (US layout)", "Testo (layout USA)"));
            changed |= ui.add(edit).changed();
            changed |= hold_value(ui, hold_ms, t("each key ", "ogni tasto "));
        }
        Step::Down(key) => changed |= key_step(ui, i, t("Hold down", "Tieni premuto"), key),
        Step::Up(key) => changed |= key_step(ui, i, t("Release", "Rilascia"), key),
        Step::Delay(ms) => {
            ui.label(RichText::new("🕒").weak());
            ui.label(t("Wait", "Attendi"));
            changed |= ui.add(DragValue::new(ms).range(0..=u16::MAX).suffix(" ms")).changed();
        }
        Step::Click { button, hold_ms } => {
            ui.label(RichText::new("🖱").color(accent(ui)));
            ui.label(t("Click", "Clic"));
            changed |= mouse_combo(ui, i, button);
            changed |= hold_value(ui, hold_ms, t("held ", "premuto "));
        }
        Step::MouseDown(button) => {
            changed |= mouse_step(ui, i, t("Hold mouse button", "Tieni premuto il tasto del mouse"), button);
        }
        Step::MouseUp(button) => {
            changed |= mouse_step(ui, i, t("Release mouse button", "Rilascia il tasto del mouse"), button);
        }
    }
    changed
}

fn key_step(ui: &mut Ui, i: usize, verb: &str, key: &mut String) -> bool {
    ui.label(RichText::new("⌨").color(accent(ui)));
    ui.label(verb);
    let names: Vec<&str> = MODS.iter().chain(KEY_NAMES).copied().collect();
    let before = key.clone();
    name_combo(ui, &format!("step-key{i}"), key, &names, false);
    *key != before
}

fn mouse_step(ui: &mut Ui, i: usize, verb: &str, button: &mut String) -> bool {
    ui.label(RichText::new("🖱").color(accent(ui)));
    ui.label(verb);
    mouse_combo(ui, i, button)
}

/// Modifier switches and a key, kept in the `ctrl+shift+c` form.
fn combo_editor(ui: &mut Ui, i: usize, keys: &mut String) -> bool {
    let mut mods = [false; 4];
    let mut rest = Vec::new();
    for part in keys.split('+').filter(|p| !p.is_empty()) {
        match MODS.iter().position(|m| *m == part) {
            Some(m) => mods[m] = true,
            None => rest.push(part),
        }
    }
    let mut key = rest.join("+");
    let mut changed = false;
    for (m, label) in ["Ctrl", "Shift", "Alt", "Win"].iter().enumerate() {
        changed |= ui.toggle_value(&mut mods[m], *label).changed();
    }
    ui.label("+");
    let before = key.clone();
    name_combo(ui, &format!("step-combo{i}"), &mut key, KEY_NAMES, true);
    changed |= key != before;
    if changed {
        let mut parts: Vec<&str> = MODS.iter().zip(mods).filter(|(_, on)| *on).map(|(m, _)| *m).collect();
        if !key.is_empty() {
            parts.push(&key);
        }
        *keys = parts.join("+");
    }
    changed
}

fn mouse_combo(ui: &mut Ui, i: usize, button: &mut String) -> bool {
    let before = button.clone();
    ComboBox::from_id_salt(("mouse-button", i)).selected_text(mouse_label(button)).show_ui(ui, |ui| {
        for b in ["left", "right", "middle"] {
            ui.selectable_value(button, b.to_string(), mouse_label(b));
        }
    });
    *button != before
}

/// Why a step can't be sent (error) or won't play as expected (warning).
fn step_problem(model: Model, step: &str) -> Option<(String, bool)> {
    let parsed = step.parse::<Step>();
    if let Ok(Step::Text { text, .. }) = &parsed {
        if text.is_empty() {
            return Some((t("Write the text to type", "Scrivi il testo da digitare").into(), false));
        }
    }
    if let Err(e) = macros::events(&[step.to_string()]) {
        return Some((e.to_string(), false));
    }
    let mouse = matches!(parsed, Ok(Step::Click { .. } | Step::MouseDown(_) | Step::MouseUp(_)));
    if mouse && model == Model::Ak820 {
        let msg = t(
            "This app can't send mouse clicks in AK820 macros",
            "Con la AK820 questa app non invia i clic del mouse nelle macro",
        );
        return Some((msg.into(), true));
    }
    None
}
