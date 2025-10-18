//! Dining Philosophers Problem implemented on top of the Rust `qf` kernel.
//!
//! This example mirrors the reference application in
//! `scratch/qp-8.1.1/qpcpp/examples/posix-win32/dpp_comp`, showing how active
//! objects, the cooperative kernel, time events, and QS tracing integrate in
//! Rust.

use std::env;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use rand::{rngs::SmallRng, Rng, SeedableRng};

use qf::active::{new_active_object, ActiveBehavior, ActiveContext};
use qf::event::{DynEvent, DynPayload, Event};
use qf::kernel::{Kernel, KernelError};
use qf::time::{TimeEvent, TimeEventConfig, TimerWheel};
use qf::{ActiveObjectId, Signal, TraceError};
use qf_port_posix::PosixPort;
use qs::{TargetInfo, UserRecordBuilder};

const N_PHILO: usize = 5;
const TABLE_ID: ActiveObjectId = ActiveObjectId::new(1);
const PHILO_BASE_ID: u8 = 2;

const EAT_SIG: Signal = Signal(4);
const DONE_SIG: Signal = Signal(5);
const PAUSE_SIG: Signal = Signal(6);
const SERVE_SIG: Signal = Signal(7);
const TEST_SIG: Signal = Signal(8);
const TIMEOUT_SIG: Signal = Signal(10);
const HUNGRY_SIG: Signal = Signal(11);

const PHILO_STAT_RECORD: u8 = 100;
const PAUSED_STAT_RECORD: u8 = 101;

static KERNEL: OnceLock<Arc<Kernel>> = OnceLock::new();

