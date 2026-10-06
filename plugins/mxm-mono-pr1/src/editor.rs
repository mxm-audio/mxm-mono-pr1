//! Production editor for `mxm-mono-pr1`, implementing its approved §14 brief.
//!
//! This is a software-native panel over the shared MXM shell. Its stable cards follow task and
//! signal-flow order, reflow through the shared paging renderer, and never reproduce the hardware
//! panel. All sound edits are host parameters; telemetry is observational only.

pub mod binding;
pub mod sections;
mod visuals;

use std::{collections::HashMap, sync::Arc};

use egui::Ui;
use mxm_ui::{space::SPACE_5, theme::Tokens};
use nice_plug::{context::gui::GuiContext, prelude::*};
use nice_plug_egui::{EguiEditorState, NiceEguiApp, create_egui_editor};

use crate::{params::MxmMonoPr1Params, telemetry::Telemetry};

/// The opening frame, **the quarter-4K budget hugged** — the panel laid out at 1920 points wide,
/// asking for exactly the height that takes and no more.
///
/// It was 1920 x 833 while `Modulation buses` existed: that card carried three source knobs each
/// beside a Direct/Wheel selector and five destination selectors under a caption, and it was the
/// tallest thing in its row. Its routes moved under the controls they move and the height fell to
/// 571 — then the multiplier module took a card of its own, which needed an extra row, and it came
/// back to 899. Moving Volume into the app bar retired the Amplifier and output card and left that
/// unchanged. Route rows named by their source alone, under their target's title (2026-09-24),
/// narrowed every card with a stack, and the cards pack onto fewer rows: 727. **Measured by
/// `proof::the_opening_size_is_the_budget_hugged`, not chosen**, so this number is a consequence of
/// the card list rather than a decision to revisit.
const REFERENCE: (u32, u32) = (1881, 760);
/// The wider of one widest card with the shell's two gutters (held by the paging proofs) and the app
/// bar at its last compact step. The bar decides it: `proof::the_app_bar_holds_in_the_minimum_window`
/// measures it.
const MINIMUM: (u32, u32) = (424, 360);

/// The one parameter drawn outside the cards: the instrument's output level, which design system
/// §3.1 puts in the app bar beside the meter.
pub(crate) const BAR_PARAMETER: &str = "volume";
/// The keyboard cursor's card for [`BAR_PARAMETER`], outside the paging keys 0…8.
const VOLUME_CARD: u64 = 64;

pub const SECTIONS: &[Section] = &[
    Section::Voice,
    Section::Lfo,
    Section::Envelopes,
    Section::ModBus,
    Section::WheelBus,
    Section::OscillatorA,
    Section::OscillatorB,
    Section::Mixer,
    Section::Filter,
];

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Section {
    Voice,
    Lfo,
    /// Both envelopes, side by side as the source sets them (the owner, 2026-09-25: *"these should
    /// be on the same card"*).
    Envelopes,
    ModBus,
    WheelBus,
    OscillatorA,
    OscillatorB,
    Mixer,
    Filter,
}

impl Section {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Voice => "Voice",
            Self::Lfo => "LFO",
            Self::Envelopes => "Envelopes",
            Self::ModBus => "Mod bus",
            Self::WheelBus => "Wheel bus",
            Self::OscillatorA => "Oscillator A",
            Self::OscillatorB => "Oscillator B",
            Self::Mixer => "Mixer",
            Self::Filter => "Filter",
        }
    }

    #[cfg(test)]
    pub(crate) const fn parameters(self) -> &'static [&'static str] {
        match self {
            Self::Voice => &[
                "key_mode",
                "glide_mode",
                "glide_time",
                "repeat_external",
                "drone",
                "master_tune",
                "bend_range",
            ],
            Self::Lfo => &[
                "lfo_rate",
                "lfo_sync",
                "lfo_saw",
                "lfo_triangle",
                "lfo_square",
            ],
            Self::Envelopes => &[
                "filter_env_attack",
                "filter_env_decay",
                "filter_env_sustain",
                "filter_env_release",
                "amp_env_attack",
                "amp_env_decay",
                "amp_env_sustain",
                "amp_env_release",
            ],
            // Routing pairs are not listed here. This inventory is the **permanent musician
            // controls each card owns**, and a route is revealed by its own presence: at Init only
            // seven of 125 pairs are drawn at all, so listing them would assert a card contains
            // controls that are deliberately absent. `routes_are_drawn_under_their_target` is the
            // check that covers them instead.
            Self::ModBus | Self::WheelBus => &[],
            Self::OscillatorA => &[
                "osc_a_octave",
                "osc_a_frequency",
                "osc_a_pulse_width",
                "osc_a_saw",
                "osc_a_pulse",
                "osc_a_sync",
            ],
            Self::OscillatorB => &[
                "osc_b_octave",
                "osc_b_frequency",
                "osc_b_pulse_width",
                "osc_b_saw",
                "osc_b_triangle",
                "osc_b_pulse",
                "osc_b_low_frequency",
                "osc_b_keyboard_follow",
            ],
            Self::Mixer => &["osc_a_level", "osc_b_level", "noise_external_level"],
            // `filter_keyboard_amount` and `filter_env_amount` retired into the cutoff stack.
            Self::Filter => &["cutoff", "resonance"],
            // Volume is on no card: it is `BAR_PARAMETER`, in the app bar.
        }
    }
}

