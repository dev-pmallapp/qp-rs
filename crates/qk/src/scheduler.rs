use crate::sync::Mutex;
use qf::{ContextSwitchHook, TraceHook};

#[cfg(feature = "qs")]
use qs::records::sched;

#[cfg(not(feature = "qs"))]
mod sched {
    pub const LOCK: u8 = 0;
    pub const UNLOCK: u8 = 1;
    pub const NEXT: u8 = 2;
    pub const IDLE: u8 = 3;
}

const SCHED_UNLOCKED: u8 = 0xFF;

/// Saved scheduler-lock status, returned by [`QkScheduler::lock`] and passed
/// back to [`QkScheduler::unlock`] to restore the previous ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedStatus {
    /// Scheduler is locked at the given priority ceiling.
    Locked(u8),
    /// Scheduler is unlocked.
    Unlocked,
}

impl SchedStatus {
    /// Decodes a status from its raw `u8` encoding (`0xFF` means unlocked).
    pub fn from_raw(raw: u8) -> Self {
        if raw == SCHED_UNLOCKED {
            Self::Unlocked
        } else {
            Self::Locked(raw)
        }
    }

    /// Encodes the status as a raw `u8` (`0xFF` means unlocked).
    pub fn to_raw(self) -> u8 {
        match self {
            Self::Locked(value) => value,
            Self::Unlocked => SCHED_UNLOCKED,
        }
    }

    /// Returns `true` if the scheduler is currently locked.
    pub fn is_locked(self) -> bool {
        matches!(self, Self::Locked(_))
    }
}

/// 64-bit bitmap for O(1) priority tracking.
///
/// Supports priorities 0-63 using a single u64 bitset. Uses leading_zeros
/// for constant-time maximum priority lookup.
#[derive(Default, Clone, Copy)]
struct ReadySet {
    bits: u64,
}

impl ReadySet {
    /// Marks the given priority as ready.
    fn insert(&mut self, prio: u8) {
        Self::assert_range(prio);
        self.bits |= 1u64 << prio;
    }

    /// Marks the given priority as not ready.
    fn remove(&mut self, prio: u8) {
        Self::assert_range(prio);
        self.bits &= !(1u64 << prio);
    }

    /// Returns true if the given priority is marked ready.
    fn contains(&self, prio: u8) -> bool {
        Self::assert_range(prio);
        (self.bits & (1u64 << prio)) != 0
    }

    /// Returns the highest priority marked ready, or None if empty.
    ///
    /// Uses leading_zeros for O(1) lookup.
    fn max(&self) -> Option<u8> {
        if self.bits == 0 {
            None
        } else {
            Some(63 - self.bits.leading_zeros() as u8)
        }
    }

    /// Clears all ready priorities.
    fn clear(&mut self) {
        self.bits = 0;
    }

    /// Validates that priority is in the supported range 0-63.
    fn assert_range(prio: u8) {
        assert!(prio < 64, "priority {prio} exceeds supported range 0..63");
    }
}

#[cfg(feature = "smp")]
#[derive(Default, Clone, Copy)]
struct CoreState {
    active_prio: u8,
    active_threshold: u8,
    next_prio: u8,
}

#[cfg(feature = "smp")]
struct State {
    lock_ceiling: u8,
    cores: [CoreState; 8],
    ready: ReadySet,
    executing_cores: [u8; 64],
}

#[cfg(feature = "smp")]
impl Default for State {
    fn default() -> Self {
        Self {
            lock_ceiling: 0,
            cores: [CoreState::default(); 8],
            ready: ReadySet::default(),
            executing_cores: [0xFF; 64],
        }
    }
}

#[cfg(not(feature = "smp"))]
#[derive(Default)]
struct State {
    lock_ceiling: u8,
    active_prio: u8,
    active_threshold: u8,
    next_prio: u8,
    ready: ReadySet,
}

/// Outcome of a scheduling pass: which priority should run next and which one
/// it displaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleDecision {
    /// Priority selected to run next.
    pub next_prio: u8,
    /// Priority of the active object that was running before this decision.
    pub previous_prio: u8,
}

