//! Whole control-side coordinator and its fixed per-sample event order.
//!
//! 1. Apply parameter-mode transitions (including explicit panic re-arm edges).
//! 2. Apply same-sample host events in slice order; trigger pulses coalesce.
//! 3. Resolve glide and the sounding pitch, then **publish the event-driven gestures**.
//! 4. The caller publishes the noise output.
//! 5. Sum the LFO-rate target, advance the LFO, then resolve Drone > Repeat > keyboard and publish
//!    the resulting gate.
//! 6. Apply the shared gate/trigger to both envelopes and publish them.
//! 7. Sum the `Mod bus` module and publish it, then **multiply** the `Wheel bus` module and
//!    publish that.
//!
//! Steps 3 to 7 are **publication order, and it is the same as the declared source order** in
//! [`crate::routing`] — which is what makes the unit delay mean anything. The caller continues with
//! Oscillator B and then Oscillator A, so B's own routes read the preceding sample; that is the
//! machine's deliberately-last B commit, and it is a row in [`crate::routing::FORWARD_THROUGH`]
//! rather than a warning about a line's position.
//!
//! This order is independent of host block partitioning. A note-on after All Sound Off at one
//! timestamp deliberately re-arms; the reverse order deliberately leaves the panic latched.
//!
//! **A sample that does not run publishes nothing and never opens the frame**, which is what stops
//! repeated host calls on an inert plugin altering the next phrase — the shared frame needs no
//! read-without-commit mode to give that, only the discipline of not writing.

use mxm_modulation::standard;

use crate::{
    envelope::{Adsr, AdsrParams},
    finite_or,
    keyboard::{KeyMode, Keyboard, NoteId, Outcome, Owner},
    lfo::{Lfo, Output as LfoOutput, WaveEnables},
    routing::{Graph, Routing, source, target},
    signal::{Cv, Gate, Pulse},
};

pub const DEFAULT_KEY: u8 = 60;
pub const GLIDE_REFERENCE_SEMITONES: f64 = 36.0;
pub const GLIDE_MAX_S: f32 = 30.0; // Chosen guard; service evidence establishes only >=3 s/3 oct.

