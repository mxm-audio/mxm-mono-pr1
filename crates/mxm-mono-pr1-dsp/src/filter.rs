//! Pro-One-specific four-stage CEM3320 low-pass model.
//!
//! This is original code from the published TPT/ZDF implicit-integrator technique (Vadim
//! Zavalishin, *The Art of VA Filter Design*) and the gm-cell topology recorded in
//! `research:filters/machines/ssm2040-cem3320-prophet.md` §§6–7. Saturation is on both sides of
//! every integrator, not in a generic ladder feedback clipper. The Pro-One's output buffer is the
//! schematic-derived ×3.4 and the resonance return is taken from that buffered node. Four equal
//! 150 pF capacitors establish the machine variation, but without measured CV calibration they do
//! not determine a unique software cutoff offset; the public cutoff is therefore in Hz.

use std::f64::consts::PI;

use crate::{finite_or, flush};

pub const BUFFER_GAIN: f32 = 3.4; // Derived: 1 + 240 kΩ / 100 kΩ; prose conflicts at 2.4.
pub const Q_RETURN_MAX: f32 = 2.2; // Chosen: puts onset near control 5.35 with post-buffer return.
pub const OSCILLATION_LOOP_GAIN: f32 = 4.0; // CEM/SSI four-cell datasheet condition.
pub const CELL_DRIVE: f32 = 0.35; // Chosen; normal levels remain well inside the 51:1 cell input.
pub const CELL_ASYMMETRY: f32 = 0.12; // Chosen pending second-harmonic hardware measurement.
pub const OUTPUT_COUPLING_HZ: f32 = 3.0; // Chosen for the documented 2.2 µF coupling capacitor.
pub const OUTPUT_BOUND: f32 = 32.0;
const CELL_STATE_BOUND: f32 = 8.0;
const NEWTON_ITERATIONS: usize = 5;
const CUTOFF_MIN_HZ: f32 = 10.0;
const NYQUIST_FRACTION: f32 = 0.45;

#[derive(Debug, Clone, Copy)]
pub struct GmCell {
    a: f32,
    b: f32,
    bias: f32,
}

impl Default for GmCell {
    fn default() -> Self {
        Self {
            a: 0.16,
            b: 0.01,
            bias: CELL_ASYMMETRY,
        }
    }
}

impl GmCell {
    fn raw(self, x: f32) -> (f32, f32) {
        let x = x.clamp(-12.0, 12.0);
        let x2 = x * x;
        let p = x * (1.0 + x2 * (self.a + self.b * x2));
        let derivative = 1.0 + x2 * (3.0 * self.a + 5.0 * self.b * x2);
        let denominator = 1.0 + p * p;
        let inverse_root = 1.0 / denominator.sqrt();
        (
            (p * inverse_root).clamp(-1.0, 1.0),
            derivative * inverse_root / denominator,
        )
    }

    /// Asymmetric, origin-preserving and normalized to unit small-signal slope.
    pub fn shape(self, x: f32) -> (f32, f32) {
        let (zero, slope) = self.raw(self.bias);
        let (value, derivative) = self.raw(x + self.bias);
        ((value - zero) / slope, derivative / slope)
    }

    #[inline]
    fn driven(self, x: f32, drive: f32) -> (f32, f32) {
        let drive = drive.max(1e-4);
        let (value, derivative) = self.shape(drive * x);
        (value / drive, derivative)
    }
}

#[inline]
fn solve_arrowhead(d: [f32; 4], a: [f32; 3], corner: f32, r: [f32; 4]) -> [f32; 4] {
    let p1 = r[1] / d[1];
    let q1 = a[0] / d[1];
    let p2 = (r[2] + a[1] * p1) / d[2];
    let q2 = a[1] * q1 / d[2];
    let p3 = (r[3] + a[2] * p2) / d[3];
    let q3 = a[2] * q2 / d[3];
    let x0 = (r[0] - corner * p3) / (d[0] + corner * q3);
    [x0, p1 + q1 * x0, p2 + q2 * x0, p3 + q3 * x0]
}

#[derive(Debug, Clone, Default)]
struct DcBlocker {
    previous_input: f32,
    previous_output: f32,
}

impl DcBlocker {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn process(&mut self, input: f32, sample_rate: f32) -> f32 {
        let coefficient =
            (-2.0 * PI * OUTPUT_COUPLING_HZ as f64 / sample_rate.max(1.0) as f64).exp() as f32;
        let output = input - self.previous_input + coefficient * self.previous_output;
        self.previous_input = input;
        self.previous_output = flush(output.clamp(-OUTPUT_BOUND, OUTPUT_BOUND));
        self.previous_output
    }
}

