//! Two CEM3340-style cores with additive fixed-gain waveform paths and B-to-A hard sync.
//!
//! Facts from `research:instruments/pro-one.md` §4: saw and pulse may sound together on A; saw,
//! centred triangle and pulse may sound together on B; the unequal summing resistors are not a
//! normalised mixer; and B's falling saw edge resets A before B's waveform switches and mixer.
//! A two-sample PolyBLEP corrects the ordered natural and hard-sync saw/pulse edges. A reset
//! preempts later free-running edges, contributes its actual before/phase-zero jump, and may create
//! new pulse-width crossings after reset; every surviving edge contributes both lobes. Exact levels, edge shape and DC offsets
//! remain unmeasured; the normalized levels below preserve
//! the established unipolar saw/pulse versus centred-triangle distinction.

use crate::finite_or;

pub const A_SAW_GAIN: f32 = 1.0;
pub const A_PULSE_GAIN: f32 = 0.6; // Derived conductance ratio: 120 kΩ / 200 kΩ.
pub const B_SAW_GAIN: f32 = 1.0;
pub const B_TRIANGLE_GAIN: f32 = 0.8; // Chosen: the switch network's resulting level is unknown.
pub const B_PULSE_GAIN: f32 = 0.5; // Derived conductance ratio: 100 kΩ / 200 kΩ.
pub const A_SUM_BOUND: f32 = A_SAW_GAIN + A_PULSE_GAIN;
pub const B_SUM_BOUND: f32 = B_SAW_GAIN + B_TRIANGLE_GAIN * 0.5 + B_PULSE_GAIN;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AWaves {
    pub saw: bool,
    pub pulse: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BWaves {
    pub saw: bool,
    pub triangle: bool,
    pub pulse: bool,
}

/// **The saw is on by default, matching Oscillator A's one plain waveform.**
///
/// A waveform switch is *configuration*, so the init contract asks for a useful one. B's mixer level
/// is the amount and starts at zero — that is what makes B silent at Init — but with **no** wave
/// enabled, raising that level produces nothing at all, and the brief's own promise that *"Oscillator
/// B starts slightly detuned but silent in the mixer, so raising its level immediately produces
/// beating"* is simply false. The owner found this on 2026-09-14, in the same sitting as the LFO's
/// missing shape.
///
/// The saw rather than the triangle or pulse because A's default is the saw and beating between two
/// of the same waveform is what the sentence above is about. All three remain independently enabled
/// and additive, and B's *core* drives sync whatever the switches say.
impl Default for BWaves {
    fn default() -> Self {
        Self {
            saw: true,
            triangle: false,
            pulse: false,
        }
    }
}

impl BWaves {
    /// Every path off. **Not the default**, so a caller building one wave at a time starts here.
    pub const NONE: Self = Self {
        saw: false,
        triangle: false,
        pulse: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CoreOutput {
    pub saw: f32,
    pub triangle: f32,
    pub pulse: f32,
    pub wrapped: bool,
    /// Fraction of the sample interval remaining after a natural falling saw edge.
    pub remaining_after_wrap: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AOutput {
    pub core: CoreOutput,
    pub sum: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BOutput {
    pub core: CoreOutput,
    pub sum: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct BlepTail {
    saw: f32,
    pulse: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct Edge {
    time: f64,
    saw_delta: f32,
    pulse_delta: f32,
}

fn push_edge(edges: &mut [Edge; 5], count: &mut usize, edge: Edge) {
    let mut position = *count;
    while position > 0 && edges[position - 1].time > edge.time {
        edges[position] = edges[position - 1];
        position -= 1;
    }
    edges[position] = edge;
    *count += 1;
}

#[derive(Debug, Clone, Copy)]
struct PhaseSegment {
    phase_start: f64,
    time_start: f64,
    duration: f64,
    include_end: bool,
}

/// Append the free-running edges in one uninterrupted phase segment. `include_end` is false before
/// a sync reset so an exactly coincident natural edge is preempted by the reset's ordered edge.
fn push_natural_edges(
    edges: &mut [Edge; 5],
    count: &mut usize,
    segment: PhaseSegment,
    increment: f64,
    width: f32,
) {
    let PhaseSegment {
        phase_start,
        time_start,
        duration,
        include_end,
    } = segment;
    if increment <= 0.0 || duration <= 0.0 {
        return;
    }
    let phase_end = phase_start + increment * duration;
    let in_segment = |phase: f64| {
        if phase <= phase_start {
            false
        } else if include_end {
            phase <= phase_end
        } else {
            phase < phase_end
        }
    };
    let time_of = |phase: f64| time_start + (phase - phase_start) / increment;

    if in_segment(1.0) {
        push_edge(
            edges,
            count,
            Edge {
                time: time_of(1.0),
                saw_delta: -1.0,
                pulse_delta: if width > 0.0 && width < 1.0 { 1.0 } else { 0.0 },
            },
        );
    }
    if width > 0.0 && width < 1.0 {
        let width = f64::from(width);
        let falling_phase = if width > phase_start {
            width
        } else {
            1.0 + width
        };
        if in_segment(falling_phase) {
            push_edge(
                edges,
                count,
                Edge {
                    time: time_of(falling_phase),
                    pulse_delta: -1.0,
                    ..Edge::default()
                },
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct Oscillator {
    phase: f64,
    /// Post-edge PolyBLEP lobes created by the actual ordered edges in the preceding sample.
    blep_tail: BlepTail,
}

impl Default for Oscillator {
    fn default() -> Self {
        Self::new()
    }
}

impl Oscillator {
    pub const fn new() -> Self {
        Self {
            phase: 0.0,
            blep_tail: BlepTail {
                saw: 0.0,
                pulse: 0.0,
            },
        }
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.blep_tail = BlepTail::default();
    }

    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// Render this sample, then advance. `hard_sync_remaining` is B's edge position in this same
    /// interval; it replaces A's natural next phase with the time elapsed since B reset it.
    #[inline]
    pub fn process(
        &mut self,
        frequency_hz: f32,
        pulse_width: f32,
        sample_rate: f32,
        hard_sync_remaining: Option<f64>,
    ) -> CoreOutput {
        let sample_rate = finite_or(sample_rate, 48_000.0).max(1.0);
        let frequency = finite_or(frequency_hz, 0.0).clamp(0.0, 0.45 * sample_rate);
        let increment = frequency / sample_rate;
        let phase = self.phase as f32;
        let width = finite_or(pulse_width, 0.5).clamp(0.0, 1.0);
        let increment = f64::from(increment);
        let mut output = CoreOutput {
            saw: phase + self.blep_tail.saw,
            triangle: if phase < 0.5 {
                2.0 * phase - 0.5
            } else {
                1.5 - 2.0 * phase
            },
            pulse: if width <= 0.0 {
                0.0
            } else if width >= 1.0 {
                1.0
            } else {
                if phase < width { 1.0 } else { 0.0 }
            } + self.blep_tail.pulse,
            wrapped: false,
            remaining_after_wrap: 0.0,
        };
        self.blep_tail = BlepTail::default();

        // Build the actual edge timeline. A sync reset splits the interval: natural edges before it
        // survive, coincident/later free-running edges are preempted, the reset is next, and edges
        // created by the reset-to-zero trajectory are superposed afterward.
        let mut edges = [Edge::default(); 5];
        let mut edge_count = 0;
        let sync_remaining = hard_sync_remaining.map(|value| value.clamp(0.0, 1.0));
        if let Some(remaining) = sync_remaining {
            let reset_time = 1.0 - remaining;
            push_natural_edges(
                &mut edges,
                &mut edge_count,
                PhaseSegment {
                    phase_start: self.phase,
                    time_start: 0.0,
                    duration: reset_time,
                    include_end: false,
                },
                increment,
                width,
            );
            let unwrapped_before = self.phase + increment * reset_time;
            let fractional = unwrapped_before.fract();
            let before_phase = if unwrapped_before > 0.0 && fractional.abs() < 1e-12 {
                1.0
            } else {
                fractional
            } as f32;
            let pulse_before = if width >= 1.0 || (width > 0.0 && before_phase <= width) {
                1.0
            } else {
                0.0
            };
            let pulse_after = if width > 0.0 { 1.0 } else { 0.0 };
            push_edge(
                &mut edges,
                &mut edge_count,
                Edge {
                    time: reset_time,
                    saw_delta: -before_phase,
                    pulse_delta: pulse_after - pulse_before,
                },
            );
            push_natural_edges(
                &mut edges,
                &mut edge_count,
                PhaseSegment {
                    phase_start: 0.0,
                    time_start: reset_time,
                    duration: remaining,
                    include_end: true,
                },
                increment,
                width,
            );
        } else {
            push_natural_edges(
                &mut edges,
                &mut edge_count,
                PhaseSegment {
                    phase_start: self.phase,
                    time_start: 0.0,
                    duration: 1.0,
                    include_end: true,
                },
                increment,
                width,
            );
        }

        for edge in edges.into_iter().take(edge_count) {
            let before_weight = (1.0 - edge.time) as f32;
            let after_weight = edge.time as f32;
            output.saw += 0.5 * edge.saw_delta * before_weight * before_weight;
            output.pulse += 0.5 * edge.pulse_delta * before_weight * before_weight;
            self.blep_tail.saw -= 0.5 * edge.saw_delta * after_weight * after_weight;
            self.blep_tail.pulse -= 0.5 * edge.pulse_delta * after_weight * after_weight;
        }

        let next = self.phase + increment;
        let natural_wrap_time = if increment > 0.0 {
            (1.0 - self.phase) / increment
        } else {
            f64::INFINITY
        };
        let wrapped = if let Some(remaining) = sync_remaining {
            natural_wrap_time < 1.0 - remaining
        } else {
            natural_wrap_time <= 1.0
        };
        let remaining_after_wrap = if wrapped {
            (1.0 - natural_wrap_time).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.phase = if let Some(remaining) = sync_remaining {
            (increment * remaining).fract()
        } else {
            next.fract()
        };

        CoreOutput {
            wrapped,
            remaining_after_wrap,
            ..output
        }
    }

    #[inline]
    pub fn process_a(
        &mut self,
        frequency_hz: f32,
        pulse_width: f32,
        waves: AWaves,
        sample_rate: f32,
        hard_sync_remaining: Option<f64>,
    ) -> AOutput {
        let core = self.process(frequency_hz, pulse_width, sample_rate, hard_sync_remaining);
        let sum = (if waves.saw {
            A_SAW_GAIN * core.saw
        } else {
            0.0
        }) + (if waves.pulse {
            A_PULSE_GAIN * core.pulse
        } else {
            0.0
        });
        AOutput { core, sum }
    }

    #[inline]
    pub fn process_b(
        &mut self,
        frequency_hz: f32,
        pulse_width: f32,
        waves: BWaves,
        sample_rate: f32,
    ) -> BOutput {
        let core = self.process(frequency_hz, pulse_width, sample_rate, None);
        let sum = (if waves.saw {
            B_SAW_GAIN * core.saw
        } else {
            0.0
        }) + (if waves.triangle {
            B_TRIANGLE_GAIN * core.triangle
        } else {
            0.0
        }) + (if waves.pulse {
            B_PULSE_GAIN * core.pulse
        } else {
            0.0
        });
        BOutput { core, sum }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    #[test]
    fn natural_frequency_and_sync_edge_position_are_exact_over_whole_cycles() {
        for sample_rate in [1_000.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0, 768_000.0] {
            let mut oscillator = Oscillator::new();
            let mut wraps = 0;
            for _ in 0..sample_rate as usize {
                wraps += usize::from(oscillator.process(440.0, 0.5, sample_rate, None).wrapped);
            }
            assert!((439..=440).contains(&wraps), "{sample_rate} Hz: {wraps}");
        }

        let mut slave = Oscillator::new();
        for _ in 0..100 {
            slave.process(137.0, 0.5, FS, Some(0.25));
            let expected = (137.0f32 / FS) as f64 * 0.25;
            assert!((slave.phase() - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn enabled_waveforms_add_at_fixed_unequal_gains_without_normalisation() {
        let (mut all, mut saw, mut pulse) =
            (Oscillator::new(), Oscillator::new(), Oscillator::new());
        for _ in 0..2_000 {
            let combined = all
                .process_a(
                    311.0,
                    0.3,
                    AWaves {
                        saw: true,
                        pulse: true,
                    },
                    FS,
                    None,
                )
                .sum;
            let expected =
                saw.process_a(
                    311.0,
                    0.3,
                    AWaves {
                        saw: true,
                        pulse: false,
                    },
                    FS,
                    None,
                )
                .sum + pulse
                    .process_a(
                        311.0,
                        0.3,
                        AWaves {
                            saw: false,
                            pulse: true,
                        },
                        FS,
                        None,
                    )
                    .sum;
            assert_eq!(combined, expected);
            assert!((0.0..=A_SUM_BOUND).contains(&combined));
        }
    }

    #[test]
    fn polyblep_softens_the_audio_rate_saw_reset() {
        let mut oscillator = Oscillator::new();
        let mut previous = oscillator.process(8_000.0, 0.5, FS, None).saw;
        let mut largest_step = 0.0f32;
        for _ in 0..2_000 {
            let current = oscillator.process(8_000.0, 0.5, FS, None).saw;
            largest_step = largest_step.max((current - previous).abs());
            previous = current;
        }
        assert!(
            largest_step < 0.7,
            "uncorrected reset-sized step: {largest_step}"
        );
    }

    #[test]
    fn pulse_endpoints_are_dc_and_full_pwm_can_cross_them() {
        for width in [0.0, 1.0] {
            let mut oscillator = Oscillator::new();
            let samples: Vec<_> = (0..1_000)
                .map(|_| oscillator.process(440.0, width, FS, None).pulse)
                .collect();
            assert!(samples.iter().all(|&sample| sample == width));
        }
    }

    #[test]
    fn hard_sync_uses_bs_core_even_when_no_b_waveform_is_enabled() {
        let (mut b_silent, mut b_audible) = (Oscillator::new(), Oscillator::new());
        let (mut a_silent_b, mut a_audible_b) = (Oscillator::new(), Oscillator::new());
        for _ in 0..5_000 {
            let silent = b_silent.process_b(997.0, 0.5, BWaves::default(), FS);
            let audible = b_audible.process_b(
                997.0,
                0.5,
                BWaves {
                    saw: true,
                    triangle: true,
                    pulse: true,
                },
                FS,
            );
            let sync_a = silent
                .core
                .wrapped
                .then_some(silent.core.remaining_after_wrap);
            let sync_b = audible
                .core
                .wrapped
                .then_some(audible.core.remaining_after_wrap);
            let left = a_silent_b.process_a(
                233.0,
                0.8,
                AWaves {
                    saw: true,
                    pulse: true,
                },
                FS,
                sync_a,
            );
            let right = a_audible_b.process_a(
                233.0,
                0.8,
                AWaves {
                    saw: true,
                    pulse: true,
                },
                FS,
                sync_b,
            );
            assert_eq!(left, right);
        }
    }

    #[test]
    fn wide_a_pulse_can_become_dc_when_b_resets_before_discharge() {
        let (mut a, mut b) = (Oscillator::new(), Oscillator::new());
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for sample in 0..20_000 {
            let b = b.process_b(2_000.0, 0.5, BWaves::default(), FS);
            let sync = b.core.wrapped.then_some(b.core.remaining_after_wrap);
            let pulse = a.process(100.0, 0.9, FS, sync).pulse;
            if sample > 1_000 {
                min = min.min(pulse);
                max = max.max(pulse);
            }
        }
        assert!(
            max - min < 0.05,
            "sync should hold the wide pulse near DC: {min}..{max}"
        );
    }

    fn two_sample_reference(raw_now: f32, raw_next: f32, edges: &[(f32, f32)]) -> (f32, f32) {
        let mut now = raw_now;
        let mut next = raw_next;
        for &(time, delta) in edges {
            now += 0.5 * delta * (1.0 - time).powi(2);
            next -= 0.5 * delta * time.powi(2);
        }
        (now, next)
    }

    #[test]
    fn hard_sync_orders_preempted_reset_and_reset_created_edges() {
        // Reset at t=.25: saw jumps .30 -> 0; the low pulse rises at reset, then the reset-created
        // trajectory crosses width .10 at t=.50. Both pulse edges need both BLEP lobes.
        let mut created = Oscillator::new();
        created.phase = 0.2;
        let now = created.process(19_200.0, 0.1, FS, Some(0.75));
        let next = created.process(0.0, 0.1, FS, None);
        let saw = two_sample_reference(0.2, 0.3, &[(0.25, -0.3)]);
        let pulse = two_sample_reference(0.0, 0.0, &[(0.25, 1.0), (0.5, -1.0)]);
        assert!((now.saw - saw.0).abs() < 2e-6 && (next.saw - saw.1).abs() < 2e-6);
        assert!((now.pulse - pulse.0).abs() < 2e-6 && (next.pulse - pulse.1).abs() < 2e-6);

        // The width edge at t=1/3 survives, reset follows at t=.4, and the free-running wrap at
        // t=2/3 is preempted. Including that wrap is the specific aggregate-BLEP failure.
        let mut preempted_wrap = Oscillator::new();
        preempted_wrap.phase = 0.8;
        let now = preempted_wrap.process(14_400.0, 0.9, FS, Some(0.6));
        let next = preempted_wrap.process(0.0, 0.9, FS, None);
        let saw = two_sample_reference(0.8, 0.18, &[(0.4, -0.92)]);
        let pulse = two_sample_reference(1.0, 1.0, &[(1.0 / 3.0, -1.0), (0.4, 1.0)]);
        assert!((now.saw - saw.0).abs() < 2e-6 && (next.saw - saw.1).abs() < 2e-6);
        assert!((now.pulse - pulse.0).abs() < 2e-6 && (next.pulse - pulse.1).abs() < 2e-6);
        assert!(!now.wrapped, "the reset preempts the later natural wrap");

        // This free-running falling edge would occur at t=.5, after a reset at t=.25. The reset
        // happens while pulse is already high and its new trajectory does not reach width, so the
        // actual ordered sequence contains no pulse edge at all.
        let mut preempted_width = Oscillator::new();
        preempted_width.phase = 0.2;
        let now = preempted_width.process(19_200.0, 0.4, FS, Some(0.75));
        let next = preempted_width.process(0.0, 0.4, FS, None);
        assert_eq!((now.pulse, next.pulse), (1.0, 1.0));
    }

    fn upper_band_energy(samples: &[f32]) -> f64 {
        let n = samples.len();
        ((3 * n / 8)..(n / 2))
            .map(|bin| {
                let omega = std::f64::consts::TAU * bin as f64 / n as f64;
                let (mut re, mut im) = (0.0, 0.0);
                for (index, &sample) in samples.iter().enumerate() {
                    let angle = omega * index as f64;
                    re += f64::from(sample) * angle.cos();
                    im -= f64::from(sample) * angle.sin();
                }
                re * re + im * im
            })
            .sum()
    }

    fn additive_sync_reference(
        ratio: f64,
        width: f64,
        saw_gain: f64,
        pulse_gain: f64,
        master_offset: f64,
    ) -> Vec<f32> {
        const INTEGRATION_POINTS: usize = 32_768;
        const PERIOD_SAMPLES: usize = 64;
        const HARMONICS: usize = 31; // 750 Hz fundamental: strictly below 24 kHz Nyquist.

        let ideal = |master_phase: f64| {
            let slave_phase = (ratio * master_phase).fract();
            saw_gain * slave_phase + pulse_gain * if slave_phase < width { 1.0 } else { 0.0 }
        };
        let mut coefficients = vec![(0.0, 0.0); HARMONICS + 1];
        for (harmonic, coefficient) in coefficients.iter_mut().enumerate() {
            for point in 0..INTEGRATION_POINTS {
                let phase = (point as f64 + 0.5) / INTEGRATION_POINTS as f64;
                let angle = -std::f64::consts::TAU * harmonic as f64 * phase;
                let value = ideal(phase) / INTEGRATION_POINTS as f64;
                coefficient.0 += value * angle.cos();
                coefficient.1 += value * angle.sin();
            }
        }

        (0..PERIOD_SAMPLES)
            .map(|sample| {
                let phase = (master_offset + sample as f64 / PERIOD_SAMPLES as f64).fract();
                let mut value = coefficients[0].0;
                for (harmonic, &(re, im)) in coefficients.iter().enumerate().skip(1) {
                    let angle = std::f64::consts::TAU * harmonic as f64 * phase;
                    value += 2.0 * (re * angle.cos() - im * angle.sin());
                }
                value as f32
            })
            .collect()
    }

    #[test]
    fn synced_saw_pulse_and_pwm_track_independent_additive_references() {
        const MASTER_HZ: f32 = 750.0;
        const PERIOD_SAMPLES: usize = 64;
        const MASTER_OFFSET: f64 = 0.37; // Forces every master reset between host samples.

        for (ratio, width) in [(0.37, 0.005), (1.75, 0.19), (3.4, 0.61), (7.3, 0.93)] {
            let saw_reference =
                additive_sync_reference(ratio as f64, width as f64, 1.0, 0.0, MASTER_OFFSET);
            let pulse_reference =
                additive_sync_reference(ratio as f64, width as f64, 0.0, 1.0, MASTER_OFFSET);
            let mut slave = Oscillator::new();
            let mut master = Oscillator::new();
            master.phase = MASTER_OFFSET;
            let mut corrected_saw = Vec::with_capacity(PERIOD_SAMPLES);
            let mut corrected_pulse = Vec::with_capacity(PERIOD_SAMPLES);
            for sample in 0..(10 * PERIOD_SAMPLES) {
                let master_output = master.process(MASTER_HZ, 0.5, FS, None);
                let sync = master_output
                    .wrapped
                    .then_some(master_output.remaining_after_wrap);
                let output = slave.process(MASTER_HZ * ratio, width, FS, sync);
                if sample >= 9 * PERIOD_SAMPLES {
                    corrected_saw.push(output.saw);
                    corrected_pulse.push(output.pulse);
                }
            }
            let (naive_saw, naive_pulse): (Vec<_>, Vec<_>) = (0..PERIOD_SAMPLES)
                .map(|sample| {
                    let master_phase =
                        (MASTER_OFFSET as f32 + sample as f32 / PERIOD_SAMPLES as f32).fract();
                    let slave_phase = (ratio * master_phase).fract();
                    (slave_phase, if slave_phase < width { 1.0 } else { 0.0 })
                })
                .unzip();
            let mean_square_error = |actual: &[f32], reference: &[f32]| {
                actual
                    .iter()
                    .zip(reference)
                    .map(|(&actual, &reference)| f64::from(actual - reference).powi(2))
                    .sum::<f64>()
                    / PERIOD_SAMPLES as f64
            };
            let corrected_saw_error = mean_square_error(&corrected_saw, &saw_reference);
            let naive_saw_error = mean_square_error(&naive_saw, &saw_reference);
            let corrected_pulse_error = mean_square_error(&corrected_pulse, &pulse_reference);
            let naive_pulse_error = mean_square_error(&naive_pulse, &pulse_reference);
            assert!(
                corrected_saw_error < naive_saw_error * 0.9,
                "synced saw A/B={ratio}: corrected {corrected_saw_error}, naive {naive_saw_error}"
            );
            assert!(
                corrected_pulse_error < naive_pulse_error * 0.9,
                "synced pulse/PWM A/B={ratio} width={width}: corrected {corrected_pulse_error}, naive {naive_pulse_error}"
            );
        }
    }

    #[test]
    fn polyblep_reduces_upper_band_alias_energy_across_saw_and_pwm_cases() {
        const N: usize = 1_024;
        for frequency in [3_100.0, 7_300.0, 11_700.0] {
            for width in [0.1, 0.3, 0.5, 0.8] {
                let increment = frequency / FS;
                let mut oscillator = Oscillator::new();
                let mut naive_phase = 0.0f32;
                let mut corrected_saw = Vec::with_capacity(N);
                let mut corrected_pulse = Vec::with_capacity(N);
                let mut naive_saw = Vec::with_capacity(N);
                let mut naive_pulse = Vec::with_capacity(N);
                for _ in 0..N {
                    let output = oscillator.process(frequency, width, FS, None);
                    corrected_saw.push(output.saw);
                    corrected_pulse.push(output.pulse);
                    naive_saw.push(naive_phase);
                    naive_pulse.push(if naive_phase < width { 1.0 } else { 0.0 });
                    naive_phase = (naive_phase + increment).fract();
                }
                let saw_ratio = upper_band_energy(&corrected_saw) / upper_band_energy(&naive_saw);
                let pulse_ratio =
                    upper_band_energy(&corrected_pulse) / upper_band_energy(&naive_pulse);
                assert!(
                    saw_ratio < 0.9,
                    "saw {frequency} Hz retained too much upper-band energy: {saw_ratio}"
                );
                assert!(
                    pulse_ratio < 0.95,
                    "pulse {frequency} Hz width {width} retained too much upper-band energy: {pulse_ratio}"
                );
            }
        }
    }
}
