//! Automated proof for the production editor: dynamic geometry from the paging report,
//! accessibility through AccessKit, and balanced parameter gestures through an applying host.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use egui::{Rect, ThemePreference, vec2};
use kittest::{By, Queryable};
use nice_plug::{
    params::internals::ParamPtr,
    prelude::{ParamSetter, PluginApi, PluginState},
};

use super::*;

#[derive(Default)]
struct ApplyingHost(Mutex<Vec<(&'static str, String)>>);

impl ApplyingHost {
    fn record(&self, action: &'static str, param: ParamPtr) {
        self.0
            .lock()
            .unwrap()
            .push((action, unsafe { param.name() }.to_owned()));
    }

    fn assert_gesture(&self, name: &str) {
        assert_eq!(
            *self.0.lock().unwrap(),
            ["begin", "set", "end"].map(|action| (action, name.to_owned()))
        );
        self.0.lock().unwrap().clear();
    }
}

impl nice_plug::context::gui::GuiContextInner for ApplyingHost {
    // A test double has no host to ask for a restart (nice-plug 0.4).
    fn request_restart(&self) {}
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, param: ParamPtr) {
        self.record("begin", param);
    }
    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, value: f32) {
        self.record("set", param);
        unsafe {
            param._internal_set_normalized_value(value);
        }
    }
    unsafe fn raw_end_set_parameter(&self, param: ParamPtr) {
        self.record("end", param);
    }
    fn get_state(&self) -> PluginState {
        PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        }
    }
    fn set_state(&self, _state: PluginState) {}
}

use mxm_plugin_test::{opening_size, paging_checks};

fn render_layout(width: f32) -> Vec<Rect> {
    let params = MxmMonoPr1Params::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let mut view = 0;
    let mut entries = HashMap::new();
    let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
    let mut nav = mxm_ui::navigation::State::default();
    let ctx = egui::Context::default();
    mxm_ui::typography::apply(&ctx);
    mxm_ui::theme::apply(&ctx);
    ctx.set_theme(ThemePreference::Light);
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 20_000.0))),
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
    assert!(
        host.0.lock().unwrap().is_empty(),
        "measurement emitted a host gesture"
    );
    paging_checks::all_rects(&ctx, SECTIONS.len())
}

fn render_section_text(section: Section, width: f32) -> Vec<(String, Rect, f32)> {
    fn collect(shape: &egui::Shape, runs: &mut Vec<(String, Rect, f32)>) {
        match shape {
            egui::Shape::Text(text) => {
                let preceding_rows: f32 = text
                    .galley
                    .rows
                    .iter()
                    .take(text.galley.rows.len().saturating_sub(1))
                    .map(|row| row.size.y)
                    .sum();
                let baseline = text
                    .galley
                    .rows
                    .last()
                    .and_then(|row| row.glyphs.first())
                    .map_or(text.pos.y, |glyph| {
                        text.pos.y + preceding_rows + glyph.pos.y
                    });
                runs.push((
                    text.galley.text().to_owned(),
                    text.visual_bounding_rect(),
                    baseline,
                ));
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, runs);
                }
            }
            _ => {}
        }
    }

    let params = MxmMonoPr1Params::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let mut entries = HashMap::new();
    let ctx = egui::Context::default();
    mxm_ui::typography::apply(&ctx);
    mxm_ui::theme::apply(&ctx);
    ctx.set_theme(ThemePreference::Light);
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 2_000.0))),
        ..Default::default()
    };
    let mut output = None;
    for _ in 0..3 {
        let mut frame = ctx.run_ui(input.clone(), |ui| {
            mxm_ui::ModuleCard::new(section.title()).show(ui, &mxm_ui::LIGHT, |ui| {
                let tree = sections::card(ui, section, &params);
                let mut live = sections::Live {
                    params: &params,
                    telemetry: &telemetry,
                    setter: &setter,
                    entries: &mut entries,
                };
                mxm_ui::tree::show(ui, &mxm_ui::LIGHT, &tree, |ui, leaf, rect| {
                    sections::paint(ui, &mxm_ui::LIGHT, leaf, rect, &mut live);
                });
            });
        });
        frame.textures_delta.clear();
        output = Some(frame);
    }
    assert!(host.0.lock().unwrap().is_empty());
    let mut runs = Vec::new();
    for clipped in &output.expect("section rendered").shapes {
        collect(&clipped.shape, &mut runs);
    }
    runs
}

