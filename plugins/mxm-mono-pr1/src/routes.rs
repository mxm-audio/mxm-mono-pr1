//! mxm-mono-pr1's routing parameters: one presence and one amount per *(target, source)* pair.
//!
//! `plan-mxm-mono-pr1-modulation.md` §2 and §4. The derive needs concrete fields and this
//! instrument's source list is its own, so the struct is declared here rather than generated — the
//! shape `mxm-mono-08` established for its 120 routing ids and the pilot follows for its 88. What is
//! shared is everything around these fields: [`mxm_modulation_params`] reads them, and
//! [`mxm_modulation`] evaluates them.
//!
//! # Permanent ids
//!
//! One `#[nested(id_prefix = …)]` per target, so a pair's id is `<target>_<source>` and
//! `<target>_<source>on`. Mechanically derived, never typed. **Permanent from here on.**
//!
//! # What retired into this file
//!
//! Fifteen ids. The eleven the Direct/Wheel buses named — three source amounts, two bus
//! assignments and five destination switches — plus `filter_env_amount` and
//! `filter_keyboard_amount`, the two hard-wired cutoff paths, plus `mod_modbus_wheel` and
//! `mod_modbus_wheelon`, which went when the wheel stopped being an additive source on the bus it
//! multiplies (see [`BusRoutes`]). None of them could be re-used:
//! `lfo_amount` was *one depth shared across every destination on a bus*, where a route amount is
//! per target, and the destination enums named `Direct / Off / Wheel`, which is a stored-value
//! hazard to re-letter and names nothing once the buses are gone. `plan-modulation-routing.md`
//! decision 1.13 permits the retirement because every sound stays reachable, and
//! `crates/mxm-mono-pr1-dsp/tests/legacy_reachability.rs` is where that is demonstrated rather than
//! asserted.

use mxm_modulation_params::Route;
use mxm_modulation_params::reading::{self, Fader, Reach};
use mxm_mono_pr1_dsp::routing::{
    Routing, SOURCE_NAMES, SOURCES, TARGET_NAMES, TARGETS, init_amount, init_present, offer,
};
use nice_plug::prelude::*;

