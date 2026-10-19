//! Physical key layout of the AK820 Pro (ANSI 75%), drawn by the GUI.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

/// `(key name, label, x, y, width)` in key units.
pub const LAYOUT: &[(&str, &str, f32, f32, f32)] = &[
    ("esc", "Esc", 0.0, 0.0, 1.0),
    ("f1", "F1", 1.25, 0.0, 1.0),
    ("f2", "F2", 2.25, 0.0, 1.0),
    ("f3", "F3", 3.25, 0.0, 1.0),
    ("f4", "F4", 4.25, 0.0, 1.0),
    ("f5", "F5", 5.5, 0.0, 1.0),
    ("f6", "F6", 6.5, 0.0, 1.0),
    ("f7", "F7", 7.5, 0.0, 1.0),
    ("f8", "F8", 8.5, 0.0, 1.0),
    ("f9", "F9", 9.75, 0.0, 1.0),
    ("f10", "F10", 10.75, 0.0, 1.0),
    ("f11", "F11", 11.75, 0.0, 1.0),
    ("f12", "F12", 12.75, 0.0, 1.0),
    ("delete", "Del", 15.0, 0.0, 1.0),
    ("grave", "`", 0.0, 1.25, 1.0),
    ("1", "1", 1.0, 1.25, 1.0),
    ("2", "2", 2.0, 1.25, 1.0),
    ("3", "3", 3.0, 1.25, 1.0),
    ("4", "4", 4.0, 1.25, 1.0),
    ("5", "5", 5.0, 1.25, 1.0),
    ("6", "6", 6.0, 1.25, 1.0),
    ("7", "7", 7.0, 1.25, 1.0),
    ("8", "8", 8.0, 1.25, 1.0),
    ("9", "9", 9.0, 1.25, 1.0),
    ("0", "0", 10.0, 1.25, 1.0),
    ("minus", "-", 11.0, 1.25, 1.0),
    ("equal", "=", 12.0, 1.25, 1.0),
    ("backspace", "Backspace", 13.0, 1.25, 2.0),
    ("home", "Home", 15.0, 1.25, 1.0),
    ("tab", "Tab", 0.0, 2.25, 1.5),
    ("q", "Q", 1.5, 2.25, 1.0),
    ("w", "W", 2.5, 2.25, 1.0),
    ("e", "E", 3.5, 2.25, 1.0),
    ("r", "R", 4.5, 2.25, 1.0),
    ("t", "T", 5.5, 2.25, 1.0),
    ("y", "Y", 6.5, 2.25, 1.0),
    ("u", "U", 7.5, 2.25, 1.0),
    ("i", "I", 8.5, 2.25, 1.0),
    ("o", "O", 9.5, 2.25, 1.0),
    ("p", "P", 10.5, 2.25, 1.0),
    ("lbracket", "[", 11.5, 2.25, 1.0),
    ("rbracket", "]", 12.5, 2.25, 1.0),
    ("backslash", "\\", 13.5, 2.25, 1.5),
    ("pgup", "PgUp", 15.0, 2.25, 1.0),
    ("capslock", "Caps", 0.0, 3.25, 1.75),
    ("a", "A", 1.75, 3.25, 1.0),
    ("s", "S", 2.75, 3.25, 1.0),
    ("d", "D", 3.75, 3.25, 1.0),
    ("f", "F", 4.75, 3.25, 1.0),
    ("g", "G", 5.75, 3.25, 1.0),
    ("h", "H", 6.75, 3.25, 1.0),
    ("j", "J", 7.75, 3.25, 1.0),
    ("k", "K", 8.75, 3.25, 1.0),
    ("l", "L", 9.75, 3.25, 1.0),
    ("semicolon", ";", 10.75, 3.25, 1.0),
    ("quote", "'", 11.75, 3.25, 1.0),
    ("enter", "Enter", 12.75, 3.25, 2.25),
    ("pgdn", "PgDn", 15.0, 3.25, 1.0),
    ("lshift", "Shift", 0.0, 4.25, 2.25),
    ("z", "Z", 2.25, 4.25, 1.0),
    ("x", "X", 3.25, 4.25, 1.0),
    ("c", "C", 4.25, 4.25, 1.0),
    ("v", "V", 5.25, 4.25, 1.0),
    ("b", "B", 6.25, 4.25, 1.0),
    ("n", "N", 7.25, 4.25, 1.0),
    ("m", "M", 8.25, 4.25, 1.0),
    ("comma", ",", 9.25, 4.25, 1.0),
    ("period", ".", 10.25, 4.25, 1.0),
    ("slash", "/", 11.25, 4.25, 1.0),
    ("rshift", "Shift", 12.25, 4.25, 1.75),
    ("up", "↑", 14.0, 4.25, 1.0),
    ("lctrl", "Ctrl", 0.0, 5.25, 1.25),
    ("win", "Win", 1.25, 5.25, 1.25),
    ("lalt", "Alt", 2.5, 5.25, 1.25),
    ("space", "", 3.75, 5.25, 6.25),
    ("ralt", "Alt", 10.0, 5.25, 1.0),
    ("fn", "Fn", 11.0, 5.25, 1.0),
    ("rctrl", "Ctrl", 12.0, 5.25, 1.0),
    ("left", "←", 13.0, 5.25, 1.0),
    ("down", "↓", 14.0, 5.25, 1.0),
    ("right", "→", 15.0, 5.25, 1.0),
];

