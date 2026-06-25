//! Platform-independent trigger matcher. All detection logic lives here and is
//! exhaustively unit-tested; backends only feed normalized [`RawKeyEvent`]s.
//!
//! Design invariants (see plans/022 §6):
//! - Modifier state is tracked per *physical* modifier in [`Matcher::mods_down`];
//!   the matcher NEVER trusts an OS aggregate flag (Windows has none; the macOS
//!   aggregate cannot disambiguate sides).
//! - Level triggers (ModifierHold/Chord/SingleKey) are re-evaluated on every
//!   event and emit on state transitions.
//! - DoubleTap is a one-shot edge trigger (`Pressed` then `Released` same tick).
//! - Reset / set_bindings synthesize `Released` for active triggers BEFORE
//!   clearing state, so the host never leaves an action stuck "on".

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::types::{
    KeyPhase, KeySpec, ModSet, Modifier, NamedKey, RawKeyEvent, Side, TapKey, Trigger,
    TriggerEvent, TriggerId,
};

/// Maps a side-specific modifier [`KeySpec`] to its `(Modifier, Side)`.
/// Returns `None` for non-modifier keys.
fn modifier_of(key: KeySpec) -> Option<(Modifier, Side)> {
    match key {
        KeySpec::Named(NamedKey::AltLeft) => Some((Modifier::Alt, Side::Left)),
        KeySpec::Named(NamedKey::AltRight) => Some((Modifier::Alt, Side::Right)),
        KeySpec::Named(NamedKey::ControlLeft) => Some((Modifier::Control, Side::Left)),
        KeySpec::Named(NamedKey::ControlRight) => Some((Modifier::Control, Side::Right)),
        KeySpec::Named(NamedKey::MetaLeft) => Some((Modifier::Meta, Side::Left)),
        KeySpec::Named(NamedKey::MetaRight) => Some((Modifier::Meta, Side::Right)),
        KeySpec::Named(NamedKey::ShiftLeft) => Some((Modifier::Shift, Side::Left)),
        KeySpec::Named(NamedKey::ShiftRight) => Some((Modifier::Shift, Side::Right)),
        _ => None,
    }
}

/// True if a binding's [`TapKey`] matches a concrete gesture (`Either` matches
/// either physical side; concrete gestures always carry a concrete side).
fn tapkey_matches(binding: TapKey, concrete: TapKey) -> bool {
    match (binding, concrete) {
        (TapKey::Key(a), TapKey::Key(b)) => a == b,
        (TapKey::Mod(m1, s1), TapKey::Mod(m2, s2)) => m1 == m2 && (s1 == Side::Either || s1 == s2),
        _ => false,
    }
}

#[derive(Default)]
pub struct Matcher {
    bindings: Vec<(TriggerId, Trigger)>,
    /// Non-modifier physical keys currently down.
    keys_down: HashSet<KeySpec>,
    /// Physical modifier keys currently down (side-specific).
    mods_down: HashSet<(Modifier, Side)>,
    /// Per concrete gesture: (time of last non-repeat down, whether a release
    /// has been seen since that down).
    last_tap: HashMap<TapKey, (Instant, bool)>,
    /// Currently-active level triggers (so `Released` is emitted exactly once).
    active: HashMap<TriggerId, bool>,
    /// Per concrete gesture currently down: (down time, still-isolated flag).
    /// Drives [`Trigger::IsolatedTap`]: an entry stays isolated only while no
    /// OTHER key is pressed during its hold.
    isolated: HashMap<TapKey, (Instant, bool)>,
}

