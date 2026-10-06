//! Integrated Pro-One signal path: controls → B/A CEM3340 cores → mixer → CEM3320 → CA3280 VCA.
//!
//! There is deliberately no post-VCA effect stage: `research:instruments/pro-one.md` §§2, 8 and 9
//! establish that the hardware output ends at the VCA/output driver. The mixer's third source is
//! the noise generator; the hardware's switched external-audio jack is not reproduced (the owner,
//! 2026-09-26).

use crate::{
    Rng,
    control::{Activity, Controller, Event, Inputs as ControlInputs, Params as ControlParams},
    filter::{GmFilter, OUTPUT_BOUND as FILTER_OUTPUT_BOUND},
    finite_or,
    oscillator::{AOutput, AWaves, BOutput, BWaves, Oscillator},
    routing::{Routing, source, target},
};

pub const FREQUENCY_MOD_SEMITONES: f32 = 24.0; // Chosen destination scale.
pub const PWM_MOD_DEPTH: f32 = 0.5; // Chosen: full CV can reach either documented DC endpoint.
pub const FILTER_MOD_OCTAVES: f32 = 8.0; // Chosen pending cutoff-CV measurement.
// The three above are each target's *destination* scale, which `routing::UNIFORM_SCALE` and
// `routing::CUTOFF_SCALE` carry into the route table. They are still the machine's numbers.
pub const B_LOW_FREQUENCY_OFFSET: f32 = -84.0; // Chosen seven-octave extension into sub-audio.
pub const OUTPUT_BOUND: f32 = 1.0;
const NOISE_SEED: u32 = 0x0100_1981;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OscillatorAParams {
    pub octave: i8,
    pub tune_semitones: f32,
    pub pulse_width: f32,
    pub waves: AWaves,
    pub sync: bool,
}

