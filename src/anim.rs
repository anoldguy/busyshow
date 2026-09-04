//! The `.anim` container ("bicycle0"), as written by the firmware's `seq2anim.py`.
//!
//! Layout: a fixed header, then a sections chunk (we always write just the
//! mandatory `default` section spanning every frame), then a frames chunk of
//! `[encoding u8][duration u8][len u16][data]` records. Identical consecutive
//! frames collapse into one record with a longer duration. All integers are
//! little-endian.

use image::RgbaImage;

use crate::rle;

const SIGNATURE: &[u8; 8] = b"bicycle0";
const HEADER_LEN: usize = 36;
const DEFAULT_SECTION: &str = "default";
const SECTION_LEN: usize = 4 + 4 + 4 + 1 + DEFAULT_SECTION.len() + 1;
const FRAME_HEADER_LEN: usize = 4;
const ENCODING_RAW: u8 = 0;
const ENCODING_RLE: u8 = 1;

/// How pixels are stored in the file. Either mode plays on either display;
/// the front display is true colour, the back one is 16-level grey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// 3 bytes per pixel, alpha dropped. Suggested for the front display.
    Rgb888,
    /// 2 pixels per byte, the top nibble of the red channel, exactly as the
    /// reference encoder does it. Grey your frames first (see
    /// [`convert`](crate::convert), which uses ITU-R 601 luma) or you get the
    /// red channel. Suggested for the back display.
    Gray4,
    /// 4 bytes per pixel.
    Argb8888,
}

impl ColorMode {
    fn id(self) -> u8 {
        match self {
            ColorMode::Rgb888 => 0,
            ColorMode::Gray4 => 1,
            ColorMode::Argb8888 => 2,
        }
    }

    fn block_size(self) -> usize {
        match self {
            ColorMode::Rgb888 => 3,
            ColorMode::Gray4 => 1,
            ColorMode::Argb8888 => 4,
        }
    }

    fn pack(self, frame: &RgbaImage) -> Vec<u8> {
        let px = frame.as_raw();
        match self {
            ColorMode::Rgb888 => px
                .chunks_exact(4)
                .flat_map(|p| [p[2], p[1], p[0]])
                .collect(),
            ColorMode::Argb8888 => px
                .chunks_exact(4)
                .flat_map(|p| [p[2], p[1], p[0], p[3]])
                .collect(),
            ColorMode::Gray4 => {
                debug_assert!(px.len().is_multiple_of(8), "gray4 needs pixel pairs");
                px.chunks_exact(8)
                    .map(|pair| (pair[0] & 0xF0) | (pair[4] >> 4))
                    .collect()
            }
        }
    }
}

/// Why a set of frames could not be encoded
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    #[error("an animation needs at least one frame")]
    NoFrames,
    #[error("frame {index} is {width}x{height}, but frame 0 is {expected_width}x{expected_height}")]
    FrameSize {
        index: usize,
        width: u32,
        height: u32,
        expected_width: u32,
        expected_height: u32,
    },
    #[error("frames are {width}x{height}, the format allows at most 255x255")]
    TooLarge { width: u32, height: u32 },
    #[error("gray4 packs two pixels per byte, so frames need an even number of pixels")]
    OddPixelCount,
    #[error("an encoded frame is {len} bytes, the format allows at most 65535")]
    FrameTooLong { len: usize },
    #[error("fps must be at least 1; the player's frame period is 1000 / fps")]
    ZeroFps,
}

#[derive(Clone)]
struct FileFrame {
    encoding: u8,
    duration: u8,
    len: u16,
    data: Vec<u8>,
}

impl FileFrame {
    fn new(frame: &RgbaImage, color: ColorMode) -> Result<Self, EncodeError> {
        let raw = color.pack(frame);
        let compressed = rle::compress(&raw, color.block_size());
        let (encoding, data) = if compressed.len() < raw.len() {
            (ENCODING_RLE, compressed)
        } else {
            (ENCODING_RAW, raw)
        };
        let len =
            u16::try_from(data.len()).map_err(|_| EncodeError::FrameTooLong { len: data.len() })?;
        Ok(Self {
            encoding,
            duration: 1,
            len,
            data,
        })
    }

