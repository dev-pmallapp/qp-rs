use std::sync::Mutex;

use qs::records::sched;
use qs::TraceHook;

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

#[derive(Default)]
struct State {
    lock_ceiling: u8,
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
        *self.trace.lock().expect("trace mutex poisoned") = trace;
    }

    pub fn lock(&self, ceiling: u8) -> SchedStatus {
        let mut state = self.state.lock().expect("scheduler mutex poisoned");
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
            let mut state = self.state.lock().expect("scheduler mutex poisoned");
            if state.lock_ceiling > value {
                let current = state.lock_ceiling;
                state.lock_ceiling = value;
                drop(state);
                self.emit_record(sched::UNLOCK, &[current, value], true);
            }
        }
    }

    fn emit_record(&self, record: u8, payload: &[u8], timestamp: bool) {
        let trace = self.trace.lock().expect("trace mutex poisoned").clone();

        if let Some(trace) = trace {
            if let Err(err) = trace(record, payload, timestamp) {
                eprintln!("failed to emit QK trace record {record}: {err}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn lock_unlock_sequence() {
        let records = Arc::new(Mutex::new(Vec::new()));
        let hook_records = Arc::clone(&records);
        let hook: TraceHook = Arc::new(move |id, payload, timestamp| {
            hook_records
                .lock()
                .unwrap()
                .push((id, payload.to_vec(), timestamp));
            Ok(())
        });

        let scheduler = QkScheduler::new(Some(hook));

        let status = scheduler.lock(5);
        assert_eq!(status, SchedStatus::Locked(0));

        scheduler.unlock(status);

        let recorded = records.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0], (sched::LOCK, vec![0, 5], true));
        assert_eq!(recorded[1], (sched::UNLOCK, vec![5, 0], true));
    }
}
