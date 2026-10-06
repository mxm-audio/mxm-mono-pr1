//! mxm-mono-pr1 through MXM Player's ordinary bundle discovery, state and audio paths.
//!
//! This file names the product only at the boundary. The player remains generic: build first with
//! `cargo xtask bundle mxm-mono-pr1 --release`; missing artifacts skip with that instruction.

use mxm_player_harness::app_harness;
use mxm_player_harness::harness;

use clack_extensions::audio_ports_config::{AudioPortsConfigBuffer, PluginAudioPortsConfig};
use clack_host::events::event_types::ParamValueEvent;
use clack_host::prelude::*;
use clack_host::utils::Cookie;
use mxm_player::engine::Engine;
use mxm_player::engine::editor::Ownership;
use mxm_player::events::input::Payload;
use mxm_player::session::{FRAMES_PER_BLOCK, Session};
use std::path::PathBuf;

const PLUGIN: &str = "dk.mxm.mxm-mono-pr1";
const SAMPLE_RATE: f64 = 48_000.0;
const SKIP: &str = "skipping: run `cargo xtask bundle mxm-mono-pr1 --release`";

fn bundle() -> Option<(PathBuf, PathBuf)> {
    let dir = app_harness::any_bundled_dir()?;
    let file = dir.join("mxm-mono-pr1.clap");
    file.exists().then_some((dir, file))
}

fn session(name: &str) -> Option<Session> {
    let (dir, file) = bundle()?;
    let mut session = Session::scratch(name, vec![dir]);
    session.load(&file, PLUGIN);
    Some(session)
}

use mxm_measure::channels::left;

use mxm_measure::convert::note_hz;
/// Peak magnitude of a capture.
///
/// **A shim over `mxm-measure`, and the `expect` is the point.** The shared ruler reports absence for
/// a **non-finite** buffer rather than the largest number in it, because `f32::max` would otherwise
/// let a render that is half NaN measure as perfectly healthy — and then pass every "is it quiet?"
/// assertion below. Panicking here is the loud failure that behaviour deserves.
fn peak(samples: &[f32]) -> f32 {
    mxm_measure::level::peak(samples).expect("the capture is finite")
}

/// How much of one frequency is in a captured window — **a relative figure, not an amplitude.**
///
/// Two reasons it is relative, and both matter to anyone quoting a number from these tests:
///
/// - **The capture length is the session's, not ours.** `component_amplitude` reads a component's
///   true amplitude only over a whole number of cycles; these windows are whole blocks, so the
///   reading carries spectral leakage. Comparing one pitch against another in the same window is
///   sound — the leakage is common to both — and calling the result an absolute amplitude is not.
/// - **The absolute value moved by 6 dB with the migration to the shared probe**, which is a
///   correction rather than a regression: every local copy of this helper computed `|X|/N`, half a
///   component's amplitude, and the shared probe reports the amplitude. A figure quoted from an
///   older run of these tests is 6 dB low.
///
/// **Absence panics rather than reading as zero.** The probe declines for two reasons — an empty
/// window, which cannot happen here, and a **non-finite render**, which can. Folding that into `0.0`
/// would let a NaN-producing plugin sail through every "quieter than" and "silent" assertion below,
/// which is the precise failure the shared crate's result-form contract exists to prevent.
fn magnitude_at(samples: &[f32], hz: f64) -> f64 {
    mxm_measure::spectrum::component_amplitude(samples, hz, SAMPLE_RATE)
        .expect("the capture is non-empty and finite")
}

fn brightness(samples: &[f32]) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    samples
        .windows(2)
        .map(|window| (window[1] - window[0]).abs())
        .sum::<f32>()
        / (samples.len() - 1) as f32
}

fn set(session: &mut Session, name: &str, fraction: f64) -> String {
    let parameter = session
        .state()
        .param(name)
        .unwrap_or_else(|| panic!("missing parameter {name}"))
        .clone();
    session
        .app()
        .engine_mut()
        .push_gui_event(Payload::ParamValue {
            param_id: parameter.id,
            value: parameter.min + fraction * (parameter.max - parameter.min),
        });
    session.advance_blocks(4).expect("parameter edit settles");
    session.state().param(name).unwrap().text.clone()
}