    fn write(&self, out: &mut Vec<u8>) {
        out.push(self.encoding);
        out.push(self.duration);
        out.extend(self.len.to_le_bytes());
        out.extend_from_slice(&self.data);
    }
}

/// Encode `frames`, all the same size, into a `.anim` file.
///
/// Pixels are stored as-is in the given [`ColorMode`]; for
/// [`ColorMode::Gray4`] that means the red channel, so grey frames first.
pub fn encode(frames: &[RgbaImage], fps: u8, color: ColorMode) -> Result<Vec<u8>, EncodeError> {
    let runs: Vec<(&RgbaImage, usize)> = frames.iter().map(|f| (f, 1)).collect();
    encode_runs(&runs, fps, color)
}

/// [`encode`] with each frame shown for `count` display frames, without
/// materialising the repeats.
pub(crate) fn encode_runs(
    runs: &[(&RgbaImage, usize)],
    fps: u8,
    color: ColorMode,
) -> Result<Vec<u8>, EncodeError> {
    if fps == 0 {
        return Err(EncodeError::ZeroFps);
    }
    let display_frames: usize = runs.iter().map(|&(_, count)| count).sum();
    let &(first, _) = runs
        .first()
        .filter(|_| display_frames > 0)
        .ok_or(EncodeError::NoFrames)?;
    let (width, height) = first.dimensions();
    let (Ok(width_u8), Ok(height_u8)) = (u8::try_from(width), u8::try_from(height)) else {
        return Err(EncodeError::TooLarge { width, height });
    };
    if color == ColorMode::Gray4 && !(width * height).is_multiple_of(2) {
        return Err(EncodeError::OddPixelCount);
    }

    let mut file_frames: Vec<FileFrame> = Vec::new();
    let mut previous: Option<&RgbaImage> = None;
    for (index, &(frame, count)) in runs.iter().enumerate() {
        if frame.dimensions() != (width, height) {
            return Err(EncodeError::FrameSize {
                index,
                width: frame.width(),
                height: frame.height(),
                expected_width: width,
                expected_height: height,
            });
        }
        let mut remaining = count;
        if previous == Some(frame)
            && let Some(last) = file_frames.last_mut()
        {
            let extend = remaining.min(usize::from(u8::MAX - last.duration));
            last.duration += extend as u8;
            remaining -= extend;
        }
        if remaining == 0 {
            continue;
        }
        previous = Some(frame);
        let file_frame = FileFrame::new(frame, color)?;
        while remaining > 0 {
            let duration = remaining.min(usize::from(u8::MAX));
            file_frames.push(FileFrame {
                duration: duration as u8,
                ..file_frame.clone()
            });
            remaining -= duration;
        }
    }

    let frames_len: usize = file_frames
        .iter()
        .map(|f| FRAME_HEADER_LEN + f.data.len())
        .sum();
    let max_encoded_len = file_frames.iter().map(|f| f.len).max().unwrap_or(0);

    let mut out = Vec::with_capacity(HEADER_LEN + SECTION_LEN + frames_len);
    out.extend_from_slice(SIGNATURE);
    out.push(0); // flags
    out.push(width_u8);
    out.push(height_u8);
    out.push(color.id());
    out.push(fps);
    out.extend(max_encoded_len.to_le_bytes());
    out.push(0); // unused
    out.extend((SECTION_LEN as u32).to_le_bytes());
    out.extend((frames_len as u32).to_le_bytes());
    out.extend(1u32.to_le_bytes()); // section count
    out.extend((file_frames.len() as u32).to_le_bytes());
    out.extend((display_frames as u32).to_le_bytes());
    debug_assert_eq!(out.len(), HEADER_LEN);

    out.extend(0u32.to_le_bytes()); // first display frame
    out.extend(((display_frames - 1) as u32).to_le_bytes()); // last display frame
    out.extend(((HEADER_LEN + SECTION_LEN) as u32).to_le_bytes()); // first file frame
    out.push(file_frames[0].duration);
    out.extend_from_slice(DEFAULT_SECTION.as_bytes());
    out.push(0);
    debug_assert_eq!(out.len(), HEADER_LEN + SECTION_LEN);

    for file_frame in &file_frames {
        file_frame.write(&mut out);
    }
    debug_assert_eq!(out.len(), HEADER_LEN + SECTION_LEN + frames_len);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(w, h, image::Rgba(rgba))
    }

    #[test]
    fn rejects_empty_input() {
        assert!(matches!(
            encode(&[], 1, ColorMode::Rgb888),
            Err(EncodeError::NoFrames)
        ));
    }

    #[test]
    fn rejects_mismatched_frame_sizes() {
        let frames = [solid(2, 2, [0; 4]), solid(2, 3, [0; 4])];
        assert!(matches!(
            encode(&frames, 1, ColorMode::Rgb888),
            Err(EncodeError::FrameSize { index: 1, .. })
        ));
    }

    #[test]
    fn rejects_frames_over_255_pixels() {
        let frames = [solid(256, 1, [0; 4])];
        assert!(matches!(
            encode(&frames, 1, ColorMode::Rgb888),
            Err(EncodeError::TooLarge { .. })
        ));
    }

    #[test]
    fn rejects_odd_pixel_count_for_gray4() {
        let frames = [solid(3, 1, [0; 4])];
        assert!(matches!(
            encode(&frames, 1, ColorMode::Gray4),
            Err(EncodeError::OddPixelCount)
        ));
    }

    #[test]
    fn rejects_frames_whose_raw_bytes_exceed_u16() {
        // 255x255 noise in argb8888 cannot compress under 65535 bytes
        let mut frame = RgbaImage::new(255, 255);
        let mut state: u32 = 7;
        for p in frame.pixels_mut() {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            p.0 = state.to_le_bytes();
        }
        assert!(matches!(
            encode(&[frame], 1, ColorMode::Argb8888),
            Err(EncodeError::FrameTooLong { .. })
        ));
    }

    #[test]
    fn splits_runs_of_identical_frames_at_255() {
        let frames = vec![solid(1, 2, [1, 2, 3, 4]); 300];
        let out = encode(&frames, 10, ColorMode::Gray4).unwrap();
        let file_frame_count = u32::from_le_bytes(out[28..32].try_into().unwrap());
        let display_frame_count = u32::from_le_bytes(out[32..36].try_into().unwrap());
        assert_eq!((file_frame_count, display_frame_count), (2, 300));
        // default section's duration_override is the first file frame's duration
        assert_eq!(out[HEADER_LEN + 12], 255);
        let frames_chunk = &out[HEADER_LEN + SECTION_LEN..];
        assert_eq!(frames_chunk[1], 255);
        let first_len = usize::from(u16::from_le_bytes([frames_chunk[2], frames_chunk[3]]));
        assert_eq!(frames_chunk[FRAME_HEADER_LEN + first_len + 1], 45);
    }

    #[test]
    fn rejects_zero_fps() {
        assert!(matches!(
            encode(&[solid(1, 1, [0; 4])], 0, ColorMode::Rgb888),
            Err(EncodeError::ZeroFps)
        ));
    }

    #[test]
    fn runs_encode_like_repeated_frames() {
        let a = solid(1, 2, [1, 2, 3, 4]);
        let b = solid(1, 2, [5, 6, 7, 8]);
        // the same frame twice in a row is what convert produces for two
        // identical decoded frames; it has to merge across that boundary too
        let runs = [(&a, 200), (&a, 100), (&b, 2), (&a, 1)];
        let expanded: Vec<RgbaImage> = runs
            .iter()
            .flat_map(|&(f, n)| std::iter::repeat_n(f.clone(), n))
            .collect();
        assert_eq!(
            encode_runs(&runs, 10, ColorMode::Gray4).unwrap(),
            encode(&expanded, 10, ColorMode::Gray4).unwrap()
        );
    }
}
