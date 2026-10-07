# mxm-mono-pr1

A monophonic two-oscillator CLAP instrument with additive waveform switches, hard sync, separate
filter and amplifier envelopes, and the collection's any-to-any modulation routing.

## What it is

- Two independent, free-running oscillators. A supplies saw and pulse; B supplies saw, triangle and
  pulse, plus low-frequency and keyboard-follow modes. Enabled waves add rather than select.
- B-to-A hard sync remains effective when B is absent from the audio mix.
- **Any-to-any modulation routing**: fourteen sources — the LFO, both envelopes, the gate, the key,
  velocity, the wheel, pressure, bend, the noise, both oscillators, and the two
  modules — into nine targets. A route is a presence and a signed amount, drawn under the control it
  moves, and **a route amount reads in its target's own domain**: semitones at a pitch, octaves at
  the cutoff and the LFO's rate, and a percentage of the target's own range elsewhere — a full-depth
  pulse-width route reads `+50 %` because that is how far it moves the width.
- The machine's own two-stage routing survives as two **modules**, each a target in its own right.
  **Mod bus** adds whatever is put in it. **Wheel bus** *multiplies* what is put in it, which is what
  lets one control set how much of another gets through — put the LFO in Mod bus, route Wheel bus at
  an oscillator's frequency, and the wheel fades the vibrato in and out as you play. It ships wired
  to `Mod bus × Wheel`, the machine's own two stages, and either factor can be re-pointed at any of
  the fourteen sources. Every sound the hardware's Direct/Wheel buses could make is still reachable;
  a fresh instance is still the copy.
- A four-pole resonant low-pass path, separate ADSR envelopes, Normal/Retrig key modes, Normal/Auto
  glide, Repeat articulation clocked by the LFO core, and Drone.
- Stereo and mono output, and no input: the hardware's external audio and gate/clock jacks are not
  reproduced. Noise is the third mixer source, and the LFO — synced to the host's tempo if you
  like — is Repeat's clock.
- No effects, arpeggiator or note sequencer. Effects were not part of the approved voice, and the
  player or DAW already owns note sequencing.

## Presets and control map

Init is generated from all 318 host defaults — 40 voice controls and 139 routing pairs, a presence
and an amount each. Every amount starts at zero except the multiplier's own two factors, which start
at full so that `Mod bus × Wheel` is what a fresh instance does. Fifty complete, categorised factory
sounds exercise the additive waves, hidden sync source, pulse dropouts, the two modules,
oscillator-B modulation,
articulation modes, noise and a tempo-synced Repeat. Factory sounds are compiled into the plugin;
user presets use the collection's readable JSON format.

`control-map.json` claims only standard roles with equivalent meanings. Independent wave switches
remain unclaimed rather than being misrepresented as a single selector. The five routing roles it
does claim each name the route the init patch wires, which is why those pairs are present at zero
depth: a controller knob bound to an absent route would do nothing at all.

## Status

The DSP, permanent identity and parameters, realtime host shell, telemetry, factory content,
controller map and dynamically paged visual editor are implemented. Automated proof covers every
musician control and semantic view across themes and zoom scales — this editor has no disclosure,
so every control is on a card except Volume, which is in the app bar — plus every card's floor,
computed from its layout tree and checked with every route revealed, plus
focused MXM Player discovery, state, rendering, hostile-rate/block, allocation, lifecycle and native
resize paths. Native visual/recognisability review, real-DAW operation, final reference listening and
hand checks on Linux/macOS remain manual release gates; CI builds and tests all three platforms
when started by hand. Fidelity is **UNVERIFIED**: no hardware
comparison or recognisability trial has been run.

## Building

```bash
cargo xtask bundle mxm-mono-pr1 --release
```

The loadable bundle is written to `target/bundled/mxm-mono-pr1.clap`; bundling stages the controller
map beside it. GPL-3.0-or-later — see the repository's [`LICENSE`](../../LICENSE). All
implementation code is original.
