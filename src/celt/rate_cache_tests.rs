use super::*;
use crate::celt::modes::{CACHE_INDEX50, default_mode};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn reference_bits2pulses(m: &CeltMode, band: usize, mut lm: i32, bits: i32) -> i32 {
    lm += 1;
    let idx = lm as usize * m.nb_ebands + band;
    let cache_index = m.cache.index[idx];
    if cache_index < 0 {
        return 0;
    }
    // `cache[0]` is the highest index the search can reach, so trimming the
    // slice to that length up front bounds every read in the loop below and
    // costs one check instead of one per iteration.
    let cache = &m.cache.bits[cache_index as usize..];
    let mut hi = cache[0] as i32;
    let cache = &cache[..=hi as usize];

    let mut lo = 0i32;
    let bits = bits - 1; // bits--

    for _ in 0..6 {
        // LOG_MAX_PSEUDO = 6
        let mid = (lo + hi + 1) >> 1; // round up, matches C
        if cache[mid as usize] as i32 >= bits {
            hi = mid;
        } else {
            lo = mid;
        }
    }

    let lo_val = if lo == 0 {
        -1i32
    } else {
        cache[lo as usize] as i32
    };
    let hi_val = cache[hi as usize] as i32;
    if bits - lo_val <= hi_val - bits {
        lo
    } else {
        hi
    }
}

#[test]
fn pulse_budget_search_matches_original_search() {
    let m = default_mode();
    let mut cases = 0;
    for lm in -1..=m.max_lm as i32 {
        for band in 0..m.nb_ebands {
            for bits in -32..=16383 {
                assert_eq!(
                    bits2pulses(m, band, lm, bits),
                    reference_bits2pulses(m, band, lm, bits),
                    "lm={lm} band={band} bits={bits}"
                );
                cases += 1;
            }
            for bits in [
                i32::MIN,
                i32::MIN + 1,
                i32::MIN + 255,
                -1,
                0,
                1,
                i32::MAX - 1,
                i32::MAX,
            ] {
                let actual = catch_unwind(AssertUnwindSafe(|| bits2pulses(m, band, lm, bits)))
                    .map_err(|_| ());
                let expected = catch_unwind(AssertUnwindSafe(|| {
                    reference_bits2pulses(m, band, lm, bits)
                }))
                .map_err(|_| ());
                assert_eq!(actual, expected, "lm={lm} band={band} bits={bits}");
                cases += 1;
            }
        }
    }
    assert_eq!(cases, CACHE_INDEX50.len() * (16416 + 8));
}

#[test]
fn zero_budget_matches_original_search_for_every_cache_header() {
    let mut m = CeltMode::new_48000_960_120();
    m.cache.index = &[0];
    let mut cases = 0;
    for hi in 0..=255u8 {
        for mut seed in [0u32, 1, 0x80000000, u32::MAX] {
            let mut cache = vec![hi; hi as usize + 1];
            for value in &mut cache[1..] {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                *value = (seed >> 24) as u8;
            }
            m.cache.bits = Box::leak(cache.into_boxed_slice());
            assert_eq!(reference_bits2pulses(&m, 0, -1, 0), 0);
            assert_eq!(bits2pulses(&m, 0, -1, 0), 0);
            cases += 1;
        }
    }
    assert_eq!(cases, 1024);
}

#[test]
fn zero_budget_keeps_cache_validation() {
    let mut m = CeltMode::new_48000_960_120();
    let invalid: [(&[i16], &[u8]); 4] = [(&[], &[0]), (&[1], &[0]), (&[0], &[]), (&[0], &[2, 7])];
    for (index, bits) in invalid {
        m.cache.index = index;
        m.cache.bits = bits;
        assert!(catch_unwind(AssertUnwindSafe(|| reference_bits2pulses(&m, 0, -1, 0))).is_err());
        assert!(catch_unwind(AssertUnwindSafe(|| bits2pulses(&m, 0, -1, 0))).is_err());
    }
    m.cache.index = &[-1];
    m.cache.bits = &[];
    assert_eq!(bits2pulses(&m, 0, -1, 0), 0);
}
