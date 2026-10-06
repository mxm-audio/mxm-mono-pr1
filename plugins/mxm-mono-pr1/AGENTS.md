# AGENTS.md — plugins/mxm-mono-pr1

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

The nice-plug shell and shipped content for `mxm-mono-pr1`: a monophonic two-oscillator instrument
with additive wave paths, B-to-A sync, separate envelopes and the collection's **any-to-any
modulation routing** — fourteen sources by nine targets.
The DSP belongs to [`../../crates/mxm-mono-pr1-dsp`](../../crates/mxm-mono-pr1-dsp/AGENTS.md).
The plugin, dynamically paged production editor, automated editor/player proof and focused
real-host audio proof are implemented; visual §15 and real-DAW gates remain. Fidelity is
**UNVERIFIED**.

# Ownership

Owns `Cargo.toml`, `LICENSE`, `README.md`, `control-map.json`, `presets/` and `src/`: permanent
identity, 318 host parameters, state, host-event translation, audio layouts, realtime callback,
telemetry and factory content. It does not own DSP algorithms, shared preset behaviour, shared UI,
or the player.

# Local Contracts

## Permanent public surface

- Product literal: `mxm-mono-pr1`; permanent CLAP id: `dk.mxm.mxm-mono-pr1`.
- **The LFO rate has the collection's one tempo sync** (`lfo_sync`;
  `plans/plan-tempo-sync-controls.md`): the quarter note beside Rate, on `params::LFO_SYNC` (1/32 to
  four bars, the top the fastest). `MxmMonoPr1Params::synced_lfo_rate` resolves it once a callback
  from the modulated position, and `process` writes it over `next_voice_params`'s free rate; routes
  to the rate apply on top. The LFO core is also the internal Repeat clock, so a synced rate puts
  Repeat on the tempo too. `Telemetry::tempo` lets the knob read its division.
- **318 permanent ids**: 40 voice controls in `src/params.rs` — `lfo_sync` (2026-09-25) the newest — and **278 routing parameters** from
  `src/routes.rs`'s ten `#[nested(id_prefix = …)]` groups — nine targets of fourteen sources, the
  standard Amplitude (2026-09-26) among them, and the summing module's **thirteen**, because **the wheel is not a source on `Mod bus`**. The
  wheel already has a relationship with that bus and it is multiplication; wiring it in additively
  as well is the same control two ways into one stage, and all it can produce is a transposing
  offset riding on whatever else is in the bus rather than setting that thing's depth (the owner,
  2026-09-14). The gesture the bus exists for — *vibrato that fades in on the wheel* — is the LFO in
  the bus and `Wheel bus` at the target. `crate::routing::WHEEL_IS_NOT_A_BUS_SOURCE` and
  `Graph::set_topology` enforce it whatever a caller hands over.
  **Fifteen ids retired in all**, the two above included. Presets enumerate them in declaration
  order through `all_parameters`, routing appended after the voice so the voice controls keep the
  positions presets already wrote them in; the persisted `preset` field is state, not a host
  parameter.
- **Thirteen of those fifteen retired** under `plan-modulation-routing.md` decision 1.13, because the thing each
  named stopped existing: the eleven the Direct/Wheel buses named — `lfo_amount` was *one depth
  shared across every destination on a bus* where a route amount is per target, and the five
  destination enums named `Direct / Off / Wheel`, which names nothing once the buses are gone — plus
  `filter_env_amount` and `filter_keyboard_amount`, the two hard-wired cutoff paths that are routes
  now. **Every sound stays reachable**, and
  `crates/mxm-mono-pr1-dsp/tests/legacy_reachability.rs` demonstrates it over 20 000 randomised
  legacy patches rather than asserting it.
- **An old project loses its routing and keeps everything else.** No `filter_state` translation is
  built: this instrument is unreleased, the fifty factory sounds are regenerated either way, and a
  retirement cannot carry a host automation lane whatever `filter_state` does. That is a decision,
  not an oversight.
