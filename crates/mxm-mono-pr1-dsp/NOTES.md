# NOTES.md — crates/mxm-mono-pr1-dsp

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked examples.
AGENTS.md is the contract; this file is the reference it links to.

## The control network: why it is ordered and armed as it is

`signal.rs` keeps continuous CV, held gates and one-sample pulses as distinct types. **A gate's
level is publishable as a source; a gate is still not a trigger** — the type distinction survives at
application, so no CV route can articulate an envelope.

**A sample that does not run publishes nothing and never opens the frame.** That is what stops
repeated host calls on an inert plugin altering the next phrase, and it replaces the old
`process_frozen` read-without-commit mode: the shared frame needs no such API, only the discipline of
not writing. The noise generator likewise advances only on a sample that runs, so the length of an
inert gap cannot move the next phrase's noise.

**Topology is armed, never inferred.** `Controller::set_topology` adopts a routing grid and rebuilds
the compacted lists; `routing_mut` is how a caller advances the live amounts in place each sample.
A controller is armed with the init topology as it is constructed — which is why `new` is not `const`
— so no path can reach the render loop with empty compacted lists. **Every other path that can reach
it owes a `set_topology` call**; the collection has shipped a voice that was never armed once
already, and the first note after any allocation rendered with nothing routed.

The grid lives in the controller rather than in `control::Params` because `Params` is `Copy` and a
plugin rebuilds it every sample: 125 pairs inside it would be copied per sample for values that
change only on a parameter event.

## Repeat, Drone and panic

Repeat timing reads the LFO core square independently of enabled LFO waveform paths, and **the LFO is
its only clock**: the hardware's external-audio detector and Gate/clock jack are not reproduced (the
owner, 2026-09-26 — the LFO has tempo sync, and the sequencer an external clock would have driven is
outside this instrument). Drone forces the shared gate high and suppresses Repeat. Keys select pitch
but do not retrigger in either autonomous mode.

All Notes Off releases presses and can leave a tail. A final targeted choke cuts both envelopes.
All Sound Off clears presses, envelopes and delayed modulation state and latches silence; Drone and
Repeat cannot wake it. Only reset, note-on, or leaving and re-entering Drone/Repeat deliberately
clears it. Coincident events obey slice order, so note-on after panic re-arms and panic after
note-on remains latched.

## LFO, the frame unit, and deliberate exclusions

Rising saw, triangle and square are independently enabled fixed-gain paths whose sum is not
normalised, and **that is why this instrument needs a frame unit at all**.

**The triangle is enabled by default and the other two are not**, and **Oscillator B's saw
likewise**. A waveform switch is configuration, so the init contract asks for a useful one: with
every switch off the LFO sum is zero and B's sum is zero, so raising a routed depth or raising B's
mixer level produces *silence*. `WaveEnables::NONE` and `BWaves::NONE` are the all-off sets a caller
builds single-wave cases from, and neither is the `Default`.

## The frame unit is ⅛, and it is sized by the widest *composed* value

`SourceFrame::write` bounds every published value to unit magnitude and this machine's sources are
not unit-bounded: the LFO's enabled sum reaches ±3 (`lfo::SUM_BOUND`), Oscillator B's 1.9 and
Oscillator A's 1.6. Every source is therefore published as its raw value times `routing::FRAME_UNIT`,
and a target's scale carries the inverse.

**The unit is sized by `Mod bus`, not by the widest raw source**: a frame slot holds everything built
*from* the sources as well as the sources, and the module sums up to **5.9** raw units on the wiring
the hardware itself permits. At ¼ that clamps on publication and takes the whole legacy Wheel path
with it. At ⅛ it is 0.7375. **⅛ is a power of two**, which is what keeps the arithmetic exact —
`Σ(sᵢ ÷ 8)` is `(Σ sᵢ) ÷ 8`, and one multiply against the compile-time constant `8 × k` undoes it.

**5.9 bounds the legacy machine, not arbitrary routing.** `Mod bus` now takes fourteen signed routes,
so a player can sum past ±1 and the frame will clamp — bounded publication working, which is what
keeps a cycle finite, rather than a defect.

## A `product` target un-scales into its law

`mxm_modulation::sum` is scale-invariant; `product` is not. Its factor is `1 + amount·(source − 1)`,
and in a ⅛ frame that literal `1` is **eight raw units** — so an `n`-factor product is off by
`8ⁿ⁻¹`, a partial amount interpolates toward the wrong neutral, and removal means nothing.

An earlier revision of this instrument read that as *a product target needs an unscaled frame* and
made the wheel stage fixed wiring, which quietly dropped `plan-modulation-routing.md` decision 1.14
— the owner's multiplier module — and the owner noticed. **The repair is the rule the frame already
has, applied on the way in.** A module scales *into* the frame's domain before publishing; a product
scales *out of* it before applying its law:

