//! mxm-mono-pr1 — monophonic two-oscillator instrument with any-to-any modulation routing.
//!
//! The framework-free crate owns synthesis and control semantics; this crate owns permanent host
//! identity, parameters, layouts, event translation, realtime block splitting, state, presets,
//! lock-free telemetry and the production editor.

macro_rules! plugin_name {
    () => {
        "mxm-mono-pr1"
    };
}

pub const NAME: &str = plugin_name!();
pub const CLAP_ID: &str = concat!("dk.mxm.", plugin_name!());

pub mod editor;
pub mod params;
pub mod preset;
pub mod routes;
pub mod telemetry;

use mxm_mono_pr1_dsp::{
    control::{Activity, Event},
    keyboard::NoteId,
    voice::{Frame, Voice},
};
use nice_plug::midi::{Channel, Key, VoiceID};
use nice_plug::prelude::*;
use params::MxmMonoPr1Params;
use std::sync::Arc;

/// A note's identity in the shape the voice logic was written for. nice-plug 0.4 types it
/// (`VoiceID`, `Channel`, `Key`, each with a wildcard); 0.3 handed over a host's wildcard (-1) as
/// 255 and a missing voice id as `None`. Converting here keeps every note decision, and every
/// recorded render, exactly what it was before the upgrade.
fn legacy_note(voice_id: VoiceID, channel: Channel, key: Key) -> (Option<i32>, u8, u8) {
    (
        voice_id.id(),
        channel.number().unwrap_or(u8::MAX),
        key.number().unwrap_or(u8::MAX),
    )
}

const MAX_BLOCK_SIZE: usize = 64;
/// Same-sample events are applied in bounded chunks without advancing audio. Chunking preserves
/// order and pulse coalescing while making the queue's realtime capacity independent of host size.
const EVENT_CHUNK_CAPACITY: usize = 64;
const DEV_VIEW_CC: u8 = 119;
const DEV_DISCLOSURE_CC: u8 = 118;
const DEV_BROWSER_CC: u8 = 117;
const DEV_THEME_CC: u8 = 116;
const DEV_CC_ENV: &str = "MXM_DEV_CC";

pub struct MxmMonoPr1 {
    params: Arc<MxmMonoPr1Params>,
    voice: Voice,
    pending_events: Vec<Event>,
    /// The topology the last callback ran, so a route that has **just** become present can have its
    /// smoother snapped to its stored value rather than resumed from a skipped span.
    routing: mxm_mono_pr1_dsp::routing::Routing,
    sample_rate: f32,
    telemetry: Arc<telemetry::Telemetry>,
    dev_cc: bool,
    /// The LFO rate as its sync resolved it for this callback, or `None` for the free rate.
    synced_lfo_hz: Option<f32>,
}

impl Default for MxmMonoPr1 {
    fn default() -> Self {
        Self {
            params: Arc::new(MxmMonoPr1Params::default()),
            voice: Voice::new(),
            // Construction is not realtime. A full chunk is consumed before another event enters.
            pending_events: Vec::with_capacity(EVENT_CHUNK_CAPACITY),
            // The init topology, which is what a freshly constructed voice is armed with.
            routing: mxm_mono_pr1_dsp::routing::Routing::init(),
            sample_rate: 48_000.0,
            telemetry: telemetry::Telemetry::shared(),
            dev_cc: std::env::var_os(DEV_CC_ENV).is_some(),
            synced_lfo_hz: None,
        }
    }
}

impl MxmMonoPr1 {
    /// One sample's voice parameters: the smoothers advanced once, and the LFO rate its division
    /// while synced — routes to the rate apply on top as ever.
    fn voice_params(&self) -> mxm_mono_pr1_dsp::voice::Params {
        let mut params = self.params.next_voice_params();
        if let Some(hz) = self.synced_lfo_hz {
            params.control.lfo_rate_hz = hz;
        }
        params
    }
}

