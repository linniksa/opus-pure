use super::*;

fn normalize_guarded(rc: &mut RangeCoder<'_>) {
    let mut guard = 0u32;
    while rc.rng <= EC_CODE_BOT {
        guard += 1;
        if guard > 100 {
            rc.error = 1;
            rc.rng = EC_CODE_BOT + 1;
            break;
        }
        rc.nbits_total += EC_SYM_BITS as i32;
        rc.rng <<= EC_SYM_BITS;
        let sym = rc.rem;
        rc.rem = rc.read_byte() as i32;
        let combined_sym = ((sym << EC_SYM_BITS) | rc.rem) >> (EC_SYM_BITS - EC_CODE_EXTRA);
        rc.val = (rc.val << EC_SYM_BITS).wrapping_add(EC_SYM_MAX & !combined_sym as u32)
            & (EC_CODE_TOP - 1);
    }
}

#[test]
fn normalization_matches_guarded_state_at_boundaries_and_random_ranges() {
    let bytes: Vec<u8> = (0..257).map(|i| (i * 73 + 19) as u8).collect();
    let mut ranges = vec![
        0,
        1,
        EC_CODE_BOT - 1,
        EC_CODE_BOT,
        EC_CODE_BOT + 1,
        u32::MAX,
    ];
    for bit in 0..32 {
        let power = 1u32 << bit;
        ranges.extend([power - 1, power, power + 1]);
    }
    let mut seed = 0x9e3779b9u32;
    for _ in 0..512 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        ranges.extend([seed, seed & (EC_CODE_BOT - 1)]);
    }
    for len in [0, 1, 2, 3, 99, 100, 101, 257] {
        for offs in [0, len / 2, len] {
            for &rng in &ranges {
                let mut reference = RangeCoder::new_decoder(&bytes[..len]);
                reference.rng = rng;
                reference.val = rng.rotate_left(13) & (EC_CODE_TOP - 1);
                reference.rem = (rng & 255) as i32;
                reference.nbits_total = 37;
                reference.offs = offs as u32;
                reference.error = 7;
                let mut candidate = reference.clone();
                normalize_guarded(&mut reference);
                candidate.normalize_decoder();
                assert_eq!(
                    (
                        candidate.rng,
                        candidate.val,
                        candidate.rem,
                        candidate.nbits_total,
                        candidate.offs,
                        candidate.error,
                    ),
                    (
                        reference.rng,
                        reference.val,
                        reference.rem,
                        reference.nbits_total,
                        reference.offs,
                        reference.error,
                    ),
                    "rng={rng:#x}, len={len}, offs={offs}"
                );
            }
        }
    }
}

#[test]
fn zero_range_preserves_hundred_updates_and_exhausted_input() {
    let bytes = [0xa5; 101];
    for len in [0, 1, 99, 100, 101] {
        let mut rc = RangeCoder::new_decoder(&bytes[..len]);
        rc.rng = 0;
        rc.offs = 0;
        rc.nbits_total = 37;
        rc.error = 0;
        rc.normalize_decoder();
        assert_eq!(rc.rng, EC_CODE_BOT + 1);
        assert_eq!(rc.offs, len.min(100) as u32);
        assert_eq!(rc.nbits_total, 37 + 100 * EC_SYM_BITS as i32);
        assert_eq!(rc.error, 1);
    }
}