/// Every route present, at full negative depth: the widest reading a row can show, with its sign
/// and every digit. The init patch cannot reach it — seven of 125 pairs are present at Init.
fn reveal_every_route(params: &MxmMonoPr1Params) {
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let mut every = Vec::new();
    for (target, group) in params.routes.ordinary() {
        every.extend(group.routes(target));
    }
    every.extend(params.routes.mod_bus.routes());
    for route in &every {
        mxm_modulation_params::add(route, &setter);
        route.amount.set(&setter, 0.0);
    }
}

/// Every card, in every state that changes what it holds, passes the layout tree's checks
/// (plans/plan-layout-tree.md §4.3, `tree_checks::card`): its computed floor holds its content with
/// nothing painted outside the card, the content floor is exact, the height its tree states is the
/// height it draws, and every leaf stays in the room it was given.
///
/// The states are this editor's structural-state matrix. It has no disclosure and nothing reserved,
/// so they are: the init patch; every route revealed at full negative depth; and the LFO synced,
/// with no tempo and with one, where its rate reads a division. The floors are the Init floors in
/// every state — a route stack's floor is already every route revealed.
#[test]
fn every_card_passes_the_tree_checks_in_every_state() {
    let floors = test_floors();
    for state in [
        "init",
        "every route revealed",
        "LFO synced, no tempo",
        "LFO synced to a tempo",
    ] {
        let params = MxmMonoPr1Params::default();
        if state == "every route revealed" {
            reveal_every_route(&params);
        }
        let telemetry = Telemetry::default();
        if state.starts_with("LFO synced") {
            // SAFETY: the parameters are this test's own and nothing else reads them.
            unsafe {
                use nice_plug::params::InternalParamMut;
                let _ = params.lfo_sync._internal_set_normalized_value(1.0);
            }
        }
        if state == "LFO synced to a tempo" {
            telemetry.tempo.publish(Some(120.0));
        }
        let host = ApplyingHost::default();
        let setter = ParamSetter::new(&host);
        for (index, section) in SECTIONS.iter().enumerate() {
            let mut entries = HashMap::new();
            let mut live = sections::Live {
                params: &params,
                telemetry: &telemetry,
                setter: &setter,
                entries: &mut entries,
            };
            tree_checks::card(
                &|_| {},
                state,
                section.title(),
                floors[index],
                &|ui| sections::card(ui, *section, &params),
                &mut |ui, leaf, rect| sections::paint(ui, &mxm_ui::LIGHT, leaf, rect, &mut live),
            );
        }
        assert!(
            host.0.lock().unwrap().is_empty(),
            "drawing a card emitted a host gesture"
        );
    }
}

fn rows(rects: &[Rect]) -> Vec<Vec<(usize, Rect)>> {
    let mut ordered: Vec<_> = rects.iter().copied().enumerate().collect();
    ordered.sort_by(|(_, a), (_, b)| {
        a.top()
            .total_cmp(&b.top())
            .then(a.left().total_cmp(&b.left()))
    });
    let mut rows: Vec<Vec<(usize, Rect)>> = Vec::new();
    for item in ordered {
        match rows.last_mut() {
            Some(row)
                if row
                    .iter()
                    .any(|(_, rect)| rect.bottom() > item.1.top() + 1.0) =>
            {
                row.push(item)
            }
            _ => rows.push(vec![item]),
        }
    }
    rows
}