impl MxmMonoPr1 {
    fn queue_host_event(&mut self, event: NoteEvent<()>) {
        let translated = match event {
            NoteEvent::NoteOn {
                voice_id,
                channel,
                key,
                velocity,
                ..
            } if velocity.is_finite() && velocity > 0.0 => {
                let (voice_id, channel, note) = legacy_note(voice_id, channel, key);
                Some(Event::NoteOn(NoteId {
                    voice_id,
                    channel,
                    key: note,
                    // **New MIDI path.** The machine read no velocity; decision 1.7 makes it a
                    // source on every instrument, and it starts at zero depth so a fresh instance
                    // is the copy.
                    velocity: velocity.clamp(0.0, 1.0),
                }))
            }
            NoteEvent::NoteOn {
                voice_id,
                channel,
                key,
                velocity,
                ..
            } if velocity.is_finite() => {
                let (voice_id, channel, note) = legacy_note(voice_id, channel, key);
                Some(Event::NoteOff {
                    voice_id,
                    channel,
                    key: note,
                })
            }
            NoteEvent::NoteOff {
                voice_id,
                channel,
                key,
                ..
            } => {
                let (voice_id, channel, note) = legacy_note(voice_id, channel, key);
                Some(Event::NoteOff {
                    voice_id,
                    channel,
                    key: note,
                })
            }
            NoteEvent::Choke {
                voice_id,
                channel,
                key,
                ..
            } => {
                let (voice_id, channel, note) = legacy_note(voice_id, channel, key);
                Some(Event::Choke {
                    voice_id,
                    channel,
                    key: note,
                })
            }
            NoteEvent::PolyTuning {
                voice_id,
                channel,
                key,
                tuning,
                ..
            } if tuning.is_finite() => {
                let (voice_id, channel, note) = legacy_note(voice_id, channel, key);
                Some(Event::PerNoteTuning {
                    voice_id,
                    channel,
                    key: note,
                    semitones: tuning,
                })
            }
            NoteEvent::MidiPitchBend { channel, value, .. } if value.is_finite() => {
                Some(Event::PitchBend {
                    channel,
                    normalized: (2.0 * (value - 0.5)).clamp(-1.0, 1.0),
                })
            }
            NoteEvent::MidiCC {
                channel, cc, value, ..
            } if value.is_finite() => match cc {
                DEV_VIEW_CC if self.dev_cc => {
                    self.telemetry
                        .request_view((value.clamp(0.0, 1.0) * 127.0).round() as u8);
                    None
                }
                DEV_DISCLOSURE_CC if self.dev_cc => {
                    self.telemetry.request_disclosure(value >= 0.5);
                    None
                }
                DEV_BROWSER_CC if self.dev_cc => {
                    self.telemetry.request_browser(value >= 0.5);
                    None
                }
                DEV_THEME_CC if self.dev_cc => {
                    self.telemetry
                        .request_theme((value.clamp(0.0, 1.0) * 127.0).round() as u8);
                    None
                }
                control_change::MODULATION_MSB => Some(Event::ModWheel {
                    channel,
                    value: value.clamp(0.0, 1.0),
                }),
                control_change::ALL_SOUND_OFF => Some(Event::AllSoundOff),
                control_change::ALL_NOTES_OFF => Some(Event::AllNotesOff),
                _ => None,
            },
            // **New MIDI path.** Channel pressure is retained per channel whether or not a note
            // sounds, so a note started while a key is already leaned on inherits it. It starts at
            // zero depth on every target, so a fresh instance is still the copy.
            NoteEvent::MidiChannelPressure {
                channel, pressure, ..
            } if pressure.is_finite() => Some(Event::ChannelPressure {
                channel,
                value: pressure.clamp(0.0, 1.0),
            }),
            _ => None,
        };
        if let Some(event) = translated {
            if self.pending_events.len() == EVENT_CHUNK_CAPACITY {
                self.apply_pending_events();
            }
            self.pending_events.push(event);
        }
    }

    fn apply_pending_events(&mut self) {
        if self.pending_events.is_empty() {
            return;
        }
        let control = self.params.event_control_params();
        self.voice.handle_events(control, &self.pending_events);
        self.pending_events.clear();
    }

    fn status(&self) -> ProcessStatus {
        match self.voice.activity(self.params.event_control_params()) {
            Activity::Live => ProcessStatus::KeepAlive,
            Activity::Tailing => ProcessStatus::Tail(self.tail_upper_bound_samples()),
            Activity::Inert => ProcessStatus::Normal,
        }
    }

    /// Conservative from any current envelope level: the DSP's exponential release reaches its
    /// exact -100 dB snap in at most 2.5 configured release times. Three times plus filter margin
    /// deliberately over-reports rather than cutting an audible tail.
    fn tail_upper_bound_samples(&self) -> u32 {
        let release = self
            .params
            .filter_env_release
            .value()
            .max(self.params.amp_env_release.value())
            .clamp(
                mxm_mono_pr1_dsp::envelope::MIN_TIME_S,
                mxm_mono_pr1_dsp::envelope::MAX_TIME_S,
            );
        let samples =
            ((3.0 * f64::from(release) + 0.1) * f64::from(self.sample_rate.max(1.0))).ceil() as u64;
        samples.clamp(1, u64::from(u32::MAX)) as u32
    }
}