```text
factor = 1 + amount × (frame.read(source) × FRAME_SCALE − 1)
```

The `1` is then one raw unit, which is what the law means, and every property comes back: nothing
present is neutral, a zero amount is neutral rather than annihilating, and the machine's own
`Mod bus × wheel` falls out of two routes at full amount. **The law is the shared
`mxm_modulation::product_with_tops`** since the modulation standard, when `mxm-para-07` became its
second scaled-frame consumer: `Graph::product` calls it with this crate's `FRAME_SCALE` and
`PRODUCT_TOPS` — each factor neutral at its source's top, zero for the standard Velocity, so a
velocity factor is the number it was. `routing::PRODUCT_TARGET` names the one target it applies to,
and `Graph::sum` and `Graph::product` each `debug_assert` the target is theirs, because the
multiplier's `UNIFORM_SCALE` entry is `None` for a different reason than cutoff's and the two must
not cross.

**And the wheel is not a source on `Mod bus`.** The wheel's relationship with that bus is already
multiplication, so an additive route from it into the same stage is the same control wired two ways
into one place; all it can produce is a transposing DC offset riding on whatever else is in the bus,
where the player wanted it to set that thing's depth. `WHEEL_IS_NOT_A_BUS_SOURCE` names the pair and
`Graph::set_topology` refuses it whatever it is handed, so a hand-built `Routing` cannot reintroduce
it. The raw wheel stays an additive source at all seven knob targets, and it is a **factor** on the
multiplier, which is where it ships.

**The multiplier's factors and the wheel's vibrato are this instrument's init deviations**, recorded
in `plugins/mxm-mono-pr1/AGENTS.md`: `routing::INIT_WHEEL` (2026-09-28) wires the LFO into `Mod bus`
at full and `Wheel bus` into both oscillators' frequency at `INIT_VIBRATO_SEMITONES`, bit-identical
with the wheel at rest (`the_wheel_brings_in_vibrato_at_init`); `init_amount`/`init_present` state
every Init route once. `routing::INIT_AT_FULL` wires `Mod bus` and `Wheel` into the multiplier at
**full amount** rather than zero, because `product`'s neutral is one — a factor present at zero
depth would make the module publish a constant whatever its sources did, and the route's presence
would be a lie. Its result is published through `FRAME_UNIT` like any other source, so the frame's
bound is what keeps a cycle through the multiplier finite.

## What the machine's own scale column buys

Seven targets have a **uniform** scale column and take `sum` with the destination scale applied
outside, which is literally the instruction sequence the legacy graph executed on its already-summed
bus. **That is what keeps the four oscillator destinations bit-identical**, and
`tests/legacy_reachability.rs` is where it is proved rather than asserted — over 20 000 randomised
legacy patches, `to_bits()` equal. Applying a scale per route instead distributes a multiply over a
sum and does not round alike; the test is falsifiable and was falsified before it was trusted
(`-6.270301` against `-6.2703`).

**Filter cutoff is the one target whose column is not uniform**, because the conversion collapsed two
legacy paths into one route: the dedicated `filter_env_amount` and the bus path both reached the
cutoff and *added*, so `(Cutoff ← Filter envelope)` carries **twice** the destination scale and a
legacy patch converts as `(bus amount + dedicated amount) ÷ 2`. Twice `FILTER_MOD_OCTAVES` is also
exactly the voice's own ±16-octave cutoff clamp, so the number is arithmetic rather than taste. The
key carries one octave of cutoff per octave of keyboard, which is what the retired
`filter_keyboard_amount` meant. Cutoff's digest therefore moves and the oscillators' must not.

## Named exclusions, which are decisions rather than omissions

**Amplitude is a target, after the VCA, and that keeps `Inert` free.** It was a named exclusion
until the owner's ruling of 2026-09-26 that every instrument has the collection's standard
Amplitude (`plans/plan-modulation-standard.md`). It is `standard::amplitude_factor` on the VCA's
*output* — a factor, never added — so it cannot open a closed VCA or outlive the envelope that ends
the voice; the VCA reaches at most a quarter of full scale, so double stays inside `OUTPUT_BOUND`
(`amplitude_is_the_standard_factor_after_the_vca`). Every pair is absent at Init.

**A routable gate is not a target.** A fourth contender in the Drone > Repeat > keyboard precedence
— a free-running LFO able to retrigger forever — is the one way this conversion could lose exact
`Inert`.

