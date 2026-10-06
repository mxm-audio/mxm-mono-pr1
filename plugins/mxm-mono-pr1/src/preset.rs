//! Factory sounds and the instrument-specific side of the shared preset system.

use crate::params::MxmMonoPr1Params;
pub use mxm_preset::{
    Category, Entry, INIT_NAME, Library, Loaded, Origin, Preset, PresetIdentity, Refused, Value,
    factory, loaded, mark_loaded, mark_none, read_favourites, snapshot, write_favourites,
};
use std::sync::RwLock;

/// **The tempo syncs this plugin gained on 2026-09-25** (`plans/plan-tempo-sync-controls.md`). A
/// preset file written before them was written unsynced, so each loads off rather than keeping the
/// instance's sync, and without reporting a missing control.
pub(crate) const TEMPO_SYNC_IDS: &[&str] = &["lfo_sync"];

impl mxm_preset::Instrument for MxmMonoPr1Params {
    fn clap_id(&self) -> &'static str {
        crate::CLAP_ID
    }

    fn parameters(&self) -> Vec<(&'static str, &dyn mxm_preset::ErasedParam)> {
        self.all_parameters()
    }

    fn identity(&self) -> &RwLock<PresetIdentity> {
        &self.preset
    }

    fn factory_files(&self) -> &'static [(&'static str, &'static str)] {
        FACTORY_FILES
    }

    fn default_missing_legacy_parameter(&self, id: &str) -> bool {
        TEMPO_SYNC_IDS.contains(&id)
    }
}

