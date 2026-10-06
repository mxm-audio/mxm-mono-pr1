//! Semantic, software-native telemetry views from the approved brief.

use egui::{Pos2, Sense, Stroke, Ui, Vec2, pos2};
use mxm_ui::{
    theme::Tokens,
    visual::{
        AXIS_STROKE, CANVAS_RADIUS, EMPHASIS_STROKE, INNER_GUTTER, METER_LABEL_WIDTH, METER_STROKE,
        PLOT_HEIGHT, REFERENCE_STROKE, STATUS_HEIGHT, TALL_PLOT_HEIGHT, TRACE_REACH, TRACE_STROKE,
    },
};

fn canvas(ui: &mut Ui, tokens: &Tokens, name: &str, height: f32) -> (egui::Rect, egui::Painter) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, name));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CANVAS_RADIUS, tokens.surface_2);
    painter.line_segment(
        [
            pos2(rect.left(), rect.center().y),
            pos2(rect.right(), rect.center().y),
        ],
        Stroke::new(AXIS_STROKE, tokens.border),
    );
    painter.rect_stroke(
        rect,
        CANVAS_RADIUS,
        Stroke::new(AXIS_STROKE, tokens.border),
        egui::StrokeKind::Inside,
    );
    (rect, painter)
}

/// The LFO's summed shape, as its three switches make it: a picture of the waveform, not a
/// running one — no marker moving along it (the owner, 2026-09-28: *remove the dancing ball*).
pub fn lfo_sum_trace(ui: &mut Ui, tokens: &Tokens, saw: bool, triangle: bool, square: bool) {
    let (rect, painter) = canvas(ui, tokens, "LFO summed waveform", PLOT_HEIGHT);
    let values: Vec<f32> = (0..256)
        .map(|index| {
            let phase = index as f32 / 255.0;
            let saw_value = 2.0 * phase - 1.0;
            let triangle_value = if phase < 0.25 {
                4.0 * phase
            } else if phase < 0.75 {
                2.0 - 4.0 * phase
            } else {
                4.0 * phase - 4.0
            };
            let square_value = if phase < 0.5 { 1.0 } else { -1.0 };
            (if saw { saw_value } else { 0.0 })
                + (if triangle { triangle_value } else { 0.0 })
                + (if square { square_value } else { 0.0 })
        })
        .collect();
    trace(&painter, rect, &values, TRACE_REACH, tokens.accent);
}

/// `values` across `rect`, scaled to their own peak about the centre line.
fn trace(
    painter: &egui::Painter,
    rect: egui::Rect,
    values: &[f32],
    reach: f32,
    colour: egui::Color32,
) {
    let peak = values
        .iter()
        .fold(1.0e-3f32, |peak, value| peak.max(value.abs()));
    let points: Vec<Pos2> = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            pos2(
                rect.left() + index as f32 / (values.len() - 1).max(1) as f32 * rect.width(),
                rect.center().y - (value / peak).clamp(-1.0, 1.0) * rect.height() * reach,
            )
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(TRACE_STROKE, colour)));
}

/// One oscillator's waveform, as its switches and pulse width make it: two cycles of the waves
/// it has on, added and centred. **A picture, not a scope** (the owner, 2026-09-28: *not
/// animated — just show the waveform*): it changes when a setting does, and holds still otherwise.
/// The shapes are the oscillator's own — a rising saw, a triangle, a pulse high for `width` of
/// the cycle.
pub fn oscillator_shape(
    ui: &mut Ui,
    tokens: &Tokens,
    name: &str,
    waves: OscillatorWaves,
    colour: egui::Color32,
) {
    let (rect, painter) = canvas(ui, tokens, name, TALL_PLOT_HEIGHT);
    if let Some(values) = waves.cycles(2, 384) {
        trace(&painter, rect, &values, TRACE_REACH, colour);
    }
}

/// Which waves an oscillator has on, and its pulse width, `0..=1`.
#[derive(Clone, Copy, Debug)]
pub struct OscillatorWaves {
    pub saw: bool,
    pub triangle: bool,
    pub pulse: bool,
    pub width: f32,
}

impl OscillatorWaves {
    /// `cycles` cycles in `points` samples, centred on zero; `None` with every wave off.
    fn cycles(self, cycles: usize, points: usize) -> Option<Vec<f32>> {
        if !(self.saw || self.triangle || self.pulse) {
            return None;
        }
        let width = self.width.clamp(0.0, 1.0);
        let values: Vec<f32> = (0..points)
            .map(|index| {
                let phase = (index as f32 / (points - 1) as f32 * cycles as f32).fract();
                (if self.saw { phase - 0.5 } else { 0.0 })
                    + (if self.triangle {
                        if phase < 0.5 {
                            2.0 * phase - 0.5
                        } else {
                            1.5 - 2.0 * phase
                        }
                    } else {
                        0.0
                    })
                    + (if self.pulse {
                        (if phase < width { 1.0 } else { 0.0 }) - width
                    } else {
                        0.0
                    })
            })
            .collect();
        Some(values)
    }
}