/// Every routing pair's two permanent ids, `(amount, presence)`, in `[target][source]` order.
///
/// **`Mod bus` has one fewer than the rest**: the wheel is not a source on it. See [`BusRoutes`].
///
/// **Written out rather than derived at runtime**, because a preset's parameter list is
/// `&'static str` and because these are permanent ids: they belong in the source where they can be
/// read, grepped and diffed. They are still *produced* by the `#[nested(id_prefix = …)]` groups
/// below — this table only names what the derive emits, and
/// `tests::the_id_table_is_what_the_derive_actually_produces` is what holds the two together.
pub const ROUTE_IDS: [&[(&str, &str)]; TARGETS] = [
    &[
        ("mod_afreq_key", "mod_afreq_keyon"),
        ("mod_afreq_vel", "mod_afreq_velon"),
        ("mod_afreq_wheel", "mod_afreq_wheelon"),
        ("mod_afreq_press", "mod_afreq_presson"),
        ("mod_afreq_bend", "mod_afreq_bendon"),
        ("mod_afreq_noise", "mod_afreq_noiseon"),
        ("mod_afreq_lfo", "mod_afreq_lfoon"),
        ("mod_afreq_gate", "mod_afreq_gateon"),
        ("mod_afreq_fenv", "mod_afreq_fenvon"),
        ("mod_afreq_aenv", "mod_afreq_aenvon"),
        ("mod_afreq_bus", "mod_afreq_buson"),
        ("mod_afreq_wbus", "mod_afreq_wbuson"),
        ("mod_afreq_oscb", "mod_afreq_oscbon"),
        ("mod_afreq_osca", "mod_afreq_oscaon"),
    ],
    &[
        ("mod_apw_key", "mod_apw_keyon"),
        ("mod_apw_vel", "mod_apw_velon"),
        ("mod_apw_wheel", "mod_apw_wheelon"),
        ("mod_apw_press", "mod_apw_presson"),
        ("mod_apw_bend", "mod_apw_bendon"),
        ("mod_apw_noise", "mod_apw_noiseon"),
        ("mod_apw_lfo", "mod_apw_lfoon"),
        ("mod_apw_gate", "mod_apw_gateon"),
        ("mod_apw_fenv", "mod_apw_fenvon"),
        ("mod_apw_aenv", "mod_apw_aenvon"),
        ("mod_apw_bus", "mod_apw_buson"),
        ("mod_apw_wbus", "mod_apw_wbuson"),
        ("mod_apw_oscb", "mod_apw_oscbon"),
        ("mod_apw_osca", "mod_apw_oscaon"),
    ],
    &[
        ("mod_bfreq_key", "mod_bfreq_keyon"),
        ("mod_bfreq_vel", "mod_bfreq_velon"),
        ("mod_bfreq_wheel", "mod_bfreq_wheelon"),
        ("mod_bfreq_press", "mod_bfreq_presson"),
        ("mod_bfreq_bend", "mod_bfreq_bendon"),
        ("mod_bfreq_noise", "mod_bfreq_noiseon"),
        ("mod_bfreq_lfo", "mod_bfreq_lfoon"),
        ("mod_bfreq_gate", "mod_bfreq_gateon"),
        ("mod_bfreq_fenv", "mod_bfreq_fenvon"),
        ("mod_bfreq_aenv", "mod_bfreq_aenvon"),
        ("mod_bfreq_bus", "mod_bfreq_buson"),
        ("mod_bfreq_wbus", "mod_bfreq_wbuson"),
        ("mod_bfreq_oscb", "mod_bfreq_oscbon"),
        ("mod_bfreq_osca", "mod_bfreq_oscaon"),
    ],
    &[
        ("mod_bpw_key", "mod_bpw_keyon"),
        ("mod_bpw_vel", "mod_bpw_velon"),
        ("mod_bpw_wheel", "mod_bpw_wheelon"),
        ("mod_bpw_press", "mod_bpw_presson"),
        ("mod_bpw_bend", "mod_bpw_bendon"),
        ("mod_bpw_noise", "mod_bpw_noiseon"),
        ("mod_bpw_lfo", "mod_bpw_lfoon"),
        ("mod_bpw_gate", "mod_bpw_gateon"),
        ("mod_bpw_fenv", "mod_bpw_fenvon"),
        ("mod_bpw_aenv", "mod_bpw_aenvon"),
        ("mod_bpw_bus", "mod_bpw_buson"),
        ("mod_bpw_wbus", "mod_bpw_wbuson"),
        ("mod_bpw_oscb", "mod_bpw_oscbon"),
        ("mod_bpw_osca", "mod_bpw_oscaon"),
    ],
    &[
        ("mod_cutoff_key", "mod_cutoff_keyon"),
        ("mod_cutoff_vel", "mod_cutoff_velon"),
        ("mod_cutoff_wheel", "mod_cutoff_wheelon"),
        ("mod_cutoff_press", "mod_cutoff_presson"),
        ("mod_cutoff_bend", "mod_cutoff_bendon"),
        ("mod_cutoff_noise", "mod_cutoff_noiseon"),
        ("mod_cutoff_lfo", "mod_cutoff_lfoon"),
        ("mod_cutoff_gate", "mod_cutoff_gateon"),
        ("mod_cutoff_fenv", "mod_cutoff_fenvon"),
        ("mod_cutoff_aenv", "mod_cutoff_aenvon"),
        ("mod_cutoff_bus", "mod_cutoff_buson"),
        ("mod_cutoff_wbus", "mod_cutoff_wbuson"),
        ("mod_cutoff_oscb", "mod_cutoff_oscbon"),
        ("mod_cutoff_osca", "mod_cutoff_oscaon"),
    ],
    &[
        ("mod_reso_key", "mod_reso_keyon"),
        ("mod_reso_vel", "mod_reso_velon"),
        ("mod_reso_wheel", "mod_reso_wheelon"),
        ("mod_reso_press", "mod_reso_presson"),
        ("mod_reso_bend", "mod_reso_bendon"),
        ("mod_reso_noise", "mod_reso_noiseon"),
        ("mod_reso_lfo", "mod_reso_lfoon"),
        ("mod_reso_gate", "mod_reso_gateon"),
        ("mod_reso_fenv", "mod_reso_fenvon"),
        ("mod_reso_aenv", "mod_reso_aenvon"),
        ("mod_reso_bus", "mod_reso_buson"),
        ("mod_reso_wbus", "mod_reso_wbuson"),
        ("mod_reso_oscb", "mod_reso_oscbon"),
        ("mod_reso_osca", "mod_reso_oscaon"),
    ],
    &[
        ("mod_lforate_key", "mod_lforate_keyon"),
        ("mod_lforate_vel", "mod_lforate_velon"),
        ("mod_lforate_wheel", "mod_lforate_wheelon"),
        ("mod_lforate_press", "mod_lforate_presson"),
        ("mod_lforate_bend", "mod_lforate_bendon"),
        ("mod_lforate_noise", "mod_lforate_noiseon"),
        ("mod_lforate_lfo", "mod_lforate_lfoon"),
        ("mod_lforate_gate", "mod_lforate_gateon"),
        ("mod_lforate_fenv", "mod_lforate_fenvon"),
        ("mod_lforate_aenv", "mod_lforate_aenvon"),
        ("mod_lforate_bus", "mod_lforate_buson"),
        ("mod_lforate_wbus", "mod_lforate_wbuson"),
        ("mod_lforate_oscb", "mod_lforate_oscbon"),
        ("mod_lforate_osca", "mod_lforate_oscaon"),
    ],
    &[
        ("mod_modbus_key", "mod_modbus_keyon"),
        ("mod_modbus_vel", "mod_modbus_velon"),
        ("mod_modbus_press", "mod_modbus_presson"),
        ("mod_modbus_bend", "mod_modbus_bendon"),
        ("mod_modbus_noise", "mod_modbus_noiseon"),
        ("mod_modbus_lfo", "mod_modbus_lfoon"),
        ("mod_modbus_gate", "mod_modbus_gateon"),
        ("mod_modbus_fenv", "mod_modbus_fenvon"),
        ("mod_modbus_aenv", "mod_modbus_aenvon"),
        ("mod_modbus_bus", "mod_modbus_buson"),
        ("mod_modbus_wbus", "mod_modbus_wbuson"),
        ("mod_modbus_oscb", "mod_modbus_oscbon"),
        ("mod_modbus_osca", "mod_modbus_oscaon"),
    ],
    &[
        ("mod_wbus_key", "mod_wbus_keyon"),
        ("mod_wbus_vel", "mod_wbus_velon"),
        ("mod_wbus_wheel", "mod_wbus_wheelon"),
        ("mod_wbus_press", "mod_wbus_presson"),
        ("mod_wbus_bend", "mod_wbus_bendon"),
        ("mod_wbus_noise", "mod_wbus_noiseon"),
        ("mod_wbus_lfo", "mod_wbus_lfoon"),
        ("mod_wbus_gate", "mod_wbus_gateon"),
        ("mod_wbus_fenv", "mod_wbus_fenvon"),
        ("mod_wbus_aenv", "mod_wbus_aenvon"),
        ("mod_wbus_bus", "mod_wbus_buson"),
        ("mod_wbus_wbus", "mod_wbus_wbuson"),
        ("mod_wbus_oscb", "mod_wbus_oscbon"),
        ("mod_wbus_osca", "mod_wbus_oscaon"),
    ],
    &[
        ("mod_amp_key", "mod_amp_keyon"),
        ("mod_amp_vel", "mod_amp_velon"),
        ("mod_amp_wheel", "mod_amp_wheelon"),
        ("mod_amp_press", "mod_amp_presson"),
        ("mod_amp_bend", "mod_amp_bendon"),
        ("mod_amp_noise", "mod_amp_noiseon"),
        ("mod_amp_lfo", "mod_amp_lfoon"),
        ("mod_amp_gate", "mod_amp_gateon"),
        ("mod_amp_fenv", "mod_amp_fenvon"),
        ("mod_amp_aenv", "mod_amp_aenvon"),
        ("mod_amp_bus", "mod_amp_buson"),
        ("mod_amp_wbus", "mod_amp_wbuson"),
        ("mod_amp_oscb", "mod_amp_oscbon"),
        ("mod_amp_osca", "mod_amp_oscaon"),
    ],
];

/// One target's routes: a presence and a signed amount for every source the instrument declares.
///
/// **Presence is the enable and the amount is the depth**, and nothing else. No selector, because a
/// pair *is* its source; no polarity switch, because the amount is signed; no "via" modifier,
/// because a gesture scaling a route is what the wheel stage does and it is fixed wiring.
#[derive(Params)]
pub struct TargetRoutes {
    #[id = "keyon"]
    pub key_on: BoolParam,
    #[id = "key"]
    pub key: FloatParam,
    #[id = "velon"]
    pub vel_on: BoolParam,
    #[id = "vel"]
    pub vel: FloatParam,
    #[id = "wheelon"]
    pub wheel_on: BoolParam,
    #[id = "wheel"]
    pub wheel: FloatParam,
    #[id = "presson"]
    pub press_on: BoolParam,
    #[id = "press"]
    pub press: FloatParam,
    #[id = "bendon"]
    pub bend_on: BoolParam,
    #[id = "bend"]
    pub bend: FloatParam,
    #[id = "noiseon"]
    pub noise_on: BoolParam,
    #[id = "noise"]
    pub noise: FloatParam,
    #[id = "lfoon"]
    pub lfo_on: BoolParam,
    #[id = "lfo"]
    pub lfo: FloatParam,
    #[id = "gateon"]
    pub gate_on: BoolParam,
    #[id = "gate"]
    pub gate: FloatParam,
    #[id = "fenvon"]
    pub fenv_on: BoolParam,
    #[id = "fenv"]
    pub fenv: FloatParam,
    #[id = "aenvon"]
    pub aenv_on: BoolParam,
    #[id = "aenv"]
    pub aenv: FloatParam,
    #[id = "buson"]
    pub bus_on: BoolParam,
    #[id = "bus"]
    pub bus: FloatParam,
    #[id = "wbuson"]
    pub wbus_on: BoolParam,
    #[id = "wbus"]
    pub wbus: FloatParam,
    #[id = "oscbon"]
    pub oscb_on: BoolParam,
    #[id = "oscb"]
    pub oscb: FloatParam,
    #[id = "oscaon"]
    pub osca_on: BoolParam,
    #[id = "osca"]
    pub osca: FloatParam,
}

