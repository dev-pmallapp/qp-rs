//! Hierarchical State Machine (QHsm) framework — Phase 2.
//!
//! Provides a QHsm-compatible framework for code-based hierarchical state
//! machines, compatible with QP/C++ v8.x conventions.
//!
//! # Usage
//!
//! 1. Define your state machine data type `S`.
//! 2. Write state handler functions of type `StateHandler<S>`.
//! 3. Wrap everything in a `QHsm<S>` and register it as an `ActiveObject`.
//!
//! ```rust,ignore
//! use qf::hsm::{QHsm, QHsmResult, StateHandler};
//! use qf::hsm::reserved::*;
//! use qf::event::DynEvent;
//!
//! const TIMEOUT_SIG: u16 = 4; // Q_USER_SIG + 0
//!
//! struct Blinky { count: u32 }
//!
//! fn initial(sm: &mut Blinky, _e: &DynEvent) -> QHsmResult<Blinky> {
//!     QHsmResult::Tran(off)
//! }
//!
//! fn off(sm: &mut Blinky, e: &DynEvent) -> QHsmResult<Blinky> {
//!     match e.signal().0 {
//!         Q_ENTRY_SIG_VAL => { /* LED off */ QHsmResult::Handled }
//!         TIMEOUT_SIG     => QHsmResult::Tran(on),
//!         _               => QHsmResult::Super(QHsm::<Blinky>::top_state),
//!     }
//! }
//!
//! fn on(sm: &mut Blinky, e: &DynEvent) -> QHsmResult<Blinky> {
//!     match e.signal().0 {
//!         Q_ENTRY_SIG_VAL => { sm.count += 1; /* LED on */ QHsmResult::Handled }
//!         TIMEOUT_SIG     => QHsmResult::Tran(off),
//!         _               => QHsmResult::Super(QHsm::<Blinky>::top_state),
//!     }
//! }
//!
//! let mut ao_data = Blinky { count: 0 };
//! let mut hsm = QHsm::new(ao_data, initial);
//! hsm.init();
//! ```

#[cfg(not(feature = "static-alloc"))]
use alloc::collections::BTreeMap;

use crate::active::{ActiveBehavior, ActiveContext};
use crate::dis::Dup;
use crate::event::{DynEvent, Event, Signal};
use crate::trace::TraceHook;

/// Shallow-history table type. Dynamic: a heap [`BTreeMap`]; `static-alloc`: a
/// fixed-capacity, heap-free [`heapless::FnvIndexMap`] (capacity must be a power
/// of two). Keyed by parent-state fn-pointer address.
#[cfg(not(feature = "static-alloc"))]
type HistoryMap<S> = BTreeMap<usize, StateHandler<S>>;
/// Maximum number of composite states with remembered shallow history under the
/// heap-free build; exceeding it is a configuration fault.
#[cfg(feature = "static-alloc")]
pub const HSM_HISTORY_CAP: usize = 16;
#[cfg(feature = "static-alloc")]
type HistoryMap<S> = heapless::FnvIndexMap<usize, StateHandler<S>, HSM_HISTORY_CAP>;

// ── QS record IDs for QEP events ────────────────────────────────────────────
// Matches QP/C++ v8.x canonical values.
const QS_QEP_STATE_ENTRY:  u8 = 1;
const QS_QEP_STATE_EXIT:   u8 = 2;
const QS_QEP_STATE_INIT:   u8 = 3;
const QS_QEP_INIT_TRAN:    u8 = 4;
const QS_QEP_INTERN_TRAN:  u8 = 5;
const QS_QEP_TRAN:         u8 = 6;
const QS_QEP_IGNORED:      u8 = 7;
const QS_QEP_DISPATCH:     u8 = 8;
#[allow(dead_code)]
const QS_QEP_UNHANDLED:    u8 = 9;
const QS_QEP_TRAN_HIST:    u8 = 55;

/// Maximum nesting depth supported by the hierarchy traversal algorithm.
pub const MAX_NEST_DEPTH: usize = 6;

// ── Reserved signals ─────────────────────────────────────────────────────────

/// Reserved signals used internally by the QHsm framework.
///
/// Every state handler receives these signals from the framework during
/// `init()` and `dispatch()`.  User-defined signals **must** start at
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

use reserved::{Q_EMPTY_SIG, Q_ENTRY_SIG, Q_EXIT_SIG, Q_INIT_SIG};

// ── Core types ───────────────────────────────────────────────────────────────

