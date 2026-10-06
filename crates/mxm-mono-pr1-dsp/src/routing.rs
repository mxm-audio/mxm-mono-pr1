//! What mxm-mono-pr1 can modulate, and with what.
//!
//! `plans/plan-mxm-mono-pr1-modulation.md` §2 and §3. The shared machinery is [`mxm_modulation`];
//! this module is the instrument's own declaration — its **source list**, its **target list**, its
//! **frame unit**, and each target's **scale**.
//!
//! # The frame unit is ⅛, and it is sized by the widest *composed* value
//!
//! [`mxm_modulation::SourceFrame::write`] bounds every published value to unit magnitude, and this
//! machine's sources are not unit-bounded: the LFO's three enabled paths sum to ±3 without
//! normalisation, Oscillator B's enabled sum reaches 1.9 and Oscillator A's 1.6. **Publishing any of
//! them raw would clamp**, so every source is expressed in one shared unit.
//!
//! That unit is **⅛ rather than ¼**, and the reason is the part that is easy to get wrong: a frame
//! slot holds everything *built from* the sources as well as the sources themselves. [`MOD_BUS`]
//! sums up to **5.9** raw units on the wiring the hardware itself permits, which at ¼ is 1.475 and
//! would clamp on publication — taking the whole legacy Wheel path with it, since the multiplier
//! reads it. At ⅛ it is 0.7375.
//!
//! **⅛ is a power of two, and that is what keeps the arithmetic exact.** Scaling by a power of two
//! commutes with `f32` rounding away from the subnormal range, so `s ÷ 8` carries no error,
//! `Σ(sᵢ ÷ 8)` is `(Σ sᵢ) ÷ 8`, and a target scale of `8 × k` undoes it in one multiply against a
//! constant that is itself exact. [`Graph::sum_uniform`] is the form that relies on it.
//!
//! **The bound is the legacy machine's, not a promise about arbitrary routing.** `Mod bus` is now an
//! ordinary summing target taking fourteen sources at signed amounts, so a player can sum past ±1
//! and the frame will clamp. That is the bounded-publication contract working — it is what keeps a
//! cycle finite — rather than a defect.
//!
//! # A `product` target un-scales into its law, which is why one works here at all
//!
//! [`mxm_modulation::sum`] is scale-invariant; [`mxm_modulation::product`] is not. Its factor is
//! `1 + amount·(source − 1)`, and in a ⅛ frame that literal `1` is **eight raw units**, so an
//! `n`-factor product is off by `8ⁿ⁻¹` and a partial amount interpolates toward the wrong neutral.
//! An earlier revision of this instrument read that as *a product target needs an unscaled frame*
//! and made the wheel stage fixed wiring — which quietly dropped decision 1.14, the owner's
//! multiplier module, and the owner noticed.
//!
//! **The fix is the same rule the frame already has, applied on the way in.** A module scales *into*
//! the frame's domain before publishing; a product scales *out of* it before applying its law:
//!
//! ```text
//! factor = 1 + amount × (frame.read(source) × FRAME_SCALE − 1)
//! ```
//!
//! The `1` is then one raw unit, which is what the law means, and every property comes back: nothing
//! present is neutral, a zero amount is neutral rather than annihilating, and the machine's own
//! `Mod bus × wheel` falls out of two routes at full amount. The result is published back through
//! [`FRAME_UNIT`] like any other source, so [`mxm_modulation::SourceFrame::write`]'s bound still
//! keeps a cycle through the multiplier finite — the guarantee the unit delay alone does not give.
//!
//! [`Graph::product`] is that law — `mxm_modulation::product_with_tops` since two instruments carry
//! it, each factor neutral at its source's **top**: one raw unit, and zero for the standard
//! Velocity, which is `v − 1`.
//!
//! # The collection's standard
//!
//! Key, Velocity, Wheel, Pressure and Bend mean what they mean on every instrument, and a route the
//! Pro-One never had reaches what it reaches on every instrument ([`mxm_modulation::standard`];
//! `plans/plan-modulation-standard.md`). **A target whose machine routes shared one bus scale keeps
//! that scale for them** — `mxm_modulation::sum_split`'s uniform half, bit-identical to the bus while
//! nothing added is live — **and every added pair takes the standard reach** in its added half
//! ([`ADDED_SCALE`]). The instrument gains the standard **Amplitude** target, after the VCA.

use mxm_modulation::standard::{self, AMPLITUDE_SUM_BOUND, Law, Offer, Performance, reach};
use mxm_modulation::{Compacted, SourceFrame};

use crate::voice::{FILTER_MOD_OCTAVES, FREQUENCY_MOD_SEMITONES, PWM_MOD_DEPTH};

/// Every source this instrument can route, in **declared evaluation order**.
///
/// The order is load-bearing rather than cosmetic, and it is the order the instrument *publishes*
/// in: a route whose source is published before its target is summed reads *this* sample, and one
/// published after reads *last* sample. [`FORWARD_THROUGH`] is the table that says where each
/// target is summed, because three of them are summed out of source order.
pub mod source {
    /// The sounding pitch's distance from middle C, **in octaves** (`standard::key` over a
    /// twelve-semitone unit). Asymmetric: MIDI 127 is 67 semitones above key 60, so the range is
    /// −5.00 … +5.58 octaves.
    pub const KEY: usize = 0;
    /// The velocity of the press that last triggered the envelopes, `v − 1` (`standard::velocity`):
    /// zero at the hardest note. **New MIDI path** — this machine read no velocity at all before
    /// the conversion.
    pub const VELOCITY: usize = 1;
    /// The mod wheel, CC 1, as the owning channel holds it. Unipolar.
    pub const WHEEL: usize = 2;
    /// Channel pressure. Unipolar. **New MIDI path.**
    pub const PRESSURE: usize = 3;
    /// The pitch bender's normalised position, signed.
    pub const BEND: usize = 4;
    /// Noise, tapped **before** the noise mixer level so a route does not change depth when the
    /// mix moves.
    pub const NOISE: usize = 5;
    /// The LFO's enabled-waveform sum. Bipolar, and **not normalised by the enables**: three unity
    /// paths reach ±3, which is why the frame unit exists.
    pub const LFO: usize = 6;
    /// The resolved shared gate, after Drone > Repeat > keyboard. Held 0 or 1, not a pulse.
    pub const GATE: usize = 7;
    /// The filter envelope. Unipolar.
    pub const FILTER_ENVELOPE: usize = 8;
    /// The amplifier envelope. Unipolar. **New as a source** — it reached nothing but the VCA.
    pub const AMPLIFIER_ENVELOPE: usize = 9;
    /// The summing module: *what the wheel scales*. A target as well as a source — but **the wheel
    /// is not one of its sources**, because the wheel's relationship with this bus is already
    /// defined and it is multiplication, which is [`WHEEL_BUS`]'s job. See
    /// `WHEEL_IS_NOT_A_BUS_SOURCE`.
    pub const MOD_BUS: usize = 10;
    /// The multiplier module's output. Wired at Init to [`MOD_BUS`] × [`WHEEL`], which is the
    /// machine's own two-stage routing, but **any source can be a factor** — decision 1.14.
    pub const WHEEL_BUS: usize = 11;
    /// Oscillator B's enabled-waveform sum, at audio rate — the same sum the mixer hears.
    pub const OSCILLATOR_B: usize = 12;
    /// Oscillator A's enabled-waveform sum, at audio rate. **New as a source.**
    pub const OSCILLATOR_A: usize = 13;
}

