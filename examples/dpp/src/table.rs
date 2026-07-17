use crate::*;
use qf::active::ActiveObjectId;
use qf::event::DynEvent;
use qf::hsm::reserved::*;
use qf::{q_handled, q_super, q_tran, QHsm, QHsmResult};

#[derive(Clone, Copy, Debug)]
pub(crate) struct TableMsg {
    pub(crate) philo: ActiveObjectId,
}

impl TableMsg {
    pub(crate) fn new(philo: ActiveObjectId) -> Self {
        Self { philo }
    }
}

pub(crate) struct TableData {
    forks: [bool; N_PHILO],
    pub(crate) hungry: [bool; N_PHILO],
}

impl TableData {
    pub(crate) fn new() -> Self {
        Self {
            forks: [true; N_PHILO],
            hungry: [false; N_PHILO],
        }
    }

    fn left(n: usize) -> usize {
        (n + 1) % N_PHILO
    }

    fn right(n: usize) -> usize {
        (n + N_PHILO - 1) % N_PHILO
    }

    fn can_eat(&self, idx: usize) -> bool {
        self.forks[idx] && self.forks[Self::left(idx)]
    }

    fn take_forks(&mut self, idx: usize) {
        self.forks[idx] = false;
        self.forks[Self::left(idx)] = false;
    }

    fn release_forks(&mut self, idx: usize) {
        self.forks[idx] = true;
        self.forks[Self::left(idx)] = true;
    }

    fn grant_forks(&mut self, idx: usize, prefix: &str) {
        self.take_forks(idx);
        self.hungry[idx] = false;
        println!("{} {}", prefix, NAMES[idx]);
        if let Some(kernel) = KERNEL.get() {
            let target = ActiveObjectId::new(PHILO_BASE_ID + idx as u8);
            let _ = kernel.post(target, DynEvent::empty_dyn(EAT_SIG));
        }
    }

    fn handle_hungry(&mut self, idx: usize) {
        if self.can_eat(idx) {
            self.grant_forks(idx, "Table grants forks to");
        } else {
            self.hungry[idx] = true;
            println!("{} waits for forks", NAMES[idx]);
        }
    }

    fn handle_done(&mut self, idx: usize) {
        self.release_forks(idx);
        println!("{} is done eating", NAMES[idx]);

        let right = Self::right(idx);
        if self.hungry[right] && self.can_eat(right) {
            self.grant_forks(right, "Table now serves");
        }

        let left = Self::left(idx);
        if self.hungry[left] && self.can_eat(left) {
            self.grant_forks(left, "Table now serves");
        }
    }

    fn handle_paused_done(&mut self, idx: usize) {
        self.release_forks(idx);
        println!("{} is done eating", NAMES[idx]);
    }

    fn msg_index(&self, msg: &TableMsg) -> usize {
        (msg.philo.0 - PHILO_BASE_ID) as usize
    }
}

pub(crate) fn table_initial(_sm: &mut TableData, _e: &DynEvent) -> QHsmResult<TableData> {
    q_tran!(serving)
}

pub(crate) fn active(_sm: &mut TableData, e: &DynEvent) -> QHsmResult<TableData> {
    match e.signal().0 {
        Q_INIT_SIG_VAL => {
            q_tran!(serving)
        }
        8 => { // TEST_SIG
            q_handled!()
        }
        _ => q_super!(QHsm::<TableData>::top_state),
    }
}

pub(crate) fn serving(sm: &mut TableData, e: &DynEvent) -> QHsmResult<TableData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => {
            println!("Table is ready");
            for idx in 0..N_PHILO {
                if sm.hungry[idx] && sm.can_eat(idx) {
                    sm.grant_forks(idx, "Table now serves");
                }
            }
            q_handled!()
        }
        11 => { // HUNGRY_SIG
            if let Some(msg) = e.payload.as_ref().downcast_ref::<TableMsg>() {
                let idx = sm.msg_index(msg);
                sm.handle_hungry(idx);
            }
            q_handled!()
        }
        5 => { // DONE_SIG
            if let Some(msg) = e.payload.as_ref().downcast_ref::<TableMsg>() {
                let idx = sm.msg_index(msg);
                sm.handle_done(idx);
            }
            q_handled!()
        }
        6 => { // PAUSE_SIG
            q_tran!(paused)
        }
        _ => q_super!(active),
    }
}

pub(crate) fn paused(sm: &mut TableData, e: &DynEvent) -> QHsmResult<TableData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => {
            println!("Table paused");
            if let Some(port) = PORT.get() {
                let _ = port.emit_record(PAUSED_STAT_RECORD, &[1], true);
            }
            q_handled!()
        }
        Q_EXIT_SIG_VAL => {
            println!("Table resumed");
            if let Some(port) = PORT.get() {
                let _ = port.emit_record(PAUSED_STAT_RECORD, &[0], true);
            }
            q_handled!()
        }
        7 => { // SERVE_SIG
            q_tran!(serving)
        }
        11 => { // HUNGRY_SIG
            if let Some(msg) = e.payload.as_ref().downcast_ref::<TableMsg>() {
                let idx = sm.msg_index(msg);
                sm.hungry[idx] = true;
                println!("{} waits for forks", NAMES[idx]);
            }
            q_handled!()
        }
        5 => { // DONE_SIG
            if let Some(msg) = e.payload.as_ref().downcast_ref::<TableMsg>() {
                let idx = sm.msg_index(msg);
                sm.handle_paused_done(idx);
            }
            q_handled!()
        }
        _ => q_super!(active),
    }
}
