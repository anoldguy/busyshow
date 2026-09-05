//! The `.anim` container as written by the firmware's `seq2anim.py`

use crate::layout::Target;
use crate::rle;

pub(crate) const SIGNATURE: &[u8; 8] = b"bicycle0";
pub(crate) const HEADER_LEN: usize = 36;
pub(crate) const DEFAULT_SECTION: &str = "default";
/// The fields before a section's NUL-terminated name
pub(crate) const SECTION_FIXED_LEN: usize = 4 + 4 + 4 + 1;
pub(crate) const SECTION_LEN: usize = SECTION_FIXED_LEN + DEFAULT_SECTION.len() + 1;
pub(crate) const FRAME_HEADER_LEN: usize = 4;
pub(crate) const ENCODING_RAW: u8 = 0;
pub(crate) const ENCODING_RLE: u8 = 1;

/// One packed frame and the number of display frames it is held for
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pixels: Vec<u8>,
    repeats: u32,
}

impl Frame {
    /// Frame held for one display frame
    pub fn new(pixels: impl Into<Vec<u8>>) -> Self {
        Self::repeated(pixels, 1)
    }

    /// Frame held for `repeats` display frames, but zero drops the frame
    pub fn repeated(pixels: impl Into<Vec<u8>>, repeats: u32) -> Self {
        Self {
            pixels: pixels.into(),
            repeats,
        }
    }

    /// Pixels packed in the target's layout
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Display frames it is held for
    pub fn repeats(&self) -> u32 {
        self.repeats
    }
}

/// Why frames could not be encoded
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    /// No frame had a non-zero repeat count
    #[error("an animation needs at least one frame")]
    NoFrames,

    /// A frame's pixels do not match the target's frame length
    #[error("frame {index} is {actual} bytes, but the target needs {expected}")]
    FrameLength {
        /// Index of the frame
        index: usize,
        /// Bytes the target needs
        expected: usize,
        /// Bytes given
        actual: usize,
    },

    /// A frame record does not fit its 16-bit length field
    #[error("an encoded frame is {len} bytes, the format allows at most 65535")]
    FrameTooLong {
        /// Bytes the record needs
        len: usize,
    },

    /// The frames chunk does not fit its 32-bit length field
    #[error(
        "the frames chunk is {len} bytes, the format allows at most {}",
        u32::MAX
    )]
    FramesChunkTooLong {
        /// Bytes the chunk needs
        len: usize,
    },

    /// The frame rate is zero
    #[error("fps must be at least 1; the player's frame period is 1000 / fps")]
    ZeroFps,

    /// The repeat counts overflow the header's 32-bit display frame count
    #[error(
        "the animation is {count} display frames long, the format allows at most {}",
        u32::MAX
    )]
    TooManyFrames {
        /// Display frames given
        count: u64,
    },
}