/// What one full-amount route reaches on a given target, **in the target's own unit**.
///
/// `plan-modulation-routing.md` decision 1.11: *"a route amount reads in the target's own unit, not
/// as a percent — Cutoff from LFO −2.4 oct."* It is a deliberate departure from every other control
/// in this collection, which reads as a percentage, and **this is the instrument that needs it**:
/// cutoff is the one target whose scale column is not uniform, so two rows under it at the same
/// percentage mean different numbers of octaves. A percentage would have said they were the same.
///
/// The reach comes from the DSP's own tables, so a scale that moves moves the reading with it.
fn reach(target: usize, source: usize) -> Reach {
    use mxm_mono_pr1_dsp::routing::{
        ADDED_SCALE, AMPLITUDE_SCALE, CUTOFF_SCALE, FRAME_SCALE, UNIFORM_SCALE, source as s,
        target as t,
    };
    let unit = match target {
        t::OSCILLATOR_A_FREQUENCY | t::OSCILLATOR_B_FREQUENCY => reading::SEMITONES,
        t::FILTER_CUTOFF | t::LFO_RATE => reading::OCTAVES,
        // A width offset and the resonance control's own range are fractions of a span with no
        // name a player would recognise, and the module publishes in frame units. A percentage is
        // the honest reading for those, and for Amplitude's share of the level.
        _ => reading::PERCENT,
    };
    // **In the destination's own domain, so the frame unit is divided back out.** `sum_uniform`
    // applies `FRAME_SCALE * scale` to a value the frame already scaled *down* by the same factor;
    // `sum_scaled` carries both in its column. What a player reads is what a **unit-magnitude**
    // source delivers, which is the scale with the frame unit removed — otherwise the cutoff's
    // collapsed envelope route would read `20.48 oct` where it delivers `2.56`.
    //
    // **A percentage carries the scale too.** Pulse width moves by `PWM_MOD_DEPTH`, not by one, so a
    // full-amount route is `50 %` of the width's range rather than `100 %`; reading it as `100 %`
    // would mean a player typing `50 %` got half of what they asked for.
    let full = match target {
        // The summing module publishes a normalised bus level, so full amount *is* a full bus.
        t::MOD_BUS => return Reach::new(1.0, unit),
        // **The multiplier's amount is not a reach at all**, so it cannot borrow a scale. It blends
        // the factor between the law's neutral and the source's top — `1 + amount × (source − top)`
        // — so full amount means *this factor applies in full* and reads `+100 %`. Its
        // `UNIFORM_SCALE` entry is `None` like cutoff's, and falling through to cutoff's per-source
        // column is what made a shipped factor read `+800 %`.
        t::WHEEL_BUS => return Reach::new(1.0, unit),
        t::AMPLITUDE => AMPLITUDE_SCALE[source] / FRAME_SCALE,
        _ => match (ADDED_SCALE[target][source], UNIFORM_SCALE[target]) {
            // An added pair's standard reach, in the split sum's added half.
            (Some(added), _) => added / FRAME_SCALE,
            (None, Some(scale)) => scale,
            (None, None) => CUTOFF_SCALE[source] / FRAME_SCALE,
        },
    };
    // The key source is in octaves of keyboard, so its full scale is already per octave.
    if source == s::KEY {
        Reach::per_octave(full, unit)
    } else {
        Reach::new(full, unit)
    }
}

/// A route amount: signed, centred on zero, and **an amount, so it starts there**.
///
/// Smoothed, because it multiplies a signal — `plugins/AGENTS.md`'s *smooth signals, not
/// coefficients*. Drawn bipolar, because its centre is *no modulation* and a half-filled track would
/// read as "half on" when it means "off". **Read in the target's own unit** — see [`reach`].
fn amount(target: usize, source: usize, name: String) -> FloatParam {
    // **Almost always zero.** The exceptions: the multiplier's own two factors at full — `product`'s
    // neutral is one, so a factor at zero depth would make the module publish a constant whatever
    // its sources do — and the wheel's vibrato (`INIT_WHEEL`). `init_amount` is the one statement of
    // both, and `plugins/mxm-mono-pr1/AGENTS.md` records them.
    let default = init_amount(target, source);
    // The collection's one route parameter (`mxm_modulation_params::reading`): what is displayed
    // is always what the route delivers, on the travel the pair's offer allows.
    reading::amount_param_at(
        name,
        default,
        reach(target, source),
        Fader::for_offer(offer(target, source), false),
        15.0,
    )
}

/// Whether a route exists. **Configuration, not an amount**, so its default is the machine's own
/// wiring plus the three pairs the control map claims as collection roles.
///
/// A controller knob bound to an absent route does nothing at all, because an absent pair
/// contributes nothing whatever its amount holds — so claiming a role without wiring its route
/// would ship a dead knob. It is safe because the depth beside it is an **amount** and starts at
/// zero: a revealed route at zero depth is audible as nothing.
fn present(target: &str, source: &str, wired: bool) -> BoolParam {
    BoolParam::new(format!("{target} from {source} on"), wired)
}

impl TargetRoutes {
    /// Every pair for one target, at the init patch: the wired routes present, the rest absent, and
    /// **every one of them at zero depth**.
    pub fn new(index: usize) -> Self {
        let n = SOURCE_NAMES;
        let wired = |s: usize| init_present(index, s);
        let target = TARGET_NAMES[index];
        Self {
            key_on: present(target, n[0], wired(0)),
            key: amount(index, 0, format!("{target} from {}", n[0])),
            vel_on: present(target, n[1], wired(1)),
            vel: amount(index, 1, format!("{target} from {}", n[1])),
            wheel_on: present(target, n[2], wired(2)),
            wheel: amount(index, 2, format!("{target} from {}", n[2])),
            press_on: present(target, n[3], wired(3)),
            press: amount(index, 3, format!("{target} from {}", n[3])),
            bend_on: present(target, n[4], wired(4)),
            bend: amount(index, 4, format!("{target} from {}", n[4])),
            noise_on: present(target, n[5], wired(5)),
            noise: amount(index, 5, format!("{target} from {}", n[5])),
            lfo_on: present(target, n[6], wired(6)),
            lfo: amount(index, 6, format!("{target} from {}", n[6])),
            gate_on: present(target, n[7], wired(7)),
            gate: amount(index, 7, format!("{target} from {}", n[7])),
            fenv_on: present(target, n[8], wired(8)),
            fenv: amount(index, 8, format!("{target} from {}", n[8])),
            aenv_on: present(target, n[9], wired(9)),
            aenv: amount(index, 9, format!("{target} from {}", n[9])),
            bus_on: present(target, n[10], wired(10)),
            bus: amount(index, 10, format!("{target} from {}", n[10])),
            wbus_on: present(target, n[11], wired(11)),
            wbus: amount(index, 11, format!("{target} from {}", n[11])),
            oscb_on: present(target, n[12], wired(12)),
            oscb: amount(index, 12, format!("{target} from {}", n[12])),
            osca_on: present(target, n[13], wired(13)),
            osca: amount(index, 13, format!("{target} from {}", n[13])),
        }
    }