fn assert_geometry(rects: &[Rect]) {
    assert_eq!(rects.len(), SECTIONS.len());
    for (index, (rect, floor)) in rects.iter().zip(test_floors()).enumerate() {
        assert!(
            (rect.width() - floor).abs() <= 0.75,
            "{} is not its {floor}-point floor: {rect:?}",
            SECTIONS[index].title()
        );
    }
    for (index, first) in rects.iter().enumerate() {
        for second in rects.iter().skip(index + 1) {
            let overlap = first.intersect(*second);
            assert!(
                overlap.width() <= 0.75 || overlap.height() <= 0.75,
                "cards overlap: {first:?} and {second:?}"
            );
        }
    }
    for row in rows(rects) {
        if row.len() > 1 {
            let low = row
                .iter()
                .map(|(_, rect)| rect.bottom())
                .fold(f32::INFINITY, f32::min);
            let high = row
                .iter()
                .map(|(_, rect)| rect.bottom())
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                high - low < 1.0,
                "row bottoms differ by {:.1}: {row:?}",
                high - low
            );
        }
    }
    let order: Vec<_> = rows(rects)
        .into_iter()
        .flatten()
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        order,
        (0..SECTIONS.len()).collect::<Vec<_>>(),
        "reflow changed stable sequence"
    );
}

#[test]
fn dense_cards_have_compact_labels_and_no_painted_text_collisions_at_their_floors() {
    let floors = test_floors();
    let cases = [
        (Section::Envelopes, floors[2]),
        (Section::ModBus, floors[3]),
        (Section::WheelBus, floors[4]),
        (Section::Mixer, floors[7]),
        (Section::Filter, floors[8]),
    ];
    for (section, width) in cases {
        let runs = render_section_text(section, width);
        for (index, (first_text, first, _)) in runs.iter().enumerate() {
            for (second_text, second, _) in runs.iter().skip(index + 1) {
                let overlap = first.intersect(*second);
                assert!(
                    overlap.width() <= 0.5 || overlap.height() <= 0.5,
                    "{} paints {first_text:?} over {second_text:?}: {first:?} / {second:?}",
                    section.title()
                );
            }
        }
    }

    let filter_envelope: Vec<_> = render_section_text(Section::Envelopes, floors[2])
        .into_iter()
        .map(|(text, _, _)| text)
        .collect();
    // An envelope's faders paint their stages' letters, by the convention (the owner, 2026-09-25).
    for label in ["A", "D", "S", "R"] {
        assert!(filter_envelope.iter().any(|text| text == label), "{label}");
    }
    assert!(
        !filter_envelope
            .iter()
            .any(|text| text.starts_with("Filter envelope "))
    );

    // **The bus card's own baseline check retired with its knobs.** It held three source knobs
    // each beside a Direct/Wheel selector, and the alignment between the two was what it asserted.
    // What is on that card now is one route stack, whose row alignment belongs to the shared widget
    // and is checked by its consumer rule rather than by three hand-named labels.
    // **Each module's card is named for the module**, so its title, its rows and its entry in
    // every `‹ modulate ›` list read the same word — the owner's ruling, 2026-09-14.
    for (section, floor, name) in [
        (Section::ModBus, floors[3], "Mod bus"),
        (Section::WheelBus, floors[4], "Wheel bus"),
    ] {
        let painted = render_section_text(section, floor);
        assert_eq!(section.title(), name);
        assert!(
            painted.iter().any(|(text, _, _)| text == name),
            "{name} must name itself on its own card"
        );
    }
}

#[test]
fn narrow_default_and_wide_reflow_keep_floors_order_alignment_and_no_overlap() {
    for width in [MINIMUM.0 as f32, REFERENCE.0 as f32, 2200.0] {
        assert_geometry(&render_layout(width));
    }
}

#[test]
fn every_dynamic_page_fits_the_quarter_4k_budget_and_every_card_is_reachable() {
    let params = MxmMonoPr1Params::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let mut view = 0;
    let mut entries = HashMap::new();
    let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
    let mut nav = mxm_ui::navigation::State::default();
    paging_checks::verify(
        &test_items(),
        &[
            vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            vec2(1880.0, 1040.0),
            vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
        ],
        |ui| {
            panel(
                ui,
                &params,
                &telemetry,
                &ParamSetter::new(&host),
                &mut view,
                &mut entries,
                &mut presets,
                &mut nav,
            )
        },
    );
    assert!(
        host.0.lock().unwrap().is_empty(),
        "measurement/navigation emitted parameter gestures"
    );
}