/// Every paging item, each floor **computed from its card's tree** in `ui`'s fonts
/// (plans/plan-layout-tree.md): the tree's narrowest plus the card's chrome, every route revealed at
/// its widest reading. Nothing here is a typed number; the ceiling stays this editor's policy.
pub fn page_items(ui: &Ui, params: &MxmMonoPr1Params) -> Vec<mxm_ui::paging::Item<'static>> {
    use mxm_ui::{
        flow::Card,
        paging::{Category as C, Item, Key},
    };
    SECTIONS
        .iter()
        .enumerate()
        .map(|(i, section)| {
            let floor = mxm_ui::tree::card_floor(
                ui,
                section.title(),
                &sections::card(ui, *section, params),
            );
            (i, section, floor)
        })
        .map(|(i, section, floor)| Item {
            key: Key(i as u64),
            // Exactly as wide as its content: the ceiling is the floor (plans/plan-editor-standard.md
            // A1).
            card: Card::new(section.title(), floor).capped(floor),
            category: match section {
                Section::Voice => C::Performance,
                Section::Lfo | Section::Envelopes | Section::ModBus | Section::WheelBus => {
                    C::Modulators
                }
                Section::OscillatorA | Section::OscillatorB => C::Generators,
                Section::Mixer | Section::Filter => C::Tone,
            },
            kind: section.title(),
        })
        .collect()
}

pub fn create(
    params: Arc<MxmMonoPr1Params>,
    telemetry: Arc<Telemetry>,
) -> Option<MxmMonoPr1Editor> {
    let state = EguiEditorState::from_size(
        nice_plug::editor::dpi::LogicalSize::new(REFERENCE.0, REFERENCE.1),
        1.0,
    );
    create_egui_editor(
        state,
        nice_plug_egui::RepaintNotifier::new(),
        nice_plug_egui::EguiNiceSettings {
            title: crate::NAME.to_owned(),
            resize_hint: ResizeHint {
                size_constraints: nice_plug::editor::SizeConstraints::min_logical_size(
                    nice_plug::editor::dpi::LogicalSize::new(MINIMUM.0 as f32, MINIMUM.1 as f32),
                ),
                ..ResizeHint::RESIZABLE
            },
            ..Default::default()
        },
        MxmMonoPr1App::new(params, telemetry),
    )
}

pub type MxmMonoPr1Editor = nice_plug_egui::EguiEditor<MxmMonoPr1App>;
pub use mxm_preset::PresetUi;

pub struct MxmMonoPr1App {
    params: Arc<MxmMonoPr1Params>,
    telemetry: Arc<Telemetry>,
    gui_context: Option<GuiContext>,
    view: usize,
    text_entry: HashMap<&'static str, Option<String>>,
    presets: PresetUi,
    /// Where the keyboard is: a card, and a parameter inside it. Transient, like the text
    /// buffers — it is not a parameter and nothing durable reads it.
    nav: mxm_ui::navigation::State,
}

impl MxmMonoPr1App {
    pub fn new(params: Arc<MxmMonoPr1Params>, telemetry: Arc<Telemetry>) -> Self {
        let presets = PresetUi::new(params.as_ref());
        Self {
            params,
            telemetry,
            gui_context: None,
            view: 0,
            text_entry: HashMap::new(),
            presets,
            nav: mxm_ui::navigation::State::default(),
        }
    }
}

impl NiceEguiApp for MxmMonoPr1App {
    fn build(
        &mut self,
        ctx: egui::Context,
        gui: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        mxm_ui::theme::apply(&ctx);
        mxm_ui::typography::apply(&ctx);
        ctx.set_theme(mxm_ui::theme::preference());
        self.gui_context = Some(gui);
        Ok(())
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut nice_plug_egui::Frame) {
        let Some(gui) = self.gui_context.clone() else {
            return;
        };
        panel(
            ui,
            &self.params,
            &self.telemetry,
            &gui.param_setter(),
            &mut self.view,
            &mut self.text_entry,
            &mut self.presets,
            &mut self.nav,
        );
    }