static NAMES: [&str; N_PHILO] = ["Aristotle", "Kant", "Spinoza", "Marx", "Russell"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PhiloState {
    Thinking,
    Hungry,
    Eating,
}

impl PhiloState {
    fn as_str(&self) -> &'static str {
        match self {
            PhiloState::Thinking => "thinking",
            PhiloState::Hungry => "hungry",
            PhiloState::Eating => "eating",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TableMsg {
    philo: ActiveObjectId,
}

impl TableMsg {
    fn new(philo: ActiveObjectId) -> Self {
        Self { philo }
    }
}

struct Philosopher {
    index: usize,
    name: &'static str,
    state: PhiloState,
    timer: Arc<TimeEvent>,
    rng: SmallRng,
}

impl Philosopher {
    fn new(index: usize, timer: Arc<TimeEvent>) -> Self {
        Self {
            index,
            name: NAMES[index],
            state: PhiloState::Thinking,
            timer,
            rng: SmallRng::seed_from_u64(index as u64 + 1),
        }
    }

    fn id(&self) -> ActiveObjectId {
        ActiveObjectId::new(PHILO_BASE_ID + self.index as u8)
    }

    fn think_ticks(&mut self) -> u64 {
        self.rng.gen_range(3..=6)
    }

    fn eat_ticks(&mut self) -> u64 {
        self.rng.gen_range(2..=5)
    }

    fn schedule_think(&mut self) {
        let ticks = self.think_ticks();
        self.timer.arm(ticks, None);
    }

    fn schedule_eat(&mut self) {
        let ticks = self.eat_ticks();
        self.timer.arm(ticks, None);
    }

    fn post_table(&self, signal: Signal) {
        if let Some(kernel) = KERNEL.get() {
            let payload: DynPayload = Arc::new(TableMsg::new(self.id()));
            let evt = Event::with_arc(signal, payload);
            let _ = kernel.post(TABLE_ID, evt);
        }
    }

    fn log_state(&self, ctx: &mut ActiveContext, state: PhiloState) {
        println!("{} is {:?}", self.name, state);
        let mut builder = UserRecordBuilder::with_capacity(self.name.len() + 8);
        builder.push_u8(1, self.index as u8);
        builder.push_str(state.as_str());
        let payload = builder.into_vec();
        let _ = ctx.emit_trace(PHILO_STAT_RECORD, &payload);
    }
}

impl ActiveBehavior for Philosopher {
    fn on_start(&mut self, ctx: &mut ActiveContext) {
        self.state = PhiloState::Thinking;
        self.log_state(ctx, self.state);
        self.schedule_think();
    }

    fn on_event(&mut self, ctx: &mut ActiveContext, event: DynEvent) {
        match event.signal() {
            TIMEOUT_SIG => match self.state {
                PhiloState::Thinking => {
                    self.state = PhiloState::Hungry;
                    self.log_state(ctx, self.state);
                    self.post_table(HUNGRY_SIG);
                }
                PhiloState::Eating => {
                    self.state = PhiloState::Thinking;
                    self.log_state(ctx, self.state);
                    self.post_table(DONE_SIG);
                    self.schedule_think();
                }
                PhiloState::Hungry => {}
            },
            EAT_SIG => {
                if self.state == PhiloState::Hungry {
                    self.state = PhiloState::Eating;
                    self.log_state(ctx, self.state);
                    self.schedule_eat();
                }
            }
            _ => {}
        }
    }
}

struct TableState {
    forks: [bool; N_PHILO],
    hungry: [bool; N_PHILO],
}

impl TableState {
    fn new() -> Self {
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
}

impl ActiveBehavior for TableState {
    fn on_start(&mut self, _ctx: &mut ActiveContext) {
        println!("Table is ready");
    }

    fn on_event(&mut self, _ctx: &mut ActiveContext, event: DynEvent) {
        match event.signal() {
            HUNGRY_SIG => {
                if let Some(msg) = event.payload.as_ref().downcast_ref::<TableMsg>() {
                    let idx = (msg.philo.0 - PHILO_BASE_ID) as usize;
                    if self.can_eat(idx) {
                        self.take_forks(idx);
                        self.hungry[idx] = false;
                        println!("Table grants forks to {}", NAMES[idx]);
                        if let Some(kernel) = KERNEL.get() {
                            let _ = kernel.post(msg.philo, DynEvent::empty_dyn(EAT_SIG));
                        }
                    } else {
                        self.hungry[idx] = true;
                        println!("{} waits for forks", NAMES[idx]);
                    }
                }
            }
            DONE_SIG => {
                if let Some(msg) = event.payload.as_ref().downcast_ref::<TableMsg>() {
                    let idx = (msg.philo.0 - PHILO_BASE_ID) as usize;
                    self.release_forks(idx);
                    println!("{} is done eating", NAMES[idx]);

                    let right = Self::right(idx);
                    if self.hungry[right] && self.can_eat(right) {
                        self.take_forks(right);
                        self.hungry[right] = false;
                        let target = ActiveObjectId::new(PHILO_BASE_ID + right as u8);
                        println!("Table now serves {}", NAMES[right]);
                        if let Some(kernel) = KERNEL.get() {
                            let _ = kernel.post(target, DynEvent::empty_dyn(EAT_SIG));
                        }
                    }

                    let left = Self::left(idx);
                    if self.hungry[left] && self.can_eat(left) {
                        self.take_forks(left);
                        self.hungry[left] = false;
                        let target = ActiveObjectId::new(PHILO_BASE_ID + left as u8);
                        println!("Table now serves {}", NAMES[left]);
                        if let Some(kernel) = KERNEL.get() {
                            let _ = kernel.post(target, DynEvent::empty_dyn(EAT_SIG));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn build_kernel() -> (Arc<Kernel>, Vec<Arc<TimeEvent>>, PosixPort) {
    let port = init_port();
    let mut builder = Kernel::builder().with_trace_hook(port.trace_hook());

    builder = builder.register(new_active_object(TABLE_ID, 10, TableState::new()));

    let mut timers = Vec::new();
    for index in 0..N_PHILO {
        let id = ActiveObjectId::new(PHILO_BASE_ID + index as u8);
        let timer = TimeEvent::new(id, TimeEventConfig::new(TIMEOUT_SIG));
        timers.push(Arc::clone(&timer));
        let behavior = Philosopher::new(index, timer);
        builder = builder.register(new_active_object(id, (index + 1) as u8, behavior));
    }

    let kernel = Arc::new(builder.build());
    (kernel, timers, port)
}

fn init_port() -> PosixPort {
    if let Ok(raw_addr) = env::var("QSPY_ADDR") {
        let addr = raw_addr.trim().to_string();
        match PosixPort::connect(&addr) {
            Ok(port) => {
                println!("QS tracing connected to tcp://{addr}");
                port
            }
            Err(err) => {
                eprintln!("failed to connect to qspy at {addr}: {err}; falling back to stdout");
                PosixPort::new()
            }
        }
    } else {
        PosixPort::new()
    }
}

fn emit_reference_dictionary(port: &PosixPort) -> Result<(), TraceError> {
    const QS_RX_ADDR: u64 = 0x0000_0000_0041_0900;
    const CLOCK_TICK_ADDR: u64 = 0x0000_0000_0040_C358;
    const EVT_POOL_ADDR: u64 = 0x0000_0000_0041_0500;
    const TABLE_ADDR: u64 = 0x0000_0000_0041_04A0;
    const QHSM_TOP_ADDR: u64 = 0x0000_0000_0040_2A56;
    const PHILO_FUNS: &[(u64, &str)] = &[
        (0x0000_0000_0040_2854, "Philo::initial"),
        (0x0000_0000_0040_28EE, "Philo::thinking"),
        (0x0000_0000_0040_296A, "Philo::hungry"),
        (0x0000_0000_0040_2A20, "Philo::eating"),
    ];
    const TABLE_FUNS: &[(u64, &str)] = &[
        (0x0000_0000_0040_2C2A, "Table::active"),
        (0x0000_0000_0040_3022, "Table::serving"),
        (0x0000_0000_0040_31A0, "Table::paused"),
    ];
    const PHILO_OBJECTS: &[(u64, u64)] = &[
        (0x0000_0000_0041_0340, 0x0000_0000_0041_0358),
        (0x0000_0000_0041_0380, 0x0000_0000_0041_0398),
        (0x0000_0000_0041_03C0, 0x0000_0000_0041_03D8),
        (0x0000_0000_0041_0400, 0x0000_0000_0041_0418),
        (0x0000_0000_0041_0440, 0x0000_0000_0041_0458),
    ];

    port.emit_target_info(&TargetInfo::default())?;
    port.emit_obj_dict(QS_RX_ADDR, "QS_RX")?;
    port.emit_obj_dict(CLOCK_TICK_ADDR, "l_clock_tick")?;
    port.emit_usr_dict(PHILO_STAT_RECORD, "PHILO_STAT")?;
    port.emit_usr_dict(PAUSED_STAT_RECORD, "PAUSED_STAT")?;

    for (signal, name) in [
        (EAT_SIG, "EAT_SIG"),
        (DONE_SIG, "DONE_SIG"),
        (PAUSE_SIG, "PAUSE_SIG"),
        (SERVE_SIG, "SERVE_SIG"),
        (TEST_SIG, "TEST_SIG"),
        (TIMEOUT_SIG, "TIMEOUT_SIG"),
        (HUNGRY_SIG, "HUNGRY_SIG"),
    ] {
        port.emit_sig_dict(signal.0, 0, name)?;
    }

    port.emit_obj_dict(EVT_POOL_ADDR, "EvtPool1")?;
    port.emit_fun_dict(QHSM_TOP_ADDR, "QP::QHsm::top")?;
    port.emit_obj_dict(TABLE_ADDR, "Table::inst")?;

    for (idx, (philo_addr, timer_addr)) in PHILO_OBJECTS.iter().enumerate() {
        let inst_name = format!("Philo::inst[{idx}]");
        port.emit_obj_dict(*philo_addr, &inst_name)?;
        let timer_name = format!("Philo::inst[{idx}].m_timeEvt");
        port.emit_obj_dict(*timer_addr, &timer_name)?;
    }

    for (addr, name) in PHILO_FUNS {
        port.emit_fun_dict(*addr, name)?;
    }

    for (addr, name) in TABLE_FUNS {
        port.emit_fun_dict(*addr, name)?;
    }

    Ok(())
}

fn main() -> Result<(), KernelError> {
    println!("Starting Dining Philosophers demo");

    let (kernel, timers, port) = build_kernel();
    emit_reference_dictionary(&port).map_err(KernelError::from)?;
    let _port = port;
    if KERNEL.set(Arc::clone(&kernel)).is_err() {
        panic!("kernel already initialised");
    }
    kernel.start();

    let mut wheel = TimerWheel::new(Arc::clone(&kernel));
    for timer in timers {
        wheel.register(timer);
    }

    const TICKS: usize = 60;
    for _ in 0..TICKS {
        wheel.tick().expect("timer tick failed");
        kernel.run_until_idle();
        thread::sleep(Duration::from_millis(200));
    }

    Ok(())
}