#[test]
fn preferred_parallel_groups_stay_together_when_the_default_row_can_hold_them() {
    let rects = render_layout(REFERENCE.0 as f32);
    for (a, b, name) in [
        // The envelopes are one card now (the owner, 2026-09-25), so they are no pair to keep.
        // The two modules are a pair because one feeds the other.
        (3, 4, "modules"),
        (5, 6, "oscillators"),
        // No filter-and-amplifier pair: Volume, the amplifier card's one control, is in the app
        // bar, so that card and its pairing are gone.
    ] {
        assert!(
            (rects[a].top() - rects[b].top()).abs() < 1.0,
            "the preferred {name} pair split despite fitting: {:?} {:?}",
            rects[a],
            rects[b]
        );
    }
}

#[test]
fn lone_cards_never_stretch_and_the_narrow_flow_stays_inside_the_workspace() {
    let rects = render_layout(MINIMUM.0 as f32);
    let floors = test_floors();
    for row in rows(&rects) {
        if row.len() == 1 {
            assert!(
                (row[0].1.width() - floors[row[0].0]).abs() <= 0.75,
                "a lone card stretched: {row:?}"
            );
        }
    }
    for rect in rects {
        assert!(
            rect.left() >= SPACE_5 - 0.75,
            "card left the workspace: {rect:?}"
        );
        assert!(
            rect.right() <= MINIMUM.0 as f32 - SPACE_5 + 0.75,
            "card clipped at the narrow edge: {rect:?}"
        );
    }
}

fn editor_harness(
    params: Arc<MxmMonoPr1Params>,
    telemetry: Arc<Telemetry>,
    host: Arc<ApplyingHost>,
    size: egui::Vec2,
) -> egui_kittest::Harness<'static> {
    let params = Box::leak(Box::new(params));
    let telemetry = Box::leak(Box::new(telemetry));
    let host = Box::leak(Box::new(host));
    let view = Box::leak(Box::new(0usize));
    let entries = Box::leak(Box::new(HashMap::new()));
    let presets = Box::leak(Box::new(PresetUi::at(
        crate::preset::Library::at(None),
        params.as_ref(),
    )));
    // Leaked with the rest: the harness keeps the panel alive past this call, and the cursor is
    // per-editor state that has to survive between its frames.
    let nav = Box::leak(Box::new(mxm_ui::navigation::State::default()));
    egui_kittest::Harness::builder()
        .with_size(size)
        .build_ui(move |ui| {
            mxm_ui::typography::apply(ui.ctx());
            mxm_ui::theme::apply(ui.ctx());
            panel(
                ui,
                params.as_ref(),
                telemetry.as_ref(),
                &ParamSetter::new(host.as_ref()),
                view,
                entries,
                presets,
                nav,
            );
        })
}

fn assert_label_reachable_in_card(
    harness: &mut egui_kittest::Harness<'static>,
    card_index: usize,
    label: &str,
    context: &str,
) {
    let report = mxm_ui::paging::editor::report(&harness.ctx).expect("production paging report");
    let card = report
        .visible
        .iter()
        .find(|(key, _)| key.0 == card_index as u64)
        .expect("requested card is visible")
        .1;
    let node = harness
        .query_all_by_label_contains(label)
        .find(|node| card.contains_rect(node.rect()))
        .unwrap_or_else(|| panic!("{label} did not paint in card {card_index} ({context})"));
    if !report.viewport.contains_rect(node.rect()) {
        node.scroll_to_me();
        harness.run_steps(3);
    }
    let report = mxm_ui::paging::editor::report(&harness.ctx).expect("production paging report");
    let card = report
        .visible
        .iter()
        .find(|(key, _)| key.0 == card_index as u64)
        .expect("requested card remains visible")
        .1;
    let node = harness
        .query_all_by_label_contains(label)
        .find(|node| card.contains_rect(node.rect()))
        .unwrap_or_else(|| panic!("{label} disappeared from card {card_index} ({context})"));
    assert!(
        report.viewport.contains_rect(node.rect()),
        "{label} cannot be scrolled into the fixed physical viewport ({context}): {:?}",
        node.rect()
    );
}