The hardware note arpeggiator/sequencer is deliberately absent under the player/DAW boundary. The
machine has no random control source or sample-and-hold; `Rng` drives deterministic audio noise, which
is routable **because it is audio this machine already makes** rather than because a generator was
added. Shared source-frame timing and the combination laws belong to
[`crates/mxm-modulation`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/mxm-modulation/AGENTS.md);
this instrument retains the meaning and application of every value.

## The modulation standard, on a bus machine

Key (in octaves, from the sounding pitch — this machine keeps tuning and bend in it, as the retired
keyboard amount tracked them), **Velocity as `v − 1` of the press that last triggered the
envelopes** (`envelope_velocity`, latched exactly where step 5 triggers them under keyboard
articulation: a Normal-mode legato press or a fallback keeps the phrase's; Repeat and Drone trigger
from their clock and keep it too; a fresh instance, a reset and All Sound Off rest at full), the
wheel, pressure and the bend lever are published through `mxm_modulation::standard`. **A target
whose machine routes shared one bus scale keeps that scale for them** — `mxm_modulation::sum_split`,
the bus's instruction sequence to the bit while no added pair is live, which is why every digest held
— and **every added pair takes the standard reach** (`routing::ADDED_SCALE`): 12 st where the bus
gave 24, 45 % of width where it gave half, 12 st/oct from Key; cutoff's added pairs 4 octaves where
they took 8; the LFO rate 4 octaves where every route took 6. The multiplier is
`mxm_modulation::product_with_tops`, each factor neutral at its source's top — zero for the standard
Velocity, so a velocity factor is the number it was. `conformance.rs` runs the standard's checks,
each falsified once; the owner's refusal of the wheel into its own bus is the declaration's
`ruled_out`.

## Oscillators and mixer

A and B are separate free-running cores. A's saw/pulse and B's saw/centred-triangle/pulse switches
sum fixed unequal gains without normalisation. The documented summing-resistor ratios derive A
pulse at 0.6 of saw and B pulse at 0.5; B triangle at 0.8 is chosen. Saw and pulse retain unipolar DC
while triangle is centred. A fixed-capacity ordered edge timeline drives both PolyBLEP lobes:
natural edges before B's edge survive, coincident/later free-running edges are preempted, the reset
uses the value immediately before B's edge and phase zero immediately after it, and pulse-width
crossings created by the post-reset trajectory are superposed. Exact 0/100% pulse endpoints create
no edge. Pulse width reaches exact DC at 0/100%, and full PWM can cross either endpoint.

B's falling saw/core wrap resets A at the sub-sample remainder whether B has a waveform enabled or
a mixer level. B's one enabled-wave sum feeds both audio and modulation; mixer level affects audio
only. Low-frequency mode's seven-octave extension, frequency destination's 24 semitones per unit and
PWM's half-range per unit are chosen.

The mixer's third source is **noise**, deterministic white noise; the documented
production-level/spectrum change is not guessed. The hardware's switched external-audio input — its
preamp, and the detector it fed — is not reproduced (the owner, 2026-09-26).

## Filter, VCA and no effects

`filter.rs` is original code from published TPT/ZDF technique and the researched gm-cell circuit:
four implicit integrators, saturation on both sides of every cell, fixed five-step Newton solve and
an arrowhead Jacobian. Cell scaling preserves unit small-signal gain. The chosen asymmetric curve
can produce the datasheet's stated second-harmonic tendency but is not a hardware fit.

The four 150 pF stages are represented as equal poles with caller-supplied Hz because no measured CV
offset exists. The output buffer is the schematic-derived **×3.4** (`1 + 240k/100k`, conflicting
manual prose says 2.4), and resonance returns from that buffered node. Chosen Q return 2.2 puts the
running model's onset between control 5 and 6, matching the service check; the datasheet loop-gain
condition is 4. The documented 2.2 µF coupling is represented by a chosen 3 Hz output corner outside
the implicit audio-band loop approximation.

Volume scales amplifier-envelope control into the VCA and changes its chosen operating drive; it is
not a post-output multiplier. The VCA is exactly closed at zero control. The path ends there:
**there are no built-in effects**, because the original had none.

## Activity, reset, numeric and chosen values

Activity is Live for held, Repeat or Drone state; Tailing while the amplifier envelope releases;
and Inert after that final VCA control settles or immediately under panic. Every filter-envelope
destination is upstream of the VCA, so routed and disconnected filter envelopes are both silenced
when the amplifier settles: neither can keep the core awake or reappear as a ghost after rerouting.
At Inert LFO and oscillator phases freeze, glide settles, filter tails clear, noise state freezes,
and output is exact zero. Reset clears every control, pulse, oscillator, random, filter, coupling and
delayed-route state. Per-sample paths allocate and lock nothing. `[dependencies]` carries exactly
one entry, `mxm-modulation`, which is dependency-free at 1.87; MSRV is **1.88** because
`keyboard.rs` uses `let` chains, which stabilised there, so 1.87 was never true of this crate.
`[dev-dependencies]` carries `mxm-measure`, `mxm-audio-file` and `mxm-audio-file-decode`, test-only
edges outside the shipped graph.