fn capture(session: &mut Session, blocks: u64) -> Vec<f32> {
    session.clear_capture();
    session.advance_blocks(blocks).expect("session advances");
    let audio = session.captured();
    let skip = (FRAMES_PER_BLOCK * 2 * 3).min(audio.len());
    left(&audio[skip..])
}

fn held_note(session: &mut Session, note: u8, blocks: u64) -> Vec<f32> {
    session.app().note_on(note, 0.8);
    let audio = capture(session, blocks);
    session.app().note_off(note);
    session.advance_blocks(160).expect("release advances");
    audio
}

struct ConfigRender {
    name: String,
    input_ports: usize,
    channels: Vec<Vec<f32>>,
}

/// Direct CLAP path used where the ordinary player intentionally does not choose a configuration.
/// It selects one advertised configuration while deactivated, supplies every declared input port
/// in order, and drives the real release bundle.
fn render_configuration(
    file: &std::path::Path,
    config_index: u32,
    edits: &[(&str, f64)],
    input_values: &[f32],
) -> ConfigRender {
    // SAFETY: this is the collection's own bundle from the build tree.
    let entry = unsafe { PluginEntry::load(file) }.expect("load mxm-mono-pr1 bundle");
    let (_shared, mut instance) = harness::instantiate(&entry, PLUGIN).expect("instantiate bundle");
    let extension: PluginAudioPortsConfig = instance
        .plugin_shared_handle()
        .get_extension()
        .expect("audio-ports-config extension");
    assert_eq!(extension.count(&mut instance.plugin_handle()), 2);
    let mut config_buffer = AudioPortsConfigBuffer::new();
    let (id, name, input_ports, output_channels) = {
        let config = extension
            .get(
                &mut instance.plugin_handle(),
                config_index,
                &mut config_buffer,
            )
            .expect("advertised configuration");
        (
            config.id,
            String::from_utf8_lossy(config.name).to_string(),
            config.input_port_count as usize,
            config.main_output.expect("main output").channel_count as usize,
        )
    };
    extension
        .select(&mut instance.plugin_handle(), id)
        .expect("select configuration while deactivated");
    assert_eq!(input_values.len(), input_ports);

    let parameters = mxm_player::params::ParamSet::read(&mut instance);
    let resolved_edits: Vec<_> = edits
        .iter()
        .map(|(name, fraction)| {
            let parameter = parameters
                .params
                .iter()
                .find(|parameter| parameter.name == *name)
                .unwrap_or_else(|| panic!("missing parameter {name}"));
            (
                ClapId::from_raw(parameter.id).expect("valid parameter id"),
                parameter.denormalise(*fraction),
            )
        })
        .collect();

    let processor = instance
        .activate(
            |shared, _| mxm_player::host::HostAudioProcessor::new(shared),
            PluginAudioConfiguration {
                sample_rate: SAMPLE_RATE,
                min_frames_count: 256,
                max_frames_count: 256,
            },
        )
        .expect("activate selected configuration");
    let mut rendered = vec![Vec::new(); output_channels];
    let stopped = {
        let mut processor = processor.start_processing().expect("start processing");
        let mut inputs = vec![vec![0.0f32; 256]; input_ports];
        let mut outputs = vec![vec![0.0f32; 256]; output_channels];
        let mut input_ports_storage = AudioPorts::with_capacity(input_ports, input_ports);
        let mut output_ports_storage = AudioPorts::with_capacity(output_channels, 1);
        let mut input_events = EventBuffer::new();
        let mut output_events = EventBuffer::new();
        for block in 0..48 {
            for (buffer, value) in inputs.iter_mut().zip(input_values) {
                buffer.fill(*value);
            }
            for buffer in &mut outputs {
                buffer.fill(0.0);
            }
            input_events.clear();
            output_events.clear();
            if block == 0 {
                for (id, value) in &resolved_edits {
                    input_events.push(&ParamValueEvent::new(
                        0,
                        *id,
                        Pckn::match_all(),
                        *value,
                        Cookie::empty(),
                    ));
                }
            }
            let audio_inputs =
                input_ports_storage.with_input_buffers(inputs.iter_mut().map(|buffer| {
                    AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_input_only(std::iter::once(
                            InputChannel::variable(buffer),
                        )),
                    }
                }));
            let mut audio_outputs = output_ports_storage.with_output_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only(
                    outputs.iter_mut().map(Vec::as_mut_slice),
                ),
            }]);
            processor
                .process(
                    &audio_inputs,
                    &mut audio_outputs,
                    &InputEvents::from_buffer(&input_events),
                    &mut OutputEvents::from_buffer(&mut output_events),
                    Some(block * 256),
                    None,
                )
                .expect("process selected configuration");
            if block >= 32 {
                for (destination, block) in rendered.iter_mut().zip(&outputs) {
                    destination.extend_from_slice(block);
                }
            }
        }
        processor.stop_processing()
    };
    instance.deactivate(stopped);
    ConfigRender {
        name,
        input_ports,
        channels: rendered,
    }
}