#[test]
fn essential_controls_and_non_text_visuals_are_accessible_and_inside_their_cards() {
    let params = Arc::new(MxmMonoPr1Params::default());
    let telemetry = Arc::new(Telemetry::default());
    let host = Arc::new(ApplyingHost::default());
    let mut harness = editor_harness(
        Arc::clone(&params),
        telemetry,
        Arc::clone(&host),
        vec2(MINIMUM.0 as f32, 2600.0),
    );
    harness.run_steps(20);
    let checks = [
        (0, "Glide time"),
        (0, "Master tune"),
        (1, "LFO summed waveform"),
        (1, "LFO rate"),
        (2, "Filter envelope attack"),
        (2, "Amplifier envelope attack"),
        // Each module names itself on its own card — title, rows and menu, one word.
        (3, "Mod bus activity"),
        (3, "Mod bus"),
        (4, "Wheel bus activity"),
        (4, "Wheel bus"),
        (5, "Oscillator A waveform"),
        (5, "Oscillator A pulse width"),
        (6, "Oscillator B keyboard follow"),
        (7, "Noise level"),
        (8, "Analytic set and effective filter response"),
        (8, "Cutoff"),
    ];
    for (card_index, label) in checks {
        mxm_ui::paging::editor::request_card(&harness.ctx, mxm_ui::paging::Key(card_index));
        harness.run_steps(3);
        let card = mxm_ui::paging::editor::report(&harness.ctx)
            .unwrap()
            .visible
            .into_iter()
            .find(|(key, _)| key.0 == card_index)
            .unwrap()
            .1;
        // `get_by_label` is exact, and a target names more than one thing on its card: its own
        // `‹ modulate ›` menu, the knob it moves where it has one, and — for the module — the card
        // itself. **Whichever of them is inside this card is the one being asked about**, and where
        // several are, the slider is the control rather than the caption.
        let inside: Vec<_> = harness
            .query_all_by_label(label)
            .filter(|node| card.contains_rect(node.rect()))
            .collect();
        assert!(
            !inside.is_empty(),
            "nothing labelled {label} inside its card"
        );
        let control = harness
            .query_all_by(|node| {
                node.label().as_deref() == Some(label)
                    && node.role() == egui::accesskit::Role::Slider
            })
            .find(|node| card.contains_rect(node.rect()))
            .map_or_else(|| inside[0].rect(), |slider| slider.rect());
        assert!(
            card.contains_rect(control),
            "{label} clips its card: {control:?} outside {card:?}"
        );
    }
    // Volume is on no card: it is in the app bar, which must still hold it at this width.
    assert_in_the_app_bar(&harness, "Volume", MINIMUM.0 as f32);
    assert!(
        host.0.lock().unwrap().is_empty(),
        "accessibility inspection emitted gestures"
    );
}

/// A control the app bar draws: **once**, above every card, and inside the window.
///
/// Above the paging viewport rather than inside a named rectangle, because the bar has no report of
/// its own: every card is drawn inside that viewport, so a control wholly above it is on none.
fn assert_in_the_app_bar(harness: &egui_kittest::Harness<'static>, label: &str, width: f32) {
    let nodes: Vec<_> = harness.query_all_by_label(label).collect();
    assert_eq!(nodes.len(), 1, "{label} must be drawn exactly once");
    let control = nodes[0].rect();
    let viewport = mxm_ui::paging::editor::report(&harness.ctx)
        .expect("production paging report")
        .viewport;
    assert!(
        control.bottom() <= viewport.top(),
        "{label} {control:?} is not in the app bar above the cards {viewport:?}"
    );
    assert!(
        control.left() >= 0.0 && control.right() <= width,
        "{label} {control:?} leaves the {width}-point window"
    );
}

