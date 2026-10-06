//! Two instances of this CEM3310-style ADSR form the filter and amplifier envelopes.
//!
//! The documented 2 ms floor is preserved. Segment curvature and retrigger-from-current-level are
//! unmeasured; exponential segments and current-level retrigger are chosen until hardware evidence
//! replaces them. Gate/trigger policy belongs to `control`, because both envelopes share it.

use crate::{finite_or, flush};

pub const MIN_TIME_S: f32 = 0.002;
pub const MAX_TIME_S: f32 = 30.0; // Chosen ceiling; the published top is only “more than 6 s”.
pub const ZERO_THRESHOLD: f32 = 1e-5; // Chosen digital snap, -100 dB.
const ATTACK_OVERSHOOT: f64 = 0.2;
const ATTACK_TAUS: f64 = 1.791_759_469_228_055; // ln(1.2 / 0.2)
const DECAY_TAUS: f64 = 4.605_170_185_988_092; // ln(100)

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdsrParams {
    pub attack_s: f32,
    pub decay_s: f32,
    pub sustain: f32,
    pub release_s: f32,
}

impl Default for AdsrParams {
    fn default() -> Self {
        Self {
            attack_s: 0.002,
            decay_s: 0.2,
            sustain: 0.8,
            release_s: 0.2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone)]
pub struct Adsr {
    stage: Stage,
    level: f32,
    sample_rate: f64,
    cached: [f32; 3],
    coefficients: [f64; 3],
}

impl Default for Adsr {
    fn default() -> Self {
        Self::new()
    }
}

impl Adsr {
    pub const fn new() -> Self {
        Self {
            stage: Stage::Idle,
            level: 0.0,
            sample_rate: 48_000.0,
            cached: [-1.0; 3],
            coefficients: [0.0; 3],
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = finite_or(sample_rate, 48_000.0).max(1.0) as f64;
        self.cached = [-1.0; 3];
    }

    pub fn reset(&mut self) {
        self.stage = Stage::Idle;
        self.level = 0.0;
    }

    pub fn trigger(&mut self) {
        self.stage = Stage::Attack;
    }

    pub fn release(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    pub fn silence(&mut self) {
        self.reset();
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    pub fn is_active(&self) -> bool {
        self.stage != Stage::Idle
    }

    fn bounded_time(value: f32) -> f32 {
        finite_or(value, MIN_TIME_S).clamp(MIN_TIME_S, MAX_TIME_S)
    }

    fn coefficient(time_s: f32, taus: f64, sample_rate: f64) -> f64 {
        (-taus / (time_s as f64 * sample_rate)).exp()
    }

    fn update_coefficients(&mut self, params: AdsrParams) {
        let times = [
            Self::bounded_time(params.attack_s),
            Self::bounded_time(params.decay_s),
            Self::bounded_time(params.release_s),
        ];
        let taus = [ATTACK_TAUS, DECAY_TAUS, DECAY_TAUS];
        for index in 0..3 {
            if times[index] != self.cached[index] {
                self.cached[index] = times[index];
                self.coefficients[index] =
                    Self::coefficient(times[index], taus[index], self.sample_rate);
            }
        }
    }

    #[inline]
    pub fn process(&mut self, params: AdsrParams) -> f32 {
        self.update_coefficients(params);
        let sustain = finite_or(params.sustain, 0.0).clamp(0.0, 1.0) as f64;
        let before = self.level;
        let level = self.level as f64;

        self.level = match self.stage {
            Stage::Idle => return 0.0,
            Stage::Attack => {
                let target = 1.0 + ATTACK_OVERSHOOT;
                let next = target + (level - target) * self.coefficients[0];
                if next >= 1.0 {
                    self.stage = Stage::Decay;
                    1.0
                } else {
                    next as f32
                }
            }
            Stage::Decay => {
                let next = sustain + (level - sustain) * self.coefficients[1];
                let next = next as f32;
                if (next - sustain as f32).abs() <= ZERO_THRESHOLD || next == before {
                    self.stage = Stage::Sustain;
                    sustain as f32
                } else {
                    next
                }
            }
            Stage::Sustain => (sustain + (level - sustain) * self.coefficients[1]) as f32,
            Stage::Release => {
                let next = (level * self.coefficients[2]) as f32;
                if next <= ZERO_THRESHOLD || next == before {
                    self.stage = Stage::Idle;
                    0.0
                } else {
                    next
                }
            }
        };
        self.level = flush(self.level.clamp(0.0, 1.0));
        self.level
    }

    pub fn tail_samples(&self, release_s: f32) -> u32 {
        if !self.is_active() || self.level <= ZERO_THRESHOLD {
            return 0;
        }
        let coefficient =
            Self::coefficient(Self::bounded_time(release_s), DECAY_TAUS, self.sample_rate);
        let samples = (ZERO_THRESHOLD as f64 / self.level as f64).ln() / coefficient.ln();
        samples.max(0.0).ceil().min(u32::MAX as f64) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    fn run(envelope: &mut Adsr, seconds: f32, params: AdsrParams) {
        for _ in 0..(seconds * FS) as usize {
            envelope.process(params);
        }
    }

    #[test]
    fn segment_times_follow_the_documented_floor_and_reach_exact_idle() {
        let params = AdsrParams {
            attack_s: 0.002,
            decay_s: 0.02,
            sustain: 0.5,
            release_s: 0.02,
        };
        let mut envelope = Adsr::new();
        envelope.set_sample_rate(FS);
        envelope.trigger();
        let mut attack_samples = 0;
        while envelope.stage() == Stage::Attack {
            envelope.process(params);
            attack_samples += 1;
        }
        assert!((95..=97).contains(&attack_samples));
        run(&mut envelope, 0.2, params);
        assert_eq!(envelope.stage(), Stage::Sustain);
        envelope.release();
        run(&mut envelope, 0.2, params);
        assert_eq!((envelope.stage(), envelope.level()), (Stage::Idle, 0.0));
    }

    #[test]
    fn the_two_envelopes_are_independent_and_retrigger_from_current_level() {
        let params = AdsrParams::default();
        let (mut filter, mut amplifier) = (Adsr::new(), Adsr::new());
        filter.trigger();
        for _ in 0..500 {
            filter.process(params);
            amplifier.process(params);
        }
        assert!(filter.level() > 0.0);
        assert_eq!(amplifier.level(), 0.0);
        filter.release();
        run(&mut filter, 0.05, params);
        let before = filter.level();
        filter.trigger();
        let after = filter.process(params);
        assert!((after - before).abs() < 0.02);
    }

    #[test]
    fn coefficients_and_output_stay_finite_at_legal_sample_rates_and_extremes() {
        for sample_rate in [1_000.0, 44_100.0, 48_000.0, 192_000.0, 768_000.0] {
            let mut envelope = Adsr::new();
            envelope.set_sample_rate(sample_rate);
            envelope.trigger();
            for params in [
                AdsrParams {
                    attack_s: 0.0,
                    decay_s: f32::NAN,
                    sustain: 2.0,
                    release_s: f32::INFINITY,
                },
                AdsrParams {
                    attack_s: 30.0,
                    decay_s: 30.0,
                    sustain: -1.0,
                    release_s: 30.0,
                },
            ] {
                for _ in 0..10_000 {
                    let value = envelope.process(params);
                    assert!(value.is_finite() && (0.0..=1.0).contains(&value));
                }
            }
        }
    }
}