/// O(1) priority scheduler for the QK kernel: a 64-bit ready-set bitmap plus
/// preemption-threshold and lock-ceiling bookkeeping.
pub struct QkScheduler {
    state: Mutex<State>,
    trace: Mutex<Option<TraceHook>>,
    context_sw: Mutex<Option<ContextSwitchHook>>,
}

impl QkScheduler {
    /// Creates a scheduler with an optional QS trace hook.
    pub fn new(trace: Option<TraceHook>) -> Self {
        Self {
            state: Mutex::new(State::default()),
            trace: Mutex::new(trace),
            context_sw: Mutex::new(None),
        }
    }

    /// Installs (or clears) the QS trace hook.
    pub fn set_trace_hook(&self, trace: Option<TraceHook>) {
        *self.trace.lock() = trace;
    }

    /// Installs (or clears) the context-switch hook, invoked on every change of
    /// the running priority — see [`ContextSwitchHook`] (QP/C++ `QF_onContextSw`).
    pub fn set_context_switch_hook(&self, hook: Option<ContextSwitchHook>) {
        *self.context_sw.lock() = hook;
    }

    /// Locks the scheduler at the given priority ceiling.
    ///
    /// Prevents preemption by tasks with priority <= ceiling. Returns the
    /// previous lock status for restoration via `unlock()`.
    ///
    /// # Parameters
    /// - `ceiling`: Maximum priority that can execute while locked
    pub fn lock(&self, ceiling: u8) -> SchedStatus {
        let mut state = self.state.lock();
        if ceiling > state.lock_ceiling {
            let previous = state.lock_ceiling;
            state.lock_ceiling = ceiling;
            drop(state);
            self.emit_record(sched::LOCK, &[previous, ceiling], true);
            SchedStatus::Locked(previous)
        } else {
            SchedStatus::Unlocked
        }
    }

    /// Unlocks the scheduler, restoring the previous lock status.
    ///
    /// # Parameters
    /// - `prev`: Status returned from `lock()` to restore
    pub fn unlock(&self, prev: SchedStatus) {
        if let SchedStatus::Locked(value) = prev {
            let mut state = self.state.lock();
            if state.lock_ceiling > value {
                let current = state.lock_ceiling;
                state.lock_ceiling = value;
                drop(state);
                self.emit_record(sched::UNLOCK, &[current, value], true);
            }
        }
    }

    /// Marks the given priority as ready to run.
    pub fn mark_ready(&self, prio: u8) {
        let mut state = self.state.lock();
        state.ready.insert(prio);
    }

    /// Clears the ready flag for the given priority.
    pub fn mark_not_ready(&self, prio: u8) {
        let mut state = self.state.lock();
        state.ready.remove(prio);
    }

    /// Returns `true` if the given priority is currently ready.
    pub fn is_ready(&self, prio: u8) -> bool {
        let state = self.state.lock();
        state.ready.contains(prio)
    }

    /// Clears the entire ready set.
    pub fn reset_ready(&self) {
        let mut state = self.state.lock();
        state.ready.clear();
    }

    /// Records the currently running active object's priority and preemption
    /// threshold, used to gate subsequent scheduling decisions.
    #[cfg(not(feature = "smp"))]
    pub fn configure_active(&self, active_prio: u8, threshold: u8) {
        let mut state = self.state.lock();
        state.active_prio = active_prio;
        state.active_threshold = threshold;
    }

    /// Records the currently running active object's priority and preemption
    /// threshold, used to gate subsequent scheduling decisions.
    #[cfg(feature = "smp")]
    pub fn configure_active(&self, active_prio: u8, threshold: u8) {
        let core_id = qf::port::current_core_id() as usize;
        let mut state = self.state.lock();
        state.cores[core_id].active_prio = active_prio;
        state.cores[core_id].active_threshold = threshold;
        if active_prio > 0 && (active_prio as usize) < 64 {
            state.executing_cores[active_prio as usize] = core_id as u8;
        }
    }

    /// Selects the highest-priority ready task that may preempt the current one
    /// (above both the active threshold and the lock ceiling), or `None`.
    #[cfg(not(feature = "smp"))]
    pub fn plan_activation(&self) -> Option<ScheduleDecision> {
        let mut state = self.state.lock();

        let candidate = match state.ready.max() {
            Some(prio) if prio > state.active_threshold && prio > state.lock_ceiling => prio,
            _ => {
                state.next_prio = 0;
                return None;
            }
        };

        state.next_prio = candidate;
        Some(ScheduleDecision {
            next_prio: candidate,
            previous_prio: state.active_prio,
        })
    }

