//! The Pro-One's bounded press ledger and performance ownership.
//!
//! NORMAL is low-note priority and single-trigger: only opening an empty held set triggers.
//! RETRIG is last-note priority and every new press triggers. Releases can hand the voice to a
//! fallback press but never trigger. Presses, rather than pitches, are stored so overlapping
//! same-pitch notes and host voice ids remain distinct.

use crate::finite_or;

pub const CAPACITY: usize = 16;
const CHANNELS: usize = 16;
const EXPRESSION_LIMIT_SEMITONES: f32 = 48.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyMode {
    #[default]
    Normal,
    Retrig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteId {
    pub voice_id: Option<i32>,
    pub channel: u8,
    pub key: u8,
    /// Note-on velocity, 0..=1. **Not part of the identity**: [`NoteId::matches`] ignores it, and a
    /// note-off's velocity never has to equal its note-on's for the press to be found.
    pub velocity: f32,
}

impl NoteId {
    pub fn matches(self, voice_id: Option<i32>, channel: u8, key: u8) -> bool {
        match (self.voice_id, voice_id) {
            (Some(a), Some(b)) => a == b,
            _ => self.channel == channel && self.key == key,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Press {
    pub id: NoteId,
    pub tuning_semitones: f32,
    order: u64,
}

impl NoteId {
    /// A press with no velocity information — the shape every call site used before velocity became
    /// a modulation source.
    pub const fn keyed(voice_id: Option<i32>, channel: u8, key: u8) -> Self {
        Self {
            voice_id,
            channel,
            key,
            velocity: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Owner {
    pub key: u8,
    pub channel: u8,
    pub tuning_semitones: f32,
    /// Bipolar channel bend position, `-1..=1`. The current patch range is applied when the
    /// control frame is built, not when the MIDI event arrives.
    pub bend_normalized: f32,
    pub wheel: f32,
    /// The sounding press's note-on velocity, `0..=1`. **Belongs to the press**, so it follows
    /// Normal/Retrig selection and release fallback exactly as pitch does.
    pub velocity: f32,
    /// Channel pressure, `0..=1`. **Retained per channel** like the wheel, so a note started while
    /// a key is already leaned on inherits the pressure rather than beginning at zero.
    pub pressure: f32,
}

impl Owner {
    pub fn pitch_semitones(self, bend_range_semitones: f32) -> f32 {
        self.key as f32
            + self.tuning_semitones
            + self.bend_normalized * finite_or(bend_range_semitones, 0.0).clamp(0.0, 48.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outcome {
    pub gate_opened: bool,
    pub gate_closed: bool,
    pub trigger: bool,
    pub sounding_changed: bool,
    /// The sounding identity changed while the held gate remained open. AUTO glide uses this.
    pub overlap_change: bool,
    pub cut: bool,
}

#[derive(Debug, Clone)]
pub struct Keyboard {
    presses: [Option<Press>; CAPACITY],
    len: usize,
    mode: KeyMode,
    next_order: u64,
    owner: Owner,
    default_owner: Owner,
    bend_positions: [f32; CHANNELS],
    wheels: [f32; CHANNELS],
    pressures: [f32; CHANNELS],
}

impl Keyboard {
    pub const fn new(default_key: u8, default_channel: u8) -> Self {
        let owner = Owner {
            key: default_key,
            channel: default_channel,
            tuning_semitones: 0.0,
            bend_normalized: 0.0,
            wheel: 0.0,
            // Full until a press sets it, so nothing that reads an owner no press has set can
            // publish the standard Velocity's `−1`.
            velocity: 1.0,
            pressure: 0.0,
        };
        Self {
            presses: [None; CAPACITY],
            len: 0,
            mode: KeyMode::Normal,
            next_order: 0,
            owner,
            default_owner: owner,
            bend_positions: [0.0; CHANNELS],
            wheels: [0.0; CHANNELS],
            pressures: [0.0; CHANNELS],
        }
    }

    pub fn reset(&mut self) {
        let default = self.default_owner;
        *self = Self::new(default.key, default.channel);
    }

    pub fn mode(&self) -> KeyMode {
        self.mode
    }

    pub fn held(&self) -> usize {
        self.len
    }

    pub fn is_held(&self) -> bool {
        self.len != 0
    }

    pub fn sounding(&self) -> Option<Press> {
        self.sounding_index().map(|i| self.presses[i].unwrap())
    }

    pub fn owner(&self) -> Owner {
        self.owner
    }

    fn channel_index(channel: u8) -> usize {
        usize::from(channel.min(15))
    }

    fn sounding_index_for(&self, mode: KeyMode) -> Option<usize> {
        match mode {
            KeyMode::Normal => {
                let mut best: Option<(usize, u8, u64)> = None;
                for (index, press) in self.presses[..self.len].iter().enumerate() {
                    let press = press.unwrap();
                    if best.is_none_or(|(_, key, order)| {
                        press.id.key < key || (press.id.key == key && press.order < order)
                    }) {
                        best = Some((index, press.id.key, press.order));
                    }
                }
                best.map(|(index, _, _)| index)
            }
            KeyMode::Retrig => (0..self.len).max_by_key(|&i| self.presses[i].unwrap().order),
        }
    }

    fn sounding_index(&self) -> Option<usize> {
        self.sounding_index_for(self.mode)
    }

    fn set_owner_from(&mut self, index: usize) {
        let press = self.presses[index].unwrap();
        let channel = Self::channel_index(press.id.channel);
        self.owner = Owner {
            key: press.id.key,
            channel: press.id.channel,
            tuning_semitones: press.tuning_semitones,
            bend_normalized: self.bend_positions[channel],
            wheel: self.wheels[channel],
            velocity: press.id.velocity,
            pressure: self.pressures[channel],
        };
    }

    fn remove_at(&mut self, index: usize) {
        for i in index..self.len - 1 {
            self.presses[i] = self.presses[i + 1];
        }
        self.len -= 1;
        self.presses[self.len] = None;
    }

    fn find(&self, voice_id: Option<i32>, channel: u8, key: u8) -> Option<usize> {
        (0..self.len).find(|&i| self.presses[i].unwrap().id.matches(voice_id, channel, key))
    }

    /// Change the hardware key mode without inventing a trigger. If held keys select a different
    /// press, the key bus changes and AUTO glide treats it as an overlapped change.
    pub fn set_mode(&mut self, mode: KeyMode) -> Outcome {
        if mode == self.mode {
            return Outcome::default();
        }
        let before_order = self.sounding().map(|p| p.order);
        self.mode = mode;
        let after = self.sounding_index();
        let changed = after.map(|i| self.presses[i].unwrap().order) != before_order;
        if changed && let Some(index) = after {
            self.set_owner_from(index);
        }
        Outcome {
            sounding_changed: changed,
            overlap_change: changed && self.is_held(),
            ..Outcome::default()
        }
    }

    pub fn note_on(&mut self, id: NoteId) -> Outcome {
        let was_held = self.is_held();
        let before_order = self.sounding().map(|p| p.order);

        if self.len == CAPACITY {
            let sounding = self.sounding_index().unwrap();
            let victim = (0..self.len)
                .filter(|&i| i != sounding)
                .min_by_key(|&i| self.presses[i].unwrap().order)
                .expect("capacity is greater than one");
            self.remove_at(victim);
        }

        let order = self.next_order;
        self.next_order = self.next_order.wrapping_add(1);
        self.presses[self.len] = Some(Press {
            id,
            tuning_semitones: 0.0,
            order,
        });
        self.len += 1;

        let after = self.sounding_index().unwrap();
        let changed = Some(self.presses[after].unwrap().order) != before_order;
        if changed {
            self.set_owner_from(after);
        }
        Outcome {
            gate_opened: !was_held,
            trigger: match self.mode {
                KeyMode::Normal => !was_held,
                KeyMode::Retrig => true,
            },
            sounding_changed: changed,
            overlap_change: was_held && changed,
            ..Outcome::default()
        }
    }

    pub fn note_off(&mut self, voice_id: Option<i32>, channel: u8, key: u8) -> Outcome {
        self.retire(voice_id, channel, key, false)
    }

    pub fn choke(&mut self, voice_id: Option<i32>, channel: u8, key: u8) -> Outcome {
        self.retire(voice_id, channel, key, true)
    }

    fn retire(&mut self, voice_id: Option<i32>, channel: u8, key: u8, choke: bool) -> Outcome {
        let Some(index) = self.find(voice_id, channel, key) else {
            return Outcome::default();
        };
        let before_order = self.sounding().map(|p| p.order);
        self.remove_at(index);
        let after = self.sounding_index();
        let changed = after.map(|i| self.presses[i].unwrap().order) != before_order;
        if changed && let Some(index) = after {
            self.set_owner_from(index);
        }
        Outcome {
            gate_closed: before_order.is_some() && after.is_none(),
            sounding_changed: changed && after.is_some(),
            overlap_change: changed && after.is_some(),
            cut: choke && before_order.is_some() && after.is_none(),
            ..Outcome::default()
        }
    }

    pub fn all_notes_off(&mut self) -> Outcome {
        let was_held = self.is_held();
        self.presses = [None; CAPACITY];
        self.len = 0;
        Outcome {
            gate_closed: was_held,
            ..Outcome::default()
        }
    }

    pub fn set_tuning(
        &mut self,
        voice_id: Option<i32>,
        channel: u8,
        key: u8,
        semitones: f32,
    ) -> bool {
        let sounding = self.sounding_index();
        let target = match sounding {
            Some(i) if self.presses[i].unwrap().id.matches(voice_id, channel, key) => Some(i),
            _ => self.find(voice_id, channel, key),
        };
        let Some(index) = target else {
            return false;
        };
        let value = finite_or(semitones, 0.0)
            .clamp(-EXPRESSION_LIMIT_SEMITONES, EXPRESSION_LIMIT_SEMITONES);
        self.presses[index].as_mut().unwrap().tuning_semitones = value;
        if sounding == Some(index) {
            self.owner.tuning_semitones = value;
        }
        true
    }

    pub fn set_channel_bend(&mut self, channel: u8, normalized: f32) {
        let index = Self::channel_index(channel);
        self.bend_positions[index] = finite_or(normalized, 0.0).clamp(-1.0, 1.0);
        if Self::channel_index(self.owner.channel) == index {
            self.owner.bend_normalized = self.bend_positions[index];
        }
    }

    pub fn set_channel_wheel(&mut self, channel: u8, value: f32) {
        let index = Self::channel_index(channel);
        self.wheels[index] = finite_or(value, 0.0).clamp(0.0, 1.0);
        if Self::channel_index(self.owner.channel) == index {
            self.owner.wheel = self.wheels[index];
        }
    }

    /// Channel pressure, retained whether or not a note is sounding.
    ///
    /// **Retention is the point, not an accident.** A message arriving with nothing held must not be
    /// discarded: a note started afterwards has to inherit it, or pressing a key while already
    /// leaning on the keyboard gives that note no pressure at all.
    ///
    /// It therefore behaves exactly as the wheel and the bender do, **All Sound Off included**:
    /// panic silences the voice, it does not decide where the player's hand is. Only `reset` returns
    /// a gesture to neutral, because only `reset` means *this instance has no history*.
    pub fn set_channel_pressure(&mut self, channel: u8, value: f32) {
        let index = Self::channel_index(channel);
        self.pressures[index] = finite_or(value, 0.0).clamp(0.0, 1.0);
        if Self::channel_index(self.owner.channel) == index {
            self.owner.pressure = self.pressures[index];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(key: u8) -> NoteId {
        NoteId::keyed(None, 0, key)
    }

    fn vid(voice_id: i32, channel: u8, key: u8) -> NoteId {
        NoteId::keyed(Some(voice_id), channel, key)
    }

    #[test]
    fn normal_is_low_note_priority_and_single_trigger() {
        let mut keyboard = Keyboard::new(60, 0);
        assert!(keyboard.note_on(id(60)).trigger);
        let low = keyboard.note_on(id(48));
        assert!(low.sounding_changed && low.overlap_change && !low.trigger);
        assert_eq!(keyboard.owner().key, 48);
        assert!(!keyboard.note_on(id(72)).trigger);
        assert_eq!(keyboard.owner().key, 48);
        let fallback = keyboard.note_off(None, 0, 48);
        assert!(fallback.sounding_changed && fallback.overlap_change && !fallback.trigger);
        assert_eq!(keyboard.owner().key, 60);
    }

    #[test]
    fn retrig_is_last_press_priority_and_every_press_triggers() {
        let mut keyboard = Keyboard::new(60, 0);
        keyboard.set_mode(KeyMode::Retrig);
        keyboard.note_on(vid(1, 0, 72));
        let second = keyboard.note_on(vid(2, 1, 48));
        assert!(second.trigger && second.sounding_changed && second.overlap_change);
        assert_eq!((keyboard.owner().key, keyboard.owner().channel), (48, 1));
        let same = keyboard.note_on(vid(3, 2, 48));
        assert!(same.trigger && same.sounding_changed);
        assert_eq!(keyboard.owner().channel, 2);
        let fallback = keyboard.note_off(Some(3), 2, 48);
        assert!(fallback.sounding_changed && !fallback.trigger);
        assert_eq!(keyboard.owner().channel, 1);
    }

    #[test]
    fn changing_mode_moves_the_bus_without_a_trigger() {
        let mut keyboard = Keyboard::new(60, 0);
        keyboard.note_on(vid(1, 0, 48));
        keyboard.note_on(vid(2, 0, 72));
        let outcome = keyboard.set_mode(KeyMode::Retrig);
        assert!(outcome.sounding_changed && outcome.overlap_change && !outcome.trigger);
        assert_eq!(keyboard.owner().key, 72);
        let outcome = keyboard.set_mode(KeyMode::Normal);
        assert!(outcome.sounding_changed && !outcome.trigger);
        assert_eq!(keyboard.owner().key, 48);
    }

    #[test]
    fn same_pitch_presses_and_authoritative_ids_survive_a_tie() {
        let mut keyboard = Keyboard::new(60, 0);
        keyboard.note_on(vid(1, 0, 60));
        keyboard.note_on(vid(2, 0, 60));
        assert_eq!(keyboard.held(), 2);
        assert_eq!(keyboard.sounding().unwrap().id.voice_id, Some(1));
        assert_eq!(keyboard.note_off(Some(9), 0, 60), Outcome::default());
        let outcome = keyboard.note_off(Some(1), 0, 60);
        assert!(outcome.sounding_changed && !outcome.gate_closed);
        assert_eq!(keyboard.sounding().unwrap().id.voice_id, Some(2));
    }

    #[test]
    fn performance_follows_the_sounding_channel_and_outlives_release() {
        let mut keyboard = Keyboard::new(60, 0);
        keyboard.set_channel_bend(3, 2.0);
        keyboard.set_channel_wheel(3, 0.75);
        keyboard.note_on(vid(1, 3, 64));
        keyboard.set_tuning(Some(1), 3, 64, -0.25);
        let owner = keyboard.owner();
        assert_eq!((owner.pitch_semitones(2.0), owner.wheel), (65.75, 0.75));
        keyboard.note_off(Some(1), 3, 64);
        assert_eq!(keyboard.owner(), owner);
        keyboard.set_channel_bend(3, -0.5);
        assert_eq!(keyboard.owner().pitch_semitones(2.0), 62.75);
        assert_eq!(
            keyboard.owner().pitch_semitones(12.0),
            57.75,
            "a held bend is re-scaled by the current patch range"
        );
        keyboard.reset();
        assert_eq!(keyboard.owner().pitch_semitones(12.0), 60.0);
    }

    #[test]
    fn exhaustion_never_evicts_the_sounding_press() {
        let mut keyboard = Keyboard::new(60, 0);
        keyboard.note_on(vid(0, 0, 24));
        for i in 1..CAPACITY as i32 + 5 {
            keyboard.note_on(vid(i, 0, 60 + (i % 12) as u8));
        }
        assert_eq!(keyboard.held(), CAPACITY);
        assert_eq!(keyboard.sounding().unwrap().id.voice_id, Some(0));
        let outcome = keyboard.note_off(Some(0), 0, 24);
        assert!(outcome.sounding_changed && !outcome.gate_closed);
    }

    #[test]
    fn all_notes_off_releases_but_choke_cuts_only_the_last_press() {
        let mut keyboard = Keyboard::new(60, 0);
        keyboard.note_on(id(48));
        keyboard.note_on(id(60));
        let outcome = keyboard.choke(None, 0, 48);
        assert!(outcome.sounding_changed && !outcome.cut);
        let outcome = keyboard.choke(None, 0, 60);
        assert!(outcome.gate_closed && outcome.cut);
        keyboard.note_on(id(64));
        let outcome = keyboard.all_notes_off();
        assert!(outcome.gate_closed && !outcome.cut);
    }
}
