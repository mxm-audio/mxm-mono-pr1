//! Permanent host parameters for the complete voice, plus its modulation routing in
//! [`crate::routes`].
//!
//! Every `#[id]` is public compatibility state: presets, automation and saved projects retain it.
//! Continuous values that directly scale or offset audio/control signals are smoothed. Envelope and
//! glide times, rates and switches configure state machines and remain unsmoothed.
//!
//! **Thirteen ids retired when the Direct/Wheel buses did** — the eleven the buses named, plus
//! `filter_env_amount` and `filter_keyboard_amount`. `crate::routes` records why none could be
//! re-used and where the reachability argument lives.

use mxm_mono_pr1_dsp::{
    control::{GlideMode, Params as ControlParams},
    envelope::{AdsrParams, MAX_TIME_S, MIN_TIME_S},
    keyboard::KeyMode,
    lfo::{RATE_MAX_HZ, RATE_MIN_HZ, WaveEnables},
    oscillator::{AWaves, BWaves},
    voice::{
        FilterParams, MixerParams, OscillatorAParams, OscillatorBParams, Params as VoiceParams,
    },
};
use nice_plug::prelude::*;
use std::sync::{Arc, RwLock};

const SIGNAL_SMOOTHING_MS: f32 = 10.0;
const PITCH_SMOOTHING_MS: f32 = 20.0;
pub const GLIDE_MAX_S: f32 = mxm_mono_pr1_dsp::control::GLIDE_MAX_S;
pub const OSCILLATOR_FREQUENCY_REACH_ST: f32 = 6.0;
pub const MASTER_TUNE_REACH_ST: f32 = 2.0;
pub const BEND_RANGE_MAX_ST: f32 = 24.0;
pub const CUTOFF_MIN_HZ: f32 = 10.0;
pub const CUTOFF_MAX_HZ: f32 = 20_000.0;

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyModeParam {
    #[id = "normal"]
    #[name = "Normal"]
    Normal,
    #[id = "retrigger"]
    #[name = "Retrigger"]
    Retrigger,
}
impl From<KeyModeParam> for KeyMode {
    fn from(value: KeyModeParam) -> Self {
        match value {
            KeyModeParam::Normal => Self::Normal,
            KeyModeParam::Retrigger => Self::Retrig,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlideModeParam {
    #[id = "normal"]
    #[name = "Normal"]
    Normal,
    #[id = "auto"]
    #[name = "Auto"]
    Auto,
}
impl From<GlideModeParam> for GlideMode {
    fn from(value: GlideModeParam) -> Self {
        match value {
            GlideModeParam::Normal => Self::Normal,
            GlideModeParam::Auto => Self::Auto,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OctaveParam {
    #[id = "0"]
    #[name = "0"]
    Zero,
    #[id = "1"]
    #[name = "+1"]
    One,
    #[id = "2"]
    #[name = "+2"]
    Two,
    #[id = "3"]
    #[name = "+3"]
    Three,
}
impl OctaveParam {
    pub const fn octaves(self) -> i8 {
        match self {
            Self::Zero => 0,
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
        }
    }
}

type ValueToString = Arc<dyn Fn(f32) -> String + Send + Sync>;
type StringToValue = Arc<dyn Fn(&str) -> Option<f32> + Send + Sync>;

fn v2s_percent() -> ValueToString {
    Arc::new(|value| format!("{:.0} %", value * 100.0))
}
fn s2v_percent() -> StringToValue {
    Arc::new(|text| {
        text.trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| value / 100.0)
    })
}
/// Tenths of a millisecond below a second, hundredths of a second from it — **the unit chosen from
/// the rounded tenths, not the raw value.** Chosen from the raw value, 0.99995 s printed
/// `1000.0 ms`, which parses to one second and prints `1.00 s`.
fn v2s_time() -> ValueToString {
    Arc::new(|seconds| {
        let ms = seconds * 1_000.0;
        if (ms * 10.0).round() >= 10_000.0 {
            format!("{seconds:.2} s")
        } else {
            format!("{ms:.1} ms")
        }
    })
}
fn v2s_cutoff_hz_then_khz() -> ValueToString {
    Arc::new(|value| {
        // The mixed-unit formatter's `1.0 kHz` bucket parses to 1000 Hz, whose normalized inverse
        // can preview just below the unit boundary and become `1000.0 Hz`. Give only that ambiguous
        // bucket an integer-Hz spelling; all other values retain the usual compact presentation.
        let rounded_hz = value.round();
        if (1_000.0..=1_050.0).contains(&rounded_hz) {
            format!("{rounded_hz:.0} Hz")
        } else if value < 1_000.0 {
            format!("{value:.1} Hz")
        } else {
            format!("{:.1} kHz", value / 1_000.0)
        }
    })
}

fn s2v_time() -> StringToValue {
    Arc::new(|text| {
        let text = text.trim().to_ascii_lowercase();
        let (number, scale) = if let Some(number) = text.strip_suffix("ms") {
            (number, 0.001)
        } else if let Some(number) = text.strip_suffix('s') {
            (number, 1.0)
        } else {
            (text.as_str(), 0.001)
        };
        number.trim().parse::<f32>().ok().map(|value| value * scale)
    })
}

fn amount(name: &'static str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(SIGNAL_SMOOTHING_MS))
        .with_value_to_string(v2s_percent())
        .with_string_to_value(s2v_percent())
}

fn seconds(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min,
            max,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_value_to_string(v2s_time())
    .with_string_to_value(s2v_time())
}

fn semitones(name: &'static str, default: f32, reach: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Linear {
            min: -reach,
            max: reach,
        },
    )
    .with_smoother(SmoothingStyle::Linear(PITCH_SMOOTHING_MS))
    .with_unit(" st")
    .with_value_to_string(formatters::v2s_f32_rounded(2))
}

fn envelope(
    prefix: &'static str,
    defaults: AdsrParams,
) -> (FloatParam, FloatParam, FloatParam, FloatParam) {
    (
        seconds(
            match prefix {
                "Filter" => "Filter envelope attack",
                _ => "Amplifier envelope attack",
            },
            defaults.attack_s,
            MIN_TIME_S,
            MAX_TIME_S,
        ),
        seconds(
            match prefix {
                "Filter" => "Filter envelope decay",
                _ => "Amplifier envelope decay",
            },
            defaults.decay_s,
            MIN_TIME_S,
            MAX_TIME_S,
        ),
        amount(
            match prefix {
                "Filter" => "Filter envelope sustain",
                _ => "Amplifier envelope sustain",
            },
            defaults.sustain,
        ),
        seconds(
            match prefix {
                "Filter" => "Filter envelope release",
                _ => "Amplifier envelope release",
            },
            defaults.release_s,
            MIN_TIME_S,
            MAX_TIME_S,
        ),
    )
}

/// **The LFO rate's tempo sync** (`plans/plan-tempo-sync-controls.md`): every LFO's ladder, 1/32 to
/// four bars, the top the fastest.
pub const LFO_SYNC: mxm_tempo::Ladder =
    mxm_tempo::Ladder::new(mxm_tempo::Span::LFO, mxm_tempo::Direction::Rate);

#[derive(Params)]
pub struct MxmMonoPr1Params {
    // Voice and articulation.
    #[id = "key_mode"]
    pub key_mode: EnumParam<KeyModeParam>,
    #[id = "glide_mode"]
    pub glide_mode: EnumParam<GlideModeParam>,
    #[id = "glide_time"]
    pub glide_time: FloatParam,
    #[id = "repeat_external"]
    pub repeat_external: BoolParam,
    #[id = "drone"]
    pub drone: BoolParam,
    #[id = "master_tune"]
    pub master_tune: FloatParam,
    #[id = "bend_range"]
    pub bend_range: FloatParam,

    // Additive LFO.
    #[id = "lfo_rate"]
    pub lfo_rate: FloatParam,
    /// The LFO rate's tempo sync: its position picks a division of the host's tempo.
    #[id = "lfo_sync"]
    pub lfo_sync: BoolParam,
    #[id = "lfo_saw"]
    pub lfo_saw: BoolParam,
    #[id = "lfo_triangle"]
    pub lfo_triangle: BoolParam,
    #[id = "lfo_square"]
    pub lfo_square: BoolParam,

    // Independent envelopes.
    #[id = "filter_env_attack"]
    pub filter_env_attack: FloatParam,
    #[id = "filter_env_decay"]
    pub filter_env_decay: FloatParam,
    #[id = "filter_env_sustain"]
    pub filter_env_sustain: FloatParam,
    #[id = "filter_env_release"]
    pub filter_env_release: FloatParam,
    #[id = "amp_env_attack"]
    pub amp_env_attack: FloatParam,
    #[id = "amp_env_decay"]
    pub amp_env_decay: FloatParam,
    #[id = "amp_env_sustain"]
    pub amp_env_sustain: FloatParam,
    #[id = "amp_env_release"]
    pub amp_env_release: FloatParam,

    /// Modulation routing: one presence and one amount per *(target, source)* pair.
    ///
    /// **Thirteen permanent ids retired into this.** The eleven the Direct/Wheel buses named, plus
    /// `filter_env_amount` and `filter_keyboard_amount`, the two hard-wired cutoff paths. See
    /// [`crate::routes`].
    #[nested(group = "Modulation")]
    pub routes: crate::routes::Routes,

    // Oscillator A.
    #[id = "osc_a_octave"]
    pub osc_a_octave: EnumParam<OctaveParam>,
    #[id = "osc_a_frequency"]
    pub osc_a_frequency: FloatParam,
    #[id = "osc_a_pulse_width"]
    pub osc_a_pulse_width: FloatParam,
    #[id = "osc_a_saw"]
    pub osc_a_saw: BoolParam,
    #[id = "osc_a_pulse"]
    pub osc_a_pulse: BoolParam,
    #[id = "osc_a_sync"]
    pub osc_a_sync: BoolParam,

    // Oscillator B.
    #[id = "osc_b_octave"]
    pub osc_b_octave: EnumParam<OctaveParam>,
    #[id = "osc_b_frequency"]
    pub osc_b_frequency: FloatParam,
    #[id = "osc_b_pulse_width"]
    pub osc_b_pulse_width: FloatParam,
    #[id = "osc_b_saw"]
    pub osc_b_saw: BoolParam,
    #[id = "osc_b_triangle"]
    pub osc_b_triangle: BoolParam,
    #[id = "osc_b_pulse"]
    pub osc_b_pulse: BoolParam,
    #[id = "osc_b_low_frequency"]
    pub osc_b_low_frequency: BoolParam,
    #[id = "osc_b_keyboard_follow"]
    pub osc_b_keyboard_follow: BoolParam,

    // Audio path.
    #[id = "osc_a_level"]
    pub osc_a_level: FloatParam,
    #[id = "osc_b_level"]
    pub osc_b_level: FloatParam,
    #[id = "noise_external_level"]
    pub noise_external_level: FloatParam,
    #[id = "cutoff"]
    pub cutoff: FloatParam,
    #[id = "resonance"]
    pub resonance: FloatParam,
    #[id = "volume"]
    pub volume: FloatParam,

    /// Loaded preset identity and canonical baseline, persisted with plugin state.
    #[persist = "preset"]
    pub preset: RwLock<mxm_preset::PresetIdentity>,
}

impl Default for MxmMonoPr1Params {
    fn default() -> Self {
        let defaults = VoiceParams::init();
        let (filter_env_attack, filter_env_decay, filter_env_sustain, filter_env_release) =
            envelope("Filter", defaults.control.filter_envelope);
        let (amp_env_attack, amp_env_decay, amp_env_sustain, amp_env_release) =
            envelope("Amplifier", defaults.control.amplifier_envelope);

        Self {
            key_mode: EnumParam::new("Key mode", KeyModeParam::Normal),
            glide_mode: EnumParam::new("Glide mode", GlideModeParam::Auto),
            glide_time: seconds(
                "Glide time",
                defaults.control.glide_time_s,
                0.0,
                GLIDE_MAX_S,
            ),
            // The id keeps the name the external clock gave it; the external clock is gone (the
            // owner, 2026-09-26), so the name is what the switch does.
            repeat_external: BoolParam::new("Repeat", defaults.control.repeat),
            drone: BoolParam::new("Drone", defaults.control.drone),
            master_tune: semitones(
                "Master tune",
                defaults.master_tune_semitones,
                MASTER_TUNE_REACH_ST,
            ),
            bend_range: FloatParam::new(
                "Bend range",
                7.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: BEND_RANGE_MAX_ST,
                },
            )
            .with_smoother(SmoothingStyle::Linear(PITCH_SMOOTHING_MS))
            .with_unit(" st")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            lfo_rate: FloatParam::new(
                "LFO rate",
                defaults.control.lfo_rate_hz,
                FloatRange::Skewed {
                    min: RATE_MIN_HZ,
                    max: RATE_MAX_HZ,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            lfo_sync: BoolParam::new("LFO sync", false),
            lfo_saw: BoolParam::new("LFO saw", defaults.control.lfo_waves.saw),
            lfo_triangle: BoolParam::new("LFO triangle", defaults.control.lfo_waves.triangle),
            lfo_square: BoolParam::new("LFO square", defaults.control.lfo_waves.square),

            filter_env_attack,
            filter_env_decay,
            filter_env_sustain,
            filter_env_release,
            amp_env_attack,
            amp_env_decay,
            amp_env_sustain,
            amp_env_release,

            routes: crate::routes::Routes::new(),

            osc_a_octave: EnumParam::new("Oscillator A octave", OctaveParam::Zero),
            osc_a_frequency: semitones(
                "Oscillator A frequency",
                defaults.oscillator_a.tune_semitones,
                OSCILLATOR_FREQUENCY_REACH_ST,
            ),
            osc_a_pulse_width: amount(
                "Oscillator A pulse width",
                defaults.oscillator_a.pulse_width,
            ),
            osc_a_saw: BoolParam::new("Oscillator A saw", defaults.oscillator_a.waves.saw),
            osc_a_pulse: BoolParam::new("Oscillator A pulse", defaults.oscillator_a.waves.pulse),
            osc_a_sync: BoolParam::new("Oscillator A sync", defaults.oscillator_a.sync),

            osc_b_octave: EnumParam::new("Oscillator B octave", OctaveParam::Zero),
            osc_b_frequency: semitones(
                "Oscillator B frequency",
                defaults.oscillator_b.tune_semitones,
                OSCILLATOR_FREQUENCY_REACH_ST,
            ),
            osc_b_pulse_width: amount(
                "Oscillator B pulse width",
                defaults.oscillator_b.pulse_width,
            ),
            osc_b_saw: BoolParam::new("Oscillator B saw", defaults.oscillator_b.waves.saw),
            osc_b_triangle: BoolParam::new(
                "Oscillator B triangle",
                defaults.oscillator_b.waves.triangle,
            ),
            osc_b_pulse: BoolParam::new("Oscillator B pulse", defaults.oscillator_b.waves.pulse),
            osc_b_low_frequency: BoolParam::new(
                "Oscillator B low frequency",
                defaults.oscillator_b.low_frequency,
            ),
            osc_b_keyboard_follow: BoolParam::new(
                "Oscillator B keyboard follow",
                defaults.oscillator_b.keyboard_follow,
            ),

            osc_a_level: amount("Oscillator A level", defaults.mixer.oscillator_a),
            osc_b_level: amount("Oscillator B level", defaults.mixer.oscillator_b),
            // The id keeps the name the external audio input gave it; that input is gone (the
            // owner, 2026-09-26), so the name is what the level sets.
            noise_external_level: amount("Noise level", defaults.mixer.noise),
            cutoff: FloatParam::new(
                "Cutoff",
                defaults.filter.cutoff_hz,
                FloatRange::Skewed {
                    min: CUTOFF_MIN_HZ,
                    max: CUTOFF_MAX_HZ,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(PITCH_SMOOTHING_MS))
            .with_value_to_string(v2s_cutoff_hz_then_khz())
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            resonance: amount("Resonance", defaults.filter.resonance),
            volume: amount("Volume", defaults.volume),
            preset: RwLock::new(mxm_preset::PresetIdentity::none()),
        }
    }
}

impl MxmMonoPr1Params {
    /// Every permanent parameter in declaration order. Preset serialization and the editor's
    /// coverage test compare against this inventory so neither can silently omit a control.
    pub fn all_parameters(&self) -> Vec<(&'static str, &dyn mxm_preset::ErasedParam)> {
        let mut out: Vec<(&'static str, &dyn mxm_preset::ErasedParam)> = vec![
            ("key_mode", &self.key_mode),
            ("glide_mode", &self.glide_mode),
            ("glide_time", &self.glide_time),
            ("repeat_external", &self.repeat_external),
            ("drone", &self.drone),
            ("master_tune", &self.master_tune),
            ("bend_range", &self.bend_range),
            ("lfo_rate", &self.lfo_rate),
            ("lfo_sync", &self.lfo_sync),
            ("lfo_saw", &self.lfo_saw),
            ("lfo_triangle", &self.lfo_triangle),
            ("lfo_square", &self.lfo_square),
            ("filter_env_attack", &self.filter_env_attack),
            ("filter_env_decay", &self.filter_env_decay),
            ("filter_env_sustain", &self.filter_env_sustain),
            ("filter_env_release", &self.filter_env_release),
            ("amp_env_attack", &self.amp_env_attack),
            ("amp_env_decay", &self.amp_env_decay),
            ("amp_env_sustain", &self.amp_env_sustain),
            ("amp_env_release", &self.amp_env_release),
            ("osc_a_octave", &self.osc_a_octave),
            ("osc_a_frequency", &self.osc_a_frequency),
            ("osc_a_pulse_width", &self.osc_a_pulse_width),
            ("osc_a_saw", &self.osc_a_saw),
            ("osc_a_pulse", &self.osc_a_pulse),
            ("osc_a_sync", &self.osc_a_sync),
            ("osc_b_octave", &self.osc_b_octave),
            ("osc_b_frequency", &self.osc_b_frequency),
            ("osc_b_pulse_width", &self.osc_b_pulse_width),
            ("osc_b_saw", &self.osc_b_saw),
            ("osc_b_triangle", &self.osc_b_triangle),
            ("osc_b_pulse", &self.osc_b_pulse),
            ("osc_b_low_frequency", &self.osc_b_low_frequency),
            ("osc_b_keyboard_follow", &self.osc_b_keyboard_follow),
            ("osc_a_level", &self.osc_a_level),
            ("osc_b_level", &self.osc_b_level),
            ("noise_external_level", &self.noise_external_level),
            ("cutoff", &self.cutoff),
            ("resonance", &self.resonance),
            ("volume", &self.volume),
        ];
        // Every routing pair, in `[target][source]` order. Appended rather than interleaved so the
        // voice's own controls keep the positions presets already wrote them in.
        for (target, group) in self.routes.ordinary() {
            for (slot, route) in group.routes(target).into_iter().enumerate() {
                let (amount_id, present_id) = crate::routes::ROUTE_IDS[target][slot];
                out.push((present_id, route.present));
                out.push((amount_id, route.amount));
            }
        }
        // The module last, with one fewer pair: the wheel is not a source on it.
        let bus = mxm_mono_pr1_dsp::routing::target::MOD_BUS;
        for (slot, route) in self.routes.mod_bus.routes().into_iter().enumerate() {
            let (amount_id, present_id) = crate::routes::ROUTE_IDS[bus][slot];
            out.push((present_id, route.present));
            out.push((amount_id, route.amount));
        }
        out
    }

    /// The LFO rate while its sync follows the host, or `None` for its free value: the modulated
    /// position picks a division on [`LFO_SYNC`]. Resolved once a buffer by the plugin.
    pub fn synced_lfo_rate(&self, tempo: Option<f64>) -> Option<f32> {
        let param = &self.lfo_rate;
        LFO_SYNC
            .resolve(
                self.lfo_sync.value(),
                tempo,
                param.modulated_normalized_value(),
                f64::from(param.preview_plain(0.0)),
                f64::from(param.preview_plain(1.0)),
            )
            .map(|hz| hz as f32)
    }

    /// Advance every smoothed signal exactly once and build the framework-free DSP patch.
    #[inline]
    pub fn next_voice_params(&self) -> VoiceParams {
        VoiceParams {
            control: ControlParams {
                key_mode: self.key_mode.value().into(),
                glide_mode: self.glide_mode.value().into(),
                glide_time_s: self.glide_time.value(),
                repeat: self.repeat_external.value(),
                drone: self.drone.value(),
                lfo_rate_hz: self.lfo_rate.value(),
                lfo_waves: WaveEnables {
                    saw: self.lfo_saw.value(),
                    triangle: self.lfo_triangle.value(),
                    square: self.lfo_square.value(),
                },
                filter_envelope: AdsrParams {
                    attack_s: self.filter_env_attack.value(),
                    decay_s: self.filter_env_decay.value(),
                    sustain: self.filter_env_sustain.smoothed.next(),
                    release_s: self.filter_env_release.value(),
                },
                amplifier_envelope: AdsrParams {
                    attack_s: self.amp_env_attack.value(),
                    decay_s: self.amp_env_decay.value(),
                    sustain: self.amp_env_sustain.smoothed.next(),
                    release_s: self.amp_env_release.value(),
                },
                bend_range_semitones: self.bend_range.smoothed.next(),
            },
            master_tune_semitones: self.master_tune.smoothed.next(),
            oscillator_a: OscillatorAParams {
                octave: self.osc_a_octave.value().octaves(),
                tune_semitones: self.osc_a_frequency.smoothed.next(),
                pulse_width: self.osc_a_pulse_width.smoothed.next(),
                waves: AWaves {
                    saw: self.osc_a_saw.value(),
                    pulse: self.osc_a_pulse.value(),
                },
                sync: self.osc_a_sync.value(),
            },
            oscillator_b: OscillatorBParams {
                octave: self.osc_b_octave.value().octaves(),
                tune_semitones: self.osc_b_frequency.smoothed.next(),
                pulse_width: self.osc_b_pulse_width.smoothed.next(),
                waves: BWaves {
                    saw: self.osc_b_saw.value(),
                    triangle: self.osc_b_triangle.value(),
                    pulse: self.osc_b_pulse.value(),
                },
                low_frequency: self.osc_b_low_frequency.value(),
                keyboard_follow: self.osc_b_keyboard_follow.value(),
            },
            mixer: MixerParams {
                oscillator_a: self.osc_a_level.smoothed.next(),
                oscillator_b: self.osc_b_level.smoothed.next(),
                noise: self.noise_external_level.smoothed.next(),
            },
            filter: FilterParams {
                cutoff_hz: self.cutoff.smoothed.next(),
                resonance: self.resonance.smoothed.next(),
            },
            volume: self.volume.smoothed.next(),
        }
    }

    /// Fields consulted while applying host events or classifying activity without advancing a
    /// sample. Nothing here advances a smoother, and **no routing is read**: topology is armed once
    /// per interval and the amounts belong to the rendered sample.
    pub fn event_control_params(&self) -> ControlParams {
        ControlParams {
            key_mode: self.key_mode.value().into(),
            glide_mode: self.glide_mode.value().into(),
            repeat: self.repeat_external.value(),
            drone: self.drone.value(),
            bend_range_semitones: self.bend_range.value(),
            ..ControlParams::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The LFO sync picks a division and is inert without a tempo**
    /// (`plans/plan-tempo-sync-controls.md`): off, or with no tempo, the knob's own hertz stand; on
    /// at 120 bpm the ends are the ladder's ends that the range can hold, the top the fastest.
    #[test]
    fn lfo_sync_picks_a_division_and_is_inert_without_a_tempo() {
        use nice_plug::params::InternalParamMut;
        fn set<P: InternalParamMut>(param: &P, normalized: f32) {
            unsafe {
                let _ = param._internal_set_normalized_value(normalized);
            }
        }
        let p = MxmMonoPr1Params::default();
        set(&p.lfo_rate, 1.0);
        assert_eq!(p.synced_lfo_rate(Some(120.0)), None, "off is the free rate");
        set(&p.lfo_sync, 1.0);
        assert_eq!(p.synced_lfo_rate(None), None, "no tempo is the free rate");

        let top = p.synced_lfo_rate(Some(120.0)).expect("synced at a tempo");
        set(&p.lfo_rate, 0.0);
        let bottom = p.synced_lfo_rate(Some(120.0)).expect("synced at a tempo");
        let (lo, hi) = (
            f64::from(p.lfo_rate.preview_plain(0.0)),
            f64::from(p.lfo_rate.preview_plain(1.0)),
        );
        assert!(
            top > bottom,
            "the top of a rate is the fastest: {bottom} to {top}"
        );
        let reach = LFO_SYNC.reachable(120.0, lo, hi).divisions();
        let fastest = reach[0].hz(120.0) as f32;
        let slowest = reach[reach.len() - 1].hz(120.0) as f32;
        assert!((top - fastest).abs() < 1e-4, "{top} against {fastest}");
        assert!(
            (bottom - slowest).abs() < 1e-4,
            "{bottom} against {slowest}"
        );
    }

    /// Plain values either side of every point where a formatter here changes unit, precision or
    /// sign. Each parameter clamps what lies outside its own range, so one list serves them all.
    const BOUNDARIES: [f32; 30] = [
        // Master tune, the oscillator frequencies and every route amount cross zero: a
        // ten-thousandth and a thousandth of the unit, and the half-hundredth where two decimals tie.
        -0.005, -1.0e-3, -1.0e-4, 0.0, 1.0e-4, 1.0e-3, 0.005,
        // Envelope and glide times read tenths of a millisecond below a second and hundredths of a
        // second above: the rounding edge below one second and the `1.00 s` bucket above it.
        0.9999, 0.99994, 0.99995, 0.99996, 1.0, 1.004, 1.005, 1.006,
        // Cutoff reads tenths of a hertz, then whole hertz across the `1.0 kHz` bucket, then kHz:
        // either side of the tenth that rounds to 999.5 Hz, of 1000 Hz, and of 1050.5 Hz.
        999.4, 999.44, 999.45, 999.46, 999.49, 999.5, 999.9, 999.95, 1_000.0, 1_000.1, 1_049.9,
        1_050.4, 1_050.45, 1_050.5, 1_050.6,
    ];

    /// **Every parameter's text survives the host's own conversion.** The CLAP wrapper formats a
    /// normalised value, parses the text back to a normalised value and formats that again, so a
    /// reading that chooses its unit or its sign from the raw value can print one text, parse to the
    /// other side of its own switch and print another — which `clap-validator`'s
    /// `param-conversions` fails only when its values land in that sliver, so a clean run proves
    /// nothing (mxm-kit's `docs/code-review-notes.md` §6). This walks every parameter, with the
    /// unit on as the host sees it, across clap-validator 0.4.1's own grid, the collection's
    /// `i / 19` grid, and the normalised neighbours of every value in [`BOUNDARIES`].
    #[test]
    fn every_parameter_text_is_idempotent_through_the_hosts_conversion() {
        let params = MxmMonoPr1Params::default();
        let map = params.param_map();
        let validator_values = 4_000_usize.div_ceil(map.len()).clamp(5, 100);
        let mut failures: Vec<String> = Vec::new();
        for (id, ptr, _group) in &map {
            // SAFETY: `params` owns every parameter these pointers refer to and outlives the loop;
            // this is the same access `param-conversions` makes through CLAP.
            unsafe {
                // The wrapper hands CLAP `normalised × step count` and divides by it on the way in.
                let steps = ptr.step_count().unwrap_or(1) as f64;
                let from_clap = |value: f64| value as f32 / steps as f32;
                let grid = (0..=19).map(|i| (i as f32 / 19.0, "grid"));
                let validator = (0..validator_values).map(|i| {
                    let value = steps * (i as f64 / (validator_values - 1) as f64);
                    (from_clap(value), "validator grid")
                });
                let boundary = BOUNDARIES.into_iter().flat_map(|plain| {
                    let at = ptr.preview_normalized(plain).clamp(0.0, 1.0);
                    [at.next_down().max(0.0), at, at.next_up().min(1.0)].map(|n| (n, "boundary"))
                });
                for (value, from) in grid.chain(validator).chain(boundary) {
                    let first = ptr.normalized_value_to_string(value, true);
                    let second = ptr.string_to_normalized_value(&first).map(|parsed| {
                        ptr.normalized_value_to_string(from_clap(parsed as f64 * steps), true)
                    });
                    if second.as_deref() != Some(first.as_str()) {
                        let failure = format!("{id}: {first:?} reads back as {second:?}");
                        if failures
                            .last()
                            .is_none_or(|last| !last.starts_with(&failure))
                        {
                            let plain = ptr.preview_plain(value);
                            failures.push(format!("{failure} ({from}, plain {plain})"));
                        }
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} parameter texts changed through the host's conversion:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