#[derive(Debug, Clone)]
pub struct GmFilter {
    cell: GmCell,
    state: [f32; 4],
    solution: [f32; 4],
    coupling: DcBlocker,
    g: f32,
    q_return: f32,
    sample_rate: f32,
    above_oscillation: bool,
}

impl Default for GmFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl GmFilter {
    pub fn new() -> Self {
        Self {
            cell: GmCell::default(),
            state: [0.0; 4],
            solution: [0.0; 4],
            coupling: DcBlocker::default(),
            g: 0.0,
            q_return: 0.0,
            sample_rate: 48_000.0,
            above_oscillation: false,
        }
    }

    pub fn reset(&mut self) {
        self.state = [0.0; 4];
        self.solution = [0.0; 4];
        self.coupling.reset();
        self.above_oscillation = false;
    }

    pub fn effective_loop_gain(resonance: f32) -> f32 {
        finite_or(resonance, 0.0).clamp(0.0, 1.0) * Q_RETURN_MAX * BUFFER_GAIN
    }

    pub fn set(&mut self, cutoff_hz: f32, resonance: f32, sample_rate: f32) {
        self.sample_rate = finite_or(sample_rate, 48_000.0).max(1.0);
        let cutoff = finite_or(cutoff_hz, CUTOFF_MIN_HZ).clamp(
            CUTOFF_MIN_HZ.min(NYQUIST_FRACTION * self.sample_rate),
            NYQUIST_FRACTION * self.sample_rate,
        );
        self.g = (PI * cutoff as f64 / self.sample_rate as f64).tan() as f32;
        self.q_return = finite_or(resonance, 0.0).clamp(0.0, 1.0) * Q_RETURN_MAX;
        let above = self.q_return * BUFFER_GAIN >= OSCILLATION_LOOP_GAIN;
        if above && !self.above_oscillation && self.solution == [0.0; 4] {
            // A physical filter has noise to start it. Deterministic -100 dB excitation avoids a
            // zero-state numerical fixed point without creating a random control source.
            self.state[0] = 1e-5;
        }
        self.above_oscillation = above;
    }