- Init is generated from those host defaults. **Every amount begins at zero, routing included,
  except the multiplier's own two factors and the wheel's vibrato** — `routing::INIT_AT_FULL` wires
  `Mod bus` and `Wheel` into `Wheel bus` at **full** amount, and `routing::INIT_WHEEL` (the owner,
  2026-09-28: the wheel did nothing on a fresh instance) wires the LFO into `Mod bus` at full and
  `Wheel bus` into both oscillators' frequency at `INIT_VIBRATO_SEMITONES` (reading *+0.50 st*), so
  pushing the wheel brings in vibrato. `routing::init_amount` and `init_present` are the one
  statement of all of it, which the DSP's `Routing::init`, the plugin's route defaults and the Mod
  bus's own group (`BusRoutes`) read. With the wheel at rest `Wheel bus` is zero, so **Init and all
  fifty factory sounds render bit-identically** (`preset::the_bank_digests`, 51 of 51, before and
  after); `the_wheel_brings_in_vibrato_at_init` holds the gesture (falsified with the LFO route
  out). The player's golden score moves the wheel, so it switches the three routes off to keep the
  patch it pinned. `product`'s
  neutral is one, so a factor present at zero depth makes the module publish a constant whatever its
  sources do and the route's presence would be a lie; full amount is what makes `Mod bus × wheel`
  come out, which is the machine's own two-stage routing and what a fresh instance must do. A's saw is the one audible source; B has its
  saw enabled but is mixed out and slightly detuned, so raising its level immediately beats; and the
  LFO has its triangle enabled, so raising any LFO route's depth immediately gives vibrato. **Both of
  those were off until 2026-09-14**, which meant the one control each card is named for produced
  silence — a waveform switch is *configuration* and the init contract asks for a useful one. The
  plugin default and Init are the same values.
- **Init wires seven routes: five at zero depth, plus the multiplier's two factors at full.** Two
  of the five are the machine's own hard wiring — the
  dedicated filter-envelope amount and the filter keyboard amount, which were knobs that always
  reached the cutoff. Three exist because `control-map.json` claims them as collection roles, and
  **a controller knob bound to an absent route does nothing at all**: an absent pair contributes
  nothing whatever its amount holds, so claiming a role without wiring its route would ship a dead
  knob. A route present at zero is audible as nothing, so none of the five changes the init sound.
  The five legacy destination switches defaulted to `Off`, so nothing else is wired.
- **A route's amount reads in its target's own domain**, semitones, octaves, or a percentage that
  carries its target's scale, and a typed number sets the depth it describes
  (`a_route_amount_reads_and_parses_in_its_targets_own_domain`). **It never prints a negative
  zero**: its number goes through `mxm_modulation_params::signed`, and
  `every_reading_survives_the_hosts_round_trip_a_rounded_zero_included` sends every amount, the
  bus's included, through the host's own conversion either side of zero, where a plain signed
  format printed `-0`, which `clap-validator`'s `param-conversions` fails whenever its random
  values land there.
- **No external input** (the owner, 2026-09-26). The External audio and Gate / clock auxiliary
  inputs — the hardware's switched audio jack, its detector, and its gate/clock jack — were removed:
  the LFO, Repeat's clock, has tempo sync, and the sequencer an external clock would have driven is
  outside this instrument. Noise is the mixer's third source and the LFO core is Repeat's only clock,
  as each was by default. **No id retired with them**: `repeat_external` and `noise_external_level`
  keep the ids their first names gave them and are named *Repeat* and *Noise level*; the routing
  source is *Noise*. A state or preset naming either id is unaffected.
- No effect, arpeggiator, note sequencer or random modulation may be added without changing the
  approved product boundary. **Velocity and channel pressure are now sources**, under
  `plan-modulation-routing.md` decision 1.7, which overrides this instrument's earlier boundary; each
  is a MIDI path the conversion added, starting at zero depth on every target so a fresh instance is
  still the copy. **A routable gate remains excluded** — the DSP crate's AGENTS.md carries why.
  **Amplitude is a target since 2026-09-26** (`mod_amp_*`, 28 new permanent ids, every pair absent at
  Init): the collection's standard factor after the VCA, drawn under the amplifier envelope on the
  Envelopes card. Every route amount is the collection's one parameter
  (`mxm_modulation_params::reading::amount_param_at`), reading what it delivers — a Key route per
  octave of keyboard — and `every_route_parameter_says_what_the_dsp_does` holds each to
  `mxm_mono_pr1_dsp::conformance` (`mxm_plugin_test::routing_checks`).

## Layouts and event handling

