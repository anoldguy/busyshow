//! Encoder and decoder for the BUSY Bar `.anim` animation container
//!
//! Frames drawn in plain RGB pack into a display's layout through
//! [`Target::frame_from_rgb`], which also checks that the canvas is the right size:
//!
//! ```
//! use busybar_anim::{Target, encode};
//!
//! // Sweep a red bar across the front display, one column per frame
//! let mut frames = Vec::new();
//!
//! for column in 0..Target::FRONT.width() {
//!     let mut rgb = vec![0u8; Target::FRONT.pixels() * 3];
//!
//!     for row in 0..Target::FRONT.height() {
//!         let pixel = usize::from(row) * usize::from(Target::FRONT.width()) + usize::from(column);
//!         rgb[pixel * 3] = 0xFF;
//!     }
//!
//!     frames.push(Target::FRONT.frame_from_rgb(&rgb)?);
//! }
//!
//! let anim = encode(Target::FRONT, 30, &frames)?;
//! assert!(anim.starts_with(b"bicycle0"));
//! # Ok::<(), busybar_anim::EncodeError>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod anim;
#[cfg(feature = "decoders")]
mod convert;
mod decode;
mod layout;
mod rle;
mod timing;

pub use crate::anim::{EncodeError, Frame, encode};
#[cfg(feature = "decoders")]
pub use crate::convert::{ConvertError, convert};
pub use crate::decode::{Animation, DecodeError, decode};
pub use crate::layout::{PixelLayout, Target};
pub use crate::timing::FrameRate;
