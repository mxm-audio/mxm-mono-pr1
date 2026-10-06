# mxm-mono-pr1 — UI design brief

Required by mxm-kit's [`MXM_DESIGN_SYSTEM.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/MXM_DESIGN_SYSTEM.md) §14 and written before editor work. The instrument is a
functional copy of the documented 1981 Sequential Circuits Pro-One voice, cited as
`research:instruments/pro-one.md`; the interface is not a copy of its panel. **Fidelity is
UNVERIFIED:** no hardware or installed emulation was measured for the research page.

**Brief status: IMPLEMENTED through editor phase 2.** The production editor follows this inventory;
automated painted-layout, accessibility, gesture and player open/close/reopen proof passes. The owner
approved the silent-idle adaptation and shared collection accent on 2026-09-08. The product name is
settled as `mxm-mono-pr1` by the owner's normalised-model-token ruling of the same date.
Recognisability, native-window visual fit, real-DAW parenting/resizing and design-system §15 remain
manual release gates.

## Product boundary before the ten questions

- **Keep the analogue voice, both key modes, automatic legato glide, Repeat articulation and
  Drone.** These determine how the envelopes and voice respond.
- **Leave out the arpeggiator and forty-event note sequencer.** The machine's own documentation
  separates the Intel 8021 note machinery from the sound generator, and MXM Player/the DAW already
  own note sequencing and arpeggiation. Their volatile memory, traversal rules and destructive
  forty-event limit are keyboard behaviour, not voice behaviour. The omitted modes' special LFO
  phase reset is therefore omitted too rather than exposed out of context.
- **Keep no effects.** The hardware has none (`research:instruments/pro-one.md` §1 and §9). Delay,
  chorus, reverb, EQ, resonators and wavefolding remain separate products; no effect promotion is
  created by this instrument.
- **No external inputs** (the owner, 2026-09-26, amending this brief). The hardware's external audio
  input with its detector, and its gate/clock jack, are not reproduced: the LFO — Repeat's clock —
  has tempo sync, and the original's sequencer is outside this instrument, so an external clock has
  no use here. Noise, which a connected audio jack used to replace, is the mixer's third source, and
  the plugin offers stereo and mono output with no input. A host parameter/note path replaces the
  original pitch and filter-CV jacks without drawing CV sockets.
- **Modulation is the collection's any-to-any routing, and the machine's own graph is the init
  patch.** The fixed three sources, two buses and five destinations were the instrument until
  `plan-modulation-routing.md` decision 1.1; they are now fourteen sources by nine targets, with the
  summing stage surviving as a `Mod bus` module and the wheel stage as a **`Wheel bus` multiplier
  module** — decision 1.14 — whose rows are factors rather than addends. **Every sound the buses
  could make is still reachable**, demonstrated rather than asserted, and a fresh instance still
  behaves like the hardware: every route starts at zero depth except the multiplier's own two
  factors, which start at full so that `Mod bus × Wheel` is what a fresh instance does. An **editable
  filter-model library** remains out. A deterministic calibration models one unit; unmeasured unit
  variation remains labelled rather than filled with recovered competitor constants.
- **Freeze free-running sources while inert.** The owner approved this host adaptation on
  2026-09-08: once no reachable source or tail can sound, oscillator/LFO phase freezes and targeted
  state settles. It satisfies zero-cost idle and deterministic rendering while deliberately
  differing from continuously powered hardware. There is no local silent-live exception.

## Established warts to preserve

The DSP contract must carry the full evidence table from `research:instruments/pro-one.md` §8.
The editor must make the operational ones understandable: mode-dependent low/last-note priority and
triggering; additive fixed-gain audio-oscillator and LFO waveforms; pulse silence at either duty
extreme and under deep PWM;
wide-pulse dropout under sync; inaudible Oscillator B still resetting Oscillator A; Oscillator B's
audio and modulation roles sharing one summed waveform; Oscillator B self-modulation; the duplicate
filter-envelope route; Drone holding envelope
gate rather than bypassing the VCA; Auto glide being overlap-only; and Volume changing VCA drive
rather than attenuating a completed output. The excluded arpeggiator/sequencer warts are not quietly
recreated elsewhere.

## 1. Primary sound-design task

**Build an animated two-oscillator tone by combining waveforms, sync and Oscillator B modulation,
then route what moves it — immediately, or through the wheel.** The instrument's distinguishing
workflow is the interaction between Oscillator B and the two modules, not a generic
oscillator–filter patch: B's own waveform sum is a modulation source, and `Mod bus` → `Wheel bus` is
where a player decides which motion the wheel governs — and, because the second module multiplies,
how much of it gets through moment to moment.