    /// Selects the highest-priority ready task that may preempt the current one
    /// (above both the active threshold and the lock ceiling), or `None`.
    #[cfg(feature = "smp")]
    pub fn plan_activation(&self) -> Option<ScheduleDecision> {
        let core_id = qf::port::current_core_id() as usize;
        let mut state = self.state.lock();
        let active_threshold = state.cores[core_id].active_threshold;
        let lock_ceiling = state.lock_ceiling;

        let mut found_candidate = None;
        for prio in (1..64).rev() {
            if state.ready.contains(prio) {
                if prio > active_threshold && prio > lock_ceiling {
                    let already_running = (0..8).any(|c| {
                        c != core_id && (state.cores[c].active_prio == prio || state.cores[c].next_prio == prio)
                    }) || state.executing_cores[prio as usize] != 0xFF;
                    if !already_running {
                        found_candidate = Some(prio);
                        break;
                    }
                }
            }
        }

        if let Some(candidate) = found_candidate {
            state.cores[core_id].next_prio = candidate;
            Some(ScheduleDecision {
                next_prio: candidate,
                previous_prio: state.cores[core_id].active_prio,
            })
        } else {
            state.cores[core_id].next_prio = 0;
            None
        }
    }

    /// Returns `true` if any ready task could preempt the current one under the
    /// active threshold and lock ceiling.
    #[cfg(not(feature = "smp"))]
    pub fn has_ready_to_run(&self) -> bool {
        let state = self.state.lock();
        matches!(state.ready.max(), Some(prio) if prio > state.active_threshold && prio > state.lock_ceiling)
    }