Two configurations, stereo then mono, and **no input of any kind**
(`the_layouts_are_stereo_then_mono_with_no_inputs`). A direct CLAP test selects both while
deactivated, requires bit-identical audible duplication from stereo, and hears noise alone and the
LFO-clocked Repeat through the real bundle.

The process callback translates supported note, choke, note-expression, channel bend/wheel,
**channel pressure** and all-notes/all-sound events in host order. A note-on's velocity reaches the
press ledger and is not part of a note's identity, so a note-off still matches on voice id or
channel/key.

**Topology is resolved once per buffer and the amounts advance in place per sample.** Which routes
are live changes only on a parameter event, and decision 1.9 puts a route's arrival at the processing
interval rather than at an exact sample, so it is resolved before the event-split loop rather than
inside it. Doing it at all is what stops the first block after any allocation rendering unrouted.
**Only a live route's smoother is advanced**, so an absent pair costs nothing per sample beyond the
branch that skips it and its stored depth stays exactly where the player put it. Pitch bend remains a normalized per-channel position; the Bend range target is available for
non-advancing event bookkeeping while its 20 ms pitch-signal smoother is applied every sounding
control frame, including after ownership fallback. It splits at sample offsets into a fixed 64-event stack
chunk; a full same-sample chunk is applied without advancing audio. Callback-end and zero-frame
events use the DSP's event-only transition and leave trigger pulses for the next rendered sample.
The audio thread allocates and locks nothing.

Activity comes from the DSP's effective Live/Tailing/Inert state. Inert returns sleep and exact
zero; the amplifier envelope owns the tail because every filter-envelope route is upstream of its
final VCA. Routed and disconnected filter envelopes neither outlive that VCA nor survive parking as
a ghost. Tail reporting is a conservative three times the longest envelope release plus 100
ms. All Sound Off's panic latch and deliberate recovery rules belong to the DSP contract and must
not be bypassed in the shell.

**Every parameter's host text is idempotent through CLAP's normalized value conversion** (format,
parse, format gives the same text). Cutoff's ambiguous `1.0 kHz` rounding bucket uses integer Hz
because 1000 Hz can preview just below the unit boundary after the normalized inverse; all other
values retain the usual Hz/kHz presentation. Its integer bucket is chosen from `value.round()`
while the Hz branch prints tenths, so `999.5 Hz` stays put only because this range's inverse of
999.5 lands at 999.49994 — not reproduced as a defect, and pinned rather than assumed. Envelope and
glide times choose `ms` or `s` from the *rounded* tenths of a millisecond: chosen from the raw value,
0.99995 s printed `1000.0 ms` and read back `1.00 s`.
`params::tests::every_parameter_text_is_idempotent_through_the_hosts_conversion` walks all 290
parameters with the unit on, over clap-validator 0.4.1's own grid, the `i / 19` grid and both sides
of every unit, precision and sign switch — 999.46 and 999.49 Hz among them.

## Presets, map and telemetry

**The fifty sounds' routing was converted once, and that conversion is unverified until B5.**
`crates/mxm-mono-pr1-dsp/tests/legacy_reachability.rs` is the *specification* of the translation and
proves the mapping itself over 20 000 randomised patches; the fifty stored values were produced from
that same table but are not re-derived by any test, so nothing mechanically links a particular
sound's numbers to what it used to be. The retired graph is gone, and keeping it as a live oracle
would mean shipping dead code. **The owner's listening pass is what closes this**, and until then no
claim is made that a factory sound still sounds as it did.

The factory set is exactly fifty complete, categorised, distinct snapshots plus generated Init.
`src/preset.rs` is the readable overrides-on-defaults design and its ignored generator writes the
compiled JSON files. Keep maker/model names out of preset names. Do not hand-edit generated files;
change the design and regenerate. Every sound must render finite, bounded audible output under its
intended note, and factory content must not move master Volume.

`control-map.json` claims only equivalent collection roles. Independent waveform enables are not a
single wave role. `osc2.sync` maps the switch that enables B as A's sync source. `mixer.src3` maps
the noise level, `noise_external_level`, whose id outlived the external input. Unsupported roles remain
unclaimed.

