//! Listening harness for the mechanisms covered by the focused DSP proofs.
//!
//! This is not a fidelity assertion. It renders additive oscillators, inaudible-B hard sync,
//! routed modulation direct and through the wheel stage, Normal/Auto legato, Repeat with no LFO
//! waveform enabled, and Drone. The machine has no effects, so the path ends at its VCA.
//!
//! Run: `cargo run -p mxm-mono-pr1-dsp --release --example pro_one_demo`

#![allow(clippy::field_reassign_with_default)]

/// Writes a listening demo, applying this collection's demo headroom law **at the call site**.
///
/// `mxm_audio_file` encodes what it is given and applies no gain — normalisation is a judgement
/// about the material and the file crate carries no policy. The law here is the one the
/// six hand-written writers all applied internally: leave 2 % of headroom, and scale down further if
/// the material is over full scale.
fn write_demo(path: &str, interleaved: &[f32], channels: u16, rate: u32) {
    let peak = mxm_measure::level::peak(interleaved)
        .expect("a rendered demo is finite; a NaN here is a DSP defect, not a level");
    let gain = if peak > 1.0 { 0.98 / peak } else { 0.98 };
    let scaled: Vec<f32> = interleaved.iter().map(|s| s * gain).collect();
    mxm_audio_file::write(
        path,
        &scaled,
        channels,
        rate,
        mxm_audio_file::Target::Wav(mxm_audio_file::Bits::Sixteen),
    )
    .expect("the demo is written");
}

/// **Where this demo's channel count and sample rate are decided — once, for `main` and for the
/// test below.** Both call this, so a change to either constant changes both paths and the test's
/// literal expectations catch it. With the two supplied separately at each site, a `main` passing
/// the wrong channel count left the test perfectly green.
const DEMO_CHANNELS: u16 = 1;

fn write_demo_file(path: &str, interleaved: &[f32]) {
    write_demo(path, interleaved, DEMO_CHANNELS, FS as u32);
}

use mxm_mono_pr1_dsp::{
    control::{Activity, Event},
    keyboard::{KeyMode, NoteId},
    oscillator::{AWaves, BWaves},
    routing::{Routing, source, target},
    voice::{Params, Voice},
};

const FS: f32 = 48_000.0;

fn note(key: u8) -> Event {
    Event::NoteOn(NoteId::keyed(None, 0, key))
}

struct Take {
    voice: Voice,
    samples: Vec<f32>,
}

impl Take {
    fn new() -> Self {
        let mut voice = Voice::new();
        voice.set_sample_rate(FS);
        Self {
            voice,
            samples: Vec::new(),
        }
    }

    fn run(&mut self, params: Params, seconds: f32) {
        for _ in 0..(FS * seconds) as usize {
            self.samples
                .push(self.voice.process_sample(params, &[]).output);
        }
    }

    /// Every entry point that renders arms the voice first. It is cheap — once per call, not per
    /// sample — and it is what stops a take rendering unrouted.
    fn play(&mut self, params: Params, routing: &Routing, key: u8, seconds: f32) {
        self.voice.set_topology(routing);
        self.samples
            .push(self.voice.process_sample(params, &[note(key)]).output);
        self.run(params, seconds);
        self.voice.process_sample(params, &[Event::AllNotesOff]);
        self.run(params, 0.35);
    }
}

