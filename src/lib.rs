//! Convert animated images into the BUSY Bar `.anim` container.

mod anim;
mod convert;
mod rle;

pub use anim::{ColorMode, EncodeError, encode};
pub use convert::{Error, Target, convert};
