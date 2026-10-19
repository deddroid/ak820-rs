//! 128×128 TFT screen content: still images and animated GIFs.
//!
//! Upload blob: 256-byte header (`[0]` = frame count, `[1+i]` = frame *i*
//! duration in 2 ms units, min 1, rest `FF`) followed by RGB565 little-endian
//! frames, padded with `FF` to whole 4096-byte blocks.

use std::path::Path;

use image::{imageops, AnimationDecoder, DynamicImage, RgbImage, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::protocol::SCREEN_BLOCK;
use crate::Error;

pub const SIZE: u32 = 128;
pub const MAX_FRAMES: usize = 140;
const HEADER: usize = 256;
pub const FRAME_BYTES: usize = (SIZE * SIZE * 2) as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Fill the screen, cropping the overflow.
    #[default]
    Crop,
    /// Show the whole image, with black bars.
    Pad,
    /// Stretch to 128×128.
    Stretch,
}

pub struct Frame {
    pub rgb565: Vec<u8>,
    pub duration_ms: u32,
}

pub fn rgb565(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::with_capacity(img.len() / 3 * 2);
    for p in img.pixels() {
        let [r, g, b] = p.0;
        let v = ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | (b as u16 >> 3);
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Which part of the image to show in [`Fit::Crop`] mode: a square window of
/// side `min(w, h) / zoom`, centered at (`x`, `y`) in 0..1 image coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crop {
    #[serde(default = "one")]
    pub zoom: f32,
    #[serde(default = "half")]
    pub x: f32,
    #[serde(default = "half")]
    pub y: f32,
}

fn one() -> f32 {
    1.0
}
fn half() -> f32 {
    0.5
}

impl Default for Crop {
    fn default() -> Self {
        Crop { zoom: 1.0, x: 0.5, y: 0.5 }
    }
}

pub const MAX_ZOOM: f32 = 8.0;

impl Crop {
    pub fn is_default(&self) -> bool {
        *self == Crop::default()
    }

    /// The source square `(x0, y0, side)` in pixels for a `w`×`h` image, kept inside it.
    pub fn rect(&self, w: u32, h: u32) -> (f32, f32, f32) {
        let (w, h) = (w as f32, h as f32);
        let side = (w.min(h) / self.zoom.clamp(1.0, MAX_ZOOM)).max(1.0);
        let cx = (self.x * w).clamp(side / 2.0, w - side / 2.0);
        let cy = (self.y * h).clamp(side / 2.0, h - side / 2.0);
        (cx - side / 2.0, cy - side / 2.0, side)
    }

    /// The same window with its center moved back inside the image.
    pub fn clamped(self, w: u32, h: u32) -> Crop {
        let (x0, y0, side) = self.rect(w, h);
        Crop {
            zoom: self.zoom.clamp(1.0, MAX_ZOOM),
            x: (x0 + side / 2.0) / w as f32,
            y: (y0 + side / 2.0) / h as f32,
        }
    }
}

pub fn fit_image(img: &RgbaImage, fit: Fit, crop: Crop) -> RgbImage {
    let filter = imageops::FilterType::Lanczos3;
    let dynamic = DynamicImage::ImageRgba8(img.clone());
    let rgba = match fit {
        Fit::Crop => {
            let (w, h) = img.dimensions();
            let (x0, y0, side) = crop.rect(w, h);
            let (x0, y0) = (x0.round() as u32, y0.round() as u32);
            let side = (side.round() as u32).max(1).min(w - x0).min(h - y0);
            let part = imageops::crop_imm(img, x0, y0, side, side).to_image();
            imageops::resize(&part, SIZE, SIZE, filter)
        }
        Fit::Stretch => dynamic.resize_exact(SIZE, SIZE, filter).to_rgba8(),
        Fit::Pad => {
            let small = dynamic.resize(SIZE, SIZE, filter).to_rgba8();
            let mut canvas = RgbaImage::from_pixel(SIZE, SIZE, image::Rgba([0, 0, 0, 255]));
            let x = (SIZE - small.width()) / 2;
            let y = (SIZE - small.height()) / 2;
            imageops::overlay(&mut canvas, &small, x as i64, y as i64);
            canvas
        }
    };
    // Flatten transparency onto black.
    let mut out = RgbImage::new(SIZE, SIZE);
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        let a = p.0[3] as u16;
        o.0 = [0, 1, 2].map(|i| (p.0[i] as u16 * a / 255) as u8);
    }
    out
}

/// A decoded source frame, before fitting to the screen.
pub struct SourceFrame {
    pub image: RgbaImage,
    /// 0 for a still image.
    pub duration_ms: u32,
}

