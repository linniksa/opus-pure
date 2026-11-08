use super::*;

// Original core oracle; only reciprocal placement differs in the implementation under test.
fn original_core(
    ps_dec: &mut SilkDecoderState,
    ps_dec_ctrl: &mut SilkDecoderControl,
    xq: &mut [i16],
    pulses: &[i16],
) {
    let nlsf_interpolation_flag = if ps_dec.indices.nlsf_interp_coef_q2 < 4 {
        1
    } else {
        0
    };

    let offset_q10 = SILK_QUANTIZATION_OFFSETS_Q10[(ps_dec.indices.signal_type >> 1) as usize]
        [ps_dec.indices.quant_offset_type as usize] as i32;

    let mut rand_seed = ps_dec.indices.seed as i32;
    for i in 0..ps_dec.frame_length as usize {
        rand_seed = silk_rand(rand_seed);
        ps_dec.exc_q14[i] = (pulses[i] as i32) << 14;
        if ps_dec.exc_q14[i] > 0 {
            ps_dec.exc_q14[i] -= QUANT_LEVEL_ADJUST_Q10 << 4;
        } else if ps_dec.exc_q14[i] < 0 {
            ps_dec.exc_q14[i] += QUANT_LEVEL_ADJUST_Q10 << 4;
        }
        ps_dec.exc_q14[i] += offset_q10 << 4;
        if rand_seed < 0 {
            ps_dec.exc_q14[i] = -ps_dec.exc_q14[i];
        }
        rand_seed = silk_add32_ovflw(rand_seed, pulses[i] as i32);
    }

    let mut s_lpc_q14: [i32; MAX_SUB_FRAME_LENGTH + MAX_LPC_ORDER] =
        [0; MAX_SUB_FRAME_LENGTH + MAX_LPC_ORDER];
    s_lpc_q14[..MAX_LPC_ORDER].copy_from_slice(&ps_dec.s_lpc_q14_buf);

    let mut pexc_q14_idx: usize = 0;
    let mut pxq_idx: usize = 0;
    let mut s_ltp_buf_idx = ps_dec.ltp_mem_length;

    let s_ltp_q15_len = ps_dec.ltp_mem_length as usize + ps_dec.frame_length as usize;
    let s_ltp_len = ps_dec.ltp_mem_length as usize;

    const MAX_S_LTP_Q15: usize = 640;
    const MAX_S_LTP: usize = 320;
    debug_assert!(s_ltp_q15_len <= MAX_S_LTP_Q15);
    debug_assert!(s_ltp_len <= MAX_S_LTP);
    let mut s_ltp_q15_buf = [0i32; MAX_S_LTP_Q15];
    let s_ltp_q15 = &mut s_ltp_q15_buf[..s_ltp_q15_len];
    let mut s_ltp_buf = [0i16; MAX_S_LTP];
    let s_ltp = &mut s_ltp_buf[..s_ltp_len];

    for k in 0..ps_dec.nb_subfr as usize {
        // "Avoid abrupt transition from voiced PLC to unvoiced normal decoding"
        // (silk/decode_core.c:131). Concealment of a voiced frame leaves a
        // strongly periodic signal in the LTP history; decoding the next frame as
        // unvoiced with no long-term prediction at all cuts that off mid-pitch-
        // period, which is a click. libopus instead codes the first half of the
        // frame as voiced at the concealment's own lag with a fixed 0.25 LTP
        // gain, so the periodicity fades out rather than stopping. This decoder
        // took the lag and the signal type but left the LTP gains at the decoded
        // zeros, which is the transition it was meant to avoid.
        let transition = ps_dec.loss_cnt > 0
            && ps_dec.prev_signal_type == TYPE_VOICED
            && ps_dec.indices.signal_type as i32 != TYPE_VOICED
            && k < MAX_NB_SUBFR / 2;
        if transition {
            let b = &mut ps_dec_ctrl.ltp_coef_q14[k * LTP_ORDER..(k + 1) * LTP_ORDER];
            b.fill(0);
            b[LTP_ORDER / 2] = 1 << 12; // 0.25 in Q14
            ps_dec_ctrl.pitch_l[k] = ps_dec.lag_prev;
        }

        let a_q12 = &ps_dec_ctrl.pred_coef_q12[k >> 1];
        let b_q14 = &ps_dec_ctrl.ltp_coef_q14[k * LTP_ORDER..];
        let signal_type = ps_dec.indices.signal_type;

        let mut inv_gain_q31 = silk_inverse32_varq(ps_dec_ctrl.gains_q16[k], 47);

        let gain_adj_q16 = if ps_dec_ctrl.gains_q16[k] != ps_dec.prev_gain_q16 {
            let adj = silk_div32_varq(ps_dec.prev_gain_q16, ps_dec_ctrl.gains_q16[k], 16);

            for i in 0..MAX_LPC_ORDER {
                s_lpc_q14[i] = silk_smulww(adj, s_lpc_q14[i]);
            }
            adj
        } else {
            1 << 16
        };

        ps_dec.prev_gain_q16 = ps_dec_ctrl.gains_q16[k];

        let (eff_signal_type, eff_pitch_l) = if transition {
            (TYPE_VOICED, ps_dec.lag_prev)
        } else {
            (signal_type as i32, ps_dec_ctrl.pitch_l[k])
        };

        let mut lag = 0;
        if eff_signal_type == TYPE_VOICED {
            lag = eff_pitch_l;

            if k == 0 || (k == 2 && nlsf_interpolation_flag != 0) {
                let start_idx =
                    ps_dec.ltp_mem_length - lag - ps_dec.lpc_order - (LTP_ORDER / 2) as i32;
                debug_assert!(start_idx > 0);

                if k == 2 {
                    let copy_start = ps_dec.ltp_mem_length as usize;
                    let copy_len = 2 * ps_dec.subfr_length as usize;
                    ps_dec.out_buf[copy_start..copy_start + copy_len]
                        .copy_from_slice(&xq[0..copy_len]);
                }

                let filter_input_offset = start_idx as usize + k * ps_dec.subfr_length as usize;
                let filter_len = (ps_dec.ltp_mem_length - start_idx) as usize;
                silk_lpc_analysis_filter(
                    &mut s_ltp[start_idx as usize..],
                    &ps_dec.out_buf[filter_input_offset..],
                    a_q12,
                    filter_len,
                    ps_dec.lpc_order as usize,
                    0,
                );

                if k == 0 {
                    inv_gain_q31 =
                        silk_lshift(silk_smulwb(inv_gain_q31, ps_dec_ctrl.ltp_scale_q14), 2);
                }
                for i in 0..(lag + LTP_ORDER as i32 / 2) as usize {
                    s_ltp_q15[s_ltp_buf_idx as usize - i - 1] = silk_smulwb(
                        inv_gain_q31,
                        s_ltp[ps_dec.ltp_mem_length as usize - i - 1] as i32,
                    );
                }
            } else if gain_adj_q16 != (1 << 16) {
                for i in 0..(lag + LTP_ORDER as i32 / 2) as usize {
                    s_ltp_q15[s_ltp_buf_idx as usize - i - 1] =
                        silk_smulww(gain_adj_q16, s_ltp_q15[s_ltp_buf_idx as usize - i - 1]);
                }
            }
        }

        let mut res_q14: [i32; MAX_SUB_FRAME_LENGTH] = [0; MAX_SUB_FRAME_LENGTH];

        if eff_signal_type == TYPE_VOICED {
            let pred_lag_ptr_start = (s_ltp_buf_idx - lag + LTP_ORDER as i32 / 2) as usize;
            for i in 0..ps_dec.subfr_length as usize {
                let mut ltp_pred_q13: i32 = 2;
                for j in 0..LTP_ORDER {
                    ltp_pred_q13 = silk_smlawb(
                        ltp_pred_q13,
                        s_ltp_q15[pred_lag_ptr_start + i - j],
                        b_q14[j] as i32,
                    );
                }

                res_q14[i] = silk_add_lshift32(ps_dec.exc_q14[pexc_q14_idx + i], ltp_pred_q13, 1);

                s_ltp_q15[s_ltp_buf_idx as usize] = res_q14[i] << 1;
                s_ltp_buf_idx += 1;
            }
        } else {
            res_q14[..(ps_dec.subfr_length as usize)].copy_from_slice(
                &ps_dec.exc_q14[pexc_q14_idx..(ps_dec.subfr_length as usize + pexc_q14_idx)],
            );
        }

        let subfr_length = ps_dec.subfr_length as usize;
        let gain_q10 = ps_dec_ctrl.gains_q16[k] >> 6;
        // The order is 10 or 16 and nothing else, which is what the reference's
        // `celt_assert` says before it writes the taps out one by one.
        if ps_dec.lpc_order == 16 {
            lpc_synthesis::<16>(
                &mut s_lpc_q14,
                &res_q14[..subfr_length],
                a_q12,
                &mut xq[pxq_idx..pxq_idx + subfr_length],
                gain_q10,
            );
        } else {
            lpc_synthesis::<10>(
                &mut s_lpc_q14,
                &res_q14[..subfr_length],
                a_q12,
                &mut xq[pxq_idx..pxq_idx + subfr_length],
                gain_q10,
            );
        }

        for i in 0..MAX_LPC_ORDER {
            s_lpc_q14[i] = s_lpc_q14[ps_dec.subfr_length as usize + i];
        }
        pexc_q14_idx += ps_dec.subfr_length as usize;
        pxq_idx += ps_dec.subfr_length as usize;
    }

    ps_dec
        .s_lpc_q14_buf
        .copy_from_slice(&s_lpc_q14[..MAX_LPC_ORDER]);
}

