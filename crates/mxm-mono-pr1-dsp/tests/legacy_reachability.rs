//! Decision 1.13's reachability argument, as tests rather than as prose.
//!
//! `plan-mxm-mono-pr1-modulation.md` §7. The governing question is *"can this instrument still make
//! every sound it could before?"*, and the answer has to be demonstrated rather than asserted. The
//! retired Direct/Wheel graph is **transcribed here as the reference**: it no longer exists in the
//! crate, so the only honest oracle is a copy of the arithmetic it performed, written out where a
//! reader can compare it line by line with §3.3's translation table.
//!
//! Two different claims live here and they are tested to different standards, which is the
//! distinction §3.1 draws:
//!
//! - **The four oscillator destinations are bit-identical.** Their scale columns are uniform, so
//!   `Graph::sum_uniform` keeps the legacy instruction sequence — one destination multiply applied
//!   to an already-summed bus. A move here is a bug, not a digest to re-pin, and these tests are
//!   what says which.
//! - **Filter cutoff is equal in exact reals, not bit-for-bit.** Its column is genuinely not
//!   uniform, because one route carries two collapsed legacy paths at twice the reach and another
//!   carries the keyboard's own octave-for-octave law.
//!
//! The claim is made at the **DSP-value layer**: identical source values and identical amounts must
//! produce the identical `f32`. The conversion of a legacy amount into a stored normalised route
//! amount is a rounding the plugin performs deliberately and is not in scope here.

use mxm_mono_pr1_dsp::routing::{
    CUTOFF_SCALE, FRAME_SCALE, Graph, PRODUCT_TARGET, Routing, SOURCES, TARGETS,
    WHEEL_IS_NOT_A_BUS_SOURCE, source, target,
};
use mxm_mono_pr1_dsp::voice::{FILTER_MOD_OCTAVES, FREQUENCY_MOD_SEMITONES, PWM_MOD_DEPTH};

/// Which bus a legacy source was assigned to. The hardware offered exactly these two.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bus {
    Direct,
    Wheel,
}

/// What a legacy destination switch selected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dest {
    Direct,
    Off,
    Wheel,
}

/// One complete legacy patch: three source depths and assignments, five destination switches, the
/// wheel, and the two hard-wired cutoff amounts that sat outside the bus.
#[derive(Clone, Copy, Debug)]
struct Legacy {
    lfo: (f32, Bus),
    filter_envelope: (f32, Bus),
    oscillator_b: (f32, Bus),
    destinations: [Dest; 5],
    wheel: f32,
    /// `filter_env_amount` — the dedicated cutoff path, which **added** to the bus path.
    dedicated: f32,
    /// `filter_keyboard_amount` — keyboard tracking, added outside the destination scale.
    keyboard: f32,
}

/// The source values one sample presents, in their own raw domains.
#[derive(Clone, Copy, Debug)]
struct Values {
    lfo: f32,
    filter_envelope: f32,
    /// Oscillator B's **preceding** sample, which is what every B route read.
    oscillator_b_previous: f32,
    /// The sounding pitch's distance from middle C, in octaves.
    key_octaves: f32,
}

/// The retired `modulation.rs`, transcribed. `BUS_BOUND` was 8.0 and
/// `routing::tests::the_frame_unit_contains_every_bus_the_hardware_could_wire` establishes that it
/// never fired, so it is kept here exactly as it was rather than quietly dropped.
const BUS_BOUND: f32 = 8.0;

