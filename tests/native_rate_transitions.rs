//! Synthetic mode switches encoded by libopus; see fixtures/TRANSITIONS.md.
use opus_pure::OpusDecoder;

#[test]
fn native_rate_redundancy_keeps_the_reference_range() {
    for (coded, fixture) in [
        (1, include_str!("fixtures/transition-1ch.txt")),
        (2, include_str!("fixtures/transition-2ch.txt")),
    ] {
        for rate in [8_000, 12_000, 16_000, 24_000, 48_000] {
            for outputs in [1, 2] {
                let mut decoder = OpusDecoder::new(rate, outputs).unwrap();
                let frame = rate as usize / 50;
                for (index, line) in fixture.lines().enumerate() {
                    let (range, hex) = line.split_once(' ').unwrap();
                    let packet: Vec<u8> = (0..hex.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                        .collect();
                    assert_eq!(opus_pure::packet::channels(&packet).unwrap(), coded);
                    let mut pcm = vec![0.0; frame * outputs + 4];
                    pcm[frame * outputs..].fill(12.5);
                    assert_eq!(
                        decoder
                            .decode(&packet, frame, &mut pcm[..frame * outputs])
                            .unwrap(),
                        frame
                    );
                    assert_eq!(
                        decoder.final_range(),
                        u32::from_str_radix(range, 16).unwrap(),
                        "packet {index}, {rate} Hz, coded {coded}, outputs {outputs}"
                    );
                    assert!(pcm[..frame * outputs].iter().all(|v| v.is_finite()));
                    assert_eq!(&pcm[frame * outputs..], &[12.5; 4]);
                }
            }
        }
    }
}