/// How many sources the instrument declares.
pub const SOURCES: usize = 14;

/// Their names, in source order, for the interface and for accessibility.
pub const SOURCE_NAMES: [&str; SOURCES] = [
    "Key",
    "Velocity",
    "Wheel",
    "Pressure",
    "Bend",
    "Noise",
    "LFO",
    "Gate",
    "Filter envelope",
    "Amplifier envelope",
    "Mod bus",
    "Wheel bus",
    "Oscillator B",
    "Oscillator A",
];

/// The frame unit: every source is published as its raw value times this.
///
/// A power of two, so the division and the target scale that undoes it are both exact.
pub const FRAME_UNIT: f32 = 0.125;

/// The inverse of [`FRAME_UNIT`], which a target scale carries.
pub const FRAME_SCALE: f32 = 8.0;

/// The keyboard's own span in octaves either side of middle C, used to publish [`source::KEY`].
///
/// **Asymmetric**, because MIDI runs 0…127 against key 60: the bottom is five octaves and the top is
/// 67 semitones. The larger of the two is what the frame unit has to hold.
pub const KEY_OCTAVES_BELOW: f32 = 5.0;
/// See [`KEY_OCTAVES_BELOW`].
pub const KEY_OCTAVES_ABOVE: f32 = 67.0 / 12.0;

/// Every target this instrument declares.
///
/// **Deliberate rather than maximal.** A routable gate is a named exclusion rather than an omission:
/// it would add a fourth contender to the Drone > Repeat > keyboard precedence — a free-running source
/// able to retrigger forever, the one way this instrument could lose exact `Inert`. **Amplitude was
/// one too, and is a target now** (the owner, 2026-09-26: every instrument has the standard
/// Amplitude): as a *factor on the VCA's output* it cannot open a closed VCA or outlive the envelope
/// that ends the voice, so exact `Inert` stands.
pub mod target {
    /// Oscillator A's pitch, summed in semitones.
    pub const OSCILLATOR_A_FREQUENCY: usize = 0;
    /// Oscillator A's pulse width, summed as a width offset.
    pub const OSCILLATOR_A_PULSE_WIDTH: usize = 1;
    /// Oscillator B's pitch, summed in semitones.
    pub const OSCILLATOR_B_FREQUENCY: usize = 2;
    /// Oscillator B's pulse width, summed as a width offset.
    pub const OSCILLATOR_B_PULSE_WIDTH: usize = 3;
    /// Filter cutoff, summed in octaves. **The one target whose scale column is not uniform.**
    pub const FILTER_CUTOFF: usize = 4;
    /// Filter resonance, summed as a fraction of the control's own range.
    pub const RESONANCE: usize = 5;
    /// The LFO's rate, summed in octaves.
    pub const LFO_RATE: usize = 6;
    /// The summing module, whose result is published as [`source::MOD_BUS`].
    pub const MOD_BUS: usize = 7;
    /// The **multiplier** module, whose result is published as [`source::WHEEL_BUS`]. Decision
    /// 1.14: a target whose law is product and whose output is a source like any other.
    pub const WHEEL_BUS: usize = 8;
    /// **The collection's standard Amplitude** — `standard::amplitude_factor` on the output after
    /// the VCA, silence to double. Added by the modulation standard (the owner, 2026-09-26), absent
    /// at Init. A factor on the VCA's output cannot open a closed VCA, so exact `Inert` stands.
    pub const AMPLITUDE: usize = 9;
}

/// How many targets the instrument declares.
pub const TARGETS: usize = 10;

/// The one target whose law is **product** rather than sum — [`Graph::product`], not [`Graph::sum`].
pub const PRODUCT_TARGET: usize = target::WHEEL_BUS;

/// **The wheel is not a source on `Mod bus`.**
///
/// The wheel already has a relationship with that bus: `Wheel bus` is `Mod bus × wheel`. Routing the
/// raw wheel *into* the same bus additively is the same control wired two ways into one stage, and
/// the only thing it can produce is a transposing DC offset riding on whatever else is in there
/// instead of controlling that thing's depth — the owner's report, 2026-09-14: *"I get pitch change
/// both from the wheel, which transposes, and from the LFO, which makes vibrato. That is wrong
/// behaviour! The wheel should change the level of vibrato."*
///
/// The gesture the bus exists for is exactly that: put the LFO in it, route `Wheel bus` at the
/// pitch, and the wheel fades the vibrato in and out while playing. The raw wheel remains an
/// additive source at every one of the seven ordinary targets, so nothing goes out of reach, and the
/// plugin simply does not give the pair a parameter.
///
/// [`Graph::set_topology`] enforces it rather than trusting its caller.
pub const WHEEL_IS_NOT_A_BUS_SOURCE: (usize, usize) = (target::MOD_BUS, source::WHEEL);

/// Their names, in target order.
pub const TARGET_NAMES: [&str; TARGETS] = [
    "Oscillator A frequency",
    "Oscillator A pulse width",
    "Oscillator B frequency",
    "Oscillator B pulse width",
    "Cutoff",
    "Resonance",
    "LFO rate",
    "Mod bus",
    "Wheel bus",
    "Amplitude",
];

/// The key source's unit, in semitones: a voice publishes the key in octaves.
pub const KEY_UNIT_SEMITONES: f32 = 12.0;

/// Which of the standard's performance sources each source is — `None` for the machine's own
/// generators, modules and audio.
pub const PERFORMANCE: [Option<Performance>; SOURCES] = [
    Some(Performance::Key),
    Some(Performance::Velocity),
    Some(Performance::Wheel),
    Some(Performance::Pressure),
    Some(Performance::Bend),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
];

/// Each target's law, for the standard's offer: sums (the two modules included) and the amplitude
/// factor.
pub const LAW: [Law; TARGETS] = {
    let mut law = [Law::Sum; TARGETS];
    law[target::AMPLITUDE] = Law::Factor;
    law
};