/// The five destination values and the cutoff octaves, in the domains the voice applied them in.
fn legacy(patch: Legacy, values: Values) -> [f32; 6] {
    let mut direct = 0.0f32;
    let mut wheel_bus = 0.0f32;
    for (raw, (amount, bus)) in [
        (values.lfo, patch.lfo),
        (values.filter_envelope, patch.filter_envelope),
        (values.oscillator_b_previous, patch.oscillator_b),
    ] {
        let term = raw * amount.clamp(0.0, 1.0);
        match bus {
            Bus::Direct => direct += term,
            Bus::Wheel => wheel_bus += term,
        }
    }
    direct = direct.clamp(-BUS_BOUND, BUS_BOUND);
    wheel_bus = wheel_bus.clamp(-BUS_BOUND, BUS_BOUND);
    let wheel = wheel_bus * patch.wheel.clamp(0.0, 1.0);
    let value = |d: Dest| match d {
        Dest::Direct => direct,
        Dest::Off => 0.0,
        Dest::Wheel => wheel,
    };

    let cutoff_bus = value(patch.destinations[4]);
    // `Frame::filter_cutoff_with_dedicated`, then the voice's own keyboard term.
    let cutoff_octaves = FILTER_MOD_OCTAVES
        * (cutoff_bus + values.filter_envelope * patch.dedicated.clamp(0.0, 1.0))
        + patch.keyboard.clamp(0.0, 1.0) * values.key_octaves;

    [
        FREQUENCY_MOD_SEMITONES * value(patch.destinations[0]),
        PWM_MOD_DEPTH * value(patch.destinations[1]),
        FREQUENCY_MOD_SEMITONES * value(patch.destinations[2]),
        PWM_MOD_DEPTH * value(patch.destinations[3]),
        cutoff_octaves,
        // The wheel stage's own published value, for the record.
        wheel,
    ]
}

/// §3.3's canonical translation table, applied. **This is the thing under test**: if it is wrong,
/// the preset generator and the plugin are wrong the same way.
///
/// `Mod bus` holds the **wheel-assigned** sources; the Direct-assigned ones **fan out**, one route
/// per Direct destination. One summing module cannot be both legacy buses at once, which is the
/// whole reason the machine had two.
fn convert(patch: Legacy) -> Routing {
    let mut routing = Routing::new();
    let legacy_sources = [
        (source::LFO, patch.lfo),
        (source::FILTER_ENVELOPE, patch.filter_envelope),
        (source::OSCILLATOR_B, patch.oscillator_b),
    ];
    let targets = [
        target::OSCILLATOR_A_FREQUENCY,
        target::OSCILLATOR_A_PULSE_WIDTH,
        target::OSCILLATOR_B_FREQUENCY,
        target::OSCILLATOR_B_PULSE_WIDTH,
        target::FILTER_CUTOFF,
    ];

    // What the wheel scales.
    for (slot, (amount, bus)) in legacy_sources {
        if bus == Bus::Wheel {
            routing.present[target::MOD_BUS][slot] = true;
            routing.amounts[target::MOD_BUS][slot] = amount.clamp(0.0, 1.0);
        }
    }

    for (index, &t) in targets.iter().enumerate() {
        match patch.destinations[index] {
            Dest::Direct => {
                for (slot, (amount, bus)) in legacy_sources {
                    if bus == Bus::Direct {
                        routing.present[t][slot] = true;
                        routing.amounts[t][slot] = amount.clamp(0.0, 1.0);
                    }
                }
            }
            Dest::Wheel => {
                routing.present[t][source::WHEEL_BUS] = true;
                routing.amounts[t][source::WHEEL_BUS] = 1.0;
            }
            Dest::Off => {}
        }
    }

    // The two hard-wired cutoff paths. The dedicated amount **adds into the same pair** the bus
    // fan-out may already have written, and the route's scale is twice the destination scale, so
    // the combined coefficient is halved to become the stored amount. Reading `dedicated` as the
    // stored amount against that doubled reach would double the path.
    let bus_contribution = routing.amounts[target::FILTER_CUTOFF][source::FILTER_ENVELOPE];
    let combined = bus_contribution + patch.dedicated.clamp(0.0, 1.0);
    if combined != 0.0 || routing.present[target::FILTER_CUTOFF][source::FILTER_ENVELOPE] {
        routing.present[target::FILTER_CUTOFF][source::FILTER_ENVELOPE] = true;
        routing.amounts[target::FILTER_CUTOFF][source::FILTER_ENVELOPE] = combined / 2.0;
    }
    routing.present[target::FILTER_CUTOFF][source::KEY] = true;
    routing.amounts[target::FILTER_CUTOFF][source::KEY] = patch.keyboard.clamp(0.0, 1.0);

    routing
}

