//! Reproducible measurements of this provisional model, never hardware evidence.
//!
//! Run: `cargo run -p mxm-mono-pr1-dsp --release --example pro_one_measure`

use mxm_mono_pr1_dsp::{
    filter::{BUFFER_GAIN, GmFilter, OSCILLATION_LOOP_GAIN, Q_RETURN_MAX},
    oscillator::{AWaves, BWaves, Oscillator},
    voice::vca,
};

const FS: f32 = 48_000.0;

fn filter_gain(frequency: f32, cutoff: f32, resonance: f32) -> f32 {
    let mut filter = GmFilter::new();
    filter.set(cutoff, resonance, FS);
    let mut input_energy = 0.0f64;
    let mut output_energy = 0.0f64;
    for sample in 0..48_000 {
        let input = 0.01 * (2.0 * std::f32::consts::PI * frequency * sample as f32 / FS).sin();
        let output = filter.process(input);
        if sample >= 24_000 {
            input_energy += (input * input) as f64;
            output_energy += (output * output) as f64;
        }
    }
    20.0 * ((output_energy / input_energy).sqrt() as f32).log10()
}

fn main() {
    let onset = OSCILLATION_LOOP_GAIN / (Q_RETURN_MAX * BUFFER_GAIN);
    println!("Pro-One provisional DSP model — not hardware measurements");
    println!("buffer gain: {BUFFER_GAIN:.3}; derived control onset: {onset:.3}");
    for frequency in [100.0, 1_000.0, 8_000.0] {
        println!(
            "filter fc=1 kHz, resonance=0, {frequency:>6.0} Hz: {:>7.2} dB",
            filter_gain(frequency, 1_000.0, 0.0)
        );
    }

    let (mut a, mut b) = (Oscillator::new(), Oscillator::new());
    let mut a_peak = 0.0f32;
    let mut b_peak = 0.0f32;
    for _ in 0..48_000 {
        a_peak = a_peak.max(
            a.process_a(
                220.0,
                0.35,
                AWaves {
                    saw: true,
                    pulse: true,
                },
                FS,
                None,
            )
            .sum
            .abs(),
        );
        b_peak = b_peak.max(
            b.process_b(
                220.0,
                0.35,
                BWaves {
                    saw: true,
                    triangle: true,
                    pulse: true,
                },
                FS,
            )
            .sum
            .abs(),
        );
    }
    println!("fixed waveform sums at 220 Hz: A peak {a_peak:.3}, B peak {b_peak:.3}");
    println!(
        "VCA at input 2/env 1: volume 0.5 {:.4}, volume 1 {:.4}",
        vca(2.0, 1.0, 0.5),
        vca(2.0, 1.0, 1.0)
    );
}