impl Plugin for MxmMonoPr1 {
    const NAME: &'static str = NAME;
    const VENDOR: &'static str = "mxm";
    const URL: &'static str = "https://mxm.dk";
    const EMAIL: &'static str = "plugins@mxm.dk";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Stereo, then mono. **No input of any kind**: the External audio and Gate / clock auxiliary
    /// inputs were removed (the owner, 2026-09-26), noise is the mixer's third source and the LFO
    /// core is Repeat's only clock.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type Editor = editor::MxmMonoPr1Editor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        editor::create(self.params.clone(), self.telemetry.clone())
    }

    fn activate(
        &mut self,
        _layout: &AudioIOLayout,
        config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        // A new activation starts with no tempo and nothing resolved: the first callback reports
        // the tempo, so neither the audio nor an editor frame before it shows the last session's
        // divisions (`plans/plan-tempo-sync-controls.md`).
        self.telemetry.tempo.publish(None);
        self.synced_lfo_hz = None;
        self.sample_rate = if config.sample_rate.is_finite() {
            config.sample_rate.max(1.0)
        } else {
            48_000.0
        };
        self.voice = Voice::new();
        self.voice.set_sample_rate(self.sample_rate);
        self.voice.reset();
        self.pending_events.clear();
        debug_assert!(self.pending_events.capacity() >= EVENT_CHUNK_CAPACITY);
        self.telemetry.publish_sample_rate(self.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.voice.reset();
        self.pending_events.clear();
        self.telemetry.publish_activity(Activity::Inert, false);
    }

    /// **A project saved before the tempo syncs** restores each Off rather than keeping this
    /// instance's, and a loaded preset's baseline gains it, so the preset stays clean
    /// (`mxm_preset::add_switches_off`).
    fn filter_state(state: &mut PluginState) {
        mxm_preset::add_switches_off(state, crate::preset::TEMPO_SYNC_IDS);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let samples = buffer.samples();
        let mut next_event = context.next_event();
        let mut start = 0usize;

        // The LFO's tempo sync, once per callback (`plans/plan-tempo-sync-controls.md`), and the
        // tempo in force for the editor's reading.
        let tempo = context.transport().tempo;
        self.synced_lfo_hz = self.params.synced_lfo_rate(tempo);
        self.telemetry.tempo.publish(tempo);

        // **Topology, once per callback.** Which routes are live changes only on a parameter event,
        // and `plan-modulation-routing.md` decision 1.9 puts a route's arrival at the processing
        // interval rather than at an exact sample — *moving a modulator can probably never be
        // sample accurate; the modulation itself is what must be*. So it is resolved here rather
        // than inside the event-split loop, where it would be redone for every sub-block to no
        // audible end. Doing it at all is what stops the first block after any allocation
        // rendering unrouted.
        //
        // `topology_from` also snaps every **newly present** route's smoother to its stored value.
        // An absent route's smoother is never advanced, so resuming it would ramp the route in from
        // wherever the last live sample left it, over a span set by how long it was absent.
        self.routing = self.params.routes.topology_from(&self.routing);
        self.voice.set_topology(&self.routing);
        let routed = self.routing.present.iter().flatten().any(|&p| p);

        while start < samples {
            let mut end = (start + MAX_BLOCK_SIZE).min(samples);
            loop {
                match next_event {
                    Some(event) if event.timing() as usize <= start => {
                        self.queue_host_event(event);
                        next_event = context.next_event();
                    }
                    Some(event) if (event.timing() as usize) < end => {
                        end = event.timing() as usize;
                        break;
                    }
                    _ => break,
                }
            }
            self.apply_pending_events();

            let output = buffer.as_slice();
            let mut peak = 0.0f32;
            let mut last: Option<(Frame, mxm_mono_pr1_dsp::voice::Params)> = None;

            for index in start..end {
                let params = self.voice_params();
                // The amounts for this sample, into the topology the interval already resolved.
                // **In place and only where a route is live**: an absent pair's smoother is never
                // advanced, so it costs nothing, and its stored depth is left exactly where the
                // player put it so re-adding the source restores it.
                if routed {
                    self.params.routes.advance_into(self.voice.routing_mut());
                }
                let frame = self.voice.process_sample(params, &[]);
                peak = peak.max(frame.output.abs());
                for channel in output.iter_mut() {
                    channel[index] = frame.output;
                }
                last = Some((frame, params));
            }
            self.telemetry.publish_peak(peak);
            if let Some((frame, params)) = last {
                self.telemetry
                    .publish_frame(frame, params, self.sample_rate);
            }
            start = end;
        }

        // nice-plug permits events at the callback's end offset, including offset zero in an empty
        // callback. Apply those without advancing the voice; pending trigger/cut edges are consumed
        // by the next actual sample.
        while let Some(event) = next_event {
            self.queue_host_event(event);
            next_event = context.next_event();
        }
        self.apply_pending_events();
        self.telemetry.publish_activity(
            self.voice.activity(self.params.event_control_params()),
            self.voice.panic_latched(),
        );
        self.status()
    }
}

impl ClapPlugin for MxmMonoPr1 {
    const CLAP_ID: &'static str = CLAP_ID;
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "A two-oscillator monophonic synthesizer with layered waveforms and free modulation routing",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

nice_export_clap!(MxmMonoPr1);

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::context::process::SendEventError;
    use nice_plug::params::InternalParamMut;
    use std::collections::{HashSet, VecDeque};

    struct TestContext {
        events: VecDeque<NoteEvent<()>>,
        transport: Transport,
    }

    impl TestContext {
        fn new(events: impl IntoIterator<Item = NoteEvent<()>>) -> Self {
            Self {
                events: events.into_iter().collect(),
                transport: Transport::new(48_000.0),
            }
        }
    }

    impl ProcessContext<MxmMonoPr1> for TestContext {
        // A test double has no host to ask for a restart (nice-plug 0.4).
        fn request_restart(&self) {}
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute_background(&self, _task: ()) {}
        fn execute_gui(&self, _task: ()) {}
        fn transport(&self) -> &Transport {
            &self.transport
        }
        fn next_event(&mut self) -> Option<NoteEvent<()>> {
            self.events.pop_front()
        }
        fn try_send_event(
            &mut self,
            _event: NoteEvent<()>,
        ) -> Result<(), (NoteEvent<()>, SendEventError)> {
            Ok(())
        }
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    fn process_for_test(
        plugin: &mut MxmMonoPr1,
        frames: usize,
        events: impl IntoIterator<Item = NoteEvent<()>>,
    ) -> (Vec<f32>, ProcessStatus) {
        let mut samples = vec![0.0; frames];
        let mut buffer = Buffer::default();
        unsafe {
            buffer.set_slices(frames, |channels| {
                channels.clear();
                channels.push(samples.as_mut_slice());
            });
        }
        let mut inputs = [];
        let mut outputs = [];
        let mut aux = AuxiliaryBuffers {
            inputs: &mut inputs,
            outputs: &mut outputs,
        };
        let mut context = TestContext::new(events);
        let status = plugin.process(&mut buffer, &mut aux, &mut context);
        drop(buffer);
        (samples, status)
    }

