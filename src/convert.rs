//! Animated image in, `.anim` out.

use std::io::Cursor;

use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::imageops::{self, FilterType};
use image::{AnimationDecoder, Frame, ImageFormat, RgbaImage};

use crate::anim::{self, ColorMode, EncodeError};

/// Size and colour depth to encode for. [`Target::FRONT`] and [`Target::BACK`]
/// are the bar's two displays; anything else is for testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    /// Width in pixels, at most 255
    pub width: u32,
    /// Height in pixels, at most 255
    pub height: u32,
    /// Pixel format to store
    pub color: ColorMode,
}

impl Target {
    /// The 72x16 true colour front display
    pub const FRONT: Target = Target {
        width: 72,
        height: 16,
        color: ColorMode::Rgb888,
    };
    /// The 160x80 16-level grey back display
    pub const BACK: Target = Target {
        width: 160,
        height: 80,
        color: ColorMode::Gray4,
    };
}

/// Why an image could not be converted
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not decode the image")]
    Decode(#[from] image::ImageError),
    #[error("{0:?} is not supported; use a GIF, an animated WebP, or an APNG")]
    Unsupported(ImageFormat),
    #[error("the image has no animation frames")]
    NotAnimated,
    #[error(transparent)]
    Encode(#[from] EncodeError),
}

/// What browsers substitute for a zero frame delay.
const DEFAULT_DELAY_MS: u32 = 100;

/// The player's frame period is `1000 / fps` whole milliseconds, so this is
/// the fastest rate it plays at true speed. GIFs cannot express finer timing.
const MAX_FPS: u8 = 100;

/// Decode a GIF, animated WebP, or APNG, fit every frame to `target` (scale to
/// cover, then centre crop), and encode it. Frame timing comes from the file's
/// own delays.
pub fn convert(data: &[u8], target: Target) -> Result<Vec<u8>, Error> {
    let decoded = decode(data)?;
    let (fps, period_ms) = frame_rate(decoded.iter().map(delay_ms));

    let mut frames = Vec::new();
    for frame in decoded {
        let repeats = repeats(delay_ms(&frame), period_ms);
        let mut fitted = fit(frame.into_buffer(), target.width, target.height);
        if target.color == ColorMode::Gray4 {
            to_luma(&mut fitted);
        }
        frames.push((fitted, repeats));
    }

    let runs: Vec<(&RgbaImage, usize)> = frames.iter().map(|(f, n)| (f, *n)).collect();
    Ok(anim::encode_runs(&runs, fps, target.color)?)
}

fn decode(data: &[u8]) -> Result<Vec<Frame>, Error> {
    let reader = Cursor::new(data);
    let frames = match image::guess_format(data)? {
        ImageFormat::Gif => GifDecoder::new(reader)?.into_frames(),
        ImageFormat::WebP => WebPDecoder::new(reader)?.into_frames(),
        ImageFormat::Png => PngDecoder::new(reader)?.apng()?.into_frames(),
        other => return Err(Error::Unsupported(other)),
    };
    let frames = frames.collect_frames()?;
    if frames.is_empty() {
        return Err(Error::NotAnimated);
    }
    Ok(frames)
}

fn delay_ms(frame: &Frame) -> u32 {
    let (numer, denom) = frame.delay().numer_denom_ms();
    match numer / denom.max(1) {
        0 => DEFAULT_DELAY_MS,
        ms => ms,
    }
}

/// Pick the fps to declare from the delays' common tick, and return it with
/// the whole-millisecond period the player will actually run at.
fn frame_rate(delays_ms: impl Iterator<Item = u32>) -> (u8, u32) {
    let tick = delays_ms.fold(0, gcd).max(1);
    let fps = u8::try_from(1000 / tick)
        .unwrap_or(u8::MAX)
        .clamp(1, MAX_FPS);
    (fps, 1000 / u32::from(fps))
}

/// Display frames a delay spans at the player's real period, nearest whole.
fn repeats(delay_ms: u32, period_ms: u32) -> usize {
    ((delay_ms + period_ms / 2) / period_ms).max(1) as usize
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Scale so the image covers `width`x`height`, then crop the middle.
fn fit(image: RgbaImage, width: u32, height: u32) -> RgbaImage {
    if image.dimensions() == (width, height) {
        return image;
    }
    let scale = f64::max(
        f64::from(width) / f64::from(image.width()),
        f64::from(height) / f64::from(image.height()),
    );
    let scaled_w = ((f64::from(image.width()) * scale).ceil() as u32).max(width);
    let scaled_h = ((f64::from(image.height()) * scale).ceil() as u32).max(height);
    let filter = if scale < 1.0 {
        FilterType::Lanczos3
    } else {
        FilterType::Nearest
    };
    let scaled = imageops::resize(&image, scaled_w, scaled_h, filter);
    imageops::crop_imm(
        &scaled,
        (scaled_w - width) / 2,
        (scaled_h - height) / 2,
        width,
        height,
    )
    .to_image()
}

/// Replace colour with ITU-R 601 luma, the same weights Pillow's `L` mode
/// uses, so grey output matches what the reference pipeline would produce.
fn to_luma(image: &mut RgbaImage) {
    for p in image.pixels_mut() {
        let [r, g, b, a] = p.0;
        let l = ((u32::from(r) * 19595 + u32::from(g) * 38470 + u32::from(b) * 7471 + 0x8000) >> 16)
            as u8;
        p.0 = [l, l, l, a];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_covers_then_centre_crops() {
        // 4x2 image, columns of distinct values; fitting to 2x2 keeps the middle columns
        let mut img = RgbaImage::new(4, 2);
        for (x, _, p) in img.enumerate_pixels_mut() {
            p.0 = [x as u8, 0, 0, 255];
        }
        let out = fit(img, 2, 2);
        assert_eq!(out.dimensions(), (2, 2));
        assert_eq!(out.get_pixel(0, 0).0[0], 1);
        assert_eq!(out.get_pixel(1, 0).0[0], 2);
    }

    #[test]
    fn luma_matches_pillow() {
        let mut img = RgbaImage::from_pixel(1, 1, image::Rgba([200, 100, 50, 7]));
        to_luma(&mut img);
        assert_eq!(img.get_pixel(0, 0).0, [124, 124, 124, 7]);
    }

    #[test]
    fn frame_rate_follows_the_player_period() {
        let rate = |delays: &[u32]| frame_rate(delays.iter().copied());
        assert_eq!(rate(&[1000, 1000]), (1, 1000));
        assert_eq!(rate(&[2000, 4000]), (1, 1000)); // slower than 1 fps clamps up
        assert_eq!(rate(&[400, 1200]), (2, 500)); // 1000/400 truncates to 2
        assert_eq!(rate(&[30, 60]), (33, 30)); // 1000/33 is 30, the tick itself
        assert_eq!(rate(&[33, 67]), (100, 10)); // 1ms tick clamps to MAX_FPS
    }

    #[test]
    fn repeats_round_to_the_player_period() {
        assert_eq!(repeats(2000, 1000), 2);
        assert_eq!(repeats(4000, 1000), 4);
        assert_eq!(repeats(400, 500), 1); // 25% slow, the best an integer fps can do
        assert_eq!(repeats(1200, 500), 2);
        assert_eq!(repeats(33, 10), 3);
        assert_eq!(repeats(67, 10), 7);
        assert_eq!(repeats(1, 1000), 1); // never drop a frame
    }

    #[test]
    fn rejects_still_image_formats() {
        // BMP magic and padding; the format is recognised but not decoded
        let bmp = b"BM\0\0\0\0\0\0\0\0\0\0\0\0";
        assert!(matches!(
            convert(bmp, Target::FRONT),
            Err(Error::Unsupported(ImageFormat::Bmp))
        ));
    }

    fn gif(delay_ms: u32) -> Vec<u8> {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame};
        let frame = |v| {
            Frame::from_parts(
                RgbaImage::from_pixel(2, 2, image::Rgba([v, 0, 0, 255])),
                0,
                0,
                Delay::from_numer_denom_ms(delay_ms, 1),
            )
        };
        let mut out = Vec::new();
        GifEncoder::new(&mut out)
            .encode_frames([frame(10), frame(200)])
            .unwrap();
        out
    }

    #[test]
    fn zero_delay_defaults_to_100ms() {
        let target = Target {
            width: 2,
            height: 2,
            color: ColorMode::Rgb888,
        };
        let out = convert(&gif(0), target).unwrap();
        assert_eq!(out[12], 10, "fps byte should be 1000 / DEFAULT_DELAY_MS");
    }

    #[test]
    fn rejects_a_still_png() {
        let mut png = Vec::new();
        RgbaImage::new(2, 2)
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        assert!(matches!(
            convert(&png, Target::FRONT),
            Err(Error::NotAnimated)
        ));
    }
}