    /// Returns `true` if any ready task could preempt the current one under the
    /// active threshold and lock ceiling.
    #[cfg(feature = "smp")]
    pub fn has_ready_to_run(&self) -> bool {
        let core_id = qf::port::current_core_id() as usize;
        let state = self.state.lock();
        let active_threshold = state.cores[core_id].active_threshold;
        let lock_ceiling = state.lock_ceiling;

        for prio in (1..64).rev() {
            if state.ready.contains(prio) {
                if prio > active_threshold && prio > lock_ceiling {
                    let already_running = (0..8).any(|c| {
                        c != core_id && (state.cores[c].active_prio == prio || state.cores[c].next_prio == prio)
                    }) || state.executing_cores[prio as usize] != 0xFF;
                    if !already_running {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Like [`plan_activation`](Self::plan_activation) but gated by an explicit
    /// initial threshold — used after a dispatch completes to pick the follow-up.
    #[cfg(not(feature = "smp"))]
    pub fn next_after_dispatch(&self, initial_threshold: u8) -> Option<ScheduleDecision> {
        let mut state = self.state.lock();

        let candidate = match state.ready.max() {
            Some(prio) if prio > initial_threshold && prio > state.lock_ceiling => prio,
            _ => {
                state.next_prio = 0;
                return None;
            }
        };

        state.next_prio = candidate;
        Some(ScheduleDecision {
            next_prio: candidate,
            previous_prio: state.active_prio,
        })
    }

    /// Like [`plan_activation`](Self::plan_activation) but gated by an explicit
    /// initial threshold — used after a dispatch completes to pick the follow-up.
    #[cfg(feature = "smp")]
    pub fn next_after_dispatch(&self, initial_threshold: u8) -> Option<ScheduleDecision> {
        let core_id = qf::port::current_core_id() as usize;
        let mut state = self.state.lock();
        let lock_ceiling = state.lock_ceiling;

        let mut found_candidate = None;
        for prio in (1..64).rev() {
            if state.ready.contains(prio) {
                if prio > initial_threshold && prio > lock_ceiling {
                    let already_running = (0..8).any(|c| {
                        c != core_id && (state.cores[c].active_prio == prio || state.cores[c].next_prio == prio)
                    }) || state.executing_cores[prio as usize] != 0xFF;
                    if !already_running {
                        found_candidate = Some(prio);
                        break;
                    }
                }
            }
        }

        if let Some(candidate) = found_candidate {
            state.cores[core_id].next_prio = candidate;
            Some(ScheduleDecision {
                next_prio: candidate,
                previous_prio: state.cores[core_id].active_prio,
            })
        } else {
            state.cores[core_id].next_prio = 0;
            None
        }
    }

    /// Commits a planned [`ScheduleDecision`], making `next_prio` the active
    /// priority with the given threshold and emitting a `NEXT` trace record.
    #[cfg(not(feature = "smp"))]
    pub fn commit_activation(&self, decision: &ScheduleDecision, next_threshold: u8) {
        let mut state = self.state.lock();
        debug_assert_eq!(state.next_prio, decision.next_prio);

        let previous = state.active_prio;
        state.active_prio = decision.next_prio;
        state.active_threshold = next_threshold;
        state.next_prio = 0;
        drop(state);

        if decision.next_prio != previous {
            self.emit_record(sched::NEXT, &[decision.next_prio, previous], true);
            self.emit_context_sw(previous, decision.next_prio);
        }
    }

    /// Commits a planned [`ScheduleDecision`], making `next_prio` the active
    /// priority with the given threshold and emitting a `NEXT` trace record.
    #[cfg(feature = "smp")]
    pub fn commit_activation(&self, decision: &ScheduleDecision, next_threshold: u8) {
        let core_id = qf::port::current_core_id() as usize;
        let mut state = self.state.lock();
        debug_assert_eq!(state.cores[core_id].next_prio, decision.next_prio);

        let previous = state.cores[core_id].active_prio;
        state.cores[core_id].active_prio = decision.next_prio;
        state.cores[core_id].active_threshold = next_threshold;
        state.cores[core_id].next_prio = 0;
        if decision.next_prio > 0 && (decision.next_prio as usize) < 64 {
            state.executing_cores[decision.next_prio as usize] = core_id as u8;
        }
        drop(state);

        if decision.next_prio != previous {
            self.emit_record(sched::NEXT, &[decision.next_prio, previous], true);
            self.emit_context_sw(previous, decision.next_prio);
        }
    }

    /// Restores the active priority and threshold after a preemption returns,
    /// emitting a `NEXT` or `IDLE` trace record as appropriate.
    #[cfg(not(feature = "smp"))]
    pub fn restore_active(&self, prio: u8, threshold: u8) {
        let mut state = self.state.lock();
        let previous = state.active_prio;
        state.active_prio = prio;
        state.active_threshold = threshold;
        state.next_prio = 0;
        drop(state);

        if prio == 0 {
            if previous != 0 {
                self.emit_record(sched::IDLE, &[previous], true);
                self.emit_context_sw(previous, 0);
            }
        } else if prio != previous {
            self.emit_record(sched::NEXT, &[prio, previous], true);
            self.emit_context_sw(previous, prio);
        }
    }

    /// Restores the active priority and threshold after a preemption returns,
    /// emitting a `NEXT` or `IDLE` trace record as appropriate.
    #[cfg(feature = "smp")]
    pub fn restore_active(&self, prio: u8, threshold: u8) {
        let core_id = qf::port::current_core_id() as usize;
        let mut state = self.state.lock();
        let previous = state.cores[core_id].active_prio;
        state.cores[core_id].active_prio = prio;
        state.cores[core_id].active_threshold = threshold;
        state.cores[core_id].next_prio = 0;
        drop(state);

        if prio == 0 {
            if previous != 0 {
                self.emit_record(sched::IDLE, &[previous], true);
                self.emit_context_sw(previous, 0);
            }
        } else if prio != previous {
            self.emit_record(sched::NEXT, &[prio, previous], true);
            self.emit_context_sw(previous, prio);
        }
    }

    /// Returns the priority that would preempt the current task, if any.
    pub fn preemption_candidate(&self) -> Option<u8> {
        self.plan_activation().map(|decision| decision.next_prio)
    }

    /// Returns the priority planned to run next (0 if none is planned).
    #[cfg(not(feature = "smp"))]
    pub fn next_priority(&self) -> u8 {
        self.state.lock().next_prio
    }

    /// Returns the priority planned to run next (0 if none is planned).
    #[cfg(feature = "smp")]
    pub fn next_priority(&self) -> u8 {
        let core_id = qf::port::current_core_id() as usize;
        self.state.lock().cores[core_id].next_prio
    }

    /// Returns the priority of the currently active task.
    #[cfg(not(feature = "smp"))]
    pub fn current_priority(&self) -> u8 {
        self.state.lock().active_prio
    }

    /// Returns the priority of the currently active task.
    #[cfg(feature = "smp")]
    pub fn current_priority(&self) -> u8 {
        let core_id = qf::port::current_core_id() as usize;
        self.state.lock().cores[core_id].active_prio
    }

    #[cfg(not(feature = "smp"))]
    #[inline(always)]
    pub fn complete_execution(&self, _prio: u8) {}

    #[cfg(feature = "smp")]
    pub fn complete_execution(&self, prio: u8) {
        let mut state = self.state.lock();
        if prio > 0 && (prio as usize) < 64 {
            state.executing_cores[prio as usize] = 0xFF;
        }
    }

    fn emit_record(&self, record: u8, payload: &[u8], timestamp: bool) {
        let trace = self.trace.lock().clone();

        if let Some(trace) = trace {
            let _ = trace(record, payload, timestamp);
        }
    }

    fn emit_context_sw(&self, prev: u8, next: u8) {
        // Take a copy of the hook and release the lock before invoking it. The
        // hook is `Arc`-cloned on the dynamic build; under `static-alloc` it is a
        // `&'static` function object (Copy), so copy it out directly.
        #[cfg(not(feature = "static-alloc"))]
        let hook = self.context_sw.lock().clone();
        #[cfg(feature = "static-alloc")]
        let hook = *self.context_sw.lock();

        if let Some(hook) = hook {
            hook(prev, next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::Arc;

    #[test]
    fn lock_unlock_sequence() {
        let records = Arc::new(Mutex::new(Vec::new()));
        let hook_records = Arc::clone(&records);
        let hook: TraceHook = Arc::new(move |id, payload, timestamp| {
            hook_records.lock().push((id, payload.to_vec(), timestamp));
            Ok(())
        });

        let scheduler = QkScheduler::new(Some(hook));

        let status = scheduler.lock(5);
        assert_eq!(status, SchedStatus::Locked(0));

        scheduler.unlock(status);

        let recorded = records.lock();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0], (sched::LOCK, vec![0, 5], true));
        assert_eq!(recorded[1], (sched::UNLOCK, vec![5, 0], true));
    }

    #[test]
    fn ready_set_operations() {
        let scheduler = QkScheduler::new(None);
        scheduler.mark_ready(3);
        scheduler.mark_ready(7);
        assert!(scheduler.is_ready(3));
        assert!(scheduler.is_ready(7));
        scheduler.mark_not_ready(3);
        assert!(!scheduler.is_ready(3));
        assert!(scheduler.is_ready(7));
        scheduler.reset_ready();
        assert!(!scheduler.is_ready(7));
    }

    #[test]
    fn plan_and_commit_activation() {
        let scheduler = QkScheduler::new(None);
        scheduler.configure_active(2, 4);
        scheduler.mark_ready(5);
        scheduler.mark_ready(6);

        let decision = scheduler
            .plan_activation()
            .expect("priority 6 should preempt");
        assert_eq!(decision.next_prio, 6);
        assert_eq!(decision.previous_prio, 2);

        scheduler.commit_activation(&decision, 5);
        assert_eq!(scheduler.current_priority(), 6);
        assert_eq!(scheduler.next_priority(), 0);

        let status = scheduler.lock(6);
        assert!(scheduler.plan_activation().is_none());
        scheduler.unlock(status);

        scheduler.restore_active(2, 4);
        assert_eq!(scheduler.current_priority(), 2);
    }

    #[test]
    fn commit_and_restore_emit_records() {
        let records = Arc::new(Mutex::new(Vec::new()));
        let hook_records = Arc::clone(&records);
        let hook: TraceHook = Arc::new(move |id, payload, timestamp| {
            let mut guard = hook_records.lock();
            guard.push((id, payload.to_vec(), timestamp));
            Ok(())
        });

        let scheduler = QkScheduler::new(Some(hook));
        scheduler.configure_active(0, 0);
        scheduler.mark_ready(4);

        let decision = scheduler
            .plan_activation()
            .expect("priority 4 should be scheduled");
        scheduler.commit_activation(&decision, 2);
        scheduler.restore_active(0, 0);

        let recorded = records.lock();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0], (sched::NEXT, vec![4, 0], true));
        assert_eq!(recorded[1], (sched::IDLE, vec![4], true));
    }

    // The context-switch hook is a `&'static` function object under
    // `static-alloc` (no allocator), so these `Arc`-closure tests are
    // dynamic-only; the hook firing logic is identical on both builds.
    #[cfg(not(feature = "static-alloc"))]
    #[test]
    fn context_switch_hook_fires_on_commit_and_restore() {
        let switches = Arc::new(Mutex::new(Vec::new()));
        let hook_switches = Arc::clone(&switches);
        let hook: qf::ContextSwitchHook = Arc::new(move |prev, next| {
            hook_switches.lock().push((prev, next));
        });

        let scheduler = QkScheduler::new(None);
        scheduler.set_context_switch_hook(Some(hook));
        scheduler.configure_active(0, 0);
        scheduler.mark_ready(4);

        let decision = scheduler
            .plan_activation()
            .expect("priority 4 should be scheduled");
        scheduler.commit_activation(&decision, 2); // 0 -> 4
        scheduler.restore_active(0, 0); // 4 -> idle (0)

        let recorded = switches.lock();
        assert_eq!(recorded.as_slice(), &[(0, 4), (4, 0)]);
    }

    #[cfg(not(feature = "static-alloc"))]
    #[test]
    fn context_switch_hook_silent_when_priority_unchanged() {
        let switches = Arc::new(Mutex::new(Vec::new()));
        let hook_switches = Arc::clone(&switches);
        let hook: qf::ContextSwitchHook = Arc::new(move |prev, next| {
            hook_switches.lock().push((prev, next));
        });

        let scheduler = QkScheduler::new(None);
        scheduler.set_context_switch_hook(Some(hook));
        scheduler.configure_active(3, 3);
        // Restoring to the same active priority is not a context switch.
        scheduler.restore_active(3, 3);

        assert!(switches.lock().is_empty());
    }

    #[test]
    fn next_after_dispatch_respects_threshold() {
        let scheduler = QkScheduler::new(None);
        scheduler.configure_active(2, 4);
        scheduler.mark_ready(6);
        scheduler.mark_ready(3);

        let decision = scheduler
            .plan_activation()
            .expect("priority 6 should preempt");
        scheduler.commit_activation(&decision, 6);
        scheduler.mark_not_ready(6);

        assert!(
            scheduler.next_after_dispatch(4).is_none(),
            "threshold should block priority 3"
        );

        let follow_up = scheduler
            .next_after_dispatch(0)
            .expect("threshold cleared should allow scheduling");
        assert_eq!(follow_up.next_prio, 3);
        assert_eq!(follow_up.previous_prio, 6);
    }

    #[test]
    fn next_after_dispatch_respects_lock_ceiling() {
        let scheduler = QkScheduler::new(None);
        scheduler.configure_active(0, 0);
        scheduler.mark_ready(7);
        scheduler.mark_ready(5);

        let decision = scheduler
            .plan_activation()
            .expect("priority 7 should preempt");
        scheduler.commit_activation(&decision, 7);
        scheduler.mark_not_ready(7);

        let status = scheduler.lock(5);
        assert!(status.is_locked());
        assert!(
            scheduler.next_after_dispatch(0).is_none(),
            "lock ceiling should block priority 5"
        );

        scheduler.unlock(status);

        let follow_up = scheduler
            .next_after_dispatch(0)
            .expect("lock ceiling cleared should allow scheduling");
        assert_eq!(follow_up.next_prio, 5);
        assert_eq!(follow_up.previous_prio, 7);
    }

    #[test]
    fn has_ready_to_run_reflects_constraints() {
        let scheduler = QkScheduler::new(None);
        scheduler.configure_active(1, 3);
        scheduler.mark_ready(5);

        assert!(scheduler.has_ready_to_run());

        let lock_status = scheduler.lock(5);
        assert!(!scheduler.has_ready_to_run());

        scheduler.unlock(lock_status);
        scheduler.reset_ready();
        scheduler.mark_ready(2);
        assert!(!scheduler.has_ready_to_run());
    }
}
