//! mxm-mono-pr1's routing as the collection's modulation standard checks it
//! (`mxm_modulation::conformance`; `plans/plan-modulation-standard.md`).
//!
//! Behind the `conformance` feature, which only `[dev-dependencies]` enable — this crate's own
//! tests, and the plugin's, whose route readings are held to [`Declared::deliver`] — so no shipped
//! graph carries it. [`Declared`] answers every question through [`crate::routing`]'s own tables
//! and a real [`Graph`], its frame unit included, never a copy of them.

use mxm_modulation::conformance::{Declaration, Kind};
use mxm_modulation::standard::{self, Law, Offer, Performance};

use crate::routing::{
    self, Graph, KEY_UNIT_SEMITONES, PRODUCT_TARGET, Routing, SOURCE_NAMES, SOURCES, TARGET_NAMES,
    TARGETS, WHEEL_IS_NOT_A_BUS_SOURCE, target,
};

/// What each target is, for the standard.
const KINDS: [Kind; TARGETS] = {
    let mut kinds = [Kind::Pitch; TARGETS];
    kinds[target::OSCILLATOR_A_PULSE_WIDTH] = Kind::Width;
    kinds[target::OSCILLATOR_B_PULSE_WIDTH] = Kind::Width;
    kinds[target::FILTER_CUTOFF] = Kind::Cutoff;
    kinds[target::RESONANCE] = Kind::Control;
    kinds[target::LFO_RATE] = Kind::Rate;
    // The two modules are the machine's own: a bus in frame units, and a multiplier.
    kinds[target::MOD_BUS] = Kind::Machine(Law::Sum);
    kinds[target::WHEEL_BUS] = Kind::Machine(Law::Product);
    kinds[target::AMPLITUDE] = Kind::Amplitude;
    kinds
};

/// mxm-mono-pr1's routing declaration.
#[derive(Debug, Clone, Copy, Default)]
pub struct Declared;

/// Exactly one route, at `amount`.
fn one_route(target: usize, source: usize, amount: f32) -> Routing {
    let mut routing = Routing::new();
    routing.present[target][source] = true;
    routing.amounts[target][source] = amount;
    routing
}

impl Declaration for Declared {
    fn sources(&self) -> usize {
        SOURCES
    }

    fn targets(&self) -> usize {
        TARGETS
    }

    fn performance(&self, source: usize) -> Option<Performance> {
        routing::PERFORMANCE[source]
    }

    fn kind(&self, target: usize) -> Kind {
        KINDS[target]
    }

    fn machine(&self, target: usize, source: usize) -> bool {
        routing::machine(target, source)
    }

    fn offered(&self, target: usize, source: usize) -> Offer {
        routing::offer(target, source)
    }

    fn key_unit(&self) -> f32 {
        KEY_UNIT_SEMITONES
    }

    /// One route alone through a real [`Graph`] — published through the frame unit, summed through
    /// the split or the per-route column; Amplitude through the factor the voice applies, and the
    /// multiplier as its factor's change.
    fn deliver(&self, target: usize, source: usize, amount: f32, raw: f32) -> f32 {
        let routing = one_route(target, source, amount);
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        graph.begin_sample();
        graph.write(source, raw);
        if target == PRODUCT_TARGET {
            graph.product(target, &routing) - 1.0
        } else if target == target::AMPLITUDE {
            standard::amplitude_factor(graph.sum(target, &routing)) - 1.0
        } else {
            graph.sum(target, &routing)
        }
    }

    fn name(&self, target: usize, source: usize) -> String {
        format!("{} from {}", TARGET_NAMES[target], SOURCE_NAMES[source])
    }

    /// The wheel into its own bus, which the multiplier already means (the owner, 2026-09-14).
    fn ruled_out(&self, target: usize, source: usize) -> bool {
        (target, source) == WHEEL_IS_NOT_A_BUS_SOURCE
    }
}

#[cfg(test)]
mod tests {
    use mxm_modulation::conformance::{self, Case, Input};

    use super::*;
    use crate::control::{Controller, Event, Inputs, Params as ControlParams};
    use crate::keyboard::NoteId;
    use crate::routing::source;
    use crate::voice::{Params, Voice};

    fn report(result: Result<(), Vec<String>>) {
        if let Err(failures) = result {
            panic!("{} failure(s):\n{}", failures.len(), failures.join("\n"));
        }
    }

    fn press(key: u8, velocity: f32) -> Event {
        Event::NoteOn(NoteId {
            voice_id: None,
            channel: 0,
            key,
            velocity,
        })
    }

    /// **Every pair means what the standard says**: offered as `standard::offer` says (the ruled-out
    /// wheel into its own bus refused), nothing at a source's rest, a meaningful move at full, and
    /// the standard reach for every pair the Pro-One did not have.
    ///
    /// Falsified before trusted: with the added oscillator pitch left at the bus's 24 semitones, it
    /// names every added pitch pair from a performance source.
    #[test]
    fn every_pair_means_what_the_standard_says() {
        report(conformance::check_declaration(&Declared));
    }