## 2. The three to five parameters users reach for most

1. **Cutoff** and **Resonance** — the CEM3320 low-pass is the final tonal centre.
2. **Oscillator B frequency** — detune, interval, sync ratio and modulation rate all meet here.
3. **Cutoff from Filter envelope** — one route on the Filter card, carrying what the dedicated
   amount and the bus path used to add together. Still the fastest way to shape motion.
4. **Mod bus from LFO** — the depth the wheel governs, on the `Mod bus` card.
5. **Oscillator mix** — balancing A against B exposes beating and sync tone.

Cutoff and Resonance are Primary. Oscillator B frequency and the source levels are Standard; a route
amount is a slider in its target's own stack and reads in that target's unit. Density elsewhere uses Compact presentation without changing the shared
pointer or typography floors.

## 3. Signal flow that must be visible

```text
host notes → key mode → key CV → Normal/Auto glide ───────┬→ Oscillator A ─┐
                                                          └→ Oscillator B ─┤
Noise → source level ──────────────────────────────────────────────────────┤
                                              source mixer → 4-pole LPF → VCA → Output
host key gate/trigger ───────────────────────────────┐          ↑     ↑    ↑
LFO core clock → Repeat                              ├→ Filter envelope
Drone held gate ────────────────────────────────────┘          └→ Amplifier envelope

sources, published in this order each sample:
  key · velocity · wheel · pressure · bend · noise · LFO · gate ·
  filter envelope · amplifier envelope · Mod bus · Wheel bus · Oscillator B · Oscillator A

                  ┌──────────────┐          ┌──────────────┐
  any source ────→│   Mod bus    │─────────→│  Wheel bus   │──→ Wheel bus (a source)
                  │  (adds)      │  ×wheel  │  (multiplies)│
                  └──────────────┘          └──────────────┘
                        ▲                          ▲
                   any source                 any source: its rows are factors, and
                                              `Mod bus × Wheel` is only how it ships

targets, each combining its own present routes:
  A freq · A PW · B freq · B PW · cutoff · resonance · LFO rate · Mod bus · Wheel bus
```

Eight relationships must read without a manual:

- Oscillator and LFO wave switches add rather than select; the LFO's enabled rising saw, triangle
  and square form one modulation sum;
- Repeat timing reads the LFO core clock independently of those enables, so no enabled waveform is
  required and combinations do not change gate timing;
- Oscillator B's enabled waveform sum is both audio and a modulation source;
- **any source reaches any target**, one row per route beneath the control it moves, and a target
  with nothing routed shows nothing at all;
- `Mod bus` **adds** whatever is put in it and `Wheel bus` **multiplies** whatever is put in it, and
  the two wired together as shipped — `Mod bus × Wheel` — are what the hardware's two-stage routing
  was. The same source can now be immediate at one destination *and* wheel-controlled at another,
  which the two-bus machine could not do; and because the second module multiplies rather than adds,
  the wheel sets *how much* of the motion gets through — vibrato that fades in while you play —
  rather than adding a transposing offset of its own;
- the same gate/trigger logic drives both envelopes, the Amplifier envelope drives the VCA, and the
  Filter envelope reaches the cutoff as one route carrying what the dedicated amount and the bus
  path used to add together;
- sync follows Oscillator B's core even when its mixer level and wave switches make it inaudible.

## 4. Which controls belong in Play view

**No Play view.** Host note input, pitch bend and the modulation wheel are performance gestures, not
a second panel — and each is *also* a routable source now, which is where a player reaches for them
rather than on a macro page. The reached-for controls in §2 remain prominent on their owning cards.
A compact live readout of Normal/Repeat/Drone state and the two bus stages belongs with those cards.

## 5. Advanced controls and disclosure

- **There is no musician-control disclosure.** Master tuning and bend range were physical controls
  on the source instrument and remain directly visible in **Voice**. Treating a less frequently
  adjusted original control as software-added depth would hide part of the machine rather than
  improve its interface.
- **This editor shows no voice or output status at all** — the owner's ruling, 2026-09-14: *drop
  that text completely*. A running Live/Tailing/Inert readout, a gate-source label and a panic line
  were all removed; the first two changed on every note and told a player what they could already
  hear, and the panic line went with them on the owner's word. All Sound Off's latch is therefore
  **not displayed**, which amends this brief's earlier requirement rather than leaving it
  contradicting the editor. A player clears the latch by playing a note, and the DSP's own recovery
  rules are unchanged.
- Oscillator B's low-frequency and keyboard-follow switches, Oscillator A sync, waveform enables,
  pulse widths, key mode, Auto glide, Repeat and Drone remain visible. Hiding them would conceal
  the machine's defining couplings.
