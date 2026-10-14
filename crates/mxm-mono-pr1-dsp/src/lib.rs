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
//! The implementation is original code — MIT-licensed until the split of 2026-10-06, and
//! GPL-3.0-or-later with the rest of this repository since — informed by
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