fn channel_peak(channels: &[Vec<f32>]) -> f32 {
    channels
        .iter()
        .flatten()
        .fold(0.0, |peak: f32, sample| peak.max(sample.abs()))
}

/// **Two configurations, stereo then mono, and no input of any kind** — the External audio and
/// Gate / clock inputs were removed (the owner, 2026-09-26). Noise is the mixer's third source and
/// the LFO core is Repeat's only clock, and both still sound through the real bundle.
#[test]
fn both_audio_configurations_negotiate_with_no_inputs_and_noise_and_repeat_still_sound() {
    let Some((_, file)) = bundle() else {
        eprintln!("{SKIP}");
        return;
    };
    for (index, name) in ["Stereo", "Mono"].into_iter().enumerate() {
        let render = render_configuration(&file, index as u32, &[("Drone", 1.0)], &[]);
        assert_eq!((render.name.as_str(), render.input_ports), (name, 0));
        assert!(channel_peak(&render.channels) > 1e-4, "{name} was silent");
        if index == 0 {
            assert_eq!(render.channels.len(), 2);
            assert_eq!(
                render.channels[0], render.channels[1],
                "{name} is not identical stereo duplication"
            );
        } else {
            assert_eq!(render.channels.len(), 1);
        }
    }

    let noise = render_configuration(
        &file,
        0,
        &[
            ("Oscillator A saw", 0.0),
            ("Oscillator A level", 0.0),
            ("Noise level", 1.0),
            ("Drone", 1.0),
        ],
        &[],
    );
    assert!(channel_peak(&noise.channels) > 1e-4, "noise alone sounds");

    let repeat = render_configuration(&file, 0, &[("Repeat", 1.0)], &[]);
    assert!(
        channel_peak(&repeat.channels) > 1e-4,
        "the LFO clock drives Repeat"
    );
}

#[test]
fn player_discovers_the_bundle_and_data_shipped_control_map() {
    let Some((dir, _)) = bundle() else {
        eprintln!("{SKIP}");
        return;
    };
    let mut app = app_harness::AppHarness::new("mono-pr1-discovery", vec![dir]);
    app.harness.state_mut().rescan();
    app.run();

    assert!(
        app.state().lists_plugin(PLUGIN),
        "ordinary discovery missed {PLUGIN}"
    );
    assert!(
        app.app().control_map().knows_instrument(PLUGIN),
        "the map staged beside the bundle was not loaded"
    );
}

#[test]
fn bundle_reports_the_complete_generic_parameter_and_map_surface() {
    let Some(mut session) = session("mono-pr1-params") else {
        eprintln!("{SKIP}");
        return;
    };
    // 40 voice controls — the LFO's tempo sync the newest — and 278 routing parameters: nine
    // targets by fourteen sources plus the summing module's thirteen, a presence and an amount
    // each; the ninth is the standard Amplitude (2026-09-26). Fifteen ids retired — the eleven the
    // Direct/Wheel buses named, the two hard-wired cutoff amounts, and the wheel's pair on the bus
    // it multiplies.
    assert_eq!(session.state().plugin.as_ref().unwrap().params.len(), 318);
    for name in [
        "Key mode",
        "LFO square",
        "Oscillator A sync",
        "Oscillator B low frequency",
        "Noise level",
        "Volume",
        // The routing surface as a host sees it: a presence and a signed amount per pair, named for
        // the target and its source.
        "Cutoff from Filter envelope",
        "Cutoff from Filter envelope on",
        "Mod bus from Oscillator B",
        "Oscillator A frequency from Wheel bus on",
        // The multiplier is a routable target of its own — decision 1.14.
        "Wheel bus from Mod bus",
        "Wheel bus from Wheel",
        "Wheel bus from LFO on",
    ] {
        assert!(session.state().param(name).is_some(), "missing {name}");
    }
    for name in [
        "LFO amount",
        "Filter envelope bus amount",
        "Filter envelope route",
        "Oscillator A frequency route",
        "Filter envelope amount",
        "Filter keyboard amount",
        // The wheel has no additive pair on the bus it multiplies.
        "Mod bus from Wheel",
        "Mod bus from Wheel on",
    ] {
        assert!(
            session.state().param(name).is_none(),
            "{name} retired with the Direct/Wheel buses and must not reappear"
        );
    }
    let cutoff = session.state().param("Cutoff").unwrap().id;
    let sync = session.state().param("Oscillator A sync").unwrap().id;
    assert_eq!(
        session
            .app()
            .control_map()
            .param_for(PLUGIN, "filter.cutoff"),
        Some(cutoff)
    );
    assert_eq!(
        session.app().control_map().param_for(PLUGIN, "osc2.sync"),
        Some(sync)
    );
}

