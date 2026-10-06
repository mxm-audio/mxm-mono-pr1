//! The Pro-One LFO: one free-running core, three independently enabled waveform paths.
//!
//! Rising saw, triangle and square are additive, not a selector. Repeat timing reads the core
//! square independently, so disabling every modulation waveform does not stop or retime Repeat.
//! Waveform levels and phase are unmeasured; bipolar unit paths with fixed unity gains are chosen.

use crate::finite_or;

pub const RATE_MIN_HZ: f32 = 0.1;
pub const RATE_MAX_HZ: f32 = 30.0;
pub const SAW_GAIN: f32 = 1.0; // Chosen fixed path gain.
pub const TRIANGLE_GAIN: f32 = 1.0; // Chosen fixed path gain.
pub const SQUARE_GAIN: f32 = 1.0; // Chosen fixed path gain.
pub const SUM_BOUND: f32 = SAW_GAIN + TRIANGLE_GAIN + SQUARE_GAIN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaveEnables {
    pub saw: bool,
    pub triangle: bool,
    pub square: bool,
}

/// **The triangle is on by default, and that is the init contract rather than a preference.**
///
/// mxm-kit's `docs/plugin-conventions.md`, which `plugins/AGENTS.md` links: a *configuration*
/// control starts at a musically useful value, and *"no LFO, but the LFO at a good vibrato rate" is
/// not a contradiction — depth zero makes it inaudible, and a sensible rate makes it vibrato the
/// moment depth is raised rather than a drift or a buzz.* With **no** wave enabled, raising a depth
/// makes no sound at all, which is what the owner found on 2026-09-14: the routing was wired and
/// working and the LFO had nothing to say.
///
/// The triangle rather than the saw because the saw at a vibrato rate is a rising drift and the
/// square is a trill; the triangle is the shape the sentence above is about. The switches are the
/// machine's own and all three remain independently enabled and additive.
impl Default for WaveEnables {
    fn default() -> Self {
        Self {
            saw: false,
            triangle: true,
            square: false,
        }
    }
}

impl WaveEnables {
    /// Every path off. **Not the default**, and that is the point: `..WaveEnables::NONE` now
    /// leaves the triangle on, so a caller building one wave at a time has to start from here.
    pub const NONE: Self = Self {
        saw: false,
        triangle: false,
        square: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Output {
    pub saw: f32,
    pub triangle: f32,
    pub square: f32,
    pub sum: f32,
    pub clock_high: bool,
    pub clock_rose: bool,
}

#[derive(Debug, Clone)]
pub struct Lfo {
    phase: f64,
    clock_was_high: bool,
}

impl Default for Lfo {
    fn default() -> Self {
        Self::new()
    }
}

impl Lfo {
    pub const fn new() -> Self {
        Self {
            phase: 0.0,
            clock_was_high: true,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn phase(&self) -> f64 {
        self.phase
    }

    #[inline]
    pub fn process(&mut self, rate_hz: f32, enables: WaveEnables, sample_rate: f32) -> Output {
        let phase = self.phase as f32;
        let saw = 2.0 * phase - 1.0;
        let triangle = if phase < 0.25 {
            4.0 * phase
        } else if phase < 0.75 {
            2.0 - 4.0 * phase
        } else {
            4.0 * phase - 4.0
        };
        let clock_high = phase < 0.5;
        let square = if clock_high { 1.0 } else { -1.0 };
        let clock_rose = clock_high && !self.clock_was_high;
        self.clock_was_high = clock_high;

        let sum = (if enables.saw { SAW_GAIN * saw } else { 0.0 })
            + (if enables.triangle {
                TRIANGLE_GAIN * triangle
            } else {
                0.0
            })
            + (if enables.square {
                SQUARE_GAIN * square
            } else {
                0.0
            });

        let rate = finite_or(rate_hz, RATE_MIN_HZ).clamp(RATE_MIN_HZ, RATE_MAX_HZ);
        let sample_rate = finite_or(sample_rate, 48_000.0).max(1.0);
        self.phase += rate as f64 / sample_rate as f64;
        self.phase -= self.phase.floor();

        Output {
            saw,
            triangle,
            square,
            sum,
            clock_high,
            clock_rose,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    #[test]
    fn every_waveform_subset_is_the_fixed_sum_of_its_individual_paths() {
        for mask in 0u8..8 {
            let enables = WaveEnables {
                saw: mask & 1 != 0,
                triangle: mask & 2 != 0,
                square: mask & 4 != 0,
            };
            let (mut combined, mut saw, mut triangle, mut square) =
                (Lfo::new(), Lfo::new(), Lfo::new(), Lfo::new());
            for _ in 0..4_000 {
                let actual = combined.process(7.0, enables, FS).sum;
                let expected = (if enables.saw {
                    saw.process(
                        7.0,
                        WaveEnables {
                            saw: true,
                            ..WaveEnables::NONE
                        },
                        FS,
                    )
                    .sum
                } else {
                    saw.process(7.0, WaveEnables::NONE, FS);
                    0.0
                }) + (if enables.triangle {
                    triangle
                        .process(
                            7.0,
                            WaveEnables {
                                triangle: true,
                                ..WaveEnables::NONE
                            },
                            FS,
                        )
                        .sum
                } else {
                    triangle.process(7.0, WaveEnables::NONE, FS);
                    0.0
                }) + (if enables.square {
                    square
                        .process(
                            7.0,
                            WaveEnables {
                                square: true,
                                ..WaveEnables::NONE
                            },
                            FS,
                        )
                        .sum
                } else {
                    square.process(7.0, WaveEnables::NONE, FS);
                    0.0
                });
                assert_eq!(actual, expected);
                assert!(actual.abs() <= SUM_BOUND);
            }
        }
    }

    #[test]
    fn repeat_clock_is_independent_of_enabled_waveforms() {
        let mut edge_counts = [0usize; 8];
        for (mask, count) in edge_counts.iter_mut().enumerate() {
            let mut lfo = Lfo::new();
            let enables = WaveEnables {
                saw: mask & 1 != 0,
                triangle: mask & 2 != 0,
                square: mask & 4 != 0,
            };
            for _ in 0..(FS * 10.0) as usize {
                let output = lfo.process(3.0, enables, FS);
                *count += usize::from(output.clock_rose);
            }
        }
        assert!(
            edge_counts.iter().all(|&count| count == edge_counts[0]),
            "wave enables changed repeat timing: {edge_counts:?}"
        );
        assert!((29..=30).contains(&edge_counts[0]));
    }

    #[test]
    fn reset_is_deterministic_and_idle_can_freeze_the_core() {
        let mut lfo = Lfo::new();
        for _ in 0..1_000 {
            lfo.process(5.0, WaveEnables::default(), FS);
        }
        let phase = lfo.phase();
        assert_eq!(lfo.phase(), phase, "not calling process freezes phase");
        lfo.reset();
        assert_eq!(lfo.phase(), 0.0);
    }
}