- **Routing is drawn under the control it moves**, never in a detached footer: a stack of rows
  beneath each oscillator's frequency and pulse width, the filter's cutoff and resonance, and the
  LFO's rate. A target with nothing routed draws no group at all, and `‹ modulate ›` beneath it adds
  the next source. Two **module** cards carry the two targets with no knob to sit under — **`Mod
  bus`**, which adds, and **`Wheel bus`**, which multiplies — each with its own live level and each
  **named for the target it holds**, so a card's title and its entry in every `‹ modulate ›` list
  read the same word (the owner, 2026-09-14). The multiplier's level display says on hover that
  its rows are factors (a caption did, until the owner ruled out help text on the panel, 2026-09-27),
  because a stack that looked identical to a summing one would misdescribe what the knobs do. The set
  of parameters is fixed at release like every other; what a player edits is which of them are
  present.

## 6. Primary categories, kinds, stable cards and groups

Space-derived pages follow design-system §3.2 in category and authored order:

| Category | Stable cards | Kind / grouping |
|---|---|---|
| Performance | **Voice** | key mode, glide, Repeat, Drone, tuning and bend range; one performance card |
| Modulators | **LFO**, **Envelopes**, **Mod bus**, **Wheel bus** | LFO exposes three independent waveform enables and their summed trace; the two envelopes share one card, one under the other as faders (the owner, 2026-09-25); the two modules are a preferred parallel pair — one feeds the other; **Mod bus** and **Wheel bus** are the two targets with no knob of their own, each holding its own stack and live level, one adding and one multiplying |
| Generators | **Oscillator A**, **Oscillator B** | parallel oscillator pair; keep together while space permits |
| Tone | **Mixer**, **Filter** | audio-chain run |

