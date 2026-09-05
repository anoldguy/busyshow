//! Decoder for `.anim` files

use crate::anim::{
    DEFAULT_SECTION, ENCODING_RAW, ENCODING_RLE, EncodeError, FRAME_HEADER_LEN, Frame, HEADER_LEN,
    SECTION_FIXED_LEN, SIGNATURE, encode,
};
use crate::layout::{PixelLayout, Target};
use crate::rle;

/// Why a file could not be decoded
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// The file does not start with `bicycle0`
    #[error("not a .anim file: the signature is missing")]
    MissingSignature,

    /// The file length disagrees with the header
    #[error("the file is {actual} bytes but its header describes {expected}")]
    Length {
        /// Bytes the header describes
        expected: u64,
        /// Bytes in the file
        actual: usize,
    },

    /// The header's color byte is not a known layout
    #[error("unknown pixel layout {id}")]
    UnknownLayout {
        /// Color byte found
        id: u8,
    },

    /// The header describes a target no frame can have
    #[error(
        "the header describes {width}x{height} in {layout}, which is not a shape a frame can have"
    )]
    InvalidTarget {
        /// Width the header declares
        width: u8,
        /// Height the header declares
        height: u8,
        /// Layout the header declares
        layout: PixelLayout,
    },

    /// The header's fps byte is zero
    #[error("the header declares 0 fps")]
    ZeroFps,

    /// The header sets a flag the firmware does not know
    #[error("the header sets unknown flags {flags:#04x}")]
    UnknownFlags {
        /// Flags byte found
        flags: u8,
    },

    /// The header declares no sections, but the `default` section is mandatory
    #[error("the header declares no sections, but a default section is mandatory")]
    NoSections,

    /// The sections chunk cannot hold even one section
    #[error("the sections chunk is {len} bytes, too short for the default section")]
    SectionsTooShort {
        /// Bytes in the sections chunk
        len: u32,
    },

    /// The sections chunk does not end with the NUL that terminates its last name
    #[error("the sections chunk does not end with a NUL")]
    SectionsUnterminated,

    /// The sections chunk holds more sections than the header declares
    #[error("the sections chunk holds more than the {declared} sections the header declares")]
    SectionCount {
        /// Sections the header declares
        declared: u32,
    },

    /// The first section is not a `default` section covering every display frame
    #[error(
        "the first section is {name:?} over display frames {start}..={end} at offset {frame_offs}, \
         but the default section must cover 0..={last} at offset {expected_offs}"
    )]
    DefaultSection {
        /// Name of the first section
        name: String,
        /// First display frame it declares
        start: u32,
        /// Last display frame it declares
        end: u32,
        /// File offset of its first record
        frame_offs: u32,
        /// Last display frame of the animation
        last: u32,
        /// Where the frames chunk starts
        expected_offs: u32,
    },

    /// A frame record is longer than the header's maximum encoded length
    #[error("frame {index} is {len} bytes, but the header allows at most {max} per frame")]
    FrameTooLong {
        /// Index of the frame
        index: usize,
        /// Bytes in the record
        len: u16,
        /// Bytes the header allows
        max: u16,
    },

    /// A frame record is neither raw nor run-length encoded
    #[error("frame {index} uses unknown encoding {encoding}")]
    UnknownEncoding {
        /// Index of the frame
        index: usize,
        /// Encoding byte found
        encoding: u8,
    },

    /// A frame record has a zero duration
    #[error("frame {index} has a zero duration")]
    ZeroDuration {
        /// Index of the frame
        index: usize,
    },

    /// A frame record extends past the frames chunk
    #[error("frame {index} runs past the end of the frames chunk")]
    Truncated {
        /// Index of the frame
        index: usize,
    },

    /// A run-length encoded record does not unpack
    #[error("frame {index} is run-length encoded but does not unpack cleanly")]
    BadRle {
        /// Index of the frame
        index: usize,
    },

    /// A frame's pixels do not match the header's size and layout
    #[error(
        "frame {index} unpacks to {actual} bytes, but the header's size and layout need {expected}"
    )]
    FrameLength {
        /// Index of the frame
        index: usize,
        /// Bytes the header's size and layout need
        expected: usize,
        /// Bytes the frame unpacked to
        actual: usize,
    },

    /// The header's file frame count disagrees with the records found
    #[error("the frames chunk holds {found} records, the header declares {declared}")]
    FrameCount {
        /// Records the header declares
        declared: u32,
        /// Records found
        found: u32,
    },

    /// The header's display frame count disagrees with the record durations
    #[error("the frames add up to {found} display frames, the header declares {declared}")]
    DisplayFrames {
        /// Display frames the header declares
        declared: u32,
        /// Display frames the records add up to
        found: u64,
    },
}

