//! Run-length encoding compatible with the firmware's `toolbox/rle_encode`.
//!
//! Input is viewed as blocks of `blk_size` bytes. Each opcode byte either
//! introduces `n` verbatim blocks (`0x80 | n`) or one block to repeat `n`
//! times (`n`). `n` never exceeds 127.

const MAX_BLOCKS_PER_OPCODE: usize = 127;
const RUN_THRESHOLD: usize = 3;

/// Compress `source` as the reference encoder does, byte for byte.
///
/// # Panics
/// If `blk_size` is zero or `source.len()` is not a multiple of it.
pub fn compress(source: &[u8], blk_size: usize) -> Vec<u8> {
    assert!(blk_size > 0, "block size must be positive");
    assert!(
        source.len().is_multiple_of(blk_size),
        "source length must be a multiple of the block size"
    );
    let count = source.len() / blk_size;
    let block = |j: usize| &source[j * blk_size..(j + 1) * blk_size];
    let mut dest = Vec::with_capacity(source.len() + count / MAX_BLOCKS_PER_OPCODE + 1);
    let mut i = 0;

    while i < count {
        let run = (i..count)
            .take_while(|&j| block(j) == block(i))
            .take(MAX_BLOCKS_PER_OPCODE)
            .count();

        if run >= RUN_THRESHOLD {
            dest.push(run as u8);
            dest.extend_from_slice(block(i));
            i += run;
            continue;
        }

        // Verbatim until a run longer than the threshold begins. Like the
        // reference, a run that is exactly at the threshold gets swallowed
        // into the verbatim stretch rather than starting a repeat opcode.
        let mut verbatim = 0;
        let mut pending = 0;
        for j in i..count {
            if j + 1 < count && block(j + 1) == block(j) {
                pending += 1;
                if pending > RUN_THRESHOLD {
                    break;
                }
            } else {
                verbatim += 1 + pending;
                pending = 0;
                if verbatim >= MAX_BLOCKS_PER_OPCODE {
                    break;
                }
            }
        }
        let verbatim = (verbatim + pending).min(MAX_BLOCKS_PER_OPCODE);
        dest.push(0x80 | verbatim as u8);
        dest.extend_from_slice(&source[i * blk_size..(i + verbatim) * blk_size]);
        i += verbatim;
    }

    dest
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inverse of [`compress`], only needed to check round trips.
    fn decompress(source: &[u8], blk_size: usize) -> Vec<u8> {
        let mut dest = Vec::new();
        let mut i = 0;
        while i < source.len() {
            let opcode = source[i];
            let count = usize::from(opcode & 0x7F);
            i += 1;
            if opcode & 0x80 != 0 {
                dest.extend_from_slice(&source[i..i + count * blk_size]);
                i += count * blk_size;
            } else {
                for _ in 0..count {
                    dest.extend_from_slice(&source[i..i + blk_size]);
                }
                i += blk_size;
            }
        }
        dest
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    // Vectors produced by busybar-firmware/scripts/flipper/rle.py
    #[test]
    fn matches_reference_vectors() {
        let cases = [
            ("", 1, ""),
            ("07", 1, "8107"),
            ("0707070707", 1, "0507"),
            ("010203", 1, "83010203"),
            // the reference folds the first 4 of a repeat into the verbatim run
            ("41425858585858585858", 1, "864142585858580458"),
            ("deadbedeadbedeadbedeadbe010203", 3, "04deadbe81010203"),
            ("41414242", 1, "8441414242"),
        ];
        for (src, blk, want) in cases {
            assert_eq!(compress(&hex(src), blk), hex(want), "src={src} blk={blk}");
        }
    }

    #[test]
    fn caps_runs_at_127_blocks() {
        assert_eq!(compress(&[9; 130], 1), hex("7f090309"));
    }

    #[test]
    fn caps_verbatim_at_127_blocks() {
        let src: Vec<u8> = (0..200).collect();
        let out = compress(&src, 1);
        assert_eq!(out[0], 0xFF);
        assert_eq!(&out[1..128], &src[..127]);
        assert_eq!(out[128], 0x80 | 73);
        assert_eq!(&out[129..], &src[127..]);
    }

    // Also from rle.py: a short run that straddles the 127-block verbatim cap
    // is swallowed up to the cap, and whatever is left starts over.
    #[test]
    fn matches_reference_across_the_verbatim_cap() {
        let cases: [(usize, usize, &str, &str); 4] = [
            // (distinct prefix, 0xAA repeats, tail hex, expected after the 127-block verbatim)
            (125, 3, "f0f1", "83aaf0f1"),
            (126, 4, "f0", "03aa81f0"),
            (127, 3, "", "03aa"),
            (124, 5, "", "82aaaa"),
        ];
        for (distinct, repeats, tail, rest) in cases {
            let mut src: Vec<u8> = (0..distinct as u8).collect();
            src.extend(std::iter::repeat_n(0xAA, repeats));
            src.extend(hex(tail));
            let mut want = vec![0xFF];
            want.extend_from_slice(&src[..127]);
            want.extend(hex(rest));
            assert_eq!(
                compress(&src, 1),
                want,
                "distinct={distinct} repeats={repeats}"
            );
        }
    }

    #[test]
    fn round_trips_noisy_data_for_every_block_size() {
        // A small LCG keeps the noise deterministic without a rand dependency.
        let mut state: u32 = 0x1234_5678;
        let mut next = move || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        };
        for blk in 1..=8 {
            let mut src: Vec<u8> = (0..512 - 512 % blk).map(|_| next()).collect();
            // plant a few runs so RLE has something to do
            for start in [17, 200, 400] {
                for (j, b) in src[start..start + 8 * blk].iter_mut().enumerate() {
                    *b = (j % blk) as u8;
                }
            }
            assert_eq!(decompress(&compress(&src, blk), blk), src, "blk={blk}");
        }
    }
}