**Volume is on no card.** It is the instrument's output level, so it is an inline slider in the app
bar beside the meter (design system §3.1; the owner's ruling of 2026-09-18 for every instrument).
The *Amplifier and output* card held it alone and is gone; its explanation — Volume is VCA drive,
not an attenuator after the voice — is Volume's tooltip.

The separate generated **Parameters** surface remains developer-only at CC 119 value 127 and has no
tab. There is no Sequencers or Effects card. Mixed-purpose placement is explicit: Oscillator B stays
a Generator although its sum is also a modulation source; the two **module** cards are Modulators
because they distribute modulation rather than carry audio, and every other target's routes sit on
the card that owns the control they move.

## 7. Identity accent

**The shared collection accent, approved by the owner on 2026-09-08:** dark `#4CC9D8`, light
`#247F91`. It avoids imitating the source's black panel and coloured control scheme and does not
consume another crowded identity hue.

The existing WCAG measurements for that token are 8.84:1 / 8.09:1 against dark `surface-1` /
`surface-2`, and 4.65:1 / 3.89:1 against the corresponding light surfaces. It clears 4.5:1 for text
on `surface-1` and 3:1 for control boundaries on both; light-theme text on `surface-2` uses the
normal text token. These are token measurements, not visual QA of this editor.

## 8. Live visualizations

1. **Each oscillator's waveform**, drawn from its settings: the enabled waves summed at the set
   pulse width, two cycles, still (the owner, 2026-09-28: *not animated — just show the waveform*;
   it replaced a running dual-oscillator scope). It makes additive waves and wide-pulse dropout
   understandable.
2. **LFO sum and two-stage bus activity:** the LFO card shows each enabled rising-saw, triangle and
   square contribution in one summed trace; **Modulation** shows the summing module's level and what
   the wheel has left of it. It communicates additive routing without fake cables.
3. **Filter response** at set and effective cutoff, including the resonant peak. The curve is an
   explicitly labelled analytic view of the model, not hardware measurement.
4. **Output level and latched clip** in the app bar beside Volume, measured after it.

Telemetry is observational and droppable: atomics or a bounded lock-free snapshot only, published
at block/event boundaries where possible. Peak is max-combined and reset on read; clip latches until
acknowledged. DSP never reads editor state, and expensive traces stop when the editor is closed.

## 9. What is removed from the source hardware layout, and why

**Kept:** control membership, the machine's own routing *as the init patch*, and the sequence
implied by the signal path. **Removed:** panel geometry, hardware appearance and keyboard/storage
limitations — and the **fixed reach** of the routing itself, which decision 1.1 makes an approved
improvement on the same footing as every other interface improvement a copy is free to make. What a
route can reach is software; what it does at Init is the hardware.

| Removed | Why |
|---|---|
| Black panel, typography, coloured caps, knobs, switches, case and keyboard | Design-system §2 forbids a hardware replica and decorative keyboard |
| Original panel positions | Reflow has sequence rather than positions; cards follow category and signal flow |
| Wheel graphics | Host pitch bend and CC 1 provide the gestures without drawing hardware controls |
| Arpeggiator controls and forty-event sequencer | Player/DAW note-side responsibility; the digital scanner does not generate the voice |
| CV/gate output jacks and internal TTL interface | Host event/output routing already owns transport between devices; they do not alter this voice |
| Fake patch cords for modulation routing | A stack of rows under the control each one moves, and live bus levels, preserve the function accessibly |
| Volatile patching and absent preset memory | The collection's parameter state and preset system improve recall without changing DSP |

The source block order informs only the functional sequence. No manufacturer wordmark, model
caption, source colour arrangement, fake hardware component or source screenshot enters the editor.

## 10. Quarter-4K fit, smaller sizes and 200% zoom

The opening and minimum logical sizes are **not guessed here**. Editor implementation measures every
card's overflow and usability floors, derives an opening size inside the **1920 × 1080 physical**
budget, and records the native window chrome, available logical content and DPI scale.

Acceptance is checked on every derived page, both themes, with every source-panel control visible:

- at 100% editor zoom in a quarter-4K physical window;
- at the one-card minimum, the widest computed card floor plus gutters, with only indivisible
  overflow scrolling;
- at 150% and 200% while the physical window remains fixed, using reflow/scrolling rather than
  shrinking typography or targets;
- with painted labels, values and controls inside the viewport, not merely card rectangles.

Oscillator A/B and the two modules are preferred groups, not minimum-width requirements. The
widest honest card floor sets the resizable minimum. **No DPI or native-window fit has yet been
run**, so §15 responsive QA remains unmet rather than implied by this brief.

## Init patch

One plain Oscillator A waveform sounds through an effectively open filter and the Amplifier
envelope. **The LFO has its triangle enabled and Oscillator B its saw** — a waveform switch is
configuration, so the init contract asks for a useful one, and with every switch off the one control
each card is named for produces *silence*: raising an LFO route's depth does nothing, and raising B's
level does nothing. Both were off until the owner found it on 2026-09-14. B stays mixed out and
slightly detuned, so raising its level now immediately beats, which is what the line below has always
promised. **Every amount begins at zero, routing included, except the multiplier's own two
factors and the wheel's vibrato** — `Wheel bus` ships wired to `Mod bus × Wheel` at **full** amount,
and since 2026-09-28 the LFO feeds `Mod bus` and `Wheel bus` reaches both oscillators' pitch at
±0.5 semitone, so the mod wheel brings in vibrato on a fresh instance (the owner: the wheel did
nothing). A product's neutral is one, so a factor at zero depth would make the
module publish a constant whatever its sources did and the route's presence would be a lie; full
amount is what makes the machine's own two-stage routing come out of two routes. None of it changes
the init sound with the wheel at rest. Useful configuration defaults select
sensible waveforms, rates, envelope times and key mode. Oscillator B starts slightly detuned but silent in the mixer, so raising its level
immediately produces beating. Init is not Drone or Repeat, and it equals the CLAP defaults exactly.

**Seven routes are present: five at zero depth, plus the multiplier's two factors at full.** Two of
the five are the machine's own hard wiring — the filter
envelope and the keyboard into the cutoff, which were knobs that always reached it. Three are there
because `control-map.json` claims them as collection roles and a controller knob bound to an absent
route does nothing at all: an absent pair contributes nothing whatever its amount holds. A route at
zero is audible as nothing, so a fresh instance is the machine with its own five paths shown, each
one gesture from being turned up.

## Recognisability trial

Run first on a wireframe and then on the finished editor with someone familiar with a Pro-One's
workflow, without showing source imagery:

1. enable two Oscillator B waves and identify that both feed its audio and modulation sum;
2. enable two LFO waves, identify their summed shape, then route that sum to Oscillator A pitch
   directly while putting the Filter envelope in **Mod bus** and routing **Wheel bus** to the
   cutoff, so the wheel controls one and not the other;
3. take the Filter envelope off the cutoff and put it back, and explain why the depth returned;
4. make Oscillator A sync to an inaudible Oscillator B;
5. switch from low-note/single-trigger phrasing to last-note/retrigger phrasing;
6. turn on Repeat, sync the LFO to the host tempo, and identify that both envelopes now retrigger
   on the beat.

Each task must be found within ten seconds and at most one wrong derived page. Gate: five of six.
Results are unrun and must never be reported as passed until observed.