#[test]
fn every_musician_control_and_semantic_view_paints_inside_its_card_across_scales_and_themes() {
    // **The permanent musician controls, which is what a card owns.** Routing pairs are excluded:
    // a route row is drawn *because* its pair is present, so at Init seven of 125 exist and asking
    // for the other 118 would assert that an unrouted target draws a group.
    let expected_parameters: HashMap<&str, String> = MxmMonoPr1Params::default()
        .all_parameters()
        .into_iter()
        .filter(|(id, _)| !id.starts_with("mod_"))
        .map(|(id, parameter)| (id, parameter.name().to_owned()))
        .collect();
    // **The two activity readouts are gone**, on the owner's ruling of 2026-09-14: they rewrote
    // themselves on every note and told a player nothing they could not already hear. What replaced
    // the voice one is a panic line that draws only when the latch is set, so it is deliberately
    // absent here — `panic_latch_is_shown_only_while_it_is_set` is what covers it instead.
    let semantic_views = [
        (1, "LFO summed waveform"),
        (3, "Mod bus activity"),
        (4, "Wheel bus activity"),
        (5, "Oscillator A waveform"),
        (6, "Oscillator B waveform"),
        // The Mixer's jack status line went with the external inputs (the owner, 2026-09-26).
        (8, "Analytic set and effective filter response"),
    ];

    let fixed_physical_size = vec2(1_880.0, 1_040.0);
    for theme in [0, 1] {
        for scale in [1.0, 1.5, 2.0] {
            let params = Arc::new(MxmMonoPr1Params::default());
            let telemetry = Arc::new(Telemetry::default());
            telemetry.request_theme(theme);
            let host = Arc::new(ApplyingHost::default());
            // Keep the physical window fixed: higher scale means fewer logical points. The harness
            // takes the window at its native one point per pixel, and the scale zooms it, so the
            // window is handed over physical — handing it the logical size zoomed it twice, to a
            // quarter of the area at 2×, below this editor's own one-card minimum.
            let logical_size = fixed_physical_size / scale;
            let mut harness = editor_harness(
                params,
                Arc::clone(&telemetry),
                Arc::clone(&host),
                fixed_physical_size,
            );
            harness.ctx.set_pixels_per_point(scale);
            harness.run_steps(20);
            assert_eq!(harness.ctx.pixels_per_point(), scale);
            assert!(
                (harness.ctx.content_rect().size() - logical_size).length() < 0.5,
                "the window is {:?} logical points, not {logical_size:?}",
                harness.ctx.content_rect().size()
            );
            let mut painted_parameters = std::collections::HashSet::new();
            let mut painted_views = std::collections::HashSet::new();

            for (card_index, section) in SECTIONS.iter().enumerate() {
                mxm_ui::paging::editor::request_card(
                    &harness.ctx,
                    mxm_ui::paging::Key(card_index as u64),
                );
                harness.run_steps(3);
                let context = format!("theme {theme}, scale {scale}");

                for id in section.parameters() {
                    let name = expected_parameters
                        .get(id)
                        .unwrap_or_else(|| panic!("{id} is not a permanent parameter"));
                    assert_label_reachable_in_card(&mut harness, card_index, name, &context);
                    painted_parameters.insert(*id);
                }
                for (_, label) in semantic_views
                    .iter()
                    .filter(|(owner, _)| *owner == card_index)
                {
                    assert_label_reachable_in_card(&mut harness, card_index, label, &context);
                    painted_views.insert(*label);
                }
            }
            // The one musician control on no card: the app bar draws it on every page.
            assert_in_the_app_bar(
                &harness,
                &expected_parameters[BAR_PARAMETER],
                logical_size.x,
            );
            painted_parameters.insert(BAR_PARAMETER);
            assert_eq!(
                painted_parameters.len(),
                expected_parameters.len(),
                "not every musician control painted at theme {theme}, scale {scale}"
            );
            assert_eq!(painted_views.len(), semantic_views.len());
            assert!(host.0.lock().unwrap().is_empty());
        }
    }
}

#[test]
fn source_panel_tuning_controls_are_always_visible_and_cannot_be_collapsed() {
    let params = Arc::new(MxmMonoPr1Params::default());
    let telemetry = Arc::new(Telemetry::default());
    let host = Arc::new(ApplyingHost::default());
    let mut harness = editor_harness(
        params,
        Arc::clone(&telemetry),
        host,
        vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
    );
    mxm_ui::paging::editor::request_card(&harness.ctx, mxm_ui::paging::Key(0));
    harness.run_steps(10);
    harness.get_by_label("Glide time");
    harness.get_by_label("Master tune");
    harness.get_by_label("Bend range");
    telemetry.request_disclosure(false);
    harness.run_steps(10);
    harness.get_by_label("Master tune");
    harness.get_by_label("Bend range");
}