#[test]
fn init_is_silent_at_rest_and_a_note_has_pitch_then_reaches_exact_silence() {
    let Some(mut session) = session("mono-pr1-note") else {
        eprintln!("{SKIP}");
        return;
    };
    session.advance_blocks(20).expect("idle advances");
    assert_eq!(
        peak(&session.captured()),
        0.0,
        "rest is exact digital silence"
    );

    let note = 57;
    let held = held_note(&mut session, note, 45);
    assert!(peak(&held) > 0.01, "held note peak was {}", peak(&held));
    let hz = note_hz(f64::from(note));
    let at = magnitude_at(&held, hz);
    let below = magnitude_at(&held, hz / 2f64.powf(1.0 / 12.0));
    let above = magnitude_at(&held, hz * 2f64.powf(1.0 / 12.0));
    assert!(
        at > 2.0 * below && at > 2.0 * above,
        "A3 should dominate adjacent semitones: {at:.5} vs {below:.5}/{above:.5}"
    );

    session.clear_capture();
    session.advance_blocks(40).expect("inert interval advances");
    assert_eq!(
        peak(&session.captured()),
        0.0,
        "the finite release must end exactly"
    );
}

#[test]
fn a_host_cutoff_edit_changes_rendered_audio() {
    let Some(mut session) = session("mono-pr1-cutoff") else {
        eprintln!("{SKIP}");
        return;
    };
    let open = held_note(&mut session, 45, 45);
    let text = set(&mut session, "Cutoff", 0.25);
    let closed = held_note(&mut session, 45, 45);
    assert!(
        brightness(&closed) < 0.55 * brightness(&open),
        "closing cutoff to {text} should darken the note: {} vs {}",
        brightness(&closed),
        brightness(&open)
    );
}

#[test]
fn normal_mode_keeps_low_note_priority_across_the_host_event_path() {
    let Some(mut session) = session("mono-pr1-normal-priority") else {
        eprintln!("{SKIP}");
        return;
    };
    let low_hz = note_hz(57.0);
    let high_hz = note_hz(69.0);

    session.app().note_on(69, 0.8);
    let high = capture(&mut session, 30);
    assert!(magnitude_at(&high, high_hz) > magnitude_at(&high, low_hz));

    session.app().note_on(57, 0.8);
    let both = capture(&mut session, 30);
    assert!(
        magnitude_at(&both, low_hz) > magnitude_at(&both, high_hz),
        "the lower held note did not take ownership"
    );

    session.app().note_off(57);
    let returned = capture(&mut session, 30);
    assert!(
        magnitude_at(&returned, high_hz) > magnitude_at(&returned, low_hz),
        "releasing the low note did not return to the held high note"
    );
    session.app().note_off(69);
}

#[test]
fn retrigger_mode_gives_the_latest_press_ownership() {
    let Some(mut session) = session("mono-pr1-retrigger-priority") else {
        eprintln!("{SKIP}");
        return;
    };
    assert_eq!(set(&mut session, "Key mode", 1.0), "Retrigger");
    session.app().note_on(57, 0.8);
    let low = capture(&mut session, 25);
    session.app().note_on(69, 0.8);
    let high = capture(&mut session, 25);
    assert!(magnitude_at(&low, note_hz(57.0)) > magnitude_at(&low, note_hz(69.0)));
    assert!(magnitude_at(&high, note_hz(69.0)) > magnitude_at(&high, note_hz(57.0)));
    session.app().note_off(69);
    session.app().note_off(57);
}

