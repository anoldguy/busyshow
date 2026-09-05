//! Pixel layouts and the displays they are laid out for

use std::fmt;

/// Pixel layout of a frame, named as the firmware names it
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelLayout {
    /// 3 bytes per pixel, stored B, G, R
    Bgr888,
    /// 2 pixels per byte, the first in the high nibble
    Gray4,
    /// 4 bytes per pixel, stored B, G, R, A
    Bgra8888,
}

impl PixelLayout {
    pub(crate) fn id(self) -> u8 {
        match self {
            Self::Bgr888 => 0,
            Self::Gray4 => 1,
            Self::Bgra8888 => 2,
        }
    }

    pub(crate) fn from_id(id: u8) -> Option<Self> {
        match id {
            0 => Some(Self::Bgr888),
            1 => Some(Self::Gray4),
            2 => Some(Self::Bgra8888),
            _ => None,
        }
    }

    pub(crate) fn block_size(self) -> usize {
        match self {
            Self::Bgr888 => 3,
            Self::Gray4 => 1,
            Self::Bgra8888 => 4,
        }
    }

    pub(crate) fn frame_len(self, width: u8, height: u8) -> usize {
        let pixels = usize::from(width) * usize::from(height);

        match self {
            Self::Bgr888 => pixels * 3,
            Self::Gray4 => pixels.div_ceil(2),
            Self::Bgra8888 => pixels * 4,
        }
    }

    /// Pack 8-bit RGBA pixels into this layout
    ///
    /// `Gray4` keeps the top four bits of the red channel, so convert to gray first.
    ///
    /// # Panics
    ///
    /// If `rgba` is not a whole number of four-byte pixels.
    pub fn pack_rgba(self, rgba: &[u8]) -> Vec<u8> {
        assert!(
            rgba.len().is_multiple_of(4),
            "rgba input must be whole pixels of four bytes"
        );

        match self {
            Self::Bgr888 => rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[2], p[1], p[0]])
                .collect(),
            Self::Bgra8888 => rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[2], p[1], p[0], p[3]])
                .collect(),
            Self::Gray4 => rgba
                .chunks(8)
                .map(|pair| (pair[0] & 0xF0) | pair.get(4).map_or(0, |r| r >> 4))
                .collect(),
        }
    }

    /// Pack 8-bit RGB pixels into this layout
    ///
    /// Pixels run row by row from the top left. `Gray4` keeps the top four bits of the
    /// red channel, so convert to gray first.
    ///
    /// # Panics
    ///
    /// If `rgb` is not a whole number of three-byte pixels.
    pub fn pack_rgb(self, rgb: &[u8]) -> Vec<u8> {
        assert!(
            rgb.len().is_multiple_of(3),
            "rgb input must be whole pixels of three bytes"
        );

        let rgba: Vec<u8> = rgb
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect();

        self.pack_rgba(&rgba)
    }
}

impl fmt::Display for PixelLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bgr888 => f.write_str("bgr888"),
            Self::Gray4 => f.write_str("l4"),
            Self::Bgra8888 => f.write_str("bgra8888"),
        }
    }
}

/// Size and pixel layout of a display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Target {
    width: u8,
    height: u8,
    layout: PixelLayout,
}

impl Target {
    /// The 72x16 color front display
    pub const FRONT: Target = Target {
        width: 72,
        height: 16,
        layout: PixelLayout::Bgr888,
    };

    /// The 160x80 16-level gray back display
    pub const BACK: Target = Target {
        width: 160,
        height: 80,
        layout: PixelLayout::Gray4,
    };