    /// The pairs in **declared source order**, which is the order the frame and the interface use.
    ///
    /// `target` is its index, needed because each row's keyboard scope is its parameter's permanent
    /// id and those live in [`ROUTE_IDS`], keyed by target.
    pub fn routes(&self, target: usize) -> [Route<'_>; SOURCES] {
        [
            Route {
                source: SOURCE_NAMES[0],
                present: &self.key_on,
                amount: &self.key,
                present_id: ROUTE_IDS[target][0].1,
                amount_id: ROUTE_IDS[target][0].0,
            },
            Route {
                source: SOURCE_NAMES[1],
                present: &self.vel_on,
                amount: &self.vel,
                present_id: ROUTE_IDS[target][1].1,
                amount_id: ROUTE_IDS[target][1].0,
            },
            Route {
                source: SOURCE_NAMES[2],
                present: &self.wheel_on,
                amount: &self.wheel,
                present_id: ROUTE_IDS[target][2].1,
                amount_id: ROUTE_IDS[target][2].0,
            },
            Route {
                source: SOURCE_NAMES[3],
                present: &self.press_on,
                amount: &self.press,
                present_id: ROUTE_IDS[target][3].1,
                amount_id: ROUTE_IDS[target][3].0,
            },
            Route {
                source: SOURCE_NAMES[4],
                present: &self.bend_on,
                amount: &self.bend,
                present_id: ROUTE_IDS[target][4].1,
                amount_id: ROUTE_IDS[target][4].0,
            },
            Route {
                source: SOURCE_NAMES[5],
                present: &self.noise_on,
                amount: &self.noise,
                present_id: ROUTE_IDS[target][5].1,
                amount_id: ROUTE_IDS[target][5].0,
            },
            Route {
                source: SOURCE_NAMES[6],
                present: &self.lfo_on,
                amount: &self.lfo,
                present_id: ROUTE_IDS[target][6].1,
                amount_id: ROUTE_IDS[target][6].0,
            },
            Route {
                source: SOURCE_NAMES[7],
                present: &self.gate_on,
                amount: &self.gate,
                present_id: ROUTE_IDS[target][7].1,
                amount_id: ROUTE_IDS[target][7].0,
            },
            Route {
                source: SOURCE_NAMES[8],
                present: &self.fenv_on,
                amount: &self.fenv,
                present_id: ROUTE_IDS[target][8].1,
                amount_id: ROUTE_IDS[target][8].0,
            },
            Route {
                source: SOURCE_NAMES[9],
                present: &self.aenv_on,
                amount: &self.aenv,
                present_id: ROUTE_IDS[target][9].1,
                amount_id: ROUTE_IDS[target][9].0,
            },
            Route {
                source: SOURCE_NAMES[10],
                present: &self.bus_on,
                amount: &self.bus,
                present_id: ROUTE_IDS[target][10].1,
                amount_id: ROUTE_IDS[target][10].0,
            },
            Route {
                source: SOURCE_NAMES[11],
                present: &self.wbus_on,
                amount: &self.wbus,
                present_id: ROUTE_IDS[target][11].1,
                amount_id: ROUTE_IDS[target][11].0,
            },
            Route {
                source: SOURCE_NAMES[12],
                present: &self.oscb_on,
                amount: &self.oscb,
                present_id: ROUTE_IDS[target][12].1,
                amount_id: ROUTE_IDS[target][12].0,
            },
            Route {
                source: SOURCE_NAMES[13],
                present: &self.osca_on,
                amount: &self.osca,
                present_id: ROUTE_IDS[target][13].1,
                amount_id: ROUTE_IDS[target][13].0,
            },
        ]
    }

    /// Whether each of this target's routes exists. Read **once per interval**, never per sample.
    pub fn presences(&self, target: usize) -> [bool; SOURCES] {
        mxm_modulation_params::presences(&self.routes(target))
    }

    /// This target's presence parameters, in declared source order.
    ///
    /// **For revealing every row in a test.** An absent route draws nothing at all — that is the
    /// interface's own rule — so a check that only ever paints the init patch covers seven of 125
    /// pairs and silently passes over the other 118.
    pub fn presence_params(&self) -> [&BoolParam; SOURCES] {
        [
            &self.key_on,
            &self.vel_on,
            &self.wheel_on,
            &self.press_on,
            &self.bend_on,
            &self.noise_on,
            &self.lfo_on,
            &self.gate_on,
            &self.fenv_on,
            &self.aenv_on,
            &self.bus_on,
            &self.wbus_on,
            &self.oscb_on,
            &self.osca_on,
        ]
    }

    /// One source's amount parameter, by index, in **declared source order**.
    ///
    /// A `match` rather than an array of references, and the reason is the cost gate: building a
    /// fourteen-element array of `&FloatParam` would cost 125 pointer stores per sample across the
    /// nine targets, for the handful of routes that are actually live. This is branched past for a
    /// source nothing reads.
    #[inline]
    fn amount_param(&self, source: usize) -> &FloatParam {
        match source {
            0 => &self.key,
            1 => &self.vel,
            2 => &self.wheel,
            3 => &self.press,
            4 => &self.bend,
            5 => &self.noise,
            6 => &self.lfo,
            7 => &self.gate,
            8 => &self.fenv,
            9 => &self.aenv,
            10 => &self.bus,
            11 => &self.wbus,
            12 => &self.oscb,
            _ => &self.osca,
        }
    }

    /// Snaps a newly present route's smoother to its stored value.
    ///
    /// **An absent route's smoother is not advanced, so it must not be resumed either.** While the
    /// pair was absent nothing called `next()`, but the parameter itself stayed editable: a host
    /// automating it, or a preset load, moves the *target* and leaves the smoother's current value
    /// wherever the last live sample left it. Resuming from there ramps the route in from a stale
    /// number over a span that depends on how long it was absent — which is exactly the
    /// accumulation across skipped spans `plan-modulation-routing.md` §6.2 forbids, because it
    /// makes a route's first audible value depend on the host's buffer sizes.
    ///
    /// Called from [`Routes::set_topology`] for the pairs that just became present, so a re-added
    /// route arrives at the depth the player last set rather than sliding up to it.
    pub fn arm(&self, newly_present: &[bool; SOURCES]) {
        for (source, &now) in newly_present.iter().enumerate() {
            if now {
                let param = self.amount_param(source);
                param.smoothed.reset(param.value());
            }
        }
    }

