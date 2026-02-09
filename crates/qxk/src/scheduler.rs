//! Dual-mode scheduler for QXK.
//!
//! The scheduler manages both active objects and extended threads,
//! with active objects having priority over threads.
//!
//! ## Scheduling Policy
//!
//! 1. Active objects (event-driven) are checked first
//! 2. If no active objects are ready, extended threads are scheduled
//! 3. Within each category, highest priority executes first
//! 4. Active objects use run-to-completion semantics
//! 5. Extended threads can block and yield

use crate::sync::Mutex;
use crate::thread::{ThreadId, ThreadPriority};
use qf::TraceHook;

#[cfg(feature = "qs")]
use qs::records::sched;

#[cfg(not(feature = "qs"))]
mod sched {
    pub const LOCK: u8 = 0;
    pub const UNLOCK: u8 = 1;
    pub const NEXT: u8 = 2;
    pub const IDLE: u8 = 3;
}

// Custom QXK scheduler record for thread scheduling
const THREAD_NEXT: u8 = 100;

/// Scheduling mode indicates what type of entity should run next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleMode {
    /// An active object should run (priority-based preemption).
    ActiveObject { priority: u8 },
    /// An extended thread should run (blocking allowed).
    ExtendedThread { id: ThreadId, priority: ThreadPriority },
    /// No work is available, enter idle.
    Idle,
}

/// Scheduler lock status for nested locking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedStatus {
    Unlocked,
    Locked(u8),
}

impl SchedStatus {
    pub fn is_locked(self) -> bool {
        matches!(self, Self::Locked(_))
    }
}

/// 64-bit bitmap for O(1) priority tracking (active objects).
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

/// Thread ready queue (simple priority-based list).
#[derive(Default)]
struct ThreadReadyQueue {
    ready_threads: alloc::vec::Vec<(ThreadId, ThreadPriority)>,
}

impl ThreadReadyQueue {
    fn insert(&mut self, id: ThreadId, priority: ThreadPriority) {
        if !self.ready_threads.iter().any(|(tid, _)| *tid == id) {
            self.ready_threads.push((id, priority));
            // Keep sorted by priority (highest first)
            self.ready_threads.sort_by(|a, b| b.1.cmp(&a.1));
        }
    }

    fn remove(&mut self, id: ThreadId) {
        self.ready_threads.retain(|(tid, _)| *tid != id);
    }

    fn max(&self) -> Option<(ThreadId, ThreadPriority)> {
        self.ready_threads.first().copied()
    }

    fn is_empty(&self) -> bool {
        self.ready_threads.is_empty()
    }

    fn clear(&mut self) {
        self.ready_threads.clear();
    }
}

#[derive(Default)]
struct State {
    /// Lock ceiling for critical sections.
    lock_ceiling: u8,
    /// Active object ready set.
    ao_ready: ReadySet,
    /// Extended thread ready queue.
    thread_ready: ThreadReadyQueue,
    /// Currently executing active object priority (0 if idle).
    active_prio: u8,
    /// Currently executing thread (None if no thread running).
    active_thread: Option<ThreadId>,
}

/// Dual-mode scheduler for active objects and extended threads.
pub struct QxkScheduler {
    state: Mutex<State>,
    trace: Mutex<Option<TraceHook>>,
}

impl QxkScheduler {
    /// Creates a new QXK scheduler.
    pub fn new(trace: Option<TraceHook>) -> Self {
        Self {
            state: Mutex::new(State::default()),
            trace: Mutex::new(trace),
        }
    }