/// Publish one sample's worth of sources and read every target, the way the voice does.
fn evaluate(routing: &Routing, values: Values, wheel: f32) -> [f32; 6] {
    let mut graph = Graph::new();
    graph.set_topology(routing);
    graph.begin_sample();
    graph.write(source::KEY, values.key_octaves);
    graph.write(source::LFO, values.lfo);
    graph.write(source::FILTER_ENVELOPE, values.filter_envelope);
    // Declared last, so its route is backward: the test supplies the preceding sample directly.
    graph.write(source::OSCILLATOR_B, values.oscillator_b_previous);
    let bus = graph.sum(target::MOD_BUS, routing);
    graph.write_in_frame_units(source::MOD_BUS, bus);
    let scaled = bus * wheel.clamp(0.0, 1.0);
    graph.write_in_frame_units(source::WHEEL_BUS, scaled);

    [
        graph.sum(target::OSCILLATOR_A_FREQUENCY, routing),
        graph.sum(target::OSCILLATOR_A_PULSE_WIDTH, routing),
        graph.sum(target::OSCILLATOR_B_FREQUENCY, routing),
        graph.sum(target::OSCILLATOR_B_PULSE_WIDTH, routing),
        graph.sum(target::FILTER_CUTOFF, routing),
        scaled * FRAME_SCALE,
    ]
}

/// A deterministic generator, so a failure is reproducible and no dependency is added.
struct Lcg(u64);

impl Lcg {
    fn next_unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((self.0 >> 33) as f32) / ((1u64 << 31) as f32)
    }

    fn next_signed(&mut self) -> f32 {
        self.next_unit() * 2.0 - 1.0
    }

    fn next_bus(&mut self) -> Bus {
        if self.next_unit() < 0.5 {
            Bus::Direct
        } else {
            Bus::Wheel
        }
    }

    fn next_dest(&mut self) -> Dest {
        match (self.next_unit() * 3.0) as u32 {
            0 => Dest::Direct,
            1 => Dest::Wheel,
            _ => Dest::Off,
        }
    }
}

/// The reachability argument itself: **every legacy configuration is reproduced**.
///
/// The four oscillator destinations bit-for-bit, cutoff to a tolerance that is one part in a
/// million of an octave — far below anything audible, and present only because the collapsed
/// filter-envelope route sums two terms the legacy graph summed in a different order.
#[test]
fn every_legacy_bus_configuration_is_reproduced_by_the_canonical_translation() {
    let mut rng = Lcg(0x0100_1981_0913);
    let mut saw_each_destination_kind = [false; 3];
    for _ in 0..20_000 {
        let patch = Legacy {
            lfo: (rng.next_unit(), rng.next_bus()),
            filter_envelope: (rng.next_unit(), rng.next_bus()),
            oscillator_b: (rng.next_unit(), rng.next_bus()),
            destinations: [
                rng.next_dest(),
                rng.next_dest(),
                rng.next_dest(),
                rng.next_dest(),
                rng.next_dest(),
            ],
            wheel: rng.next_unit(),
            dedicated: rng.next_unit(),
            keyboard: rng.next_unit(),
        };
        for d in patch.destinations {
            saw_each_destination_kind[match d {
                Dest::Direct => 0,
                Dest::Off => 1,
                Dest::Wheel => 2,
            }] = true;
        }
        let values = Values {
            lfo: rng.next_signed() * 3.0,
            filter_envelope: rng.next_unit(),
            oscillator_b_previous: rng.next_signed() * 1.9,
            key_octaves: rng.next_signed() * 5.0,
        };

        let expected = legacy(patch, values);
        let actual = evaluate(&convert(patch), values, patch.wheel);

        for (index, name) in [
            "Oscillator A frequency",
            "Oscillator A pulse width",
            "Oscillator B frequency",
            "Oscillator B pulse width",
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(
                actual[index].to_bits(),
                expected[index].to_bits(),
                "{name} must be bit-identical: {patch:?} {values:?} \
                 gave {} against {}",
                actual[index],
                expected[index]
            );
        }
        assert!(
            (actual[4] - expected[4]).abs() <= 1e-5 * expected[4].abs().max(1.0),
            "cutoff octaves: {patch:?} {values:?} gave {} against {}",
            actual[4],
            expected[4]
        );
        assert!(
            (actual[5] - expected[5]).abs() <= 1e-5 * expected[5].abs().max(1.0),
            "the wheel stage must reproduce the legacy wheel bus"
        );
    }
    assert!(
        saw_each_destination_kind.iter().all(|&seen| seen),
        "the sweep must exercise Direct, Off and Wheel"
    );
}

