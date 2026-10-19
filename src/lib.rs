//! Configuration library for the AJAZZ AK820 Pro keyboard (stock firmware),
//! working on Linux and Windows through hidapi.
//!
//! The protocol was consolidated from several community projects; see
//! `docs/PROTOCOL.md` and the README for credits.

pub mod device;
pub mod keys;
pub mod lighting;
pub mod macros;
pub mod profile;
pub mod protocol;
pub mod screen;

use std::fmt;
use std::str::FromStr;

pub use device::{Connection, Keyboard};
pub use lighting::{Direction, Lighting, Mode};
pub use profile::Profile;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HID error: {0}")]
    Hid(#[from] hidapi::HidError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("{0}")]
    NotFound(String),
    #[error("unknown key '{0}' (see `ak820 keys`)")]
    UnknownKey(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Transfer(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// An RGB color, written as `RRGGBB` (optionally with `#`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn bytes(self) -> [u8; 3] {
        [self.0, self.1, self.2]
    }
}

impl FromStr for Rgb {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let h = s.trim().trim_start_matches('#');
        let named = match h.to_ascii_lowercase().as_str() {
            "black" | "off" => Some(Rgb(0, 0, 0)),
            "white" => Some(Rgb(255, 255, 255)),
            "red" => Some(Rgb(255, 0, 0)),
            "green" => Some(Rgb(0, 255, 0)),
            "blue" => Some(Rgb(0, 0, 255)),
            "yellow" => Some(Rgb(255, 255, 0)),
            "cyan" => Some(Rgb(0, 255, 255)),
            "magenta" => Some(Rgb(255, 0, 255)),
            "orange" => Some(Rgb(255, 96, 0)),
            "purple" => Some(Rgb(128, 0, 255)),
            _ => None,
        };
        if let Some(c) = named {
            return Ok(c);
        }
        let bad = || Error::Invalid(format!("'{s}' is not a color (use RRGGBB or a name like red)"));
        if h.len() != 6 || !h.is_ascii() {
            return Err(bad());
        }
        let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| bad());
        Ok(Rgb(c(0)?, c(2)?, c(4)?))
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

impl serde::Serialize for Rgb {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Rgb {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!("ff8000".parse::<Rgb>().unwrap(), Rgb(255, 128, 0));
        assert_eq!("#00FF00".parse::<Rgb>().unwrap(), Rgb(0, 255, 0));
        assert_eq!("red".parse::<Rgb>().unwrap(), Rgb(255, 0, 0));
        assert!("12345".parse::<Rgb>().is_err());
        assert_eq!(Rgb(1, 2, 255).to_string(), "0102ff");
    }
}