/// Return value from a state handler function.
///
/// Every state handler must return one of these variants to tell the framework
/// what to do next.
pub enum QHsmResult<S> {
    /// The event was fully handled; no state transition occurs.
    Handled,

    /// This state does not handle the event — delegate to the given
    /// super-state.  Also returned (implicitly) for `Q_EMPTY_SIG`.
    Super(StateHandler<S>),

    /// Execute a state transition to `target`.
    Tran(StateHandler<S>),

    /// Execute a transition to the **history** of `parent`.  If no history
    /// has been recorded yet, enters `parent`'s initial transition.
    TranHist(StateHandler<S>),

    /// The event was explicitly recognised but intentionally ignored.
    Ignored,

    /// A guard condition prevented handling; treated the same as `Ignored`.
    Unhandled,
}

/// State handler function pointer.
///
/// Each state is a plain free function (or a named function item) with this
/// signature.  The first argument is a mutable reference to the user-defined
/// state machine data; the second is the current event.
pub type StateHandler<S> = fn(&mut S, &DynEvent) -> QHsmResult<S>;

/// Compare two state handlers for identity (by function-pointer address).
#[inline]
pub fn same_state<S>(a: StateHandler<S>, b: StateHandler<S>) -> bool {
    (a as usize) == (b as usize)
}

/// Abstract state machine interface.
pub trait QAsm: Send + 'static {
    /// Initialise the state machine (execute initial transition).
    fn init(&mut self);
    /// Dispatch an event to the state machine.
    fn dispatch(&mut self, event: &DynEvent);
}

// ── QHsm struct ──────────────────────────────────────────────────────────────

/// Hierarchical State Machine.
///
/// `QHsm<S>` wraps user-defined state machine data `S` and provides the
/// standard QHsm dispatch algorithm (LCA finding, entry/exit chain
/// execution, nested initial transitions, shallow history).
///
/// It implements [`ActiveBehavior`] so it can be registered directly with the
/// QF kernel.
///
/// Traceability: ASR-007 (semi-formal behavioural model); see
/// `docs/traceability.md`.
pub struct QHsm<S> {
    /// Current stable state handler (leaf of the active configuration),
    /// protected by Duplicate Storage: a corrupted state pointer would dispatch
    /// to the wrong handler, so the two copies are verified on every read (see
    /// `docs/FUSA.md`, Phase 3).
    state: Dup<StateHandler<S>>,
    /// Temporary: initial pseudo-state before `init()`, unused afterwards.
    temp: StateHandler<S>,
    /// User-defined state machine data.
    sm: S,
    /// Shallow history table.  Key = parent state fn-pointer as `usize`,
    /// value = last active direct child state handler.
    history: HistoryMap<S>,
}

impl<S: Send + 'static> QAsm for QHsm<S> {
    fn init(&mut self) {
        self.init();
    }

    fn dispatch(&mut self, event: &DynEvent) {
        self.dispatch(event);
    }
}

