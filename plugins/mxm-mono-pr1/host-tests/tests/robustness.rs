//! Hostile activation and callback shapes for mxm-mono-pr1 through the direct CLAP host.
//!
//! Run against a debug bundle as well as release. The workspace enables nice-plug's debug process
//! allocation assertion, so an audio-thread allocation aborts the callback instead of hiding.

use mxm_player_harness::harness;

use mxm_player::events::input::Payload;

const PLUGIN: &str = "dk.mxm.mxm-mono-pr1";

fn note_on(key: u8) -> Payload {
    Payload::NoteOn {
        channel: 0,
        key,
        velocity: 0.8,
    }
}

fn note_off(key: u8) -> Payload {
    Payload::NoteOff {
        channel: 0,
        key,
        velocity: 0.0,
    }
}

#[test]
fn hostile_sample_rates_and_every_internal_block_boundary_stay_finite() {
    let Some(bundle) = harness::mxm_mono_pr1() else {
        return;
    };
    for rate in [1_000.0, 1_234.57, 44_100.0, 192_000.0, 768_000.0] {
        let mut host =
            harness::Harness::with_configuration(&bundle, PLUGIN, 1, rate, 8192).expect("hosts");
        assert!(host.push(0, note_on(127)));
        for frames in [1usize, 2, 63, 64, 65, 127, 255, 1024, 4097, 8192] {
            let audio = host.render(frames);
            assert!(
                audio
                    .iter()
                    .all(|sample| sample.is_finite() && sample.abs() <= 1.0),
                "invalid output at {rate} Hz / {frames} frames"
            );
        }
        assert!(host.push(0, note_off(127)));
        for _ in 0..32 {
            host.render(8192);
        }
        assert!(host.render(64).iter().all(|sample| sample.is_finite()));
        host.shutdown();
    }
}

#[test]
fn multiple_events_inside_one_large_callback_are_split_at_their_offsets() {
    let Some(bundle) = harness::mxm_mono_pr1() else {
        return;
    };
    let mut host =
        harness::Harness::with_configuration(&bundle, PLUGIN, 1, 48_000.0, 4096).expect("hosts");
    host.render(256);
    host.render(256);
    let base = host.clock.now_nanos();
    assert!(host.push_at(0, base, note_on(60)));
    assert!(host.push_at(0, base + 2_000_000, note_off(60)));
    assert!(host.push_at(0, base + 4_000_000, note_on(67)));
    assert!(host.push_at(0, base + 7_000_000, note_off(67)));
    let audio = host.render(4096);
    assert!(audio.iter().all(|sample| sample.is_finite()));
    assert!(
        audio.iter().any(|sample| sample.abs() > 1e-5),
        "the intervals between events vanished"
    );
    for _ in 0..500 {
        host.render(512);
    }
    assert_eq!(
        host.peak(),
        0.0,
        "the final split NoteOff must reach the voice"
    );
    host.shutdown();
}

#[test]
fn dense_same_callback_events_remain_bounded_and_panic_wins_last() {
    let Some(bundle) = harness::mxm_mono_pr1() else {
        return;
    };
    let mut host =
        harness::Harness::with_configuration(&bundle, PLUGIN, 1, 48_000.0, 4096).expect("hosts");
    let base = host.clock.now_nanos();
    for index in 0..96 {
        let key = 36 + (index % 48) as u8;
        assert!(host.push_at(0, base, note_on(key)));
        assert!(host.push_at(0, base, note_off(key)));
    }
    assert!(host.push_at(
        0,
        base,
        Payload::ControlChange {
            channel: 0,
            controller: 120,
            value: 0,
        },
    ));
    let audio = host.render(4096);
    assert!(audio.iter().all(|sample| sample.is_finite()));
    for _ in 0..8 {
        assert_eq!(
            host.render(512).iter().fold(0.0f32, |p, x| p.max(x.abs())),
            0.0
        );
    }
    host.shutdown();
}
