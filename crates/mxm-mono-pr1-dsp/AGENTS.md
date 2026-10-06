# AGENTS.md — crates/mxm-mono-pr1-dsp

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The framework-free DSP for `mxm-mono-pr1`, inspired by the documented 1981 Pro-One voice in
`research:instruments/pro-one.md`. The control and audio checkpoints exist: keyboard/performance,
glide and articulation; two additive CEM3340-style oscillators with B-to-A hard sync; noise; two
envelopes; the Pro-One CEM3320 path and a CA3280-style VCA. There is no effect stage and no external
input. Fidelity requires a manual hardware/listening comparison; none is claimed by the DSP tests.

**Modulation is the collection's shared any-to-any routing**
(mxm-kit's [`crates/mxm-modulation`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/mxm-modulation/AGENTS.md)), declared for this machine in `routing.rs`.
The hardware's Direct/Wheel buses are retired into it under `plans/plan-mxm-mono-pr1-modulation.md`
(in the private archive): the summing stage survives as a summing module, the wheel stage as a
**multiplier module**, and every destination switch as a route's presence. Fourteen sources by nine
targets. The rationale, history and measurements behind the rules below are in [NOTES.md](NOTES.md).

# Ownership

Owns `Cargo.toml`, `src/` (`keyboard.rs`, `envelope.rs`, `lfo.rs`, `signal.rs`, `routing.rs`,
`control.rs`, `oscillator.rs`, `filter.rs`, `voice.rs` and `lib.rs`) and `examples/`
(`pro_one_measure.rs`, `pro_one_demo.rs`; `common/wav.rs` retired to `mxm-measure`'s encoder, then
to `mxm-audio-file`). The plugin owns host translation, parameter ranges/smoothing, layouts,
presets, telemetry and the delivered dynamic editor.

# Local Contracts

## The control network has three signal classes and one order

`signal.rs` keeps continuous CV, held gates and one-sample pulses as distinct types. **A gate's level
is publishable as a source; a gate is still not a trigger**: no CV route can articulate an envelope.
One sample executes:

1. parameter-mode transitions, including deliberate panic re-arm edges;
2. host events in slice order, coalescing same-sample trigger pulses;
3. glide and the sounding pitch, then the **event-driven gestures** published — key (in octaves from
   middle C, from the *sounding pitch*, so glide and bend are in it), velocity, wheel, pressure, bend;
4. the caller's noise, published;
5. the **LFO-rate target summed**, then the LFO advanced and published, then Drone > Repeat >
   keyboard gate resolution and the gate published;
6. both envelopes, published;
7. `Mod bus` summed and published, then `Wheel bus` **multiplied** and published.

**Publication order is the declared source order**, which is the only thing that makes the shared
frame's unit delay mean anything, and `routing::FORWARD_THROUGH` is the table of which routes are
forward per target. Three targets are summed out of source order and that table is where it is
written down: the LFO rate before the LFO ticks, the module after the envelopes, and Oscillator B's
two targets before B renders — **the machine's deliberately-last B commit, as a row rather than as a
warning about a line's position**. The voice then renders B, publishes it, renders A against it,
publishes it, and sums cutoff and resonance last.

This order is the block-partition-independent policy, not incidental call order. `Voice::handle_events`
applies the same ordered event transition without advancing a sample, so callback-end and zero-frame
host events are retained and their trigger/cut pulses wait for the next rendered sample.

- **A sample that does not run publishes nothing and never opens the frame**, and noise advances only
  on a sample that runs, so an inert gap cannot move the next phrase.
- **Topology is armed, never inferred.** A controller is armed with the init topology as it is
  constructed (so `new` is not `const`); **every other path that can reach the render loop owes a
  `Controller::set_topology` call**. The grid lives in the controller, not in the `Copy` `Params`.

Why: [NOTES.md § The control network](NOTES.md#the-control-network-why-it-is-ordered-and-armed-as-it-is).

## Keyboard, glide and performance

A bounded 16-press ledger stores presses, not pitches. Host voice id is authoritative when both
sides supply one; otherwise channel/key matches, and an id-less same-pitch release retires the oldest
press. Exhaustion evicts the oldest non-sounding press and never the sounding press.

- **Normal:** lowest held press, older first among equal keys; only opening an empty held set
  triggers. A lower overlapping press changes pitch without retriggering.
- **Retrig:** most recently pressed held press; every new press triggers. Releases fall back without
  a trigger in both modes.
- Key-mode changes can move the bus but never trigger. Normal glide slews every selected-key change;
  Auto slews only while the held gate remains open and detached notes jump.
- Per-note tuning belongs to the addressed press. Normalized channel bend and wheel follow the
  sounding press's channel; the plugin supplies a signal-smoothed current bend range every control
  frame, so a range edit slews a held bend over the pitch-signal interval and ownership fallback
  restores each channel's retained position. The last owner and its performance state outlive release tails; reset restores
  the defined middle-C/channel-0 owner.

## Repeat, Drone and panic

- **The LFO core square is Repeat's only clock**, read independently of the enabled LFO waveform
  paths; no external detector or Gate/clock jack. Drone forces the shared gate high and suppresses
  Repeat. Keys select pitch but do not retrigger in either autonomous mode.
- All Notes Off releases presses and can leave a tail; a final targeted choke cuts both envelopes.
- **All Sound Off latches silence** (presses, envelopes, delayed modulation state cleared); Drone and
  Repeat cannot wake it. Only reset, note-on, or leaving and re-entering Drone/Repeat clears it.
  Coincident events obey slice order. Detail: [NOTES.md § Repeat, Drone and panic](NOTES.md#repeat-drone-and-panic).

## The frame unit, the product law and the scale columns

Why: [NOTES.md § The frame unit](NOTES.md#the-frame-unit-is-⅛-and-it-is-sized-by-the-widest-composed-value),
[§ A `product` target](NOTES.md#a-product-target-un-scales-into-its-law), [§ Scale column](NOTES.md#what-the-machines-own-scale-column-buys).

- **The LFO triangle and Oscillator B's saw are enabled by default**; `WaveEnables::NONE` and
  `BWaves::NONE` are the all-off test sets, never the `Default`. LFO and B wave sums are not normalised.
- **Every source is published as its raw value times `routing::FRAME_UNIT` (⅛)**, and a target's
  scale carries the inverse. The unit is sized by `Mod bus`'s composed sum, not the widest raw
  source, and stays a power of two so the arithmetic is exact. Past ±1 the frame clamps, by design.
- **A product scales out of the frame before its law**:
  `factor = 1 + amount × (frame.read(source) × FRAME_SCALE − 1)`, through the shared
  `mxm_modulation::product_with_tops` with `FRAME_SCALE` and `PRODUCT_TOPS`.
  `routing::PRODUCT_TARGET` names its one target; `Graph::sum` and `Graph::product` each
  `debug_assert` the target is theirs.
- **The wheel is not a source on `Mod bus`**: `WHEEL_IS_NOT_A_BUS_SOURCE`, refused by
  `Graph::set_topology` whatever it is handed. The wheel is an additive source at the seven knob
  targets and a factor on the multiplier.
- **Init deviations** (recorded in `plugins/mxm-mono-pr1/AGENTS.md`): `routing::INIT_AT_FULL` wires
  `Mod bus` and `Wheel` into the multiplier at full amount; `routing::INIT_WHEEL` wires the LFO into
  `Mod bus` and `Wheel bus` into both oscillators' frequency (`the_wheel_brings_in_vibrato_at_init`).
  `init_amount`/`init_present` state every Init route once.
- **Seven targets have a uniform scale column** and apply it outside the sum, which keeps the four
  oscillator destinations bit-identical to the legacy graph (`tests/legacy_reachability.rs`). Never
  apply a scale per route there. **Cutoff's column is not uniform**: `(Cutoff ← Filter envelope)`
  carries twice the destination scale; Key carries one octave of cutoff per octave.

## Named exclusions and the modulation standard

- **Amplitude is a target after the VCA**: `standard::amplitude_factor` on the VCA's output, a
  factor never added (`amplitude_is_the_standard_factor_after_the_vca`). Every pair absent at Init
  ([NOTES.md § Named exclusions](NOTES.md#named-exclusions-which-are-decisions-rather-than-omissions)).
- **A routable gate is not a target**: a fourth gate contender is the one way to lose exact `Inert`.
- No arpeggiator/sequencer, random control source or sample-and-hold. `Rng` drives audio noise only.
- Key, **Velocity as `v − 1` of the press that last triggered the envelopes** (`envelope_velocity`),
  wheel, pressure and bend publish through `mxm_modulation::standard`. A target whose machine routes
  shared one bus scale keeps it (`mxm_modulation::sum_split`); every added pair takes the standard
  reach (`routing::ADDED_SCALE`). `conformance.rs` runs the standard's checks, each falsified once
  ([NOTES.md § The modulation standard](NOTES.md#the-modulation-standard-on-a-bus-machine)).

## Oscillators, mixer, filter and VCA

- A and B are free-running cores whose wave switches sum fixed unequal gains, unnormalised. An
  ordered edge timeline drives both PolyBLEP lobes for B→A sync; B's wrap resets A at the sub-sample
  remainder whether or not B is enabled or mixed in. Mixer level affects audio only.
- The mixer's third source is deterministic white **noise**; no external-audio input.
- `filter.rs` is original TPT/ZDF gm-cell code, not a hardware fit. Volume scales envelope control
  into the VCA (not a post-output multiplier); the VCA is exactly closed at zero control. **There are
  no built-in effects.** Detail: [NOTES.md § Oscillators](NOTES.md#oscillators-and-mixer),
  [§ Filter](NOTES.md#filter-vca-and-no-effects).

## Activity, reset and numeric contract

- Live for held, Repeat or Drone state; Tailing while the amplifier envelope releases; Inert after
  the final VCA control settles, or immediately under panic. Filter-envelope routes are upstream of
  the VCA and never keep the core awake. At Inert, phases and noise freeze and output is exact zero
  ([NOTES.md § Activity](NOTES.md#activity-reset-numeric-and-chosen-values)).
- Reset clears every control, pulse, oscillator, random, filter, coupling and delayed-route state.
- Per-sample paths allocate and lock nothing. `[dependencies]` is exactly `mxm-modulation`; MSRV
  **1.88** (`let` chains in `keyboard.rs`). `mxm-measure`, `mxm-audio-file` and
  `mxm-audio-file-decode` are dev-only, outside the shipped graph.

## Chosen values

Chosen pending hardware measurement: the **⅛ frame unit** (a power of two, sized by the composed
bus, exact rather than fitted), the resonance target's full-sweep reach and the LFO-rate target's
four octaves (the standard's, every route there being an added one; six until the modulation
standard) — the reissue's RESONANCE and LFO CV jacks have no documented volts-per-unit to derive from;
unity bipolar LFO paths; current-level exponential CEM3310
curves; 30 s envelope/glide guards; envelope snap at −100 dB; linear glide at 36 semitones per
supplied time; filter cell drive/asymmetry, Q-return scale and 3 Hz coupling; destination scales;
B triangle level/low range; and VCA drive. Documented values include LFO 0.1–30 Hz, envelope 2 ms floor, additive wave switches, 150 pF
filter capacitors, ×3.4 buffer arithmetic and the four-cell loop gain. Self-consistency tests do not
establish hardware fidelity. Model measurements: [NOTES.md § Model measurements](NOTES.md#model-measurements).

# Work Guidance

- Keep this crate framework-free. **One runtime dependency**, `mxm-modulation`, which is itself
  dependency-free and below this floor; a test-only `[dev-dependencies]` edge is not one, and
  `cargo tree` is the check. Preserve the publication order above — it *is* the unit delay.
- Preserve machine constraints deliberately and label every unmeasured calibration choice.
- The arpeggiator/sequencer and random modulation remain out unless the product boundary is changed
  by the owner.
- **Do not depend on or extract a sibling crate**: this is a per-machine DSP copy
  ([NOTES.md § Extraction comparison](NOTES.md#extraction-comparison)).

# Verification

```bash
cargo test -p mxm-mono-pr1-dsp
cargo clippy -p mxm-mono-pr1-dsp --all-targets
cargo fmt --package mxm-mono-pr1-dsp -- --check
cargo run -p mxm-mono-pr1-dsp --release --example pro_one_measure
cargo run -p mxm-mono-pr1-dsp --release --example pro_one_demo # writes mxm-mono-pr1-demo.wav
```

`cargo tree -p mxm-mono-pr1-dsp -e normal` must show exactly two lines (this crate and
`mxm-modulation`). Measurements come from `mxm-measure` (dev-only); every bound and its headroom
stays in the test that argues for it. `tests/legacy_reachability.rs` is the routing conversion's
gate. What the tests cover: [NOTES.md § Measurement](NOTES.md#measurement-and-test-coverage).

# Child DOX Index

No child AGENTS.md files. `src/` is covered here.
