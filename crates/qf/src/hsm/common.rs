//! Common types, traits, and constants shared across state machine engines.

use crate::event::DynEvent;

/// Abstract state machine interface.
pub trait QAsm: Send + 'static {
    /// Initialise the state machine (execute initial transition).
    fn init(&mut self);
    /// Dispatch an event to the state machine.
    fn dispatch(&mut self, event: &DynEvent);
}

/// Trait for comparing state representations for identity.
pub trait SameState {
    /// Returns `true` if `self` and `other` represent the same state.
    fn same_state(self, other: Self) -> bool;
}

/// Reserved signals used internally by the QHsm and QMsm frameworks.
///
/// Every state handler receives these signals from the framework during
/// `init()` and `dispatch()`. User-defined signals **must** start at
/// [`Q_USER_SIG`] (value `4`) or higher.
pub mod reserved {
    use crate::event::Signal;

    /// Probe signal: the framework asks a state for its super-state.
    /// States should return `QHsmResult::Super(parent)` for this signal
    /// (achieved by the `_ => q_super!(parent)` catch-all arm).
    pub const Q_EMPTY_SIG: Signal = Signal(0);

    /// Entry action signal — perform one-time setup when entering the state.
    pub const Q_ENTRY_SIG: Signal = Signal(1);
    /// Numeric value of `Q_ENTRY_SIG` for use in `match` patterns.
    pub const Q_ENTRY_SIG_VAL: u16 = 1;

    /// Exit action signal — clean up when leaving the state.
    pub const Q_EXIT_SIG: Signal = Signal(2);
    /// Numeric value of `Q_EXIT_SIG` for use in `match` patterns.
    pub const Q_EXIT_SIG_VAL: u16 = 2;

    /// Initial transition signal — fired once to start the state's own sub-SM.
    pub const Q_INIT_SIG: Signal = Signal(3);
    /// Numeric value of `Q_INIT_SIG` for use in `match` patterns.
    pub const Q_INIT_SIG_VAL: u16 = 3;

    /// First signal value safe for user-defined signals.
    pub const Q_USER_SIG: Signal = Signal(4);
}