## Model measurements

`pro_one_measure` measured this model at 48 kHz: the chosen Q return derives onset at **0.535**;
with cutoff 1 kHz and resonance zero, gain is **+10.46 / −1.41 / −63.51 dB** at 100 Hz / 1 kHz /
8 kHz; the all-enabled A/B sums in its 220 Hz score peak at **0.995 / 0.998**; and the chosen VCA at
input 2/envelope 1 outputs **0.200 / 0.250** at Volume 0.5 / 1. These are reproducible model
measurements, not claims about hardware.

## Extraction comparison

This is the seventh honest per-machine DSP copy. Do not depend on or extract a sibling crate.

| Candidate | Standing |
|---|---|
| `flush`, xorshift `Rng` | Same established primitive form, copied locally |
| ADSR | Stall-safe exponential shape follows the sibling pattern; machine floor, snap and chosen ceiling differ |
| Press ledger | Same press/id/exhaustion policy; Pro-One Normal/Retrig selection and triggers are machine-specific |
| LFO | New additive three-enable graph with clock independent of the sum |
| Routing | **Shared** — `crates/mxm-modulation`, this crate's one runtime dependency. What stays local is the declaration: the source list, the target list, the frame unit and the scale columns |
| PolyBLEP phasor/pulse | Established local primitive form; Pro-One unipolar levels, additive sums and sub-sample B→A reset are machine-specific |
| CEM3320 filter | New per-machine gm-cell implementation; unlike sibling diode ladders, saturation is on both sides of every integrator and feedback includes this machine's buffered node |
| VCA | New machine-specific operating-drive form |

## Measurement and test coverage

**The rulers are shared, the thresholds are not.** `mxm-measure` is a `[dev-dependencies]` entry —
zero dependencies and a 1.87 floor of its own, below this crate's, and **not in the shipped graph**.
The one runtime dependency is `mxm-modulation`, which is also dependency-free and below this floor,
so the crate stays genuinely portable; `cargo tree -p mxm-mono-pr1-dsp -e normal` is the check and
must show exactly those two lines. Measurements come from there; every bound and
its headroom stays in the test that argues for it.

The crate has **70 focused tests in the library plus a seven-test reachability suite** in `tests/`.
`tests/legacy_reachability.rs` is the conversion's own gate: it transcribes the retired Direct/Wheel
graph as its reference, sweeps 20 000 randomised legacy patches through §3.3's canonical
translation, and asserts the four oscillator destinations **bit-identical** while cutoff is equal to
a stated tolerance; it sweeps the two collapsed cutoff amounts across their whole ranges rather than
at one maximum; and it covers all 125 pairs at extreme amounts — each asked for **its own law**, so
the multiplier is exercised as a product rather than through cutoff's column — a non-finite source,
a cycle through the module and a reset that must clear the delayed half of the frame. In the lib:
the frame unit contains every bus the hardware could wire (so a later gain change goes red rather
than silently invalidating the conversion), velocity follows the sounding press through selection
and fallback, channel pressure is retained and inherited, a parked gesture routed at full depth
still reaches exact `Inert`, an inert gap of any length cannot alter the next phrase, the whole
125-pair grid renders finite and deterministic at four sample rates, the multiplier keeps its law in
a scaled frame (neutral when empty, neutral at zero amount, the plain product of raw values at full
amount, and bounded through a cycle), an audio source into a pitch target is audible and bounded,
and a fully routed patch still releases to exact zero. In addition to the control checkpoint's
original 29, audio proofs cover fixed unequal waveform sums, pulse endpoints, ordered sync edge
preemption/reset-created crossings on both BLEP sides, separate synced saw and pulse/PWM
additive-reference comparisons across A/B ratios and offset sub-sample resets, broad unsynced
saw/PWM upper-band reduction, sub-sample hard sync and wide-pulse dropout; B sync while inaudible and
mixer/modulation independence; noise as the third mixer source; gm-cell small-signal
invariance/asymmetry, four-pole response, running resonance, post-buffer onset and reset; VCA
operating drive; integrated bus reach, exact release/inert silence, post-phrase idle-gap invariance,
recursive denormal exclusion, deterministic renders, and finite bounded sweeps/frequency checks from
1 kHz to 768 kHz. The 18.3 s demo rendered at peak 0.235/RMS 0.135; it has not been listened to.
Linux and macOS are unverified because there is no CI; development verification is Windows.