    /// Sets the trace hook for scheduler events.
    pub fn set_trace_hook(&self, trace: Option<TraceHook>) {
        *self.trace.lock() = trace;
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
            self.emit_trace(sched::LOCK, &[ceiling]);
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
                self.emit_trace(sched::UNLOCK, &[current, value]);
            }
        }
    }

    // Active Object Management

    /// Marks an active object priority as ready.
    pub fn mark_ao_ready(&self, prio: u8) {
        let mut state = self.state.lock();
        state.ao_ready.insert(prio);
    }

    /// Marks an active object priority as not ready.
    pub fn mark_ao_not_ready(&self, prio: u8) {
        let mut state = self.state.lock();
        state.ao_ready.remove(prio);
    }

    /// Checks if an active object priority is ready.
    pub fn is_ao_ready(&self, prio: u8) -> bool {
        let state = self.state.lock();
        state.ao_ready.contains(prio)
    }

    // Extended Thread Management

    /// Marks an extended thread as ready.
    pub fn mark_thread_ready(&self, id: ThreadId, priority: ThreadPriority) {
        let mut state = self.state.lock();
        state.thread_ready.insert(id, priority);
    }

    /// Marks an extended thread as not ready (blocked or terminated).
    pub fn mark_thread_not_ready(&self, id: ThreadId) {
        let mut state = self.state.lock();
        state.thread_ready.remove(id);
    }

    // Scheduling Decisions

    /// Plans the next scheduling decision using dual-mode policy.
    ///
    /// Active objects have priority over extended threads.
    pub fn plan_next(&self) -> ScheduleMode {
        let state = self.state.lock();

        // First check for ready active objects above lock ceiling
        if let Some(ao_prio) = state.ao_ready.max() {
            if ao_prio > state.lock_ceiling {
                self.emit_trace(sched::NEXT, &[ao_prio]);
                return ScheduleMode::ActiveObject { priority: ao_prio };
            }
        }

        // No active objects ready, check for threads
        if let Some((thread_id, thread_prio)) = state.thread_ready.max() {
            self.emit_trace(THREAD_NEXT, &[thread_id.0, thread_prio.0]);
            return ScheduleMode::ExtendedThread {
                id: thread_id,
                priority: thread_prio,
            };
        }

        // Nothing ready, idle
        self.emit_trace(sched::IDLE, &[]);
        ScheduleMode::Idle
    }

    /// Checks if there is any work ready to run.
    pub fn has_work(&self) -> bool {
        let state = self.state.lock();
        state.ao_ready.max().is_some() || !state.thread_ready.is_empty()
    }

    /// Sets the currently active priority/thread.
    pub fn set_active(&self, mode: ScheduleMode) {
        let mut state = self.state.lock();
        match mode {
            ScheduleMode::ActiveObject { priority } => {
                state.active_prio = priority;
                state.active_thread = None;
            }
            ScheduleMode::ExtendedThread { id, .. } => {
                state.active_prio = 0;
                state.active_thread = Some(id);
            }
            ScheduleMode::Idle => {
                state.active_prio = 0;
                state.active_thread = None;
            }
        }
    }

    /// Resets the scheduler state.
    pub fn reset(&self) {
        let mut state = self.state.lock();
        state.ao_ready.clear();
        state.thread_ready.clear();
        state.active_prio = 0;
        state.active_thread = None;
    }

    fn emit_trace(&self, record: u8, payload: &[u8]) {
        if let Some(ref trace) = *self.trace.lock() {
            let _ = trace(record, payload, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_set_operations() {
        let mut ready = ReadySet::default();
        assert_eq!(ready.max(), None);

        ready.insert(5);
        assert!(ready.contains(5));
        assert_eq!(ready.max(), Some(5));

        ready.insert(10);
        assert_eq!(ready.max(), Some(10));

        ready.remove(10);
        assert_eq!(ready.max(), Some(5));

        ready.clear();
        assert_eq!(ready.max(), None);
    }

    #[test]
    fn thread_queue_priority_order() {
        let mut queue = ThreadReadyQueue::default();

        queue.insert(ThreadId(1), ThreadPriority(5));
        queue.insert(ThreadId(2), ThreadPriority(10));
        queue.insert(ThreadId(3), ThreadPriority(3));

        // Highest priority should be first
        assert_eq!(queue.max(), Some((ThreadId(2), ThreadPriority(10))));

        queue.remove(ThreadId(2));
        assert_eq!(queue.max(), Some((ThreadId(1), ThreadPriority(5))));
    }

    #[test]
    fn dual_mode_scheduling() {
        let sched = QxkScheduler::new(None);

        // Initially idle
        assert!(matches!(sched.plan_next(), ScheduleMode::Idle));

        // Add active object
        sched.mark_ao_ready(5);
        assert!(matches!(
            sched.plan_next(),
            ScheduleMode::ActiveObject { priority: 5 }
        ));

        // Add thread (AO should still have priority)
        sched.mark_thread_ready(ThreadId(1), ThreadPriority(10));
        assert!(matches!(
            sched.plan_next(),
            ScheduleMode::ActiveObject { priority: 5 }
        ));

        // Remove AO, thread should run
        sched.mark_ao_not_ready(5);
        assert!(matches!(
            sched.plan_next(),
            ScheduleMode::ExtendedThread { id: ThreadId(1), .. }
        ));
    }

    #[test]
    fn lock_blocks_scheduling() {
        let sched = QxkScheduler::new(None);

        sched.mark_ao_ready(3);
        assert!(matches!(
            sched.plan_next(),
            ScheduleMode::ActiveObject { priority: 3 }
        ));

        let status = sched.lock(5);
        assert!(status.is_locked());

        // Priority 3 should be blocked by ceiling 5
        assert!(matches!(sched.plan_next(), ScheduleMode::Idle));

        sched.unlock(status);

        // Priority 3 should be available again
        assert!(matches!(
            sched.plan_next(),
            ScheduleMode::ActiveObject { priority: 3 }
        ));
    }
}