const WIDTH_UNITS: f32 = 16.0;
const HEIGHT_UNITS: f32 = 6.25;

/// How one key is drawn.
pub struct KeyLook {
    pub fill: Color32,
    /// Secondary text under the label (e.g. a remap target).
    pub sub: Option<String>,
    pub dim: bool,
    pub selected: bool,
}

pub struct KeyClick {
    pub key: &'static str,
    pub secondary: bool,
}

/// Draw the keyboard filling the available width; returns the clicked key.
pub fn keyboard(ui: &mut egui::Ui, look: impl Fn(&str) -> KeyLook) -> Option<KeyClick> {
    let width = ui.available_width().min(980.0);
    let unit = width / WIDTH_UNITS;
    let pad = unit * 0.06;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, unit * HEIGHT_UNITS), Sense::click());
    let painter = ui.painter_at(rect.expand(2.0));
    let visuals = ui.visuals();
    painter.rect_filled(rect.expand(unit * 0.25), CornerRadius::same(12), visuals.extreme_bg_color);

    let hover = resp.hover_pos();
    let mut clicked = None;
    for &(name, label, x, y, w) in LAYOUT {
        let key_rect = Rect::from_min_size(
            Pos2::new(rect.left() + x * unit + pad, rect.top() + y * unit + pad),
            Vec2::new(w * unit - 2.0 * pad, unit - 2.0 * pad),
        );
        let l = look(name);
        let hovered = hover.is_some_and(|p| key_rect.contains(p)) && !l.dim;
        let mut fill = l.fill;
        if l.dim {
            fill = fill.gamma_multiply(0.35);
        }
        painter.rect_filled(key_rect, CornerRadius::same(5), fill);
        let stroke = if l.selected {
            Stroke::new(2.5_f32, visuals.selection.stroke.color)
        } else if hovered {
            Stroke::new(1.5_f32, visuals.widgets.hovered.fg_stroke.color)
        } else {
            Stroke::new(1.0_f32, visuals.widgets.noninteractive.bg_stroke.color)
        };
        painter.rect_stroke(key_rect, CornerRadius::same(5), stroke, StrokeKind::Inside);

        let text_color = contrast(fill);
        let label_pos = if l.sub.is_some() {
            key_rect.center() - Vec2::new(0.0, unit * 0.15)
        } else {
            key_rect.center()
        };
        let size = if label.chars().count() > 3 { unit * 0.2 } else { unit * 0.27 };
        painter.text(label_pos, Align2::CENTER_CENTER, label, FontId::proportional(size), text_color);
        if let Some(sub) = &l.sub {
            let max = ((w * 7.0) as usize).max(4);
            let short: String = if sub.chars().count() > max {
                sub.chars().take(max - 1).chain(['…']).collect()
            } else {
                sub.clone()
            };
            painter.text(
                key_rect.center() + Vec2::new(0.0, unit * 0.2),
                Align2::CENTER_CENTER,
                short,
                FontId::proportional(unit * 0.17),
                text_color.gamma_multiply(0.85),
            );
        }
        if hovered && (resp.clicked() || resp.secondary_clicked()) {
            clicked = Some(KeyClick { key: name, secondary: resp.secondary_clicked() });
        }
    }
    clicked
}

/// Black or white text, whichever reads better on `bg`.
pub fn contrast(bg: Color32) -> Color32 {
    let l = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if l > 150.0 {
        Color32::from_gray(20)
    } else {
        Color32::from_gray(235)
    }
}