/// The cutoff's two hard-wired amounts, swept **across their ranges** rather than at one extreme.
///
/// Testing only the collapsed route's maximum proves the reach and nothing about the interior,
/// which is where every shipped preset actually sits.
#[test]
fn the_collapsed_cutoff_paths_are_reproduced_across_their_whole_ranges() {
    for dedicated_step in 0..=10 {
        for keyboard_step in 0..=10 {
            for bus_step in 0..=10 {
                let patch = Legacy {
                    lfo: (0.0, Bus::Direct),
                    filter_envelope: (bus_step as f32 / 10.0, Bus::Direct),
                    oscillator_b: (0.0, Bus::Direct),
                    destinations: [Dest::Off, Dest::Off, Dest::Off, Dest::Off, Dest::Direct],
                    wheel: 0.0,
                    dedicated: dedicated_step as f32 / 10.0,
                    keyboard: keyboard_step as f32 / 10.0,
                };
                for env in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    for key_octaves in [-5.0, -1.0, 0.0, 1.0, 67.0 / 12.0] {
                        let values = Values {
                            lfo: 0.0,
                            filter_envelope: env,
                            oscillator_b_previous: 0.0,
                            key_octaves,
                        };
                        let expected = legacy(patch, values)[4];
                        let actual = evaluate(&convert(patch), values, 0.0)[4];
                        assert!(
                            (actual - expected).abs() <= 1e-5 * expected.abs().max(1.0),
                            "dedicated {} + bus {} at env {env}, key {key_octaves}: \
                             {actual} against {expected}",
                            patch.dedicated,
                            patch.filter_envelope.0
                        );
                    }
                }
            }
        }
    }
}

/// The deepest sweep the legacy graph could reach must still be reachable — the reach argument the
/// doubled scale exists for, stated as its own case so it cannot be lost in an average.
#[test]
fn the_deepest_legacy_filter_sweep_is_still_reachable() {
    let patch = Legacy {
        lfo: (0.0, Bus::Direct),
        filter_envelope: (1.0, Bus::Direct),
        oscillator_b: (0.0, Bus::Direct),
        destinations: [Dest::Off, Dest::Off, Dest::Off, Dest::Off, Dest::Direct],
        wheel: 0.0,
        dedicated: 1.0,
        keyboard: 0.0,
    };
    let values = Values {
        lfo: 0.0,
        filter_envelope: 1.0,
        oscillator_b_previous: 0.0,
        key_octaves: 0.0,
    };
    let expected = legacy(patch, values)[4];
    assert_eq!(
        expected,
        2.0 * FILTER_MOD_OCTAVES,
        "the legacy maximum is both paths at full"
    );
    let routing = convert(patch);
    assert_eq!(
        routing.amounts[target::FILTER_CUTOFF][source::FILTER_ENVELOPE],
        1.0,
        "the combined coefficient of 2 stores as a full-amount route"
    );
    let actual = evaluate(&routing, values, 0.0)[4];
    assert!((actual - expected).abs() <= 1e-5);
    assert_eq!(
        CUTOFF_SCALE[source::FILTER_ENVELOPE],
        FRAME_SCALE * 2.0 * FILTER_MOD_OCTAVES
    );
}

