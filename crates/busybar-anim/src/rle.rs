//! Run-length encoding compatible with the firmware's `toolbox/rle_encode`

const MAX_BLOCKS_PER_OPCODE: usize = 127;
const RUN_THRESHOLD: usize = 3;
/// The reference's literal scanner only notices a run after `RUN_THRESHOLD + 1` equal
/// pairs, so this many blocks of the run are already in the literal stretch by then
const LITERAL_OVERSHOOT: usize = RUN_THRESHOLD + 1;

/// Compress `source` in blocks of `blk_size` bytes, byte for byte as the reference encoder does
pub(crate) fn compress(source: &[u8], blk_size: usize) -> Vec<u8> {
    assert!(blk_size > 0, "block size must be positive");
    assert!(
        source.len().is_multiple_of(blk_size),
        "source length must be a multiple of the block size"
    );

    let count = source.len() / blk_size;
    let block = |j: usize| &source[j * blk_size..(j + 1) * blk_size];
    let starts_run = |j: usize| {
        j + LITERAL_OVERSHOOT < count
            && (j + 1..=j + LITERAL_OVERSHOOT).all(|k| block(k) == block(j))
    };
    let mut dest = Vec::with_capacity(source.len() + count / MAX_BLOCKS_PER_OPCODE + 1);
    let mut i = 0;

    while i < count {
        let run = (i..count)
            .take_while(|&j| block(j) == block(i))
            .take(MAX_BLOCKS_PER_OPCODE)
            .count();

        let blocks = if run >= RUN_THRESHOLD {
            dest.push(run as u8);
            dest.extend_from_slice(block(i));
            run
        } else {
            // Literal up to the next run the reference would notice, swallowing its
            // first LITERAL_OVERSHOOT blocks, or to the end of the data
            let literal = (i..count)
                .take(MAX_BLOCKS_PER_OPCODE)
                .position(starts_run)
                .map_or(count - i, |at| at + LITERAL_OVERSHOOT)
                .min(MAX_BLOCKS_PER_OPCODE);
            dest.push(0x80 | literal as u8);
            dest.extend_from_slice(&source[i * blk_size..(i + literal) * blk_size]);
            literal
        };

        i += blocks;
    }

    dest
}

/// Inverse of [`compress`], or `None` for input the encoder could not have produced
pub(crate) fn decompress(source: &[u8], blk_size: usize) -> Option<Vec<u8>> {
    assert!(blk_size > 0, "block size must be positive");

    let mut dest = Vec::with_capacity(source.len() * 2);
    let mut rest = source;

    while let Some((&opcode, tail)) = rest.split_first() {
        let count = usize::from(opcode & 0x7F);

        if count == 0 {
            return None;
        }

        if opcode & 0x80 != 0 {
            let (blocks, tail) = tail.split_at_checked(count * blk_size)?;
            dest.extend_from_slice(blocks);
            rest = tail;
        } else {
            let (block, tail) = tail.split_at_checked(blk_size)?;
            for _ in 0..count {
                dest.extend_from_slice(block);
            }
            rest = tail;
        }
    }

    Some(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Literal port of the scanner in the firmware's `assets/frontend/util/seq2anim.ts`,
    /// kept here so `compress` can be written in whatever way reads best
    fn reference_compress(source: &[u8], blk_size: usize) -> Vec<u8> {
        let count = source.len() / blk_size;
        let block = |j: usize| &source[j * blk_size..(j + 1) * blk_size];
        let mut dest = Vec::new();
        let mut i = 0;

        while i < count {
            let run = (i..count)
                .take_while(|&j| block(j) == block(i))
                .count()
                .min(MAX_BLOCKS_PER_OPCODE);

            if run >= RUN_THRESHOLD {
                dest.push(run as u8);
                dest.extend_from_slice(block(i));
                i += run;
                continue;
            }

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
                }
            }

            let verbatim = (verbatim + pending).min(MAX_BLOCKS_PER_OPCODE);
            dest.push(0x80 | verbatim as u8);
            dest.extend_from_slice(&source[i * blk_size..(i + verbatim) * blk_size]);
            i += verbatim;
        }

        dest
    }

    #[test]
    fn compress_matches_a_literal_port_of_the_reference_scanner() {
        let mut state: u32 = 0xDEAD_BEEF;
        let mut next = move |modulus: u32| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((state >> 24) % modulus) as u8
        };

        // Small alphabets make runs of every length common, including runs that
        // straddle the 127 cap and runs of exactly 3, 4 and 5 blocks
        for blk in 1..=4 {
            for alphabet in [1, 2, 3, 5, 16] {
                for len in [
                    0, 1, 2, 3, 5, 8, 127, 128, 129, 131, 254, 255, 256, 300, 700,
                ] {
                    let src: Vec<u8> = (0..len * blk).map(|_| next(alphabet)).collect();

                    assert_eq!(
                        compress(&src, blk),
                        reference_compress(&src, blk),
                        "blk={blk} alphabet={alphabet} len={len}"
                    );
                }
            }
        }
    }

    #[test]
    fn matches_the_reference_vectors_from_rle_py() {
        let cases = [
            ("", 1, ""),
            ("07", 1, "8107"),
            ("0707070707", 1, "0507"),
            ("010203", 1, "83010203"),
            ("41425858585858585858", 1, "864142585858580458"),
            ("deadbedeadbedeadbedeadbe010203", 3, "04deadbe81010203"),
            ("41414242", 1, "8441414242"),
        ];

        for (src, blk, want) in cases {
            assert_eq!(compress(&hex(src), blk), hex(want), "src={src} blk={blk}");
            assert_eq!(
                decompress(&hex(want), blk),
                Some(hex(src)),
                "inverse of {want}"
            );
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

    #[test]
    fn a_short_run_straddling_the_verbatim_cap_is_swallowed_like_the_reference() {
        let cases: [(usize, usize, &str, &str); 4] = [
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
        let mut state: u32 = 0x1234_5678;
        let mut next = move || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        };

        for blk in 1..=8 {
            let mut src: Vec<u8> = (0..512 - 512 % blk).map(|_| next()).collect();

            for start in [17, 200, 400] {
                for (j, b) in src[start..start + 8 * blk].iter_mut().enumerate() {
                    *b = (j % blk) as u8;
                }
            }

            assert_eq!(
                decompress(&compress(&src, blk), blk),
                Some(src),
                "blk={blk}"
            );
        }
    }

    #[test]
    fn decompress_rejects_truncated_input() {
        assert_eq!(decompress(&hex("830102"), 1), None);
        assert_eq!(decompress(&hex("05"), 1), None);
        assert_eq!(decompress(&hex("04dead"), 3), None);
    }

    #[test]
    fn decompress_rejects_a_zero_count_opcode_as_corruption() {
        assert_eq!(decompress(&hex("00"), 1), None);
        assert_eq!(decompress(&hex("80"), 1), None);
    }
}