    pub fn is_self_oscillating(&self) -> bool {
        self.above_oscillation
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let input = finite_or(input, 0.0).clamp(-16.0, 16.0);
        let mut y = self.solution;
        let feedback = self.q_return * BUFFER_GAIN;

        for _ in 0..NEWTON_ITERATIONS {
            let v = [input - feedback * y[3], y[0], y[1], y[2]];
            let mut residual = [0.0; 4];
            let mut diagonal = [0.0; 4];
            let mut subdiagonal = [0.0; 3];
            for stage in 0..4 {
                let (source, source_derivative) = self.cell.driven(v[stage], CELL_DRIVE);
                let (output, output_derivative) = self.cell.driven(y[stage], CELL_DRIVE);
                residual[stage] = y[stage] - self.state[stage] - self.g * (source - output);
                diagonal[stage] = 1.0 + self.g * output_derivative;
                if stage > 0 {
                    subdiagonal[stage - 1] = self.g * source_derivative;
                }
            }
            let corner =
                self.g * feedback * self.cell.driven(input - feedback * y[3], CELL_DRIVE).1;
            let delta =
                solve_arrowhead(diagonal, subdiagonal, corner, residual.map(|value| -value));
            for stage in 0..4 {
                y[stage] = (y[stage] + delta[stage]).clamp(-CELL_STATE_BOUND, CELL_STATE_BOUND);
            }
        }

        for ((state, solution), value) in self.state.iter_mut().zip(self.solution.iter_mut()).zip(y)
        {
            *state = flush(
                (2.0 * value - *state).clamp(-2.0 * CELL_STATE_BOUND, 2.0 * CELL_STATE_BOUND),
            );
            *solution = flush(value);
        }

        // C143's exact corner depends on the following impedance and is unmeasured. The coupling
        // is placed after the ×3.4 node here; at audio frequencies the approximation leaves the
        // documented inside-loop buffer gain intact while removing oscillator DC at the output.
        self.coupling.process(BUFFER_GAIN * y[3], self.sample_rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_drive_preserves_small_signal_gain_and_asymmetry_is_not_odd() {
        let cell = GmCell::default();
        for drive in [0.1, 0.35, 1.0, 4.0] {
            let x = 1e-3;
            let (positive, _) = cell.driven(x, drive);
            assert!(
                (positive / x - 1.0).abs() < 0.01,
                "drive {drive}: {positive}"
            );
        }
        let (positive, _) = cell.shape(1.0);
        let (negative, _) = cell.shape(-1.0);
        assert!(
            (positive + negative).abs() > 0.005,
            "the chosen cell must be asymmetric"
        );
    }

    #[test]
    fn post_buffer_return_puts_oscillation_onset_between_five_and_six() {
        let onset = OSCILLATION_LOOP_GAIN / (Q_RETURN_MAX * BUFFER_GAIN);
        assert!((0.5..0.6).contains(&onset));
        assert!(GmFilter::effective_loop_gain(0.5) < OSCILLATION_LOOP_GAIN);
        assert!(GmFilter::effective_loop_gain(0.6) > OSCILLATION_LOOP_GAIN);
    }

    #[test]
    fn the_running_filter_sustains_above_the_datasheet_loop_gain() {
        let mut filter = GmFilter::new();
        filter.set(440.0, 0.8, 48_000.0);
        let mut energy = 0.0f64;
        let mut crossings = 0usize;
        let mut previous = 0.0;
        for sample in 0..96_000 {
            let output = filter.process(0.0);
            if sample >= 48_000 {
                energy += (output * output) as f64;
                crossings += usize::from(previous <= 0.0 && output > 0.0);
            }
            previous = output;
        }
        let rms = (energy / 48_000.0).sqrt() as f32;
        assert!(rms > 1e-3, "resonance did not sustain: {rms}");
        assert!(
            (300..600).contains(&crossings),
            "oscillation must track cutoff: {crossings} Hz"
        );
    }

    #[test]
    fn zero_is_an_exact_fixed_point_and_reset_clears_every_tail() {
        let mut filter = GmFilter::new();
        filter.set(1_000.0, 0.4, 48_000.0);
        for _ in 0..10_000 {
            assert_eq!(filter.process(0.0), 0.0);
        }
        for _ in 0..1_000 {
            filter.process(1.0);
        }
        filter.reset();
        filter.set(1_000.0, 0.4, 48_000.0);
        assert_eq!(filter.process(0.0), 0.0);
    }

    #[test]
    fn cutoff_is_a_four_pole_lowpass() {
        fn gain(frequency: f32) -> f32 {
            let fs = 48_000.0;
            let mut filter = GmFilter::new();
            filter.set(1_000.0, 0.0, fs);
            let mut input_energy = 0.0f64;
            let mut output_energy = 0.0f64;
            for sample in 0..48_000 {
                let x = (2.0 * std::f32::consts::PI * frequency * sample as f32 / fs).sin() * 0.01;
                let y = filter.process(x);
                if sample >= 24_000 {
                    input_energy += (x * x) as f64;
                    output_energy += (y * y) as f64;
                }
            }
            (output_energy / input_energy).sqrt() as f32
        }
        let low = gain(100.0);
        let high = gain(8_000.0);
        assert!(
            low > 2.5,
            "the derived output buffer remains in path: {low}"
        );
        assert!(
            high < low * 0.02,
            "four poles reject the high band: {high}/{low}"
        );
    }

    #[test]
    fn an_impulse_tail_never_leaves_recursive_state_in_the_denormal_range() {
        let mut filter = GmFilter::new();
        filter.set(800.0, 0.5, 48_000.0);
        filter.process(1.0);
        let mut output = 1.0;
        for _ in 0..500_000 {
            output = filter.process(0.0);
        }
        assert!(output == 0.0 || output.abs() >= 1e-20);
        assert!(
            filter
                .state
                .iter()
                .chain(filter.solution.iter())
                .chain([
                    &filter.coupling.previous_input,
                    &filter.coupling.previous_output,
                ])
                .all(|&value| value == 0.0 || value.abs() >= 1e-20)
        );
        filter.reset();
        filter.set(800.0, 0.5, 48_000.0);
        assert_eq!(filter.process(0.0), 0.0);
    }

    #[test]
    fn legal_extremes_are_finite_and_within_the_construction_bound() {
        for sample_rate in [1_000.0, 44_100.0, 48_000.0, 192_000.0, 768_000.0] {
            for cutoff in [0.0, 10.0, 1_000.0, 20_000.0, f32::INFINITY] {
                for resonance in [0.0, 0.55, 1.0, f32::NAN] {
                    let mut filter = GmFilter::new();
                    filter.set(cutoff, resonance, sample_rate);
                    for sample in 0..5_000 {
                        let input = if sample & 1 == 0 { 16.0 } else { -16.0 };
                        let output = filter.process(input);
                        assert!(output.is_finite() && output.abs() <= OUTPUT_BOUND);
                    }
                }
            }
        }
    }
}
