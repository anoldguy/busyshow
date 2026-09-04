//! Byte-for-byte comparisons against files produced by the reference
//! encoder, busybar-firmware/scripts/seq2anim.py.

use busybody::{ColorMode, Target, convert, encode};
use image::RgbaImage;

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn frames(dir: &str, stem: &str, count: usize) -> Vec<RgbaImage> {
    (0..count)
        .map(|i| {
            image::load_from_memory(&fixture(&format!("{dir}/{stem}_{i}.png")))
                .unwrap()
                .into_rgba8()
        })
        .collect()
}

#[test]
fn tracks_rgb888_matches_reference() {
    let out = encode(&frames("tracks", "tracks", 4), 1, ColorMode::Rgb888).unwrap();
    assert_eq!(out, fixture("golden/tracks_72x16.anim"));
}

#[test]
fn tracks_gray4_matches_reference() {
    let out = encode(&frames("tracks", "tracks", 4), 1, ColorMode::Gray4).unwrap();
    assert_eq!(out, fixture("golden/tracks_gray_72x16.anim"));
}

#[test]
fn train_argb8888_matches_reference() {
    let out = encode(&frames("train", "train", 4), 30, ColorMode::Argb8888).unwrap();
    assert_eq!(out, fixture("golden/train_203x15.anim"));
}

#[test]
fn gif_to_front_matches_reference() {
    let out = convert(&fixture("tracks_72x16.gif"), Target::FRONT).unwrap();
    assert_eq!(out, fixture("golden/gifrgb_72x16.anim"));
}

#[test]
fn gif_to_gray4_uses_luma_like_the_reference_pipeline() {
    let target = Target {
        color: ColorMode::Gray4,
        ..Target::FRONT
    };
    let out = convert(&fixture("tracks_72x16.gif"), target).unwrap();
    assert_eq!(out, fixture("golden/gifgray_72x16.anim"));
}

#[test]
fn gif_is_scaled_and_cropped_to_the_target() {
    let out = convert(&fixture("tracks_72x16.gif"), Target::BACK).unwrap();
    assert_eq!(&out[9..11], &[160, 80]);
}

// The WebP and APNG fixtures are lossless encodings of the tracks frames at
// 1000ms, so they must reproduce the reference bytes for those frames exactly.

#[test]
fn animated_webp_matches_reference() {
    let out = convert(&fixture("tracks_72x16.webp"), Target::FRONT).unwrap();
    assert_eq!(out, fixture("golden/tracks_72x16.anim"));
}

#[test]
fn apng_matches_reference() {
    let out = convert(&fixture("tracks_72x16.apng"), Target::FRONT).unwrap();
    assert_eq!(out, fixture("golden/tracks_72x16.anim"));
}