fn main() {
    let mut take = Take::new();

    // 1. Both fixed-gain waveform sums and a beating B.
    let mut additive = Params::init();
    additive.oscillator_a.waves = AWaves {
        saw: true,
        pulse: true,
    };
    additive.oscillator_b.waves = BWaves {
        saw: true,
        triangle: true,
        pulse: true,
    };
    additive.mixer.oscillator_b = 0.45;
    additive.oscillator_b.tune_semitones = 0.11;
    additive.filter.cutoff_hz = 3_500.0;
    additive.filter.resonance = 0.35;
    take.play(additive, &Routing::new(), 43, 2.0);

    // 2. B resets A while B is inaudible.
    let mut sync = Params::init();
    sync.oscillator_a.sync = true;
    sync.oscillator_a.waves = AWaves {
        saw: true,
        pulse: true,
    };
    sync.oscillator_a.pulse_width = 0.82;
    sync.oscillator_b.tune_semitones = 19.0;
    sync.oscillator_b.waves = BWaves::default();
    sync.mixer.oscillator_b = 0.0;
    sync.filter.cutoff_hz = 5_000.0;
    take.play(sync, &Routing::new(), 48, 2.0);

    // 3. The additive LFO on Direct, then Wheel at zero and at full.
    let mut modulation = Params::init();
    modulation.control.lfo_waves.saw = true;
    modulation.control.lfo_waves.triangle = true;
    modulation.control.lfo_rate_hz = 5.0;
    // The LFO straight at the pitch: one route.
    let mut direct = Routing::new();
    direct.present[target::OSCILLATOR_A_FREQUENCY][source::LFO] = true;
    direct.amounts[target::OSCILLATOR_A_FREQUENCY][source::LFO] = 0.2;
    take.play(modulation, &direct, 55, 1.5);
    // The same depth through the wheel: the LFO into the summing module, and the pitch reading the
    // wheel stage. The wheel is at rest, so this is silent until it moves.
    let mut through_wheel = Routing::new();
    // The multiplier's own factors, as Init wires them: `Mod bus` × `Wheel`.
    for (t, s) in mxm_mono_pr1_dsp::routing::INIT_AT_FULL {
        through_wheel.present[t][s] = true;
        through_wheel.amounts[t][s] = 1.0;
    }
    through_wheel.present[target::MOD_BUS][source::LFO] = true;
    through_wheel.amounts[target::MOD_BUS][source::LFO] = 0.2;
    through_wheel.present[target::OSCILLATOR_A_FREQUENCY][source::WHEEL_BUS] = true;
    through_wheel.amounts[target::OSCILLATOR_A_FREQUENCY][source::WHEEL_BUS] = 1.0;
    take.voice.set_topology(&through_wheel);
    take.samples
        .push(take.voice.process_sample(modulation, &[note(55)]).output);
    take.run(modulation, 0.7);
    take.voice.process_sample(
        modulation,
        &[Event::ModWheel {
            channel: 0,
            value: 1.0,
        }],
    );
    take.run(modulation, 1.2);
    take.voice.process_sample(modulation, &[Event::AllNotesOff]);
    take.run(modulation, 0.35);

    // 4. Normal low-note/single-trigger with overlap-only Auto glide, then Retrig last-note.
    let mut keys = Params::init();
    keys.control.glide_time_s = 0.8;
    keys.control.filter_envelope.attack_s = 0.08;
    keys.control.filter_envelope.sustain = 0.25;
    keys.control.amplifier_envelope = keys.control.filter_envelope;
    take.voice.process_sample(keys, &[note(60)]);
    take.run(keys, 0.5);
    take.voice.process_sample(keys, &[note(48)]);
    take.run(keys, 0.8);
    take.voice.process_sample(keys, &[Event::AllNotesOff]);
    take.run(keys, 0.4);
    keys.control.key_mode = KeyMode::Retrig;
    take.voice.process_sample(keys, &[note(48)]);
    take.run(keys, 0.4);
    take.voice.process_sample(keys, &[note(60)]);
    take.run(keys, 0.6);
    take.voice.process_sample(keys, &[Event::AllNotesOff]);
    take.run(keys, 0.4);

    // 5. Repeat still follows the LFO core with every LFO waveform disabled; Drone then holds gate.
    let mut repeat = Params::init();
    repeat.control.repeat = true;
    repeat.control.lfo_rate_hz = 4.0;
    repeat.filter.cutoff_hz = 1_800.0;
    take.run(repeat, 2.0);
    repeat.control.repeat = false;
    repeat.control.drone = true;
    repeat.control.amplifier_envelope.sustain = 0.65;
    take.run(repeat, 1.5);
    take.voice.process_sample(repeat, &[Event::AllSoundOff]);
    take.run(repeat, 0.4);

    let peak = take
        .samples
        .iter()
        .fold(0.0f32, |maximum, sample| maximum.max(sample.abs()));
    let rms = (take
        .samples
        .iter()
        .map(|sample| sample * sample)
        .sum::<f32>()
        / take.samples.len() as f32)
        .sqrt();
    write_demo_file("mxm-mono-pr1-demo.wav", &take.samples);
    println!(
        "wrote mxm-mono-pr1-demo.wav: {:.1} s, peak {peak:.3}, rms {rms:.3}; fidelity unverified",
        take.samples.len() as f32 / FS
    );
    assert!(take.voice.process_sample(Params::default(), &[]).activity == Activity::Inert);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo's own write path, exercised through the same wrapper `main` uses.
    ///
    /// The shared encoder is proved in `mxm-measure` against fixed header and payload bytes. What
    /// that cannot see is *this* file later writing the wrong channel count or rate, so the
    /// expectations here are **literals** — the facts about this instrument — rather than the
    /// constants under test.
    #[test]
    fn the_demo_write_path_produces_a_playable_file() {
        let frames = 256;
        let samples: Vec<f32> = (0..frames * DEMO_CHANNELS as usize)
            .map(|i| {
                let t = i as f32 / 48_000 as f32;
                // Past full scale, so the headroom branch is taken rather than skipped.
                1.6 * (std::f32::consts::TAU * 220.0 * t).sin()
            })
            .collect();

        let mut path = std::env::temp_dir();
        path.push(format!("pro-one-demo-demo-{}.wav", std::process::id()));
        write_demo_file(path.to_str().expect("a utf-8 path"), &samples);

        let read = mxm_audio_file_decode::decode_file(
            &path,
            &mxm_audio_file_decode::Limits::new(
                usize::MAX,
                mxm_audio_file_decode::AtLimit::Refuse,
                mxm_audio_file_decode::Keep::AllUpTo(2),
            ),
        )
        .expect("the demo file parses");
        assert_eq!(read.channels, 1, "the demo wrote the wrong channel count");
        assert_eq!(
            read.sample_rate, 48_000,
            "the demo wrote the wrong sample rate"
        );
        assert_eq!(read.frames(), frames, "the demo dropped or invented frames");

        // The headroom law, asserted rather than assumed: a source at 1.6 comes back just under
        // full scale, not clipped to it and not left loud.
        let peak = mxm_measure::level::peak(&read.interleaved).expect("a finite file");
        assert!(
            (0.97..=0.985).contains(&peak),
            "the 0.98 headroom law did not run: peak {peak}"
        );
        std::fs::remove_file(&path).ok();
    }

    /// **The whole production path, `main` included.** This is what a writer test cannot otherwise
    /// reach: the render itself, the buffer `main` chooses, and the channel count and rate it hands
    /// over. An empty or truncated render fails here and nowhere else.
    ///
    /// `#[ignore]`d because it renders the demo in full, which is tens of seconds of audio; run it
    /// with `cargo test --all-targets -- --ignored` when the demo or its write path changes.
    #[test]
    #[ignore = "renders the whole demo; run with --ignored"]
    fn the_whole_demo_renders_and_writes_a_playable_file() {
        main();
        let read = mxm_audio_file_decode::decode_file(
            "mxm-mono-pr1-demo.wav",
            &mxm_audio_file_decode::Limits::new(
                usize::MAX,
                mxm_audio_file_decode::AtLimit::Refuse,
                mxm_audio_file_decode::Keep::AllUpTo(2),
            ),
        )
        .expect("the demo file parses");
        assert_eq!(read.channels, 1, "the demo wrote the wrong channel count");
        assert_eq!(
            read.sample_rate, 48000,
            "the demo wrote the wrong sample rate"
        );
        assert!(
            read.frames() > 48000,
            "the demo rendered under a second of audio"
        );
        let peak = mxm_measure::level::peak(&read.interleaved).expect("a finite render");
        assert!(peak > 0.1, "the demo rendered near-silence: peak {peak}");

        // `main` writes into the working directory, which under `cargo test` is the crate root.
        // Leaving it there drops an untracked WAV into the tree every time this runs.
        std::fs::remove_file("mxm-mono-pr1-demo.wav").ok();
    }
}