    fn editor_closed(&mut self) {
        self.gui_context = None;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn panel(
    ui: &mut Ui,
    params: &MxmMonoPr1Params,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    view: &mut usize,
    text_entry: &mut HashMap<&'static str, Option<String>>,
    presets: &mut PresetUi,
    nav: &mut mxm_ui::navigation::State,
) {
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(50));
    // One question, and both layers suspend on it: the paging renderer's `hold` and the cursor's
    // `inert` both ask whether another surface owns this frame's keyboard.
    let busy = presets.holds_the_keyboard() || text_entry.values().any(Option::is_some);
    mxm_ui::paging::editor::hold(ui.ctx(), busy);
    mxm_ui::paging::editor::developer_request(ui.ctx(), view, telemetry.take_view_request());

    // **The keyboard cursor moves before anything is drawn**, so a navigation arrow is consumed
    // here rather than also walking egui's own focus ring. It reads the registry and the exact
    // card rectangles the previous frame built, and it resolves the developer-view request first,
    // because which surface this frame is deciding who owns its keyboard.
    if *view == mxm_ui::paging::PARAMETERS {
        // This surface has no cards. Stop rather than merely hiding the outline, or its controls
        // lose their legacy bare-arrow editing to an invisible stale musician cursor.
        mxm_ui::navigation::stop(ui.ctx());
    } else {
        mxm_ui::navigation::paged_with_bar(ui.ctx(), nav, busy, &[VOLUME_CARD]);
    }
    if let Some(open) = telemetry.take_browser_request() {
        presets.set_browser_open(open);
    }
    if let Some(index) = telemetry.take_theme_request()
        && let Some(theme) = mxm_ui::theme::from_index(index)
    {
        ui.ctx().set_theme(theme);
    }
    let tokens = tokens_for(ui);
    let peak = telemetry.take_peak();
    let clipped = telemetry.clipped();
    mxm_ui::AppBar::new(crate::NAME).show_with(
        ui,
        &tokens,
        |ui| mxm_preset::ui::preset_row(ui, &tokens, params, setter, presets),
        |ui| {
            if mxm_ui::shell::level_meter(ui, &tokens, peak, clipped) {
                telemetry.clear_clip();
            }
            // Design system §3.1 slot 6: the instrument's output level sits beside its meter, not
            // on a card.
            mxm_ui::navigation::bar_card(ui, VOLUME_CARD, |ui| {
                ui.scope(|ui| {
                    sections::binding_for(BAR_PARAMETER, params)
                        .slider_inline(ui, &tokens, setter, text_entry, 96.0);
                })
                .response
                .rect
            });
            mxm_ui::shell::zoom_control(ui);
            mxm_ui::shell::editor_theme_control(ui);
        },
    );
    mxm_preset::ui::overlays(ui, &tokens, params, setter, presets);

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(tokens.canvas)
                .inner_margin(egui::Margin::same(SPACE_5 as i8)),
        )
        .show(ui, |ui| {
            if *view == mxm_ui::paging::PARAMETERS {
                parameters_view(ui, &tokens, params, setter, text_entry);
            } else {
                paged_view(ui, &tokens, params, telemetry, setter, text_entry);
            }
        });
}

fn paged_view(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmMonoPr1Params,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    entries: &mut HashMap<&'static str, Option<String>>,
) {
    use mxm_ui::paging::Key;
    let items = page_items(ui, params);
    let text_editing = entries.values().any(Option::is_some);
    let mut live = sections::Live {
        params,
        telemetry,
        setter,
        entries,
    };
    mxm_ui::paging::editor::show(
        ui,
        tokens,
        &items,
        &[&[Key(3), Key(4)], &[Key(5), Key(6)]],
        text_editing,
        &mut |ui, index| sections::card(ui, SECTIONS[index], params),
        &mut |ui, _, leaf, rect| sections::paint(ui, tokens, leaf, rect, &mut live),
    );
}

/// The paging items as the editor computes them, from a context set up as an editor's is — three
/// passes in, so the weighted font cuts are bound — for tests, which have no editor `Ui` to hand.
#[cfg(test)]
pub(crate) fn test_items() -> Vec<mxm_ui::paging::Item<'static>> {
    let ctx = egui::Context::default();
    mxm_ui::typography::apply(&ctx);
    mxm_ui::theme::apply(&ctx);
    let params = MxmMonoPr1Params::default();
    let mut items = Vec::new();
    for _ in 0..3 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            items = page_items(ui, &params);
        });
        output.textures_delta.clear();
    }
    items
}