pub const FACTORY_FILES: &[(&str, &str)] = &[
    ("Basic saw", include_str!("../presets/basic-saw.json")),
    ("Beating pair", include_str!("../presets/beating-pair.json")),
    (
        "Twin saw bass",
        include_str!("../presets/twin-saw-bass.json"),
    ),
    (
        "Square foundation",
        include_str!("../presets/square-foundation.json"),
    ),
    (
        "Saw pulse stack",
        include_str!("../presets/saw-pulse-stack.json"),
    ),
    (
        "Triple B stack",
        include_str!("../presets/triple-b-stack.json"),
    ),
    (
        "Hollow pulses",
        include_str!("../presets/hollow-pulses.json"),
    ),
    ("Octave pair", include_str!("../presets/octave-pair.json")),
    ("Fifth pair", include_str!("../presets/fifth-pair.json")),
    ("Wide detune", include_str!("../presets/wide-detune.json")),
    ("Soft attack", include_str!("../presets/soft-attack.json")),
    ("Long release", include_str!("../presets/long-release.json")),
    ("Short pluck", include_str!("../presets/short-pluck.json")),
    ("Bright pluck", include_str!("../presets/bright-pluck.json")),
    ("Filter snap", include_str!("../presets/filter-snap.json")),
    (
        "Resonant bass",
        include_str!("../presets/resonant-bass.json"),
    ),
    (
        "Tracking lead",
        include_str!("../presets/tracking-lead.json"),
    ),
    ("Open brass", include_str!("../presets/open-brass.json")),
    ("Muted brass", include_str!("../presets/muted-brass.json")),
    ("Singing lead", include_str!("../presets/singing-lead.json")),
    (
        "Vibrato direct",
        include_str!("../presets/vibrato-direct.json"),
    ),
    (
        "Vibrato wheel",
        include_str!("../presets/vibrato-wheel.json"),
    ),
    (
        "Filter motion direct",
        include_str!("../presets/filter-motion-direct.json"),
    ),
    (
        "Filter motion wheel",
        include_str!("../presets/filter-motion-wheel.json"),
    ),
    (
        "Dual envelope sweep",
        include_str!("../presets/dual-envelope-sweep.json"),
    ),
    (
        "Pulse sweep A",
        include_str!("../presets/pulse-sweep-a.json"),
    ),
    (
        "Pulse sweep B",
        include_str!("../presets/pulse-sweep-b.json"),
    ),
    (
        "Twin pulse motion",
        include_str!("../presets/twin-pulse-motion.json"),
    ),
    (
        "Slow LFO drift",
        include_str!("../presets/slow-lfo-drift.json"),
    ),
    ("Fast trill", include_str!("../presets/fast-trill.json")),
    (
        "Hard sync lead",
        include_str!("../presets/hard-sync-lead.json"),
    ),
    ("Hidden sync", include_str!("../presets/hidden-sync.json")),
    (
        "Sync pulse dropout",
        include_str!("../presets/sync-pulse-dropout.json"),
    ),
    ("B low sweep", include_str!("../presets/b-low-sweep.json")),
    ("B self bend", include_str!("../presets/b-self-bend.json")),
    (
        "B pulse self motion",
        include_str!("../presets/b-pulse-self-motion.json"),
    ),
    (
        "B filter motion",
        include_str!("../presets/b-filter-motion.json"),
    ),
    ("Mixed buses", include_str!("../presets/mixed-buses.json")),
    ("Wheel timbre", include_str!("../presets/wheel-timbre.json")),
    ("Normal glide", include_str!("../presets/normal-glide.json")),
    ("Auto glide", include_str!("../presets/auto-glide.json")),
    (
        "Retrigger lead",
        include_str!("../presets/retrigger-lead.json"),
    ),
    ("Repeat pulse", include_str!("../presets/repeat-pulse.json")),
    (
        "Repeat bare clock",
        include_str!("../presets/repeat-bare-clock.json"),
    ),
    (
        "Sustain drone",
        include_str!("../presets/sustain-drone.json"),
    ),
    ("Noise wash", include_str!("../presets/noise-wash.json")),
    ("Noise strike", include_str!("../presets/noise-strike.json")),
    ("Repeat noise", include_str!("../presets/repeat-noise.json")),
    ("Tempo repeat", include_str!("../presets/tempo-repeat.json")),
    (
        "Signal laboratory",
        include_str!("../presets/signal-laboratory.json"),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// **A project saved before the tempo syncs restores them Off** (`mxm_preset::add_switches_off`),
    /// whatever this instance had.
    #[test]
    fn an_older_state_restores_the_tempo_syncs_off() {
        use nice_plug::prelude::Plugin as _;
        let mut state = nice_plug::prelude::PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        };
        crate::MxmMonoPr1::filter_state(&mut state);
        for id in TEMPO_SYNC_IDS {
            assert!(
                matches!(
                    state.params.get(*id),
                    Some(nice_plug::plugin::ParamValue::Bool(false))
                ),
                "{{id}} was not restored off"
            );
        }
    }

    /// **A preset saved before the tempo syncs loads them off, and cleanly** ([`TEMPO_SYNC_IDS`]).
    #[test]
    fn a_preset_from_before_the_tempo_syncs_loads_them_off() {
        let params = crate::params::MxmMonoPr1Params::default();
        let mut old = mxm_preset::Preset::init(&params);
        for id in TEMPO_SYNC_IDS {
            old.params.remove(*id);
        }
        let (writes, problems) = old.resolve(&params);
        assert!(problems.is_empty(), "{{problems:?}}");
        for id in TEMPO_SYNC_IDS {
            assert!(
                writes.iter().any(|(w, _, v)| w == id && *v == 0.0),
                "{{id}} was not written off"
            );
        }
    }

    use mxm_mono_pr1_dsp::{control::Event, keyboard::NoteId, voice::Voice};
    use mxm_preset::user_root;
    use nice_plug::prelude::{Param, Params};

    type Design = (&'static str, Category, &'static [(&'static str, f32)]);

    /// Readable overrides on the complete generated Init patch. Values are normalized host values.
    const FACTORY_DESIGN: &[Design] = &[
        ("Basic saw", Category::Template, &[("cutoff", 0.86)]),
        (
            "Beating pair",
            Category::Pad,
            &[
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.72),
                ("osc_b_frequency", 0.506),
                ("cutoff", 0.78),
                ("amp_env_release", 0.38),
            ],
        ),
        (
            "Twin saw bass",
            Category::Bass,
            &[
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.65),
                ("osc_a_octave", 0.0),
                ("osc_b_octave", 0.0),
                ("cutoff", 0.58),
                ("amp_env_decay", 0.28),
                ("amp_env_sustain", 0.35),
                ("mod_cutoff_fenv", 0.58),
            ],
        ),
        (
            "Square foundation",
            Category::Keys,
            &[
                ("osc_a_saw", 0.0),
                ("osc_a_pulse", 1.0),
                ("osc_a_pulse_width", 0.5),
                ("cutoff", 0.76),
            ],
        ),
        (
            "Saw pulse stack",
            Category::Lead,
            &[
                ("osc_a_pulse", 1.0),
                ("osc_a_pulse_width", 0.34),
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.45),
                ("cutoff", 0.8),
            ],
        ),
        (
            "Triple B stack",
            Category::Lead,
            &[
                ("osc_b_saw", 1.0),
                ("osc_b_triangle", 1.0),
                ("osc_b_pulse", 1.0),
                ("osc_b_level", 0.68),
                ("osc_a_level", 0.35),
                ("cutoff", 0.82),
            ],
        ),
        (
            "Hollow pulses",
            Category::Bass,
            &[
                ("osc_a_saw", 0.0),
                ("osc_a_pulse", 1.0),
                ("osc_b_pulse", 1.0),
                ("osc_b_level", 0.7),
                ("osc_a_pulse_width", 0.28),
                ("osc_b_pulse_width", 0.72),
                ("cutoff", 0.6),
            ],
        ),
        (
            "Octave pair",
            Category::Lead,
            &[
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.62),
                ("osc_b_octave", 0.333333),
                ("cutoff", 0.8),
            ],
        ),
        (
            "Fifth pair",
            Category::Lead,
            &[
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.7),
                ("osc_b_frequency", 0.791667),
                ("cutoff", 0.76),
            ],
        ),
        (
            "Wide detune",
            Category::Pad,
            &[
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.8),
                ("osc_b_frequency", 0.56),
                ("cutoff", 0.7),
                ("amp_env_attack", 0.32),
                ("amp_env_release", 0.46),
            ],
        ),
        (
            "Soft attack",
            Category::Pad,
            &[
                ("osc_b_triangle", 1.0),
                ("osc_b_level", 0.55),
                ("cutoff", 0.68),
                ("amp_env_attack", 0.48),
                ("amp_env_release", 0.48),
            ],
        ),
        (
            "Long release",
            Category::Pad,
            &[
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.5),
                ("cutoff", 0.7),
                ("amp_env_attack", 0.25),
                ("amp_env_release", 0.7),
                ("filter_env_release", 0.66),
            ],
        ),
        (
            "Short pluck",
            Category::Pluck,
            &[
                ("cutoff", 0.58),
                ("filter_env_decay", 0.2),
                ("filter_env_sustain", 0.0),
                ("amp_env_decay", 0.2),
                ("amp_env_sustain", 0.0),
                ("amp_env_release", 0.14),
                ("mod_cutoff_fenv", 0.645),
            ],
        ),
        (
            "Bright pluck",
            Category::Pluck,
            &[
                ("osc_a_pulse", 1.0),
                ("cutoff", 0.7),
                ("filter_env_decay", 0.16),
                ("filter_env_sustain", 0.0),
                ("amp_env_decay", 0.24),
                ("amp_env_sustain", 0.0),
                ("mod_cutoff_fenv", 0.62),
            ],
        ),
        (
            "Filter snap",
            Category::Pluck,
            &[
                ("cutoff", 0.48),
                ("resonance", 0.42),
                ("filter_env_decay", 0.12),
                ("filter_env_sustain", 0.0),
                ("amp_env_decay", 0.3),
                ("amp_env_sustain", 0.0),
                ("mod_cutoff_fenv", 0.68),
            ],
        ),
        (
            "Resonant bass",
            Category::Bass,
            &[
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.45),
                ("cutoff", 0.5),
                ("resonance", 0.58),
                ("filter_env_decay", 0.3),
                ("filter_env_sustain", 0.2),
                ("mod_cutoff_fenv", 0.605),
            ],
        ),
        (
            "Tracking lead",
            Category::Lead,
            &[
                ("cutoff", 0.62),
                ("resonance", 0.35),
                ("osc_a_pulse", 1.0),
                ("mod_cutoff_key", 0.86),
            ],
        ),
        (
            "Open brass",
            Category::Brass,
            &[
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.65),
                ("cutoff", 0.54),
                ("filter_env_attack", 0.26),
                ("filter_env_decay", 0.38),
                ("filter_env_sustain", 0.55),
                ("amp_env_attack", 0.2),
                ("amp_env_release", 0.3),
                ("mod_cutoff_fenv", 0.625),
            ],
        ),
        (
            "Muted brass",
            Category::Brass,
            &[
                ("osc_a_pulse", 1.0),
                ("osc_b_pulse", 1.0),
                ("osc_b_level", 0.55),
                ("cutoff", 0.46),
                ("filter_env_attack", 0.18),
                ("filter_env_decay", 0.35),
                ("filter_env_sustain", 0.45),
                ("mod_cutoff_fenv", 0.59),
            ],
        ),
        (
            "Singing lead",
            Category::Lead,
            &[
                ("osc_b_triangle", 1.0),
                ("osc_b_level", 0.35),
                ("cutoff", 0.7),
                ("resonance", 0.3),
                ("amp_env_attack", 0.16),
                ("amp_env_release", 0.36),
            ],
        ),
        (
            "Vibrato direct",
            Category::Lead,
            &[
                ("lfo_triangle", 1.0),
                ("cutoff", 0.74),
                ("mod_afreq_lfo", 0.54),
            ],
        ),
        (
            "Vibrato wheel",
            Category::Lead,
            &[
                ("lfo_triangle", 1.0),
                ("cutoff", 0.74),
                ("mod_modbus_lfoon", 1.0),
                ("mod_modbus_lfo", 0.59),
                ("mod_afreq_wbuson", 1.0),
                ("mod_afreq_wbus", 1.0),
            ],
        ),
        (
            "Filter motion direct",
            Category::Pad,
            &[
                ("lfo_triangle", 1.0),
                ("cutoff", 0.58),
                ("resonance", 0.32),
                ("amp_env_attack", 0.3),
                ("amp_env_release", 0.45),
                ("mod_cutoff_lfo", 0.58),
            ],
        ),
        (
            "Filter motion wheel",
            Category::Pad,
            &[
                ("lfo_saw", 1.0),
                ("lfo_triangle", 0.0),
                ("cutoff", 0.55),
                ("resonance", 0.4),
                ("mod_modbus_lfoon", 1.0),
                ("mod_modbus_lfo", 0.625),
                ("mod_cutoff_wbuson", 1.0),
                ("mod_cutoff_wbus", 1.0),
            ],
        ),
        (
            "Dual envelope sweep",
            Category::Pluck,
            &[
                ("cutoff", 0.46),
                ("filter_env_decay", 0.3),
                ("filter_env_sustain", 0.05),
                ("amp_env_decay", 0.36),
                ("amp_env_sustain", 0.2),
                ("mod_cutoff_fenv", 0.6875),
            ],
        ),
        (
            "Pulse sweep A",
            Category::Pad,
            &[
                ("osc_a_saw", 0.0),
                ("osc_a_pulse", 1.0),
                ("lfo_triangle", 1.0),
                ("lfo_rate", 0.24),
                ("cutoff", 0.7),
                ("mod_apw_lfo", 0.825),
            ],
        ),
        (
            "Pulse sweep B",
            Category::Pad,
            &[
                ("osc_a_level", 0.25),
                ("osc_b_pulse", 1.0),
                ("osc_b_level", 0.78),
                ("lfo_triangle", 1.0),
                ("lfo_rate", 0.2),
                ("cutoff", 0.68),
                ("mod_bpw_lfoon", 1.0),
                ("mod_bpw_lfo", 0.81),
            ],
        ),
        (
            "Twin pulse motion",
            Category::Pad,
            &[
                ("osc_a_saw", 0.0),
                ("osc_a_pulse", 1.0),
                ("osc_b_pulse", 1.0),
                ("osc_b_level", 0.68),
                ("lfo_triangle", 1.0),
                ("cutoff", 0.7),
                ("mod_apw_lfo", 0.75),
                ("mod_bpw_lfoon", 1.0),
                ("mod_bpw_lfo", 0.75),
            ],
        ),
        (
            "Slow LFO drift",
            Category::Pad,
            &[
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.55),
                ("lfo_saw", 1.0),
                ("lfo_triangle", 0.0),
                ("lfo_rate", 0.08),
                ("cutoff", 0.66),
                ("mod_afreq_lfo", 0.5275),
            ],
        ),
        (
            "Fast trill",
            Category::Lead,
            &[
                ("lfo_square", 1.0),
                ("lfo_triangle", 0.0),
                ("lfo_rate", 0.7),
                ("cutoff", 0.76),
                ("mod_afreq_lfo", 0.6),
            ],
        ),
        (
            "Hard sync lead",
            Category::Lead,
            &[
                ("osc_a_sync", 1.0),
                ("osc_a_pulse", 1.0),
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.28),
                ("osc_b_octave", 0.666667),
                ("cutoff", 0.76),
                ("mod_cutoff_fenv", 0.56),
            ],
        ),
        (
            "Hidden sync",
            Category::Lead,
            &[
                ("osc_a_sync", 1.0),
                ("osc_a_pulse", 1.0),
                ("osc_b_octave", 1.0),
                ("osc_b_frequency", 0.6),
                ("osc_b_level", 0.0),
                ("cutoff", 0.8),
            ],
        ),
        (
            "Sync pulse dropout",
            Category::Fx,
            &[
                ("osc_a_saw", 0.0),
                ("osc_a_pulse", 1.0),
                ("osc_a_pulse_width", 0.9),
                ("osc_a_sync", 1.0),
                ("osc_b_octave", 1.0),
                ("osc_b_frequency", 0.7),
                ("cutoff", 0.86),
                ("amp_env_sustain", 1.0),
            ],
        ),
        (
            "B low sweep",
            Category::Fx,
            &[
                ("osc_b_saw", 1.0),
                ("osc_b_low_frequency", 1.0),
                ("osc_b_keyboard_follow", 0.0),
                ("cutoff", 0.72),
                ("mod_afreq_oscbon", 1.0),
                ("mod_afreq_oscb", 0.61),
            ],
        ),
        (
            "B self bend",
            Category::Fx,
            &[
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.5),
                ("cutoff", 0.7),
                ("mod_bfreq_oscbon", 1.0),
                ("mod_bfreq_oscb", 0.58),
            ],
        ),
        (
            "B pulse self motion",
            Category::Fx,
            &[
                ("osc_b_pulse", 1.0),
                ("osc_b_level", 0.65),
                ("osc_b_pulse_width", 0.35),
                ("cutoff", 0.72),
                ("mod_bpw_oscbon", 1.0),
                ("mod_bpw_oscb", 0.71),
            ],
        ),
        (
            "B filter motion",
            Category::Fx,
            &[
                ("osc_b_triangle", 1.0),
                ("osc_b_level", 0.45),
                ("osc_b_low_frequency", 1.0),
                ("osc_b_keyboard_follow", 0.0),
                ("cutoff", 0.56),
                ("resonance", 0.36),
                ("mod_cutoff_oscbon", 1.0),
                ("mod_cutoff_oscb", 0.66),
            ],
        ),
        (
            "Mixed buses",
            Category::Fx,
            &[
                ("lfo_triangle", 1.0),
                ("cutoff", 0.56),
                ("filter_env_decay", 0.38),
                ("mod_modbus_fenvon", 1.0),
                ("mod_modbus_fenv", 0.675),
                ("mod_afreq_lfo", 0.56),
                ("mod_cutoff_wbuson", 1.0),
                ("mod_cutoff_wbus", 1.0),
            ],
        ),
        (
            "Wheel timbre",
            Category::Lead,
            &[
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.42),
                ("cutoff", 0.58),
                ("resonance", 0.34),
                ("mod_modbus_oscbon", 1.0),
                ("mod_modbus_oscb", 0.64),
                ("mod_cutoff_wbuson", 1.0),
                ("mod_cutoff_wbus", 1.0),
            ],
        ),
        (
            "Normal glide",
            Category::Lead,
            &[
                ("glide_mode", 0.0),
                ("glide_time", 0.34),
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.45),
                ("cutoff", 0.74),
            ],
        ),
        (
            "Auto glide",
            Category::Lead,
            &[
                ("glide_mode", 1.0),
                ("glide_time", 0.3),
                ("osc_a_pulse", 1.0),
                ("cutoff", 0.72),
            ],
        ),
        (
            "Retrigger lead",
            Category::Lead,
            &[
                ("key_mode", 1.0),
                ("osc_b_saw", 1.0),
                ("osc_b_level", 0.48),
                ("filter_env_decay", 0.25),
                ("cutoff", 0.6),
                ("mod_cutoff_fenv", 0.5875),
            ],
        ),
        (
            "Repeat pulse",
            Category::Sequence,
            &[
                ("repeat_external", 1.0),
                ("lfo_square", 1.0),
                ("lfo_triangle", 0.0),
                ("lfo_rate", 0.46),
                ("osc_a_pulse", 1.0),
                ("cutoff", 0.68),
                ("amp_env_decay", 0.18),
                ("amp_env_sustain", 0.35),
            ],
        ),
        (
            "Repeat bare clock",
            Category::Sequence,
            &[
                ("repeat_external", 1.0),
                ("lfo_rate", 0.4),
                ("cutoff", 0.65),
                ("amp_env_decay", 0.16),
                ("amp_env_sustain", 0.25),
                ("mod_cutoff_fenv", 0.575),
            ],
        ),
        (
            "Sustain drone",
            Category::Drone,
            &[
                ("drone", 1.0),
                ("osc_b_triangle", 1.0),
                ("osc_b_level", 0.48),
                ("cutoff", 0.6),
                ("amp_env_attack", 0.28),
                ("amp_env_sustain", 0.72),
                ("filter_env_sustain", 0.5),
            ],
        ),
        (
            "Noise wash",
            Category::Fx,
            &[
                ("osc_a_level", 0.0),
                ("noise_external_level", 0.8),
                ("cutoff", 0.52),
                ("resonance", 0.45),
                ("amp_env_attack", 0.45),
                ("amp_env_release", 0.55),
                ("mod_cutoff_fenv", 0.575),
            ],
        ),
        (
            "Noise strike",
            Category::Percussion,
            &[
                ("osc_a_level", 0.0),
                ("noise_external_level", 0.9),
                ("cutoff", 0.7),
                ("resonance", 0.22),
                ("filter_env_decay", 0.12),
                ("filter_env_sustain", 0.0),
                ("amp_env_decay", 0.12),
                ("amp_env_sustain", 0.0),
                ("amp_env_release", 0.08),
            ],
        ),
        // Was "External follower", which is what it played in any host that connected no audio:
        // the external inputs are gone (the owner, 2026-09-26) and the sound is named for itself.
        (
            "Repeat noise",
            Category::Fx,
            &[
                ("repeat_external", 1.0),
                ("osc_a_level", 0.0),
                ("noise_external_level", 0.75),
                ("cutoff", 0.7),
                ("amp_env_attack", 0.08),
                ("amp_env_sustain", 0.75),
            ],
        ),
        // Was "External clocked". With the Gate / clock input gone, the host's tempo is what clocks
        // Repeat from outside: the LFO synced to an eighth note.
        (
            "Tempo repeat",
            Category::Sequence,
            &[
                ("repeat_external", 1.0),
                ("lfo_sync", 1.0),
                (
                    "lfo_rate",
                    crate::params::LFO_SYNC.position(mxm_tempo::Division::Eighth),
                ),
                ("cutoff", 0.68),
                ("amp_env_decay", 0.15),
                ("amp_env_sustain", 0.25),
                ("mod_cutoff_fenv", 0.595),
            ],
        ),
        (
            "Signal laboratory",
            Category::Template,
            &[
                ("osc_a_pulse", 1.0),
                ("osc_b_saw", 1.0),
                ("osc_b_triangle", 1.0),
                ("osc_b_level", 0.52),
                ("lfo_saw", 1.0),
                ("lfo_triangle", 1.0),
                ("cutoff", 0.62),
                ("resonance", 0.28),
                ("mod_afreq_lfo", 0.55),
                ("mod_afreq_fenvon", 1.0),
                ("mod_afreq_fenv", 0.61),
                ("mod_cutoff_lfo", 0.55),
                ("mod_cutoff_fenv", 0.605),
            ],
        ),
    ];

    fn params() -> MxmMonoPr1Params {
        MxmMonoPr1Params::default()
    }

    fn generated(
        params: &MxmMonoPr1Params,
        name: &str,
        category: Category,
        overrides: &[(&str, f32)],
    ) -> Preset {
        let parameters = params.all_parameters();
        let mut preset = Preset::init(params);
        preset.name = name.to_owned();
        preset.category = category;
        for (id, value) in overrides {
            let (_, parameter) = parameters
                .iter()
                .find(|(candidate, _)| candidate == id)
                .unwrap_or_else(|| panic!("{name:?} names unknown parameter `{id}`"));
            preset.params.insert(
                (*id).to_owned(),
                Value {
                    v: *value,
                    text: parameter.format(*value),
                },
            );
        }
        preset
    }

    #[test]
    #[ignore = "writes generated factory preset files"]
    fn write_the_factory_presets() {
        let params = params();
        std::fs::create_dir_all(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets"))
            .unwrap();
        for (name, category, overrides) in FACTORY_DESIGN {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("presets")
                .join(format!("{}.json", name.to_lowercase().replace(' ', "-")));
            std::fs::write(
                path,
                generated(&params, name, *category, overrides).to_json(),
            )
            .unwrap();
        }
    }

    #[test]
    fn exactly_fifty_designed_factory_sounds() {
        assert_eq!(FACTORY_DESIGN.len(), 50);
        assert_eq!(FACTORY_FILES.len(), 50);
    }

    #[test]
    fn every_design_names_real_normalized_parameters() {
        let params = params();
        let parameters = params.all_parameters();
        for (name, _, overrides) in FACTORY_DESIGN {
            for (id, value) in *overrides {
                assert!(
                    parameters.iter().any(|(candidate, _)| candidate == id),
                    "{name}: {id}"
                );
                assert!((0.0..=1.0).contains(value), "{name}: {id}={value}");
            }
        }
    }

    #[test]
    fn shipped_files_are_exactly_the_readable_design() {
        let params = params();
        for (name, category, overrides) in FACTORY_DESIGN {
            let text = FACTORY_FILES
                .iter()
                .find(|(candidate, _)| candidate == name)
                .unwrap()
                .1;
            assert_eq!(
                Preset::parse(text, crate::CLAP_ID).unwrap(),
                generated(&params, name, *category, overrides),
                "regenerate {name}"
            );
        }
    }

    #[test]
    fn every_file_is_complete_distinct_categorised_and_owned() {
        let params = params();
        let mut seen = Vec::new();
        for (name, text) in FACTORY_FILES {
            let preset = Preset::parse(text, crate::CLAP_ID)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(&preset.name, name);
            assert_ne!(preset.category, Category::Uncategorised);
            assert!(preset.resolve(&params).1.is_empty(), "{name} is incomplete");
            assert!(
                !seen.iter().any(|old| old == &preset.params),
                "duplicate {name}"
            );
            seen.push(preset.params);
        }
    }

    #[test]
    fn every_factory_sound_renders_finite_audible_output() {
        for (name, text) in FACTORY_FILES {
            let params = params();
            let preset = Preset::parse(text, crate::CLAP_ID).unwrap();
            for (id, pointer, _) in params.param_map() {
                unsafe {
                    pointer._internal_set_normalized_value(preset.params[&id].v);
                    pointer._internal_update_smoother(48_000.0, true);
                }
            }
            let mut voice = Voice::new();
            voice.set_sample_rate(48_000.0);
            // **Arm the topology from the sound being rendered**, not from Init. A render that
            // skipped this would sound like the init patch's routing whatever the preset says, and
            // would pass while proving nothing.
            voice.set_topology(&params.routes.topology());
            // **And advance the amounts, which the topology alone does not.** `set_topology` says
            // which pairs are live; every depth stays at whatever the voice was built with, so a
            // render that stopped at the line above played all fifty sounds with **every route at
            // zero** — present, and contributing nothing. It passed, because a saw through an open
            // filter is audible whatever its modulation does. This is the plugin's own per-sample
            // call, made here for the same reason it is made there.
            let note = Event::NoteOn(NoteId::keyed(None, 0, 48));
            let mut peak = 0.0f32;
            for sample in 0..24_000 {
                let events = if sample == 0 { &[note][..] } else { &[] };
                params.routes.advance_into(voice.routing_mut());
                let frame = voice.process_sample(params.next_voice_params(), events);
                assert!(
                    frame.output.is_finite() && frame.output.abs() <= 1.0,
                    "{name}"
                );
                peak = peak.max(frame.output.abs());
            }
            assert!(peak > 1e-4, "{name} is inaudible under its intended note");
        }
    }

    /// Renders `text`'s parameters — or Init with `None` — through the plugin's own per-sample
    /// path, one note held, as `every_factory_sound_renders_finite_audible_output` does.
    fn render(text: Option<&str>) -> Vec<f32> {
        let params = params();
        if let Some(text) = text {
            let preset = Preset::parse(text, crate::CLAP_ID).unwrap();
            for (id, pointer, _) in params.param_map() {
                unsafe {
                    pointer._internal_set_normalized_value(preset.params[&id].v);
                    pointer._internal_update_smoother(48_000.0, true);
                }
            }
        }
        let mut voice = Voice::new();
        voice.set_sample_rate(48_000.0);
        voice.set_topology(&params.routes.topology());
        let note = Event::NoteOn(NoteId::keyed(None, 0, 48));
        (0..24_000)
            .map(|sample| {
                let events = if sample == 0 { &[note][..] } else { &[] };
                params.routes.advance_into(voice.routing_mut());
                voice
                    .process_sample(params.next_voice_params(), events)
                    .output
            })
            .collect()
    }

    /// **Init and every factory sound, fingerprinted**, so a change that should not move the sound
    /// can prove it did not: run it before and after and compare the lines.
    ///
    /// ```text
    /// cargo test --release -p mxm-mono-pr1 --lib the_bank_digests -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "prints the bank's fingerprints for a before-and-after comparison"]
    fn the_bank_digests() {
        let digest = |samples: &[f32]| {
            let mut hash = 0xcbf2_9ce4_8422_2325_u64;
            for sample in samples {
                for byte in sample.to_bits().to_le_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(0x100_0000_01b3);
                }
            }
            format!("{hash:016x}")
        };
        println!("  | `init` | `{}` |", digest(&render(None)));
        for (name, text) in FACTORY_FILES {
            println!("  | `{name}` | `{}` |", digest(&render(Some(text))));
        }
    }

    #[test]
    fn the_set_covers_the_distinct_voice_mechanisms() {
        let names: Vec<_> = FACTORY_DESIGN.iter().map(|(name, _, _)| *name).collect();
        for required in [
            "Triple B stack",
            "Hard sync lead",
            "Hidden sync",
            "Sync pulse dropout",
            "B low sweep",
            "B self bend",
            "Dual envelope sweep",
            "Mixed buses",
            "Normal glide",
            "Auto glide",
            "Retrigger lead",
            "Repeat bare clock",
            "Sustain drone",
            "Repeat noise",
            "Tempo repeat",
        ] {
            assert!(
                names.contains(&required),
                "missing mechanism sound {required}"
            );
        }
    }

    #[test]
    fn init_is_every_parameter_default_and_factory_begins_with_it() {
        let params = params();
        let init = Preset::init(&params);
        assert_eq!(init.params.len(), params.all_parameters().len());
        for (id, parameter) in params.all_parameters() {
            assert_eq!(init.params[id].v, parameter.default_normalised(), "{id}");
        }
        let all = factory(&params);
        assert_eq!(all[0].name, INIT_NAME);
        assert_eq!(all.len(), 51);
    }

    #[test]
    fn factory_content_does_not_move_master_volume() {
        let params = params();
        let default = params.volume.default_normalized_value();
        for (name, text) in FACTORY_FILES {
            let preset = Preset::parse(text, crate::CLAP_ID).unwrap();
            assert_eq!(preset.params["volume"].v, default, "{name}");
        }
    }

    #[test]
    fn user_files_are_namespaced_to_the_permanent_id() {
        if let Some(root) = user_root(crate::CLAP_ID) {
            assert!(root.to_string_lossy().contains(crate::CLAP_ID));
        }
    }
}