/// Decode a still image or an animated GIF (at most [`MAX_FRAMES`] frames).
pub fn load_source(path: &Path) -> Result<Vec<SourceFrame>, Error> {
    let is_gif = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif"));
    if is_gif {
        let file = std::io::BufReader::new(std::fs::File::open(path)?);
        let decoder = image::codecs::gif::GifDecoder::new(file)?;
        let frames = decoder.into_frames().collect_frames()?;
        if frames.len() > 1 {
            return Ok(frames
                .into_iter()
                .take(MAX_FRAMES)
                .map(|f| {
                    let (num, den) = f.delay().numer_denom_ms();
                    let ms = num.checked_div(den).unwrap_or(100);
                    SourceFrame {
                        // The official driver only allows 30..500 ms per frame.
                        duration_ms: if ms == 0 { 100 } else { ms.clamp(30, 500) },
                        image: f.into_buffer(),
                    }
                })
                .collect());
        }
    }
    Ok(vec![SourceFrame { image: image::open(path)?.to_rgba8(), duration_ms: 0 }])
}

/// Load a still image or an animated GIF as screen frames.
pub fn load(path: &Path, fit: Fit, crop: Crop) -> Result<Vec<Frame>, Error> {
    Ok(load_source(path)?
        .iter()
        .map(|f| Frame { rgb565: rgb565(&fit_image(&f.image, fit, crop)), duration_ms: f.duration_ms })
        .collect())
}

/// Number of 4096-byte blocks for `frames` frames.
pub fn block_count(frames: usize) -> usize {
    (HEADER + frames.min(MAX_FRAMES) * FRAME_BYTES).div_ceil(SCREEN_BLOCK)
}

/// Header + frames, split into 4096-byte blocks.
pub fn blocks(frames: &[Frame]) -> Vec<Vec<u8>> {
    let frames = &frames[..frames.len().min(MAX_FRAMES)];
    let mut data = vec![0xFFu8; HEADER];
    data[0] = frames.len() as u8;
    for (i, f) in frames.iter().enumerate() {
        data[1 + i] = (f.duration_ms / 2).clamp(1, 255) as u8;
    }
    for f in frames {
        data.extend_from_slice(&f.rgb565);
    }
    let n = data.len().div_ceil(SCREEN_BLOCK);
    data.resize(n * SCREEN_BLOCK, 0xFF);
    data.chunks(SCREEN_BLOCK).map(<[u8]>::to_vec).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb565_little_endian() {
        let red = RgbImage::from_pixel(1, 1, image::Rgb([255, 0, 0]));
        assert_eq!(rgb565(&red), vec![0x00, 0xF8]);
        let blue = RgbImage::from_pixel(1, 1, image::Rgb([0, 0, 255]));
        assert_eq!(rgb565(&blue), vec![0x1F, 0x00]);
    }

    #[test]
    fn blob_layout() {
        let f = || Frame { rgb565: vec![0; FRAME_BYTES], duration_ms: 100 };
        let b = blocks(&[f(), f()]);
        assert_eq!(b.len(), 17); // (256 + 2*32768) / 4096 rounded up
        assert_eq!(&b[0][..4], &[2, 50, 50, 0xFF]);
        assert_eq!(*b.last().unwrap().last().unwrap(), 0xFF);
        let still = blocks(&[Frame { rgb565: vec![0; FRAME_BYTES], duration_ms: 0 }]);
        assert_eq!(still.len(), 9);
        assert_eq!(&still[0][..2], &[1, 1]);
    }

    #[test]
    fn crop_window() {
        // 200×100 image, default crop: centered 100×100 square.
        assert_eq!(Crop::default().rect(200, 100), (50.0, 0.0, 100.0));
        // Zoom 2 at the top-left corner is pushed back inside the image.
        let c = Crop { zoom: 2.0, x: 0.0, y: 0.0 };
        assert_eq!(c.rect(200, 100), (0.0, 0.0, 50.0));
        assert_eq!(c.clamped(200, 100), Crop { zoom: 2.0, x: 0.125, y: 0.25 });
        // The right half of a red|blue image ends up blue.
        let mut img = RgbaImage::from_pixel(256, 128, image::Rgba([255, 0, 0, 255]));
        for x in 128..256 {
            for y in 0..128 {
                img.put_pixel(x, y, image::Rgba([0, 0, 255, 255]));
            }
        }
        let right = fit_image(&img, Fit::Crop, Crop { zoom: 1.0, x: 1.0, y: 0.5 });
        assert_eq!(right.get_pixel(64, 64).0, [0, 0, 255]);
        assert_eq!(block_count(1), 9);
        assert_eq!(block_count(2), 17);
    }

    #[test]
    fn pad_keeps_aspect() {
        let wide = RgbaImage::from_pixel(256, 128, image::Rgba([255, 255, 255, 255]));
        let out = fit_image(&wide, Fit::Pad, Crop::default());
        assert_eq!(out.get_pixel(64, 0).0, [0, 0, 0]);
        assert_eq!(out.get_pixel(64, 64).0, [255, 255, 255]);
    }
}