    fn activate_smoothers(plugin: &MxmMonoPr1) {
        for (_, pointer, _) in plugin.params.param_map() {
            unsafe { pointer._internal_update_smoother(48_000.0, true) };
        }
    }

    fn begin_smoother_ramps(plugin: &MxmMonoPr1) {
        for (_, pointer, _) in plugin.params.param_map() {
            unsafe { pointer._internal_update_smoother(48_000.0, false) };
        }
    }

    fn note_on(channel: u8, note: u8) -> NoteEvent<()> {
        NoteEvent::NoteOn {
            timing: 0,
            voice_id: VoiceID::Wildcard,
            channel: Channel::Number(channel),
            key: Key::Number(note),
            velocity: 0.8,
        }
    }

    #[test]
    fn identity_and_bundle_agree() {
        assert_eq!(CLAP_ID, format!("dk.mxm.{NAME}"));
        mxm_plugin_test::bundle::is_named(env!("CARGO_MANIFEST_DIR"), env!("CARGO_PKG_NAME"), NAME);
    }

    /// **Stereo, then mono, and no input of any kind** — the External audio and Gate / clock
    /// auxiliary inputs were removed (the owner, 2026-09-26).
    #[test]
    fn the_layouts_are_stereo_then_mono_with_no_inputs() {
        assert_eq!(MxmMonoPr1::AUDIO_IO_LAYOUTS.len(), 2);
        for (index, layout) in MxmMonoPr1::AUDIO_IO_LAYOUTS.iter().enumerate() {
            assert!(layout.main_input_channels.is_none(), "layout {index}");
            assert!(layout.aux_input_ports.is_empty(), "layout {index}");
            assert_eq!(
                layout.main_output_channels.unwrap().get(),
                if index == 0 { 2 } else { 1 }
            );
        }
    }

    #[test]
    fn parameter_ids_are_unique_and_the_surface_is_complete() {
        let params = MxmMonoPr1Params::default();
        let ids: Vec<_> = params
            .param_map()
            .into_iter()
            .map(|entry| entry.0)
            .collect();
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
        // 40 voice controls and **278 routing parameters**: nine targets by fourteen sources plus
        // the summing module's thirteen — the wheel has no additive pair on the bus it multiplies —
        // a presence and an amount each. The ninth is the standard Amplitude (2026-09-26). Fifteen
        // ids retired in all.
        assert_eq!(ids.len(), 318);
        assert_eq!(
            ids.iter().filter(|id| id.starts_with("mod_")).count(),
            ((mxm_mono_pr1_dsp::routing::TARGETS - 1) * mxm_mono_pr1_dsp::routing::SOURCES
                + mxm_mono_pr1_dsp::routing::SOURCES
                - 1)
                * 2
        );
    }

    #[test]
    fn init_obeys_amount_configuration_and_two_oscillator_rules() {
        let params = MxmMonoPr1Params::default();
        for (name, value) in [
            ("glide", params.glide_time.value()),
            ("oscillator B level", params.osc_b_level.value()),
            ("noise", params.noise_external_level.value()),
            ("resonance", params.resonance.value()),
        ] {
            assert_eq!(value, 0.0, "{name} is an amount");
        }
        // **Every modulation depth is a route amount, and every one starts at zero except the
        // multiplier's own two factors and the wheel's vibrato.** `product`'s neutral is one, so a
        // factor at zero makes the module publish a constant whatever its sources do; the wheel
        // brings in vibrato from a fresh instance. `init_amount` states them and the plugin's
        // AGENTS.md records them. A signed amount stores zero as a normalised 0.5.
        let at_rest = |target: usize, routes: &[mxm_modulation_params::Route<'_>]| {
            for route in routes {
                // By the source's own index: the Mod bus's rows skip the retired Wheel slot, so a
                // row's position is not its source.
                let slot = mxm_mono_pr1_dsp::routing::SOURCE_NAMES
                    .iter()
                    .position(|name| *name == route.source)
                    .expect("a row names one of the sources");
                // Every depth starts where Init puts it: `init_amount`, the one statement.
                assert_eq!(
                    route.amount.normalised(),
                    route.amount.default_normalised(),
                    "{} started away from its default",
                    route.amount_id
                );
                let expected = mxm_mono_pr1_dsp::routing::init_amount(target, slot);
                if expected == 0.0 || expected == 1.0 {
                    assert_eq!(
                        route.amount.normalised(),
                        if expected == 1.0 { 1.0 } else { 0.5 },
                        "{} started at the wrong depth",
                        route.amount_id
                    );
                }
            }
        };
        for (target, group) in params.routes.ordinary() {
            at_rest(target, &group.routes(target));
        }
        at_rest(
            mxm_mono_pr1_dsp::routing::target::MOD_BUS,
            &params.routes.mod_bus.routes(),
        );
        assert!(params.osc_a_saw.value());
        assert!(!params.osc_a_pulse.value());
        assert!(params.osc_a_level.value() > 0.0);
        assert_ne!(
            params.osc_a_frequency.value(),
            params.osc_b_frequency.value()
        );
        assert!(params.cutoff.value() >= 10_000.0 && params.cutoff.value() < params::CUTOFF_MAX_HZ);
        assert!(!params.repeat_external.value() && !params.drone.value());
    }

