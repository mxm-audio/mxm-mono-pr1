//! Framework-free DSP for mxm-mono-pr1.
//!
//! The Pro-One's control and audio checkpoints are implemented: press identity and performance
//! ownership, gate/repeat/drone logic, glide, two envelopes and the additive LFO; then two additive
//! CEM3340-style oscillators, hard sync, noise, the Pro-One CEM3320 topology and a CA3280-style
//! VCA. There is deliberately no effect stage, and no external input (the owner, 2026-09-26).
//!
//! Modulation is the collection's shared any-to-any routing ([`mxm_modulation`]), declared for this
//! machine in [`routing`]. The hardware's Direct/Wheel buses are retired into it: the summing stage
//! survives as a summing module, the wheel stage as a **multiplier** module, and every destination
//! switch as a route's presence.
//!
//! The implementation is original MIT-licensed code informed by
//! `research:instruments/pro-one.md`. Fidelity remains unverified; values not established by that
//! research are labelled chosen beside their definitions and in this crate's `AGENTS.md`.

#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
pub mod control;
pub mod envelope;
pub mod filter;
pub mod keyboard;
pub mod lfo;
pub mod oscillator;
pub mod routing;
pub mod signal;
pub mod voice;

/// Flush recursive state before it can become denormal (about -400 dB for `f32`).
#[inline(always)]
pub fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 { 0.0 } else { x }
}

/// Convert an untrusted plain value to a finite value before applying a consumer's range.
#[inline(always)]
pub(crate) fn finite_or(x: f32, fallback: f32) -> f32 {
    if x.is_finite() { x } else { fallback }
}

/// Deterministic xorshift state for the audio-noise path.
///
/// The Pro-One has no random control source or sample-and-hold, and the conversion does not invent
/// one: noise is routable because it is *audio this machine already makes*, which is what
/// `plan-modulation-routing.md` §2.3 permits, and it is the same single stream the mixer hears
/// rather than a second generator.
#[derive(Debug, Clone)]
pub struct Rng(u32);

impl Rng {
    pub const fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x9e37_79b9 } else { seed })
    }

    /// Next exactly representable value in `[-1, 1)`.
    #[inline]
    pub fn next_bipolar(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        ((self.0 >> 8) as f32 / 8_388_608.0) - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn housekeeping_is_exact_bounded_and_deterministic() {
        assert_eq!(flush(1e-30), 0.0);
        assert_eq!(flush(-0.25), -0.25);
        assert_eq!(finite_or(f32::NAN, 0.5), 0.5);
        let (mut a, mut b) = (Rng::new(0x100), Rng::new(0x100));
        for _ in 0..10_000 {
            let x = a.next_bipolar();
            assert_eq!(x, b.next_bipolar());
            assert!((-1.0..1.0).contains(&x));
        }
    }
}