fn fixture(fs: i32, subframes: i32, seed: u32) -> (SilkDecoderState, SilkDecoderControl, Vec<i16>) {
    let mut state = SilkDecoderState::default();
    state.fs_khz = fs;
    state.nb_subfr = subframes;
    state.subfr_length = 5 * fs;
    state.frame_length = subframes * state.subfr_length;
    state.ltp_mem_length = 20 * fs;
    state.lpc_order = if fs == 16 { 16 } else { 10 };
    state.lag_prev = 3 * fs;
    state.prev_gain_q16 = 65536;
    state.indices.seed = (seed % 4) as i8;
    state.indices.quant_offset_type = (seed % 2) as i8;
    for (i, v) in state.out_buf.iter_mut().enumerate() {
        *v = ((i as i32 * 71 + seed as i32 * 19) % 4001 - 2000) as i16;
    }
    for (i, v) in state.s_lpc_q14_buf.iter_mut().enumerate() {
        *v = (i as i32 * 137 - 1100) << 10;
    }
    state.exc_q14.fill(0x1234567);
    let mut ctrl = SilkDecoderControl::default();
    ctrl.pitch_l.fill(3 * fs);
    ctrl.ltp_scale_q14 = [15565, 12288, 8192, 16384][seed as usize % 4];
    for (i, v) in ctrl.ltp_coef_q14.iter_mut().enumerate() {
        *v = if i % LTP_ORDER == LTP_ORDER / 2 {
            4096
        } else {
            (i as i16 % 3 - 1) * 64
        };
    }
    for (half, a) in ctrl.pred_coef_q12.iter_mut().enumerate() {
        for (i, v) in a.iter_mut().enumerate() {
            *v = if i < state.lpc_order as usize {
                ((i as i16 + half as i16) % 5 - 2) * 32
            } else {
                0
            };
        }
    }
    let pulses = (0..state.frame_length as usize)
        .map(|i| ((i * 17 + seed as usize * 3) % 33) as i16 - 16)
        .collect();
    (state, ctrl, pulses)
}