    #[test]
    fn initialized_parameter_smoothers_reproduce_the_dsp_init_patch() {
        let plugin = MxmMonoPr1::default();
        activate_smoothers(&plugin);
        assert_eq!(
            plugin.params.next_voice_params(),
            mxm_mono_pr1_dsp::voice::Params::init()
        );
    }

    #[test]
    fn translated_events_preserve_order_ownership_and_panic_recovery() {
        let mut plugin = MxmMonoPr1::default();
        activate_smoothers(&plugin);
        plugin.queue_host_event(NoteEvent::MidiPitchBend {
            timing: 0,
            channel: 3,
            value: 1.0,
        });
        plugin.queue_host_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 3,
            cc: control_change::MODULATION_MSB,
            value: 0.75,
        });
        plugin.queue_host_event(note_on(3, 64));
        plugin.apply_pending_events();
        let owner = plugin.voice.owner();
        assert_eq!(
            (owner.channel, owner.key, owner.bend_normalized, owner.wheel),
            (3, 64, 1.0, 0.75)
        );
        assert_eq!(owner.pitch_semitones(7.0), 71.0);
        // SAFETY: this is the same write the wrapper performs for a host parameter event; this
        // unit test has no host-side ParamSetter.
        unsafe { plugin.params.bend_range._internal_set_plain_value(12.0) };
        assert_eq!(
            plugin
                .voice
                .owner()
                .pitch_semitones(plugin.params.event_control_params().bend_range_semitones),
            76.0,
            "the non-advancing target immediately rescales retained bend bookkeeping"
        );
        plugin.queue_host_event(NoteEvent::MidiPitchBend {
            timing: 0,
            channel: 2,
            value: 0.25,
        });
        plugin.queue_host_event(note_on(2, 60));
        plugin.apply_pending_events();
        assert_eq!(plugin.voice.owner().pitch_semitones(12.0), 54.0);
        plugin.queue_host_event(NoteEvent::NoteOff {
            timing: 0,
            voice_id: VoiceID::Wildcard,
            channel: Channel::Number(2),
            key: Key::Number(60),
            velocity: 0.0,
        });
        plugin.apply_pending_events();
        assert_eq!(
            plugin.voice.owner().pitch_semitones(12.0),
            76.0,
            "ownership fallback restores the prior channel's retained bend position"
        );

