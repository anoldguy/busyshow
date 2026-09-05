use busybar_anim::{PixelLayout, Target, decode, encode};

fn front_rgb(seed: u8) -> Vec<u8> {
    (0..Target::FRONT.pixels() * 3)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

#[test]
fn generated_rgb_frames_survive_an_encode_decode_round_trip() {
    let first = front_rgb(1);
    let second = front_rgb(2);
    let frames = [
        Target::FRONT.frame_from_rgb(&first).unwrap(),
        Target::FRONT
            .frame_from_rgb(&second)
            .unwrap()
            .with_repeats(3),
    ];

    let anim = decode(&encode(Target::FRONT, 30, &frames).unwrap()).unwrap();

    assert_eq!(anim.target(), Target::FRONT);
    assert_eq!(anim.fps(), 30);
    assert_eq!(anim.frames().len(), 2);
    assert_eq!(
        anim.frames()[0].pixels(),
        PixelLayout::Bgr888.pack_rgb(&first)
    );
    assert_eq!(
        anim.frames()[1].pixels(),
        PixelLayout::Bgr888.pack_rgb(&second)
    );
    assert_eq!(anim.frames()[1].repeats(), 3);
}
