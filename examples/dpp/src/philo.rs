use crate::*;
use std::sync::Arc;
use rand::{rngs::SmallRng, Rng};
use qf::event::{DynEvent, DynPayload, Event};
use qf::hsm::reserved::*;
use qf::time::TimeEvent;
use qf::{q_handled, q_super, q_tran, QHsm, QHsmResult, Signal};
use qs::qutest::make_probe_record;
use qs::records::infra::TEST_PROBE as QS_TEST_PROBE_GET;

pub(crate) struct PhiloData {
    pub(crate) index: usize,
    pub(crate) name: &'static str,
    pub(crate) timer: Arc<TimeEvent>,
    pub(crate) rng: SmallRng,
}

impl PhiloData {
    fn think_ticks(&mut self) -> u64 {
        self.rng.gen_range(3..=6)
    }

    fn eat_ticks(&mut self) -> u64 {
        self.rng.gen_range(2..=5)
    }

    fn post_table(&self, signal: Signal) {
        if let Some(kernel) = KERNEL.get() {
            let payload: DynPayload = Arc::new(TableMsg::new(ActiveObjectId::new(PHILO_BASE_ID + self.index as u8)));
            let evt = Event::with_arc(signal, payload);
            let _ = kernel.post(TABLE_ID, evt);
        }
    }

    fn log_state(&self, state_str: &'static str) {
        println!("{} is {}", self.name, state_str);
        if let Some(port) = PORT.get() {
            let mut builder = UserRecordBuilder::with_capacity(self.name.len() + 8);
            builder.push_u8(1, self.index as u8);
            builder.push_str(state_str);
            let payload = builder.into_vec();
            let _ = port.emit_record(PHILO_STAT_RECORD, &payload, true);
        }
    }
}

pub(crate) fn philo_initial(_sm: &mut PhiloData, _e: &DynEvent) -> QHsmResult<PhiloData> {
    q_tran!(thinking)
}

pub(crate) fn thinking(sm: &mut PhiloData, e: &DynEvent) -> QHsmResult<PhiloData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => {
            let ticks = sm.think_ticks();
            sm.timer.arm(ticks, None);
            sm.log_state("thinking");
            q_handled!()
        }
        Q_EXIT_SIG_VAL => {
            sm.timer.disarm();
            q_handled!()
        }
        10 => { // TIMEOUT_SIG
            let tp_fn = thinking as *const () as usize as u64;
            if let Some(tp) = qs::qutest::take_test_probe(tp_fn) {
                if let Some(port) = PORT.get() {
                    let rec = make_probe_record(tp_fn, tp);
                    let _ = port.emit_record(QS_TEST_PROBE_GET, &rec, false);
                }
                if tp != 0 {
                    return q_handled!();
                }
            }
            q_tran!(hungry)
        }
        8 => { // TEST_SIG
            q_handled!()
        }
        _ => q_super!(QHsm::<PhiloData>::top_state),
    }
}

pub(crate) fn hungry(sm: &mut PhiloData, e: &DynEvent) -> QHsmResult<PhiloData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => {
            sm.post_table(HUNGRY_SIG);
            sm.log_state("hungry");
            q_handled!()
        }
        4 => { // EAT_SIG
            q_tran!(eating)
        }
        _ => q_super!(QHsm::<PhiloData>::top_state),
    }
}

pub(crate) fn eating(sm: &mut PhiloData, e: &DynEvent) -> QHsmResult<PhiloData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => {
            let ticks = sm.eat_ticks();
            sm.timer.arm(ticks, None);
            sm.log_state("eating");
            q_handled!()
        }
        Q_EXIT_SIG_VAL => {
            sm.timer.disarm();
            sm.post_table(DONE_SIG);
            q_handled!()
        }
        10 => { // TIMEOUT_SIG
            q_tran!(thinking)
        }
        _ => q_super!(QHsm::<PhiloData>::top_state),
    }
}