/// The shortest a module's meter is drawn beside its name.
const METER_MIN: f32 = 48.0;

/// The narrowest [`bus_level`] is drawn: its name's column, the shortest meter, and the gutter
/// after it. The name is painted in its fixed [`METER_LABEL_WIDTH`] column however wide the display
/// is, so below this the meter would run backwards through it. It fills the card's width at
/// [`STATUS_HEIGHT`].
pub const BUS_LEVEL_MIN_WIDTH: f32 = METER_LABEL_WIDTH + METER_MIN + INNER_GUTTER;

/// One module's live level, on that module's own card.
///
/// It was a two-row panel on a single `Modulation` card until the owner asked, 2026-09-14, that a
/// card be named for the thing in it. Two modules, two cards, one level each.
pub fn bus_level(ui: &mut Ui, tokens: &Tokens, name: &str, level: f32, hover: &str) {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), STATUS_HEIGHT),
        Sense::hover(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, format!("{name} activity"))
    });
    response.on_hover_text(hover);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CANVAS_RADIUS, tokens.surface_2);
    let values = [(name, level.abs(), tokens.mod_lfo)];
    let row = rect.height() / values.len() as f32;
    for (index, (label, value, colour)) in values.iter().enumerate() {
        let y = rect.top() + (index as f32 + 0.5) * row;
        painter.text(
            pos2(rect.left() + INNER_GUTTER, y),
            egui::Align2::LEFT_CENTER,
            *label,
            mxm_ui::typography::caption_style(ui.style()).resolve(ui.style()),
            tokens.text_secondary,
        );
        let left = rect.left() + METER_LABEL_WIDTH;
        painter.line_segment(
            [pos2(left, y), pos2(rect.right() - INNER_GUTTER, y)],
            Stroke::new(METER_STROKE, tokens.border),
        );
        painter.line_segment(
            [
                pos2(left, y),
                pos2(
                    left + (rect.right() - INNER_GUTTER - left) * value.clamp(0.0, 1.0),
                    y,
                ),
            ],
            Stroke::new(METER_STROKE, *colour),
        );
    }
    painter.rect_stroke(
        rect,
        CANVAS_RADIUS,
        Stroke::new(AXIS_STROKE, tokens.border),
        egui::StrokeKind::Inside,
    );
}

pub fn filter_response(
    ui: &mut Ui,
    tokens: &Tokens,
    set_hz: f32,
    effective_hz: f32,
    resonance: f32,
) {
    let (rect, painter) = canvas(
        ui,
        tokens,
        "Analytic set and effective filter response",
        PLOT_HEIGHT,
    );
    // What the two curves are, on hover rather than printed (design system §7.6).
    ui.interact(rect, ui.id().with("filter-response"), Sense::hover())
        .on_hover_text(
            "A guide to the filter's shape: one curve where Cutoff is set, the other where \
             modulation has moved it.",
        );
    for (cutoff, colour, width) in [
        (effective_hz, tokens.mod_envelope, REFERENCE_STROKE),
        (set_hz, tokens.accent, EMPHASIS_STROKE),
    ] {
        let points: Vec<Pos2> = (0..128)
            .map(|index| {
                let t = index as f32 / 127.0;
                let hz = 10.0 * 2000.0f32.powf(t);
                let ratio = hz / cutoff.max(1.0);
                let rolloff = -10.0 * (1.0 + ratio.powi(8)).log10();
                let peak = resonance.clamp(0.0, 1.0) * 15.0 * (-8.0 * ratio.log2().powi(2)).exp();
                let db = rolloff + peak;
                pos2(
                    rect.left() + t * rect.width(),
                    rect.bottom() - ((db + 48.0) / 66.0).clamp(0.0, 1.0) * rect.height(),
                )
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(width, colour)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **An oscillator's picture is its settings**: a pulse is high for its width's share of each
    /// cycle, a saw rises through each cycle, and nothing is drawn with every wave off.
    #[test]
    fn an_oscillator_is_drawn_from_its_settings() {
        let off = OscillatorWaves {
            saw: false,
            triangle: false,
            pulse: false,
            width: 0.5,
        };
        assert!(off.cycles(2, 64).is_none());

        let pulse = OscillatorWaves {
            pulse: true,
            width: 0.25,
            ..off
        };
        let values = pulse.cycles(1, 401).expect("a pulse draws");
        let high = values.iter().filter(|v| **v > 0.0).count() as f32 / values.len() as f32;
        assert!((high - 0.25).abs() < 0.01, "high for {high} of the cycle");

        let saw = OscillatorWaves { saw: true, ..off };
        let values = saw.cycles(2, 400).expect("a saw draws");
        let first_half = &values[..199];
        assert!(
            first_half.windows(2).all(|pair| pair[1] > pair[0]),
            "a saw rises through its cycle"
        );
    }
}