impl<S: Send + 'static> QHsm<S> {
    // ── Construction ─────────────────────────────────────────────────────────

    /// Creates a new `QHsm` wrapping the user data `sm`.
    ///
    /// `initial` is the *initial pseudo-state* handler, which must return
    /// `QHsmResult::Tran(first_real_state)` when called with `Q_INIT_SIG`.
    pub fn new(sm: S, initial: StateHandler<S>) -> Self {
        Self {
            state: Dup::new(Self::top_state as StateHandler<S>),
            temp: initial,
            sm,
            history: HistoryMap::new(),
        }
    }

    /// Returns `true` if the state machine is in the given state (or any of its substates).
    pub fn is_in(&mut self, state: StateHandler<S>) -> bool {
        let mut cur = self.state.get();
        loop {
            if same_state(cur, state) {
                return true;
            }
            if same_state(cur, Self::top_state) {
                break;
            }
            cur = self.get_super(cur);
        }
        false
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// Returns the current (stable leaf) state handler.
    ///
    /// This is the qp-rs equivalent of QP/C++ `QAsm::getStateHandler()`. It is
    /// **always available**, not gated behind the `qs` feature (mirroring QP/C++
    /// 8.1.1, which made `getStateHandler()` unconditionally virtual). Compare
    /// results with [`same_state`] rather than `==`.
    pub fn state_handler(&self) -> StateHandler<S> {
        self.state.get()
    }

    /// Returns a shared reference to the user state machine data.
    pub fn sm(&self) -> &S {
        &self.sm
    }

    /// Returns a mutable reference to the user state machine data.
    pub fn sm_mut(&mut self) -> &mut S {
        &mut self.sm
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Drive the initial transition.  Must be called once before the first
    /// `dispatch()`.
    pub fn init(&mut self) {
        self.init_traced(None);
    }

    /// Drive the initial transition with optional QS tracing.
    pub fn init_traced(&mut self, trace: Option<TraceHook>) {
        let init_e = Event::empty_dyn(Q_INIT_SIG);

        // Call the initial pseudo-state handler.
        let target = match (self.temp)(&mut self.sm, &init_e) {
            QHsmResult::Tran(t) => t,
            // Precondition on the application's state machine: the initial
            // pseudo-state handler must request a transition to a concrete
            // state. Anything else is a contract violation — fault out.
            _ => crate::fusa::on_error(module_path!(), line!()),
        };

        // Build entry path from target up to (not including) top.
        let (path, len) = self.path_to_top(target);

        // Enter states top-down.
        for i in (0..len).rev() {
            self.call_entry(path[i]);
            if let Some(ref hook) = trace {
                emit_state_entry(hook, path[i] as usize);
            }
        }

        self.state.set(target);

        if let Some(ref hook) = trace {
            emit_init_tran(hook, target as usize);
        }

        // Resolve any nested initial transitions in the target composite state.
        self.handle_nested_init(&trace);
    }

    // ── Dispatch ─────────────────────────────────────────────────────────────

    /// Dispatch an event to the HSM.
    pub fn dispatch(&mut self, event: &DynEvent) {
        self.dispatch_traced(event, None);
    }

    /// Dispatch an event with optional QS tracing.
    pub fn dispatch_traced(&mut self, event: &DynEvent, trace: Option<TraceHook>) {
        if let Some(ref hook) = trace {
            emit_dispatch(hook, event.signal(), self.state.get() as usize);
        }

        // Walk the hierarchy upward until an event handler is found.
        let mut s = self.state.get();
        let source: StateHandler<S>;
        let result: QHsmResult<S>;

        loop {
            let r = (s)(&mut self.sm, event);
            match r {
                QHsmResult::Super(sup) => {
                    s = sup;
                }
                _ => {
                    source = s;
                    result = r;
                    break;
                }
            }
        }

        match result {
            QHsmResult::Handled => {
                if let Some(ref hook) = trace {
                    emit_intern_tran(hook, event.signal(), source as usize);
                }
            }
            QHsmResult::Ignored | QHsmResult::Unhandled => {
                if let Some(ref hook) = trace {
                    emit_ignored(hook, event.signal(), source as usize);
                }
            }
            QHsmResult::Tran(target) => {
                if let Some(ref hook) = trace {
                    emit_tran(hook, event.signal(), source as usize, target as usize);
                }
                self.execute_tran(source, target, &trace);
            }
            QHsmResult::TranHist(parent) => {
                // Look up the remembered substate, falling back to the parent
                // itself (which will trigger its own initial transition).
                let target = self
                    .history
                    .get(&(parent as usize))
                    .copied()
                    .unwrap_or(parent);
                if let Some(ref hook) = trace {
                    emit_tran_hist(hook, event.signal(), source as usize, target as usize);
                }
                self.execute_tran(source, target, &trace);
            }
            QHsmResult::Super(_) => {
                // Hierarchy walk exhausted (should not normally escape).
            }
        }
    }

    // ── Top-level superstate ─────────────────────────────────────────────────

    /// The universal top-level superstate sentinel.
    ///
    /// All user states ultimately return `Super(QHsm::<S>::top_state)` as
    /// their default case (unless they sub-class a composite state).
    /// `top_state` returns `Ignored` for every event, which terminates the
    /// upward hierarchy walk.
    pub fn top_state(_sm: &mut S, _e: &DynEvent) -> QHsmResult<S> {
        QHsmResult::Ignored
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Returns the super-state of `s` by calling it with `Q_EMPTY_SIG`.
    fn get_super(&mut self, s: StateHandler<S>) -> StateHandler<S> {
        let empty_e = Event::empty_dyn(Q_EMPTY_SIG);
        match (s)(&mut self.sm, &empty_e) {
            QHsmResult::Super(sup) => sup,
            _ => Self::top_state,
        }
    }

    /// Calls the entry action of state `s` (return value is discarded).
    fn call_entry(&mut self, s: StateHandler<S>) {
        let entry_e = Event::empty_dyn(Q_ENTRY_SIG);
        let _ = (s)(&mut self.sm, &entry_e);
    }

    /// Calls the exit action of state `s` (return value is discarded).
    fn call_exit(&mut self, s: StateHandler<S>) {
        let exit_e = Event::empty_dyn(Q_EXIT_SIG);
        let _ = (s)(&mut self.sm, &exit_e);
    }

    /// Builds the ancestry chain from `s` upward, stopping before `top_state`.
    /// Returns the chain and its length.
    ///
    /// `path[0]` = `s`, `path[len-1]` = topmost non-top ancestor.
    fn path_to_top(&mut self, s: StateHandler<S>) -> ([StateHandler<S>; MAX_NEST_DEPTH], usize) {
        let top: StateHandler<S> = Self::top_state;
        let mut path = [top; MAX_NEST_DEPTH];
        let mut len = 0usize;
        let mut cur = s;
        while !same_state(cur, Self::top_state) && len < MAX_NEST_DEPTH {
            path[len] = cur;
            len += 1;
            cur = self.get_super(cur);
        }
        (path, len)
    }

    /// Find the Lowest Common Ancestor of two states given their ancestry paths.
    /// Falls back to `top_state` if no common ancestor is found (the two
    /// sub-trees share only `top_state`).
    fn find_lca(
        path1: &[StateHandler<S>],
        path2: &[StateHandler<S>],
    ) -> StateHandler<S> {
        for &a in path1 {
            for &b in path2 {
                if same_state(a, b) {
                    return a;
                }
            }
        }
        Self::top_state
    }

    /// Execute a state transition from `source` to `target`.
    fn execute_tran(
        &mut self,
        source: StateHandler<S>,
        target: StateHandler<S>,
        trace: &Option<TraceHook>,
    ) {
        let current = self.state.get();

        // Build ancestry paths.
        let (target_path, target_len) = self.path_to_top(target);

        // For LCA finding, use super(source) when source == target (self-transition)
        // so the source state itself is exited and re-entered.
        let lca_source = if same_state(source, target) {
            self.get_super(source)
        } else {
            source
        };
        let (lca_source_path, lca_source_len) = if same_state(source, target) {
            self.path_to_top(lca_source)
        } else {
            self.path_to_top(source)
        };

        let lca = Self::find_lca(
            &lca_source_path[..lca_source_len],
            &target_path[..target_len],
        );

        // Exit from `current` up to (not including) `lca`, recording shallow
        // history at each level so TranHist can later restore any ancestor.
        let mut s = current;
        while !same_state(s, lca) {
            // get_super first so we call the state handler only once per state.
            let parent = self.get_super(s);

            // Record: `s` was the last active direct child of `parent`.
            if !same_state(parent, Self::top_state) {
                #[cfg(not(feature = "static-alloc"))]
                self.history.insert(parent as usize, s);
                // Heap-free map is fixed-capacity: a full history table is a
                // configuration fault (too many composite states with history).
                #[cfg(feature = "static-alloc")]
                if self.history.insert(parent as usize, s).is_err() {
                    crate::fusa::on_error(module_path!(), line!());
                }
            }

            self.call_exit(s);
            if let Some(ref hook) = trace {
                emit_state_exit(hook, s as usize);
            }

            s = parent;
            if same_state(s, Self::top_state) {
                break;
            }
        }

        // Enter states from (not including) `lca` down to `target`.
        // The entry chain is target_path[0..lca_idx] in reverse.
        let lca_idx = target_path[..target_len]
            .iter()
            .position(|&t| same_state(t, lca))
            .unwrap_or(target_len); // if LCA not in path, enter whole chain

        for i in (0..lca_idx).rev() {
            self.call_entry(target_path[i]);
            if let Some(ref hook) = trace {
                emit_state_entry(hook, target_path[i] as usize);
            }
        }

        self.state.set(target);

        // Resolve nested initial transitions in the new state.
        self.handle_nested_init(trace);
    }

    /// Drive nested initial transitions after entering a new composite state.
    ///
    /// Repeatedly calls the current state with `Q_INIT_SIG`.  If it returns
    /// `Tran(next)`, enters the chain from current down to `next` and
    /// continues.  Stops when the current state does not take an initial
    /// transition (leaf state reached).
    fn handle_nested_init(&mut self, trace: &Option<TraceHook>) {
        let init_e = Event::empty_dyn(Q_INIT_SIG);
        while let QHsmResult::Tran(next) = (self.state.get())(&mut self.sm, &init_e) {
            // Build path from `next` up to `self.state` (the current composite state).
            let (next_path, next_len) = self.path_to_top(next);

            // Find where `self.state` sits in that path.
            let current = self.state.get();
            let current_idx = next_path[..next_len]
                .iter()
                .position(|&t| same_state(t, current))
                .unwrap_or(next_len);

            // Enter states from (not including) current down to next.
            for i in (0..current_idx).rev() {
                self.call_entry(next_path[i]);
                if let Some(ref hook) = trace {
                    emit_state_entry(hook, next_path[i] as usize);
                }
            }

            if let Some(ref hook) = trace {
                emit_state_init(hook, self.state.get() as usize);
            }

            self.state.set(next);
        }
    }
}

// ── ActiveBehavior bridge ─────────────────────────────────────────────────────

impl<S: Send + 'static> ActiveBehavior for QHsm<S> {
    fn on_start(&mut self, ctx: &mut ActiveContext) {
        self.init_traced(ctx.trace_hook());
    }

    fn on_event(&mut self, ctx: &mut ActiveContext, event: DynEvent) {
        self.dispatch_traced(&event, ctx.trace_hook());
    }
}

// ── QS trace emission helpers ─────────────────────────────────────────────────

const PTR_SIZE: usize = core::mem::size_of::<usize>();

fn emit_state_entry(hook: &TraceHook, state_ptr: usize) {
    let _ = hook(QS_QEP_STATE_ENTRY, &state_ptr.to_le_bytes(), false);
}

fn emit_state_exit(hook: &TraceHook, state_ptr: usize) {
    let _ = hook(QS_QEP_STATE_EXIT, &state_ptr.to_le_bytes(), false);
}

fn emit_state_init(hook: &TraceHook, state_ptr: usize) {
    let _ = hook(QS_QEP_STATE_INIT, &state_ptr.to_le_bytes(), false);
}

fn emit_init_tran(hook: &TraceHook, state_ptr: usize) {
    let _ = hook(QS_QEP_INIT_TRAN, &state_ptr.to_le_bytes(), false);
}

fn emit_dispatch(hook: &TraceHook, sig: Signal, state_ptr: usize) {
    let mut buf = [0u8; 2 + PTR_SIZE];
    buf[0..2].copy_from_slice(&sig.0.to_le_bytes());
    buf[2..].copy_from_slice(&state_ptr.to_le_bytes());
    let _ = hook(QS_QEP_DISPATCH, &buf, true);
}

fn emit_intern_tran(hook: &TraceHook, sig: Signal, state_ptr: usize) {
    let mut buf = [0u8; 2 + PTR_SIZE];
    buf[0..2].copy_from_slice(&sig.0.to_le_bytes());
    buf[2..].copy_from_slice(&state_ptr.to_le_bytes());
    let _ = hook(QS_QEP_INTERN_TRAN, &buf, true);
}

fn emit_ignored(hook: &TraceHook, sig: Signal, state_ptr: usize) {
    let mut buf = [0u8; 2 + PTR_SIZE];
    buf[0..2].copy_from_slice(&sig.0.to_le_bytes());
    buf[2..].copy_from_slice(&state_ptr.to_le_bytes());
    let _ = hook(QS_QEP_IGNORED, &buf, true);
}

fn emit_tran(hook: &TraceHook, sig: Signal, source_ptr: usize, target_ptr: usize) {
    let mut buf = [0u8; 2 + PTR_SIZE * 2];
    buf[0..2].copy_from_slice(&sig.0.to_le_bytes());
    buf[2..2 + PTR_SIZE].copy_from_slice(&source_ptr.to_le_bytes());
    buf[2 + PTR_SIZE..].copy_from_slice(&target_ptr.to_le_bytes());
    let _ = hook(QS_QEP_TRAN, &buf, true);
}

fn emit_tran_hist(hook: &TraceHook, sig: Signal, source_ptr: usize, target_ptr: usize) {
    let mut buf = [0u8; 2 + PTR_SIZE * 2];
    buf[0..2].copy_from_slice(&sig.0.to_le_bytes());
    buf[2..2 + PTR_SIZE].copy_from_slice(&source_ptr.to_le_bytes());
    buf[2 + PTR_SIZE..].copy_from_slice(&target_ptr.to_le_bytes());
    let _ = hook(QS_QEP_TRAN_HIST, &buf, true);
}
