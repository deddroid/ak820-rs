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

pub fn fit_image(img: &RgbaImage, fit: Fit) -> RgbImage {
    let filter = imageops::FilterType::Lanczos3;
    let dynamic = DynamicImage::ImageRgba8(img.clone());
    let rgba = match fit {
        Fit::Crop => dynamic.resize_to_fill(SIZE, SIZE, filter).to_rgba8(),
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

/// Load a still image or an animated GIF as screen frames.
pub fn load(path: &Path, fit: Fit) -> Result<Vec<Frame>, Error> {
    let is_gif = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("gif"));
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
                    Frame {
                        rgb565: rgb565(&fit_image(f.buffer(), fit)),
                        // The official driver only allows 30..500 ms per frame.
                        duration_ms: if ms == 0 { 100 } else { ms.clamp(30, 500) },
                    }
                })
                .collect());
        }
    }
    let img = image::open(path)?.to_rgba8();
    Ok(vec![Frame { rgb565: rgb565(&fit_image(&img, fit)), duration_ms: 0 }])
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
    fn pad_keeps_aspect() {
        let wide = RgbaImage::from_pixel(256, 128, image::Rgba([255, 255, 255, 255]));
        let out = fit_image(&wide, Fit::Pad);
        assert_eq!(out.get_pixel(64, 0).0, [0, 0, 0]);
        assert_eq!(out.get_pixel(64, 64).0, [255, 255, 255]);
    }
}