    /// **The controller publishes what the standard says**, in raw units: Key from middle C in
    /// octaves, Velocity as `v − 1`, the gestures as they arrive, each exactly zero at rest.
    ///
    /// Falsified before trusted: publishing the raw velocity fails at every input.
    #[test]
    fn the_controller_publishes_what_the_standard_says() {
        report(conformance::check_publishers(&Declared, |from, input| {
            let mut routing = Routing::new();
            routing.present[target::FILTER_CUTOFF][from] = true;
            let mut controller = Controller::new();
            controller.set_topology(&routing);
            let (key, velocity, gesture) = match input {
                Input::Note(n) => (n, 1.0, None),
                Input::Normalised(value) if from == source::VELOCITY => (60, value, None),
                Input::Normalised(value) if from == source::WHEEL => {
                    (60, 1.0, Some(Event::ModWheel { channel: 0, value }))
                }
                Input::Normalised(value) => {
                    (60, 1.0, Some(Event::ChannelPressure { channel: 0, value }))
                }
                Input::Lever(normalized) => (
                    60,
                    1.0,
                    Some(Event::PitchBend {
                        channel: 0,
                        normalized,
                    }),
                ),
            };
            let mut events = vec![press(key, velocity)];
            events.extend(gesture);
            // The bend must not reach the key: it is published at a zero bend range.
            let params = ControlParams {
                bend_range_semitones: 0.0,
                ..ControlParams::default()
            };
            controller.process_sample(params, Inputs::default(), &events);
            controller.published_for_test(from)
        }));
    }

    /// **Velocity is the press that last triggered the envelopes**: under Normal, a legato press
    /// sounds its key without retriggering and keeps the phrase's; under Retrig every press
    /// retriggers and brings its own.
    ///
    /// Falsified before trusted: publishing the sounding press's velocity reads the legato press's
    /// under Normal.
    #[test]
    fn velocity_is_the_press_that_last_triggered_the_envelopes() {
        use crate::keyboard::KeyMode;
        for (key_mode, expected) in [(KeyMode::Normal, -0.75), (KeyMode::Retrig, -0.125)] {
            let mut routing = Routing::new();
            routing.present[target::FILTER_CUTOFF][source::VELOCITY] = true;
            let mut controller = Controller::new();
            controller.set_topology(&routing);
            let params = ControlParams {
                key_mode,
                ..ControlParams::default()
            };
            controller.process_sample(params, Inputs::default(), &[press(60, 0.25)]);
            assert_eq!(controller.published_for_test(source::VELOCITY), -0.75);
            // Below the held key, so it sounds under either mode's priority.
            controller.process_sample(params, Inputs::default(), &[press(48, 0.875)]);
            assert_eq!(
                controller.published_for_test(source::VELOCITY),
                expected,
                "{key_mode:?}"
            );
        }
    }

    /// **Velocity rests at full unless a press triggered the envelopes**: a fresh Drone or Repeat,
    /// playing with no key, publishes rest — never the absent owner's zero — and a press under
    /// either triggers nothing and changes nothing; a reset rests again; after All Sound Off the
    /// re-arming press brings its own.
    ///
    /// Falsified before trusted: latching whenever the gate was closed publishes `−1` from the
    /// first Drone sample.
    #[test]
    fn velocity_rests_until_a_press_triggers_the_envelopes() {
        let mut routing = Routing::new();
        routing.present[target::FILTER_CUTOFF][source::VELOCITY] = true;
        for (repeat, drone) in [(false, true), (true, false)] {
            let mut controller = Controller::new();
            controller.set_topology(&routing);
            let params = ControlParams {
                repeat,
                drone,
                ..ControlParams::default()
            };
            for _ in 0..48_000 {
                controller.process_sample(params, Inputs::default(), &[]);
                assert_eq!(
                    controller.published_for_test(source::VELOCITY),
                    0.0,
                    "repeat {repeat}, drone {drone}: no press has triggered anything"
                );
            }
            controller.process_sample(params, Inputs::default(), &[press(60, 0.25)]);
            for _ in 0..48_000 {
                controller.process_sample(params, Inputs::default(), &[]);
            }
            assert_eq!(
                controller.published_for_test(source::VELOCITY),
                0.0,
                "repeat {repeat}, drone {drone}: the clock triggers, not the press"
            );
        }

        let mut controller = Controller::new();
        controller.set_topology(&routing);
        let params = ControlParams::default();
        controller.process_sample(params, Inputs::default(), &[press(60, 0.25)]);
        assert_eq!(controller.published_for_test(source::VELOCITY), -0.75);
        controller.reset();
        controller.set_topology(&routing);
        controller.process_sample(params, Inputs::default(), &[]);
        assert_eq!(
            controller.published_for_test(source::VELOCITY),
            0.0,
            "reset rests"
        );

        controller.process_sample(params, Inputs::default(), &[press(60, 0.25)]);
        controller.process_sample(params, Inputs::default(), &[Event::AllSoundOff]);
        controller.process_sample(params, Inputs::default(), &[press(64, 0.5)]);
        assert_eq!(
            controller.published_for_test(source::VELOCITY),
            -0.5,
            "the press that re-arms after All Sound Off triggers, and brings its own"
        );

        // All Sound Off forgets the phrase: re-armed by Drone or Repeat with no press, it rests.
        for (repeat, drone) in [(false, true), (true, false)] {
            let mut controller = Controller::new();
            controller.set_topology(&routing);
            let quiet = ControlParams::default();
            controller.process_sample(quiet, Inputs::default(), &[press(60, 0.25)]);
            controller.process_sample(
                quiet,
                Inputs::default(),
                &[Event::NoteOff {
                    voice_id: None,
                    channel: 0,
                    key: 60,
                }],
            );
            controller.process_sample(quiet, Inputs::default(), &[Event::AllSoundOff]);
            let autonomous = ControlParams {
                repeat,
                drone,
                ..ControlParams::default()
            };
            for _ in 0..48_000 {
                controller.process_sample(autonomous, Inputs::default(), &[]);
            }
            assert_eq!(
                controller.published_for_test(source::VELOCITY),
                0.0,
                "repeat {repeat}, drone {drone}: re-armed after All Sound Off with no press"
            );
        }
    }

