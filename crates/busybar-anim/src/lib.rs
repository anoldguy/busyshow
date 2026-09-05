//! Encoder and decoder for the BUSY Bar `.anim` animation container

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