/// Encode `frames` already packed in `target`'s pixel layout into the bytes of a `.anim`
/// file that plays at `fps` display frames per second
pub fn encode(target: Target, fps: u8, frames: &[Frame]) -> Result<Vec<u8>, EncodeError> {
    if fps == 0 {
        return Err(EncodeError::ZeroFps);
    }

    let display_frames: u64 = frames.iter().map(|f| u64::from(f.repeats)).sum();

    if display_frames == 0 {
        return Err(EncodeError::NoFrames);
    }

    let display_frames = u32::try_from(display_frames).map_err(|_| EncodeError::TooManyFrames {
        count: display_frames,
    })?;

    let expected = target.frame_len();

    // The header and section need counts from the loop, so leave room for them and fill the
    // gap in once the records are written
    let mut out = vec![0u8; HEADER_LEN + SECTION_LEN];

    let mut record_count: u32 = 0;
    let mut max_encoded_len: u16 = 0;
    let mut last: Option<(usize, &[u8])> = None;

    for (index, frame) in frames.iter().enumerate() {
        if frame.pixels.len() != expected {
            return Err(EncodeError::FrameLength {
                index,
                expected,
                actual: frame.pixels.len(),
            });
        }

        let mut remaining = frame.repeats;

        if let Some((at, pixels)) = last
            && pixels == frame.pixels.as_slice()
        {
            let extend = remaining.min(u32::from(u8::MAX - out[at]));
            out[at] += extend as u8;
            remaining -= extend;
        }

        // Zero repeats, or the previous record took them all. Treat the frame as if it
        // were never listed: keep `last` so its neighbors still merge, and skip the
        // length bookkeeping for a frame that never reaches the file
        if remaining == 0 {
            continue;
        }

        let compressed = rle::compress(&frame.pixels, target.layout().block_size());
        let (encoding, data) = if compressed.len() < frame.pixels.len() {
            (ENCODING_RLE, compressed.as_slice())
        } else {
            (ENCODING_RAW, frame.pixels.as_slice())
        };
        let len =
            u16::try_from(data.len()).map_err(|_| EncodeError::FrameTooLong { len: data.len() })?;
        max_encoded_len = max_encoded_len.max(len);

        // A duration byte holds 255, so longer holds take a record apiece
        while remaining > 0 {
            let duration = remaining.min(u32::from(u8::MAX)) as u8;

            out.push(encoding);
            last = Some((out.len(), frame.pixels.as_slice()));
            out.push(duration);
            out.extend(len.to_le_bytes());
            out.extend_from_slice(data);

            record_count += 1;
            remaining -= u32::from(duration);
        }
    }

    let frames_chunk_len = out.len() - HEADER_LEN - SECTION_LEN;
    let frames_chunk_len =
        u32::try_from(frames_chunk_len).map_err(|_| EncodeError::FramesChunkTooLong {
            len: frames_chunk_len,
        })?;
    // At least one frame has a non-zero repeat, and the first such frame finds nothing to
    // merge into, so a first record is always there
    let first_duration = out[HEADER_LEN + SECTION_LEN + 1];

    let mut head = Vec::with_capacity(HEADER_LEN + SECTION_LEN);
    head.extend_from_slice(SIGNATURE);
    head.push(0);
    head.push(target.width());
    head.push(target.height());
    head.push(target.layout().id());
    head.push(fps);
    head.extend(max_encoded_len.to_le_bytes());
    head.push(0);
    head.extend((SECTION_LEN as u32).to_le_bytes());
    head.extend(frames_chunk_len.to_le_bytes());
    head.extend(1u32.to_le_bytes());
    head.extend(record_count.to_le_bytes());
    head.extend(display_frames.to_le_bytes());
    debug_assert_eq!(head.len(), HEADER_LEN);

    head.extend(0u32.to_le_bytes());
    head.extend((display_frames - 1).to_le_bytes());
    head.extend(((HEADER_LEN + SECTION_LEN) as u32).to_le_bytes());
    // The section's duration override, which the reference sets to its first record's
    head.push(first_duration);
    head.extend_from_slice(DEFAULT_SECTION.as_bytes());
    head.push(0);
    debug_assert_eq!(head.len(), HEADER_LEN + SECTION_LEN);

    out[..head.len()].copy_from_slice(&head);

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::PixelLayout;

    fn target(width: u8, height: u8, layout: PixelLayout) -> Target {
        Target::new(width, height, layout).unwrap()
    }

    #[test]
    fn rejects_empty_input() {
        assert!(matches!(
            encode(Target::FRONT, 1, &[]),
            Err(EncodeError::NoFrames)
        ));
    }

    #[test]
    fn rejects_input_whose_repeats_are_all_zero() {
        let frame = Frame::repeated([0; 3], 0);

        assert!(matches!(
            encode(target(1, 1, PixelLayout::Bgr888), 1, &[frame]),
            Err(EncodeError::NoFrames)
        ));
    }

    #[test]
    fn rejects_frames_of_the_wrong_length() {
        let t = target(2, 2, PixelLayout::Bgr888);
        let frames = [Frame::new([0; 12]), Frame::new([0; 11])];

        assert!(matches!(
            encode(t, 1, &frames),
            Err(EncodeError::FrameLength {
                index: 1,
                expected: 12,
                actual: 11
            })
        ));
    }

    #[test]
    fn rejects_noise_whose_encoded_bytes_exceed_u16() {
        let mut state: u32 = 7;
        let pixels: Vec<u8> = (0..255 * 255 * 4)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect();
        let t = target(255, 255, PixelLayout::Bgra8888);

        assert!(matches!(
            encode(t, 1, &[Frame::new(pixels)]),
            Err(EncodeError::FrameTooLong { .. })
        ));
    }

    #[test]
    fn rejects_zero_fps() {
        let t = target(1, 1, PixelLayout::Bgr888);

        assert!(matches!(
            encode(t, 0, &[Frame::new([0; 3])]),
            Err(EncodeError::ZeroFps)
        ));
    }

    #[test]
    fn rejects_more_display_frames_than_the_header_can_count() {
        let t = target(1, 1, PixelLayout::Bgr888);
        let frames = [
            Frame::repeated([0; 3], u32::MAX),
            Frame::repeated([0; 3], 1),
        ];

        assert!(matches!(
            encode(t, 1, &frames),
            Err(EncodeError::TooManyFrames { .. })
        ));
    }

    #[test]
    fn splits_runs_of_identical_frames_at_255_and_overrides_the_default_section_duration() {
        let t = target(1, 2, PixelLayout::Gray4);
        let frames = vec![Frame::new([0x10]); 300];
        let out = encode(t, 10, &frames).unwrap();

        let file_frame_count = u32::from_le_bytes(out[28..32].try_into().unwrap());
        let display_frame_count = u32::from_le_bytes(out[32..36].try_into().unwrap());

        assert_eq!((file_frame_count, display_frame_count), (2, 300));
        assert_eq!(out[HEADER_LEN + 12], 255);

        let frames_chunk = &out[HEADER_LEN + SECTION_LEN..];
        let first_len = usize::from(u16::from_le_bytes([frames_chunk[2], frames_chunk[3]]));

        assert_eq!(frames_chunk[1], 255);
        assert_eq!(frames_chunk[FRAME_HEADER_LEN + first_len + 1], 45);
    }

    #[test]
    fn runs_merge_across_frames_like_repeated_frames() {
        let t = target(1, 2, PixelLayout::Gray4);
        let a = [0x12];
        let b = [0x56];
        let runs = [
            Frame::repeated(a, 200),
            Frame::repeated(a, 100),
            Frame::repeated(b, 2),
            Frame::repeated(a, 1),
        ];
        let expanded: Vec<Frame> = runs
            .iter()
            .flat_map(|f| std::iter::repeat_n(Frame::new(f.pixels.clone()), f.repeats as usize))
            .collect();

        assert_eq!(
            encode(t, 10, &runs).unwrap(),
            encode(t, 10, &expanded).unwrap()
        );
    }

    #[test]
    fn writes_the_header_the_firmware_expects() {
        let t = target(2, 1, PixelLayout::Bgr888);
        let out = encode(t, 30, &[Frame::new([1, 2, 3, 4, 5, 6])]).unwrap();

        assert_eq!(&out[..8], b"bicycle0");
        assert_eq!(out[8], 0, "flags");
        assert_eq!(&out[9..11], &[2, 1], "width, height");
        assert_eq!(out[11], 0, "bgr888");
        assert_eq!(out[12], 30, "fps");
        assert_eq!(&out[13..15], &[6, 0], "max encoded frame len");
        assert_eq!(out[15], 0, "unused");
        assert_eq!(&out[16..20], &(SECTION_LEN as u32).to_le_bytes());
        assert_eq!(&out[20..24], &10u32.to_le_bytes(), "frames chunk len");
        assert_eq!(&out[24..28], &1u32.to_le_bytes(), "section count");
        assert_eq!(&out[28..32], &1u32.to_le_bytes(), "file frames");
        assert_eq!(&out[32..36], &1u32.to_le_bytes(), "display frames");
        assert_eq!(
            &out[HEADER_LEN + SECTION_LEN..],
            &[0, 1, 6, 0, 1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn a_zero_repeat_frame_does_not_interrupt_a_run() {
        let t = target(1, 2, PixelLayout::Gray4);
        let a = [0x12];
        let b = [0x56];
        let out = encode(
            t,
            10,
            &[Frame::new(a), Frame::repeated(b, 0), Frame::new(a)],
        )
        .unwrap();

        assert_eq!(&out[28..32], &1u32.to_le_bytes(), "one record");
        assert_eq!(out[HEADER_LEN + SECTION_LEN + 1], 2, "durations merged");
    }

    #[test]
    fn a_zero_repeat_frame_is_not_encoded_into_the_header() {
        let t = target(1, 8, PixelLayout::Gray4);
        let flat = [0, 0, 0, 0];
        let noisy = [1, 2, 3, 4];
        let alone = encode(t, 10, &[Frame::new(flat)]).unwrap();
        let with_dropped = encode(t, 10, &[Frame::new(flat), Frame::repeated(noisy, 0)]).unwrap();

        assert_eq!(alone, with_dropped);
    }

    #[test]
    fn a_zero_repeat_frame_is_never_too_long_to_encode() {
        let mut state: u32 = 7;
        let noise: Vec<u8> = (0..255 * 255 * 4)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect();
        let flat = vec![0u8; 255 * 255 * 4];
        let t = target(255, 255, PixelLayout::Bgra8888);

        encode(t, 1, &[Frame::new(flat), Frame::repeated(noise, 0)]).unwrap();
    }

    #[test]
    fn a_zero_repeat_frame_of_the_wrong_length_is_still_rejected() {
        let t = target(2, 2, PixelLayout::Bgr888);

        assert!(matches!(
            encode(t, 1, &[Frame::new([0; 12]), Frame::repeated([0; 11], 0)]),
            Err(EncodeError::FrameLength { index: 1, .. })
        ));
    }

    #[test]
    fn a_merge_fills_the_last_record_before_splitting_the_rest() {
        let t = target(1, 2, PixelLayout::Gray4);
        let a = [0x12];
        let frames = [Frame::repeated(a, 300), Frame::repeated(a, 500)];
        let anim = crate::decode::decode(&encode(t, 10, &frames).unwrap()).unwrap();
        let got: Vec<u32> = anim.frames().iter().map(Frame::repeats).collect();

        assert_eq!(got, [255, 255, 255, 35]);
    }

    #[test]
    fn packs_a_compressible_animation_into_run_opcodes() {
        let frames: Vec<Frame> = (0..100u8)
            .map(|v| Frame::new(vec![v; Target::BACK.frame_len()]))
            .collect();
        let out = encode(Target::BACK, 10, &frames).unwrap();

        // Every frame packs down to two-byte run opcodes
        let record = FRAME_HEADER_LEN + 2 * Target::BACK.frame_len().div_ceil(127);

        assert_eq!(out.len(), HEADER_LEN + SECTION_LEN + 100 * record);
    }
}
