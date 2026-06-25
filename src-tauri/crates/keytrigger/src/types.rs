//! Public vocabulary for the trigger engine. No `serde`/`tauri` dependency:
//! mapping to/from app-level binding types happens in the host application.

use std::time::Duration;

/// A logical modifier. Side is tracked separately (see [`Side`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    /// `Option` on macOS / `Alt` elsewhere.
    Alt,
    Control,
    /// `Command` on macOS / `Windows` key elsewhere.
    Meta,
    Shift,
}

/// Which physical instance of a modifier, or either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
    /// Matches either physical side. Concrete events never carry `Either`.
    Either,
}

/// A named physical key (layout-independent). Unknown keys fall back to
/// [`KeySpec::Raw`]. Modifier keys are represented side-specifically so a
/// [`KeySpec`] can describe a physical modifier when needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedKey {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    Space,
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadEnter,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
    // Punctuation / OEM keys (US ANSI layout names).
    Comma,
    Period,
    Semicolon,
    Quote,
    BracketLeft,
    BracketRight,
    Backslash,
    Slash,
    Equal,
    Minus,
    Backquote,
    // Modifier keys as named physical keys.
    AltLeft,
    AltRight,
    ControlLeft,
    ControlRight,
    MetaLeft,
    MetaRight,
    ShiftLeft,
    ShiftRight,
    CapsLock,
    Fn,
}

/// A physical key: a known [`NamedKey`] or a raw platform keycode fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeySpec {
    Named(NamedKey),
    Raw(u32),
}

/// A small `Copy` set of [`Modifier`]s (side-agnostic), backed by a bitset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModSet(u8);

impl ModSet {
    pub const fn empty() -> Self {
        ModSet(0)
    }

    const fn bit(m: Modifier) -> u8 {
        match m {
            Modifier::Alt => 1 << 0,
            Modifier::Control => 1 << 1,
            Modifier::Meta => 1 << 2,
            Modifier::Shift => 1 << 3,
        }
    }

    pub fn insert(&mut self, m: Modifier) {
        self.0 |= Self::bit(m);
    }

    /// Builder-style insert.
    pub fn with(mut self, m: Modifier) -> Self {
        self.insert(m);
        self
    }

    pub fn contains(&self, m: Modifier) -> bool {
        self.0 & Self::bit(m) != 0
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// True if every modifier in `self` is present in `other`.
    pub fn is_subset_of(&self, other: &ModSet) -> bool {
        self.0 & other.0 == self.0
    }
}

impl FromIterator<Modifier> for ModSet {
    fn from_iter<T: IntoIterator<Item = Modifier>>(iter: T) -> Self {
        let mut s = ModSet::empty();
        for m in iter {
            s.insert(m);
        }
        s
    }
}

/// A side-specific modifier (used by the picker / app layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModifierSpec {
    pub modifier: Modifier,
    pub side: Side,
}

/// What a double-tap watches: a key, or a modifier (with side).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TapKey {
    Key(KeySpec),
    Mod(Modifier, Side),
}

/// A trigger specification. Observation-only; never consumes the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Fires while the given modifier (optionally side-specific) is held.
    ModifierHold { modifier: Modifier, side: Side },
    /// One-shot when the key/modifier is tapped twice within `within`.
    DoubleTap { key: TapKey, within: Duration },
    /// One-shot when the key/modifier is tapped *alone* — pressed and released
    /// with no other key in between — within `within`. Holding it together with
    /// another key behaves as a normal key/modifier and never fires.
    IsolatedTap { key: TapKey, within: Duration },
    /// Fires while `key` is down and all `mods` are held (sides ignored).
    Chord { mods: ModSet, key: KeySpec },
    /// Fires while `key` is down and the held modifier set EQUALS `mods` exactly
    /// — neither a subset nor a superset. Unlike [`Trigger::Chord`] (subset),
    /// activation is strictly on the non-repeat key-down EDGE with `mods`
    /// already held: a key held before the modifiers arrive never fires, and
    /// extra modifiers held alongside `mods` suppress activation. Releases when
    /// `key` goes up OR the held modifier set drifts away from the exact `mods`
    /// (push-to-talk semantics).
    ComboExact { mods: ModSet, key: KeySpec },
    /// Fires while `key` is down and NO modifiers are held.
    SingleKey { key: KeySpec },
}

/// Press vs release phase of a trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyPhase {
    Pressed,
    Released,
}

/// Caller-supplied opaque identifier for a binding (kept stable across events).
pub type TriggerId = String;

/// An emitted trigger event for a registered [`TriggerId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerEvent {
    pub id: TriggerId,
    pub phase: KeyPhase,
}

/// A normalized physical key event produced by a backend. Modifier keys carry a
/// side-specific [`KeySpec`] (e.g. [`NamedKey::MetaRight`]) and `side = Some(..)`;
/// non-modifier keys carry `side = None`. The matcher receives `now` separately
/// so it stays clock-injectable and deterministic in tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKeyEvent {
    pub key: KeySpec,
    pub side: Option<Side>,
    pub down: bool,
    pub is_repeat: bool,
}

/// Errors from the engine lifecycle.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("trigger engine is already running")]
    AlreadyRunning,
    #[error("trigger engine is not running")]
    NotRunning,
    #[error("global key capture permission not granted")]
    PermissionDenied,
    #[error("backend failed to start: {0}")]
    Backend(String),
}