#[test]
fn private_requests_switch_theme_browser_and_parameters_without_audio_edits() {
    let params = Arc::new(MxmMonoPr1Params::default());
    let telemetry = Arc::new(Telemetry::default());
    let host = Arc::new(ApplyingHost::default());
    let mut harness = editor_harness(
        params,
        Arc::clone(&telemetry),
        Arc::clone(&host),
        vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
    );
    harness.run_steps(3);
    mxm_ui::paging::editor::request_card(&harness.ctx, mxm_ui::paging::Key(0));
    harness.run_steps(20);
    harness.get_by_label("Master tune");
    telemetry.request_theme(1);
    harness.run_steps(3);
    assert_eq!(
        mxm_ui::theme::tokens(&harness.ctx).canvas,
        mxm_ui::DARK.canvas,
        "developer theme request was not applied"
    );
    telemetry.request_browser(true);
    harness.run_steps(3);
    harness.get_by_label("Banks");
    telemetry.request_browser(false);
    harness.run_steps(3);
    telemetry.request_view(mxm_ui::paging::PARAMETERS as u8);
    harness.run_steps(3);
    // **Every voice control, and the routing pairs Init reveals.** A route row exists because its
    // pair is present, so 118 of 125 are deliberately undrawn here; asserting all 289 paint would
    // assert that an unrouted target draws a group, which is the thing the interface must not do.
    let params = crate::params::MxmMonoPr1Params::default();
    for (id, parameter) in params.all_parameters() {
        if id.starts_with("mod_") {
            continue;
        }
        // The app bar draws Volume on every view, this one included, so the complete list is its
        // second copy here and only here.
        let expected = if id == BAR_PARAMETER { 2 } else { 1 };
        assert_eq!(
            harness.query_all_by_label(parameter.name()).count(),
            expected,
            "{id}"
        );
    }
    assert!(
        host.0.lock().unwrap().is_empty(),
        "private editor state emitted parameter gestures"
    );
}

#[test]
fn semantic_toggle_segment_and_knob_each_emit_one_balanced_host_gesture() {
    let params = Arc::new(MxmMonoPr1Params::default());
    let telemetry = Arc::new(Telemetry::default());
    let host = Arc::new(ApplyingHost::default());
    let mut harness = editor_harness(
        Arc::clone(&params),
        telemetry,
        Arc::clone(&host),
        vec2(REFERENCE.0 as f32, 1800.0),
    );
    harness.run_steps(3);

    mxm_ui::paging::editor::request_card(&harness.ctx, mxm_ui::paging::Key(0));
    harness.run_steps(3);
    harness.get_by_label("Key mode: Retrigger").click();
    harness.run_steps(2);
    host.assert_gesture("Key mode");

    mxm_ui::paging::editor::request_card(&harness.ctx, mxm_ui::paging::Key(1));
    harness.run_steps(3);
    harness.get_by_label("LFO saw").click();
    harness.run_steps(2);
    host.assert_gesture("LFO saw");

    mxm_ui::paging::editor::request_card(&harness.ctx, mxm_ui::paging::Key(8));
    harness.run_steps(3);
    // The Cutoff knob, not the cutoff stack's `‹ modulate ›` menu, which shares its label.
    let cutoff = harness.get(
        By::new()
            .label("Cutoff")
            .role(egui::accesskit::Role::Slider),
    );
    cutoff.focus();
    harness.key_press(egui::Key::ArrowUp);
    harness.run_steps(2);
    host.assert_gesture("Cutoff");

    // Volume is in the app bar, on every page, and an edit there is one gesture like any other.
    let volume = harness.get_by_label("Volume");
    volume.focus();
    harness.key_press(egui::Key::ArrowUp);
    harness.run_steps(2);
    host.assert_gesture("Volume");
}

/// What this editor keeps behind a disclosure, opened so the opening size is measured with them
/// open (§4.2). Nothing here: every control is on a card or, for Volume, in the app bar.
const REVEAL: fn(&egui::Context) = |_| {};

