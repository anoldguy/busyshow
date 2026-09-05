#![cfg(feature = "decoders")]

use busybar_anim::{PixelLayout, Target, convert, decode};

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn a_gif_for_the_front_matches_the_reference_pipeline() {
    let out = convert(&fixture("tracks_72x16.gif"), Target::FRONT).unwrap();

    assert_eq!(out, fixture("golden/gifrgb_72x16.anim"));
}

#[test]
fn a_gif_for_gray4_uses_luma_like_the_reference_pipeline() {
    let target = Target::new(72, 16, PixelLayout::Gray4).unwrap();
    let out = convert(&fixture("tracks_72x16.gif"), target).unwrap();

    assert_eq!(out, fixture("golden/gifgray_72x16.anim"));
}

#[test]
fn a_gif_is_scaled_and_cropped_to_the_target() {
    let out = convert(&fixture("tracks_72x16.gif"), Target::BACK).unwrap();
    let anim = decode(&out).unwrap();

    assert_eq!(anim.target(), Target::BACK);
    assert_eq!(anim.frames().len(), 4);
}

#[test]
fn a_lossless_animated_webp_of_the_tracks_frames_matches_seq2anim() {
    let out = convert(&fixture("tracks_72x16.webp"), Target::FRONT).unwrap();

    assert_eq!(out, fixture("golden/tracks_72x16.anim"));
}

#[test]
fn a_lossless_apng_of_the_tracks_frames_matches_seq2anim() {
    let out = convert(&fixture("tracks_72x16.apng"), Target::FRONT).unwrap();

    assert_eq!(out, fixture("golden/tracks_72x16.anim"));
}
