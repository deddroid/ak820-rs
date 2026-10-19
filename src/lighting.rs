//! Built-in lighting effects.

use serde::{Deserialize, Serialize};

use crate::protocol::{Packet, PACKET};
use crate::Rgb;

pub const MAX_LEVEL: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    Off,
    /// The firmware has no real static effect: it is breath at speed 0.
    Static,
    SingleOn,
    SingleOff,
    Glittering,
    Falling,
    Colourful,
    Breath,
    Spectrum,
    Outward,
    Scrolling,
    Rolling,
    Rotating,
    Explode,
    Launch,
    Ripples,
    Flowing,
    Pulsating,
    Tilt,
    Shuttle,
    /// Per-key colors uploaded with `Keyboard::set_key_colors`.
    Custom,
}

impl Mode {
    pub const ALL: &'static [Mode] = &[
        Mode::Off,
        Mode::Static,
        Mode::SingleOn,
        Mode::SingleOff,
        Mode::Glittering,
        Mode::Falling,
        Mode::Colourful,
        Mode::Breath,
        Mode::Spectrum,
        Mode::Outward,
        Mode::Scrolling,
        Mode::Rolling,
        Mode::Rotating,
        Mode::Explode,
        Mode::Launch,
        Mode::Ripples,
        Mode::Flowing,
        Mode::Pulsating,
        Mode::Tilt,
        Mode::Shuttle,
        Mode::Custom,
    ];

    pub fn code(self) -> u8 {
        match self {
            Mode::Off => 0x00,
            Mode::Static | Mode::Breath => 0x07,
            Mode::SingleOn => 0x02,
            Mode::SingleOff => 0x03,
            Mode::Glittering => 0x04,
            Mode::Falling => 0x05,
            Mode::Colourful => 0x06,
            Mode::Spectrum => 0x08,
            Mode::Outward => 0x09,
            Mode::Scrolling => 0x0A,
            Mode::Rolling => 0x0B,
            Mode::Rotating => 0x0C,
            Mode::Explode => 0x0D,
            Mode::Launch => 0x0E,
            Mode::Ripples => 0x0F,
            Mode::Flowing => 0x10,
            Mode::Pulsating => 0x11,
            Mode::Tilt => 0x12,
            Mode::Shuttle => 0x13,
            Mode::Custom => 0x80,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Static => "static",
            Mode::SingleOn => "single-on",
            Mode::SingleOff => "single-off",
            Mode::Glittering => "glittering",
            Mode::Falling => "falling",
            Mode::Colourful => "colourful",
            Mode::Breath => "breath",
            Mode::Spectrum => "spectrum",
            Mode::Outward => "outward",
            Mode::Scrolling => "scrolling",
            Mode::Rolling => "rolling",
            Mode::Rotating => "rotating",
            Mode::Explode => "explode",
            Mode::Launch => "launch",
            Mode::Ripples => "ripples",
            Mode::Flowing => "flowing",
            Mode::Pulsating => "pulsating",
            Mode::Tilt => "tilt",
            Mode::Shuttle => "shuttle",
            Mode::Custom => "custom",
        }
    }

    /// Which way the effect can move, if it moves at all.
    ///
    /// The firmware only has two directions per effect: byte 0 (or 3) and
    /// byte 1 (or 2). Verified on hardware for scrolling and rolling.
    pub fn direction_axis(self) -> Option<Axis> {
        match self {
            Mode::Scrolling => Some(Axis::Vertical),
            Mode::Rolling | Mode::Rotating | Mode::Flowing | Mode::Tilt => Some(Axis::Horizontal),
            _ => None,
        }
    }

    pub fn from_name(s: &str) -> Option<Mode> {
        let s = s.to_ascii_lowercase().replace('_', "-");
        let s = match s.as_str() {
            "colorful" => "colourful",
            "wave" => "rolling",
            "rain" => "falling",
            "per-key" => "custom",
            other => other,
        };
        Mode::ALL.iter().copied().find(|m| m.name() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// left / right
    Horizontal,
    /// up / down
    Vertical,
}

impl Axis {
    /// The two directions available on this axis; the first is the firmware default (byte 0).
    pub fn directions(self) -> [Direction; 2] {
        match self {
            Axis::Horizontal => [Direction::Right, Direction::Left],
            Axis::Vertical => [Direction::Down, Direction::Up],
        }
    }
}

/// Direction of movement of the light. Byte 0 moves right (horizontal
/// effects) or down (scrolling); byte 1 moves left or up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Left,
    Down,
    Up,
    #[default]
    Right,
}