    /// **The Amplitude target is the standard factor on the VCA's output**: a route at −100 % from
    /// a wheel at full silences the voice exactly, and at +100 % doubles it, sample for sample.
    ///
    /// Falsified before trusted: with the voice's factor removed, the doubled render equals the
    /// plain one.
    #[test]
    fn amplitude_is_the_standard_factor_after_the_vca() {
        let render = |amount: Option<f32>| {
            let mut routing = Routing::init();
            if let Some(amount) = amount {
                routing.present[target::AMPLITUDE][source::WHEEL] = true;
                routing.amounts[target::AMPLITUDE][source::WHEEL] = amount;
            }
            let mut voice = Voice::new();
            voice.set_topology(&routing);
            let params = Params::init();
            let mut out = vec![
                voice
                    .process_sample(
                        params,
                        &[
                            press(60, 1.0),
                            Event::ModWheel {
                                channel: 0,
                                value: 1.0,
                            },
                        ],
                    )
                    .output,
            ];
            out.extend((0..2_400).map(|_| voice.process_sample(params, &[]).output));
            out
        };
        let plain = render(None);
        assert!(
            plain.iter().any(|s| s.abs() > 1e-3),
            "the premise: it sounds"
        );
        assert!(render(Some(-1.0)).iter().all(|&s| s == 0.0), "silence");
        for (doubled, plain) in render(Some(1.0)).iter().zip(&plain) {
            assert_eq!(*doubled, plain * 2.0);
        }
    }

    /// **After a release, no performance route holds a note open** — every pair, both halves, the
    /// softest and hardest notes and the keyboard's ends, gestures held at full through the note and
    /// let go at the release.
    #[test]
    fn after_a_release_no_performance_route_holds_a_note_open() {
        report(conformance::check_release_silence(
            &Declared,
            &[],
            |case: Case| {
                let mut routing = Routing::new();
                routing.present[case.target][case.source] = true;
                routing.amounts[case.target][case.source] = case.amount;
                let mut voice = Voice::new();
                voice.set_topology(&routing);
                let mut params = Params::init();
                params.control.amplifier_envelope.release_s = 0.02;
                params.control.filter_envelope.release_s = 0.02;
                voice.process_sample(
                    params,
                    &[
                        press(case.key, case.velocity),
                        Event::ModWheel {
                            channel: 0,
                            value: 1.0,
                        },
                        Event::ChannelPressure {
                            channel: 0,
                            value: 1.0,
                        },
                        Event::PitchBend {
                            channel: 0,
                            normalized: 1.0,
                        },
                    ],
                );
                for _ in 0..4_800 {
                    voice.process_sample(params, &[]);
                }
                voice.process_sample(
                    params,
                    &[
                        Event::NoteOff {
                            voice_id: None,
                            channel: 0,
                            key: case.key,
                        },
                        Event::ModWheel {
                            channel: 0,
                            value: 0.0,
                        },
                        Event::ChannelPressure {
                            channel: 0,
                            value: 0.0,
                        },
                        Event::PitchBend {
                            channel: 0,
                            normalized: 0.0,
                        },
                    ],
                );
                (0..48_000).any(|_| {
                    let frame = voice.process_sample(params, &[]);
                    frame.output == 0.0 && frame.activity == crate::control::Activity::Inert
                })
            },
        ));
    }
}