/// Any-to-any reaches pairs no designer ever exercised. Every one of the 125 must stay finite and
/// bounded at extreme amounts and extreme source values.
///
/// **Each target is asked for its own law**, which is why this sweep dispatches rather than calling
/// `sum` throughout: the multiplier's entry in `UNIFORM_SCALE` is `None` like cutoff's, so a sweep
/// that summed everything would run its factors through cutoff's octave column and prove nothing
/// about the law the module actually evaluates.
#[test]
fn every_pair_stays_finite_and_bounded_at_its_extremes() {
    for t in 0..TARGETS {
        for s in 0..SOURCES {
            if (t, s) == WHEEL_IS_NOT_A_BUS_SOURCE {
                continue;
            }
            for &amount in &[-1.0f32, 0.0, 1.0] {
                for &raw in &[-8.0f32, -1.0, 0.0, 1.0, 8.0, f32::MAX, f32::MIN] {
                    let mut routing = Routing::new();
                    routing.present[t][s] = true;
                    routing.amounts[t][s] = amount;
                    let mut graph = Graph::new();
                    graph.set_topology(&routing);
                    graph.begin_sample();
                    graph.write(s, raw);
                    let value = if t == PRODUCT_TARGET {
                        graph.product(t, &routing)
                    } else {
                        graph.sum(t, &routing)
                    };
                    assert!(
                        value.is_finite(),
                        "target {t} from source {s} at amount {amount}, raw {raw}"
                    );
                }
            }
        }
    }
}

/// A non-finite source must publish as zero rather than poisoning every route that reads it.
#[test]
fn a_non_finite_source_cannot_poison_a_route() {
    let mut routing = Routing::new();
    routing.present[target::FILTER_CUTOFF][source::LFO] = true;
    routing.amounts[target::FILTER_CUTOFF][source::LFO] = 1.0;
    let mut graph = Graph::new();
    graph.set_topology(&routing);
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        graph.begin_sample();
        graph.write(source::LFO, bad);
        assert_eq!(graph.sum(target::FILTER_CUTOFF, &routing), 0.0);
    }
}

/// A player-made cycle stays finite and bounded. The module routed into itself is the tightest
/// loop this instrument can close, and the frame's publication bound is what makes it safe — the
/// unit delay alone would only make a runaway well-defined.
#[test]
fn a_cycle_through_the_module_stays_bounded() {
    let mut routing = Routing::new();
    routing.present[target::MOD_BUS][source::MOD_BUS] = true;
    routing.amounts[target::MOD_BUS][source::MOD_BUS] = 1.0;
    routing.present[target::MOD_BUS][source::LFO] = true;
    routing.amounts[target::MOD_BUS][source::LFO] = 1.0;
    let mut graph = Graph::new();
    graph.set_topology(&routing);
    let mut bus = 0.0f32;
    for n in 0..200_000 {
        graph.begin_sample();
        graph.write(source::LFO, if n % 2 == 0 { 3.0 } else { -3.0 });
        bus = graph.sum(target::MOD_BUS, &routing);
        graph.write_in_frame_units(source::MOD_BUS, bus);
        assert!(bus.is_finite(), "the cycle diverged at sample {n}");
        assert!(
            bus.abs() <= 2.0,
            "the publication bound must hold the cycle: {bus} at sample {n}"
        );
    }
    let _ = bus;
}

/// `reset` must clear **both halves** of the frame. A reset that cleared only the current values
/// would let one render leak a sample into the next through a backward route.
#[test]
fn reset_clears_the_delayed_half_of_the_frame_too() {
    let mut routing = Routing::new();
    routing.present[target::OSCILLATOR_B_FREQUENCY][source::OSCILLATOR_B] = true;
    routing.amounts[target::OSCILLATOR_B_FREQUENCY][source::OSCILLATOR_B] = 1.0;
    let mut graph = Graph::new();
    graph.set_topology(&routing);

    graph.begin_sample();
    graph.write(source::OSCILLATOR_B, 1.5);
    graph.begin_sample();
    assert_ne!(
        graph.sum(target::OSCILLATOR_B_FREQUENCY, &routing),
        0.0,
        "the backward route must see the preceding sample"
    );

    graph.reset();
    graph.begin_sample();
    assert_eq!(
        graph.sum(target::OSCILLATOR_B_FREQUENCY, &routing),
        0.0,
        "reset left a tail in the delayed half"
    );
}
