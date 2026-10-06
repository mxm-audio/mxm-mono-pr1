# AGENTS.md — plugins/mxm-mono-pr1

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

The nice-plug shell and shipped content for `mxm-mono-pr1`: a monophonic two-oscillator instrument
with additive wave paths, B-to-A sync, separate envelopes and the collection's **any-to-any
modulation routing** — fourteen sources by nine targets.
The DSP belongs to [`../../crates/mxm-mono-pr1-dsp`](../../crates/mxm-mono-pr1-dsp/AGENTS.md).
The plugin, dynamically paged production editor, automated editor/player proof and focused
real-host audio proof are implemented; visual §15 and real-DAW gates remain. Fidelity is
**UNVERIFIED**. The history, rulings and measurements behind each rule are in [NOTES.md](NOTES.md).

# Ownership

Owns `Cargo.toml`, `README.md`, `control-map.json`, `presets/` and `src/`: permanent
identity, 318 host parameters, state, host-event translation, audio layouts, realtime callback,
telemetry and factory content. Its licence is the repository's root `LICENSE`. It does not own DSP algorithms, shared preset behaviour, shared UI,
or the player.

# Local Contracts

## Permanent public surface

Detail: [NOTES.md § Permanent public surface](NOTES.md#permanent-public-surface).

- Product literal: `mxm-mono-pr1`; permanent CLAP id: `dk.mxm.mxm-mono-pr1`.
- **One tempo sync**, on the LFO rate (`lfo_sync`, `params::LFO_SYNC`), resolved once a callback by
  `MxmMonoPr1Params::synced_lfo_rate`; routes to the rate apply on top. It also clocks Repeat.
- **318 permanent ids**: 40 voice controls in `src/params.rs` and 278 routing parameters from
  `src/routes.rs`'s ten `#[nested(id_prefix = …)]` groups. **The wheel is not a source on `Mod bus`**
  (`WHEEL_IS_NOT_A_BUS_SOURCE`, enforced by `Graph::set_topology`), so the module has thirteen.
- **Fifteen ids retired**, thirteen under decision 1.13; every sound stays reachable
  (`crates/mxm-mono-pr1-dsp/tests/legacy_reachability.rs`). Presets enumerate parameters in
  declaration order through `all_parameters`, routing appended after the voice; the persisted
  `preset` field is state, not a host parameter.
- **An old project loses its routing and keeps everything else**: no `filter_state` translation, by
  decision.
- **Init deviations** (the DSP's `routing.rs` and this crate's `routes.rs` point here). Init is
  generated from the host defaults, and **every amount begins at zero except**
  `routing::INIT_AT_FULL` (`Mod bus` and `Wheel` into `Wheel bus` at full, because `product`'s
  neutral is one) and `routing::INIT_WHEEL` (the owner, 2026-09-28: LFO into `Mod bus` at full,
  `Wheel bus` into both oscillators' frequency at `INIT_VIBRATO_SEMITONES`, so the wheel brings in
  vibrato). `routing::init_amount` and `init_present` are the one statement of it. With the wheel at
  rest, Init and all fifty factory sounds render bit-identically (`preset::the_bank_digests`);
  `the_wheel_brings_in_vibrato_at_init` holds the gesture. The plugin default and Init are the same.
- Init sounds A's saw; B's saw is enabled but mixed out and detuned; the LFO triangle is enabled.
- **Init wires seven routes**: five at zero depth (the two hard-wired cutoff paths and the three
  `control-map.json` roles, because a controller knob bound to an absent route does nothing) plus
  the multiplier's two factors at full.
- **A route's amount reads in its target's own domain** and parses typed text there
  (`a_route_amount_reads_and_parses_in_its_targets_own_domain`); **it never prints a negative zero**
  (`mxm_modulation_params::signed`; `every_reading_survives_the_hosts_round_trip_a_rounded_zero_included`).
- **No external input** (the owner, 2026-09-26). No id retired with it: `repeat_external` and
  `noise_external_level` keep their ids, named *Repeat* and *Noise level*.
- No effect, arpeggiator, note sequencer or random modulation may be added without changing the
  approved product boundary. Velocity and channel pressure are sources (decision 1.7), at zero
  depth on every target. **A routable gate stays excluded.** Amplitude is a target (`mod_amp_*`).
  `every_route_parameter_says_what_the_dsp_does` holds each route to `mxm_mono_pr1_dsp::conformance`.

## Layouts and event handling

Detail: [NOTES.md § Layouts and event handling](NOTES.md#layouts-and-event-handling).

- Two configurations, stereo then mono, and **no input of any kind**
  (`the_layouts_are_stereo_then_mono_with_no_inputs`).
- The callback translates note, choke, note-expression, channel bend/wheel, **channel pressure** and
  all-notes/all-sound events in host order. Velocity is not part of a note's identity.
- **Topology is resolved once per buffer**, before the event-split loop, and the amounts advance in
  place per sample. **Only a live route's smoother is advanced.**
- Bend is a normalized per-channel position; the Bend range's 20 ms smoother runs every sounding
  control frame. Events split at sample offsets into a fixed 64-event stack chunk; a full same-sample
  chunk is applied without advancing audio. Callback-end and zero-frame events use the DSP's
  event-only transition. The audio thread allocates and locks nothing.
- Activity is the DSP's Live/Tailing/Inert; Inert returns sleep and exact zero. Tail reporting is
  three times the longest envelope release plus 100 ms. **Never bypass the DSP's panic latch and
  recovery rules in the shell.**
- **Every parameter's host text is idempotent through CLAP's normalized conversion**
  (`params::tests::every_parameter_text_is_idempotent_through_the_hosts_conversion`); Cutoff's
  `1.0 kHz` bucket uses integer Hz.

## Presets, map and telemetry

Detail: [NOTES.md § Presets, map and telemetry](NOTES.md#presets-map-and-telemetry).

- **The fifty sounds' converted routing is unverified until the owner's listening pass (B5)**; make
  no claim that a factory sound still sounds as it did.
- The factory set is exactly fifty complete, categorised, distinct snapshots plus generated Init.
  `src/preset.rs` is the readable overrides-on-defaults design and its ignored generator writes the
  compiled JSON files. Keep maker/model names out of preset names. Do not hand-edit generated files;
  change the design and regenerate. Every sound must render finite, bounded audible output under its
  intended note, and factory content must not move master Volume.
- `control-map.json` claims only equivalent collection roles; unsupported roles stay unclaimed.
  **Five routing roles are claimed, each naming a route Init wires at zero depth** (decision 1.8).
- **No status line reports voice or output state**, and the All Sound Off latch is not displayed
  (the owner, 2026-09-14; `proof::no_card_narrates_voice_or_output_state`).
- Telemetry is atomic and observational only. The effective cutoff is **read from the voice's own
  sum, not recomputed**. No display reads the audio as it runs.

## Editor

Detail and rulings: [NOTES.md § Editor](NOTES.md#editor).

- Nine stable cards in the brief's category order; **both envelopes are one card**.
- **Volume is on no card; it is the app bar's** (`VOLUME_CARD`, `editor::BAR_PARAMETER`).
- **Routing is drawn under the control it moves**; the two module cards hold `Mod bus` (sum) and
  `Wheel bus` (product: its rows are factors). **No card carries a caption**; explanations are
  tooltips. **Each card is named for the target it holds.**
- **Every card is a `mxm_ui::tree`** (`sections::card`, `sections::paint`). Floors are computed
  (`page_items`), never typed. Painted names drop their card's prefix (`sections::TARGET_PANEL_NAMES`);
  canonical names stay the parameters'. Each knob's column is never narrower than its widest
  reading. **No card shows telemetry text.** Visual sizes live in `editor::visuals`.
- `MINIMUM` is held by `proof::the_app_bar_holds_in_the_minimum_window`; `REFERENCE` is measured by
  `proof::the_opening_size_is_the_budget_hugged`, not chosen. User zoom is fixed.
- **The LFO and oscillators are pictures, not scopes** (`an_oscillator_is_drawn_from_its_settings`).
  Telemetry views never feed DSP. Route stacks are the shared `mxm_modulation_params::ui::stack`,
  not this editor's to re-decide.
- Two labels ("Cutoff", "LFO rate") are ambiguous by design; the proof asks by role.
- `src/editor/proof.rs` owns private-state, AccessKit, balanced-gesture and layout proof;
  `proof::every_card_passes_the_tree_checks_in_every_state` runs the shared checks. These are not
  visual judgment or real-DAW parenting/resizing proof.

# Work Guidance

- Preserve the permanent ids, two-layout order, bounded callback splitting and DSP event order.
- Use `mxm-preset` in place; do not duplicate its format, storage or browser logic locally.
- Keep all shipped names software-native and preserve the no-effects/no-conveniences boundary.
- Editor work follows [`../../docs/briefs/mxm-mono-pr1.md`](../../docs/briefs/mxm-mono-pr1.md) and
  the normative design system. Keep visual judgment and real-DAW parenting/resizing explicit manual
  gates; headless geometry must not claim either.
- **A routing proof must advance the amounts, not just the topology.** `Voice::set_topology` sets no
  depth; any render that proves something about routing calls `Routes::advance_into` in its own
  sample loop, as `process` does. `tests::the_wheel_fades_the_modules_vibrato_into_rendered_audio`
  is the end-to-end check ([NOTES.md § A routing proof](NOTES.md#a-routing-proof-must-advance-the-amounts-not-just-the-topology)).
- **Gotcha: an empty `Mod bus` makes `Wheel bus` publish zero**, so the LFO on `Wheel bus` is silent.
  The working gesture is LFO → `Mod bus` → `Wheel bus` → target; nothing on the cards says so yet,
  an open question for B5 ([NOTES.md § An empty `Mod bus`](NOTES.md#an-empty-mod-bus-makes-the-multiplier-publish-zero-and-the-interface-does-not-say-so)).

# Verification

`target/bundled/` is a shared mutable profile slot. Before each collection resize command below,
stage **every name in the collection's `editor_resize` inventory (the workspace's
`collection-tests/editor_resize.rs`, local, not on GitHub)** in the stated profile; never mix
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
cargo test -p mxm-mono-pr1-host-tests --test golden_audio -- --nocapture   # digest pinned on Windows only
# then the workspace's collection-tests/editor_resize.rs: every product's editor, natively resized

# DEBUG PROFILE: stage every resize-inventory bundle without --release first.
cargo xtask bundle mxm-mono-pr1
clap-validator validate target/bundled/mxm-mono-pr1.clap
cargo test -p mxm-mono-pr1-host-tests --test behaviour editor_opens_closes_and_reopens_through_the_player -- --ignored --nocapture
cargo test -p mxm-mono-pr1-host-tests --test robustness -- --nocapture
# then the workspace's collection-tests/editor_resize.rs: every product's editor, natively resized

# RESTORE: stage every resize-inventory bundle with --release again; validate release last.
cargo xtask bundle mxm-mono-pr1 --release
clap-validator validate target/bundled/mxm-mono-pr1.clap

cargo clippy -p mxm-mono-pr1 --all-targets -- -D warnings
cargo fmt --package mxm-mono-pr1 -- --check
cargo test -p mxm-mono-pr1 --lib write_the_factory_presets -- --ignored # regenerates files
```

- Run `robustness` against debug (activates nice-plug's process-allocation assertion) and release.
- `golden_audio` regeneration is listen-before-update. The controller map also needs JSON/schema,
  role and parameter-id validation. Final reference listening is a manual release gate.
- What each suite covers and the recorded runs: [NOTES.md § Test coverage](NOTES.md#test-coverage-and-recorded-runs).

# Child DOX Index

No child AGENTS.md files. `presets/` and `src/` are covered here.
