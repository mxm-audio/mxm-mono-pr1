//! Lock-free DSP-to-editor observations and developer requests.
//!
//! Scalar values publish once per internal block. The dual oscillator trace writes per sample only
//! while an editor is open. Peaks max-combine until read, and clipping remains latched until the
//! user acknowledges it.

use mxm_mono_pr1_dsp::control::Activity;
use mxm_mono_pr1_dsp::routing::FRAME_SCALE;
use mxm_mono_pr1_dsp::voice::{Frame, Params};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

#[derive(Debug)]
pub struct Telemetry {
    /// The output level and its clip latch: the collection's one meter policy
    /// (`mxm_telemetry::PeakMeter`).
    pub meter: mxm_telemetry::PeakMeter,
    /// The developer channel's requests of the editor, each taken once
    /// (`mxm_telemetry::DevRequests`).
    pub dev: mxm_telemetry::DevRequests,
    sample_rate: AtomicU32,
    activity: AtomicU8,
    panic_latched: AtomicBool,
    /// The summing module's value, in the raw bus domain the panel reads.
    mod_bus: AtomicU32,
    /// The wheel stage: the module times the retained wheel, same domain.
    wheel_bus: AtomicU32,
    set_cutoff_hz: AtomicU32,
    effective_cutoff_hz: AtomicU32,
    /// The host tempo in force, so a synced LFO rate reads its division.
    pub tempo: mxm_tempo::TempoCell,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            meter: mxm_telemetry::PeakMeter::new(),
            dev: mxm_telemetry::DevRequests::new(),
            sample_rate: AtomicU32::new(48_000.0f32.to_bits()),
            activity: AtomicU8::new(0),
            panic_latched: AtomicBool::new(false),
            mod_bus: AtomicU32::new(0),
            wheel_bus: AtomicU32::new(0),
            set_cutoff_hz: AtomicU32::new(10_000.0f32.to_bits()),
            effective_cutoff_hz: AtomicU32::new(10_000.0f32.to_bits()),
            tempo: mxm_tempo::TempoCell::new(),
        }
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    pub fn publish_sample_rate(&self, value: f32) {
        self.sample_rate.store(value.to_bits(), Ordering::Relaxed);
    }

    pub fn publish_frame(&self, frame: Frame, params: Params, sample_rate: f32) {
        self.publish_activity(frame.activity, frame.control.panic_latched);
        // Both stages are published in frame units, so the panel's own domain is one multiply
        // away. Showing the scaled value keeps the display reading in the units the bus always had.
        self.mod_bus.store(
            (frame.control.mod_bus.value() * FRAME_SCALE).to_bits(),
            Ordering::Relaxed,
        );
        self.wheel_bus.store(
            (frame.control.wheel_bus.value() * FRAME_SCALE).to_bits(),
            Ordering::Relaxed,
        );
        self.set_cutoff_hz
            .store(params.filter.cutoff_hz.to_bits(), Ordering::Relaxed);
        // **Read, not recomputed.** Keyboard tracking and the filter envelope are routes now, so
        // the voice's own sum is the whole offset and a second implementation here would drift.
        let effective = (params.filter.cutoff_hz
            * 2.0f32.powf(frame.cutoff_octaves.clamp(-16.0, 16.0)))
        .clamp(10.0f32.min(0.45 * sample_rate), 0.45 * sample_rate);
        self.effective_cutoff_hz
            .store(effective.to_bits(), Ordering::Relaxed);
    }

    pub fn publish_activity(&self, activity: Activity, panic_latched: bool) {
        self.activity.store(
            match activity {
                Activity::Inert => 0,
                Activity::Tailing => 1,
                Activity::Live => 2,
            },
            Ordering::Relaxed,
        );
        self.panic_latched.store(panic_latched, Ordering::Relaxed);
    }

    pub fn activity(&self) -> Activity {
        match self.activity.load(Ordering::Relaxed) {
            2 => Activity::Live,
            1 => Activity::Tailing,
            _ => Activity::Inert,
        }
    }

    pub fn panic_latched(&self) -> bool {
        self.panic_latched.load(Ordering::Relaxed)
    }

    pub fn mod_bus(&self) -> f32 {
        f32::from_bits(self.mod_bus.load(Ordering::Relaxed))
    }

    pub fn wheel_bus(&self) -> f32 {
        f32::from_bits(self.wheel_bus.load(Ordering::Relaxed))
    }

    pub fn set_cutoff_hz(&self) -> f32 {
        f32::from_bits(self.set_cutoff_hz.load(Ordering::Relaxed))
    }

    pub fn effective_cutoff_hz(&self) -> f32 {
        f32::from_bits(self.effective_cutoff_hz.load(Ordering::Relaxed))
    }
}