/// The sources the Pro-One's modulation reached its destinations from — the bus's inputs and the
/// two buses themselves.
const BUS_PATHS: [usize; 5] = [
    source::LFO,
    source::FILTER_ENVELOPE,
    source::OSCILLATOR_B,
    source::MOD_BUS,
    source::WHEEL_BUS,
];

/// Whether **the Pro-One itself** had this path, so its reach is the machine's: the bus paths into
/// both oscillators' frequency and width and the cutoff, the cutoff's dedicated keyboard amount, and
/// everything into the two modules, which are the machine's own.
#[must_use]
pub const fn machine(target: usize, source: usize) -> bool {
    match target {
        target::MOD_BUS | target::WHEEL_BUS => true,
        target::FILTER_CUTOFF if source == source::KEY => true,
        target::OSCILLATOR_A_FREQUENCY
        | target::OSCILLATOR_A_PULSE_WIDTH
        | target::OSCILLATOR_B_FREQUENCY
        | target::OSCILLATOR_B_PULSE_WIDTH
        | target::FILTER_CUTOFF => {
            let mut i = 0;
            while i < BUS_PATHS.len() {
                if BUS_PATHS[i] == source {
                    return true;
                }
                i += 1;
            }
            false
        }
        _ => false,
    }
}

/// Whether and how a pair is offered — `standard::offer`, and the one pair the owner refused,
/// [`WHEEL_IS_NOT_A_BUS_SOURCE`].
#[must_use]
pub const fn offer(target: usize, source: usize) -> Offer {
    if target == WHEEL_IS_NOT_A_BUS_SOURCE.0 && source == WHEEL_IS_NOT_A_BUS_SOURCE.1 {
        return Offer::Refused;
    }
    standard::offer(LAW[target], PERFORMANCE[source], machine(target, source))
}

/// Each factor's **top** in the multiplier — the raw value at which it leaves the product alone at
/// full amount: one raw unit, and zero for the standard Velocity, so a full-velocity note passes
/// the other factors through exactly as the raw velocity at one did.
pub const PRODUCT_TOPS: [f32; SOURCES] = {
    let mut tops = [1.0; SOURCES];
    tops[source::VELOCITY] = 0.0;
    tops
};

/// The last source published **before** each target is summed.
///
/// A route from a source at or below this index reads *this* sample; one above reads *last*
/// sample — one sample of delay, which at audio rate is a comb and is declared here rather than
/// discovered. Four targets are combined out of source order and this is where that is written down:
///
/// - **LFO rate** is summed before the LFO ticks, so every route into it from the LFO onward is
///   backward, the LFO reading its own rate included.
/// - **Mod bus** is summed after the envelopes, so it reads itself, the multiplier and the two
///   oscillators one sample late.
/// - **Wheel bus** is multiplied immediately after `Mod bus` publishes — which is what lets the
///   shipped `Mod bus × Wheel` wiring be a *forward* pair rather than a one-sample-late one — so a
///   factor from itself or either oscillator reads last sample.
/// - **Oscillator B's** two targets are summed before B renders, which is the machine's
///   deliberately-last B commit expressed as a table row rather than as a warning about a line's
///   position.
pub const FORWARD_THROUGH: [usize; TARGETS] = {
    let mut table = [source::OSCILLATOR_A; TARGETS];
    table[target::LFO_RATE] = source::NOISE;
    table[target::MOD_BUS] = source::AMPLIFIER_ENVELOPE;
    table[target::WHEEL_BUS] = source::MOD_BUS;
    table[target::OSCILLATOR_B_FREQUENCY] = source::WHEEL_BUS;
    table[target::OSCILLATOR_B_PULSE_WIDTH] = source::WHEEL_BUS;
    table[target::OSCILLATOR_A_FREQUENCY] = source::OSCILLATOR_B;
    table[target::OSCILLATOR_A_PULSE_WIDTH] = source::OSCILLATOR_B;
    table
};

/// A target whose scale column is the same for every source, and what that one scale is.
///
/// **This is what buys bit-identity with the pre-conversion renders**, and it is why the table is
/// `Option` rather than a plain array. Where a column is uniform, [`Graph::sum_uniform`] takes
/// [`mxm_modulation::sum`] — every route at scale one — and applies `FRAME_SCALE × k` **outside**
/// the sum, which is literally the instruction sequence this voice executed before it had routing:
/// the legacy graph applied one destination scale to an already-summed bus. Applying a scale per
/// route instead, which is what [`mxm_modulation::sum_scaled`] does, distributes a multiply over a
/// sum and does not round alike.
///
/// [`target::FILTER_CUTOFF`] is `None` because its column is genuinely not uniform: the filter
/// envelope carries twice the destination scale (it absorbs two collapsed legacy paths) and the key
/// carries one octave of cutoff per octave of keyboard. That is what [`CUTOFF_SCALE`] is for, and it
/// is why cutoff is the destination whose digest is expected to move.
pub const UNIFORM_SCALE: [Option<f32>; TARGETS] = {
    let mut table = [None; TARGETS];
    table[target::OSCILLATOR_A_FREQUENCY] = Some(FREQUENCY_MOD_SEMITONES);
    table[target::OSCILLATOR_A_PULSE_WIDTH] = Some(PWM_MOD_DEPTH);
    table[target::OSCILLATOR_B_FREQUENCY] = Some(FREQUENCY_MOD_SEMITONES);
    table[target::OSCILLATOR_B_PULSE_WIDTH] = Some(PWM_MOD_DEPTH);
    table[target::RESONANCE] = Some(RESONANCE_REACH);
    table[target::LFO_RATE] = Some(LFO_RATE_OCTAVES);
    // Amplitude has a per-route column (`AMPLITUDE_SCALE`), like cutoff.
    // The module publishes in frame units, so its scale is the unit's own inverse: a route at full
    // amount from a unit-magnitude source reaches unit magnitude on the bus.
    table[target::MOD_BUS] = Some(FRAME_UNIT);
    // `Wheel bus` has no scale at all: its law is product, and [`Graph::product`] evaluates it.
    table[target::WHEEL_BUS] = None;
    table
};

/// How far a full-amount route moves resonance, as a fraction of the control's own 0…1 range.
///
/// Chosen. The hardware's RESONANCE CV jack is the 2019 reissue's rather than the 1981 machine's,
/// so there is no documented volts-per-unit to derive it from; a full sweep from either end is the
/// musically useful reach and is what the number says.
pub const RESONANCE_REACH: f32 = 1.0;