/// Decoded `.anim` file
///
/// The mandatory `default` section is checked and any other sections are skipped, so
/// [`Animation::encode`] writes back only the `default` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation {
    target: Target,
    fps: u8,
    frames: Vec<Frame>,
}

impl Animation {
    /// Size and pixel layout of every frame
    pub fn target(&self) -> Target {
        self.target
    }

    /// Display frames per second
    pub fn fps(&self) -> u8 {
        self.fps
    }

    /// Frames in play order, as the file stores them
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// The frames in play order, giving up the animation
    pub fn into_frames(self) -> Vec<Frame> {
        self.frames
    }

    /// Total display frames
    pub fn display_frames(&self) -> u64 {
        self.frames.iter().map(|f| u64::from(f.repeats())).sum()
    }

    /// Write the animation back out as a `.anim` file
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        encode(self.target, self.fps, &self.frames)
    }
}

fn u16_at(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

fn u32_at(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(data[at..at + 4].try_into().expect("four bytes"))
}

/// Decode a `.anim` file
pub fn decode(data: &[u8]) -> Result<Animation, DecodeError> {
    if data.len() < HEADER_LEN {
        return Err(DecodeError::Length {
            expected: HEADER_LEN as u64,
            actual: data.len(),
        });
    }

    if &data[..SIGNATURE.len()] != SIGNATURE {
        return Err(DecodeError::MissingSignature);
    }

    let flags = data[8];

    if flags != 0 {
        return Err(DecodeError::UnknownFlags { flags });
    }

    let width = data[9];
    let height = data[10];
    let layout =
        PixelLayout::from_id(data[11]).ok_or(DecodeError::UnknownLayout { id: data[11] })?;
    let fps = data[12];

    if fps == 0 {
        return Err(DecodeError::ZeroFps);
    }

    let max_encoded_len = u16_at(data, 13);
    let sections_len = u32_at(data, 16);
    let frames_chunk_len = u32_at(data, 20);
    let section_count = u32_at(data, 24);
    let file_frame_count = u32_at(data, 28);
    let display_frame_count = u32_at(data, 32);

    let expected = HEADER_LEN as u64 + u64::from(sections_len) + u64::from(frames_chunk_len);

    if data.len() as u64 != expected {
        return Err(DecodeError::Length {
            expected,
            actual: data.len(),
        });
    }

    let target = Target::new(width, height, layout).ok_or(DecodeError::InvalidTarget {
        width,
        height,
        layout,
    })?;
    let frame_len = target.frame_len();
    let (sections, mut chunk) = data[HEADER_LEN..].split_at(sections_len as usize);

    check_sections(sections, section_count, display_frame_count)?;

    let mut frames = Vec::new();
    let mut index = 0;

    while !chunk.is_empty() {
        let (header, rest) = chunk
            .split_at_checked(FRAME_HEADER_LEN)
            .ok_or(DecodeError::Truncated { index })?;
        let encoding = header[0];
        let duration = header[1];
        let len = u16_at(header, 2);

        // The player reads every record into a buffer of the header's maximum
        if len > max_encoded_len {
            return Err(DecodeError::FrameTooLong {
                index,
                len,
                max: max_encoded_len,
            });
        }

        let (record, rest) = rest
            .split_at_checked(usize::from(len))
            .ok_or(DecodeError::Truncated { index })?;

        if duration == 0 {
            return Err(DecodeError::ZeroDuration { index });
        }

        let pixels = match encoding {
            ENCODING_RAW => record.to_vec(),
            ENCODING_RLE => {
                rle::decompress(record, layout.block_size()).ok_or(DecodeError::BadRle { index })?
            }
            encoding => return Err(DecodeError::UnknownEncoding { index, encoding }),
        };

        if pixels.len() != frame_len {
            return Err(DecodeError::FrameLength {
                index,
                expected: frame_len,
                actual: pixels.len(),
            });
        }

        frames.push(Frame::repeated(pixels, u32::from(duration)));
        chunk = rest;
        index += 1;
    }

    let animation = Animation {
        target,
        fps,
        frames,
    };

    let found = u32::try_from(animation.frames.len()).unwrap_or(u32::MAX);

    if found != file_frame_count {
        return Err(DecodeError::FrameCount {
            declared: file_frame_count,
            found,
        });
    }

    let found = animation.display_frames();

    if found != u64::from(display_frame_count) {
        return Err(DecodeError::DisplayFrames {
            declared: display_frame_count,
            found,
        });
    }

    Ok(animation)
}

/// The checks the firmware makes on the sections chunk before it plays a file
///
/// Only the first section is examined in full: it must be the `default` section covering
/// every display frame from the start of the frames chunk. The rest are counted and skipped.
fn check_sections(
    sections: &[u8],
    declared: u32,
    display_frame_count: u32,
) -> Result<(), DecodeError> {
    if declared == 0 {
        return Err(DecodeError::NoSections);
    }

    if sections.len() <= SECTION_FIXED_LEN {
        return Err(DecodeError::SectionsTooShort {
            len: sections.len() as u32,
        });
    }

    if sections.last() != Some(&0) {
        return Err(DecodeError::SectionsUnterminated);
    }

    let name = |at: usize| -> &[u8] {
        let rest = &sections[at + SECTION_FIXED_LEN..];
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .expect("chunk ends in NUL");
        &rest[..end]
    };

    let start = u32_at(sections, 0);
    let end = u32_at(sections, 4);
    let frame_offs = u32_at(sections, 8);
    let last = display_frame_count.wrapping_sub(1);
    let expected_offs = (HEADER_LEN + sections.len()) as u32;

    if name(0) != DEFAULT_SECTION.as_bytes()
        || start != 0
        || end != last
        || frame_offs != expected_offs
    {
        return Err(DecodeError::DefaultSection {
            name: String::from_utf8_lossy(name(0)).into_owned(),
            start,
            end,
            frame_offs,
            last,
            expected_offs,
        });
    }

    // Like the firmware, count sections while a whole one could still fit
    let mut found: u32 = 0;
    let mut at = 0;

    while sections.len() - at > SECTION_FIXED_LEN {
        if found == declared {
            return Err(DecodeError::SectionCount { declared });
        }

        at += SECTION_FIXED_LEN + name(at).len() + 1;
        found += 1;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::SECTION_LEN;

    const FIRST: usize = HEADER_LEN + SECTION_LEN;
    const SECOND: usize = FIRST + FRAME_HEADER_LEN + 9;

    fn target(width: u8, height: u8, layout: PixelLayout) -> Target {
        Target::new(width, height, layout).unwrap()
    }

    fn sample() -> Vec<u8> {
        let t = target(3, 1, PixelLayout::Bgr888);
        let frames = [
            Frame::new([1, 2, 3, 4, 5, 6, 7, 8, 9]),
            Frame::repeated([9; 9], 3),
        ];

        encode(t, 30, &frames).unwrap()
    }

    fn corrupt(at: usize, byte: u8) -> DecodeError {
        let mut data = sample();
        data[at] = byte;

        decode(&data).unwrap_err()
    }

    #[test]
    fn reads_back_what_encode_wrote() {
        let anim = decode(&sample()).unwrap();

        assert_eq!(anim.target(), target(3, 1, PixelLayout::Bgr888));
        assert_eq!(anim.fps(), 30);
        assert_eq!(anim.display_frames(), 4);

        assert_eq!(anim.frames()[0], Frame::new([1, 2, 3, 4, 5, 6, 7, 8, 9]));
        assert_eq!(anim.frames()[1], Frame::repeated([9; 9], 3));
        assert_eq!(anim.encode().unwrap(), sample());
    }

    #[test]
    fn merged_and_split_runs_come_back_as_the_file_stores_them() {
        let t = target(1, 2, PixelLayout::Gray4);
        let a = [0x12];
        let b = [0x56];
        let runs = [
            Frame::repeated(a, 300),
            Frame::repeated(a, 100),
            Frame::repeated(b, 2),
            Frame::new(a),
        ];
        let anim = decode(&encode(t, 10, &runs).unwrap()).unwrap();

        assert_eq!(
            anim.frames(),
            [
                Frame::repeated(a, 255),
                Frame::repeated(a, 145),
                Frame::repeated(b, 2),
                Frame::new(a),
            ]
        );
        assert_eq!(anim.display_frames(), 403);
    }

    #[test]
    fn rejects_a_bad_signature() {
        assert!(matches!(corrupt(0, b'B'), DecodeError::MissingSignature));
    }

    #[test]
    fn rejects_input_too_short_for_a_header() {
        assert!(matches!(
            decode(b"bicycle"),
            Err(DecodeError::Length {
                expected: 36,
                actual: 7
            })
        ));
    }

    #[test]
    fn rejects_a_length_that_disagrees_with_the_header() {
        let mut data = sample();
        data.push(0);

        assert!(matches!(
            decode(&data),
            Err(DecodeError::Length { expected, actual }) if actual as u64 == expected + 1
        ));

        data.truncate(data.len() - 2);

        assert!(matches!(
            decode(&data),
            Err(DecodeError::Length { expected, actual }) if actual as u64 + 1 == expected
        ));
    }

    #[test]
    fn rejects_an_unknown_layout() {
        assert!(matches!(
            corrupt(11, 3),
            DecodeError::UnknownLayout { id: 3 }
        ));
    }

    #[test]
    fn rejects_a_header_whose_target_no_frame_can_have() {
        assert!(matches!(
            corrupt(9, 0),
            DecodeError::InvalidTarget {
                width: 0,
                height: 1,
                layout: PixelLayout::Bgr888
            }
        ));
        assert_eq!(
            corrupt(11, 1).to_string(),
            "the header describes 3x1 in l4, which is not a shape a frame can have"
        );
    }

    #[test]
    fn rejects_zero_fps() {
        assert!(matches!(corrupt(12, 0), DecodeError::ZeroFps));
    }

    #[test]
    fn rejects_an_unknown_frame_encoding() {
        assert!(matches!(
            corrupt(FIRST, 2),
            DecodeError::UnknownEncoding {
                index: 0,
                encoding: 2
            }
        ));
    }

    #[test]
    fn rejects_a_zero_duration() {
        assert!(matches!(
            corrupt(FIRST + 1, 0),
            DecodeError::ZeroDuration { index: 0 }
        ));
    }

    #[test]
    fn rejects_a_record_whose_length_runs_past_the_chunk() {
        let mut data = sample();
        // Lengthen the header's maximum too, so only the chunk end is in the way
        data[13] = 200;
        data[SECOND + 2] = 200;

        assert!(matches!(
            decode(&data),
            Err(DecodeError::Truncated { index: 1 })
        ));
    }

    #[test]
    fn rejects_a_zero_count_rle_opcode() {
        assert_eq!(sample()[SECOND], ENCODING_RLE);
        assert!(matches!(
            corrupt(SECOND + FRAME_HEADER_LEN, 0),
            DecodeError::BadRle { index: 1 }
        ));
    }

    #[test]
    fn rejects_a_frame_which_unpacks_to_the_wrong_length() {
        assert!(matches!(
            corrupt(SECOND + FRAME_HEADER_LEN, 4),
            DecodeError::FrameLength {
                index: 1,
                expected: 9,
                actual: 12
            }
        ));
    }

    #[test]
    fn rejects_flags_the_firmware_does_not_know() {
        assert!(matches!(
            corrupt(8, 1),
            DecodeError::UnknownFlags { flags: 1 }
        ));
    }

    #[test]
    fn rejects_a_frame_longer_than_the_header_allows() {
        // The first record is 9 raw bytes and the second packs down to 4
        assert_eq!(&sample()[13..15], &9u16.to_le_bytes());
        assert!(matches!(
            corrupt(13, 8),
            DecodeError::FrameTooLong {
                index: 0,
                len: 9,
                max: 8
            }
        ));
    }

    #[test]
    fn accepts_a_max_encoded_length_larger_than_any_frame_as_the_firmware_does() {
        let mut data = sample();
        data[13..15].copy_from_slice(&200u16.to_le_bytes());

        assert_eq!(decode(&data).unwrap(), decode(&sample()).unwrap());
    }

    #[test]
    fn rejects_a_zero_section_count() {
        assert!(matches!(corrupt(24, 0), DecodeError::NoSections));
    }

    #[test]
    fn rejects_a_sections_chunk_too_short_for_a_section() {
        let sample = sample();
        let mut data = sample[..HEADER_LEN].to_vec();
        data[16..20].copy_from_slice(&1u32.to_le_bytes());
        data.push(0);
        data.extend_from_slice(&sample[FIRST..]);

        assert!(matches!(
            decode(&data),
            Err(DecodeError::SectionsTooShort { len: 1 })
        ));
    }

    #[test]
    fn rejects_a_sections_chunk_that_does_not_end_in_nul() {
        assert!(matches!(
            corrupt(FIRST - 1, b'x'),
            DecodeError::SectionsUnterminated
        ));
    }

    #[test]
    fn rejects_a_first_section_that_is_not_the_default_one() {
        let name = HEADER_LEN + 13;
        let error = corrupt(name, b'D');

        assert!(matches!(&error, DecodeError::DefaultSection { name, .. } if name == "Default"));
        assert_eq!(
            error.to_string(),
            "the first section is \"Default\" over display frames 0..=3 at offset 57, \
             but the default section must cover 0..=3 at offset 57"
        );
    }

    #[test]
    fn rejects_a_default_section_with_the_wrong_range_or_offset() {
        assert!(matches!(
            corrupt(HEADER_LEN, 1),
            DecodeError::DefaultSection { start: 1, .. }
        ));
        assert!(matches!(
            corrupt(HEADER_LEN + 4, 9),
            DecodeError::DefaultSection { end: 9, .. }
        ));
        assert!(matches!(
            corrupt(HEADER_LEN + 8, 0),
            DecodeError::DefaultSection { frame_offs: 0, .. }
        ));
    }

    /// The sample with a second section named `extra` and the header's section count set
    fn with_extra_section(count: u32) -> Vec<u8> {
        let sample = sample();
        let extra_len = SECTION_FIXED_LEN + b"extra\0".len();
        let mut extra = Vec::with_capacity(extra_len);
        extra.extend(0u32.to_le_bytes());
        extra.extend(3u32.to_le_bytes());
        extra.extend(((FIRST + extra_len) as u32).to_le_bytes());
        extra.push(1);
        extra.extend_from_slice(b"extra\0");

        let mut data = sample[..FIRST].to_vec();
        data.extend_from_slice(&extra);
        data.extend_from_slice(&sample[FIRST..]);
        data[16..20].copy_from_slice(&((SECTION_LEN + extra.len()) as u32).to_le_bytes());
        data[24..28].copy_from_slice(&count.to_le_bytes());
        // The default section's frame offset moves with the longer chunk
        data[HEADER_LEN + 8..HEADER_LEN + 12]
            .copy_from_slice(&((FIRST + extra.len()) as u32).to_le_bytes());

        data
    }

    #[test]
    fn rejects_more_sections_than_the_header_declares() {
        assert!(matches!(
            decode(&with_extra_section(1)),
            Err(DecodeError::SectionCount { declared: 1 })
        ));
    }

    #[test]
    fn skips_extra_sections_and_does_not_write_them_back() {
        let anim = decode(&with_extra_section(2)).unwrap();

        assert_eq!(anim, decode(&sample()).unwrap());
        assert_eq!(anim.encode().unwrap(), sample());
    }

    #[test]
    fn rejects_a_frame_count_that_disagrees_with_the_records() {
        assert!(matches!(
            corrupt(28, 3),
            DecodeError::FrameCount {
                declared: 3,
                found: 2
            }
        ));
    }

    #[test]
    fn rejects_a_display_frame_count_that_disagrees_with_the_durations() {
        let mut data = sample();
        // Move the default section's end with it, so only the durations disagree
        data[32] = 5;
        data[HEADER_LEN + 4] = 4;

        assert!(matches!(
            decode(&data),
            Err(DecodeError::DisplayFrames {
                declared: 5,
                found: 4
            })
        ));
    }
}
