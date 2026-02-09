use crate::sync::Mutex;
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

const SCHED_UNLOCKED: u8 = 0xFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedStatus {
    Locked(u8),
    Unlocked,
}

impl SchedStatus {
    pub fn from_raw(raw: u8) -> Self {
        if raw == SCHED_UNLOCKED {
            Self::Unlocked
        } else {
            Self::Locked(raw)
        }
    }

    pub fn to_raw(self) -> u8 {
        match self {
            Self::Locked(value) => value,
            Self::Unlocked => SCHED_UNLOCKED,
        }
    }

    pub fn is_locked(self) -> bool {
        matches!(self, Self::Locked(_))
    }
}

#[derive(Default, Clone, Copy)]
struct ReadySet {
    bits: u64,
}

impl ReadySet {
    fn insert(&mut self, prio: u8) {
        Self::assert_range(prio);
        self.bits |= 1u64 << prio;
    }

    fn remove(&mut self, prio: u8) {
        Self::assert_range(prio);
        self.bits &= !(1u64 << prio);
    }

    fn contains(&self, prio: u8) -> bool {
        Self::assert_range(prio);
        (self.bits & (1u64 << prio)) != 0
    }

    fn max(&self) -> Option<u8> {
        if self.bits == 0 {
            None
        } else {
            Some(63 - self.bits.leading_zeros() as u8)
        }
    }

    fn clear(&mut self) {
        self.bits = 0;
    }

    fn assert_range(prio: u8) {
        assert!(prio < 64, "priority {prio} exceeds supported range 0..63");
    }
}

#[derive(Default)]
struct State {
    lock_ceiling: u8,
    active_prio: u8,
    active_threshold: u8,
    next_prio: u8,
    ready: ReadySet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleDecision {
    pub next_prio: u8,
    pub previous_prio: u8,
}

pub struct QkScheduler {
    state: Mutex<State>,
    trace: Mutex<Option<TraceHook>>,
}

impl QkScheduler {
    pub fn new(trace: Option<TraceHook>) -> Self {
        Self {
            state: Mutex::new(State::default()),
            trace: Mutex::new(trace),
        }
    }

    pub fn set_trace_hook(&self, trace: Option<TraceHook>) {
        *self.trace.lock() = trace;
    }

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

    pub fn mark_ready(&self, prio: u8) {
        let mut state = self.state.lock();
        state.ready.insert(prio);
    }

    pub fn mark_not_ready(&self, prio: u8) {
        let mut state = self.state.lock();
        state.ready.remove(prio);
    }

    pub fn is_ready(&self, prio: u8) -> bool {
        let state = self.state.lock();
        state.ready.contains(prio)
    }

    pub fn reset_ready(&self) {
        let mut state = self.state.lock();
        state.ready.clear();
    }

    pub fn configure_active(&self, active_prio: u8, threshold: u8) {
        let mut state = self.state.lock();
        state.active_prio = active_prio;
        state.active_threshold = threshold;
    }

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

    pub fn has_ready_to_run(&self) -> bool {
        let state = self.state.lock();
        matches!(state.ready.max(), Some(prio) if prio > state.active_threshold && prio > state.lock_ceiling)
    }

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
        }
    }

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
            }
        } else if prio != previous {
            self.emit_record(sched::NEXT, &[prio, previous], true);
        }
    }

    pub fn preemption_candidate(&self) -> Option<u8> {
        self.plan_activation().map(|decision| decision.next_prio)
    }

    pub fn next_priority(&self) -> u8 {
        self.state.lock().next_prio
    }

    pub fn current_priority(&self) -> u8 {
        self.state.lock().active_prio
    }

    fn emit_record(&self, record: u8, payload: &[u8], timestamp: bool) {
        let trace = self.trace.lock().clone();

        if let Some(trace) = trace {
            let _ = trace(record, payload, timestamp);
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
