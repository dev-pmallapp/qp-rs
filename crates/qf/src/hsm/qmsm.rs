//! Hierarchical State Machine optimized for code-generation (QMsm).
//!
//! Provides a static, data-driven QMsm hierarchical state machine compatible with
//! QP/C++ v8.x conventions.

#[cfg(any(not(feature = "static-alloc"), feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "static-alloc"))]
use alloc::vec::Vec;

use crate::active::{ActiveBehavior, ActiveContext};
use crate::event::DynEvent;
use crate::trace::TraceHook;

use super::common::{QAsm, SameState};
use super::history::HistoryMap;
use super::trace;

/// Maximum state-nesting depth for the heap-free ancestry path.
#[cfg(feature = "static-alloc")]
pub const QM_MAX_NEST: usize = 16;
/// Ancestry path type. Dynamic: heap [`Vec`]; `static-alloc`: heap-free
/// [`heapless::Vec`] bounded by [`QM_MAX_NEST`].
#[cfg(not(feature = "static-alloc"))]
type QmPath<S> = Vec<&'static QMState<S>>;
#[cfg(feature = "static-alloc")]
type QmPath<S> = heapless::Vec<&'static QMState<S>, QM_MAX_NEST>;

/// Return value from a QMsm state handler function.
pub enum QMsmResult<S: 'static> {
    /// Event fully handled.
    Handled,
    /// Event not handled; delegate to parent.
    Super(&'static QMState<S>),
    /// Perform transition to target.
    Tran(&'static QMState<S>),
    /// Perform transition to the shallow history of parent.
    TranHist(&'static QMState<S>),
    /// Event ignored.
    Ignored,
    /// Guard failed; treated as Ignored.
    Unhandled,
}

/// QMsm state handler function signature.
pub type QMStateHandler<S> = fn(&mut S, &DynEvent) -> QMsmResult<S>;

/// Nested initial-transition action: runs on entry to a composite state and
/// optionally returns the substate to descend into.
pub type QMInitAction<S> = fn(&mut S) -> Option<&'static QMState<S>>;

/// Static representation of a state in QMsm.
pub struct QMState<S: 'static> {
    /// Superstate of this state.
    pub superstate: Option<&'static QMState<S>>,
    /// State handler function processing events when in this state.
    pub state_handler: QMStateHandler<S>,
    /// Entry action of this state.
    pub entry_action: Option<fn(&mut S)>,
    /// Exit action of this state.
    pub exit_action: Option<fn(&mut S)>,
    /// Nested initial transition action.
    pub init_action: Option<QMInitAction<S>>,
}

// SAFETY: a `QMState` is an immutable state-table node — its fields are a
// superstate reference and bare `fn` pointers (no interior mutability, no owned
// data). It is built once in `static`/`const` storage and only ever read, so
// sharing and sending it across threads/ISRs cannot introduce a data race. The
// `S` bound is purely a type marker (`QMState` holds no `S`).
unsafe impl<S: 'static> Send for QMState<S> {}
// SAFETY: see the `Send` impl above — `QMState` is read-only after construction.
unsafe impl<S: 'static> Sync for QMState<S> {}

impl<S: 'static> SameState for &'static QMState<S> {
    #[inline]
    fn same_state(self, other: Self) -> bool {
        core::ptr::eq(self, other)
    }
}

/// Quantum Meta State Machine.
///
/// Traceability: ASR-007 (semi-formal behavioural model); see
/// `docs/traceability.md`.
pub struct QMsm<S: 'static> {
    state: &'static QMState<S>,
    temp: &'static QMState<S>,
    sm: S,
    history: HistoryMap<&'static QMState<S>>,
}

