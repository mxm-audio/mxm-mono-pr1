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

const NO_REQUEST: u8 = u8::MAX;

#[derive(Debug)]
pub struct Telemetry {
    peak: AtomicU32,
    clipped: AtomicBool,
    sample_rate: AtomicU32,
    activity: AtomicU8,
    panic_latched: AtomicBool,
    /// The summing module's value, in the raw bus domain the panel reads.
    mod_bus: AtomicU32,
    /// The wheel stage: the module times the retained wheel, same domain.
    wheel_bus: AtomicU32,
    set_cutoff_hz: AtomicU32,
    effective_cutoff_hz: AtomicU32,
    dev_view: AtomicU8,
    dev_disclosure: AtomicU8,
    dev_browser: AtomicU8,
    dev_theme: AtomicU8,
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
            peak: AtomicU32::new(0),
            clipped: AtomicBool::new(false),
            sample_rate: AtomicU32::new(48_000.0f32.to_bits()),
            activity: AtomicU8::new(0),
            panic_latched: AtomicBool::new(false),
            mod_bus: AtomicU32::new(0),
            wheel_bus: AtomicU32::new(0),
            set_cutoff_hz: AtomicU32::new(10_000.0f32.to_bits()),
            effective_cutoff_hz: AtomicU32::new(10_000.0f32.to_bits()),
            dev_view: AtomicU8::new(NO_REQUEST),
            dev_disclosure: AtomicU8::new(NO_REQUEST),
            dev_browser: AtomicU8::new(NO_REQUEST),
            dev_theme: AtomicU8::new(NO_REQUEST),
            tempo: mxm_tempo::TempoCell::new(),
        }
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    pub fn publish_sample_rate(&self, value: f32) {
        self.sample_rate.store(value.to_bits(), Ordering::Relaxed);
    }

    pub fn publish_peak(&self, peak: f32) {
        let mut current = self.peak.load(Ordering::Relaxed);
        loop {
            let combined = f32::from_bits(current).max(peak);
            match self.peak.compare_exchange_weak(
                current,
                combined.to_bits(),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(seen) => current = seen,
            }
        }
        if peak >= 1.0 {
            self.clipped.store(true, Ordering::Relaxed);
        }
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

    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak.swap(0, Ordering::Relaxed))
    }

    pub fn clipped(&self) -> bool {
        self.clipped.load(Ordering::Relaxed)
    }

    pub fn clear_clip(&self) {
        self.clipped.store(false, Ordering::Relaxed);
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

    pub fn request_view(&self, value: u8) {
        self.dev_view
            .store(value.min(NO_REQUEST - 1), Ordering::Relaxed);
    }

    pub fn request_disclosure(&self, open: bool) {
        self.dev_disclosure.store(u8::from(open), Ordering::Relaxed);
    }

    pub fn request_browser(&self, open: bool) {
        self.dev_browser.store(u8::from(open), Ordering::Relaxed);
    }

    pub fn request_theme(&self, value: u8) {
        self.dev_theme
            .store(value.min(NO_REQUEST - 1), Ordering::Relaxed);
    }

    pub fn take_view_request(&self) -> Option<usize> {
        take_u8(&self.dev_view).map(usize::from)
    }

    pub fn take_disclosure_request(&self) -> Option<bool> {
        take_u8(&self.dev_disclosure).map(|value| value != 0)
    }

    pub fn take_browser_request(&self) -> Option<bool> {
        take_u8(&self.dev_browser).map(|value| value != 0)
    }

    pub fn take_theme_request(&self) -> Option<u8> {
        take_u8(&self.dev_theme)
    }
}

fn take_u8(slot: &AtomicU8) -> Option<u8> {
    match slot.swap(NO_REQUEST, Ordering::Relaxed) {
        NO_REQUEST => None,
        value => Some(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_combines_resets_and_clip_latches() {
        let telemetry = Telemetry::new();
        telemetry.publish_peak(0.8);
        telemetry.publish_peak(0.2);
        assert_eq!(telemetry.take_peak(), 0.8);
        assert_eq!(telemetry.take_peak(), 0.0);
        telemetry.publish_peak(1.0);
        telemetry.publish_peak(0.1);
        assert!(telemetry.clipped());
        telemetry.clear_clip();
        assert!(!telemetry.clipped());
    }

    #[test]
    fn developer_requests_are_atomic_and_taken_once() {
        let telemetry = Telemetry::new();
        telemetry.request_view(2);
        telemetry.request_disclosure(true);
        telemetry.request_browser(false);
        telemetry.request_theme(1);
        assert_eq!(telemetry.take_view_request(), Some(2));
        assert_eq!(telemetry.take_disclosure_request(), Some(true));
        assert_eq!(telemetry.take_browser_request(), Some(false));
        assert_eq!(telemetry.take_theme_request(), Some(1));
        assert_eq!(telemetry.take_view_request(), None);
    }
}
