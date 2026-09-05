//! Conversion of animated images into `.anim` files

use std::io::Cursor;

use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::imageops::{self, FilterType};
use image::{AnimationDecoder, Frame as ImageFrame, ImageFormat, RgbaImage};

use crate::anim::{EncodeError, Frame, encode};
use crate::layout::{PixelLayout, Target};
use crate::timing::FrameRate;

const DEFAULT_DELAY_MS: u32 = 100;

/// Why an image could not be converted
#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    /// The image could not be read
    #[error("could not decode the image")]
    Decode(#[from] image::ImageError),

    /// The image format cannot animate
    #[error(
        "{} is not supported; use a GIF, an animated WebP, or an APNG",
        format.to_mime_type()
    )]
    Unsupported {
        /// Format the image was recognized as
        format: ImageFormat,
    },

    /// The image decoded to no frames
    #[error("the image has no animation frames")]
    NotAnimated,

    /// The fitted frames could not be encoded
    #[error(transparent)]
    Encode(#[from] EncodeError),
}

/// Convert a GIF, animated WebP or APNG into a `.anim` file fitted to `target`
pub fn convert(data: &[u8], target: Target) -> Result<Vec<u8>, ConvertError> {
    let decoded = decode_frames(data)?;
    let rate = FrameRate::from_delays(decoded.iter().map(delay_ms));

    let mut frames = Vec::with_capacity(decoded.len());

    for frame in decoded {
        let repeats = rate.repeats(delay_ms(&frame));
        let mut fitted = fit(
            frame.into_buffer(),
            u32::from(target.width()),
            u32::from(target.height()),
        );

        if target.layout() == PixelLayout::Gray4 {
            to_luma(&mut fitted);
        }

        frames.push(Frame::repeated(
            target.layout().pack_rgba(fitted.as_raw()),
            repeats,
        ));
    }

    Ok(encode(target, rate.fps(), &frames)?)
}

fn decode_frames(data: &[u8]) -> Result<Vec<ImageFrame>, ConvertError> {
    let reader = Cursor::new(data);
    let frames = match image::guess_format(data)? {
        ImageFormat::Gif => GifDecoder::new(reader)?.into_frames(),
        ImageFormat::WebP => WebPDecoder::new(reader)?.into_frames(),
        ImageFormat::Png => PngDecoder::new(reader)?.apng()?.into_frames(),
        format => return Err(ConvertError::Unsupported { format }),
    };
    let frames = frames.collect_frames()?;

    if frames.is_empty() {
        return Err(ConvertError::NotAnimated);
    }

    Ok(frames)
}

fn delay_ms(frame: &ImageFrame) -> u32 {
    let (numer, denom) = frame.delay().numer_denom_ms();

    match numer / denom.max(1) {
        0 => DEFAULT_DELAY_MS,
        ms => ms,
    }
}

fn fit(image: RgbaImage, width: u32, height: u32) -> RgbaImage {
    if image.dimensions() == (width, height) {
        return image;
    }

    // Crop to the target's shape before scaling, so nothing larger than the source is built
    let (x, y, w, h) = cover_crop(image.dimensions(), (width, height));
    let cropped = imageops::crop_imm(&image, x, y, w, h);
    let filter = if w > width || h > height {
        FilterType::Lanczos3
    } else {
        FilterType::Nearest
    };

    imageops::resize(&*cropped, width, height, filter)
}