        plugin.queue_host_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: control_change::ALL_SOUND_OFF,
            value: 0.0,
        });
        plugin.queue_host_event(note_on(0, 60));
        plugin.apply_pending_events();
        assert!(
            !plugin.voice.panic_latched(),
            "later same-sample note-on re-arms"
        );

        plugin.queue_host_event(note_on(0, 67));
        plugin.queue_host_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: control_change::ALL_SOUND_OFF,
            value: 0.0,
        });
        plugin.apply_pending_events();
        assert!(plugin.voice.panic_latched(), "later same-sample panic wins");
        assert_eq!(plugin.status(), ProcessStatus::Normal);
    }

    #[test]
    fn developer_channel_is_off_unless_requested_by_the_environment() {
        let mut plugin = MxmMonoPr1 {
            dev_cc: false,
            ..Default::default()
        };
        for (cc, value) in [
            (DEV_VIEW_CC, 1.0),
            (DEV_DISCLOSURE_CC, 1.0),
            (DEV_BROWSER_CC, 1.0),
            (DEV_THEME_CC, 1.0),
        ] {
            plugin.queue_host_event(NoteEvent::MidiCC {
                timing: 0,
                channel: 0,
                cc,
                value,
            });
        }
        assert_eq!(plugin.telemetry.take_view_request(), None);
        assert_eq!(plugin.telemetry.take_disclosure_request(), None);
        assert_eq!(plugin.telemetry.take_browser_request(), None);
        assert_eq!(plugin.telemetry.take_theme_request(), None);

        plugin.dev_cc = true;
        plugin.queue_host_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: DEV_VIEW_CC,
            value: 1.0 / 127.0,
        });
        plugin.queue_host_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: DEV_DISCLOSURE_CC,
            value: 1.0,
        });
        plugin.queue_host_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: DEV_BROWSER_CC,
            value: 0.0,
        });
        plugin.queue_host_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: DEV_THEME_CC,
            value: 1.0 / 127.0,
        });
        assert_eq!(plugin.telemetry.take_view_request(), Some(1));
        assert_eq!(plugin.telemetry.take_disclosure_request(), Some(true));
        assert_eq!(plugin.telemetry.take_browser_request(), Some(false));
        assert_eq!(plugin.telemetry.take_theme_request(), Some(1));
    }

    #[test]
    fn process_splits_at_events_and_consumes_end_events_in_empty_callbacks() {
        let mut plugin = MxmMonoPr1::default();
        activate_smoothers(&plugin);
        let (samples, status) = process_for_test(
            &mut plugin,
            128,
            [
                NoteEvent::NoteOn {
                    timing: 17,
                    voice_id: VoiceID::Wildcard,
                    channel: Channel::Number(0),
                    key: Key::Number(60),
                    velocity: 0.8,
                },
                NoteEvent::NoteOff {
                    timing: 96,
                    voice_id: VoiceID::Wildcard,
                    channel: Channel::Number(0),
                    key: Key::Number(60),
                    velocity: 0.0,
                },
            ],
        );
        assert!(samples[..17].iter().all(|sample| *sample == 0.0));
        assert!(samples[32..96].iter().any(|sample| sample.abs() > 1e-5));
        assert_eq!(
            status,
            ProcessStatus::Tail(plugin.tail_upper_bound_samples())
        );

        let (_, status) = process_for_test(
            &mut plugin,
            0,
            [NoteEvent::MidiCC {
                timing: 0,
                channel: 0,
                cc: control_change::ALL_SOUND_OFF,
                value: 0.0,
            }],
        );
        assert_eq!(status, ProcessStatus::Normal);
        assert!(plugin.voice.panic_latched());
    }

    #[test]
    fn tail_bound_covers_the_dsp_release_snap() {
        let plugin = MxmMonoPr1::default();
        let configured = plugin.params.amp_env_release.value();
        assert!(
            u64::from(plugin.tail_upper_bound_samples())
                >= (2.5 * configured * plugin.sample_rate) as u64
        );
    }

    #[test]
    fn activity_moves_from_live_through_tail_to_exact_inert_silence() {
        let mut plugin = MxmMonoPr1::default();
        activate_smoothers(&plugin);
        let (held, held_status) = process_for_test(&mut plugin, 128, [note_on(0, 60)]);
        assert!(held.iter().any(|sample| sample.abs() > 1e-5));
        assert_eq!(held_status, ProcessStatus::KeepAlive);

        let (_, released_status) = process_for_test(
            &mut plugin,
            64,
            [NoteEvent::NoteOff {
                timing: 0,
                voice_id: VoiceID::Wildcard,
                channel: Channel::Number(0),
                key: Key::Number(60),
                velocity: 0.0,
            }],
        );
        assert!(matches!(released_status, ProcessStatus::Tail(_)));

        let mut final_status = released_status;
        for _ in 0..2_000 {
            let (_, status) = process_for_test(&mut plugin, 64, []);
            final_status = status;
            if status == ProcessStatus::Normal {
                break;
            }
        }
        assert_eq!(final_status, ProcessStatus::Normal);
        // The callback that crosses the settlement point can contain its last tail samples before
        // reporting Normal. The following callback is the exact-inert contract.
        let (inert, inert_status) = process_for_test(&mut plugin, 64, []);
        assert_eq!(inert_status, ProcessStatus::Normal);
        assert!(inert.iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn process_status_follows_the_final_vca_for_both_filter_route_states() {
        fn configured(filter_amount: f32) -> MxmMonoPr1 {
            let plugin = MxmMonoPr1::default();
            // SAFETY: these are the same writes the wrapper performs for host parameter events;
            // the unit test has no host-side ParamSetter.
            unsafe {
                plugin
                    .params
                    .filter_env_release
                    ._internal_set_plain_value(2.0);
                plugin
                    .params
                    .amp_env_release
                    ._internal_set_plain_value(0.002);
                plugin
                    .params
                    .routes
                    .cutoff
                    .fenv
                    ._internal_set_plain_value(filter_amount);
            }
            activate_smoothers(&plugin);
            plugin
        }

        let (mut disconnected, mut routed) = (configured(0.0), configured(1.0));
        for plugin in [&mut disconnected, &mut routed] {
            process_for_test(plugin, 128, [note_on(0, 60)]);
            process_for_test(
                plugin,
                64,
                [NoteEvent::NoteOff {
                    timing: 0,
                    voice_id: VoiceID::Wildcard,
                    channel: Channel::Number(0),
                    key: Key::Number(60),
                    velocity: 0.0,
                }],
            );
        }
        let mut disconnected_status = ProcessStatus::KeepAlive;
        let mut routed_status = ProcessStatus::KeepAlive;
        for _ in 0..256 {
            disconnected_status = process_for_test(&mut disconnected, 64, []).1;
            routed_status = process_for_test(&mut routed, 64, []).1;
            if disconnected_status == ProcessStatus::Normal
                && routed_status == ProcessStatus::Normal
            {
                break;
            }
        }
        assert_eq!(disconnected_status, ProcessStatus::Normal);
        assert_eq!(routed_status, ProcessStatus::Normal);
        let (routed_silence, routed_idle) = process_for_test(&mut routed, 64, []);
        assert_eq!(routed_idle, ProcessStatus::Normal);
        assert!(routed_silence.iter().all(|sample| *sample == 0.0));
        // Reconnecting after sleep must not revive the discarded hidden envelope.
        unsafe {
            disconnected
                .params
                .routes
                .cutoff
                .fenv
                ._internal_set_plain_value(1.0);
        }
        let (silent, status) = process_for_test(&mut disconnected, 64, []);
        assert_eq!(status, ProcessStatus::Normal);
        assert!(silent.iter().all(|sample| *sample == 0.0));
    }

    /// Sets one parameter by its permanent id and snaps its smoother, the way a preset load does.
    fn set_by_id(plugin: &MxmMonoPr1, wanted: &str, normalised: f32) {
        let mut found = false;
        for (id, pointer, _) in plugin.params.param_map() {
            if id == wanted {
                unsafe {
                    pointer._internal_set_normalized_value(normalised);
                    pointer._internal_update_smoother(48_000.0, true);
                }
                found = true;
            }
        }
        assert!(found, "no parameter named {wanted}");
    }

    /// The spread of the rendered period as a fraction of its shortest: 0 is a steady pitch.
    fn period_spread(samples: &[f32]) -> f32 {
        let (mut lo, mut hi) = (usize::MAX, 0usize);
        let mut previous = 0.0f32;
        let mut last = 0usize;
        for (index, &sample) in samples.iter().enumerate() {
            if previous < 0.0 && sample >= 0.0 {
                if last > 0 {
                    lo = lo.min(index - last);
                    hi = hi.max(index - last);
                }
                last = index;
            }
            previous = sample;
        }
        if lo == usize::MAX {
            return 0.0;
        }
        (hi - lo) as f32 / lo as f32
    }

    /// **The instrument's headline gesture, end to end through the real callback:** the LFO into
    /// `Mod bus`, the multiplier at Oscillator A's frequency, and the wheel fading the vibrato in.
    ///
    /// Every other routing test drives the DSP directly. This one goes through `process`, so it
    /// covers the whole chain a player actually moves — parameter smoothers, `topology_from`,
    /// `advance_into`, the ⅛ frame, the product law and the oscillator's own scale — and it measures
    /// the **rendered audio's period**, not an intermediate value, because a player's complaint is
    /// always about what they can hear.
    ///
    /// It exists because the gesture shipped with no test that played it: the DSP proof held its
    /// `Routing` by hand, and the preset render left every route amount at zero, so nothing here
    /// would have gone red if the parameter side had been wired to the wrong target.
    #[test]
    fn the_wheel_fades_the_modules_vibrato_into_rendered_audio() {
        let spread_at = |wheel: f32| {
            let mut plugin = MxmMonoPr1::default();
            activate_smoothers(&plugin);
            // One unfiltered saw.
            set_by_id(&plugin, "osc_a_saw", 1.0);
            set_by_id(&plugin, "osc_a_pulse", 0.0);
            set_by_id(&plugin, "cutoff", 1.0);
            set_by_id(&plugin, "resonance", 0.0);
            // A vibrato rate and the shape a vibrato wants.
            set_by_id(&plugin, "lfo_rate", 0.65);
            set_by_id(&plugin, "lfo_triangle", 1.0);
            // The gesture: the LFO into the summing module, the multiplier at the pitch.
            set_by_id(&plugin, "mod_modbus_lfoon", 1.0);
            set_by_id(&plugin, "mod_modbus_lfo", 1.0);
            set_by_id(&plugin, "mod_afreq_wbuson", 1.0);
            set_by_id(&plugin, "mod_afreq_wbus", 0.51);

            plugin.queue_host_event(NoteEvent::MidiCC {
                timing: 0,
                channel: 0,
                cc: control_change::MODULATION_MSB,
                value: wheel,
            });
            plugin.apply_pending_events();
            let mut rendered = Vec::new();
            let mut events = vec![note_on(0, 48)];
            // A second of audio, in the host block size the rest of these tests use.
            for _ in 0..(48_000 / 64) {
                let (block, _) = process_for_test(&mut plugin, 64, events.drain(..));
                rendered.extend_from_slice(&block);
            }
            assert!(
                rendered.iter().any(|sample| sample.abs() > 0.05),
                "the saw must sound at wheel {wheel}"
            );
            period_spread(&rendered[24_000..])
        };

        let (down, half, up) = (spread_at(0.0), spread_at(0.5), spread_at(1.0));
        // A steady saw still varies by one sample of crossing quantisation; vibrato is far wider.
        assert!(down < 0.01, "the wheel down must be a steady pitch: {down}");
        assert!(
            half > down * 4.0,
            "half a wheel must be part of the way in: {half} against {down}"
        );
        assert!(up > half, "a full wheel must be the most vibrato: {up}");
    }

    #[test]
    fn held_bend_range_edit_is_pitch_smoothed_in_rendered_audio() {
        fn prepared() -> MxmMonoPr1 {
            let mut plugin = MxmMonoPr1::default();
            activate_smoothers(&plugin);
            process_for_test(
                &mut plugin,
                2_048,
                [
                    NoteEvent::MidiPitchBend {
                        timing: 0,
                        channel: 3,
                        value: 1.0,
                    },
                    note_on(3, 64),
                ],
            );
            plugin
        }

        let mut unchanged = prepared();
        let mut edited = prepared();
        // SAFETY: this is the same write the wrapper performs for host automation; there is no
        // host-side ParamSetter in this unit test.
        unsafe { edited.params.bend_range._internal_set_plain_value(12.0) };
        begin_smoother_ramps(&edited);

        let (reference_early, _) = process_for_test(&mut unchanged, 64, []);
        let (edited_early, _) = process_for_test(&mut edited, 64, []);
        let early_error = reference_early
            .iter()
            .zip(&edited_early)
            .map(|(&a, &b)| (a - b).powi(2))
            .sum::<f32>()
            / reference_early.len() as f32;
        assert!(
            early_error.sqrt() < 0.03,
            "a signal-rate Bend range edit jumped immediately: RMS {early_error}"
        );

        let first_smoothed = edited
            .params
            .next_voice_params()
            .control
            .bend_range_semitones;
        assert!(
            (7.0..7.5).contains(&first_smoothed),
            "the sounding range must start inside the ramp, got {first_smoothed}"
        );
        let first_frame = edited
            .voice
            .process_sample(edited.params.next_voice_params(), &[]);
        assert!(
            first_frame.control.pitch_semitones > 71.0
                && first_frame.control.pitch_semitones < 71.5,
            "the first rendered pitch must advance inside the ramp: {}",
            first_frame.control.pitch_semitones
        );
        let mut previous_pitch = first_frame.control.pitch_semitones;
        let mut rendered = vec![first_frame.output];
        for _ in 0..960 {
            let frame = edited
                .voice
                .process_sample(edited.params.next_voice_params(), &[]);
            assert!(
                frame.control.pitch_semitones >= previous_pitch
                    && frame.control.pitch_semitones - previous_pitch < 0.01,
                "non-monotonic or stepped sounding pitch: {previous_pitch} -> {}",
                frame.control.pitch_semitones
            );
            previous_pitch = frame.control.pitch_semitones;
            rendered.push(frame.output);
        }
        assert!(
            (previous_pitch - 76.0).abs() < 1e-3,
            "ramp ended at {previous_pitch}"
        );
        assert!(rendered.iter().all(|sample| sample.is_finite()));
        assert!(rendered.iter().any(|sample| sample.abs() > 1e-4));

        let (reference_late, _) = process_for_test(&mut unchanged, 1_024, []);
        let (edited_late, _) = process_for_test(&mut edited, 1_024, []);
        let late_error = reference_late
            .iter()
            .zip(&edited_late)
            .map(|(&a, &b)| (a - b).powi(2))
            .sum::<f32>()
            / reference_late.len() as f32;
        assert!(
            late_error.sqrt() > early_error.sqrt() * 2.0,
            "the smoothed edit never reached an audibly different pitch"
        );
    }

    #[test]
    fn dense_same_sample_events_never_grow_the_realtime_chunk() {
        let mut plugin = MxmMonoPr1::default();
        activate_smoothers(&plugin);
        let capacity = plugin.pending_events.capacity();
        assert_eq!(capacity, EVENT_CHUNK_CAPACITY);
        for index in 0..200 {
            let key = 36 + (index % 48) as u8;
            plugin.queue_host_event(note_on(0, key));
            plugin.queue_host_event(NoteEvent::NoteOff {
                timing: 0,
                voice_id: VoiceID::Wildcard,
                channel: Channel::Number(0),
                key: Key::Number(key),
                velocity: 0.0,
            });
            assert_eq!(plugin.pending_events.capacity(), capacity);
            assert!(plugin.pending_events.len() <= EVENT_CHUNK_CAPACITY);
        }
        plugin.queue_host_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: control_change::ALL_SOUND_OFF,
            value: 0.0,
        });
        plugin.apply_pending_events();
        assert!(
            plugin.voice.panic_latched(),
            "the final event must still win"
        );
        assert_eq!(plugin.pending_events.capacity(), capacity);
    }

    #[test]
    fn clap_state_has_one_persisted_preset_identity_beside_the_parameters() {
        let plugin = MxmMonoPr1::default();
        let fields = plugin.params.serialize_fields();
        assert_eq!(fields.len(), 1);
        assert!(fields.contains_key("preset"));
        assert_eq!(plugin.params.all_parameters().len(), 318);
    }
}

