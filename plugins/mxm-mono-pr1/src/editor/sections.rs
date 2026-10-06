//! Stable card bodies and complete parameter bindings from the approved brief.
//!
//! Each card is a `mxm_ui::tree` (plans/plan-layout-tree.md): [`card`] describes its body once, and
//! that one description is both measured — its floor and its height — and drawn, leaf by leaf,
//! through the bindings below ([`paint`]). Nothing is typed and nothing is drawn to learn a size.

use egui::Ui;
use mxm_ui::{
    control::{Size, Wave},
    space::{SPACE_3, SPACE_5},
    theme::Tokens,
    tree::{self, Height, Kind, Node, Share, leaf, pad, share, stack},
    visual::{PLOT_HEIGHT, STATUS_HEIGHT, TALL_PLOT_HEIGHT},
};
use nice_plug::prelude::ParamSetter;
use std::collections::HashMap;

use super::{
    Section,
    binding::{Bound, segmented_named, toggle_labelled, toggle_picture},
    visuals,
};
use crate::{params::MxmMonoPr1Params, telemetry::Telemetry};
use mxm_mono_pr1_dsp::routing::{TARGET_NAMES, target};

/// One target's routes, as the routing widget lists them: the summing module's thirteen, or an
/// ordinary target's fourteen.
fn target_routes(p: &MxmMonoPr1Params, which: usize) -> Vec<mxm_modulation_params::Route<'_>> {
    // The module carries one fewer source — the wheel is not one of its rows — so it is drawn from
    // its own list rather than through the uniform grid.
    if which == target::MOD_BUS {
        p.routes.mod_bus.routes().into_iter().collect()
    } else {
        p.routes
            .ordinary()
            .into_iter()
            .find_map(|(index, group)| (index == which).then_some(group))
            .expect("every target but the summing module has a uniform grid")
            .routes(which)
            .into_iter()
            .collect()
    }
}

/// One target's routes, drawn beneath the control they move.
///
/// **Routing belongs under the thing it affects**, never in a detached footer — the ruling
/// mxm-mono-00's `plugins/mxm-mono-00/AGENTS.md` records and design system §7.4 makes normative.
/// The rows and the `‹ modulate ›` menu come from `mxm_modulation_params`, so every editor in the
/// collection draws this the same way: a target with nothing routed draws no group at all, the
/// menu sits outside the group and is labelled with its target, and each row ends in a remove
/// rather than a switch.
fn routes(
    ui: &mut Ui,
    tokens: &Tokens,
    which: usize,
    p: &MxmMonoPr1Params,
    setter: &ParamSetter<'_>,
    entries: &mut HashMap<&'static str, Option<String>>,
) {
    let entry = entries.entry("routes").or_default();
    mxm_modulation_params::ui::stack(
        ui,
        tokens,
        TARGET_NAMES[which],
        // The short form its card allows (design system §7.1), as `TARGET_PANEL_NAMES` says.
        TARGET_PANEL_NAMES[which],
        &target_routes(p, which),
        entry,
        setter,
    );
}

/// Each target's **painted** name: the prefix its card already carries dropped — *Frequency* under
/// a card titled *Oscillator A*, *Rate* under *LFO* (design system §7.1). `TARGET_NAMES` stays the
/// canonical name, which the parameters, the host and the accessibility tree read. The buses keep
/// theirs: each is its card's whole subject.
const TARGET_PANEL_NAMES: [&str; mxm_mono_pr1_dsp::routing::TARGETS] = [
    "Frequency",
    "Pulse width",
    "Frequency",
    "Pulse width",
    "Cutoff",
    "Resonance",
    "Rate",
    "Mod bus",
    "Wheel bus",
    "Amplitude",
];

const PRIMARY: Size = Size::Primary;
const STANDARD: Size = Size::Standard;
const COMPACT: Size = Size::Compact;

