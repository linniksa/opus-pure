use super::*;
fn mirror_reference(
    f2: &[KissCpx],
    trig: &[f32],
    output: &mut [f32],
    n4: usize,
    n2: usize,
    overlap2: usize,
) {
    for i in 0..((n4 + 1) >> 1) {
        let im0 = f2[i].r;
        let re0 = f2[i].i;
        let t0_0 = trig[i];
        let t1_0 = trig[n4 + i];

        let yr0 = re0 * t0_0 + im0 * t1_0;
        let yi0 = re0 * t1_0 - im0 * t0_0;

        let j = n4 - 1 - i;
        let im1 = f2[j].r;
        let re1 = f2[j].i;
        let t0_1 = trig[j];
        let t1_1 = trig[n4 + j];

        let yr1 = re1 * t0_1 + im1 * t1_1;
        let yi1 = re1 * t1_1 - im1 * t0_1;

        output[overlap2 + 2 * i] = yr0;
        output[overlap2 + n2 - 1 - 2 * i] = yi0;
        output[overlap2 + n2 - 2 - 2 * i] = yr1;
        output[overlap2 + 2 * i + 1] = yi1;
    }
}

#[test]
fn exact_bits_and_unwritten_edges_for_all_opus_sizes_and_tails() {
    for n4 in [1, 2, 3, 4, 8, 15, 16, 30, 60, 120, 240, 480] {
        for overlap2 in [0, 1, 4, 60] {
            for scale in [0.0, f32::MIN_POSITIVE / 16.0, 0.5, 16.0] {
                let mut seed = 0x12345678u32;
                let mut next = || {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed as i32 as f32 / i32::MAX as f32) * scale
                };
                let f2: Vec<_> = (0..n4).map(|_| KissCpx::new(next(), next())).collect();
                let trig: Vec<_> = (0..n4 * 2).map(|_| next()).collect();
                let mut want = vec![f32::from_bits(0x7fc00123); overlap2 + 2 * n4 + 3];
                let mut got = want.clone();
                mirror_reference(&f2, &trig, &mut want, n4, 2 * n4, overlap2);
                mdct_backward_post_rotation_scalar(&f2, &trig, &mut got, n4, 2 * n4, overlap2);
                assert!(
                    got.iter()
                        .zip(&want)
                        .all(|(a, b)| a.to_bits() == b.to_bits()),
                    "n4={n4}, overlap={overlap2}, scale={scale}"
                );
            }
        }
    }
}