/// **A synced value reaches the patch** (`plans/plan-tempo-sync-controls.md`): what a sync resolved
/// for this callback is what the DSP is given, and with none the free value is.
#[cfg(test)]
mod tempo_sync_path {
    use super::*;

    #[test]
    fn the_synced_lfo_rate_is_the_patchs() {
        let mut plugin = MxmMonoPr1::default();
        let free = plugin.voice_params().control.lfo_rate_hz;
        plugin.synced_lfo_hz = Some(free + 1.0);
        assert_eq!(plugin.voice_params().control.lfo_rate_hz, free + 1.0);
        plugin.synced_lfo_hz = None;
        assert_eq!(plugin.voice_params().control.lfo_rate_hz, free);
    }
}

/// **Activation forgets the last session's tempo and resolved syncs**: the first callback reports the
/// tempo, so nothing — the audio, or an editor frame before it — starts from the previous session's
/// divisions.
#[cfg(test)]
mod activation_forgets_the_tempo {
    use super::*;

    #[test]
    fn activation_forgets_the_last_tempo_and_resolved_syncs() {
        use nice_plug::prelude::Plugin as _;
        let mut plugin = MxmMonoPr1::default();
        plugin.telemetry.tempo.publish(Some(120.0));
        plugin.synced_lfo_hz = Some(1.0);
        let layout = MxmMonoPr1::AUDIO_IO_LAYOUTS[0];
        let config = BufferConfig {
            sample_rate: 48_000.0,
            min_buffer_size: None,
            max_buffer_size: 512,
            process_mode: ProcessMode::Realtime,
        };
        let _ = plugin.activate(&layout, &config, &mut NoInit);
        assert_eq!(plugin.telemetry.tempo.get(), None);
        assert_eq!(plugin.synced_lfo_hz, None);
    }

    /// An activation context that asks nothing of a host.
    struct NoInit;

    impl ActivateContext<MxmMonoPr1> for NoInit {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute(&self, _task: <MxmMonoPr1 as Plugin>::BackgroundTask) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }
}

/// What a player reads — on hover in the editor, and in a host's plugin browser — speaks to the
/// player about the sound, never about the machine or the code (`mxm_plugin_test::hover_text`).
#[cfg(test)]
mod speaks_to_the_player {
    #[test]
    fn hover_text() {
        mxm_plugin_test::hover_text::speaks_to_the_player(env!("CARGO_MANIFEST_DIR"));
    }

    #[test]
    fn host_description() {
        mxm_plugin_test::hover_text::host_description_speaks_to_the_player(env!(
            "CARGO_MANIFEST_DIR"
        ));
    }
}