    /// Fills this target's amounts for the sample about to be rendered, as **fractions of each
    /// route's scale** — the scale itself is applied by the sum, not here.
    ///
    /// `(amount × source) × scale` is the instruction sequence this voice executed before it had
    /// routing, and folding the scale in here instead would move every destination's digest for one
    /// saved instruction.
    ///
    /// **Only a live route's smoother is advanced**, which is the whole of the efficiency design: an
    /// absent pair costs nothing per sample beyond the branch that skips it, and its stored depth is
    /// left exactly where the player put it so re-adding the source restores it.
    #[inline]
    pub fn advance_into(&self, live: &[bool; SOURCES], out: &mut [f32; SOURCES]) {
        for (source, &on) in live.iter().enumerate() {
            if on {
                out[source] = self.amount_param(source).smoothed.next();
            }
        }
    }
}

/// The summing module's routes: **every source except the wheel**.
///
/// The wheel is the one control that already has a relationship with this bus — it **multiplies**
/// it, which is what `Wheel bus` is. Offering it additively into the same stage is the same control
/// wired two ways into one place, and the only thing it can produce is a transposing DC offset
/// sitting on top of whatever else is in the bus rather than controlling that thing's depth. The
/// owner found exactly that on 2026-09-14: *"the wheel should change the level of vibrato to the
/// vco, so you can do vibrato fade in and out while performing."*
///
/// That gesture is what the bus is **for**: put the LFO in here, route `Wheel bus` at the pitch, and
/// the wheel fades the vibrato in. Routing the raw wheel additively is still available at every one
/// of the seven ordinary targets, so nothing goes out of reach.
///
/// `mod_modbus_wheel` and `mod_modbus_wheelon` retire with it — two more permanent ids, on decision
/// 1.13's ground that a control may go when the sound stays reachable.
#[derive(Params)]
pub struct BusRoutes {
    #[id = "keyon"]
    pub key_on: BoolParam,
    #[id = "key"]
    pub key: FloatParam,
    #[id = "velon"]
    pub vel_on: BoolParam,
    #[id = "vel"]
    pub vel: FloatParam,
    #[id = "presson"]
    pub press_on: BoolParam,
    #[id = "press"]
    pub press: FloatParam,
    #[id = "bendon"]
    pub bend_on: BoolParam,
    #[id = "bend"]
    pub bend: FloatParam,
    #[id = "noiseon"]
    pub noise_on: BoolParam,
    #[id = "noise"]
    pub noise: FloatParam,
    #[id = "lfoon"]
    pub lfo_on: BoolParam,
    #[id = "lfo"]
    pub lfo: FloatParam,
    #[id = "gateon"]
    pub gate_on: BoolParam,
    #[id = "gate"]
    pub gate: FloatParam,
    #[id = "fenvon"]
    pub fenv_on: BoolParam,
    #[id = "fenv"]
    pub fenv: FloatParam,
    #[id = "aenvon"]
    pub aenv_on: BoolParam,
    #[id = "aenv"]
    pub aenv: FloatParam,
    #[id = "buson"]
    pub bus_on: BoolParam,
    #[id = "bus"]
    pub bus: FloatParam,
    #[id = "wbuson"]
    pub wbus_on: BoolParam,
    #[id = "wbus"]
    pub wbus: FloatParam,
    #[id = "oscbon"]
    pub oscb_on: BoolParam,
    #[id = "oscb"]
    pub oscb: FloatParam,
    #[id = "oscaon"]
    pub osca_on: BoolParam,
    #[id = "osca"]
    pub osca: FloatParam,
}

impl Default for BusRoutes {
    fn default() -> Self {
        Self::new()
    }
}

impl BusRoutes {
    /// Every pair for the module, at the init patch: the LFO wired at full for the wheel's vibrato
    /// (`INIT_WHEEL`), everything else absent at zero.
    pub fn new() -> Self {
        let n = SOURCE_NAMES;
        let t = TARGET_NAMES[target::MOD_BUS];
        Self {
            key_on: present(t, n[0], init_present(target::MOD_BUS, 0)),
            key: amount(target::MOD_BUS, 0, format!("{t} from {}", n[0])),
            vel_on: present(t, n[1], init_present(target::MOD_BUS, 1)),
            vel: amount(target::MOD_BUS, 1, format!("{t} from {}", n[1])),
            press_on: present(t, n[3], init_present(target::MOD_BUS, 3)),
            press: amount(target::MOD_BUS, 3, format!("{t} from {}", n[3])),
            bend_on: present(t, n[4], init_present(target::MOD_BUS, 4)),
            bend: amount(target::MOD_BUS, 4, format!("{t} from {}", n[4])),
            noise_on: present(t, n[5], init_present(target::MOD_BUS, 5)),
            noise: amount(target::MOD_BUS, 5, format!("{t} from {}", n[5])),
            lfo_on: present(t, n[6], init_present(target::MOD_BUS, 6)),
            lfo: amount(target::MOD_BUS, 6, format!("{t} from {}", n[6])),
            gate_on: present(t, n[7], init_present(target::MOD_BUS, 7)),
            gate: amount(target::MOD_BUS, 7, format!("{t} from {}", n[7])),
            fenv_on: present(t, n[8], init_present(target::MOD_BUS, 8)),
            fenv: amount(target::MOD_BUS, 8, format!("{t} from {}", n[8])),
            aenv_on: present(t, n[9], init_present(target::MOD_BUS, 9)),
            aenv: amount(target::MOD_BUS, 9, format!("{t} from {}", n[9])),
            bus_on: present(t, n[10], init_present(target::MOD_BUS, 10)),
            bus: amount(target::MOD_BUS, 10, format!("{t} from {}", n[10])),
            wbus_on: present(t, n[11], init_present(target::MOD_BUS, 11)),
            wbus: amount(target::MOD_BUS, 11, format!("{t} from {}", n[11])),
            oscb_on: present(t, n[12], init_present(target::MOD_BUS, 12)),
            oscb: amount(target::MOD_BUS, 12, format!("{t} from {}", n[12])),
            osca_on: present(t, n[13], init_present(target::MOD_BUS, 13)),
            osca: amount(target::MOD_BUS, 13, format!("{t} from {}", n[13])),
        }
    }