fn gains(pattern: usize) -> [i32; MAX_NB_SUBFR] {
    match pattern {
        0 => [65536; MAX_NB_SUBFR],
        1 => [131072, 65536, 262144, 131072],
        2 => [1, 2, 32768, 65535],
        _ => [i32::MAX, 1 << 24, 1 << 30, i32::MAX],
    }
}

fn check(state: &mut SilkDecoderState, ctrl: &mut SilkDecoderControl, pulses: &[i16]) {
    let mut want_state = state.clone();
    let mut want_ctrl = ctrl.clone();
    let mut got = vec![12345; state.frame_length as usize + 7];
    let mut want = got.clone();
    original_core(&mut want_state, &mut want_ctrl, &mut want, pulses);
    silk_decode_core(state, ctrl, &mut got, pulses);
    assert_eq!(got, want, "PCM and output suffix");
    assert_eq!(
        state.exc_q14, want_state.exc_q14,
        "entire excitation buffer"
    );
    assert_eq!(state.s_lpc_q14_buf, want_state.s_lpc_q14_buf, "LPC state");
    assert_eq!(state.out_buf, want_state.out_buf, "entire staged history");
    assert_eq!(state.prev_gain_q16, want_state.prev_gain_q16, "gain state");
    assert_eq!(ctrl.pitch_l, want_ctrl.pitch_l);
    assert_eq!(ctrl.gains_q16, want_ctrl.gains_q16);
    assert_eq!(ctrl.pred_coef_q12, want_ctrl.pred_coef_q12);
    assert_eq!(ctrl.ltp_coef_q14, want_ctrl.ltp_coef_q14);
    assert_eq!(ctrl.ltp_scale_q14, want_ctrl.ltp_scale_q14);
}

#[test]
fn full_core_matches_original_across_rewhitening_and_bypassed_subframes() {
    let mut cases = 0;
    for fs in [8, 12, 16] {
        for subframes in [2, 4] {
            for signal_type in [0, 1, 2] {
                for interpolation in [2, 4] {
                    for loss in [0, 1] {
                        for pattern in 0..4 {
                            for seed in 0..4 {
                                let (mut state, mut ctrl, pulses) = fixture(fs, subframes, seed);
                                state.indices.signal_type = signal_type;
                                state.indices.nlsf_interp_coef_q2 = interpolation;
                                state.prev_signal_type = TYPE_VOICED;
                                state.loss_cnt = loss;
                                ctrl.gains_q16 = gains(pattern);
                                check(&mut state, &mut ctrl, &pulses);
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 1152);
}

#[test]
fn consecutive_core_states_match_original_across_signal_and_gain_transitions() {
    let mut frames = 0;
    for fs in [8, 12, 16] {
        for subframes in [2, 4] {
            for seed in 0..4 {
                let (mut state, mut ctrl, pulses) = fixture(fs, subframes, seed);
                for (step, signal_type) in [2, 2, 1, 0, 2, 1, 2, 0].into_iter().enumerate() {
                    state.indices.signal_type = signal_type;
                    state.indices.nlsf_interp_coef_q2 = if step % 2 == 0 { 2 } else { 4 };
                    state.loss_cnt = if step == 2 || step == 7 { 1 } else { 0 };
                    ctrl.gains_q16 = gains(step % 4);
                    check(&mut state, &mut ctrl, &pulses);
                    state.prev_signal_type = signal_type as i32;
                    frames += 1;
                }
            }
        }
    }
    assert_eq!(frames, 192);
}