/// The cards' floors in paging order, as [`test_items`] computes them.
#[cfg(test)]
pub(crate) fn test_floors() -> Vec<f32> {
    test_items().iter().map(|item| item.card.floor).collect()
}

fn parameters_view(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmMonoPr1Params,
    setter: &ParamSetter<'_>,
    entries: &mut HashMap<&'static str, Option<String>>,
) {
    mxm_ui::shell::scroll_list(ui).show(ui, |ui| {
        let columns = if ui.available_width() >= 1050.0 {
            3
        } else if ui.available_width() >= 620.0 {
            2
        } else {
            1
        };
        // A flat list with no card to shorten a name: every slider paints its full name.
        let bound: Vec<_> = sections::all_parameters(params)
            .into_iter()
            .map(|parameter| parameter.unlabelled())
            .collect();
        let per_column = bound.len().div_ceil(columns);
        ui.columns(columns, |uis| {
            for (column, chunk) in uis.iter_mut().zip(bound.chunks(per_column)) {
                for parameter in chunk {
                    parameter.slider(column, tokens, setter, entries);
                }
            }
        });
    });
}

fn tokens_for(ui: &Ui) -> Tokens {
    if ui.visuals().dark_mode {
        mxm_ui::DARK
    } else {
        mxm_ui::LIGHT
    }
}