impl Default for OscillatorAParams {
    fn default() -> Self {
        Self {
            octave: 0,
            tune_semitones: 0.0,
            pulse_width: 0.5,
            waves: AWaves {
                saw: true,
                pulse: false,
            },
            sync: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OscillatorBParams {
    pub octave: i8,
    pub tune_semitones: f32,
    pub pulse_width: f32,
    pub waves: BWaves,
    pub low_frequency: bool,
    pub keyboard_follow: bool,
}

impl Default for OscillatorBParams {
    fn default() -> Self {
        Self {
            octave: 0,
            tune_semitones: 0.07,
            pulse_width: 0.5,
            waves: BWaves::default(),
            low_frequency: false,
            keyboard_follow: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixerParams {
    pub oscillator_a: f32,
    pub oscillator_b: f32,
    pub noise: f32,
}

impl Default for MixerParams {
    fn default() -> Self {
        Self {
            oscillator_a: 0.7,
            oscillator_b: 0.0,
            noise: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilterParams {
    pub cutoff_hz: f32,
    pub resonance: f32,
}

impl Default for FilterParams {
    fn default() -> Self {
        Self {
            cutoff_hz: 10_000.0,
            resonance: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Params {
    pub control: ControlParams,
    pub master_tune_semitones: f32,
    pub oscillator_a: OscillatorAParams,
    pub oscillator_b: OscillatorBParams,
    pub mixer: MixerParams,
    pub filter: FilterParams,
    pub volume: f32,
}

impl Params {
    pub fn init() -> Self {
        Self {
            volume: 0.8,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub output: f32,
    pub oscillator_a: AOutput,
    pub oscillator_b: BOutput,
    pub mixer: f32,
    /// The cutoff offset this sample applied, in octaves, **as summed** — keyboard tracking and the
    /// filter envelope are two of its routes now, so there is nothing left outside it to add. An
    /// observer that recomputed it would be a second implementation of the same sum.
    pub cutoff_octaves: f32,
    /// The resonance this sample applied, after its own routes.
    pub resonance: f32,
    pub filtered: f32,
    pub control: crate::control::Frame,
    pub activity: Activity,
}

#[inline]
fn soft_clip(x: f32) -> f32 {
    let x = finite_or(x, 0.0).clamp(-8.0, 8.0);
    let square = x * x;
    (x * (27.0 + square) / (27.0 + 9.0 * square)).clamp(-1.0, 1.0)
}

/// CA3280-style operating-level model. Volume attenuates envelope control before the VCA and also
/// changes its chosen drive, rather than multiplying a completed output afterward.
#[inline]
pub fn vca(input: f32, envelope: f32, volume: f32) -> f32 {
    let control = finite_or(envelope, 0.0).clamp(0.0, 1.0) * finite_or(volume, 0.0).clamp(0.0, 1.0);
    if control == 0.0 {
        return 0.0;
    }
    let drive = 1.0 + 3.0 * control; // Chosen pending CA3280 level/distortion measurement.
    (control * soft_clip(finite_or(input, 0.0) * drive) / drive).clamp(-OUTPUT_BOUND, OUTPUT_BOUND)
}

#[derive(Debug, Clone)]
pub struct Voice {
    sample_rate: f32,
    controller: Controller,
    oscillator_a: Oscillator,
    oscillator_b: Oscillator,
    filter: GmFilter,
    noise: Rng,
}

impl Default for Voice {
    fn default() -> Self {
        Self::new()
    }
}

impl Voice {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            controller: Controller::new(),
            oscillator_a: Oscillator::new(),
            oscillator_b: Oscillator::new(),
            filter: GmFilter::new(),
            noise: Rng::new(NOISE_SEED),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = finite_or(sample_rate, 48_000.0).max(1.0);
        self.controller.set_sample_rate(self.sample_rate);
    }

    /// Adopt a routing grid and rebuild which routes are live. **Once per processing interval,
    /// never per sample**, and every path that can reach [`Voice::process_sample`] owes it.
    pub fn set_topology(&mut self, routing: &Routing) {
        self.controller.set_topology(routing);
    }

    /// The live amounts, for a caller advancing its smoothers in place each sample.
    #[inline]
    pub fn routing_mut(&mut self) -> &mut Routing {
        self.controller.routing_mut()
    }

    /// The routing grid, as it stands.
    #[inline]
    pub fn routing(&self) -> &Routing {
        self.controller.routing()
    }

    pub fn reset(&mut self) {
        self.controller.reset();
        self.oscillator_a.reset();
        self.oscillator_b.reset();
        self.filter.reset();
        self.noise = Rng::new(NOISE_SEED);
    }

    fn frequency(&self, semitones: f32) -> f32 {
        let semitones = finite_or(semitones, 60.0).clamp(-120.0, 180.0);
        (440.0f64 * 2.0f64.powf((semitones as f64 - 69.0) / 12.0)) as f32
    }

    /// Apply host events without advancing a sample. This preserves callback-end and zero-frame
    /// events until their pending gate/trigger transition is consumed by the next audio sample.
    pub fn handle_events(&mut self, params: ControlParams, events: &[Event]) {
        self.controller.handle_events(params, events);
        if events
            .iter()
            .any(|event| matches!(event, Event::AllSoundOff))
            || self.controller.activity(params) == Activity::Inert
        {
            self.filter.reset();
        }
    }

    pub fn activity(&self, params: ControlParams) -> Activity {
        self.controller.activity(params)
    }

    pub fn panic_latched(&self) -> bool {
        self.controller.panic_latched()
    }

    pub fn owner(&self) -> crate::keyboard::Owner {
        self.controller.keyboard().owner()
    }

    #[inline]
    pub fn process_sample(&mut self, params: Params, events: &[Event]) -> Frame {
        // 1-3. Events, glide and the sounding pitch; the event-driven gestures are published here.
        let begun = self.controller.begin_sample(params.control, events);

        // The generator advances only on a sample that runs, so an inert stretch cannot move the
        // noise sequence and the next phrase renders identically however long the gap was.
        let noise = if begun.runs {
            self.noise.next_bipolar()
        } else {
            0.0
        };

        // 4-7. Noise, the LFO and its routed rate, the gate, both envelopes, the two bus stages.
        let control =
            self.controller
                .process_control(params.control, ControlInputs { noise }, begun);

        if control.activity == Activity::Inert || control.panic_latched {
            self.filter.reset();
            return Frame {
                output: 0.0,
                oscillator_a: AOutput::default(),
                oscillator_b: BOutput::default(),
                mixer: 0.0,
                cutoff_octaves: 0.0,
                resonance: 0.0,
                filtered: 0.0,
                control,
                activity: Activity::Inert,
            };
        }

        let master = finite_or(params.master_tune_semitones, 0.0).clamp(-24.0, 24.0);

        // Oscillator B's two targets are summed **before** B renders, so every route from B reads
        // the preceding sample. That is the machine's deliberately-last commit, and it is now a row
        // in `routing::FORWARD_THROUGH` rather than a comment on a line's position.
        let b_pitch = (if params.oscillator_b.keyboard_follow {
            control.pitch_semitones
        } else {
            60.0
        }) + 12.0 * params.oscillator_b.octave.clamp(0, 3) as f32
            + finite_or(params.oscillator_b.tune_semitones, 0.0).clamp(-24.0, 24.0)
            + master
            + if params.oscillator_b.low_frequency {
                B_LOW_FREQUENCY_OFFSET
            } else {
                0.0
            }
            + self.controller.sum(target::OSCILLATOR_B_FREQUENCY);
        let b_width = (finite_or(params.oscillator_b.pulse_width, 0.5)
            + self.controller.sum(target::OSCILLATOR_B_PULSE_WIDTH))
        .clamp(0.0, 1.0);
        let oscillator_b = self.oscillator_b.process_b(
            self.frequency(b_pitch),
            b_width,
            params.oscillator_b.waves,
            self.sample_rate,
        );
        self.controller
            .publish(source::OSCILLATOR_B, oscillator_b.sum);

        // Oscillator A's targets are summed after B is published, which is how B modulates A
        // within the same sample — the hardware's own coupling.
        let a_pitch = control.pitch_semitones
            + 12.0 * params.oscillator_a.octave.clamp(0, 3) as f32
            + finite_or(params.oscillator_a.tune_semitones, 0.0).clamp(-24.0, 24.0)
            + master
            + self.controller.sum(target::OSCILLATOR_A_FREQUENCY);
        let a_width = (finite_or(params.oscillator_a.pulse_width, 0.5)
            + self.controller.sum(target::OSCILLATOR_A_PULSE_WIDTH))
        .clamp(0.0, 1.0);
        let sync = (params.oscillator_a.sync && oscillator_b.core.wrapped)
            .then_some(oscillator_b.core.remaining_after_wrap);
        let oscillator_a = self.oscillator_a.process_a(
            self.frequency(a_pitch),
            a_width,
            params.oscillator_a.waves,
            self.sample_rate,
            sync,
        );
        self.controller
            .publish(source::OSCILLATOR_A, oscillator_a.sum);

        let mixer = oscillator_a.sum * finite_or(params.mixer.oscillator_a, 0.0).clamp(0.0, 1.0)
            + oscillator_b.sum * finite_or(params.mixer.oscillator_b, 0.0).clamp(0.0, 1.0)
            + noise * finite_or(params.mixer.noise, 0.0).clamp(0.0, 1.0);

        // Cutoff and resonance are summed last, so every source reaches them forward. The cutoff's
        // old keyboard and dedicated-envelope paths are two of its routes now; nothing else is
        // added outside the sum.
        let modulation_octaves = self.controller.sum(target::FILTER_CUTOFF);
        let cutoff = finite_or(params.filter.cutoff_hz, 1_000.0)
            * 2.0f32.powf(modulation_octaves.clamp(-16.0, 16.0));
        let resonance = (finite_or(params.filter.resonance, 0.0)
            + self.controller.sum(target::RESONANCE))
        .clamp(0.0, 1.0);
        self.filter.set(cutoff, resonance, self.sample_rate);
        let filtered = self.filter.process(mixer);
        debug_assert!(filtered.is_finite() && filtered.abs() <= FILTER_OUTPUT_BOUND);
        let output = vca(filtered, control.amplifier_envelope.value(), params.volume);
        // **The collection's standard Amplitude**, a factor on the VCA's output: it cannot open a
        // closed VCA, and the VCA reaches at most a quarter of full scale, so double stays inside
        // `OUTPUT_BOUND`. Summed last, so every source reaches it forward.
        let output = if self.controller.routes_into(target::AMPLITUDE) {
            output
                * mxm_modulation::standard::amplitude_factor(self.controller.sum(target::AMPLITUDE))
        } else {
            output
        };

        Frame {
            output,
            oscillator_a,
            oscillator_b,
            mixer,
            cutoff_octaves: modulation_octaves,
            resonance,
            filtered,
            activity: control.activity,
            control,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{control::DEFAULT_KEY, envelope::AdsrParams, keyboard::NoteId};

    /// Wire one route at a depth, the way a legacy bus setting converts.
    fn route(routing: &mut Routing, target: usize, source: usize, amount: f32) {
        routing.present[target][source] = true;
        routing.amounts[target][source] = amount;
    }

    /// The multiplier's own two factors at full amount, which is how Init wires it.
    ///
    /// **A test that routes from `Wheel bus` without this gets the law's neutral**, because a
    /// product with nothing present is one — which is a constant, not silence.
    fn wheel_stage(routing: &mut Routing) {
        route(routing, target::WHEEL_BUS, source::MOD_BUS, 1.0);
        route(routing, target::WHEEL_BUS, source::WHEEL, 1.0);
    }

    /// A voice armed with a grid — what every caller owes before rendering.
    fn armed(routing: &Routing) -> Voice {
        let mut voice = Voice::default();
        voice.set_topology(routing);
        voice
    }

    const FS: f32 = 48_000.0;

    fn note_on(key: u8) -> Event {
        Event::NoteOn(NoteId::keyed(None, 0, key))
    }

    fn open_drone() -> Params {
        let mut params = Params::init();
        params.control.drone = true;
        params.control.filter_envelope = AdsrParams {
            attack_s: 0.002,
            decay_s: 0.002,
            sustain: 1.0,
            release_s: 0.02,
        };
        params.control.amplifier_envelope = params.control.filter_envelope;
        params
    }

    #[test]
    fn init_note_sounds_releases_to_exact_zero_and_reset_is_silent() {
        let mut voice = Voice::default();
        let params = Params::init();
        let mut peak = 0.0f32;
        for sample in 0..(FS as usize) {
            let events = if sample == 0 { &[note_on(60)][..] } else { &[] };
            peak = peak.max(voice.process_sample(params, events).output.abs());
        }
        assert!(peak > 0.01);
        let off = Event::AllNotesOff;
        voice.process_sample(params, &[off]);
        let mut frame = voice.process_sample(params, &[]);
        for _ in 0..(FS * 2.0) as usize {
            frame = voice.process_sample(params, &[]);
        }
        assert_eq!((frame.output, frame.activity), (0.0, Activity::Inert));
        voice.reset();
        assert_eq!(voice.process_sample(params, &[]).output, 0.0);
    }

    #[test]
    fn the_mixers_third_source_is_noise() {
        let mut params = open_drone();
        params.oscillator_a.waves = AWaves::default();
        params.mixer.oscillator_a = 0.0;
        params.mixer.noise = 1.0;
        let mut voice = Voice::default();
        let mut energy = 0.0f64;
        for _ in 0..(FS / 2.0) as usize {
            let output = voice.process_sample(params, &[]).output;
            energy += (output * output) as f64;
        }
        assert!(energy > 0.01, "noise alone is audible: {energy}");
    }

    #[test]
    fn b_syncs_a_when_b_is_inaudible_and_its_mixer_level_never_changes_modulation() {
        let mut left_params = open_drone();
        left_params.oscillator_a.sync = true;
        left_params.oscillator_b.tune_semitones = 19.0;
        left_params.oscillator_b.waves = BWaves::default();
        left_params.mixer.oscillator_b = 0.0;

        let mut right_params = left_params;
        right_params.oscillator_b.waves = BWaves {
            saw: true,
            triangle: true,
            pulse: true,
        };
        right_params.mixer.oscillator_b = 1.0;

        let (mut left, mut right) = (Voice::default(), Voice::default());
        for _ in 0..10_000 {
            let a = left.process_sample(left_params, &[]);
            let b = right.process_sample(right_params, &[]);
            assert_eq!(a.oscillator_a, b.oscillator_a, "sync timing follows B core");
        }

        let mut routed = left_params;
        routed.oscillator_b.waves.saw = true;
        let mut routing = Routing::new();
        route(
            &mut routing,
            target::OSCILLATOR_A_FREQUENCY,
            source::OSCILLATOR_B,
            1.0,
        );
        let mut quiet = routed;
        quiet.mixer.oscillator_b = 0.0;
        let mut loud = routed;
        loud.mixer.oscillator_b = 1.0;
        let (mut a, mut b) = (armed(&routing), armed(&routing));
        for _ in 0..2_000 {
            let qa = a.process_sample(quiet, &[]);
            let qb = b.process_sample(loud, &[]);
            assert_eq!(qa.oscillator_a, qb.oscillator_a);
        }
    }

    #[test]
    fn pulse_dc_endpoints_disappear_through_the_documented_coupled_output() {
        let mut params = open_drone();
        params.oscillator_a.waves = AWaves {
            saw: false,
            pulse: true,
        };
        params.oscillator_a.pulse_width = 1.0;
        params.filter.cutoff_hz = 15_000.0;
        let mut voice = Voice::default();
        let mut peak = 0.0f32;
        for sample in 0..(FS * 3.0) as usize {
            let output = voice.process_sample(params, &[]).output;
            if sample > (FS * 2.5) as usize {
                peak = peak.max(output.abs());
            }
        }
        assert!(
            peak < 2e-4,
            "a DC pulse endpoint should couple away: {peak}"
        );
    }

    #[test]
    fn volume_changes_vca_operating_drive_not_only_post_output_level() {
        let input = 2.0;
        let half = vca(input, 1.0, 0.5);
        let post_scaled = 0.5 * vca(input, 1.0, 1.0);
        assert!((half - post_scaled).abs() > 0.005);
        assert_eq!(vca(input, 0.0, 1.0), 0.0);
    }

    #[test]
    fn direct_and_wheel_graph_reaches_audio_destinations_in_the_integrated_order() {
        let mut dry = open_drone();
        dry.oscillator_a.waves = AWaves {
            saw: true,
            pulse: false,
        };
        dry.control.lfo_waves.saw = true;
        let mut direct = Routing::new();
        route(
            &mut direct,
            target::OSCILLATOR_A_FREQUENCY,
            source::LFO,
            0.5,
        );
        // The wheel path is the same depth through the two bus stages: the LFO into `Mod bus`, and
        // the destination reading `Wheel bus` at full amount. That is the canonical translation of
        // a legacy source assigned to Wheel.
        let mut wheel_routing = Routing::new();
        wheel_stage(&mut wheel_routing);
        route(&mut wheel_routing, target::MOD_BUS, source::LFO, 0.5);
        route(
            &mut wheel_routing,
            target::OSCILLATOR_A_FREQUENCY,
            source::WHEEL_BUS,
            1.0,
        );
        let wheel = dry;
        let (mut direct_voice, mut wheel_voice) = (armed(&direct), armed(&wheel_routing));
        let mut differs = false;
        for _ in 0..2_000 {
            let direct = direct_voice.process_sample(dry, &[]);
            let attenuated = wheel_voice.process_sample(wheel, &[]);
            differs |= direct.oscillator_a != attenuated.oscillator_a;
        }
        assert!(differs, "wheel at zero must attenuate only the Wheel bus");
    }

    #[test]
    fn extreme_legal_controls_are_finite_bounded_and_repeatable_at_all_rates() {
        for sample_rate in [1_000.0, 44_100.0, 48_000.0, 192_000.0, 768_000.0] {
            let mut params = open_drone();
            params.master_tune_semitones = f32::INFINITY;
            params.oscillator_a.octave = 99;
            params.oscillator_a.tune_semitones = -999.0;
            params.oscillator_a.pulse_width = f32::NAN;
            params.oscillator_a.waves = AWaves {
                saw: true,
                pulse: true,
            };
            params.oscillator_b.octave = -99;
            params.oscillator_b.low_frequency = true;
            params.oscillator_b.keyboard_follow = false;
            params.oscillator_b.waves = BWaves {
                saw: true,
                triangle: true,
                pulse: true,
            };
            params.mixer = MixerParams {
                oscillator_a: 99.0,
                oscillator_b: 99.0,
                noise: 99.0,
            };
            params.filter = FilterParams {
                cutoff_hz: f32::INFINITY,
                resonance: 99.0,
            };
            params.volume = 99.0;
            let (mut a, mut b) = (Voice::default(), Voice::default());
            a.set_sample_rate(sample_rate);
            b.set_sample_rate(sample_rate);
            for _ in 0..5_000 {
                let left = a.process_sample(params, &[]);
                let right = b.process_sample(params, &[]);
                assert_eq!(left.output, right.output);
                assert!(left.output.is_finite() && left.output.abs() <= OUTPUT_BOUND);
                assert!(left.filtered.is_finite() && left.filtered.abs() <= FILTER_OUTPUT_BOUND);
            }
        }
    }

    #[test]
    fn idle_gap_length_cannot_change_the_second_phrase() {
        let params = Params::init();
        let (mut uninterrupted, mut idled) = (Voice::default(), Voice::default());
        for voice in [&mut uninterrupted, &mut idled] {
            voice.process_sample(params, &[note_on(55)]);
            for _ in 0..4_000 {
                voice.process_sample(params, &[]);
            }
            voice.process_sample(params, &[Event::AllNotesOff]);
            for _ in 0..100_000 {
                if voice.process_sample(params, &[]).activity == Activity::Inert {
                    break;
                }
            }
        }
        for _ in 0..25_000 {
            let frame = idled.process_sample(params, &[]);
            assert_eq!((frame.output, frame.activity), (0.0, Activity::Inert));
        }
        assert_eq!(
            uninterrupted.process_sample(params, &[note_on(67)]),
            idled.process_sample(params, &[note_on(67)])
        );
        for _ in 0..10_000 {
            assert_eq!(
                uninterrupted.process_sample(params, &[]),
                idled.process_sample(params, &[])
            );
        }
    }

    #[test]
    fn fresh_instances_and_reset_reproduce_the_same_audio() {
        let params = open_drone();
        let (mut a, mut b) = (Voice::default(), Voice::default());
        for _ in 0..4_000 {
            assert_eq!(a.process_sample(params, &[]), b.process_sample(params, &[]));
        }
        a.reset();
        b = Voice::default();
        for _ in 0..1_000 {
            assert_eq!(a.process_sample(params, &[]), b.process_sample(params, &[]));
        }
        assert_eq!(a.controller.keyboard().owner().key, DEFAULT_KEY);
    }
    /// A patch with **every pair live at full depth** — far past anything the hardware could wire,
    /// and the cheapest high-value check any-to-any buys: it reaches pairs no designer exercised.
    fn everything_routed() -> (Params, Routing) {
        let mut params = open_drone();
        params.oscillator_a.waves = AWaves {
            saw: true,
            pulse: true,
        };
        params.oscillator_b.waves = BWaves {
            saw: true,
            triangle: true,
            pulse: true,
        };
        params.mixer.oscillator_b = 0.7;
        params.mixer.noise = 0.4;
        params.control.lfo_waves = crate::lfo::WaveEnables {
            saw: true,
            triangle: true,
            square: true,
        };
        let mut routing = Routing::new();
        for (t, (present, amounts)) in routing
            .present
            .iter_mut()
            .zip(routing.amounts.iter_mut())
            .enumerate()
        {
            for (s, (on, amount)) in present.iter_mut().zip(amounts.iter_mut()).enumerate() {
                *on = true;
                *amount = if (t + s).is_multiple_of(2) { 1.0 } else { -1.0 };
            }
        }
        (params, routing)
    }

    /// Every pair live, at both signs, at several rates: finite, bounded, and the same twice.
    ///
    /// Any-to-any is what makes this worth having — the legacy graph could reach fifteen
    /// combinations and this one reaches 125, including four audio sources feeding two pitch
    /// targets and a module reading itself.
    #[test]
    fn the_whole_routing_grid_stays_finite_bounded_and_deterministic() {
        for rate in [8_000.0, 44_100.0, 48_000.0, 192_000.0] {
            let (params, routing) = everything_routed();
            let render = || {
                let mut voice = Voice::default();
                voice.set_sample_rate(rate);
                voice.set_topology(&routing);
                voice.process_sample(params, &[note_on(60)]);
                (0..4_000)
                    .map(|n| {
                        let frame = voice.process_sample(params, &[]);
                        assert!(
                            frame.output.is_finite() && frame.output.abs() <= OUTPUT_BOUND,
                            "output left the bound at {rate} Hz, sample {n}: {}",
                            frame.output
                        );
                        frame.output.to_bits()
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(render(), render(), "the grid must render deterministically");
        }
    }

    /// An audio source into a pitch target is FM, and it is a **backward** route: one sample late,
    /// declared rather than discovered. The test is that it is audible and that it stays bounded,
    /// not that it sounds like anything in particular.
    #[test]
    fn an_audio_source_into_a_pitch_target_is_audible_and_bounded() {
        let mut quiet = open_drone();
        quiet.oscillator_b.waves.saw = true;
        quiet.mixer.oscillator_b = 0.0;
        let modulated = quiet;
        let mut fm_routing = Routing::new();
        route(
            &mut fm_routing,
            target::OSCILLATOR_A_FREQUENCY,
            source::OSCILLATOR_B,
            0.5,
        );

        let (mut plain, mut fm) = (armed(&Routing::new()), armed(&fm_routing));
        let mut differs = false;
        for _ in 0..4_000 {
            let a = plain.process_sample(quiet, &[]);
            let b = fm.process_sample(modulated, &[]);
            differs |= a.oscillator_a.sum != b.oscillator_a.sum;
            assert!(b.output.is_finite() && b.output.abs() <= OUTPUT_BOUND);
        }
        assert!(differs, "an audio-rate route must reach the carrier");
    }

    /// The routing must not be able to keep a released voice alive or leave a tail behind it.
    /// Every target this instrument declares is upstream of an envelope-bounded VCA, and that is
    /// what the amplitude and gate exclusions protect.
    #[test]
    fn a_fully_routed_patch_still_releases_to_exact_zero() {
        let (mut params, routing) = everything_routed();
        params.control.drone = false;
        params.control.amplifier_envelope = AdsrParams {
            attack_s: 0.002,
            decay_s: 0.01,
            sustain: 0.8,
            release_s: 0.02,
        };
        let mut voice = armed(&routing);
        voice.process_sample(params, &[note_on(60)]);
        for _ in 0..2_000 {
            voice.process_sample(params, &[]);
        }
        voice.process_sample(params, &[Event::AllNotesOff]);
        let mut settled = false;
        for _ in 0..48_000 {
            let frame = voice.process_sample(params, &[]);
            if frame.activity == Activity::Inert {
                settled = true;
                assert_eq!(frame.output, 0.0, "Inert must be exact zero");
                break;
            }
        }
        assert!(settled, "a fully routed patch never settled");
        for _ in 0..1_000 {
            assert_eq!(voice.process_sample(params, &[]).output, 0.0);
        }
    }
    /// **Where `Voice::process_sample` publishes the two audio sources, proved by rendering.**
    ///
    /// `control::tests::the_declared_publication_order_is_the_order_the_code_runs` covers the
    /// control side but stands in for the voice: it publishes Oscillator B itself, so moving B's
    /// real publication would leave it green. This exercises the voice's own order, and it is the
    /// half `routing::FORWARD_THROUGH` cannot check about itself.
    ///
    /// Two claims, and between them they pin B's publication to exactly one point:
    ///
    /// - **`Mod bus` reads both oscillators one sample late**, because it is summed before either
    ///   renders. Asserted numerically against the frame's own reported sums.
    /// - **B reaches A's frequency within the same sample.** Routing B straight at A's pitch and
    ///   routing it through `Mod bus` are the same arithmetic — the frame unit and the target scale
    ///   cancel identically on both paths — so the *only* thing that can separate them is the one
    ///   sample of delay the bus adds. If B were published after A's targets were summed, or before
    ///   the bus's, the two would render the same.
    #[test]
    fn the_voice_publishes_each_oscillator_between_the_bus_sum_and_the_next_target() {
        use crate::routing::FRAME_UNIT;

        let mut params = open_drone();
        params.oscillator_a.waves = AWaves {
            saw: true,
            pulse: false,
        };
        params.oscillator_b.waves = BWaves {
            saw: true,
            triangle: false,
            pulse: false,
        };
        params.oscillator_b.tune_semitones = 7.0;
        params.mixer.oscillator_b = 0.5;

        // 1. Backward: the module is summed before either oscillator renders.
        for slot in [source::OSCILLATOR_A, source::OSCILLATOR_B] {
            let mut routing = Routing::new();
            route(&mut routing, target::MOD_BUS, slot, 1.0);
            let mut voice = armed(&routing);
            voice.set_sample_rate(FS);
            let mut previous: Option<f32> = None;
            let mut moved = false;
            for _ in 0..512 {
                let frame = voice.process_sample(params, &[]);
                let raw = if slot == source::OSCILLATOR_A {
                    frame.oscillator_a.sum
                } else {
                    frame.oscillator_b.sum
                };
                if let Some(before) = previous {
                    let expected = (before * FRAME_UNIT).clamp(-1.0, 1.0);
                    assert!(
                        (frame.control.mod_bus.value() - expected).abs() < 1e-6,
                        "Mod bus must read source {slot} backward: {} against {expected}",
                        frame.control.mod_bus.value()
                    );
                    moved |= before != raw;
                }
                previous = Some(raw);
            }
            assert!(
                moved,
                "the oscillator has to move or the test proves nothing"
            );
        }

        // 2. Forward: B reaches A's frequency in the sample B was produced in.
        let depth = 0.35;
        let mut direct = Routing::new();
        route(
            &mut direct,
            target::OSCILLATOR_A_FREQUENCY,
            source::OSCILLATOR_B,
            depth,
        );
        let mut delayed = Routing::new();
        route(&mut delayed, target::MOD_BUS, source::OSCILLATOR_B, 1.0);
        route(
            &mut delayed,
            target::OSCILLATOR_A_FREQUENCY,
            source::MOD_BUS,
            depth,
        );
        let render_a = |routing: &Routing| {
            let mut voice = armed(routing);
            voice.set_sample_rate(FS);
            (0..2_000)
                .map(|_| voice.process_sample(params, &[]).oscillator_a.sum)
                .collect::<Vec<_>>()
        };
        assert!(
            render_a(&direct)
                .iter()
                .zip(render_a(&delayed).iter())
                .any(|(a, b)| a != b),
            "B must reach A's frequency within its own sample; the one-sample bus detour has to              make an audible difference or B is being published in the wrong place"
        );

        // 3. **B's own targets are summed before B renders**, so B self-modulation is one sample
        //    late. Routing B straight at its own frequency and routing it through `Mod bus` are the
        //    same arithmetic *and* the same delay — the bus is summed before B renders too — so they
        //    must render **identically**. Publishing B any earlier than it is would give the direct
        //    route no delay at all while the bus route kept one, and they would separate.
        let mut self_direct = Routing::new();
        route(
            &mut self_direct,
            target::OSCILLATOR_B_FREQUENCY,
            source::OSCILLATOR_B,
            depth,
        );
        let mut self_via_bus = Routing::new();
        route(
            &mut self_via_bus,
            target::MOD_BUS,
            source::OSCILLATOR_B,
            1.0,
        );
        route(
            &mut self_via_bus,
            target::OSCILLATOR_B_FREQUENCY,
            source::MOD_BUS,
            depth,
        );
        let render_b = |routing: &Routing| {
            let mut voice = armed(routing);
            voice.set_sample_rate(FS);
            (0..2_000)
                .map(|_| voice.process_sample(params, &[]).oscillator_b.sum.to_bits())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            render_b(&self_direct),
            render_b(&self_via_bus),
            "B self-modulation must read the preceding sample, exactly as the bus detour does"
        );
        assert_ne!(
            render_b(&self_direct),
            render_b(&Routing::new()),
            "the self route has to do something audible or the equality above is vacuous"
        );

        // 4. **A is published before the last targets are summed**, so a route from A into the
        //    cutoff is forward. The same detour, the other way round: through `Mod bus` it is one
        //    sample late, and the two must differ.
        let mut a_direct = Routing::new();
        route(
            &mut a_direct,
            target::FILTER_CUTOFF,
            source::OSCILLATOR_A,
            0.2,
        );
        let mut a_delayed = Routing::new();
        route(&mut a_delayed, target::MOD_BUS, source::OSCILLATOR_A, 1.0);
        route(&mut a_delayed, target::FILTER_CUTOFF, source::MOD_BUS, 0.2);
        let render_out = |routing: &Routing| {
            let mut voice = armed(routing);
            voice.set_sample_rate(FS);
            (0..2_000)
                .map(|_| voice.process_sample(params, &[]).filtered)
                .collect::<Vec<_>>()
        };
        assert!(
            render_out(&a_direct)
                .iter()
                .zip(render_out(&a_delayed).iter())
                .any(|(x, y)| x != y),
            "Oscillator A must reach the cutoff within its own sample"
        );
    }

    /// **The wheel fades the vibrato in, and never transposes.**
    ///
    /// The owner's report, 2026-09-14: routing the wheel *and* the LFO into `Mod bus` gave pitch
    /// change from both — the wheel as a transposing offset, the LFO as vibrato — where the wheel
    /// should have been setting how much vibrato there was. The bus sums, so that is exactly what
    /// an additive wheel route does; the gesture the owner wants is the multiplication the wheel
    /// stage already performs.
    ///
    /// Three claims, and the first is the one that makes the other two matter:
    ///
    /// - **The pair cannot be wired.** `Graph::set_topology` refuses it whatever it is handed, so
    ///   even a hand-built `Routing` cannot reintroduce the transposition.
    /// - **The wheel sets the depth.** Wheel up is more vibrato than wheel half, which is more than
    ///   wheel down — and wheel down is exactly none.
    /// - **The wheel does not shift the pitch.** With the LFO stopped, moving the wheel changes
    ///   nothing at all: a fade control with nothing to fade is silent, where an additive route
    ///   would transpose.
    #[test]
    fn the_wheel_fades_vibrato_in_rather_than_transposing() {
        let mut params = open_drone();
        params.oscillator_a.waves = AWaves {
            saw: true,
            pulse: false,
        };
        params.control.lfo_rate_hz = 5.0;

        // The gesture: the LFO into the bus, and the pitch reading the wheel stage.
        let mut routing = Routing::new();
        wheel_stage(&mut routing);
        route(&mut routing, target::MOD_BUS, source::LFO, 0.6);
        route(
            &mut routing,
            target::OSCILLATOR_A_FREQUENCY,
            source::WHEEL_BUS,
            1.0,
        );
        // Ask for the refused pair as well; it must simply not arrive.
        route(&mut routing, target::MOD_BUS, source::WHEEL, 1.0);

        let swing = |wheel: f32, lfo_running: bool| {
            let mut patch = params;
            patch.control.lfo_waves = crate::lfo::WaveEnables {
                saw: false,
                triangle: lfo_running,
                square: false,
            };
            let mut voice = armed(&routing);
            voice.set_sample_rate(FS);
            voice.process_sample(
                patch,
                &[Event::ModWheel {
                    channel: 0,
                    value: wheel,
                }],
            );
            let mut low = f32::MAX;
            let mut high = f32::MIN;
            for _ in 0..(FS as usize / 4) {
                let bus = voice.process_sample(patch, &[]).control.wheel_bus.value();
                low = low.min(bus);
                high = high.max(bus);
            }
            (low, high)
        };

        // 1. The refused pair never became live.
        let mut graph_check = Routing::new();
        let (bus, wheel) = crate::routing::WHEEL_IS_NOT_A_BUS_SOURCE;
        graph_check.present[bus][wheel] = true;
        graph_check.amounts[bus][wheel] = 1.0;
        let mut probe = crate::routing::Graph::new();
        probe.set_topology(&graph_check);
        probe.begin_sample();
        probe.write(source::WHEEL, 1.0);
        assert_eq!(
            probe.sum(bus, &graph_check),
            0.0,
            "the wheel must not reach the bus it multiplies"
        );

        // 2. The wheel sets the depth of the vibrato.
        let (down_low, down_high) = swing(0.0, true);
        let (half_low, half_high) = swing(0.5, true);
        let (up_low, up_high) = swing(1.0, true);
        assert_eq!(
            (down_low, down_high),
            (0.0, 0.0),
            "wheel down is exactly no vibrato"
        );
        assert!(
            (half_high - half_low) > 0.0 && (up_high - up_low) > (half_high - half_low) * 1.5,
            "the wheel must set how much vibrato there is: {half_low}..{half_high}              against {up_low}..{up_high}"
        );

        // 3. With the LFO stopped the wheel moves nothing — it is a depth, not an offset.
        for wheel in [0.0, 0.5, 1.0] {
            assert_eq!(
                swing(wheel, false),
                (0.0, 0.0),
                "the wheel must not transpose with nothing in the bus"
            );
        }
    }
}