/// **No status line reports what a player can hear.**
///
/// The owner's ruling, 2026-09-14: *drop that text completely*. `Voice: Live · gate source:
/// Keyboard` and `Output active` rewrote themselves on every note; a panic line kept beside them for
/// one round went the same way on the owner's word. Nothing on this panel narrates state now.
///
/// The check is negative and it is worth having, because a status line is the kind of thing that
/// creeps back one word at a time.
#[test]
fn no_card_narrates_voice_or_output_state() {
    let params = Arc::new(MxmMonoPr1Params::default());
    let telemetry = Arc::new(Telemetry::default());
    let host = Arc::new(ApplyingHost::default());
    // A live voice **and** a latched panic: the two states the retired lines used to announce.
    telemetry.publish_activity(mxm_mono_pr1_dsp::control::Activity::Live, true);
    let mut harness = editor_harness(
        Arc::clone(&params),
        Arc::clone(&telemetry),
        Arc::clone(&host),
        vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
    );
    // Every card, not the two that used to carry the lines: the output card is gone, its Volume is
    // in the app bar, and the bar is painted beside whichever card is asked for.
    for card in 0..SECTIONS.len() as u64 {
        mxm_ui::paging::editor::request_card(&harness.ctx, mxm_ui::paging::Key(card));
        harness.run_steps(3);
        for phrase in [
            "Voice:",
            "gate source",
            "Output active",
            "Output tail",
            "Output inert",
            "Panic latched",
        ] {
            assert!(
                harness.query_all_by_label_contains(phrase).next().is_none(),
                "card {card} still narrates state: {phrase:?}"
            );
        }
    }
    assert!(host.0.lock().unwrap().is_empty());
}

/// **The editor opens at the quarter-4K budget, hugged** — the owner's rule, 2026-09-09. The budget
/// is the most room an editor may ask for, so laying the panel out there shows as many modules as
/// it ever will; taking the slack away is the whole of the size.
#[test]
fn the_opening_size_is_the_budget_hugged() {
    let params = MxmMonoPr1Params::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let mut view = 0usize;
    let mut entries = HashMap::new();
    let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
    let mut nav = mxm_ui::navigation::State::default();
    opening_size::is_the_budget_hugged(
        vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
        &REVEAL,
        &mut |ui| {
            panel(
                ui,
                &params,
                &telemetry,
                &setter,
                &mut view,
                &mut entries,
                &mut presets,
                &mut nav,
            );
        },
    );
}

/// **The app bar holds in the narrowest window**: its `…` menu whole and nothing drawn over
/// anything else, from `MINIMUM` up (`opening_size::bar_holds_from_the_minimum`).
#[test]
fn the_app_bar_holds_in_the_minimum_window() {
    let params = MxmMonoPr1Params::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let mut view = 0usize;
    let mut entries = HashMap::new();
    let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
    let mut nav = mxm_ui::navigation::State::default();
    opening_size::bar_holds_from_the_minimum(vec2(MINIMUM.0 as f32, MINIMUM.1 as f32), &mut |ui| {
        panel(
            ui,
            &params,
            &telemetry,
            &setter,
            &mut view,
            &mut entries,
            &mut presets,
            &mut nav,
        );
    });
}

use mxm_plugin_test::tree_checks;

/// Every page at the opening size, light and dark, for the owner's review of the layout-tree
/// conversion (plans/plan-layout-tree.md §4.3): `target/layout-tree/mxm-mono-pr1/<tag>/`, where
/// `MXM_PICTURES` names the tag — `before` on the unconverted editor, `after` on the tree.
///
/// `MXM_PICTURES=after cargo test -p mxm-mono-pr1 --lib tree_pictures -- --ignored`
#[test]
#[ignore = "renders through wgpu; run by hand"]
fn tree_pictures() {
    let tag = std::env::var("MXM_PICTURES").unwrap_or_else(|_| "after".to_owned());
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/layout-tree/mxm-mono-pr1")
        .join(tag);
    let params = MxmMonoPr1Params::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let mut view = 0usize;
    let mut entries = HashMap::new();
    let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
    let mut nav = mxm_ui::navigation::State::default();
    tree_checks::pictures(
        &|_| {},
        vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
        &dir,
        &mut |ui| {
            panel(
                ui,
                &params,
                &telemetry,
                &setter,
                &mut view,
                &mut entries,
                &mut presets,
                &mut nav,
            );
        },
    );
}