#[cfg(test)]
mod proof;

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::{
        params::InternalParamMut,
        params::internals::ParamPtr,
        prelude::{Params, PluginApi, PluginState},
    };

    struct NoHost;
    impl nice_plug::context::gui::GuiContextInner for NoHost {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
        unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {}
        unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
        fn get_state(&self) -> PluginState {
            PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }
        fn set_state(&self, _state: PluginState) {}
    }

    use mxm_plugin_test::keyboard_checks;

    /// What this editor keeps behind a disclosure, opened so the reachability check sees it.
    /// Nothing here: every control is on a card or, for Volume, in the app bar.
    const REVEAL: fn(&egui::Context) = |_| {};

    /// The rollout's own failure mode: a control whose `navigation::at` scope was forgotten paints
    /// exactly as before and is simply unreachable from the keyboard. Nothing else would say so.
    #[test]
    fn the_keyboard_cursor_reaches_and_operates_every_parameter() {
        let params = MxmMonoPr1Params::default();
        let telemetry = Telemetry::default();
        let host = keyboard_checks::Recorder::default();
        let setter = ParamSetter::new(&host);
        // **Every route present, because an absent one draws nothing at all.** That is the routing
        // interface's own rule — a target with nothing wired draws no group — so at Init five rows
        // exist and 107 do not. Revealing them all is the only way this check covers the parameters
        // the conversion added.
        for (_, group) in params.routes.ordinary() {
            for presence in group.presence_params() {
                // Safety: a test owns these parameters outright; nothing else holds a reference.
                unsafe { presence._internal_set_plain_value(true) };
            }
        }
        for presence in params.routes.mod_bus.presence_params() {
            unsafe { presence._internal_set_plain_value(true) };
        }
        let mut ids: Vec<&str> = sections::all_parameters(&params)
            .iter()
            .map(|bound| bound.id)
            .collect();
        ids.extend(
            crate::routes::ROUTE_IDS
                .iter()
                .flat_map(|target| target.iter())
                .flat_map(|(amount, present)| [*amount, *present]),
        );
        let mut view = 0usize;
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        keyboard_checks::the_cursor_reaches_and_operates(
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &test_items(),
            keyboard_checks::Coverage::Exactly(&ids),
            &REVEAL,
            &host,
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    #[test]
    fn production_panel_paints_at_opening_and_one_card_sizes_in_both_themes() {
        let params = MxmMonoPr1Params::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut view = 0;
        let mut entries = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        for theme in [egui::ThemePreference::Light, egui::ThemePreference::Dark] {
            for size in [
                egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
                egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
            ] {
                let ctx = egui::Context::default();
                mxm_ui::typography::apply(&ctx);
                mxm_ui::theme::apply(&ctx);
                ctx.set_theme(theme);
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                for _ in 0..3 {
                    let mut output = ctx.run_ui(input.clone(), |ui| {
                        panel(
                            ui,
                            &params,
                            &telemetry,
                            &setter,
                            &mut view,
                            &mut entries,
                            &mut presets,
                            &mut nav,
                        )
                    });
                    output.textures_delta.clear();
                }
                let report =
                    mxm_ui::paging::editor::report(&ctx).expect("production panel uses paging");
                assert!(!report.visible.is_empty(), "a card is visible at {size}");
            }
        }
    }

    #[test]
    fn card_inventory_matches_the_approved_brief() {
        assert_eq!(
            SECTIONS.iter().map(|s| s.title()).collect::<Vec<_>>(),
            [
                "Voice",
                "LFO",
                "Envelopes",
                "Mod bus",
                "Wheel bus",
                "Oscillator A",
                "Oscillator B",
                "Mixer",
                "Filter",
            ]
        );
        // **No Amplifier and output card.** Its one control, Volume, is the instrument's output
        // level, which design system §3.1 puts in the app bar; a card left holding nothing is not
        // kept for its title.
        // **The widest card is no longer `Modulation buses` at 432 points.** Its three source
        // groups and five destination selectors retired into route stacks under the controls they
        // move, and a route row is one slider plus a removal mark rather than a knob beside a
        // three-cell selector. The window minimum follows the widest floor, so it moved with it.
        let widest = test_floors().into_iter().fold(0.0, f32::max);
        assert!(
            MINIMUM.0 as f32 >= widest + 2.0 * SPACE_5,
            "the widest card's floor is {widest:.1}; the window minimum must hold it and two gutters"
        );
    }

    /// Every **permanent musician control** is drawn exactly once — on a card, or for Volume in the
    /// app bar — and every **routing pair** is drawn at most once, exactly once when its presence
    /// reveals it.
    ///
    /// The distinction is the one the pair model forces and is not a weakening: a route row exists
    /// *because* its pair is present, so at Init seven of 125 pairs are drawn and the other 118
    /// deliberately are not. Asserting all 289 are drawn would assert that an unrouted target draws
    /// a group, which is exactly what the interface must not do.
    #[test]
    fn every_host_parameter_is_drawn_once() {
        let params = MxmMonoPr1Params::default();
        let declared: Vec<String> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        let drawn: Vec<&str> = SECTIONS
            .iter()
            .flat_map(|section| section.parameters())
            .copied()
            .chain([BAR_PARAMETER])
            .collect();
        let voice: Vec<&String> = declared
            .iter()
            .filter(|id| !id.starts_with("mod_"))
            .collect();
        for id in &voice {
            assert_eq!(
                drawn.iter().filter(|drawn| **drawn == id.as_str()).count(),
                1,
                "{id}"
            );
        }
        assert_eq!(drawn.len(), voice.len());
        assert_eq!(sections::ALL_IDS.len(), voice.len());
        assert_eq!(
            declared.len() - voice.len(),
            ((mxm_mono_pr1_dsp::routing::TARGETS - 1) * mxm_mono_pr1_dsp::routing::SOURCES
                + mxm_mono_pr1_dsp::routing::SOURCES
                - 1)
                * 2,
            "every id that is not a voice control must be a routing pair"
        );
    }

    /// Each target's stack is drawn under the control it moves, and **no target draws twice**.
    ///
    /// The check is on the id table rather than on painted output, because the pairs a target owns
    /// are exactly the ones `ROUTE_IDS` names for it: if a stack were drawn on the wrong card, or
    /// twice, the keyboard-cursor registry below would carry the same id from two places.
    #[test]
    fn routes_are_drawn_under_their_target() {
        let params = MxmMonoPr1Params::default();
        let mut seen: Vec<&str> = crate::routes::ROUTE_IDS
            .iter()
            .flat_map(|target| target.iter())
            .flat_map(|(amount, present)| [*amount, *present])
            .collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "a routing id is claimed twice");
        let check = |target: usize, routes: &[mxm_modulation_params::Route<'_>]| {
            for (slot, route) in routes.iter().enumerate() {
                assert_eq!(route.amount_id, crate::routes::ROUTE_IDS[target][slot].0);
                assert_eq!(route.present_id, crate::routes::ROUTE_IDS[target][slot].1);
            }
        };
        for (target, group) in params.routes.ordinary() {
            check(target, &group.routes(target));
        }
        check(
            mxm_mono_pr1_dsp::routing::target::MOD_BUS,
            &params.routes.mod_bus.routes(),
        );
        // **The wheel has no pair on the module**, which is the whole of the fix.
        assert_eq!(
            crate::routes::ROUTE_IDS[mxm_mono_pr1_dsp::routing::target::MOD_BUS].len(),
            mxm_mono_pr1_dsp::routing::SOURCES - 1
        );
        assert!(
            !seen.contains(&"mod_modbus_wheel"),
            "the wheel must have no additive pair on the bus it multiplies"
        );
    }
}
