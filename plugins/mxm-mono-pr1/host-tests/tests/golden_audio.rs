//! Golden audio for mxm-mono-pr1 through the real MXM Player and release bundle.
//!
//! This score deliberately spans additive saw/pulse oscillators, B-to-A sync, PWM/modulation,
//! filter articulation, channel performance controls, ownership changes and the release-to-idle
//! path. A one-note init render is not an oracle for those interactions.
//!
//! # Regenerating the oracle after an intentional sound change
//!
//! 1. Run `cargo xtask bundle mxm-mono-pr1 --release`.
//! 2. Run `cargo test -p mxm-mono-pr1-host-tests --test golden_audio -- --nocapture`.
//! 3. Listen to the WAV path printed by the failed assertion and compare the intended behavior;
//!    review the DSP diff rather than accepting a digest merely because output is non-silent.
//! 4. Replace `GOLDEN_DIGEST` with the reported digest, document why beside the constant, and run
//!    this test again. Never regenerate the digest without the listening and diff review.

use mxm_player_harness::app_harness;

use mxm_player::events::input::Payload;
use mxm_player::session::{FRAMES_PER_BLOCK, Session};
use std::path::PathBuf;

const PLUGIN: &str = "dk.mxm.mxm-mono-pr1";
/// Provisionally repinned after the ordered-edge F001 repair. The focused sync/PWM discriminator
/// passes and `8e1c04ab75ef28f5` was independently reproduced and authorized. No human listening is
/// claimed, and final reference listening remains a manual release gate. Later changes must follow
/// the documented listen-before-update procedure above and record their audible reason here.
const GOLDEN_DIGEST: &str = "8e1c04ab75ef28f5";
const GOLDEN_SAMPLES: usize = 100 * FRAMES_PER_BLOCK * 2;

fn bundle() -> Option<(PathBuf, PathBuf)> {
    let dir = app_harness::any_bundled_dir()?;
    let file = dir.join("mxm-mono-pr1.clap");
    file.exists().then_some((dir, file))
}

fn set(session: &mut Session, name: &str, fraction: f64) {
    let parameter = session.state().param(name).unwrap().clone();
    session
        .app()
        .engine_mut()
        .push_gui_event(Payload::ParamValue {
            param_id: parameter.id,
            value: parameter.min + fraction * (parameter.max - parameter.min),
        });
}

fn score(session: &mut Session, altered_sync_pwm: bool) -> Result<(), String> {
    for (name, value) in [
        // **The patch that was pinned**: Init gained the wheel's vibrato on 2026-09-28 (the LFO
        // into Mod bus, the wheel-scaled bus into both pitches), and this score moves the wheel,
        // so the three routes are switched off here to keep the sound it recorded.
        ("Mod bus from LFO on", 0.0),
        ("Oscillator A frequency from Wheel bus on", 0.0),
        ("Oscillator B frequency from Wheel bus on", 0.0),
        ("Key mode", 1.0),
        ("Oscillator A pulse", 1.0),
        ("Oscillator A pulse width", 0.27),
        ("Oscillator A sync", 1.0),
        ("Oscillator B saw", 1.0),
        ("Oscillator B triangle", 1.0),
        ("Oscillator B level", 0.38),
        ("Oscillator B frequency", 0.63),
        ("LFO saw", 1.0),
        // **The same sound, through the routes that replaced three retired controls.** The legacy
        // score set `LFO amount` to 0.31 with every destination switch left at its `Off` default
        // and `Oscillator A pulse width route` explicitly at `Off`, so that depth reached nothing
        // at all: the LFO ticked and modulated no destination. There is nothing to translate.
        // `Filter envelope amount` 0.72 was the dedicated cutoff path, which is now the
        // `(Cutoff ← Filter envelope)` route at half the coefficient against twice the reach —
        // normalised 0.68 is a signed plain 0.36. See
        // `crates/mxm-mono-pr1-dsp/tests/legacy_reachability.rs`.
        ("Cutoff from Filter envelope", 0.68),
        ("Cutoff", 0.43),
        ("Resonance", 0.41),
        ("Amplifier envelope release", 0.32),
        ("Filter envelope release", 0.48),
        ("Bend range", 0.42),
    ] {
        set(session, name, value);
    }
    if altered_sync_pwm {
        set(session, "Oscillator A sync", 0.0);
        set(session, "Oscillator A pulse width", 0.83);
    }
    session.advance_blocks(4)?;
    session.app().note_on(48, 0.82);
    session.advance_blocks(14)?;
    session
        .app()
        .engine_mut()
        .push_gui_event(Payload::ControlChange {
            channel: 0,
            controller: 1,
            value: 93,
        });
    session.advance_blocks(8)?;
    session.app().note_on(55, 0.67);
    session.advance_blocks(12)?;
    session
        .app()
        .engine_mut()
        .push_gui_event(Payload::PitchBend {
            channel: 0,
            value: 0.82,
        });
    session.advance_blocks(10)?;
    session.app().note_off(55);
    session.advance_blocks(10)?;
    session
        .app()
        .engine_mut()
        .push_gui_event(Payload::PitchBend {
            channel: 0,
            value: 0.5,
        });
    session.advance_blocks(8)?;
    session.app().note_off(48);
    session.advance_blocks(34)
}

fn render(name: &str) -> Option<(Vec<f32>, PathBuf)> {
    let (dir, file) = bundle()?;
    let mut session = Session::scratch(name, vec![dir]);
    session.load(&file, PLUGIN);
    score(&mut session, false).expect("the deterministic score advances");
    let samples = session.captured();
    let (wav, _) = session
        .write_artifacts(name)
        .expect("write golden artifacts");
    Some((samples, wav))
}

#[test]
fn broad_fixed_score_has_not_moved() {
    let Some((samples, wav)) = render("golden-mono-pr1") else {
        eprintln!("skipping: run `cargo xtask bundle mxm-mono-pr1 --release`");
        return;
    };
    assert_eq!(samples.len(), GOLDEN_SAMPLES);
    assert!(samples.iter().any(|sample| sample.abs() > 1e-4));
    let actual = digest(&samples);
    assert_eq!(
        actual,
        GOLDEN_DIGEST,
        "mxm-mono-pr1 render moved; follow the regeneration procedure and listen to {} before pinning {actual}",
        wav.display()
    );
}

#[test]
fn oracle_is_sensitive_to_sync_and_pwm() {
    let Some((dir, file)) = bundle() else {
        eprintln!("skipping: run `cargo xtask bundle mxm-mono-pr1 --release`");
        return;
    };
    let mut session = Session::scratch("golden-mono-pr1-sensitive", vec![dir]);
    session.load(&file, PLUGIN);
    score(&mut session, true).expect("sensitivity score advances");
    assert_ne!(digest(&session.captured()), GOLDEN_DIGEST);
}

fn digest(samples: &[f32]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for sample in samples {
        for byte in sample.to_bits().to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}