/// How far a full-amount route moves the LFO's rate, in octaves — **the collection's standard
/// four** (`standard::reach::OCTAVES`). The Pro-One never modulated its LFO rate, so every route here
/// is an added one; it was six until the modulation standard.
pub const LFO_RATE_OCTAVES: f32 = reach::OCTAVES;

/// The **added half** of each uniform target's split sum: `Some(scale)` for a pair whose standard
/// reach differs from the column's machine scale, in frame-scaled units like [`CUTOFF_SCALE`], and
/// `None` for a pair summed on the machine's bus scale.
///
/// - **The oscillators' frequency and width**: the bus paths keep the machine's 24 semitones and half
///   of the width; every added pair takes the standard octave and 45 %, and Key twelve semitones and
///   9 % per octave.
/// - **Resonance and the LFO rate**: every pair is added and the column is the standard's, but Key
///   reads per octave of keyboard, a fifth of the reach and one octave per octave.
pub const ADDED_SCALE: [[Option<f32>; SOURCES]; TARGETS] = {
    let mut table = [[None; SOURCES]; TARGETS];
    let pitch = [
        target::OSCILLATOR_A_FREQUENCY,
        target::OSCILLATOR_B_FREQUENCY,
    ];
    let width = [
        target::OSCILLATOR_A_PULSE_WIDTH,
        target::OSCILLATOR_B_PULSE_WIDTH,
    ];
    let key_linear = reach::KEY_LINEAR_FRACTION_PER_OCTAVE;
    let mut s = 0;
    while s < SOURCES {
        if !machine(target::OSCILLATOR_A_FREQUENCY, s) {
            let (p, w) = if s == source::KEY {
                (
                    standard::key_scale(reach::KEY_PITCH_SEMITONES_PER_OCTAVE, KEY_UNIT_SEMITONES),
                    standard::key_scale(reach::WIDTH * key_linear, KEY_UNIT_SEMITONES),
                )
            } else {
                (reach::PITCH_SEMITONES, reach::WIDTH)
            };
            let mut i = 0;
            while i < 2 {
                table[pitch[i]][s] = Some(FRAME_SCALE * p);
                table[width[i]][s] = Some(FRAME_SCALE * w);
                i += 1;
            }
        }
        s += 1;
    }
    table[target::RESONANCE][source::KEY] =
        Some(FRAME_SCALE * standard::key_scale(reach::CONTROL * key_linear, KEY_UNIT_SEMITONES));
    table[target::LFO_RATE][source::KEY] =
        Some(FRAME_SCALE * standard::key_scale(reach::KEY_OCTAVES_PER_OCTAVE, KEY_UNIT_SEMITONES));
    table
};

/// The Amplitude target's per-route column, in frame-scaled units: the standard factor's whole swing
/// on every source, a fifth of it per octave from Key.
pub const AMPLITUDE_SCALE: [f32; SOURCES] = {
    let mut table = [FRAME_SCALE * reach::AMPLITUDE; SOURCES];
    table[source::KEY] = FRAME_SCALE
        * standard::key_scale(
            reach::AMPLITUDE * reach::KEY_LINEAR_FRACTION_PER_OCTAVE,
            KEY_UNIT_SEMITONES,
        );
    table
};

/// Cutoff's per-route scale column — the one target that needs one.
///
/// **Each entry carries [`FRAME_SCALE`] as well as its reach in octaves**, because
/// [`mxm_modulation::sum_scaled`] applies this column to values that are already in frame units and
/// nothing else undoes the unit for this target. [`Graph::sum_uniform`] carries the same factor in
/// its single outside multiply; here it is per column entry.
///
/// Two entries differ from the destination scale and both are arithmetic rather than taste:
///
/// - **The filter envelope carries twice it.** Its dedicated amount and its bus path both reached
///   the cutoff and *added*, so the combined coefficient reaches 2. One route replaces two, and its
///   reach has to cover their sum or the deepest sweeps go out of reach. Twice
///   [`crate::voice::FILTER_MOD_OCTAVES`] is also exactly the voice's own ±16-octave cutoff clamp.
///   A legacy patch converts as `(bus amount + dedicated amount) ÷ 2`.
/// - **The key carries one octave of cutoff per octave of keyboard**, which is what keyboard
///   tracking is, and what the retired `filter_keyboard_amount` meant. A legacy patch converts as
///   that amount unchanged.
///
/// **Every pair the Pro-One did not have takes the standard four octaves** — the performance sources,
/// noise, the gate, the amplifier envelope and Oscillator A — where they took the bus's eight.
pub const CUTOFF_SCALE: [f32; SOURCES] = {
    let mut table = [FRAME_SCALE * reach::OCTAVES; SOURCES];
    let mut i = 0;
    while i < BUS_PATHS.len() {
        table[BUS_PATHS[i]] = FRAME_SCALE * FILTER_MOD_OCTAVES;
        i += 1;
    }
    table[source::FILTER_ENVELOPE] = FRAME_SCALE * 2.0 * FILTER_MOD_OCTAVES;
    table[source::KEY] = FRAME_SCALE;
    table
};

/// The generous per-target bound, in the summed domain, before a target's own scale.
///
/// Each target applies its real limit where it matters — the voice clamps cutoff octaves, the
/// oscillator clamps width, `frequency` clamps semitones — so a tight bound here would silently
/// narrow what a player can reach. It exists to keep a runaway finite, not to shape a sound.
pub const SUM_BOUND: f32 = 64.0;

/// The routes **the machine itself wires**, present in the init patch **at zero depth**.
///
/// Five, and they come from two places. Two are the machine's own hard wiring: the dedicated
/// filter-envelope amount and the filter keyboard amount are knobs on the panel that always reach
/// the cutoff. The other three exist because `plugins/mxm-mono-pr1/control-map.json` claims them as
/// collection roles, and **a controller knob bound to an absent route does nothing at all** — an
/// absent pair contributes nothing whatever its amount holds, so claiming a role without wiring the
/// route would ship a dead knob.
///
/// The five legacy destination switches default to `Off`, so nothing else is wired: every amount
/// here is zero, a revealed route at zero depth is audible as nothing, and this instrument
/// therefore declares **no init deviation at all**.
pub const INIT_PRESENT: [(usize, usize); 5] = [
    (target::FILTER_CUTOFF, source::FILTER_ENVELOPE),
    (target::FILTER_CUTOFF, source::KEY),
    (target::FILTER_CUTOFF, source::LFO),
    (target::OSCILLATOR_A_FREQUENCY, source::LFO),
    (target::OSCILLATOR_A_PULSE_WIDTH, source::LFO),
];