    /// The pairs it does have, in declared source order. **Thirteen, not fourteen.**
    pub fn routes(&self) -> [Route<'_>; SOURCES - 1] {
        [
            Route {
                source: SOURCE_NAMES[0],
                present: &self.key_on,
                amount: &self.key,
                present_id: ROUTE_IDS[target::MOD_BUS][0].1,
                amount_id: ROUTE_IDS[target::MOD_BUS][0].0,
            },
            Route {
                source: SOURCE_NAMES[1],
                present: &self.vel_on,
                amount: &self.vel,
                present_id: ROUTE_IDS[target::MOD_BUS][1].1,
                amount_id: ROUTE_IDS[target::MOD_BUS][1].0,
            },
            Route {
                source: SOURCE_NAMES[3],
                present: &self.press_on,
                amount: &self.press,
                present_id: ROUTE_IDS[target::MOD_BUS][2].1,
                amount_id: ROUTE_IDS[target::MOD_BUS][2].0,
            },
            Route {
                source: SOURCE_NAMES[4],
                present: &self.bend_on,
                amount: &self.bend,
                present_id: ROUTE_IDS[target::MOD_BUS][3].1,
                amount_id: ROUTE_IDS[target::MOD_BUS][3].0,
            },
            Route {
                source: SOURCE_NAMES[5],
                present: &self.noise_on,
                amount: &self.noise,
                present_id: ROUTE_IDS[target::MOD_BUS][4].1,
                amount_id: ROUTE_IDS[target::MOD_BUS][4].0,
            },
            Route {
                source: SOURCE_NAMES[6],
                present: &self.lfo_on,
                amount: &self.lfo,
                present_id: ROUTE_IDS[target::MOD_BUS][5].1,
                amount_id: ROUTE_IDS[target::MOD_BUS][5].0,
            },
            Route {
                source: SOURCE_NAMES[7],
                present: &self.gate_on,
                amount: &self.gate,
                present_id: ROUTE_IDS[target::MOD_BUS][6].1,
                amount_id: ROUTE_IDS[target::MOD_BUS][6].0,
            },
            Route {
                source: SOURCE_NAMES[8],
                present: &self.fenv_on,
                amount: &self.fenv,
                present_id: ROUTE_IDS[target::MOD_BUS][7].1,
                amount_id: ROUTE_IDS[target::MOD_BUS][7].0,
            },
            Route {
                source: SOURCE_NAMES[9],
                present: &self.aenv_on,
                amount: &self.aenv,
                present_id: ROUTE_IDS[target::MOD_BUS][8].1,
                amount_id: ROUTE_IDS[target::MOD_BUS][8].0,
            },
            Route {
                source: SOURCE_NAMES[10],
                present: &self.bus_on,
                amount: &self.bus,
                present_id: ROUTE_IDS[target::MOD_BUS][9].1,
                amount_id: ROUTE_IDS[target::MOD_BUS][9].0,
            },
            Route {
                source: SOURCE_NAMES[11],
                present: &self.wbus_on,
                amount: &self.wbus,
                present_id: ROUTE_IDS[target::MOD_BUS][10].1,
                amount_id: ROUTE_IDS[target::MOD_BUS][10].0,
            },
            Route {
                source: SOURCE_NAMES[12],
                present: &self.oscb_on,
                amount: &self.oscb,
                present_id: ROUTE_IDS[target::MOD_BUS][11].1,
                amount_id: ROUTE_IDS[target::MOD_BUS][11].0,
            },
            Route {
                source: SOURCE_NAMES[13],
                present: &self.osca_on,
                amount: &self.osca,
                present_id: ROUTE_IDS[target::MOD_BUS][12].1,
                amount_id: ROUTE_IDS[target::MOD_BUS][12].0,
            },
        ]
    }

    /// Its presences, widened into the frame's `SOURCES`-shaped slot. **The wheel's slot is always
    /// false**, which is what makes the pair structurally impossible rather than merely discouraged.
    pub fn presences(&self) -> [bool; SOURCES] {
        let mut out = [false; SOURCES];
        for (route, slot) in self.routes().iter().zip(BUS_SOURCES) {
            out[slot] = route.is_present();
        }
        out
    }

    /// Its presence parameters, for revealing every row in a test.
    pub fn presence_params(&self) -> [&BoolParam; SOURCES - 1] {
        [
            &self.key_on,
            &self.vel_on,
            &self.press_on,
            &self.bend_on,
            &self.noise_on,
            &self.lfo_on,
            &self.gate_on,
            &self.fenv_on,
            &self.aenv_on,
            &self.bus_on,
            &self.wbus_on,
            &self.oscb_on,
            &self.osca_on,
        ]
    }

    /// One source's amount parameter, by **frame** index. `None` for the wheel, which has none.
    #[inline]
    fn amount_param(&self, source: usize) -> Option<&FloatParam> {
        Some(match source {
            0 => &self.key,
            1 => &self.vel,
            3 => &self.press,
            4 => &self.bend,
            5 => &self.noise,
            6 => &self.lfo,
            7 => &self.gate,
            8 => &self.fenv,
            9 => &self.aenv,
            10 => &self.bus,
            11 => &self.wbus,
            12 => &self.oscb,
            13 => &self.osca,
            _ => return None,
        })
    }

    /// See [`TargetRoutes::arm`].
    pub fn arm(&self, newly_present: &[bool; SOURCES]) {
        for (source, &now) in newly_present.iter().enumerate() {
            if now && let Some(param) = self.amount_param(source) {
                param.smoothed.reset(param.value());
            }
        }
    }

    /// See [`TargetRoutes::advance_into`].
    #[inline]
    pub fn advance_into(&self, live: &[bool; SOURCES], out: &mut [f32; SOURCES]) {
        for (source, &on) in live.iter().enumerate() {
            if on && let Some(param) = self.amount_param(source) {
                out[source] = param.smoothed.next();
            }
        }
    }
}

/// Which frame slots [`BusRoutes`] carries, in its own order.
pub const BUS_SOURCES: [usize; SOURCES - 1] = [0, 1, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];

/// All nine targets' routes.
#[derive(Params)]
pub struct Routes {
    #[nested(id_prefix = "mod_afreq", group = "Modulation - Oscillator A frequency")]
    pub oscillator_a_frequency: TargetRoutes,
    #[nested(id_prefix = "mod_apw", group = "Modulation - Oscillator A pulse width")]
    pub oscillator_a_pulse_width: TargetRoutes,
    #[nested(id_prefix = "mod_bfreq", group = "Modulation - Oscillator B frequency")]
    pub oscillator_b_frequency: TargetRoutes,
    #[nested(id_prefix = "mod_bpw", group = "Modulation - Oscillator B pulse width")]
    pub oscillator_b_pulse_width: TargetRoutes,
    #[nested(id_prefix = "mod_cutoff", group = "Modulation - Cutoff")]
    pub cutoff: TargetRoutes,
    #[nested(id_prefix = "mod_reso", group = "Modulation - Resonance")]
    pub resonance: TargetRoutes,
    #[nested(id_prefix = "mod_lforate", group = "Modulation - LFO rate")]
    pub lfo_rate: TargetRoutes,
    #[nested(id_prefix = "mod_modbus", group = "Modulation - Mod bus")]
    pub mod_bus: BusRoutes,
    #[nested(id_prefix = "mod_wbus", group = "Modulation - Wheel bus")]
    pub wheel_bus: TargetRoutes,
    /// **The collection's standard Amplitude**, after the VCA — added by the modulation standard
    /// (2026-09-26), every pair absent at Init.
    #[nested(id_prefix = "mod_amp", group = "Modulation - Amplitude")]
    pub amplitude: TargetRoutes,
}

impl Default for Routes {
    fn default() -> Self {
        Self::new()
    }
}