#[test]
fn drone_keeps_the_host_awake_without_a_key_then_returns_to_silence() {
    let Some(mut session) = session("mono-pr1-drone") else {
        eprintln!("{SKIP}");
        return;
    };
    assert_eq!(set(&mut session, "Drone", 1.0), "On");
    let drone = capture(&mut session, 120);
    assert!(
        peak(&drone) > 0.01,
        "Drone slept or was inaudible: {}",
        peak(&drone)
    );

    assert_eq!(set(&mut session, "Drone", 0.0), "Off");
    session.advance_blocks(240).expect("tail advances");
    session.clear_capture();
    session.advance_blocks(30).expect("inert interval advances");
    assert_eq!(
        peak(&session.captured()),
        0.0,
        "leaving Drone must finish its tail"
    );
}

#[test]
fn clap_state_restores_the_same_instances_complete_patch() {
    let Some(mut session) = session("mono-pr1-state") else {
        eprintln!("{SKIP}");
        return;
    };
    assert!(
        session
            .app()
            .run_cli_command("set Cutoff 0.73")
            .contains("\"ok\"")
    );
    assert!(
        session
            .app()
            .run_cli_command("set Oscillator_B_level 0.41")
            .contains("\"ok\"")
    );
    assert!(
        session
            .app()
            .run_cli_command("set Oscillator_A_pulse 1")
            .contains("\"ok\"")
    );
    session.advance_blocks(4).expect("edits settle");
    let expected: Vec<_> = ["Cutoff", "Oscillator B level", "Oscillator A pulse"]
        .into_iter()
        .map(|name| (name, session.state().param(name).unwrap().value))
        .collect();
    assert!(
        session
            .app()
            .run_cli_command("dumpstate")
            .contains("\"ok\"")
    );

    assert!(
        session
            .app()
            .run_cli_command("set Cutoff 0.11")
            .contains("\"ok\"")
    );
    assert!(
        session
            .app()
            .run_cli_command("set Oscillator_B_level 0.91")
            .contains("\"ok\"")
    );
    assert!(
        session
            .app()
            .run_cli_command("set Oscillator_A_pulse 0")
            .contains("\"ok\"")
    );
    session.advance_blocks(4).expect("mutations settle");
    assert!(
        session
            .app()
            .run_cli_command("loadstate")
            .contains("\"ok\"")
    );
    session.advance_blocks(4).expect("state restore settles");

    let actual = session.app().engine_mut().read_params();
    for (name, value) in expected {
        let restored = actual
            .params
            .iter()
            .find(|param| param.name == name)
            .unwrap()
            .value;
        assert!(
            (restored - value).abs() < 1e-6,
            "{name}: {restored} != {value}"
        );
    }
}

#[test]
fn editor_is_advertised_through_the_player_hosting_path() {
    let Some((_, file)) = bundle() else {
        eprintln!("{SKIP}");
        return;
    };
    let mut engine = Engine::new();
    engine.load(&file, PLUGIN).expect("load mxm-mono-pr1");
    assert!(
        engine.editor_available().is_ok(),
        "the bundle does not expose clap.gui"
    );
    assert!(
        engine.editor_floating_supported(),
        "the production editor does not support the player's floating path"
    );
}

/// Real lifecycle proof, ignored in ordinary package runs because it creates an OS window. The
/// factory invokes it deliberately after bundling; visual quality and DAW parenting remain manual.
#[test]
#[ignore = "creates a real window; needs a display"]
fn editor_opens_closes_and_reopens_through_the_player() {
    let Some((_, file)) = bundle() else {
        eprintln!("{SKIP}");
        return;
    };
    let mut engine = Engine::new();
    engine.load(&file, PLUGIN).expect("load mxm-mono-pr1");
    for attempt in 1..=2 {
        let state = engine
            .open_editor(None)
            .unwrap_or_else(|why| panic!("attempt {attempt}: {}", why.message()));
        assert!(state.open, "attempt {attempt}: editor did not report open");
        assert!(
            matches!(state.owned, Ownership::Unowned(_)),
            "attempt {attempt}: claimed ownership without a host window"
        );
        engine.close_editor();
        assert!(
            !engine.editor_state().open,
            "attempt {attempt}: close was not acknowledged"
        );
    }
}