    /// A display of `width` by `height` pixels in `layout`, or `None` for a shape no frame can have
    ///
    /// Both dimensions must be non-zero, and `Gray4` packs two pixels per byte, so it needs
    /// an even number of them.
    pub const fn new(width: u8, height: u8, layout: PixelLayout) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }

        let pixels = width as usize * height as usize;

        if matches!(layout, PixelLayout::Gray4) && !pixels.is_multiple_of(2) {
            return None;
        }

        Some(Self {
            width,
            height,
            layout,
        })
    }

    /// Width in pixels
    pub fn width(self) -> u8 {
        self.width
    }

    /// Height in pixels
    pub fn height(self) -> u8 {
        self.height
    }

    /// Pixel layout of each frame
    pub fn layout(self) -> PixelLayout {
        self.layout
    }

    /// Pixels in one frame
    pub fn pixels(self) -> usize {
        usize::from(self.width) * usize::from(self.height)
    }

    /// Bytes in one packed frame
    pub fn frame_len(self) -> usize {
        self.layout.frame_len(self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_len_follows_the_layout() {
        assert_eq!(PixelLayout::Bgr888.frame_len(72, 16), 72 * 16 * 3);
        assert_eq!(PixelLayout::Bgra8888.frame_len(203, 15), 203 * 15 * 4);
        assert_eq!(PixelLayout::Gray4.frame_len(160, 80), 160 * 80 / 2);
        assert_eq!(Target::FRONT.frame_len(), 72 * 16 * 3);
        assert_eq!(Target::BACK.frame_len(), 160 * 80 / 2);
    }

    #[test]
    fn ids_round_trip() {
        for layout in [
            PixelLayout::Bgr888,
            PixelLayout::Gray4,
            PixelLayout::Bgra8888,
        ] {
            assert_eq!(PixelLayout::from_id(layout.id()), Some(layout));
        }
        assert_eq!(PixelLayout::from_id(3), None);
    }

    #[test]
    fn displays_as_the_sibling_crate_does() {
        assert_eq!(PixelLayout::Bgr888.to_string(), "bgr888");
        assert_eq!(PixelLayout::Gray4.to_string(), "l4");
        assert_eq!(PixelLayout::Bgra8888.to_string(), "bgra8888");
    }

    #[test]
    fn a_target_needs_both_dimensions() {
        assert!(Target::new(0, 16, PixelLayout::Bgr888).is_none());
        assert!(Target::new(72, 0, PixelLayout::Bgr888).is_none());

        let target = Target::new(1, 1, PixelLayout::Bgr888).unwrap();

        assert_eq!(
            (target.width(), target.height(), target.layout()),
            (1, 1, PixelLayout::Bgr888)
        );
    }

    #[test]
    fn a_gray4_target_needs_an_even_number_of_pixels() {
        assert!(Target::new(3, 1, PixelLayout::Gray4).is_none());
        assert!(Target::new(3, 2, PixelLayout::Gray4).is_some());
        assert!(Target::new(3, 1, PixelLayout::Bgr888).is_some());
    }

    #[test]
    fn bgr888_stores_blue_first() {
        let rgba = [0x11, 0x22, 0x33, 0x44, 0xAA, 0xBB, 0xCC, 0xDD];

        assert_eq!(
            PixelLayout::Bgr888.pack_rgba(&rgba),
            [0x33, 0x22, 0x11, 0xCC, 0xBB, 0xAA]
        );
    }

    #[test]
    fn bgra8888_keeps_alpha_last() {
        let rgba = [0x11, 0x22, 0x33, 0x44];

        assert_eq!(
            PixelLayout::Bgra8888.pack_rgba(&rgba),
            [0x33, 0x22, 0x11, 0x44]
        );
    }

    #[test]
    fn gray4_packs_the_red_channel_two_per_byte_first_pixel_high() {
        let rgba = [0xAB, 0, 0, 255, 0xCD, 0, 0, 255, 0xEF, 0, 0, 255];

        assert_eq!(PixelLayout::Gray4.pack_rgba(&rgba), [0xAC, 0xE0]);
    }

    #[test]
    #[should_panic(expected = "whole pixels")]
    fn pack_rgba_wants_whole_pixels() {
        PixelLayout::Bgr888.pack_rgba(&[1, 2, 3]);
    }

    #[test]
    fn pack_rgb_bgr888_stores_blue_first() {
        let rgb = [0x11, 0x22, 0x33, 0xAA, 0xBB, 0xCC];

        assert_eq!(
            PixelLayout::Bgr888.pack_rgb(&rgb),
            [0x33, 0x22, 0x11, 0xCC, 0xBB, 0xAA]
        );
    }

    #[test]
    fn pack_rgb_bgra8888_fills_alpha_with_255() {
        let rgb = [0x11, 0x22, 0x33, 0xAA, 0xBB, 0xCC];

        assert_eq!(
            PixelLayout::Bgra8888.pack_rgb(&rgb),
            [0x33, 0x22, 0x11, 0xFF, 0xCC, 0xBB, 0xAA, 0xFF]
        );
    }

    #[test]
    fn pack_rgb_gray4_packs_the_red_channel_two_per_byte_first_pixel_high() {
        let rgb = [0xAB, 0, 0, 0xCD, 0, 0, 0xEF, 0, 0];

        assert_eq!(PixelLayout::Gray4.pack_rgb(&rgb), [0xAC, 0xE0]);
    }

    #[test]
    #[should_panic(expected = "whole pixels")]
    fn pack_rgb_wants_whole_pixels() {
        PixelLayout::Bgr888.pack_rgb(&[1, 2, 3, 4]);
    }
}