/// `2^x`, in the one place a routed octave offset needs it.
#[inline]
fn exp2(octaves: f32) -> f32 {
    2.0f32.powf(octaves)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlideMode {
    Normal,
    #[default]
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GateSource {
    #[default]
    None,
    Keyboard,
    LfoClock,
    Drone,
    Panic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Activity {
    Live,
    Tailing,
    #[default]
    Inert,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    NoteOn(NoteId),
    NoteOff {
        voice_id: Option<i32>,
        channel: u8,
        key: u8,
    },
    Choke {
        voice_id: Option<i32>,
        channel: u8,
        key: u8,
    },
    AllNotesOff,
    AllSoundOff,
    PerNoteTuning {
        voice_id: Option<i32>,
        channel: u8,
        key: u8,
        semitones: f32,
    },
    PitchBend {
        channel: u8,
        normalized: f32,
    },
    ModWheel {
        channel: u8,
        value: f32,
    },
    /// Channel pressure. **Retained per channel whether or not a note sounds**, so a note started
    /// while a key is already leaned on inherits it rather than beginning at zero.
    ChannelPressure {
        channel: u8,
        value: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Inputs {
    /// The noise output, before the mixer level.
    ///
    /// The caller owns it because the noise generator lives with the audio path, and it is
    /// published as [`crate::routing::source::NOISE`] before the LFO ticks.
    pub noise: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    pub key_mode: KeyMode,
    pub glide_mode: GlideMode,
    pub glide_time_s: f32,
    pub repeat: bool,
    pub drone: bool,
    pub lfo_rate_hz: f32,
    pub lfo_waves: WaveEnables,
    pub filter_envelope: AdsrParams,
    pub amplifier_envelope: AdsrParams,
    /// Current patch reach for the normalized per-channel pitch-bend position.
    pub bend_range_semitones: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            key_mode: KeyMode::Normal,
            glide_mode: GlideMode::Auto,
            glide_time_s: 0.0,
            repeat: false,
            drone: false,
            lfo_rate_hz: 4.0,
            lfo_waves: WaveEnables::default(),
            filter_envelope: AdsrParams::default(),
            amplifier_envelope: AdsrParams::default(),
            bend_range_semitones: 7.0,
        }
    }
}

/// What [`Controller::begin_sample`] settled before any source but the gestures was published.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Begun {
    /// Whether this sample runs at all. A sample that does not run publishes nothing.
    pub runs: bool,
    pub owner: Owner,
    pub pitch_semitones: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub owner: Owner,
    pub pitch_semitones: f32,
    pub gate: Gate,
    pub trigger: Pulse,
    pub gate_source: GateSource,
    pub filter_envelope: Cv,
    pub amplifier_envelope: Cv,
    pub lfo: LfoOutput,
    /// The LFO's rate after its own routed modulation, in Hz.
    pub lfo_rate_hz: f32,
    /// The summing module's published value, **in frame units**.
    pub mod_bus: Cv,
    /// The multiplier module's published value, **in frame units**. As it ships that is
    /// [`Frame::mod_bus`] times the wheel, but either factor can be re-pointed at any source.
    pub wheel_bus: Cv,
    pub activity: Activity,
    pub panic_latched: bool,
}

#[derive(Debug, Clone)]
struct Glide {
    current: f64,
    target: f64,
}

impl Glide {
    const fn new(note: u8) -> Self {
        Self {
            current: note as f64,
            target: note as f64,
        }
    }

    fn reset(&mut self, note: u8) {
        *self = Self::new(note);
    }

    fn target(&mut self, semitones: f32, slew: bool) {
        self.target = finite_or(semitones, DEFAULT_KEY as f32) as f64;
        if !slew {
            self.current = self.target;
        }
    }

    fn settle(&mut self) {
        self.current = self.target;
    }

    fn process(&mut self, time_s: f32, sample_rate: f32) -> f32 {
        let time = finite_or(time_s, 0.0).clamp(0.0, GLIDE_MAX_S) as f64;
        if time <= 0.0 {
            self.settle();
        } else {
            let step = GLIDE_REFERENCE_SEMITONES
                / (time * finite_or(sample_rate, 48_000.0).max(1.0) as f64);
            let error = self.target - self.current;
            if error.abs() <= step {
                self.current = self.target;
            } else {
                self.current += step.copysign(error);
            }
        }
        self.current as f32
    }
}

#[derive(Debug, Clone)]
pub struct Controller {
    sample_rate: f32,
    keyboard: Keyboard,
    glide: Glide,
    filter_envelope: Adsr,
    amplifier_envelope: Adsr,
    lfo: Lfo,
    graph: Graph,
    /// Which sources are live into which targets, and how much of each.
    ///
    /// **It lives here rather than in [`Params`]** because `Params` is `Copy` and a plugin rebuilds
    /// it every sample: a 125-pair grid inside it would be copied per sample for values that change
    /// only on a parameter event. Topology arrives through [`Controller::set_topology`] once per
    /// interval; the live amounts are advanced in place through [`Controller::routing_mut`].
    routing: Routing,
    effective_gate: bool,
    /// The velocity of **the press that last triggered the envelopes** — the Velocity source, by
    /// the modulation standard. Full before any press, so the source rests at zero.
    envelope_velocity: f32,
    panic_latched: bool,
    previous_repeat: bool,
    previous_drone: bool,
    pending_keyboard_trigger: bool,
    pending_cut: bool,
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

impl Controller {
    /// **Not `const`, deliberately**: a controller is armed with the init topology as it is built,
    /// so no path can reach the render loop with an empty compacted list. A caller holding any
    /// other routing owes [`Controller::set_topology`] before the next rendered sample.
    pub fn new() -> Self {
        let mut controller = Self {
            sample_rate: 48_000.0,
            keyboard: Keyboard::new(DEFAULT_KEY, 0),
            glide: Glide::new(DEFAULT_KEY),
            filter_envelope: Adsr::new(),
            amplifier_envelope: Adsr::new(),
            lfo: Lfo::new(),
            graph: Graph::new(),
            routing: Routing::init(),
            effective_gate: false,
            envelope_velocity: 1.0,
            panic_latched: false,
            previous_repeat: false,
            previous_drone: false,
            pending_keyboard_trigger: false,
            pending_cut: false,
        };
        controller.set_topology(&Routing::init());
        controller
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = finite_or(sample_rate, 48_000.0).max(1.0);
        self.filter_envelope.set_sample_rate(self.sample_rate);
        self.amplifier_envelope.set_sample_rate(self.sample_rate);
    }

    pub fn reset(&mut self) {
        self.keyboard.reset();
        self.glide.reset(DEFAULT_KEY);
        self.filter_envelope.reset();
        self.amplifier_envelope.reset();
        self.lfo.reset();
        self.graph.reset();
        self.effective_gate = false;
        self.envelope_velocity = 1.0;
        self.panic_latched = false;
        self.previous_repeat = false;
        self.previous_drone = false;
        self.pending_keyboard_trigger = false;
        self.pending_cut = false;
    }

    pub fn keyboard(&self) -> &Keyboard {
        &self.keyboard
    }

    pub fn panic_latched(&self) -> bool {
        self.panic_latched
    }

    /// Adopt a routing grid and rebuild which routes are live. **Once per processing interval,
    /// never per sample.**
    ///
    /// Every path that can reach the render loop owes this: a voice started without it renders with
    /// nothing routed, which is a defect the collection has already shipped once.
    pub fn set_topology(&mut self, routing: &Routing) {
        self.routing = *routing;
        self.graph.set_topology(routing);
    }

    /// The live amounts, for a caller advancing its smoothers in place each sample.
    ///
    /// **Presences here are read-only in practice**: changing one without
    /// [`Controller::set_topology`] leaves the compacted lists disagreeing with the grid, and the
    /// compacted lists are what the sums visit.
    #[inline]
    pub fn routing_mut(&mut self) -> &mut Routing {
        &mut self.routing
    }

    /// The routing grid, as it stands.
    #[inline]
    pub fn routing(&self) -> &Routing {
        &self.routing
    }

    /// Whether anything reads that source, so a caller can skip producing a value for it.
    #[inline]
    pub fn needs(&self, source: usize) -> bool {
        self.graph.needs(source)
    }

    /// Publish an audio-rate source the voice produces, in its own raw domain.
    ///
    /// Called by the whole voice between target sums, in [`crate::routing`]'s declared order.
    #[inline]
    pub fn publish(&mut self, source: usize, raw: f32) {
        self.graph.write(source, raw);
    }

    /// A target's summed modulation, in the target's own domain.
    #[inline]
    pub fn sum(&self, target: usize) -> f32 {
        self.graph.sum(target, &self.routing)
    }

    /// What the frame holds for `source`, in raw units, for tests.
    #[cfg(test)]
    pub(crate) fn published_for_test(&self, source: usize) -> f32 {
        self.graph.read_for_test(source)
    }

    /// Whether any route is live into `target`, so a caller can leave an unrouted target's
    /// arithmetic exactly as it was.
    #[inline]
    pub fn routes_into(&self, target: usize) -> bool {
        !self.graph.is_empty(target)
    }

    fn apply_outcome(&mut self, outcome: Outcome, glide_mode: GlideMode) {
        if outcome.sounding_changed {
            let slew = match glide_mode {
                GlideMode::Normal => true,
                GlideMode::Auto => outcome.overlap_change,
            };
            self.glide.target(self.keyboard.owner().key as f32, slew);
        }
        self.pending_keyboard_trigger |= outcome.trigger;
        self.pending_cut |= outcome.cut;
    }

    fn apply_event(&mut self, event: Event, params: Params) {
        match event {
            Event::NoteOn(id) => {
                self.panic_latched = false;
                let outcome = self.keyboard.note_on(id);
                self.apply_outcome(outcome, params.glide_mode);
            }
            Event::NoteOff {
                voice_id,
                channel,
                key,
            } => {
                let outcome = self.keyboard.note_off(voice_id, channel, key);
                self.apply_outcome(outcome, params.glide_mode);
            }
            Event::Choke {
                voice_id,
                channel,
                key,
            } => {
                let outcome = self.keyboard.choke(voice_id, channel, key);
                self.apply_outcome(outcome, params.glide_mode);
            }
            Event::AllNotesOff => {
                let outcome = self.keyboard.all_notes_off();
                self.apply_outcome(outcome, params.glide_mode);
            }
            Event::AllSoundOff => {
                self.keyboard.all_notes_off();
                self.filter_envelope.silence();
                self.amplifier_envelope.silence();
                self.graph.reset();
                self.effective_gate = false;
                // Every press is gone, so no phrase is left to keep: the Velocity source rests.
                self.envelope_velocity = 1.0;
                self.pending_keyboard_trigger = false;
                self.pending_cut = false;
                self.panic_latched = true;
            }
            Event::PerNoteTuning {
                voice_id,
                channel,
                key,
                semitones,
            } => {
                self.keyboard.set_tuning(voice_id, channel, key, semitones);
            }
            Event::PitchBend {
                channel,
                normalized,
            } => {
                self.keyboard.set_channel_bend(channel, normalized);
            }
            Event::ModWheel { channel, value } => {
                self.keyboard.set_channel_wheel(channel, value);
            }
            Event::ChannelPressure { channel, value } => {
                self.keyboard.set_channel_pressure(channel, value);
            }
        }
    }

    fn gate_source(&self, params: Params, lfo: LfoOutput) -> (GateSource, bool) {
        if self.panic_latched {
            (GateSource::Panic, false)
        } else if params.drone {
            (GateSource::Drone, true)
        } else if params.repeat {
            (GateSource::LfoClock, lfo.clock_high)
        } else if self.keyboard.is_held() {
            (GateSource::Keyboard, true)
        } else {
            (GateSource::None, false)
        }
    }

    fn apply_parameter_transitions(&mut self, params: Params) {
        // A rising edge after leaving either autonomous mode is an explicit re-arm. An unrelated
        // edit and merely processing another sample are not.
        if self.panic_latched
            && ((!self.previous_repeat && params.repeat) || (!self.previous_drone && params.drone))
        {
            self.panic_latched = false;
        }
        self.previous_repeat = params.repeat;
        self.previous_drone = params.drone;

        let mode_outcome = self.keyboard.set_mode(params.key_mode);
        self.apply_outcome(mode_outcome, params.glide_mode);
    }

    /// Apply sample-aligned host events without advancing any control or audio source.
    ///
    /// A plugin uses this for events at a host callback's end offset, including zero-frame
    /// callbacks. Trigger/cut pulses remain pending for the next rendered sample.
    pub fn handle_events(&mut self, params: Params, events: &[Event]) {
        self.apply_parameter_transitions(params);
        for &event in events {
            self.apply_event(event, params);
        }
    }

    /// Activity after parameter transitions and queued host events, without advancing a sample.
    pub fn activity(&self, params: Params) -> Activity {
        if self.panic_latched {
            Activity::Inert
        } else if self.effective_gate || self.keyboard.is_held() || params.repeat || params.drone {
            Activity::Live
        } else if self.amplifier_envelope.is_active() {
            // Every filter-envelope destination is before the final VCA. Once its amplifier
            // envelope is exactly closed, no filter modulation can make this voice audible.
            Activity::Tailing
        } else {
            Activity::Inert
        }
    }

    /// Step 1: events, transitions, glide and the **event-driven gestures**, published.
    ///
    /// Returns whether this sample runs at all. A sample that does not run never opens the frame,
    /// so nothing it does can be observed by the next phrase.
    pub fn begin_sample(&mut self, params: Params, events: &[Event]) -> Begun {
        self.handle_events(params, events);

        let runs = !self.panic_latched
            && (self.keyboard.is_held()
                || params.repeat
                || params.drone
                || self.amplifier_envelope.is_active());

        let glided_key = if runs {
            self.glide.process(params.glide_time_s, self.sample_rate)
        } else {
            self.glide.settle();
            self.glide.current as f32
        };
        let owner = self.keyboard.owner();
        let pitch_semitones =
            owner.pitch_semitones(params.bend_range_semitones) - owner.key as f32 + glided_key;
        // **The press that triggers the envelopes carries the Velocity source.** Exactly the
        // condition step 5 triggers them on under keyboard articulation — the keyboard is the gate
        // (not Repeat, Drone or a panic), a key is held, and the gate was closed or a press asked
        // for a trigger — every term of which is settled once this sample's events are applied.
        // A fallback to a held key triggers nothing and keeps the phrase's velocity; **Repeat and
        // Drone trigger from their own clock, never from a press**, so they keep it too, and a
        // fresh instance or a reset rests at full.
        if !params.repeat
            && !params.drone
            && !self.panic_latched
            && self.keyboard.is_held()
            && (!self.effective_gate || self.pending_keyboard_trigger)
        {
            self.envelope_velocity = owner.velocity;
        }

        if runs {
            self.graph.begin_sample();
            // Declared source order 0..=4. The key is published from the *sounding pitch*, glide,
            // tuning and bend included, because that is what the retired `filter_keyboard_amount`
            // tracked and a keyboard-tracking filter that ignored glide would not be one.
            // The collection's standard publishers, each zero at its rest. The key keeps this
            // machine's tuning and bend in it, as the retired keyboard amount tracked them.
            self.graph.write(
                source::KEY,
                standard::key(pitch_semitones, crate::routing::KEY_UNIT_SEMITONES),
            );
            self.graph
                .write(source::VELOCITY, standard::velocity(self.envelope_velocity));
            self.graph
                .write(source::WHEEL, standard::wheel(owner.wheel));
            self.graph
                .write(source::PRESSURE, standard::pressure(owner.pressure));
            self.graph
                .write(source::BEND, standard::bend(owner.bend_normalized));
        }

        Begun {
            runs,
            owner,
            pitch_semitones,
        }
    }

    /// Steps 4 to 7: the noise output, the LFO and its own routed rate, the gate, both envelopes,
    /// and the two bus stages.
    ///
    /// `begun` must come from [`Controller::begin_sample`] on this same sample.
    pub fn process_control(&mut self, params: Params, inputs: Inputs, begun: Begun) -> Frame {
        let Begun {
            runs,
            owner,
            pitch_semitones,
        } = begun;

        // 4. The noise, before the mixer level.
        if runs {
            self.graph.write(source::NOISE, inputs.noise);
        }

        // 5. The LFO's rate is itself a target, and it is summed *before* the LFO ticks — so every
        // route into it from the LFO onward is backward, this LFO reading its own rate included.
        let rate_octaves = if runs {
            self.graph.sum(target::LFO_RATE, &self.routing)
        } else {
            0.0
        };
        let lfo_rate_hz = finite_or(params.lfo_rate_hz, crate::lfo::RATE_MIN_HZ)
            * exp2(rate_octaves.clamp(-16.0, 16.0));

        let lfo = if runs {
            self.lfo
                .process(lfo_rate_hz, params.lfo_waves, self.sample_rate)
        } else {
            LfoOutput::default()
        };
        if runs {
            self.graph.write(source::LFO, lfo.sum);
        }

        let (gate_source, gate) = self.gate_source(params, lfo);
        let keyboard_articulation = !params.repeat && !params.drone;
        let trigger = gate
            && (!self.effective_gate || (keyboard_articulation && self.pending_keyboard_trigger));

        if self.pending_cut && !gate {
            self.filter_envelope.silence();
            self.amplifier_envelope.silence();
        } else {
            if trigger {
                self.filter_envelope.trigger();
                self.amplifier_envelope.trigger();
            }
            if self.effective_gate && !gate {
                self.filter_envelope.release();
                self.amplifier_envelope.release();
            }
        }
        self.effective_gate = gate;
        if runs {
            self.graph.write(source::GATE, if gate { 1.0 } else { 0.0 });
        }

        // 6. Both envelopes, then published.
        let mut filter_level = self.filter_envelope.process(params.filter_envelope);
        let amplifier_level = self.amplifier_envelope.process(params.amplifier_envelope);
        // Every filter-envelope route is upstream of the final VCA. Once the amplifier envelope
        // has settled shut, silence the now-inaudible filter state rather than freezing a ghost
        // tail that could reappear if a route is opened later.
        if !gate
            && !self.keyboard.is_held()
            && !params.repeat
            && !params.drone
            && !self.amplifier_envelope.is_active()
        {
            self.filter_envelope.silence();
            filter_level = 0.0;
        }
        if runs {
            self.graph.write(source::FILTER_ENVELOPE, filter_level);
            self.graph
                .write(source::AMPLIFIER_ENVELOPE, amplifier_level);
        }

        // 7. The two modules, in declared order: the sum, then the **multiplier** that reads it.
        //
        // `Mod bus` publishes in frame units already, because `sum` over frame values *is* the bus
        // divided by the unit. `Wheel bus` is decision 1.14's multiplier module — a target whose law
        // is product and whose result is a source — so it is evaluated from its own routes rather
        // than hard-wired. Its law works in **raw** domain (`Graph::product` un-scales each factor),
        // and the raw result is published back through the frame unit like any other source, which
        // is what keeps a cycle through it finite.
        //
        // At Init its two factors are `Mod bus` and `Wheel`, both at full amount, so what comes out
        // is `Mod bus × wheel` — the machine's own two-stage routing — and a player can re-point
        // either factor at anything else.
        let (mod_bus, wheel_bus) = if runs {
            let bus = self.graph.sum(target::MOD_BUS, &self.routing);
            self.graph.write_in_frame_units(source::MOD_BUS, bus);
            let product = self.graph.product(target::WHEEL_BUS, &self.routing);
            self.graph.write(source::WHEEL_BUS, product);
            (bus, product * crate::routing::FRAME_UNIT)
        } else {
            (0.0, 0.0)
        };

        let activity = self.activity(params);

        self.pending_keyboard_trigger = false;
        self.pending_cut = false;

        Frame {
            owner,
            pitch_semitones,
            gate: Gate(gate),
            trigger: Pulse(trigger),
            gate_source,
            filter_envelope: Cv::new(filter_level),
            amplifier_envelope: Cv::new(amplifier_level),
            lfo,
            lfo_rate_hz,
            mod_bus: Cv::new(mod_bus),
            wheel_bus: Cv::new(wheel_bus),
            activity,
            panic_latched: self.panic_latched,
        }
    }

    /// The whole control side of one sample, for a caller with no audio path of its own.
    ///
    /// The audio sources stay unpublished, so every route from one of them reads the preceding
    /// sample — which is what they would do anyway, being declared last.
    #[inline]
    pub fn process_sample(&mut self, params: Params, inputs: Inputs, events: &[Event]) -> Frame {
        let begun = self.begin_sample(params, events);
        self.process_control(params, inputs, begun)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 1_000.0;

    fn note_on(key: u8) -> Event {
        Event::NoteOn(NoteId::keyed(None, 0, key))
    }

    fn note_off(key: u8) -> Event {
        Event::NoteOff {
            voice_id: None,
            channel: 0,
            key,
        }
    }

    fn controller() -> Controller {
        let mut controller = Controller::new();
        controller.set_sample_rate(FS);
        controller
    }

    #[test]
    fn normal_and_retrig_drive_the_shared_envelope_trigger_differently() {
        let mut normal = controller();
        let params = Params::default();
        assert!(
            normal
                .process_sample(params, Inputs::default(), &[note_on(60)])
                .trigger
                .0
        );
        assert!(
            !normal
                .process_sample(params, Inputs::default(), &[note_on(48)])
                .trigger
                .0
        );

        let mut retrig = controller();
        let params = Params {
            key_mode: KeyMode::Retrig,
            ..Params::default()
        };
        retrig.process_sample(params, Inputs::default(), &[note_on(60)]);
        assert!(
            retrig
                .process_sample(params, Inputs::default(), &[note_on(48)])
                .trigger
                .0
        );
    }

    #[test]
    fn auto_glide_is_overlap_only_while_normal_glide_also_slews_detached_notes() {
        let params = Params {
            glide_time_s: 3.0,
            ..Params::default()
        };
        let mut auto = controller();
        let first = auto.process_sample(params, Inputs::default(), &[note_on(72)]);
        assert_eq!(first.pitch_semitones, 72.0, "detached AUTO note jumps");
        let overlap = auto.process_sample(params, Inputs::default(), &[note_on(48)]);
        assert!(overlap.pitch_semitones > 48.0 && overlap.pitch_semitones < 72.0);

        let mut normal = controller();
        let params = Params {
            glide_mode: GlideMode::Normal,
            glide_time_s: 3.0,
            ..Params::default()
        };
        let first = normal.process_sample(params, Inputs::default(), &[note_on(72)]);
        assert!(first.pitch_semitones > 60.0 && first.pitch_semitones < 72.0);
    }

    #[test]
    fn repeat_is_clocked_by_the_lfo_core() {
        let params = Params {
            repeat: true,
            lfo_rate_hz: 1.0,
            ..Params::default()
        };
        let frame = controller().process_sample(params, Inputs::default(), &[]);
        assert_eq!(
            (frame.gate_source, frame.gate.0),
            (GateSource::LfoClock, true)
        );
    }

    #[test]
    fn drone_overrides_repeat_and_key_presses_do_not_retrigger_autonomous_modes() {
        let mut controller = controller();
        let params = Params {
            repeat: true,
            drone: true,
            ..Params::default()
        };
        let first = controller.process_sample(params, Inputs::default(), &[]);
        assert_eq!(
            (first.gate_source, first.trigger.0),
            (GateSource::Drone, true)
        );
        let key = controller.process_sample(params, Inputs::default(), &[note_on(72)]);
        assert!(!key.trigger.0);
        assert_eq!(key.gate_source, GateSource::Drone);
    }

    #[test]
    fn all_sound_off_latches_until_one_of_the_three_deliberate_rearms() {
        let mut controller = controller();
        let drone = Params {
            drone: true,
            ..Params::default()
        };
        controller.process_sample(drone, Inputs::default(), &[]);
        let panic = controller.process_sample(drone, Inputs::default(), &[Event::AllSoundOff]);
        assert_eq!((panic.activity, panic.gate.0), (Activity::Inert, false));
        for _ in 0..20 {
            let frame = controller.process_sample(drone, Inputs::default(), &[]);
            assert!(frame.panic_latched && !frame.gate.0);
        }
        controller.process_sample(Params::default(), Inputs::default(), &[]);
        let rearmed = controller.process_sample(drone, Inputs::default(), &[]);
        assert!(!rearmed.panic_latched && rearmed.gate.0 && rearmed.trigger.0);

        controller.process_sample(drone, Inputs::default(), &[Event::AllSoundOff]);
        let note = controller.process_sample(drone, Inputs::default(), &[note_on(64)]);
        assert!(!note.panic_latched && note.gate.0);
        controller.reset();
        assert!(!controller.panic_latched());
    }

    #[test]
    fn coincident_events_obey_their_slice_order() {
        let mut first = controller();
        let frame = first.process_sample(
            Params::default(),
            Inputs::default(),
            &[Event::AllSoundOff, note_on(60)],
        );
        assert!(!frame.panic_latched && frame.gate.0);

        let mut second = controller();
        let frame = second.process_sample(
            Params::default(),
            Inputs::default(),
            &[note_on(60), Event::AllSoundOff],
        );
        assert!(frame.panic_latched && !frame.gate.0);
    }

    #[test]
    fn all_notes_off_releases_a_tail_while_choke_cuts_and_panic_clears() {
        let mut released = controller();
        let params = Params::default();
        released.process_sample(params, Inputs::default(), &[note_on(60)]);
        for _ in 0..50 {
            released.process_sample(params, Inputs::default(), &[]);
        }
        let release = released.process_sample(params, Inputs::default(), &[Event::AllNotesOff]);
        assert_eq!(release.activity, Activity::Tailing);

        let mut cut = controller();
        cut.process_sample(params, Inputs::default(), &[note_on(60)]);
        let frame = cut.process_sample(
            params,
            Inputs::default(),
            &[Event::Choke {
                voice_id: None,
                channel: 0,
                key: 60,
            }],
        );
        assert_eq!(
            (frame.activity, frame.amplifier_envelope.value()),
            (Activity::Inert, 0.0)
        );
    }

    #[test]
    fn filter_release_cannot_outlive_the_final_vca_even_when_routed() {
        let envelope = |release_s| AdsrParams {
            attack_s: 0.002,
            decay_s: 0.002,
            sustain: 1.0,
            release_s,
        };
        let disconnected = Params {
            filter_envelope: envelope(2.0),
            amplifier_envelope: envelope(0.002),
            ..Params::default()
        };
        // The dedicated filter-envelope amount is a route now, so "routed" means the pair is
        // present at full depth rather than a knob being turned.
        let routed = disconnected;
        let mut wired = Routing::new();
        wired.present[target::FILTER_CUTOFF][source::FILTER_ENVELOPE] = true;
        wired.amounts[target::FILTER_CUTOFF][source::FILTER_ENVELOPE] = 1.0;
        let (mut parked, mut connected) = (controller(), controller());
        parked.set_topology(&Routing::new());
        connected.set_topology(&wired);
        parked.process_sample(disconnected, Inputs::default(), &[note_on(60)]);
        connected.process_sample(routed, Inputs::default(), &[note_on(60)]);
        for _ in 0..20 {
            parked.process_sample(disconnected, Inputs::default(), &[]);
            connected.process_sample(routed, Inputs::default(), &[]);
        }
        parked.process_sample(disconnected, Inputs::default(), &[Event::AllNotesOff]);
        connected.process_sample(routed, Inputs::default(), &[Event::AllNotesOff]);

        let mut parked_frame = parked.process_sample(disconnected, Inputs::default(), &[]);
        let mut connected_frame = connected.process_sample(routed, Inputs::default(), &[]);
        let mut routed_while_vca_open = false;
        for _ in 0..200 {
            parked_frame = parked.process_sample(disconnected, Inputs::default(), &[]);
            connected_frame = connected.process_sample(routed, Inputs::default(), &[]);
            routed_while_vca_open |= connected_frame.activity == Activity::Tailing
                && connected_frame.amplifier_envelope.value() > 0.0
                && connected_frame.filter_envelope.value() > 0.0;
            if parked_frame.activity == Activity::Inert
                && connected_frame.activity == Activity::Inert
            {
                break;
            }
        }
        assert!(
            routed_while_vca_open,
            "the route must work while the VCA is open"
        );
        assert_eq!(parked_frame.activity, Activity::Inert);
        assert_eq!(parked_frame.filter_envelope.value(), 0.0);
        let first_parked = parked
            .process_sample(disconnected, Inputs::default(), &[])
            .lfo;
        let second_parked = parked
            .process_sample(disconnected, Inputs::default(), &[])
            .lfo;
        assert_eq!(first_parked, LfoOutput::default());
        assert_eq!(
            second_parked, first_parked,
            "the parked core cannot advance"
        );
        assert_eq!(connected_frame.activity, Activity::Inert);
        assert_eq!(
            connected_frame.filter_envelope.value(),
            0.0,
            "a pre-VCA route cannot make a closed VCA audible"
        );

        let rerouted = parked.process_sample(routed, Inputs::default(), &[]);
        assert_eq!(rerouted.activity, Activity::Inert);
        assert_eq!(rerouted.filter_envelope.value(), 0.0, "no ghost tail");
    }

    #[test]
    fn inert_calls_freeze_free_running_phase_and_settle_targeted_state() {
        let (mut untouched, mut idled) = (controller(), controller());
        for _ in 0..10_000 {
            let frame = idled.process_sample(Params::default(), Inputs::default(), &[]);
            assert_eq!(frame.activity, Activity::Inert);
        }
        let repeat = Params {
            repeat: true,
            lfo_waves: WaveEnables {
                saw: true,
                triangle: true,
                square: true,
            },
            ..Params::default()
        };
        for _ in 0..2_000 {
            let a = untouched.process_sample(repeat, Inputs::default(), &[]);
            let b = idled.process_sample(repeat, Inputs::default(), &[]);
            assert_eq!((a.lfo, a.gate, a.trigger), (b.lfo, b.gate, b.trigger));
        }
    }

    #[test]
    fn release_and_repress_are_sample_partition_independent() {
        let mut a = controller();
        let mut b = controller();
        let params = Params::default();
        for sample in 0..500 {
            let events: &[Event] = match sample {
                0 => &[note_on(60)],
                100 => &[note_on(48)],
                200 => &[note_off(48)],
                300 => &[Event::AllNotesOff],
                _ => &[],
            };
            let fa = a.process_sample(params, Inputs::default(), events);
            let fb = b.process_sample(params, Inputs::default(), events);
            assert_eq!(fa, fb);
        }
    }

    /// A grid whose cutoff reads one source at full depth, for the gesture tests below.
    fn routed_to_cutoff(slot: usize) -> Routing {
        let mut routing = Routing::new();
        routing.present[target::FILTER_CUTOFF][slot] = true;
        routing.amounts[target::FILTER_CUTOFF][slot] = 1.0;
        routing
    }

    fn armed(routing: &Routing) -> Controller {
        let mut controller = controller();
        controller.set_topology(routing);
        controller
    }

    fn fast_envelope() -> AdsrParams {
        AdsrParams {
            attack_s: 0.001,
            decay_s: 0.001,
            sustain: 1.0,
            release_s: 0.001,
        }
    }

    /// **The owner's velocity belongs to the sounding press**, so it follows Normal/Retrig selection
    /// and release fallback exactly as pitch does — and it is what an envelope trigger latches as
    /// the Velocity source. The published source keeps the triggering press's through all of this
    /// (`conformance::tests::velocity_is_the_press_that_last_triggered_the_envelopes`).
    #[test]
    fn velocity_follows_the_sounding_press_through_selection_and_fallback() {
        let routing = routed_to_cutoff(source::VELOCITY);
        let params = Params {
            key_mode: KeyMode::Normal,
            ..Params::default()
        };
        let mut controller = armed(&routing);

        let soft = Event::NoteOn(NoteId {
            voice_id: None,
            channel: 0,
            key: 48,
            velocity: 0.25,
        });
        let hard = Event::NoteOn(NoteId {
            voice_id: None,
            channel: 0,
            key: 72,
            velocity: 1.0,
        });

        let frame = controller.process_sample(params, Inputs::default(), &[soft]);
        assert_eq!(frame.owner.velocity, 0.25);
        assert_eq!(controller.published_for_test(source::VELOCITY), -0.75);

        // Normal is low-note priority: the higher, harder press does not take the voice.
        let frame = controller.process_sample(params, Inputs::default(), &[hard]);
        assert_eq!(
            frame.owner.velocity, 0.25,
            "the sounding press keeps the voice, so it keeps its velocity"
        );

        // Releasing the sounding press falls back to the one still held, velocity and all.
        let frame = controller.process_sample(
            params,
            Inputs::default(),
            &[Event::NoteOff {
                voice_id: None,
                channel: 0,
                key: 48,
            }],
        );
        assert_eq!(
            frame.owner.velocity, 1.0,
            "fallback carries its own velocity"
        );
        assert_eq!(
            controller.published_for_test(source::VELOCITY),
            -0.75,
            "a fallback triggers nothing, so the source keeps the triggering press's"
        );
    }

    /// Channel pressure is **retained**, which is the whole point: a message arriving with nothing
    /// held must be inherited by the next note, or leaning on the keyboard before pressing a key
    /// gives that note no pressure at all.
    #[test]
    fn channel_pressure_is_retained_and_inherited_by_a_later_note() {
        let routing = routed_to_cutoff(source::PRESSURE);
        let params = Params::default();
        let mut controller = armed(&routing);

        // Arrives with nothing sounding.
        controller.handle_events(
            params,
            &[Event::ChannelPressure {
                channel: 0,
                value: 0.75,
            }],
        );
        let frame = controller.process_sample(params, Inputs::default(), &[note_on(60)]);
        assert_eq!(
            frame.owner.pressure, 0.75,
            "a note started under an existing lean inherits it"
        );

        // It follows the owner while the note sounds.
        let frame = controller.process_sample(
            params,
            Inputs::default(),
            &[Event::ChannelPressure {
                channel: 0,
                value: 0.25,
            }],
        );
        assert_eq!(frame.owner.pressure, 0.25);

        // Another channel's pressure does not reach this owner.
        let frame = controller.process_sample(
            params,
            Inputs::default(),
            &[Event::ChannelPressure {
                channel: 5,
                value: 1.0,
            }],
        );
        assert_eq!(frame.owner.pressure, 0.25);

        // **All Sound Off does not move it, and that is the decision rather than an omission.**
        // Panic silences the voice; it does not decide where the player's hand is, which is exactly
        // how the wheel and the bender already behave. Only `reset` means *this instance has no
        // history*.
        controller.process_sample(params, Inputs::default(), &[Event::AllSoundOff]);
        let frame = controller.process_sample(params, Inputs::default(), &[note_on(60)]);
        assert_eq!(
            frame.owner.pressure, 0.25,
            "a note after panic inherits the lean the player is still applying"
        );

        // Reset returns every gesture to neutral with everything else.
        controller.reset();
        let frame = controller.process_sample(params, Inputs::default(), &[note_on(60)]);
        assert_eq!(frame.owner.pressure, 0.0);
        assert_eq!(frame.owner.velocity, 0.0);
    }

    /// **The wheel brings in vibrato at Init** (the owner, 2026-09-28: the wheel did nothing on a
    /// fresh instance). The LFO feeds `Mod bus` and the wheel-scaled bus reaches both oscillators'
    /// pitch, so with a note held the wheel pushed up moves `Wheel bus` with the LFO — and at rest
    /// it is exactly zero, which is why Init and every factory sound render as they did.
    #[test]
    fn the_wheel_brings_in_vibrato_at_init() {
        let mut params = Params::default();
        params.lfo_waves.triangle = true;
        let reach = |wheel: f32| {
            let mut controller = armed(&Routing::init());
            controller.process_sample(
                params,
                Inputs::default(),
                &[Event::ModWheel {
                    channel: 0,
                    value: wheel,
                }],
            );
            controller.process_sample(params, Inputs::default(), &[note_on(60)]);
            (0..24_000)
                .map(|_| {
                    controller
                        .process_sample(params, Inputs::default(), &[])
                        .wheel_bus
                        .value()
                        .abs()
                })
                .fold(0.0f32, f32::max)
        };
        assert_eq!(reach(0.0), 0.0, "the wheel at rest must add nothing");
        assert!(
            reach(1.0) > 0.05,
            "the wheel pushed up brings the LFO in: {}",
            reach(1.0)
        );
    }

    /// A gesture parked non-neutral and routed cannot keep the instrument awake, because every
    /// target this instrument declares is upstream of an envelope-bounded VCA. That is the property
    /// the amplitude and gate exclusions protect, and it is asserted rather than assumed.
    #[test]
    fn a_parked_gesture_routed_at_full_depth_still_reaches_exact_inert() {
        let mut routing = routed_to_cutoff(source::WHEEL);
        routing.present[target::LFO_RATE][source::WHEEL] = true;
        routing.amounts[target::LFO_RATE][source::WHEEL] = 1.0;
        routing.present[target::RESONANCE][source::WHEEL] = true;
        routing.amounts[target::RESONANCE][source::WHEEL] = 1.0;
        let params = Params {
            amplifier_envelope: fast_envelope(),
            ..Params::default()
        };
        let mut controller = armed(&routing);
        controller.process_sample(
            params,
            Inputs::default(),
            &[Event::ModWheel {
                channel: 0,
                value: 1.0,
            }],
        );
        controller.process_sample(params, Inputs::default(), &[note_on(60)]);
        for _ in 0..50 {
            controller.process_sample(params, Inputs::default(), &[]);
        }
        controller.process_sample(params, Inputs::default(), &[Event::AllNotesOff]);
        let mut frame = controller.process_sample(params, Inputs::default(), &[]);
        for _ in 0..5_000 {
            frame = controller.process_sample(params, Inputs::default(), &[]);
            if frame.activity == Activity::Inert {
                break;
            }
        }
        assert_eq!(
            frame.activity,
            Activity::Inert,
            "a wheel parked up must not hold this instrument live"
        );
        assert_eq!(frame.amplifier_envelope.value(), 0.0);
    }

    /// An inert stretch must not move anything a later phrase can observe. A sample that does not
    /// run never opens the frame, which is what replaces the retired read-without-commit mode.
    #[test]
    fn repeated_inert_samples_cannot_alter_the_next_phrase() {
        let routing = routed_to_cutoff(source::LFO);
        let mut params = Params {
            amplifier_envelope: fast_envelope(),
            ..Params::default()
        };
        params.lfo_waves.saw = true;

        let render = |idle: usize| {
            let mut controller = armed(&routing);
            controller.process_sample(params, Inputs::default(), &[note_on(60)]);
            for _ in 0..20 {
                controller.process_sample(params, Inputs::default(), &[]);
            }
            controller.process_sample(params, Inputs::default(), &[Event::AllNotesOff]);
            for _ in 0..5_000 {
                if controller
                    .process_sample(params, Inputs::default(), &[])
                    .activity
                    == Activity::Inert
                {
                    break;
                }
            }
            for _ in 0..idle {
                controller.process_sample(params, Inputs::default(), &[]);
            }
            controller.process_sample(params, Inputs::default(), &[note_on(60)]);
            (0..40)
                .map(|_| {
                    let f = controller.process_sample(params, Inputs::default(), &[]);
                    (f.lfo.sum.to_bits(), f.filter_envelope.value().to_bits())
                })
                .collect::<Vec<_>>()
        };

        assert_eq!(
            render(0),
            render(4_321),
            "the length of an inert gap must not reach the next phrase"
        );
    }

    /// **`FORWARD_THROUGH` is checked against what the code does, not against itself.**
    ///
    /// `routing::tests::every_target_is_summed_after_a_declared_source` compares constants with
    /// constants: it would stay green through any reordering of `process_control`. This drives real
    /// samples and reads the delay off the frame the controller publishes, so a call moved above or
    /// below its declared position goes red.
    ///
    /// Three cases, chosen because each is observable from [`Frame`] and because between them they
    /// cover every class the table distinguishes: a **forward** control source, a **backward**
    /// control source into a target summed early, and a **backward** audio source.
    #[test]
    fn the_declared_publication_order_is_the_order_the_code_runs() {
        /// Inside the LFO's own range, so nothing is clamped away before the routed offset applies.
        const BASE_RATE_HZ: f32 = 4.0;

        fn frames(routing: Routing, oscillator_b: impl Fn(usize) -> f32) -> Vec<Frame> {
            let mut params = Params {
                drone: true,
                ..Params::default()
            };
            params.lfo_waves.saw = true;
            params.lfo_rate_hz = BASE_RATE_HZ;
            let mut controller = armed(&routing);
            controller.set_sample_rate(FS);
            (0..64)
                .map(|n| {
                    // The voice publishes Oscillator B between the module and A; here the test
                    // stands in for it so the delay is unambiguous.
                    let begun = controller.begin_sample(params, &[]);
                    let frame = controller.process_control(params, Inputs::default(), begun);
                    controller.publish(source::OSCILLATOR_B, oscillator_b(n));
                    frame
                })
                .collect()
        }

        // 1. **Forward.** `Mod bus` is summed after the LFO is published, so the bus at sample n
        //    carries *this* sample's LFO.
        let mut forward = Routing::new();
        forward.present[target::MOD_BUS][source::LFO] = true;
        forward.amounts[target::MOD_BUS][source::LFO] = 1.0;
        let rendered = frames(forward, |_| 0.0);
        let mut moved = false;
        for frame in &rendered {
            let published = (frame.lfo.sum * crate::routing::FRAME_UNIT).clamp(-1.0, 1.0);
            assert!(
                (frame.mod_bus.value() - published).abs() < 1e-6,
                "Mod bus must read the LFO forward: {} against {published}",
                frame.mod_bus.value()
            );
            moved |= frame.lfo.sum != rendered[0].lfo.sum;
        }
        assert!(moved, "the LFO has to move or the test proves nothing");

        // 2. **Backward into an early target.** LFO rate is summed *before* the LFO ticks, so the
        //    rate at sample n reflects the LFO value from sample n-1.
        let mut backward = Routing::new();
        backward.present[target::LFO_RATE][source::LFO] = true;
        backward.amounts[target::LFO_RATE][source::LFO] = 1.0;
        let rendered = frames(backward, |_| 0.0);
        for (n, frame) in rendered.iter().enumerate().skip(1) {
            let previous = (rendered[n - 1].lfo.sum * crate::routing::FRAME_UNIT).clamp(-1.0, 1.0);
            // The target's scale carries `FRAME_SCALE` because the source was published through the
            // frame unit; the two cancel, so a **unit-magnitude** source at full amount reaches
            // `LFO_RATE_OCTAVES`. The LFO is not unit-magnitude — its enabled sum runs to ±3 — so it
            // reaches three times that, exactly as the legacy graph's unnormalised bus did.
            let octaves = previous * crate::routing::FRAME_SCALE * crate::routing::LFO_RATE_OCTAVES;
            let expected = BASE_RATE_HZ * exp2(octaves.clamp(-16.0, 16.0));
            assert!(
                (frame.lfo_rate_hz - expected).abs() <= expected.abs() * 1e-5,
                "LFO rate at {n} must read the preceding sample's LFO: {} against {expected}",
                frame.lfo_rate_hz
            );
        }

        // 3. **Backward audio source.** `Mod bus` is summed before Oscillator B is published, so
        //    the bus at sample n carries B from sample n-1.
        let mut audio = Routing::new();
        audio.present[target::MOD_BUS][source::OSCILLATOR_B] = true;
        audio.amounts[target::MOD_BUS][source::OSCILLATOR_B] = 1.0;
        let b = |n: usize| if n.is_multiple_of(2) { 0.8 } else { -0.8 };
        let rendered = frames(audio, b);
        assert_eq!(
            rendered[0].mod_bus.value(),
            0.0,
            "nothing has been published yet at the first sample"
        );
        for (n, frame) in rendered.iter().enumerate().skip(1) {
            let expected = (b(n - 1) * crate::routing::FRAME_UNIT).clamp(-1.0, 1.0);
            assert!(
                (frame.mod_bus.value() - expected).abs() < 1e-6,
                "Mod bus at {n} must read Oscillator B backward: {} against {expected}",
                frame.mod_bus.value()
            );
        }
    }

    /// A source that starts being read must not deliver a value from a previous phrase.
    ///
    /// While nothing reads a source the instrument does not publish it, so its slot keeps whatever
    /// it held the last time something did. `Graph::set_topology` clears a slot that has just become
    /// needed, and this is what says so through the controller rather than through the frame.
    #[test]
    fn adding_a_route_does_not_read_an_ancient_value() {
        let mut params = Params {
            drone: true,
            ..Params::default()
        };
        params.lfo_waves.saw = true;

        // A phrase in which the LFO is read, so its slot is filled.
        let mut wired = Routing::new();
        wired.present[target::MOD_BUS][source::LFO] = true;
        wired.amounts[target::MOD_BUS][source::LFO] = 1.0;
        let mut controller = armed(&wired);
        controller.set_sample_rate(FS);
        for _ in 0..64 {
            let begun = controller.begin_sample(params, &[]);
            controller.process_control(params, Inputs::default(), begun);
        }

        // The route is removed and the LFO goes unpublished for a long time.
        controller.set_topology(&Routing::new());
        for _ in 0..10_000 {
            let begun = controller.begin_sample(params, &[]);
            controller.process_control(params, Inputs::default(), begun);
        }

        // It is added back at a target that reads the LFO **backward**, so the first sample would
        // otherwise deliver whatever the slot still held from the earlier phrase.
        let mut back = Routing::new();
        back.present[target::LFO_RATE][source::LFO] = true;
        back.amounts[target::LFO_RATE][source::LFO] = 1.0;
        controller.set_topology(&back);
        let begun = controller.begin_sample(params, &[]);
        let frame = controller.process_control(params, Inputs::default(), begun);
        assert_eq!(
            frame.lfo_rate_hz, params.lfo_rate_hz,
            "a newly read source starts from silence, not from an ancient sample"
        );
    }
}