/// The multiplier's own wiring — **present at full amount, not at zero**, and this instrument's one
/// init deviation.
///
/// `product`'s neutral is one, so a factor present at zero depth contributes a constant 1 and the
/// module would publish the same thing whatever the wheel did: the route's presence would be a lie.
/// Full amount is what makes `Mod bus × wheel` come out, which is the machine's own two-stage
/// routing and what a fresh instance must do. `plugins/mxm-mono-pr1/AGENTS.md` records the deviation.
pub const INIT_AT_FULL: [(usize, usize); 2] = [
    (target::WHEEL_BUS, source::MOD_BUS),
    (target::WHEEL_BUS, source::WHEEL),
];

/// How far the mod wheel bends both oscillators at Init, pushed all the way with the LFO at its
/// peak: a gentle vibrato.
pub const INIT_VIBRATO_SEMITONES: f32 = 0.5;

/// **The wheel's job at Init** (the owner, 2026-09-28: the wheel did nothing on a fresh instance):
/// the LFO into `Mod bus` at full, and the wheel-scaled bus into both oscillators' pitch at
/// [`INIT_VIBRATO_SEMITONES`], so pushing the wheel brings in vibrato, as on most synths. With the
/// wheel at rest `Wheel bus` is zero and the two pitch routes add nothing, so Init sounds as it did
/// — and every factory sound, each written against it, renders as it did.
pub const INIT_WHEEL: [(usize, usize, f32); 3] = [
    (target::MOD_BUS, source::LFO, 1.0),
    (
        target::OSCILLATOR_A_FREQUENCY,
        source::WHEEL_BUS,
        INIT_VIBRATO_SEMITONES / FREQUENCY_MOD_SEMITONES,
    ),
    (
        target::OSCILLATOR_B_FREQUENCY,
        source::WHEEL_BUS,
        INIT_VIBRATO_SEMITONES / FREQUENCY_MOD_SEMITONES,
    ),
];

/// Whether the init patch holds this route: [`INIT_PRESENT`], [`INIT_AT_FULL`] and
/// [`INIT_WHEEL`].
#[must_use]
pub const fn init_present(target: usize, source: usize) -> bool {
    let mut i = 0;
    while i < INIT_PRESENT.len() {
        if INIT_PRESENT[i].0 == target && INIT_PRESENT[i].1 == source {
            return true;
        }
        i += 1;
    }
    init_amount(target, source) != 0.0
}

/// The depth the init patch gives this route: one for [`INIT_AT_FULL`], [`INIT_WHEEL`]'s own, and
/// zero for everything else.
#[must_use]
pub const fn init_amount(target: usize, source: usize) -> f32 {
    let mut i = 0;
    while i < INIT_AT_FULL.len() {
        if INIT_AT_FULL[i].0 == target && INIT_AT_FULL[i].1 == source {
            return 1.0;
        }
        i += 1;
    }
    let mut j = 0;
    while j < INIT_WHEEL.len() {
        if INIT_WHEEL[j].0 == target && INIT_WHEEL[j].1 == source {
            return INIT_WHEEL[j].2;
        }
        j += 1;
    }
    0.0
}

/// Which sources are live into which targets, and how much of each.
///
/// **Presence is what the DSP reads.** An absent route contributes nothing whatever its amount
/// holds, which is what makes removing a source one parameter write and re-adding it restore the
/// depth the player last set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Routing {
    /// Per target, per source: whether that route exists.
    pub present: [[bool; SOURCES]; TARGETS],
    /// Per target, per source: how much, signed, as a **fraction of that route's scale**.
    ///
    /// The scale is applied by [`Graph::sum`] rather than folded in here, because
    /// `(amount × source) × scale` is the instruction sequence this voice executed before it had
    /// routing and the other order does not round the same way.
    pub amounts: [[f32; SOURCES]; TARGETS],
}

impl Default for Routing {
    fn default() -> Self {
        Self::new()
    }
}

impl Routing {
    /// Nothing routed anywhere.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            present: [[false; SOURCES]; TARGETS],
            amounts: [[0.0; SOURCES]; TARGETS],
        }
    }

    /// The machine's own wiring, at zero depth — [`INIT_PRESENT`]. What the init patch holds.
    #[must_use]
    pub const fn init() -> Self {
        let mut routing = Self::new();
        let mut i = 0;
        while i < INIT_PRESENT.len() {
            let (t, s) = INIT_PRESENT[i];
            routing.present[t][s] = true;
            i += 1;
        }
        let mut j = 0;
        while j < INIT_AT_FULL.len() {
            let (t, s) = INIT_AT_FULL[j];
            routing.present[t][s] = true;
            routing.amounts[t][s] = 1.0;
            j += 1;
        }
        let mut k = 0;
        while k < INIT_WHEEL.len() {
            let (t, s, amount) = INIT_WHEEL[k];
            routing.present[t][s] = true;
            routing.amounts[t][s] = amount;
            k += 1;
        }
        routing
    }
}