**Five routing roles are claimed, and each names the route Init wires** — `osc1.pwm_depth`,
`filter.env_amount`, `filter.key_track`, `filter.lfo_amount` and `lfo1.to_pitch`/`lfo1.to_filter`.
Decision 1.8: a fixed knob's promise is about Init, and a player who re-points that route has chosen
to. **Each claimed pair is present in Init at zero depth for that reason**; the note that Direct/Wheel
routing could not reduce to these roles retired with the buses.

**No status line reports voice or output state at all** — the owner's ruling, 2026-09-14: *drop that
text completely*. `Voice: Live · gate source: Keyboard` and `Output active` rewrote themselves on
every note and reported what a player could already hear; a panic line kept beside them for one round
went the same way on the owner's word. **The All Sound Off latch is therefore not displayed** — a
deliberate departure from the brief's earlier requirement, which is amended rather than left
contradicting the code. A player diagnoses a latched panic by playing a note, which clears it. The
gate-source telemetry went with the line that displayed it.
`proof::no_card_narrates_voice_or_output_state` is the negative check that keeps it gone.

Telemetry is atomic and observational only: meters, activity, panic and modulation
observations must not alter sound. The two module levels are `Mod bus` and `Wheel bus` — **two
stages rather than three**, because a destination that used to read a Direct bus now sums its own
routes at the target. The effective cutoff is **read from the voice's own sum, not recomputed**: keyboard
tracking and the filter envelope are routes now, so a second implementation here would drift.
**No display reads the audio as it runs** since 2026-09-28: the oscillator traces and the LFO's
live marker went (below), and with them the per-sample scope ring and the editor-open flag that
gated it. Developer requests remain gated by the collection's developer channel.

## Editor