/// A stepped switch's cells, **labelled by the parameter itself**: each its option's own
/// formatted value, so a cell reads what the host's automation list reads.
fn switch_options(p: &MxmMonoPr1Params, id: &'static str) -> Vec<String> {
    let param = binding_for(id, p).param;
    let last = param
        .steps()
        .unwrap_or_else(|| unreachable!("{id} is not a switch"));
    (0..=last)
        .map(|option| param.format(option as f32 / last as f32))
        .collect()
}

/// The waveform a shape switch draws instead of its name (design system §7.3), or `None` for a
/// switch that is not a shape. Every saw here rises.
fn wave_of(id: &str) -> Option<Wave> {
    match id {
        "lfo_saw" | "osc_a_saw" | "osc_b_saw" => Some(Wave::RampUp),
        "lfo_triangle" | "osc_b_triangle" => Some(Wave::Triangle),
        "lfo_square" => Some(Wave::Square),
        "osc_a_pulse" | "osc_b_pulse" => Some(Wave::Pulse),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// The cards, as trees (plans/plan-layout-tree.md).
// ---------------------------------------------------------------------------------------------

/// What a leaf of this editor's cards draws. Hashed by what it names, which is also what keeps its
/// widget ids stable when a route appears above it.
#[derive(Clone, Debug, Hash)]
pub enum Leaf {
    Knob(&'static str, Size),
    /// A fader in the collection's fader row.
    Fader(&'static str),
    /// A stepped switch as a segmented control.
    Switch(&'static str),
    /// An on/off switch: named, or a picture of its wave.
    Toggle(&'static str),
    /// A control's tempo sync, the quarter note beside it.
    Picture(&'static str),
    /// The name over one envelope's faders on the Envelopes card.
    Heading(&'static str),
    Routes(usize),
    LfoTrace,
    /// An oscillator's trace, `true` for A's card.
    /// An oscillator's waveform, drawn from its settings: `true` for A.
    Waveform(bool),
    /// A module's live level, by target.
    BusLevel(usize),
    FilterResponse,
}

/// A row of knobs: `ui.columns` inside a width capped at `Σ max(diameter + SPACE_5, 78)`, so each
/// column is that sum's share, gaps included — and below the cap, only as narrow as the widest knob.
/// Levels as the collection's fader row (`tree::fader_row`): the mixer's three sources, an
/// envelope's A, D, S and R.
fn faders(ui: &Ui, p: &MxmMonoPr1Params, ids: &[&'static str]) -> Node<Leaf> {
    mxm_ui::tree::fader_row(
        ui,
        ids.iter()
            .map(|&id| {
                let bound = binding_for(id, p);
                mxm_ui::tree::fader(
                    Leaf::Fader(id),
                    bound.painted(),
                    mxm_ui::control::widest_value(|n| bound.param.format(n as f32)),
                )
            })
            .collect(),
    )
}

fn knobs(ui: &Ui, p: &MxmMonoPr1Params, knobs: &[(&'static str, Size)]) -> Node<Leaf> {
    mxm_ui::tree::knob_row(
        ui,
        knobs
            .iter()
            .map(|(id, size)| (*size, knob(ui, p, id, *size)))
            .collect(),
    )
}

/// A knob, in a column never narrower than its widest reading. The reading is drawn on one line and
/// elided where it does not fit, so a column that holds only its longest word — what
/// `control::knob_size` counts — would print *200.0 …* at the card's floor.
fn knob(ui: &Ui, p: &MxmMonoPr1Params, id: &'static str, size: Size) -> Node<Leaf> {
    let bound = binding_for(id, p);
    // A syncable control's column holds its free readings and its divisions.
    let widest = if id == "lfo_rate" {
        super::binding::synced_widest(bound.param, crate::params::LFO_SYNC.span)
    } else {
        mxm_ui::control::widest_value(|n| bound.param.format(n as f32))
    };
    let column = if size.value_always_visible() {
        let font = mxm_ui::typography::value_style(ui.style()).resolve(ui.style());
        ui.painter()
            .layout_no_wrap(widest.clone(), font, egui::Color32::PLACEHOLDER)
            .size()
            .x
    } else {
        0.0
    };
    leaf(
        Leaf::Knob(id, size),
        Kind::Knob {
            name: bound.painted().to_owned(),
            widest,
            size,
            column,
        },
    )
}

fn switch(p: &MxmMonoPr1Params, id: &'static str) -> Node<Leaf> {
    leaf(
        Leaf::Switch(id),
        Kind::Segmented {
            label: binding_for(id, p).painted().to_owned(),
            options: switch_options(p, id),
            beside: None,
        },
    )
}

/// Switches that stand together in one row, wrapping when the card is too narrow for them: the
/// named ones as wide as the row's longest name, so the row reads as one set; the pictures a fixed
/// cell. `horizontal_wrapped` as data.
fn toggles(ui: &Ui, p: &MxmMonoPr1Params, ids: &[&'static str]) -> Node<Leaf> {
    share(
        Share::Toggles,
        Node::Wrap {
            gap: ui.spacing().item_spacing.x,
            row_gap: SPACE_3,
            children: ids
                .iter()
                .map(|id| {
                    let kind = if wave_of(id).is_some() {
                        Kind::PictureToggle
                    } else {
                        Kind::Toggle {
                            label: binding_for(id, p).painted().to_owned(),
                        }
                    };
                    leaf(Leaf::Toggle(id), kind)
                })
                .collect(),
        },
    )
}

/// A target's routes, `SPACE_3` below what precedes it. A route stack is a composite with a rule of
/// its own — its floor is every route revealed at its widest reading — so it states its size
/// (`stack_size`) and takes the card's width.
fn routes_leaf(ui: &Ui, p: &MxmMonoPr1Params, which: usize) -> Node<Leaf> {
    let size = mxm_modulation_params::ui::stack_size(
        ui,
        TARGET_PANEL_NAMES[which],
        &target_routes(p, which),
    );
    pad(
        SPACE_3,
        leaf(
            Leaf::Routes(which),
            Kind::Custom {
                min_width: size.x,
                height: Height::Fixed(size.y),
                fills: true,
            },
        ),
    )
}

/// A display across the card's width at a fixed height, never narrower than `min_width`.
fn display(key: Leaf, min_width: f32, height: f32) -> Node<Leaf> {
    leaf(
        key,
        Kind::Custom {
            min_width,
            height: Height::Fixed(height),
            fills: true,
        },
    )
}

/// A section's body, as a tree, from the parameters and the telemetry text it shows.
pub fn card(ui: &Ui, section: Section, p: &MxmMonoPr1Params) -> Node<Leaf> {
    match section {
        Section::Voice => stack(vec![
            switch(p, "key_mode"),
            switch(p, "glide_mode"),
            knobs(ui, p, &[("glide_time", STANDARD)]),
            knobs(ui, p, &[("master_tune", COMPACT), ("bend_range", COMPACT)]),
            toggles(ui, p, &["repeat_external", "drone"]),
        ]),
        Section::Lfo => stack(vec![
            display(Leaf::LfoTrace, 0.0, PLOT_HEIGHT),
            // The rate's tempo sync is the quarter note beside it
            // (`plans/plan-tempo-sync-controls.md`).
            tree::row_gap(
                ui.spacing().item_spacing.x,
                vec![
                    knobs(ui, p, &[("lfo_rate", STANDARD)]),
                    tree::switch_beside_knob(
                        STANDARD,
                        leaf(Leaf::Picture("lfo_sync"), Kind::SyncToggle),
                    ),
                ],
            ),
            routes_leaf(ui, p, target::LFO_RATE),
            toggles(ui, p, &["lfo_saw", "lfo_triangle", "lfo_square"]),
        ]),
        // **Both envelopes on one card, one under the other** (the owner, 2026-09-25: *"underneath
        // each other. So they take up less space"*), each its name over its A, D, S and R. No note:
        // *"it explains the obvious"*.
        Section::Envelopes => {
            let envelope = |name: &'static str, ids: [&'static str; 4]| {
                stack(vec![
                    leaf(
                        Leaf::Heading(name),
                        Kind::Text {
                            text: name.to_owned(),
                            font: tree::Font::Body,
                            flow: tree::Flow::Line,
                        },
                    ),
                    faders(ui, p, &ids),
                ])
            };
            tree::stack_gap(
                mxm_ui::space::SPACE_6,
                vec![
                    envelope(
                        "Filter",
                        [
                            "filter_env_attack",
                            "filter_env_decay",
                            "filter_env_sustain",
                            "filter_env_release",
                        ],
                    ),
                    envelope(
                        "Amplifier",
                        [
                            "amp_env_attack",
                            "amp_env_decay",
                            "amp_env_sustain",
                            "amp_env_release",
                        ],
                    ),
                    // The collection's standard Amplitude, a factor after the VCA this envelope
                    // drives (`plans/plan-modulation-standard.md`).
                    routes_leaf(ui, p, target::AMPLITUDE),
                ],
            )
        }
        // **The summing module.** One of the two targets with no knob to sit under — it *is*
        // modulation rather than a control being modulated — so it gets a card of its own, named
        // for itself so that its title, its rows and its entry in every `‹ modulate ›` list read the
        // same word.
        Section::ModBus => stack(vec![
            display(
                Leaf::BusLevel(target::MOD_BUS),
                visuals::BUS_LEVEL_MIN_WIDTH,
                STATUS_HEIGHT,
            ),
            routes_leaf(ui, p, target::MOD_BUS),
        ]),
        // **The multiplier module** — decision 1.14. Its law is product, so its rows are *factors*
        // rather than things that add: what comes out is every factor multiplied together. It
        // ships wired to `Mod bus × Wheel`, which is the machine's own two-stage routing and is
        // what makes vibrato fade in on the wheel. Either factor can be re-pointed at anything.
        Section::WheelBus => stack(vec![
            display(
                Leaf::BusLevel(target::WHEEL_BUS),
                visuals::BUS_LEVEL_MIN_WIDTH,
                STATUS_HEIGHT,
            ),
            routes_leaf(ui, p, target::WHEEL_BUS),
        ]),
        // The wave switches first, right under the trace they change (the owner, 2026-09-25), then
        // the octave, the knobs and their routes.
        Section::OscillatorA => stack(vec![
            display(Leaf::Waveform(true), 0.0, TALL_PLOT_HEIGHT),
            toggles(ui, p, &["osc_a_saw", "osc_a_pulse", "osc_a_sync"]),
            switch(p, "osc_a_octave"),
            knobs(
                ui,
                p,
                &[
                    ("osc_a_frequency", STANDARD),
                    ("osc_a_pulse_width", STANDARD),
                ],
            ),
            routes_leaf(ui, p, target::OSCILLATOR_A_FREQUENCY),
            routes_leaf(ui, p, target::OSCILLATOR_A_PULSE_WIDTH),
        ]),
        Section::OscillatorB => stack(vec![
            display(Leaf::Waveform(false), 0.0, TALL_PLOT_HEIGHT),
            toggles(ui, p, &["osc_b_saw", "osc_b_triangle", "osc_b_pulse"]),
            toggles(ui, p, &["osc_b_low_frequency", "osc_b_keyboard_follow"]),
            switch(p, "osc_b_octave"),
            knobs(
                ui,
                p,
                &[
                    ("osc_b_frequency", STANDARD),
                    ("osc_b_pulse_width", STANDARD),
                ],
            ),
            routes_leaf(ui, p, target::OSCILLATOR_B_FREQUENCY),
            routes_leaf(ui, p, target::OSCILLATOR_B_PULSE_WIDTH),
        ]),
        Section::Mixer => faders(
            ui,
            p,
            &["osc_a_level", "osc_b_level", "noise_external_level"],
        ),
        Section::Filter => stack(vec![
            display(Leaf::FilterResponse, 0.0, PLOT_HEIGHT),
            knobs(ui, p, &[("cutoff", PRIMARY), ("resonance", PRIMARY)]),
            // The dedicated filter-envelope amount and the keyboard amount were two knobs here.
            // They are the first two rows of this stack now, wired in Init at zero, and taking one
            // off is a gesture the knobs never offered. `SPACE_5` above it keeps the knob row and
            // the stack visibly separate.
            pad(SPACE_5, routes_leaf(ui, p, target::FILTER_CUTOFF)),
            routes_leaf(ui, p, target::RESONANCE),
        ]),
    }
}

/// Everything a leaf draws with: the parameters and their host, and the live telemetry.
pub struct Live<'a, 'b> {
    pub params: &'a MxmMonoPr1Params,
    pub telemetry: &'a Telemetry,
    pub setter: &'a ParamSetter<'b>,
    pub entries: &'a mut HashMap<&'static str, Option<String>>,
}

/// Draws one leaf, in the `Ui` the tree bounded to `rect`, through the bindings above — so the
/// controls, their gestures and their names are exactly what they were.
pub fn paint(ui: &mut Ui, tokens: &Tokens, leaf: &Leaf, rect: egui::Rect, live: &mut Live<'_, '_>) {
    let p = live.params;
    match *leaf {
        // Synced to a tempo, the LFO rate reads its division; the host still reads its hertz.
        Leaf::Knob(id, size) => {
            let bound = binding_for(id, p);
            let division = {
                use nice_plug::prelude::Param as _;
                let synced: Option<(bool, &nice_plug::prelude::FloatParam, mxm_tempo::Ladder)> =
                    match id {
                        "lfo_rate" => {
                            Some((p.lfo_sync.value(), &p.lfo_rate, crate::params::LFO_SYNC))
                        }
                        _ => None,
                    };
                synced
                    .filter(|(on, _, _)| *on)
                    .and_then(|(_, param, ladder)| {
                        ladder.shown(
                            param.unmodulated_normalized_value(),
                            live.telemetry.tempo.get(),
                            f64::from(param.preview_plain(0.0)),
                            f64::from(param.preview_plain(1.0)),
                        )
                    })
            };
            match division {
                Some(division) => bound.knob_with_reading(
                    ui,
                    tokens,
                    live.setter,
                    size,
                    rect.width(),
                    live.entries,
                    division.label(),
                ),
                None => bound.knob(ui, tokens, live.setter, size, rect.width(), live.entries),
            }
        }
        Leaf::Picture(id) => {
            super::binding::sync_picture(ui, tokens, id, binding_for(id, p).param, live.setter);
        }
        Leaf::Fader(id) => {
            let bound = binding_for(id, p);
            bound.slider_vertical(
                ui,
                tokens,
                live.setter,
                live.entries,
                bound.painted(),
                rect.width(),
                mxm_ui::control::FADER_HEIGHT,
            );
        }
        // Each cell as wide as the longest option and no wider (design system §7.3).
        Leaf::Switch(id) => {
            let bound = binding_for(id, p);
            let labels = switch_options(p, id);
            let options: Vec<&str> = labels.iter().map(String::as_str).collect();
            segmented_named(
                ui,
                tokens,
                bound.id,
                bound.param,
                bound.panel.as_deref(),
                &options,
                bound.details,
                live.setter,
                0.0,
            );
        }
        // Picture switches (the owner, 2026-09-23): the shapes combine, so each is its own on/off,
        // drawn as its wave. The others paint their short name and announce their own.
        Leaf::Toggle(id) => {
            let bound = binding_for(id, p);
            match wave_of(id) {
                Some(wave) => toggle_picture(
                    ui,
                    tokens,
                    bound.id,
                    bound.param,
                    wave,
                    bound.description,
                    live.setter,
                ),
                None => toggle_labelled(
                    ui,
                    tokens,
                    bound.id,
                    bound.param,
                    bound.painted(),
                    bound.description,
                    live.setter,
                    0.0,
                ),
            }
        }
        Leaf::Heading(text) => {
            ui.label(text);
        }
        Leaf::Routes(which) => routes(ui, tokens, which, p, live.setter, live.entries),
        Leaf::LfoTrace => visuals::lfo_sum_trace(
            ui,
            tokens,
            p.lfo_saw.value(),
            p.lfo_triangle.value(),
            p.lfo_square.value(),
        ),
        Leaf::Waveform(a) => {
            let (name, waves, colour) = if a {
                (
                    "Oscillator A waveform",
                    visuals::OscillatorWaves {
                        saw: p.osc_a_saw.value(),
                        triangle: false,
                        pulse: p.osc_a_pulse.value(),
                        width: p.osc_a_pulse_width.value(),
                    },
                    tokens.accent,
                )
            } else {
                (
                    "Oscillator B waveform",
                    visuals::OscillatorWaves {
                        saw: p.osc_b_saw.value(),
                        triangle: p.osc_b_triangle.value(),
                        pulse: p.osc_b_pulse.value(),
                        width: p.osc_b_pulse_width.value(),
                    },
                    tokens.mod_lfo,
                )
            };
            visuals::oscillator_shape(ui, tokens, name, waves, colour);
        }
        Leaf::BusLevel(which) => {
            let level = if which == target::MOD_BUS {
                live.telemetry.mod_bus()
            } else {
                live.telemetry.wheel_bus()
            };
            // What the bus does, on hover rather than printed (design system §7.6).
            let hover = if which == target::MOD_BUS {
                "Everything routed here adds together; Wheel bus sets how much of it gets through."
            } else {
                "Sets how much of Mod bus gets through; at first, the mod wheel does."
            };
            visuals::bus_level(ui, tokens, TARGET_NAMES[which], level, hover);
        }
        Leaf::FilterResponse => visuals::filter_response(
            ui,
            tokens,
            live.telemetry.set_cutoff_hz(),
            live.telemetry.effective_cutoff_hz(),
            p.resonance.value(),
        ),
    }
}

/// How the keyboard steps a parameter: a semitone and an octave for the bend reach
/// and for the cutoff in hertz,
/// the owner's ruling of 2026-09-23. Everything not named keeps its own step. See
/// [`crate::editor::binding::StepLaw`].
fn step_law(id: &str) -> super::binding::StepLaw {
    use super::binding::StepLaw;
    match id {
        "bend_range" => StepLaw::Semitones,
        "cutoff" => StepLaw::Hertz,
        _ => StepLaw::Own,
    }
}

pub fn binding_for<'a>(id: &'static str, p: &'a MxmMonoPr1Params) -> Bound<'a> {
    let (param, description, bipolar): (&'a dyn super::binding::ErasedParam, &'static str, bool) =
        match id {
            "key_mode" => (
                &p.key_mode,
                "Which held key sounds, and whether each new key restarts the envelopes.",
                false,
            ),
            "glide_mode" => (
                &p.glide_mode,
                "Normal glides every pitch change; Auto glides only when notes overlap.",
                false,
            ),
            "glide_time" => (
                &p.glide_time,
                "Glide time between notes; zero turns glide off.",
                false,
            ),
            "repeat_external" => (
                &p.repeat_external,
                "Restarts both envelopes on every LFO cycle, so the sound repeats by itself.",
                false,
            ),
            "drone" => (
                &p.drone,
                "Holds the note on with no key, for drones.",
                false,
            ),
            "master_tune" => (
                &p.master_tune,
                "Offsets the instrument's pitch in semitones.",
                true,
            ),
            "bend_range" => (&p.bend_range, "The pitch-bend range, in semitones.", false),
            "lfo_rate" => (&p.lfo_rate, "How fast the LFO runs.", false),
            "lfo_sync" => (&p.lfo_sync, super::binding::SYNC_DESCRIPTION, false),
            "lfo_saw" => (
                &p.lfo_saw,
                "Turns the LFO's saw on. The shapes you turn on add together.",
                false,
            ),
            "lfo_triangle" => (
                &p.lfo_triangle,
                "Turns the LFO's triangle on. The shapes you turn on add together.",
                false,
            ),
            "lfo_square" => (
                &p.lfo_square,
                "Turns the LFO's square on. The shapes you turn on add together.",
                false,
            ),
            "filter_env_attack" => (
                &p.filter_env_attack,
                "Time for the filter envelope to reach full level.",
                false,
            ),
            "filter_env_decay" => (
                &p.filter_env_decay,
                "Time for the filter envelope to fall to sustain.",
                false,
            ),
            "filter_env_sustain" => (
                &p.filter_env_sustain,
                "The filter envelope's held level.",
                false,
            ),
            "filter_env_release" => (
                &p.filter_env_release,
                "How long the filter envelope takes to fall after the key is released.",
                false,
            ),
            "amp_env_attack" => (
                &p.amp_env_attack,
                "Time for the amplifier envelope to reach full level.",
                false,
            ),
            "amp_env_decay" => (
                &p.amp_env_decay,
                "Time for the amplifier envelope to fall to sustain.",
                false,
            ),
            "amp_env_sustain" => (
                &p.amp_env_sustain,
                "The amplifier envelope's held level.",
                false,
            ),
            "amp_env_release" => (
                &p.amp_env_release,
                "How long the sound takes to fade after the key is released.",
                false,
            ),
            "osc_a_octave" => (
                &p.osc_a_octave,
                "Raises Oscillator A in exact octave steps.",
                false,
            ),
            "osc_a_frequency" => (
                &p.osc_a_frequency,
                "Tunes Oscillator A around the played pitch.",
                true,
            ),
            "osc_a_pulse_width" => (
                &p.osc_a_pulse_width,
                "How narrow Oscillator A's pulse is; at either extreme it goes silent.",
                false,
            ),
            "osc_a_saw" => (
                &p.osc_a_saw,
                "Turns Oscillator A's saw on. The shapes you turn on add together.",
                false,
            ),
            "osc_a_pulse" => (
                &p.osc_a_pulse,
                "Turns Oscillator A's pulse on. The shapes you turn on add together.",
                false,
            ),
            "osc_a_sync" => (
                &p.osc_a_sync,
                "Locks Oscillator A to B for the hard-sync sound, even when B is silent.",
                false,
            ),
            "osc_b_octave" => (
                &p.osc_b_octave,
                "Raises Oscillator B in exact octave steps.",
                false,
            ),
            "osc_b_frequency" => (
                &p.osc_b_frequency,
                "Tunes Oscillator B for detune, intervals, sync ratios, or modulation rate.",
                true,
            ),
            "osc_b_pulse_width" => (
                &p.osc_b_pulse_width,
                "How narrow Oscillator B's pulse is; at either extreme it goes silent.",
                false,
            ),
            "osc_b_saw" => (
                &p.osc_b_saw,
                "Turns Oscillator B's saw on. The shapes you turn on add together.",
                false,
            ),
            "osc_b_triangle" => (
                &p.osc_b_triangle,
                "Turns Oscillator B's triangle on. The shapes you turn on add together.",
                false,
            ),
            "osc_b_pulse" => (
                &p.osc_b_pulse,
                "Turns Oscillator B's pulse on. The shapes you turn on add together.",
                false,
            ),
            "osc_b_low_frequency" => (
                &p.osc_b_low_frequency,
                "Moves Oscillator B into its low-frequency modulation range.",
                false,
            ),
            "osc_b_keyboard_follow" => (
                &p.osc_b_keyboard_follow,
                "Lets played key pitch drive Oscillator B.",
                false,
            ),
            "osc_a_level" => (&p.osc_a_level, "Oscillator A's level in the mixer.", false),
            "osc_b_level" => (&p.osc_b_level, "Oscillator B's level in the mixer.", false),
            "noise_external_level" => (
                &p.noise_external_level,
                "The noise's level in the mixer.",
                false,
            ),
            "cutoff" => (&p.cutoff, "The filter's cutoff: lower is darker.", false),
            "resonance" => (&p.resonance, "Emphasis at the cutoff.", false),
            // Drawn in the app bar, where no card caption sits beside it, so the tooltip carries
            // what the retired card's caption said: this is drive, not a clean output attenuator.
            "volume" => (
                &p.volume,
                "The instrument's level; turned up, it also saturates.",
                false,
            ),
            other => unreachable!("no binding for {other}"),
        };
    Bound {
        id,
        param,
        description,
        panel: match id {
            // The envelopes' faders, by the convention (the owner, 2026-09-25).
            "filter_env_attack" | "amp_env_attack" => Some("A"),
            "filter_env_decay" | "amp_env_decay" => Some("D"),
            "filter_env_sustain" | "amp_env_sustain" => Some("S"),
            "filter_env_release" | "amp_env_release" => Some("R"),
            "osc_a_frequency" | "osc_b_frequency" => Some("Frequency"),
            "osc_a_pulse_width" | "osc_b_pulse_width" => Some("Pulse width"),
            "osc_a_octave" | "osc_b_octave" => Some("Octave"),
            "osc_a_sync" => Some("Sync"),
            "osc_b_low_frequency" => Some("Low frequency"),
            "osc_b_keyboard_follow" => Some("Keyboard follow"),
            "lfo_rate" => Some("Rate"),
            "osc_a_level" => Some("A level"),
            "osc_b_level" => Some("B level"),
            "noise_external_level" => Some("Noise"),
            _ => None,
        }
        .map(std::borrow::Cow::Borrowed),
        bipolar,
        law: step_law(id),
        stepped: None,
        details: details_of(id),
    }
}

/// What each option of a stepped control does, one sentence per cell in the parameter's own order
/// (design system §7.3; the owner, 2026-09-27: the cells of a row do not share one sentence).
/// Empty for everything drawn as a knob, slider or toggle.
fn details_of(id: &str) -> &'static [&'static str] {
    match id {
        "key_mode" => &[
            "The lowest held key sounds; a new key over held ones does not restart the envelopes.",
            "The key pressed last sounds, and every new key restarts the envelopes.",
        ],
        "glide_mode" => &[
            "Glides on every new note.",
            "Glides only when you play a note before releasing the last.",
        ],
        "osc_a_octave" | "osc_b_octave" => &[
            "The oscillator's own pitch.",
            "One octave up.",
            "Two octaves up.",
            "Three octaves up.",
        ],
        _ => &[],
    }
}

pub const ALL_IDS: &[&str] = &[
    "key_mode",
    "glide_mode",
    "glide_time",
    "repeat_external",
    "drone",
    "master_tune",
    "bend_range",
    "lfo_rate",
    "lfo_sync",
    "lfo_saw",
    "lfo_triangle",
    "lfo_square",
    "filter_env_attack",
    "filter_env_decay",
    "filter_env_sustain",
    "filter_env_release",
    "amp_env_attack",
    "amp_env_decay",
    "amp_env_sustain",
    "amp_env_release",
    "osc_a_octave",
    "osc_a_frequency",
    "osc_a_pulse_width",
    "osc_a_saw",
    "osc_a_pulse",
    "osc_a_sync",
    "osc_b_octave",
    "osc_b_frequency",
    "osc_b_pulse_width",
    "osc_b_saw",
    "osc_b_triangle",
    "osc_b_pulse",
    "osc_b_low_frequency",
    "osc_b_keyboard_follow",
    "osc_a_level",
    "osc_b_level",
    "noise_external_level",
    "cutoff",
    "resonance",
    "volume",
];

pub fn all_parameters(params: &MxmMonoPr1Params) -> Vec<Bound<'_>> {
    ALL_IDS.iter().map(|id| binding_for(id, params)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_binding_has_a_sentence() {
        let params = MxmMonoPr1Params::default();
        for id in ALL_IDS {
            assert!(binding_for(id, &params).description.ends_with('.'), "{id}");
        }
    }
}