/// The voice's routing state: one frame, and one compacted list per target.
///
/// Compaction runs **once per processing interval**, not per sample, because topology is discrete
/// and changes only on a parameter event. The per-sample loop then runs over the live routes rather
/// than over all 125 pairs.
#[derive(Debug, Clone)]
pub struct Graph {
    frame: SourceFrame<SOURCES>,
    live: [Compacted<SOURCES>; TARGETS],
    /// Each uniform target's two halves for `mxm_modulation::sum_split`: the pairs on the machine's
    /// bus scale, and the added pairs on their standard reach ([`ADDED_SCALE`]).
    uniform: [Compacted<SOURCES>; TARGETS],
    added: [Compacted<SOURCES>; TARGETS],
    /// Which sources any live route actually reads, cached at [`Graph::set_topology`].
    ///
    /// **A source nothing reads is not published.** Publishing all fourteen would cost a finite
    /// check, a clamp and two array stores each for values no sum would look at; the init patch
    /// wires three sources into five routes.
    needed: [bool; SOURCES],
    /// Whether any route at all is live, cached at [`Graph::set_topology`].
    any: bool,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    /// An empty graph.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            frame: SourceFrame::new(),
            live: [const { Compacted::new() }; TARGETS],
            uniform: [const { Compacted::new() }; TARGETS],
            added: [const { Compacted::new() }; TARGETS],
            needed: [false; SOURCES],
            any: false,
        }
    }

    /// Rebuilds which routes are live. Call once per interval, never per sample.
    ///
    /// **Every path that can reach the render loop owes this** — the sampler's conversion shipped a
    /// voice that was never armed with the topology, so the first note after any allocation
    /// rendered with nothing routed.
    ///
    /// **A source that becomes needed starts from silence.** While nothing read it, nothing
    /// published it, so its slot still holds whatever it held the last time something did — which
    /// may be from a different phrase entirely. A backward route added to a running voice would
    /// then read that ancient value for exactly one sample. Clearing the slot makes the first
    /// sample a deterministic zero instead: bounded, the same however long the source went unread,
    /// and therefore independent of how the host split its buffers.
    pub fn set_topology(&mut self, routing: &Routing) {
        // The one pair this instrument refuses, enforced here so no caller can wire it by accident.
        let mut present = routing.present;
        let (bus, wheel) = WHEEL_IS_NOT_A_BUS_SOURCE;
        present[bus][wheel] = false;
        let routing = &Routing {
            present,
            ..*routing
        };
        for (target, present) in routing.present.iter().enumerate() {
            self.live[target].build(present);
            let mut uniform = [false; SOURCES];
            let mut added = [false; SOURCES];
            for (source, &on) in present.iter().enumerate() {
                let split = ADDED_SCALE[target][source].is_some();
                uniform[source] = on && !split;
                added[source] = on && split;
            }
            self.uniform[target].build(&uniform);
            self.added[target].build(&added);
        }
        self.any = self.live.iter().any(|l| !l.is_empty());
        let was_needed = self.needed;
        self.needed = [false; SOURCES];
        for present in routing.present.iter() {
            for (needed, &on) in self.needed.iter_mut().zip(present.iter()) {
                *needed |= on;
            }
        }
        for (source, (&needed, &before)) in self.needed.iter().zip(was_needed.iter()).enumerate() {
            if needed && !before {
                self.frame.clear(source);
            }
        }
    }

    /// Whether anything reads this source, so a caller can skip producing a value for it.
    #[inline]
    #[must_use]
    pub fn needs(&self, source: usize) -> bool {
        self.needed[source]
    }

    /// Whether anything is routed at all.
    #[inline]
    #[must_use]
    pub fn any_live(&self) -> bool {
        self.any
    }

    /// Opens a sample.
    #[inline]
    pub fn begin_sample(&mut self) {
        self.frame.begin_sample();
    }

    /// Publishes a source's **raw** value for this sample, scaled into the frame unit.
    ///
    /// The scaling is here rather than at every call site so that no caller can forget it, and it is
    /// exact because [`FRAME_UNIT`] is a power of two.
    #[inline]
    pub fn write(&mut self, source: usize, raw: f32) {
        if self.needed[source] {
            self.frame.write(source, raw * FRAME_UNIT);
        }
    }

    /// Publishes a value that is **already in frame units** — the two modules, which compose from
    /// sources that are themselves scaled.
    #[inline]
    pub fn write_in_frame_units(&mut self, source: usize, value: f32) {
        if self.needed[source] {
            self.frame.write(source, value);
        }
    }

    /// This target's summed modulation, in its own domain.
    ///
    /// Dispatches on [`UNIFORM_SCALE`]: a uniform column takes [`Graph::sum_uniform`], which keeps
    /// the legacy instruction sequence; cutoff takes the per-route column.
    ///
    /// **Not for [`PRODUCT_TARGET`].** Its entry in [`UNIFORM_SCALE`] is `None` for a different
    /// reason than cutoff's — it has no scale at all rather than a per-route one — so falling
    /// through to the cutoff column would silently sum the multiplier's factors against octave
    /// scales. [`Graph::product`] is its law; the debug assertion is what stops the two crossing.
    #[inline]
    #[must_use]
    pub fn sum(&self, target: usize, routing: &Routing) -> f32 {
        debug_assert_ne!(
            target, PRODUCT_TARGET,
            "the multiplier's law is product, not sum"
        );
        match UNIFORM_SCALE[target] {
            Some(scale) => self.sum_uniform(target, routing, scale),
            None => mxm_modulation::sum_scaled(
                &self.frame,
                &self.live[target],
                &routing.amounts[target],
                if target == target::AMPLITUDE {
                    &AMPLITUDE_SCALE
                } else {
                    &CUTOFF_SCALE
                },
                if target == target::AMPLITUDE {
                    AMPLITUDE_SUM_BOUND
                } else {
                    SUM_BOUND
                },
            ),
        }
    }

    /// The **multiplier module's** value, in raw domain — decision 1.14's law, in a scaled frame.
    ///
    /// Each factor is un-scaled back out of [`FRAME_UNIT`] before the law is applied, so the `1` in
    /// `1 + amount × (source − 1)` means *one raw unit* and every property the law is supposed to
    /// have survives: nothing present is neutral, a zero amount is neutral rather than
    /// annihilating, and two routes at full amount give the plain product of their sources.
    ///
    /// The caller publishes the result through [`Graph::write`], so the frame's own bound is what
    /// keeps a cycle through the multiplier finite.
    #[inline]
    #[must_use]
    pub fn product(&self, target: usize, routing: &Routing) -> f32 {
        debug_assert_eq!(
            target, PRODUCT_TARGET,
            "only the multiplier has a product law"
        );
        mxm_modulation::product_with_tops(
            &self.frame,
            &self.live[target],
            &routing.amounts[target],
            &PRODUCT_TOPS,
            FRAME_SCALE,
        )
    }

    /// A uniform-column target's sum: every route at scale one, the destination scale applied last.
    ///
    /// **Falsified before it was trusted**, which this crate's shared routing DOX asks for: replacing
    /// this with [`mxm_modulation::sum_scaled`] over a uniform column makes
    /// `tests/legacy_reachability.rs` go red in the last bits — `-6.270301` against `-6.2703`.
    ///
    /// `FRAME_SCALE * scale` is a compile-time constant product of a power of two with the
    /// destination scale, so it is itself exact, and one multiply against it undoes the frame unit
    /// and applies the destination scale together — which is the single multiply the legacy graph
    /// performed on its already-summed bus.
    #[inline]
    #[must_use]
    pub fn sum_uniform(&self, target: usize, routing: &Routing, scale: f32) -> f32 {
        // The machine's pairs on its bus scale, the added ones on theirs; while no added pair is
        // live this is the bus's own instruction sequence to the bit.
        let mut added_scales = [0.0; SOURCES];
        for (slot, scale) in added_scales.iter_mut().zip(ADDED_SCALE[target]) {
            *slot = scale.unwrap_or(0.0);
        }
        mxm_modulation::sum_split(
            &self.frame,
            &self.uniform[target],
            &routing.amounts[target],
            FRAME_SCALE * scale,
            &self.added[target],
            &added_scales,
            SUM_BOUND,
        )
    }

    /// Whether any route is live into that target, which is what lets a caller skip the work.
    #[inline]
    #[must_use]
    pub fn is_empty(&self, target: usize) -> bool {
        self.live[target].is_empty()
    }

    /// Clears the frame, leaving no tail between renders.
    ///
    /// Both halves: `previous` is state, so a reset clearing only `current` would let one render
    /// leak a sample into the next.
    pub fn reset(&mut self) {
        self.frame.reset();
    }

    /// What the frame holds for one source, **in raw units**, for tests.
    #[cfg(test)]
    pub(crate) fn read_for_test(&self, source: usize) -> f32 {
        self.frame.read(source) * FRAME_SCALE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bound the frame unit was sized against, and the argument for it written as arithmetic.
    ///
    /// This is `plan-mxm-mono-pr1-modulation.md` §3.2's finding as a test rather than as a claim:
    /// the legacy ±8 bus clamp was unreachable, so removing it puts no sound out of reach. It is
    /// here rather than in the plan because **a later change that raises a source's gain has to go
    /// red** instead of silently invalidating the conversion.
    #[test]
    fn the_frame_unit_contains_every_bus_the_hardware_could_wire() {
        // The three sources the legacy bus could sum, at amounts the hardware clamped to 0..=1.
        let legacy_worst = crate::lfo::SUM_BOUND + 1.0 + crate::oscillator::B_SUM_BOUND;
        assert!(
            legacy_worst < 8.0,
            "the legacy \u{b1}8 bus clamp was unreachable: worst case {legacy_worst}"
        );
        assert!(
            legacy_worst * FRAME_UNIT <= 1.0,
            "the frame unit must contain the composed bus, not just the raw sources: \
             {legacy_worst} \u{d7} {FRAME_UNIT} must not clamp"
        );
        // Every raw source must fit too, including the widest.
        for raw in [
            crate::lfo::SUM_BOUND,
            crate::oscillator::A_SUM_BOUND,
            crate::oscillator::B_SUM_BOUND,
            KEY_OCTAVES_ABOVE,
            KEY_OCTAVES_BELOW,
            1.0,
        ] {
            assert!(raw * FRAME_UNIT <= 1.0, "{raw} clamps at the frame unit");
        }
    }

    /// `FRAME_SCALE` must be exactly the inverse, or [`Graph::sum_uniform`] is not exact.
    #[test]
    fn the_frame_scale_is_the_units_exact_inverse() {
        assert_eq!(FRAME_UNIT * FRAME_SCALE, 1.0);
        assert_eq!(FRAME_UNIT.recip(), FRAME_SCALE);
    }

    /// Publication order and the forward table have to agree, or the unit delay means nothing.
    #[test]
    fn every_target_is_summed_after_a_declared_source() {
        for (&through, name) in FORWARD_THROUGH.iter().zip(TARGET_NAMES) {
            assert!(
                through < SOURCES,
                "{name} names a source that does not exist"
            );
        }
        // The module is summed after everything it reads forward and before the audio sources.
        assert_eq!(
            FORWARD_THROUGH[target::MOD_BUS],
            source::AMPLIFIER_ENVELOPE,
            "Mod bus reads the control sources forward and the audio sources backward"
        );
        // B's targets are summed before B renders: the deliberately-last commit.
        assert!(FORWARD_THROUGH[target::OSCILLATOR_B_FREQUENCY] < source::OSCILLATOR_B);
        // A's targets are summed after B, which is how B modulates A within the sample.
        assert!(FORWARD_THROUGH[target::OSCILLATOR_A_FREQUENCY] >= source::OSCILLATOR_B);
        assert!(FORWARD_THROUGH[target::OSCILLATOR_A_FREQUENCY] < source::OSCILLATOR_A);
    }

    /// Init wires the machine's own paths and every one of them at zero.
    #[test]
    fn the_init_patch_is_the_machines_own_wiring_and_nothing_is_wired_twice() {
        let routing = Routing::init();
        let mut count = 0;
        for (t, ((present, amounts), target)) in routing
            .present
            .iter()
            .zip(routing.amounts.iter())
            .zip(TARGET_NAMES)
            .enumerate()
        {
            for (sc, ((&on, &amount), name)) in present
                .iter()
                .zip(amounts.iter())
                .zip(SOURCE_NAMES)
                .enumerate()
            {
                if on {
                    count += 1;
                    // The multiplier's own two factors are at full — `product`'s neutral is one,
                    // so a factor at zero would make the module publish a constant — and the
                    // wheel's vibrato at its own depth; everything else at zero.
                    assert_eq!(amount, init_amount(t, sc), "{target} from {name}");
                    assert!(init_present(t, sc), "{target} from {name}");
                }
            }
        }
        assert_eq!(
            count,
            INIT_PRESENT.len() + INIT_AT_FULL.len() + INIT_WHEEL.len()
        );
        for (i, pair) in INIT_PRESENT.iter().enumerate() {
            assert!(
                !INIT_PRESENT[i + 1..].contains(pair),
                "{pair:?} is wired twice"
            );
        }
    }

    /// Cutoff and Amplitude are the non-uniform columns, and cutoff's machine entries that differ
    /// are the two the conversion collapsed.
    #[test]
    fn cutoff_and_amplitude_are_the_targets_with_a_per_route_scale() {
        for (t, (scale, name)) in UNIFORM_SCALE.iter().zip(TARGET_NAMES).enumerate() {
            // Cutoff and Amplitude have a per-route column; the multiplier has no scale at all,
            // because its law is product and `Graph::product` evaluates it.
            let expected =
                t == target::FILTER_CUTOFF || t == target::AMPLITUDE || t == PRODUCT_TARGET;
            assert_eq!(
                scale.is_none(),
                expected,
                "{name} disagrees about its scale column"
            );
        }
        assert_eq!(
            CUTOFF_SCALE[source::FILTER_ENVELOPE],
            2.0 * CUTOFF_SCALE[source::LFO],
            "the collapsed route must reach the dedicated and bus paths' sum"
        );
        assert_eq!(
            CUTOFF_SCALE[source::KEY],
            FRAME_SCALE,
            "one octave of cutoff per octave of keyboard"
        );
        for (s, &scale) in CUTOFF_SCALE.iter().enumerate() {
            if s == source::FILTER_ENVELOPE || s == source::KEY {
                continue;
            }
            // The bus paths keep the machine's reach; every other pair takes the standard's.
            let expected = if machine(target::FILTER_CUTOFF, s) {
                FILTER_MOD_OCTAVES
            } else {
                reach::OCTAVES
            };
            assert_eq!(scale, FRAME_SCALE * expected, "{}", SOURCE_NAMES[s]);
        }
    }

    /// An absent route contributes nothing whatever its amount holds, and a present one at zero is
    /// still a route. This is the property removal depends on.
    #[test]
    fn presence_decides_and_a_zero_amount_is_not_absence() {
        let mut routing = Routing::new();
        routing.amounts[target::FILTER_CUTOFF][source::LFO] = 1.0;
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        graph.begin_sample();
        graph.write(source::LFO, 1.0);
        assert_eq!(
            graph.sum(target::FILTER_CUTOFF, &routing),
            0.0,
            "an absent pair's amount must never be read"
        );

        routing.present[target::FILTER_CUTOFF][source::LFO] = true;
        graph.set_topology(&routing);
        graph.begin_sample();
        graph.write(source::LFO, 1.0);
        assert_eq!(
            graph.sum(target::FILTER_CUTOFF, &routing),
            FILTER_MOD_OCTAVES,
            "a present route at full amount, from a source at raw 1.0, reaches the destination scale"
        );

        routing.amounts[target::FILTER_CUTOFF][source::LFO] = 0.0;
        graph.begin_sample();
        graph.write(source::LFO, 1.0);
        assert_eq!(graph.sum(target::FILTER_CUTOFF, &routing), 0.0);
        assert!(
            !graph.is_empty(target::FILTER_CUTOFF),
            "a zero amount is not absence"
        );
    }

    /// The uniform form must reproduce `scale × Σ(amount × source)` **bit for bit**, because that
    /// is the expression the legacy graph evaluated and four destinations' digests depend on it.
    #[test]
    fn a_uniform_column_is_bit_identical_to_the_legacy_instruction_sequence() {
        let sources = [0.37, -0.91, 0.05];
        let amounts = [0.63, 0.29, 0.84];
        let mut routing = Routing::new();
        for (slot, &s) in [source::LFO, source::FILTER_ENVELOPE, source::OSCILLATOR_B]
            .iter()
            .enumerate()
        {
            routing.present[target::OSCILLATOR_A_FREQUENCY][s] = true;
            routing.amounts[target::OSCILLATOR_A_FREQUENCY][s] = amounts[slot];
        }
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        graph.begin_sample();
        for (slot, &s) in [source::LFO, source::FILTER_ENVELOPE, source::OSCILLATOR_B]
            .iter()
            .enumerate()
        {
            graph.write(s, sources[slot]);
        }

        // Exactly what `modulation::Router` did: accumulate in bus domain, then one destination
        // multiply. The iteration order is source order, which is the order the legacy array used.
        let mut bus = 0.0f32;
        for slot in 0..3 {
            bus += sources[slot] * amounts[slot];
        }
        let legacy = FREQUENCY_MOD_SEMITONES * bus;

        assert_eq!(
            graph
                .sum(target::OSCILLATOR_A_FREQUENCY, &routing)
                .to_bits(),
            legacy.to_bits(),
            "the uniform form moved a destination that must not move"
        );
    }

    /// **Decision 1.14's multiplier, and every property the law is supposed to have.**
    ///
    /// The obstacle this instrument hit was that `product`'s neutral is wrong in a scaled frame:
    /// the `1` in `1 + amount × (source − 1)` means one *frame* unit, which is eight raw units.
    /// `Graph::product` un-scales each factor first, so the `1` is one raw unit again. These are the
    /// properties that buys — if any of them fails, the multiplier is not the thing the owner
    /// specified.
    #[test]
    fn the_multiplier_keeps_its_law_in_a_scaled_frame() {
        let bus = PRODUCT_TARGET;
        let mut routing = Routing::new();
        let mut graph = Graph::new();

        // **Nothing present is neutral.** One, not zero, and not eight.
        graph.set_topology(&routing);
        graph.begin_sample();
        assert_eq!(graph.product(bus, &routing), 1.0);

        // **A zero amount is neutral, not annihilating.** Turning a factor down must not silence
        // the product.
        routing.present[bus][source::WHEEL] = true;
        routing.amounts[bus][source::WHEEL] = 0.0;
        graph.set_topology(&routing);
        graph.begin_sample();
        graph.write(source::WHEEL, 0.25);
        assert_eq!(graph.product(bus, &routing), 1.0);

        // **Two factors at full amount are the plain product of their raw values** — which is what
        // makes `Mod bus × wheel` fall out of the machine's own wiring.
        routing.amounts[bus][source::WHEEL] = 1.0;
        routing.present[bus][source::LFO] = true;
        routing.amounts[bus][source::LFO] = 1.0;
        graph.set_topology(&routing);
        graph.begin_sample();
        graph.write(source::WHEEL, 0.5);
        graph.write(source::LFO, 1.5);
        assert!(
            (graph.product(bus, &routing) - 0.75).abs() < 1e-6,
            "0.5 × 1.5 must be 0.75 in raw domain, not {} — the un-scaling is what buys this",
            graph.product(bus, &routing)
        );

        // **An amount blends between neutral and the source.** Half of a 0.5 factor is 0.75.
        routing.present[bus][source::LFO] = false;
        routing.amounts[bus][source::WHEEL] = 0.5;
        graph.set_topology(&routing);
        graph.begin_sample();
        graph.write(source::WHEEL, 0.5);
        assert!((graph.product(bus, &routing) - 0.75).abs() < 1e-6);

        // **A cycle through it stays finite**, because the result publishes through the frame's own
        // bound rather than escaping it.
        routing.present[bus][source::WHEEL_BUS] = true;
        routing.amounts[bus][source::WHEEL_BUS] = 1.0;
        routing.amounts[bus][source::WHEEL] = 1.0;
        graph.set_topology(&routing);
        for _ in 0..10_000 {
            graph.begin_sample();
            graph.write(source::WHEEL, 8.0);
            let product = graph.product(bus, &routing);
            graph.write(source::WHEEL_BUS, product);
            assert!(product.is_finite());
        }
        graph.begin_sample();
        assert!(graph.product(bus, &routing).abs() <= 64.0, "the cycle grew");
    }

    /// Deep multi-source routing into the module saturates at the publication bound rather than
    /// running away — the bound doing its job, not a defect.
    #[test]
    fn the_module_saturates_gracefully_past_the_legacy_bound() {
        let mut routing = Routing::new();
        for s in 0..SOURCES {
            routing.present[target::MOD_BUS][s] = true;
            routing.amounts[target::MOD_BUS][s] = 1.0;
        }
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        for _ in 0..64 {
            graph.begin_sample();
            for s in 0..SOURCES {
                if s != source::MOD_BUS && s != source::WHEEL_BUS {
                    graph.write(s, 8.0);
                }
            }
            let bus = graph.sum(target::MOD_BUS, &routing);
            graph.write_in_frame_units(source::MOD_BUS, bus);
            graph.write_in_frame_units(source::WHEEL_BUS, bus);
            assert!(bus.is_finite());
        }
        graph.begin_sample();
        assert!(
            graph.sum(target::MOD_BUS, &routing).abs() <= SUM_BOUND,
            "a fed-back module must stay bounded"
        );
    }
}