impl<S: Send + 'static> QMsm<S> {
    /// Creates a new `QMsm` wrapping the user data `sm` starting in `initial`.
    pub fn new(sm: S, initial: &'static QMState<S>) -> Self {
        Self {
            state: initial, // Will be set to target during init
            temp: initial,
            sm,
            history: HistoryMap::new(),
        }
    }

    /// Returns a shared reference to the user state machine data.
    pub fn sm(&self) -> &S {
        &self.sm
    }

    /// Returns a mutable reference to the user state machine data.
    pub fn sm_mut(&mut self) -> &mut S {
        &mut self.sm
    }

    /// Returns the current active state.
    pub fn state(&self) -> &'static QMState<S> {
        self.state
    }

    /// Returns the current state's handler function.
    ///
    /// qp-rs equivalent of QP/C++ `QAsm::getStateHandler()`, kept symmetric with
    /// [`QHsm::state_handler`](crate::QHsm::state_handler). Always available,
    /// not gated behind the `qs` feature.
    pub fn state_handler(&self) -> QMStateHandler<S> {
        self.state.state_handler
    }

    /// Check if the state machine is currently in `state` (or any sub-state).
    pub fn is_in(&self, state: &'static QMState<S>) -> bool {
        let mut cur = Some(self.state);
        while let Some(s) = cur {
            if s.same_state(state) {
                return true;
            }
            cur = s.superstate;
        }
        false
    }

    /// Drive the initial transition.
    pub fn init(&mut self) {
        self.init_traced(None);
    }

    /// Drive initial transition with tracing.
    pub fn init_traced(&mut self, trace: Option<TraceHook>) {
        let target = self.temp;
        let path = path_to_top(target);

        // Enter states top-down
        for i in (0..path.len()).rev() {
            let state = path[i];
            if let Some(entry_act) = state.entry_action {
                entry_act(&mut self.sm);
            }
            if let Some(ref hook) = trace {
                trace::emit_state_entry(hook, state as *const _ as usize);
            }
        }

        self.state = target;

        if let Some(ref hook) = trace {
            trace::emit_init_tran(hook, target as *const _ as usize);
        }

        self.handle_nested_init(&trace);
    }

    /// Dispatch an event to the state machine.
    pub fn dispatch(&mut self, event: &DynEvent) {
        self.dispatch_traced(event, None);
    }

    /// Dispatch an event with tracing.
    pub fn dispatch_traced(&mut self, event: &DynEvent, trace: Option<TraceHook>) {
        if let Some(ref hook) = trace {
            trace::emit_dispatch(hook, event.signal(), self.state as *const _ as usize);
        }

        let mut s = self.state;
        let source: &'static QMState<S>;
        let result: QMsmResult<S>;

        loop {
            let r = (s.state_handler)(&mut self.sm, event);
            match r {
                QMsmResult::Super(sup) => {
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
            QMsmResult::Handled => {
                if let Some(ref hook) = trace {
                    trace::emit_intern_tran(hook, event.signal(), source as *const _ as usize);
                }
            }
            QMsmResult::Ignored | QMsmResult::Unhandled => {
                if let Some(ref hook) = trace {
                    trace::emit_ignored(hook, event.signal(), source as *const _ as usize);
                }
            }
            QMsmResult::Tran(target) => {
                if let Some(ref hook) = trace {
                    trace::emit_tran(hook, event.signal(), source as *const _ as usize, target as *const _ as usize);
                }
                self.execute_tran(source, target, &trace);
            }
            QMsmResult::TranHist(parent) => {
                let target = self
                    .history
                    .get(&(parent as *const _ as usize))
                    .copied()
                    .unwrap_or(parent);
                if let Some(ref hook) = trace {
                    trace::emit_tran_hist(hook, event.signal(), source as *const _ as usize, target as *const _ as usize);
                }
                self.execute_tran(source, target, &trace);
            }
            QMsmResult::Super(_) => {}
        }
    }

    fn execute_tran(
        &mut self,
        source: &'static QMState<S>,
        target: &'static QMState<S>,
        trace: &Option<TraceHook>,
    ) {
        let current = self.state;

        let target_path = path_to_top(target);
        let lca_source = if source.same_state(target) {
            source.superstate.unwrap_or(source)
        } else {
            source
        };
        let lca_source_path = path_to_top(lca_source);

        let lca = find_lca(&lca_source_path, &target_path);

        let mut s = current;
        while lca.map_or(true, |lca_state| !s.same_state(lca_state)) {
            if let Some(parent) = s.superstate {
                #[cfg(not(feature = "static-alloc"))]
                self.history.insert(parent as *const _ as usize, s);
                #[cfg(feature = "static-alloc")]
                if self.history.insert(parent as *const _ as usize, s).is_err() {
                    crate::fusa::on_error(module_path!(), line!());
                }
            }

            if let Some(exit_act) = s.exit_action {
                exit_act(&mut self.sm);
            }

            if let Some(ref hook) = trace {
                trace::emit_state_exit(hook, s as *const _ as usize);
            }

            if let Some(parent) = s.superstate {
                s = parent;
            } else {
                break;
            }
        }

        let lca_idx = if let Some(lca_state) = lca {
            target_path.iter().position(|&t| t.same_state(lca_state)).unwrap_or(target_path.len())
        } else {
            target_path.len()
        };

        for i in (0..lca_idx).rev() {
            let state = target_path[i];
            if let Some(entry_act) = state.entry_action {
                entry_act(&mut self.sm);
            }
            if let Some(ref hook) = trace {
                trace::emit_state_entry(hook, state as *const _ as usize);
            }
        }

        self.state = target;
        self.handle_nested_init(trace);
    }

    fn handle_nested_init(&mut self, trace: &Option<TraceHook>) {
        while let Some(init_act) = self.state.init_action {
            let Some(next) = init_act(&mut self.sm) else {
                break;
            };
            let next_path = path_to_top(next);
            let current = self.state;
            let current_idx = next_path.iter().position(|&t| t.same_state(current)).unwrap_or(next_path.len());

            for i in (0..current_idx).rev() {
                let state = next_path[i];
                if let Some(entry_act) = state.entry_action {
                    entry_act(&mut self.sm);
                }
                if let Some(ref hook) = trace {
                    trace::emit_state_entry(hook, state as *const _ as usize);
                }
            }

            if let Some(ref hook) = trace {
                trace::emit_state_init(hook, self.state as *const _ as usize);
            }

            self.state = next;
        }
    }
}

impl<S: Send + 'static> QAsm for QMsm<S> {
    fn init(&mut self) {
        self.init();
    }

    fn dispatch(&mut self, event: &DynEvent) {
        self.dispatch(event);
    }
}

impl<S: Send + 'static> ActiveBehavior for QMsm<S> {
    fn on_start(&mut self, ctx: &mut ActiveContext) {
        self.init_traced(ctx.trace_hook());
    }

    fn on_event(&mut self, ctx: &mut ActiveContext, event: DynEvent) {
        self.dispatch_traced(&event, ctx.trace_hook());
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn path_to_top<S: 'static>(s: &'static QMState<S>) -> QmPath<S> {
    let mut path = QmPath::new();
    let mut cur = Some(s);
    while let Some(state) = cur {
        #[cfg(not(feature = "static-alloc"))]
        path.push(state);
        // Heap-free ancestry path is bounded by `QM_MAX_NEST`; deeper nesting is
        // a configuration fault.
        #[cfg(feature = "static-alloc")]
        if path.push(state).is_err() {
            crate::fusa::on_error(module_path!(), line!());
        }
        cur = state.superstate;
    }
    path
}

fn find_lca<S: 'static>(
    path1: &[&'static QMState<S>],
    path2: &[&'static QMState<S>],
) -> Option<&'static QMState<S>> {
    for &a in path1 {
        for &b in path2 {
            if a.same_state(b) {
                return Some(a);
            }
        }
    }
    None
}