impl Matcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process one normalized key event.
    pub fn handle(&mut self, ev: RawKeyEvent, now: Instant, emit: &mut dyn FnMut(TriggerEvent)) {
        let modifier = modifier_of(ev.key);
        let gesture = match modifier {
            Some((m, s)) => TapKey::Mod(m, s),
            None => TapKey::Key(ev.key),
        };

        if ev.down {
            let already = match modifier {
                Some(ms) => self.mods_down.contains(&ms),
                None => self.keys_down.contains(&ev.key),
            };
            let repeat = ev.is_repeat || already;

            match modifier {
                Some(ms) => {
                    self.mods_down.insert(ms);
                }
                None => {
                    self.keys_down.insert(ev.key);
                }
            }

            if !repeat {
                self.handle_double_tap(gesture, now, emit);
                self.handle_isolated_down(gesture, now);
                self.handle_combo_exact_down(ev, emit);
            }
        } else {
            match modifier {
                Some(ms) => {
                    self.mods_down.remove(&ms);
                }
                None => {
                    self.keys_down.remove(&ev.key);
                }
            }
            if let Some(entry) = self.last_tap.get_mut(&gesture) {
                entry.1 = true; // saw_up
            }
            self.handle_isolated_up(gesture, now, emit);
        }

        self.recompute_levels(emit);
        self.maintain_combo_exact(emit);
    }

    /// Double-tap edge detection for a concrete `gesture` (a non-repeat down).
    fn handle_double_tap(
        &mut self,
        gesture: TapKey,
        now: Instant,
        emit: &mut dyn FnMut(TriggerEvent),
    ) {
        let mut any_fire = false;

        let bindings = std::mem::take(&mut self.bindings);
        for (id, trig) in &bindings {
            if let Trigger::DoubleTap { key, within } = trig {
                if !tapkey_matches(*key, gesture) {
                    continue;
                }
                // For an `Either` modifier double-tap a prior tap on *either*
                // side counts (Left-Cmd then Right-Cmd is valid); otherwise only
                // the exact concrete gesture's record qualifies.
                if let Some((t, true)) = self.prev_tap_for(*key, gesture) {
                    if now.duration_since(t) <= *within {
                        emit(TriggerEvent {
                            id: id.clone(),
                            phase: KeyPhase::Pressed,
                        });
                        emit(TriggerEvent {
                            id: id.clone(),
                            phase: KeyPhase::Released,
                        });
                        any_fire = true;
                    }
                }
            }
        }
        self.bindings = bindings;

        if any_fire {
            // Consume so a third tap does not double-fire.
            self.consume_tap(gesture);
        } else {
            self.last_tap.insert(gesture, (now, false));
        }
    }

    /// The qualifying previous-tap record for `binding_key` given the current
    /// concrete `gesture`. `Either` modifier bindings consider both sides.
    fn prev_tap_for(&self, binding_key: TapKey, gesture: TapKey) -> Option<(Instant, bool)> {
        match binding_key {
            TapKey::Mod(m, Side::Either) => {
                let left = self.last_tap.get(&TapKey::Mod(m, Side::Left)).copied();
                let right = self.last_tap.get(&TapKey::Mod(m, Side::Right)).copied();
                match (left, right) {
                    (Some(l), Some(r)) => Some(if l.0 >= r.0 { l } else { r }),
                    (Some(l), None) => Some(l),
                    (None, Some(r)) => Some(r),
                    (None, None) => None,
                }
            }
            _ => self.last_tap.get(&gesture).copied(),
        }
    }

    /// Clear tap records that could re-fire after a double-tap. For a modifier
    /// gesture both sides are cleared (an `Either` binding may match cross-side).
    fn consume_tap(&mut self, gesture: TapKey) {
        match gesture {
            TapKey::Mod(m, _) => {
                self.last_tap.remove(&TapKey::Mod(m, Side::Left));
                self.last_tap.remove(&TapKey::Mod(m, Side::Right));
            }
            other => {
                self.last_tap.remove(&other);
            }
        }
    }

    /// Track the isolated-tap candidate for a non-repeat `gesture` down. Any
    /// other key going down marks every *other* held candidate non-isolated; the
    /// new candidate is isolated only if nothing else is currently down.
    fn handle_isolated_down(&mut self, gesture: TapKey, now: Instant) {
        for (k, (_, clean)) in self.isolated.iter_mut() {
            if *k != gesture {
                *clean = false;
            }
        }
        let others_down = self.mods_down.len() + self.keys_down.len() > 1;
        self.isolated.insert(gesture, (now, !others_down));
    }

    /// One-shot isolated-tap edge detection on `gesture` up: fire when the key
    /// was tapped alone (no other key during its hold) within `within`.
    fn handle_isolated_up(
        &mut self,
        gesture: TapKey,
        now: Instant,
        emit: &mut dyn FnMut(TriggerEvent),
    ) {
        let Some((down_time, clean)) = self.isolated.remove(&gesture) else {
            return;
        };
        if !clean {
            return;
        }
        let elapsed = now.duration_since(down_time);
        let bindings = std::mem::take(&mut self.bindings);
        for (id, trig) in &bindings {
            if let Trigger::IsolatedTap { key, within } = trig {
                if tapkey_matches(*key, gesture) && elapsed <= *within {
                    emit(TriggerEvent {
                        id: id.clone(),
                        phase: KeyPhase::Pressed,
                    });
                    emit(TriggerEvent {
                        id: id.clone(),
                        phase: KeyPhase::Released,
                    });
                }
            }
        }
        self.bindings = bindings;
    }

    /// Re-evaluate all level triggers against current key/modifier state.
    fn recompute_levels(&mut self, emit: &mut dyn FnMut(TriggerEvent)) {
        let modset = self.current_modset();
        let bindings = std::mem::take(&mut self.bindings);
        for (id, trig) in &bindings {
            let now_active = match trig {
                Trigger::ModifierHold { modifier, side } => self
                    .mods_down
                    .iter()
                    .any(|(m, s)| m == modifier && (*side == Side::Either || s == side)),
                Trigger::Chord { mods, key } => {
                    self.keys_down.contains(key) && mods.is_subset_of(&modset)
                }
                Trigger::SingleKey { key } => {
                    self.keys_down.contains(key) && self.mods_down.is_empty()
                }
                Trigger::DoubleTap { .. } => continue, // edge trigger, handled elsewhere
                Trigger::IsolatedTap { .. } => continue,
                Trigger::ComboExact { .. } => continue, // edge-activated; see handle_combo_exact_down / maintain_combo_exact
            };
            let was = self.active.get(id).copied().unwrap_or(false);
            if now_active && !was {
                self.active.insert(id.clone(), true);
                emit(TriggerEvent {
                    id: id.clone(),
                    phase: KeyPhase::Pressed,
                });
            } else if !now_active && was {
                self.active.insert(id.clone(), false);
                emit(TriggerEvent {
                    id: id.clone(),
                    phase: KeyPhase::Released,
                });
            }
        }
        self.bindings = bindings;
    }

    /// Edge activation for [`Trigger::ComboExact`]: on a NON-REPEAT down of a
    /// non-modifier `key`, fire `Pressed` iff the currently-held modifier set
    /// EQUALS the binding's `mods` exactly. A superset (or subset) of modifiers
    /// does NOT activate, and a key held before the modifiers arrive never
    /// activates — activation is strictly the key-down edge with exact mods
    /// already held. This is what separates `ComboExact` from the subset-based
    /// [`Trigger::Chord`] evaluated in [`Matcher::recompute_levels`].
    fn handle_combo_exact_down(&mut self, ev: RawKeyEvent, emit: &mut dyn FnMut(TriggerEvent)) {
        // Only a non-repeat, non-modifier key-down can start a combo; modifier
        // presses arrive as their own events and never feed this path.
        if ev.is_repeat || modifier_of(ev.key).is_some() {
            return;
        }
        let modset = self.current_modset();
        let bindings = std::mem::take(&mut self.bindings);
        for (id, trig) in &bindings {
            if let Trigger::ComboExact { mods, key } = trig {
                if *key == ev.key && *mods == modset {
                    self.active.insert(id.clone(), true);
                    emit(TriggerEvent {
                        id: id.clone(),
                        phase: KeyPhase::Pressed,
                    });
                }
            }
        }
        self.bindings = bindings;
    }

    /// Release any active [`Trigger::ComboExact`] binding whose hold condition
    /// no longer holds: the bound `key` is no longer down, or the held modifier
    /// set drifted away from the exact `mods`. Runs on every event so PTT-style
    /// releases (lifting a required modifier while the key stays down) fire
    /// promptly. Bindings that never activated are skipped (nothing to release).
    fn maintain_combo_exact(&mut self, emit: &mut dyn FnMut(TriggerEvent)) {
        let modset = self.current_modset();
        let bindings = std::mem::take(&mut self.bindings);
        for (id, trig) in &bindings {
            if let Trigger::ComboExact { mods, key } = trig {
                let active = self.active.get(id).copied().unwrap_or(false);
                if active {
                    let holds = self.keys_down.contains(key) && *mods == modset;
                    if !holds {
                        self.active.insert(id.clone(), false);
                        emit(TriggerEvent {
                            id: id.clone(),
                            phase: KeyPhase::Released,
                        });
                    }
                }
            }
        }
        self.bindings = bindings;
    }

    fn current_modset(&self) -> ModSet {
        let mut s = ModSet::empty();
        for (m, _side) in &self.mods_down {
            s.insert(*m);
        }
        s
    }

    /// Replace the binding set. Emits synthetic `Released` for any active binding
    /// that is removed or whose trigger changed, BEFORE swapping (so the host's
    /// per-binding state clears). New/unchanged bindings are not auto-fired; they
    /// take effect on the next key event.
    pub fn set_bindings(
        &mut self,
        new: Vec<(TriggerId, Trigger)>,
        emit: &mut dyn FnMut(TriggerEvent),
    ) {
        let old = std::mem::take(&mut self.bindings);
        for (id, trig) in &old {
            let unchanged = new.iter().any(|(nid, ntrig)| nid == id && ntrig == trig);
            if !unchanged {
                if self.active.get(id).copied().unwrap_or(false) {
                    emit(TriggerEvent {
                        id: id.clone(),
                        phase: KeyPhase::Released,
                    });
                }
                self.active.remove(id);
            }
        }
        self.bindings = new;
    }

    /// Reset all transient state, emitting synthetic `Released` for every active
    /// trigger first (the stuck-mic guard). Call on (re)start, tap re-enable, and
    /// session transitions where the event stream may have gaps.
    pub fn reset(&mut self, emit: &mut dyn FnMut(TriggerEvent)) {
        // Stable order is not required; release every active trigger.
        let active_ids: Vec<TriggerId> = self
            .active
            .iter()
            .filter(|(_, &v)| v)
            .map(|(id, _)| id.clone())
            .collect();
        for id in active_ids {
            emit(TriggerEvent {
                id,
                phase: KeyPhase::Released,
            });
        }
        self.keys_down.clear();
        self.mods_down.clear();
        self.last_tap.clear();
        self.isolated.clear();
        self.active.clear();
    }

    /// Test/inspection helper: number of currently-active level triggers.
    pub fn active_count(&self) -> usize {
        self.active.values().filter(|&&v| v).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ev(key: NamedKey, side: Option<Side>, down: bool, is_repeat: bool) -> RawKeyEvent {
        RawKeyEvent {
            key: KeySpec::Named(key),
            side,
            down,
            is_repeat,
        }
    }
    fn kdown(k: NamedKey) -> RawKeyEvent {
        ev(k, None, true, false)
    }
    fn kup(k: NamedKey) -> RawKeyEvent {
        ev(k, None, false, false)
    }
    fn mdown(k: NamedKey, s: Side) -> RawKeyEvent {
        ev(k, Some(s), true, false)
    }
    fn mup(k: NamedKey, s: Side) -> RawKeyEvent {
        ev(k, Some(s), false, false)
    }

    fn drive(m: &mut Matcher, events: &[(RawKeyEvent, Instant)]) -> Vec<TriggerEvent> {
        let mut out = Vec::new();
        for (e, t) in events {
            m.handle(*e, *t, &mut |te| out.push(te));
        }
        out
    }

    fn press(id: &str) -> TriggerEvent {
        TriggerEvent {
            id: id.to_string(),
            phase: KeyPhase::Pressed,
        }
    }
    fn release(id: &str) -> TriggerEvent {
        TriggerEvent {
            id: id.to_string(),
            phase: KeyPhase::Released,
        }
    }

    fn with_bindings(b: Vec<(TriggerId, Trigger)>) -> Matcher {
        let mut m = Matcher::new();
        m.set_bindings(b, &mut |_| {});
        m
    }

    #[test]
    fn modifier_hold_right_fires_press_and_release() {
        let mut m = with_bindings(vec![(
            "hold".into(),
            Trigger::ModifierHold {
                modifier: Modifier::Alt,
                side: Side::Right,
            },
        )]);
        let t = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::AltRight, Side::Right), t),
                (mup(NamedKey::AltRight, Side::Right), t),
            ],
        );
        assert_eq!(out, vec![press("hold"), release("hold")]);
        assert_eq!(m.active_count(), 0);
    }

    #[test]
    fn modifier_hold_left_vs_right_distinct() {
        let mut m = with_bindings(vec![(
            "right".into(),
            Trigger::ModifierHold {
                modifier: Modifier::Alt,
                side: Side::Right,
            },
        )]);
        let t = Instant::now();
        // Left Option should NOT fire a Right-Option hold.
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::AltLeft, Side::Left), t),
                (mup(NamedKey::AltLeft, Side::Left), t),
            ],
        );
        assert!(out.is_empty());
    }

    #[test]
    fn modifier_hold_either_matches_any_side() {
        let mut m = with_bindings(vec![(
            "any".into(),
            Trigger::ModifierHold {
                modifier: Modifier::Meta,
                side: Side::Either,
            },
        )]);
        let t = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::MetaLeft, Side::Left), t),
                (mup(NamedKey::MetaLeft, Side::Left), t),
            ],
        );
        assert_eq!(out, vec![press("any"), release("any")]);
    }

    #[test]
    fn chord_fires_and_releases_on_key_up() {
        let mut m = with_bindings(vec![(
            "chord".into(),
            Trigger::Chord {
                mods: ModSet::empty().with(Modifier::Meta).with(Modifier::Shift),
                key: KeySpec::Named(NamedKey::Space),
            },
        )]);
        let t = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::MetaLeft, Side::Left), t),
                (mdown(NamedKey::ShiftLeft, Side::Left), t),
                (kdown(NamedKey::Space), t), // all held -> Pressed
                (kup(NamedKey::Space), t),   // key up -> Released
                (mup(NamedKey::ShiftLeft, Side::Left), t),
                (mup(NamedKey::MetaLeft, Side::Left), t),
            ],
        );
        assert_eq!(out, vec![press("chord"), release("chord")]);
    }

    #[test]
    fn chord_releases_when_required_modifier_drops_while_key_held() {
        let mut m = with_bindings(vec![(
            "chord".into(),
            Trigger::Chord {
                mods: ModSet::empty().with(Modifier::Meta),
                key: KeySpec::Named(NamedKey::Space),
            },
        )]);
        let t = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::MetaLeft, Side::Left), t),
                (kdown(NamedKey::Space), t),              // Pressed
                (mup(NamedKey::MetaLeft, Side::Left), t), // modifier drops -> Released even though key still down
            ],
        );
        assert_eq!(out, vec![press("chord"), release("chord")]);
    }

    #[test]
    fn chord_does_not_fire_without_all_modifiers() {
        let mut m = with_bindings(vec![(
            "chord".into(),
            Trigger::Chord {
                mods: ModSet::empty().with(Modifier::Meta).with(Modifier::Shift),
                key: KeySpec::Named(NamedKey::Space),
            },
        )]);
        let t = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::MetaLeft, Side::Left), t), // only Meta, no Shift
                (kdown(NamedKey::Space), t),
                (kup(NamedKey::Space), t),
            ],
        );
        assert!(out.is_empty());
    }

    #[test]
    fn single_key_bare_fires() {
        let mut m = with_bindings(vec![(
            "f8".into(),
            Trigger::SingleKey {
                key: KeySpec::Named(NamedKey::F8),
            },
        )]);
        let t = Instant::now();
        let out = drive(&mut m, &[(kdown(NamedKey::F8), t), (kup(NamedKey::F8), t)]);
        assert_eq!(out, vec![press("f8"), release("f8")]);
    }

    #[test]
    fn single_key_with_modifier_held_does_not_fire() {
        let mut m = with_bindings(vec![(
            "f8".into(),
            Trigger::SingleKey {
                key: KeySpec::Named(NamedKey::F8),
            },
        )]);
        let t = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::ShiftLeft, Side::Left), t),
                (kdown(NamedKey::F8), t), // Shift held -> must NOT fire
                (kup(NamedKey::F8), t),
                (mup(NamedKey::ShiftLeft, Side::Left), t),
            ],
        );
        assert!(out.is_empty());
    }

    #[test]
    fn double_tap_within_window_fires_once() {
        let mut m = with_bindings(vec![(
            "dt".into(),
            Trigger::DoubleTap {
                key: TapKey::Key(KeySpec::Named(NamedKey::Space)),
                within: Duration::from_millis(300),
            },
        )]);
        let base = Instant::now();
        let out = drive(
            &mut m,
            &[
                (kdown(NamedKey::Space), base),
                (kup(NamedKey::Space), base + Duration::from_millis(50)),
                (kdown(NamedKey::Space), base + Duration::from_millis(150)),
                (kup(NamedKey::Space), base + Duration::from_millis(200)),
            ],
        );
        assert_eq!(out, vec![press("dt"), release("dt")]);
    }

    #[test]
    fn double_tap_outside_window_does_not_fire() {
        let mut m = with_bindings(vec![(
            "dt".into(),
            Trigger::DoubleTap {
                key: TapKey::Key(KeySpec::Named(NamedKey::Space)),
                within: Duration::from_millis(300),
            },
        )]);
        let base = Instant::now();
        let out = drive(
            &mut m,
            &[
                (kdown(NamedKey::Space), base),
                (kup(NamedKey::Space), base + Duration::from_millis(50)),
                (kdown(NamedKey::Space), base + Duration::from_millis(500)), // too late
                (kup(NamedKey::Space), base + Duration::from_millis(550)),
            ],
        );
        assert!(out.is_empty());
    }

    #[test]
    fn double_tap_autorepeat_does_not_fire() {
        let mut m = with_bindings(vec![(
            "dt".into(),
            Trigger::DoubleTap {
                key: TapKey::Key(KeySpec::Named(NamedKey::Space)),
                within: Duration::from_millis(300),
            },
        )]);
        let base = Instant::now();
        // down, then autorepeat down (no intervening up) -> not a double-tap.
        let repeat = RawKeyEvent {
            key: KeySpec::Named(NamedKey::Space),
            side: None,
            down: true,
            is_repeat: true,
        };
        let out = drive(
            &mut m,
            &[
                (kdown(NamedKey::Space), base),
                (repeat, base + Duration::from_millis(40)),
                (repeat, base + Duration::from_millis(80)),
            ],
        );
        assert!(out.is_empty());
    }

    #[test]
    fn double_tap_modifier_meta_right() {
        let mut m = with_bindings(vec![(
            "dtm".into(),
            Trigger::DoubleTap {
                key: TapKey::Mod(Modifier::Meta, Side::Right),
                within: Duration::from_millis(300),
            },
        )]);
        let base = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::MetaRight, Side::Right), base),
                (
                    mup(NamedKey::MetaRight, Side::Right),
                    base + Duration::from_millis(50),
                ),
                (
                    mdown(NamedKey::MetaRight, Side::Right),
                    base + Duration::from_millis(150),
                ),
                (
                    mup(NamedKey::MetaRight, Side::Right),
                    base + Duration::from_millis(200),
                ),
            ],
        );
        assert_eq!(out, vec![press("dtm"), release("dtm")]);
    }

    #[test]
    fn double_tap_either_fires_cross_side() {
        // Left-Cmd then Right-Cmd must count as a double-tap for `Either`.
        let mut m = with_bindings(vec![(
            "dte".into(),
            Trigger::DoubleTap {
                key: TapKey::Mod(Modifier::Meta, Side::Either),
                within: Duration::from_millis(300),
            },
        )]);
        let base = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::MetaLeft, Side::Left), base),
                (
                    mup(NamedKey::MetaLeft, Side::Left),
                    base + Duration::from_millis(50),
                ),
                (
                    mdown(NamedKey::MetaRight, Side::Right),
                    base + Duration::from_millis(150),
                ),
                (
                    mup(NamedKey::MetaRight, Side::Right),
                    base + Duration::from_millis(200),
                ),
            ],
        );
        assert_eq!(out, vec![press("dte"), release("dte")]);
    }

    #[test]
    fn double_tap_concrete_side_ignores_cross_side() {
        // A Right-only double-tap must NOT fire on Left-then-Right.
        let mut m = with_bindings(vec![(
            "dtr".into(),
            Trigger::DoubleTap {
                key: TapKey::Mod(Modifier::Meta, Side::Right),
                within: Duration::from_millis(300),
            },
        )]);
        let base = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::MetaLeft, Side::Left), base),
                (
                    mup(NamedKey::MetaLeft, Side::Left),
                    base + Duration::from_millis(50),
                ),
                (
                    mdown(NamedKey::MetaRight, Side::Right),
                    base + Duration::from_millis(150),
                ),
                (
                    mup(NamedKey::MetaRight, Side::Right),
                    base + Duration::from_millis(200),
                ),
            ],
        );
        assert!(out.is_empty());
    }

    #[test]
    fn set_bindings_releases_removed_active_binding() {
        let mut m = with_bindings(vec![(
            "hold".into(),
            Trigger::ModifierHold {
                modifier: Modifier::Alt,
                side: Side::Right,
            },
        )]);
        let t = Instant::now();
        // Activate the hold.
        let mut out = Vec::new();
        m.handle(mdown(NamedKey::AltRight, Side::Right), t, &mut |e| {
            out.push(e)
        });
        assert_eq!(out, vec![press("hold")]);
        // Remove the binding while active -> synthetic Released.
        out.clear();
        m.set_bindings(vec![], &mut |e| out.push(e));
        assert_eq!(out, vec![release("hold")]);
        assert_eq!(m.active_count(), 0);
    }

    #[test]
    fn reset_releases_all_active_no_stuck_triggers() {
        let mut m = with_bindings(vec![
            (
                "hold".into(),
                Trigger::ModifierHold {
                    modifier: Modifier::Alt,
                    side: Side::Right,
                },
            ),
            (
                "single".into(),
                Trigger::SingleKey {
                    key: KeySpec::Named(NamedKey::F8),
                },
            ),
        ]);
        let t = Instant::now();
        let mut out = Vec::new();
        m.handle(mdown(NamedKey::AltRight, Side::Right), t, &mut |e| {
            out.push(e)
        });
        // F8 alone won't fire while Alt is held (SingleKey is bare-only); use a
        // separate matcher state: release Alt first so F8 can be the active one.
        // Here we just assert the hold is active, then reset releases it.
        assert_eq!(out, vec![press("hold")]);
        out.clear();
        m.reset(&mut |e| out.push(e));
        assert_eq!(out, vec![release("hold")]);
        assert_eq!(m.active_count(), 0);
        assert!(m.mods_down.is_empty() && m.keys_down.is_empty());
    }

    #[test]
    fn overlapping_triggers_on_same_key_both_fire() {
        let mut m = with_bindings(vec![
            (
                "a".into(),
                Trigger::SingleKey {
                    key: KeySpec::Named(NamedKey::F8),
                },
            ),
            (
                "b".into(),
                Trigger::SingleKey {
                    key: KeySpec::Named(NamedKey::F8),
                },
            ),
        ]);
        let t = Instant::now();
        let out = drive(&mut m, &[(kdown(NamedKey::F8), t)]);
        // Both bindings fire Pressed (order follows binding order).
        assert!(out.contains(&press("a")));
        assert!(out.contains(&press("b")));
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn isolated_tap_alone_fires() {
        let mut m = with_bindings(vec![(
            "tap".into(),
            Trigger::IsolatedTap {
                key: TapKey::Mod(Modifier::Control, Side::Either),
                within: Duration::from_millis(350),
            },
        )]);
        let base = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::ControlLeft, Side::Left), base),
                (
                    mup(NamedKey::ControlLeft, Side::Left),
                    base + Duration::from_millis(80),
                ),
            ],
        );
        assert_eq!(out, vec![press("tap"), release("tap")]);
    }

    #[test]
    fn isolated_tap_with_other_key_does_not_fire() {
        // Control held while another key is pressed = normal modifier use.
        let mut m = with_bindings(vec![(
            "tap".into(),
            Trigger::IsolatedTap {
                key: TapKey::Mod(Modifier::Control, Side::Either),
                within: Duration::from_millis(350),
            },
        )]);
        let base = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::ControlLeft, Side::Left), base),
                (kdown(NamedKey::C), base + Duration::from_millis(30)),
                (kup(NamedKey::C), base + Duration::from_millis(60)),
                (
                    mup(NamedKey::ControlLeft, Side::Left),
                    base + Duration::from_millis(90),
                ),
            ],
        );
        assert!(out.is_empty());
    }

    #[test]
    fn isolated_tap_too_slow_does_not_fire() {
        let mut m = with_bindings(vec![(
            "tap".into(),
            Trigger::IsolatedTap {
                key: TapKey::Mod(Modifier::Control, Side::Either),
                within: Duration::from_millis(350),
            },
        )]);
        let base = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::ControlLeft, Side::Left), base),
                (
                    mup(NamedKey::ControlLeft, Side::Left),
                    base + Duration::from_millis(500),
                ),
            ],
        );
        assert!(out.is_empty());
    }

    #[test]
    fn isolated_tap_pressed_while_other_held_does_not_fire() {
        // Another key already down when the modifier is tapped = not isolated.
        let mut m = with_bindings(vec![(
            "tap".into(),
            Trigger::IsolatedTap {
                key: TapKey::Mod(Modifier::Control, Side::Either),
                within: Duration::from_millis(350),
            },
        )]);
        let base = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::ShiftLeft, Side::Left), base),
                (
                    mdown(NamedKey::ControlLeft, Side::Left),
                    base + Duration::from_millis(20),
                ),
                (
                    mup(NamedKey::ControlLeft, Side::Left),
                    base + Duration::from_millis(60),
                ),
                (
                    mup(NamedKey::ShiftLeft, Side::Left),
                    base + Duration::from_millis(90),
                ),
            ],
        );
        assert!(out.is_empty());
    }

    #[test]
    fn isolated_tap_twice_fires_each_time() {
        // Each clean tap toggles, so it fires every time (unlike double-tap).
        let mut m = with_bindings(vec![(
            "tap".into(),
            Trigger::IsolatedTap {
                key: TapKey::Mod(Modifier::Control, Side::Either),
                within: Duration::from_millis(350),
            },
        )]);
        let base = Instant::now();
        let out = drive(
            &mut m,
            &[
                (mdown(NamedKey::ControlLeft, Side::Left), base),
                (
                    mup(NamedKey::ControlLeft, Side::Left),
                    base + Duration::from_millis(40),
                ),
                (
                    mdown(NamedKey::ControlLeft, Side::Left),
                    base + Duration::from_millis(400),
                ),
                (
                    mup(NamedKey::ControlLeft, Side::Left),
                    base + Duration::from_millis(440),
                ),
            ],
        );
        assert_eq!(
            out,
            vec![press("tap"), release("tap"), press("tap"), release("tap")]
        );
    }

    #[test]
    fn isolated_tap_with_normal_key_already_held_does_not_fire() {
        // A normal (non-modifier) key already down when the modifier is tapped
        // = not isolated. This is the anti-misfire guarantee: a bound modifier
        // tapped mid-shortcut must never toggle recording.
        let mut m = with_bindings(vec![(
            "tap".into(),
            Trigger::IsolatedTap {
                key: TapKey::Mod(Modifier::Control, Side::Either),
                within: Duration::from_millis(350),
            },
        )]);
        let base = Instant::now();
        let out = drive(
            &mut m,
            &[
                (kdown(NamedKey::C), base),
                (
                    mdown(NamedKey::ControlLeft, Side::Left),
                    base + Duration::from_millis(20),
                ),
                (
                    mup(NamedKey::ControlLeft, Side::Left),
                    base + Duration::from_millis(60),
                ),
                (kup(NamedKey::C), base + Duration::from_millis(90)),
            ],
        );
        assert!(out.is_empty());
    }
}