impl Direction {
    /// Wire value for this direction on `mode`.
    pub fn code(self, mode: Mode) -> u8 {
        match (mode.direction_axis(), self) {
            (Some(Axis::Horizontal), Direction::Left) | (Some(Axis::Vertical), Direction::Up) => 1,
            _ => 0,
        }
    }

    /// The same direction if it fits `axis`, otherwise the axis default.
    pub fn on_axis(self, axis: Axis) -> Direction {
        let dirs = axis.directions();
        if dirs.contains(&self) {
            self
        } else {
            dirs[0]
        }
    }

    pub fn from_name(s: &str) -> Option<Direction> {
        match s.to_ascii_lowercase().as_str() {
            "left" | "l" => Some(Direction::Left),
            "down" | "d" => Some(Direction::Down),
            "up" | "u" => Some(Direction::Up),
            "right" | "r" => Some(Direction::Right),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lighting {
    pub mode: Mode,
    #[serde(default = "white")]
    pub color: Rgb,
    /// Cycle through colors instead of using `color`.
    #[serde(default)]
    pub rainbow: bool,
    #[serde(default = "max_level")]
    pub brightness: u8,
    #[serde(default = "mid_level")]
    pub speed: u8,
    #[serde(default)]
    pub direction: Direction,
}

fn white() -> Rgb {
    Rgb(255, 255, 255)
}
fn max_level() -> u8 {
    MAX_LEVEL
}
fn mid_level() -> u8 {
    3
}

impl Lighting {
    pub fn new(mode: Mode) -> Self {
        Lighting {
            mode,
            color: white(),
            rainbow: false,
            brightness: MAX_LEVEL,
            speed: 3,
            direction: Direction::Right,
        }
    }

    /// Factory effect: rainbow wave, brightness 5, speed 3.
    pub fn factory() -> Self {
        Lighting { rainbow: true, ..Lighting::new(Mode::Rolling) }
    }

    pub fn packet(&self) -> Packet {
        let mut p = [0u8; PACKET];
        p[0] = self.mode.code();
        if self.mode != Mode::Off {
            p[1..4].copy_from_slice(&[self.color.0, self.color.1, self.color.2]);
            p[8] = self.rainbow as u8;
            p[9] = self.brightness.min(MAX_LEVEL);
            p[10] = if self.mode == Mode::Static { 0 } else { self.speed.min(MAX_LEVEL) };
            p[11] = self.direction.code(self.mode);
        }
        p[14] = 0xAA;
        p[15] = 0x55;
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_layout() {
        let l = Lighting {
            mode: Mode::Rolling,
            color: Rgb(0x11, 0x22, 0x33),
            rainbow: false,
            brightness: 5,
            speed: 3,
            direction: Direction::Left,
        };
        let p = l.packet();
        assert_eq!(&p[..4], &[0x0B, 0x11, 0x22, 0x33]);
        assert_eq!(&p[8..12], &[0, 5, 3, 1]);
        assert_eq!(&p[14..16], &[0xAA, 0x55]);
    }

    #[test]
    fn static_is_breath_at_speed_zero() {
        let p = Lighting::new(Mode::Static).packet();
        assert_eq!((p[0], p[10]), (0x07, 0));
    }

    #[test]
    fn two_directions_per_axis() {
        // Verified on hardware: 0 = rightward / downward, 1 = leftward / upward.
        assert_eq!(Direction::Right.code(Mode::Rolling), 0);
        assert_eq!(Direction::Left.code(Mode::Rolling), 1);
        assert_eq!(Direction::Down.code(Mode::Scrolling), 0);
        assert_eq!(Direction::Up.code(Mode::Scrolling), 1);
        assert_eq!(Direction::Up.code(Mode::Rolling), 0);
        assert_eq!(Direction::Left.code(Mode::Breath), 0);
        assert_eq!(Direction::Left.on_axis(Axis::Vertical), Direction::Down);
    }

    #[test]
    fn off_is_empty() {
        let p = Lighting::new(Mode::Off).packet();
        assert!(p[..14].iter().all(|&b| b == 0));
    }

    #[test]
    fn names_roundtrip() {
        for m in Mode::ALL {
            assert_eq!(Mode::from_name(m.name()), Some(*m));
        }
        assert_eq!(Mode::from_name("wave"), Some(Mode::Rolling));
    }
}