/// The largest middle of a `source` with the shape of `target`, as `(x, y, width, height)`
fn cover_crop(source: (u32, u32), target: (u32, u32)) -> (u32, u32, u32, u32) {
    let (source_w, source_h) = source;
    let (target_w, target_h) = target;
    let wider =
        u64::from(source_w) * u64::from(target_h) > u64::from(source_h) * u64::from(target_w);
    let (w, h) = if wider {
        let w = u64::from(source_h) * u64::from(target_w) / u64::from(target_h);
        (w as u32, source_h)
    } else {
        let h = u64::from(source_w) * u64::from(target_h) / u64::from(target_w);
        (source_w, h as u32)
    };
    let (w, h) = (w.max(1), h.max(1));

    ((source_w - w) / 2, (source_h - h) / 2, w, h)
}

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

    /// A 2x2 GIF of two frames, red then green, held for the given delays
    fn gif(delays_ms: [u32; 2]) -> Vec<u8> {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame};

        let frame = |rgb: [u8; 3], delay_ms| {
            let [r, g, b] = rgb;
            Frame::from_parts(
                RgbaImage::from_pixel(2, 2, image::Rgba([r, g, b, 255])),
                0,
                0,
                Delay::from_numer_denom_ms(delay_ms, 1),
            )
        };
        let mut out = Vec::new();

        GifEncoder::new(&mut out)
            .encode_frames([
                frame([255, 0, 0], delays_ms[0]),
                frame([0, 255, 0], delays_ms[1]),
            ])
            .unwrap();

        out
    }

    #[test]
    fn cover_crop_keeps_the_middle_of_the_longer_axis() {
        assert_eq!(cover_crop((4, 2), (2, 2)), (1, 0, 2, 2), "wider source");
        assert_eq!(cover_crop((2, 4), (2, 2)), (0, 1, 2, 2), "taller source");
        assert_eq!(cover_crop((72, 16), (72, 16)), (0, 0, 72, 16), "same shape");
        assert_eq!(
            cover_crop((144, 32), (72, 16)),
            (0, 0, 144, 32),
            "same aspect"
        );
        assert_eq!(
            cover_crop((1, 8000), (72, 16)),
            (0, 3999, 1, 1),
            "never below a pixel"
        );
        assert_eq!(cover_crop((1, 1), (72, 16)), (0, 0, 1, 1), "single pixel");
    }

    #[test]
    fn fit_never_scales_more_than_the_target_needs() {
        let out = fit(RgbaImage::new(1, 8000), 72, 16);

        assert_eq!(out.dimensions(), (72, 16));
    }

    #[test]
    fn each_frame_repeats_for_its_own_delay() {
        let target = Target::new(2, 2, PixelLayout::Bgr888).unwrap();
        let anim = crate::decode::decode(&convert(&gif([100, 300]), target).unwrap()).unwrap();
        let repeats: Vec<u32> = anim.frames().iter().map(Frame::repeats).collect();

        assert_eq!(anim.fps(), 10);
        assert_eq!(repeats, [1, 3]);
    }

    #[test]
    fn fit_scales_to_cover_then_keeps_the_middle() {
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
    fn luma_uses_the_itu_r_601_weights_pillow_uses() {
        let mut img = RgbaImage::from_pixel(1, 1, image::Rgba([200, 100, 50, 7]));
        to_luma(&mut img);

        assert_eq!(img.get_pixel(0, 0).0, [124, 124, 124, 7]);
    }

    #[test]
    fn rejects_a_recognized_still_image_format() {
        let bmp = b"BM\0\0\0\0\0\0\0\0\0\0\0\0";

        let error = convert(bmp, Target::FRONT).unwrap_err();

        assert!(matches!(
            error,
            ConvertError::Unsupported {
                format: ImageFormat::Bmp
            }
        ));
        assert_eq!(
            error.to_string(),
            "image/bmp is not supported; use a GIF, an animated WebP, or an APNG"
        );
    }

    #[test]
    fn rejects_things_that_are_not_images() {
        assert!(matches!(
            convert(b"hello", Target::FRONT),
            Err(ConvertError::Decode(_))
        ));
    }

    #[test]
    fn a_zero_delay_defaults_to_100ms() {
        let target = Target::new(2, 2, PixelLayout::Bgr888).unwrap();
        let out = convert(&gif([0, 0]), target).unwrap();

        assert_eq!(out[12], 10, "fps byte is 1000 / DEFAULT_DELAY_MS");
    }

    #[test]
    fn rejects_a_still_png() {
        let mut png = Vec::new();
        RgbaImage::new(2, 2)
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();

        assert!(matches!(
            convert(&png, Target::FRONT),
            Err(ConvertError::NotAnimated)
        ));
    }
}