impl Routes {
    /// The init patch: **five routes present, every depth at zero.**
    ///
    /// Two are the machine's own hard wiring — the dedicated filter-envelope amount and the filter
    /// keyboard amount, which were knobs that always reached the cutoff. Three exist because
    /// `control-map.json` claims them as collection roles and a role on an absent route is a dead
    /// knob. The five legacy destination switches all defaulted to `Off`, so nothing else is wired.
    ///
    /// It is the same sound it always was, because a route at zero depth adds exactly nothing.
    pub fn new() -> Self {
        Self {
            oscillator_a_frequency: TargetRoutes::new(target::OSCILLATOR_A_FREQUENCY),
            oscillator_a_pulse_width: TargetRoutes::new(target::OSCILLATOR_A_PULSE_WIDTH),
            oscillator_b_frequency: TargetRoutes::new(target::OSCILLATOR_B_FREQUENCY),
            oscillator_b_pulse_width: TargetRoutes::new(target::OSCILLATOR_B_PULSE_WIDTH),
            cutoff: TargetRoutes::new(target::FILTER_CUTOFF),
            resonance: TargetRoutes::new(target::RESONANCE),
            lfo_rate: TargetRoutes::new(target::LFO_RATE),
            mod_bus: BusRoutes::new(),
            wheel_bus: TargetRoutes::new(target::WHEEL_BUS),
            amplitude: TargetRoutes::new(target::AMPLITUDE),
        }
    }

    /// The **seven ordinary** targets, in declared target order. The module is not one of them —
    /// it carries one fewer source, so it is handled beside this rather than through it.
    pub fn ordinary(&self) -> [(usize, &TargetRoutes); TARGETS - 1] {
        [
            (target::OSCILLATOR_A_FREQUENCY, &self.oscillator_a_frequency),
            (
                target::OSCILLATOR_A_PULSE_WIDTH,
                &self.oscillator_a_pulse_width,
            ),
            (target::OSCILLATOR_B_FREQUENCY, &self.oscillator_b_frequency),
            (
                target::OSCILLATOR_B_PULSE_WIDTH,
                &self.oscillator_b_pulse_width,
            ),
            (target::FILTER_CUTOFF, &self.cutoff),
            (target::RESONANCE, &self.resonance),
            (target::LFO_RATE, &self.lfo_rate),
            // The multiplier's grid is uniform like the seven above; only the summing module's is
            // short, because the wheel is not a source on it.
            (target::WHEEL_BUS, &self.wheel_bus),
            (target::AMPLITUDE, &self.amplitude),
        ]
    }

    /// Which routes are live, for the whole instrument. **Once per interval.**
    pub fn topology(&self) -> Routing {
        let mut routing = Routing::new();
        for (index, group) in self.ordinary() {
            routing.present[index] = group.presences(index);
        }
        routing.present[target::MOD_BUS] = self.mod_bus.presences();
        routing
    }

    /// The topology for this interval, with every **newly present** route's smoother snapped to its
    /// stored value.
    ///
    /// `previous` is the topology the last interval ran, so the caller keeps it across buffers. See
    /// [`TargetRoutes::arm`] for why resuming a skipped smoother is wrong.
    pub fn topology_from(&self, previous: &Routing) -> Routing {
        let routing = self.topology();
        let newly = |index: usize| {
            let mut out = [false; SOURCES];
            for (slot, (&now, &before)) in out.iter_mut().zip(
                routing.present[index]
                    .iter()
                    .zip(previous.present[index].iter()),
            ) {
                *slot = now && !before;
            }
            out
        };
        for (index, group) in self.ordinary() {
            group.arm(&newly(index));
        }
        self.mod_bus.arm(&newly(target::MOD_BUS));
        routing
    }

    /// Fills this sample's amounts into an already-topologised [`Routing`].
    #[inline]
    pub fn advance_into(&self, routing: &mut Routing) {
        for (index, group) in self.ordinary() {
            let live = routing.present[index];
            group.advance_into(&live, &mut routing.amounts[index]);
        }
        let live = routing.present[target::MOD_BUS];
        self.mod_bus
            .advance_into(&live, &mut routing.amounts[target::MOD_BUS]);
    }
}

use mxm_mono_pr1_dsp::routing::target;

#[cfg(test)]
mod tests {
    use super::*;

    use mxm_plugin_test::routing_checks;

    /// **Every route parameter says what the DSP does** — the modulation standard's plugin half:
    /// each pair's travel is its offer's (the wheel into its own bus minted nowhere), its reading
    /// carries its target's unit and states what `mxm_mono_pr1_dsp::conformance` measures the
    /// graph delivering, and every reading survives the host's round trip.
    ///
    /// Falsified before trusted: with the oscillators' added pitch read at the bus's 24 semitones,
    /// it names every added pitch pair from a performance source.
    #[test]
    fn every_route_parameter_says_what_the_dsp_does() {
        let routes = Routes::new();
        let groups = routes.ordinary();
        if let Err(failures) = routing_checks::amounts(
            &mxm_mono_pr1_dsp::conformance::Declared,
            |target, source| {
                if target == mxm_mono_pr1_dsp::routing::target::MOD_BUS {
                    routes.mod_bus.amount_param(source)
                } else {
                    groups
                        .iter()
                        .find(|(index, _)| *index == target)
                        .map(|(_, group)| group.amount_param(source))
                }
            },
        ) {
            panic!(
                "{} failure(s):
{}",
                failures.len(),
                failures.join(
                    "
"
                )
            );
        }
    }