The production surface has nine stable cards in the brief's category order: Voice; LFO,
**Envelopes** and the two modules — **`Mod bus`** and **`Wheel bus`**; the oscillator pair; Mixer, and
Filter. The module pair and the oscillator pair are preferred groups, never window
floors. **Both envelopes are one card** (the owner, 2026-09-25: *"these should be on the same
card"*, and *"underneath each other. So they take up less space"*): Filter over Amplifier, each
its name over its A, D, S, R faders (`tree::fader_row`), with no note (*"it explains the
obvious"*).

**Volume is on no card; it is the app bar's** (owner, 2026-09-18: every instrument's master volume
is in the app bar; design system §3.1). It is an inline slider beside the meter
(`Bound::slider_inline`, reserving the widest reading so the bar does not move under a drag), drawn
inside `mxm_ui::navigation::bar_card` under key `VOLUME_CARD` (64, outside the paging keys 0–9), and
the cursor reaches it through `paged_with_bar`. `editor::BAR_PARAMETER` names it for the inventory
tests. The *Amplifier and output* card held Volume alone, so it was removed rather than kept empty;
its caption — Volume is VCA drive, not an attenuator after the voice — is now Volume's tooltip. The
app bar is on every view, so the developer Parameters list is the one surface that draws Volume
twice.

**Routing is drawn under the control it moves**, never in a detached footer: each oscillator's
frequency and pulse-width stacks, the filter's cutoff and resonance stacks, and the LFO's rate stack.
The two **module** cards hold the two targets that have no knob to sit under, each with its own
live level: `Mod bus`, whose law is sum, and `Wheel bus`, whose law is **product** — its rows are
*factors*, and its level display's hover text says so, because a stack that looked identical to a
summing one would misdescribe what the knobs do. **No card carries a caption** (the owner,
2026-09-27; design system §7.6): what the seven used to say is the tooltip of the control it is
about — Glide mode, Repeat, LFO Rate and its shapes, Oscillator A's shapes and Sync, Oscillator B's
shapes — or a display's hover text (the two buses, the filter response), written for the player.
**Each card is named for the target it holds**, so its title, its rows and its entry in every
`‹ modulate ›` list read the same word — the owner's ruling, 2026-09-14: a card called *Modulation*
holding a source called *Mod bus* made the two look like different things.

**Every card is a `mxm_ui::tree`** (`crates/ui/AGENTS.md`, *A card body as data*).
`sections::card` describes a card's body once — knob columns, switches, wrapping toggle rows, route
stacks, captions and displays — and that one description is measured for the card's floor and
height and drawn leaf by leaf through the same bindings (`sections::paint`); the paged view is
`paging::editor::show`. **Floors are computed, not typed**: `page_items` takes each card's
tree's narrowest plus its chrome every frame, and this editor declares no usability minimum, so the
content floor is the floor, and the ceiling too: every card is exactly as wide as its content
(`plans/plan-editor-standard.md` A1). What sets a floor is what cannot shrink: a route stack at every
route revealed and its widest reading (`stack_size`), switch cells, and knob rows. **Painted names
drop the prefix their card already carries** (design system §7.1): *Octave*, *Sync*, *Low
frequency*, *Keyboard follow*, *Rate* (`Bound::panel`), and each stack's title from
`sections::TARGET_PANEL_NAMES` — *Frequency* and *Pulse width* on an oscillator, *Rate* on the LFO;
the canonical names stay the parameters' and the accessibility tree's. A switch's cells are its
parameter's own option text. A knob row is the collection's `mxm_ui::tree::knob_row` (at
`mxm_ui::control::knob_column`) as data, and **each knob's column is never narrower than its widest
reading**, which the knob draws on one line and elides where it does not fit. **No card shows
telemetry text**: the Mixer is its three faders, its jack status line and captions gone with the
external inputs, so a card's tree is built from the parameters alone. The plugin states
its visuals' sizes in `editor::visuals`: the LFO trace and the filter response fill the card at
`PLOT_HEIGHT`, the oscillator pictures at `TALL_PLOT_HEIGHT`, and a module's level at `STATUS_HEIGHT`
and at least `BUS_LEVEL_MIN_WIDTH` — its name's fixed `METER_LABEL_WIDTH` column, a `METER_MIN`
meter and the gutter. The native window minimum holds the widest card plus gutters (`MINIMUM`), and
the app bar at its last compact step is wider and sets it (`proof::the_app_bar_holds_in_the_minimum_window`); the
opening frame (`REFERENCE`) is measured by `proof::the_opening_size_is_the_budget_hugged`, not
chosen. User zoom remains fixed rather than inferred from window size.

Voice has no disclosure: Master tune and Bend range were source-panel controls and remain directly
visible; developer CC 118 therefore has no effect. Parameters remains a developer-only surface. The
shared app bar owns presets, Volume, output meter, zoom and persistent theme choice. **The LFO and
the two oscillators are pictures, not scopes** (the owner, 2026-09-28: *remove the dancing ball*;
*not animated — just show the waveform*): the LFO's summed shape from its three switches, with no
marker running along it (`visuals::lfo_sum_trace`), and each oscillator's two cycles from its wave
switches and pulse width (`visuals::oscillator_shape`, `OscillatorWaves`; held by
`an_oscillator_is_drawn_from_its_settings`), still until a setting changes. Bus, filter, activity
and panic views read lock-free telemetry and never feed DSP. **The Mod bus and Wheel bus levels**
follow the LFO and, for Wheel bus, the wheel — and both are computed only while the voice runs (a
key held, Repeat, Drone or the amplifier envelope), so they move while a note is sounding. The shared collection accent is used unchanged in both themes. **A target's routes are drawn by
`mxm_modulation_params::ui::stack`, which every editor in the collection shares**: rows come from
parameter values rather than editor state, a target with nothing routed draws no group at all,
`‹ modulate ›` sits outside the group and is labelled with its target, and each row ends in a remove
mark rather than a switch. None of those is this editor's to re-decide.

**Two labels are ambiguous by design and the proof asks by role.** A target's menu is named for its
target, so "Cutoff" and "LFO rate" each name a knob *and* a menu; the knob is the slider.
The card fits the quarter-4K page without weakening names or targets. Card-local knob
labels omit module words already supplied by their card while the full host names remain the
accessible identities and tooltip headings. The groups do not repeat a source caption over those
labels, and Filter's consecutive knob rows reserve explicit vertical
separation. This repairs the native 2026-09-08 report where full three-word labels and duplicated
group titles painted into the following text at the card floors.

`src/editor/proof.rs` owns private-state, AccessKit, balanced-gesture and layout proof.
`proof::every_card_passes_the_tree_checks_in_every_state` runs the shared per-card checks
(`mxm_plugin_test::tree_checks`) over every card in its structural-state matrix — the init patch,
**every route revealed at full negative depth** (Init draws seven of 125 pairs, which is why a check
there alone passes while a live row runs its removal mark through a card's border), and the LFO
synced with no tempo and with one; there is no disclosure and nothing reserved. It checks
both themes and fixed-physical quarter-4K production painting at 1×/1.5×/2× — the harness is handed
the physical window and the scale zooms it — plus 1×/2× paging;
narrow, opening and wide reflow; floor/ceiling enforcement; overlap, clipping, row bottoms and stable
sequence; pairwise painted-text separation in every dense card at its floor, including amount/route
baseline alignment; preferred envelope/module/oscillator pairs; every permanent musician
control and semantic view inside its requested card, and Volume drawn once in the app bar, above
every card and inside the window at each tested scale and at the one-card minimum, editable there as one
balanced gesture; direct visibility of Master tune and Bend range; and no lone-card stretch. Visualization
geometry and stroke hierarchy come from `mxm-ui::visual`; consumers derive no theme colors. The player behavior suite proves GUI advertisement headlessly and has a
deliberately invoked native test that opens, closes and reopens the release bundle. These are not
visual judgment or real-DAW parenting/resizing proof.

# Work Guidance

- Preserve the permanent ids, two-layout order, bounded callback splitting and DSP event order.
- Use `mxm-preset` in place; do not duplicate its format, storage or browser logic locally.
- Keep all shipped names software-native and preserve the no-effects/no-conveniences boundary.
- Editor work follows [`../../docs/briefs/mxm-mono-pr1.md`](../../docs/briefs/mxm-mono-pr1.md) and
  the normative design system. Keep visual judgment and real-DAW parenting/resizing explicit manual
  gates; headless geometry must not claim either.

## A routing proof must advance the amounts, not just the topology

`Voice::set_topology` says **which pairs are live**; it does not set a single depth. The amounts
arrive from the parameter smoothers through `Routes::advance_into`, which the realtime callback calls
**per sample**. A test that arms the topology and then renders is therefore playing every route at
**zero depth** — present, and contributing nothing.

That is not hypothetical: `preset::tests::every_factory_sound_renders_finite_audible_output` did
exactly this, so all fifty factory sounds were rendered with their entire routing silent and the test
passed, because a saw through an open filter is audible whatever its modulation does. Any render that
means to prove something about routing calls `advance_into` in its own sample loop, the way
`process` does.

**`tests::the_wheel_fades_the_modules_vibrato_into_rendered_audio` is the end-to-end check**, and it
goes through the real callback rather than the DSP: it sets the LFO into `Mod bus` and the multiplier
at Oscillator A's frequency **by permanent id**, then measures the *rendered audio's period* at three
wheel positions. It exists because the instrument's headline gesture shipped with nothing that
played it — the DSP proof held its `Routing` by hand, so a parameter side wired to the wrong target
would have gone unnoticed. Falsified: pointing the pitch route at the raw LFO makes the wheel-down
case fail.

## An empty `Mod bus` makes the multiplier publish zero, and the interface does not say so

`Wheel bus` ships with `Mod bus` as a factor. `product`'s factor for a route at full amount is the
source itself, so an **empty `Mod bus` is a zero factor and the whole product is zero** — correct
arithmetic, and a trap: the two module cards look alike, so putting the LFO on `Wheel bus` is the
obvious thing to try and it yields *silence at every wheel position*, not merely a different sound.
The working gesture is **LFO → `Mod bus` → `Wheel bus` → target**, and as of 2026-09-14 nothing on
either card says so. The owner hit it. It is an open interface question for the plan's B5 — a caption
naming the chain, an inert `Wheel bus` reading on the meter, or a different init wiring — and it is
recorded here rather than patched on a guess.

# Verification

`target/bundled/` is a shared mutable profile slot. Before each collection resize command below,
stage **every name in the collection's `editor_resize` inventory (newdawn-workspace)** in the stated profile; never mix
profiles. The native lifecycle, validator, robustness and resize checks run while their matching
artifact is staged. After debug proof, repeat release staging for the complete inventory and leave
release in the slot.

```bash
cargo test -p mxm-mono-pr1 --no-fail-fast    # 58 tests, the ignored generator and pictures; the routing gesture is one of them
# Every page, light and dark, for review -> target/layout-tree/mxm-mono-pr1/<MXM_PICTURES tag>/
MXM_PICTURES=after cargo test -p mxm-mono-pr1 --lib tree_pictures -- --ignored

# RELEASE PROFILE: stage every resize-inventory bundle with --release first.
cargo xtask bundle mxm-mono-pr1 --release
clap-validator validate target/bundled/mxm-mono-pr1.clap
cargo test -p mxm-mono-pr1-host-tests --test behaviour -- --nocapture
cargo test -p mxm-mono-pr1-host-tests --test behaviour editor_opens_closes_and_reopens_through_the_player -- --ignored --nocapture
cargo test -p mxm-mono-pr1-host-tests --test robustness -- --nocapture
cargo test -p mxm-mono-pr1-host-tests --test golden_audio -- --nocapture
# then newdawn-workspace's editor_resize: every product's editor, natively resized

# DEBUG PROFILE: stage every resize-inventory bundle without --release first.
cargo xtask bundle mxm-mono-pr1
clap-validator validate target/bundled/mxm-mono-pr1.clap
cargo test -p mxm-mono-pr1-host-tests --test behaviour editor_opens_closes_and_reopens_through_the_player -- --ignored --nocapture
cargo test -p mxm-mono-pr1-host-tests --test robustness -- --nocapture
# then newdawn-workspace's editor_resize: every product's editor, natively resized

# RESTORE: stage every resize-inventory bundle with --release again; validate release last.
cargo xtask bundle mxm-mono-pr1 --release
clap-validator validate target/bundled/mxm-mono-pr1.clap

cargo clippy -p mxm-mono-pr1 --all-targets -- -D warnings
cargo fmt --package mxm-mono-pr1 -- --check
cargo test -p mxm-mono-pr1 --lib write_the_factory_presets -- --ignored # regenerates files
```

The focused plugin suite currently has **58** passing tests, one ignored generator and the ignored
pictures. Preset tests pin
50 files, completeness, uniqueness, categories, exact generated content, unchanged output volume,
and finite audible renders. Core tests pin identity/bundling, all ids/defaults, persisted preset
state, layouts, dense bounded event splitting/order/panic, telemetry and the Live → Tailing → exact
Inert lifecycle.

The host tests' `behaviour` (`cargo test -p mxm-mono-pr1-host-tests`) includes default real-bundle tests through ordinary player discovery plus
a direct all-configuration harness: map and all 318 parameters, exact rest/release silence, pitch,
host cutoff edits, both key modes, Drone wake, a same-instance CLAP-state round trip, floating-GUI
advertisement, both layouts with no input, noise and the LFO-clocked Repeat heard through them, and
stereo duplication.
Their `golden_audio` pins a broad deterministic real-host score and documents controlled
listen-before-update regeneration. Its ignored native lifecycle test opens, closes and reopens this
editor in both profiles, and the collection `editor_resize` inventory then natively resizes it in
both profiles before release is restored. The 2026-09-08 Windows run passed the focused native
open/close/reopen lifecycle and the manifest-checked ten-bundle resize inventory in both debug and
release:
mxm-mono-pr1 opened twice per resize profile, accepted all 24 sizes per open and produced zero host
round-trips; after the Cutoff text repair, release validation passes all 35 applicable checks with 9
unsupported-extension checks skipped, and the release inventory was left staged. Their `robustness` has three direct-host tests at 1 kHz
to 768 kHz and callback sizes 1 to 8192, with several event offsets and a 193-event same-callback
burst; run it against debug to activate nice-plug's process-allocation assertion and against release.
The controller map also requires focused JSON/schema, role and parameter-id validation.
`clap-validator validate target/bundled/mxm-mono-pr1.clap` on the repaired release bundle passes all
35 applicable tests with 9 unsupported-extension checks skipped. The former Cutoff
`param-conversions` failure is pinned by
`every_parameter_text_is_idempotent_through_the_hosts_conversion` rather than waived; the
2026-09-16 debug and release runs pass 35 with 9 skipped. The ordered-edge sync repair's independently reproduced broad release render
`8e1c04ab75ef28f5` is provisionally pinned and its focused sensitivity discriminator passes. No
human listening is claimed, and final reference listening remains a manual release gate. Linux and macOS are unverified because there is no CI; development
verification is Windows.

# Child DOX Index

No child AGENTS.md files. `presets/` and `src/` are covered here.
