use busybar_anim::{Frame, PixelLayout, Target, decode, encode};

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn packed_frames(dir: &str, stem: &str, count: usize, layout: PixelLayout) -> Vec<Vec<u8>> {
    (0..count)
        .map(|i| {
            let rgba = image::load_from_memory(&fixture(&format!("{dir}/{stem}_{i}.png")))
                .unwrap()
                .into_rgba8();
            layout.pack_rgba(rgba.as_raw())
        })
        .collect()
}

fn encode_sequence(target: Target, fps: u8, dir: &str, stem: &str, count: usize) -> Vec<u8> {
    let frames: Vec<Frame> = packed_frames(dir, stem, count, target.layout())
        .into_iter()
        .map(Frame::new)
        .collect();

    encode(target, fps, &frames).unwrap()
}

#[test]
fn tracks_rgb888_matches_seq2anim() {
    let out = encode_sequence(Target::FRONT, 1, "tracks", "tracks", 4);

    assert_eq!(out, fixture("golden/tracks_72x16.anim"));
}

#[test]
fn tracks_gray4_matches_seq2anim() {
    let target = Target::new(72, 16, PixelLayout::Gray4).unwrap();
    let out = encode_sequence(target, 1, "tracks", "tracks", 4);

    assert_eq!(out, fixture("golden/tracks_gray_72x16.anim"));
}

#[test]
fn train_argb8888_matches_seq2anim() {
    let target = Target::new(203, 15, PixelLayout::Bgra8888).unwrap();
    let out = encode_sequence(target, 30, "train", "train", 4);

    assert_eq!(out, fixture("golden/train_203x15.anim"));
}

#[test]
fn every_golden_file_survives_a_decode_encode_round_trip() {
    for name in [
        "tracks_72x16",
        "tracks_gray_72x16",
        "train_203x15",
        "gifrgb_72x16",
        "gifgray_72x16",
    ] {
        let golden = fixture(&format!("golden/{name}.anim"));
        let anim = decode(&golden).unwrap_or_else(|e| panic!("{name}: {e}"));

        assert_eq!(anim.encode().unwrap(), golden, "{name}");
    }
}

#[test]
fn decode_recovers_the_packed_frames() {
    let anim = decode(&fixture("golden/train_203x15.anim")).unwrap();

    assert_eq!(
        anim.target(),
        Target::new(203, 15, PixelLayout::Bgra8888).unwrap()
    );
    assert_eq!(anim.fps(), 30);

    let packed: Vec<Frame> = packed_frames("train", "train", 4, PixelLayout::Bgra8888)
        .into_iter()
        .map(Frame::new)
        .collect();

    assert_eq!(anim.frames(), packed);
}