    /// [`ROUTE_IDS`] is a hand-written mirror of what the derive emits, and the two have to agree or
    /// every preset, control-map entry and keyboard scope that reads the table is pointing at
    /// nothing. This is the test that holds them together.
    #[test]
    fn the_id_table_is_what_the_derive_actually_produces() {
        let routes = Routes::new();
        let emitted: Vec<String> = routes
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        // Eight uniform targets and the summing module, which carries one fewer: the wheel is not
        // a source on the bus it multiplies.
        assert_eq!(
            emitted.len(),
            ((TARGETS - 1) * SOURCES + SOURCES - 1) * 2,
            "the derive must emit one presence and one amount per pair"
        );
        for target in ROUTE_IDS.iter() {
            for (amount, present) in target.iter() {
                assert!(
                    emitted.iter().any(|id| id == amount),
                    "{amount} is in the table but not in the derive"
                );
                assert!(
                    emitted.iter().any(|id| id == present),
                    "{present} is in the table but not in the derive"
                );
            }
        }
        let mut seen: Vec<&str> = ROUTE_IDS
            .iter()
            .flat_map(|target| target.iter())
            .flat_map(|(a, p)| [*a, *p])
            .collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "a permanent id is duplicated");
    }

    /// Init wires exactly what the DSP declares, and **every amount starts at zero** — this
    /// instrument's init patch has no deviation to record.
    #[test]
    fn init_wires_the_declared_routes_and_nothing_is_turned_up() {
        let routes = Routes::new();
        let topology = routes.topology();
        assert_eq!(topology.present, Routing::init().present);
        for (index, group) in routes.ordinary() {
            for (slot, route) in group.routes(index).iter().enumerate() {
                // The multiplier's two factors at full, the wheel's vibrato at its reading.
                let amount = init_amount(index, slot);
                if amount == 0.0 || amount == 1.0 {
                    assert_eq!(
                        route.amount.normalised(),
                        if amount == 1.0 { 1.0 } else { 0.5 },
                        "{} started at the wrong depth",
                        route.amount_id
                    );
                } else {
                    assert_eq!(
                        route.amount.text(),
                        "+0.50 st",
                        "{} is the wheel's vibrato",
                        route.amount_id
                    );
                }
            }
        }
    }

    /// **A route amount reads and parses in the target's own domain** — decision 1.11, and the
    /// reason this instrument needed it: cutoff is the one target whose scale column is not
    /// uniform, so two rows under it at the same percentage mean different numbers of octaves.
    ///
    /// One case per class, and the round trip is what makes the parse honest: a number a player
    /// types must set the depth that number describes. A percentage carries its target's scale too,
    /// or a full-amount pulse-width route reads `100 %` while moving the width by half its range.
    #[test]
    fn a_route_amount_reads_and_parses_in_its_targets_own_domain() {
        use mxm_mono_pr1_dsp::routing::source as src;
        use nice_plug::prelude::Param;

        let routes = Routes::new();
        // (parameter, typed text, the plain amount it must mean, what full amount must display)
        let cases: [(&FloatParam, &str, f32, &str); 5] = [
            // Semitones: `FREQUENCY_MOD_SEMITONES` is 24, so half an amount is 12 st.
            (
                &routes.oscillator_a_frequency.lfo,
                "12 st",
                0.5,
                "+24.00 st",
            ),
            // Octaves, uniform column: `FILTER_MOD_OCTAVES` is 8.
            (&routes.cutoff.lfo, "4 oct", 0.5, "+8.00 oct"),
            // Octaves, **the collapsed column**: twice the destination scale.
            (&routes.cutoff.fenv, "4 oct", 0.25, "+16.00 oct"),
            // A width offset: `PWM_MOD_DEPTH` is 0.5, so full amount is 50 points of width.
            (&routes.oscillator_a_pulse_width.lfo, "25 %", 0.5, "+50 %"),
            // The module publishes a normalised bus level, so full amount is a full bus.
            (&routes.mod_bus.lfo, "50 %", 0.5, "+100 %"),
        ];
        for (param, typed, expected, at_full) in cases {
            let parsed = param
                .string_to_normalized_value(typed)
                .unwrap_or_else(|| panic!("{typed} did not parse"));
            let plain = param.preview_plain(parsed);
            assert!(
                (plain - expected).abs() < 1e-3,
                "{typed} must mean {expected}, not {plain}"
            );
            assert_eq!(param.normalized_value_to_string(1.0, false), at_full);
            // And the round trip closes: what it prints is what it parses back.
            let printed = param.normalized_value_to_string(parsed, false);
            let reparsed = param
                .string_to_normalized_value(&printed)
                .expect("its own text must parse");
            assert!(
                (reparsed - parsed).abs() < 1e-3,
                "{printed} did not round-trip"
            );
        }
        let _ = src::LFO;
    }

    /// **Every reading survives the host's round trip, a rounded zero included**: printed, parsed and
    /// printed again, it is the same text — at amounts either side of zero, not only the round numbers.
    /// A plain signed format printed `-0 %` there, which parses to zero and prints `+0 %`, and
    /// `clap-validator`'s `param-conversions` fails on that whenever its random values land in the
    /// sliver (`mxm_modulation_params::signed`).
    #[test]
    fn every_reading_survives_the_hosts_round_trip_a_rounded_zero_included() {
        let routes = Routes::new();
        let check = |param: &FloatParam| {
            for normalised in [
                0.0f32, 0.25, 0.4999, 0.49999, 0.5, 0.50001, 0.5001, 0.75, 1.0,
            ] {
                let text = param.normalized_value_to_string(normalised, true);
                let back = param
                    .string_to_normalized_value(&text)
                    .unwrap_or_else(|| panic!("{}: {text} does not parse", param.name()));
                assert_eq!(
                    text,
                    param.normalized_value_to_string(back, true),
                    "{} at {normalised}",
                    param.name()
                );
            }
        };
        for (_, group) in routes.ordinary() {
            for source in 0..SOURCES {
                check(group.amount_param(source));
            }
        }
        for source in 0..SOURCES {
            if let Some(param) = routes.mod_bus.amount_param(source) {
                check(param);
            }
        }
    }

    /// **Remove, edit while absent, re-add: the route arrives at the depth the player set.**
    ///
    /// An absent route's smoother is never advanced, so it must not be resumed either. While the
    /// pair is absent the parameter itself stays editable — a host automating it, or a preset load —
    /// and that moves the *target* while the smoother's current value stays wherever the last live
    /// sample left it. Resuming would ramp the route in from that stale number over a span set by
    /// how long it was absent, which is the accumulation across skipped spans
    /// `plan-modulation-routing.md` §6.2 forbids: it makes a route's first audible value depend on
    /// the host's buffer sizes.
    ///
    /// **Falsified before trusted**: with `arm`'s body removed, the first sample after re-adding
    /// reads `0.898` — a point on the ramp down from the old depth — instead of the stored `-0.4`.
    #[test]
    fn a_re_added_route_arrives_at_its_stored_depth_rather_than_ramping_from_a_stale_one() {
        use nice_plug::params::InternalParamMut;

        let routes = Routes::new();
        assert!(
            !routes.cutoff.oscb_on.value(),
            "this pair starts absent, which is what the test needs"
        );

        // Present it, and let it settle at one depth. `_internal_update_smoother(_, false)` is what
        // the wrapper calls after every host parameter change, and it moves the *target* only.
        const RATE: f32 = 48_000.0;
        unsafe {
            routes.cutoff.oscb_on._internal_set_plain_value(true);
            routes.cutoff.oscb._internal_set_plain_value(0.9);
            routes.cutoff.oscb._internal_update_smoother(RATE, true);
        }
        let mut routing = Routing::new();
        routing = routes.topology_from(&routing);
        let mut amounts = routing;
        routes.advance_into(&mut amounts);
        assert_eq!(
            amounts.amounts[target::FILTER_CUTOFF][mxm_mono_pr1_dsp::routing::source::OSCILLATOR_B],
            0.9
        );

        // Remove it, then edit the depth while it is absent.
        unsafe {
            routes.cutoff.oscb_on._internal_set_plain_value(false);
        }
        let absent = routes.topology_from(&routing);
        unsafe {
            // A host automation write moves the target. Nothing advances the smoother while the
            // pair is absent, so its current value is still 0.9 and it is now mid-ramp.
            routes.cutoff.oscb._internal_set_plain_value(-0.4);
            routes.cutoff.oscb._internal_update_smoother(RATE, false);
        }
        assert!(
            routes.cutoff.oscb.smoothed.is_smoothing(),
            "the edit must leave the smoother mid-ramp, or this proves nothing"
        );

        // Re-add it. The very first sample must be the stored depth, not a ramp from 0.9.
        unsafe {
            routes.cutoff.oscb_on._internal_set_plain_value(true);
        }
        let mut back = routes.topology_from(&absent);
        routes.advance_into(&mut back);
        assert_eq!(
            back.amounts[target::FILTER_CUTOFF][mxm_mono_pr1_dsp::routing::source::OSCILLATOR_B],
            -0.4,
            "a re-added route must arrive at its stored depth"
        );
    }
}
