#![no_std]
#![no_main]

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::main;

use qf::active::{new_active_object, ActiveObjectId};
use qf::event::{DynEvent, DynPayload, Event, Signal};
use qf::time::{TimeEvent, TimeEventConfig};
use qf::{qm_tran, qm_super, qm_handled, qm_ignored, QMsm, QMState, QMsmResult};
use qf_port_esp32_c6::{Esp32C6Port, Esp32C6QkRuntime, PortConfig};
use qk::{QkKernel, QkKernelBuilder};

#[cfg(feature = "qs")]
use qs;

static KERNEL: spin::Once<Arc<QkKernel>> = spin::Once::new();

const N_PHILO: usize = 5;
const TABLE_ID: ActiveObjectId = ActiveObjectId::new(1);
const PHILO_BASE_ID: u8 = 2;

const EAT_SIG: Signal = Signal(4);
const DONE_SIG: Signal = Signal(5);
const TIMEOUT_SIG: Signal = Signal(10);
const HUNGRY_SIG: Signal = Signal(11);

macro_rules! dpp_println {
    ($($arg:tt)*) => {
        #[cfg(not(feature = "qs"))]
        esp_println::println!($($arg)*);
    };
}

#[cfg(feature = "qs")]
struct EspPrintlnBackend;

#[cfg(feature = "qs")]
impl qs::TraceBackend for EspPrintlnBackend {
    fn write_frame(&self, frame: &[u8]) -> Result<(), qs::TraceError> {
        esp_println::Printer::write_bytes(frame);
        Ok(())
    }
}

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let _peripherals = esp_hal::init(config);
    esp_alloc::heap_allocator!(size: 72 * 1024);

    dpp_println!("DPP starting on ESP32-C6 (bare-metal)");

    let resources = build_application();

    let kernel = Arc::new(resources.builder.build().expect("kernel should build"));
    kernel.start();

    KERNEL.call_once(|| Arc::clone(&kernel));

    let mut config = PortConfig::new();
    config.tick_hz = 100;

    let port = Esp32C6Port::new();
    let mut runtime = Esp32C6QkRuntime::new(kernel, port, config);

    for event in &resources.timers {
        runtime.register_time_event(Arc::clone(event));
    }

    let delay = esp_hal::delay::Delay::new();

    loop {
        runtime
            .tick()
            .expect("timer wheel should tick successfully");
        runtime.run_until_idle();
        delay.delay_millis(10);
    }
}

struct ApplicationResources {
    builder: QkKernelBuilder,
    timers: Vec<Arc<TimeEvent>>,
}

fn build_application() -> ApplicationResources {
    let mut builder = QkKernel::builder();

    #[cfg(feature = "qs")]
    let mut builder = {
        let tracer = qs::Tracer::new(qs::QsConfig::default(), EspPrintlnBackend).into_handle();
        let payload = qs::predefined::target_info_payload(&qs::TargetInfo::default());
        let _ = tracer.emit(qs::predefined::TARGET_INFO, &payload);

        let _ = tracer.emit(qs::predefined::SIG_DICT, &qs::predefined::sig_dict_payload(EAT_SIG.0, 0, "EAT"));
        let _ = tracer.emit(qs::predefined::SIG_DICT, &qs::predefined::sig_dict_payload(DONE_SIG.0, 0, "DONE"));
        let _ = tracer.emit(qs::predefined::SIG_DICT, &qs::predefined::sig_dict_payload(TIMEOUT_SIG.0, 0, "TIMEOUT"));
        let _ = tracer.emit(qs::predefined::SIG_DICT, &qs::predefined::sig_dict_payload(HUNGRY_SIG.0, 0, "HUNGRY"));

        builder.with_trace_hook(tracer.hook())
    };

    let table_sm = QMsm::new(Table::new(), &TABLE_ACTIVE);
    builder = builder
        .register(new_active_object(TABLE_ID, 6, table_sm))
        .expect("table registration should succeed");

    let mut timers = Vec::with_capacity(N_PHILO);

    for index in 0..N_PHILO {
        let philo_id = ActiveObjectId::new(PHILO_BASE_ID + index as u8);
        let timer = TimeEvent::new(philo_id, TimeEventConfig::new(TIMEOUT_SIG));
        timers.push(Arc::clone(&timer));

        let priority = 3 + index as u8;
        let philo_data = Philosopher::new(index, philo_id, Arc::clone(&timer));
        let philo_sm = QMsm::new(philo_data, &PHILO_ACTIVE);
        let philo = new_active_object(
            philo_id,
            priority,
            philo_sm,
        );
        builder = builder
            .register(philo)
            .expect("philosopher registration should succeed");
    }

    ApplicationResources { builder, timers }
}

struct Philosopher {
    index: usize,
    id: ActiveObjectId,
    timer: Arc<TimeEvent>,
}

impl Philosopher {
    fn new(index: usize, id: ActiveObjectId, timer: Arc<TimeEvent>) -> Self {
        Self {
            index,
            id,
            timer,
        }
    }

    fn schedule_think(&self) {
        let ticks = 3 + (self.index as u64 % 3);
        self.timer.arm(ticks, None);
    }

    fn schedule_eat(&self) {
        let ticks = 2 + (self.index as u64 % 3);
        self.timer.arm(ticks, None);
    }

    fn post_table(&self, signal: Signal) {
        if let Some(kernel) = KERNEL.get() {
            let payload: DynPayload = Arc::new(TableMsg { index: self.index });
            let evt = Event::with_arc(signal, payload);
            let _ = kernel.post(TABLE_ID, evt);
        }
    }
}

static PHILO_ACTIVE: QMState<Philosopher> = QMState {
    superstate: None,
    state_handler: philo_active,
    entry_action: None,
    exit_action: None,
    init_action: Some(|_sm| Some(&PHILO_THINKING)),
};

static PHILO_THINKING: QMState<Philosopher> = QMState {
    superstate: Some(&PHILO_ACTIVE),
    state_handler: philo_thinking,
    entry_action: Some(|sm| {
        sm.schedule_think();
        dpp_println!("Philosopher {} starts thinking", sm.index);
    }),
    exit_action: Some(|sm| {
        sm.timer.disarm();
    }),
    init_action: None,
};

static PHILO_HUNGRY: QMState<Philosopher> = QMState {
    superstate: Some(&PHILO_ACTIVE),
    state_handler: philo_hungry,
    entry_action: Some(|sm| {
        sm.post_table(HUNGRY_SIG);
        dpp_println!("Philosopher {} is hungry", sm.index);
    }),
    exit_action: None,
    init_action: None,
};

static PHILO_EATING: QMState<Philosopher> = QMState {
    superstate: Some(&PHILO_ACTIVE),
    state_handler: philo_eating,
    entry_action: Some(|sm| {
        sm.schedule_eat();
        dpp_println!("Philosopher {} starts eating", sm.index);
    }),
    exit_action: Some(|sm| {
        sm.timer.disarm();
        sm.post_table(DONE_SIG);
        dpp_println!("Philosopher {} returns to thinking", sm.index);
    }),
    init_action: None,
};

fn philo_active(_sm: &mut Philosopher, _e: &DynEvent) -> QMsmResult<Philosopher> {
    qm_ignored!()
}

fn philo_thinking(sm: &mut Philosopher, e: &DynEvent) -> QMsmResult<Philosopher> {
    match e.signal() {
        TIMEOUT_SIG => qm_tran!(&PHILO_HUNGRY),
        _ => qm_super!(&PHILO_ACTIVE),
    }
}

fn philo_hungry(sm: &mut Philosopher, e: &DynEvent) -> QMsmResult<Philosopher> {
    match e.signal() {
        EAT_SIG => qm_tran!(&PHILO_EATING),
        _ => qm_super!(&PHILO_ACTIVE),
    }
}

fn philo_eating(sm: &mut Philosopher, e: &DynEvent) -> QMsmResult<Philosopher> {
    match e.signal() {
        TIMEOUT_SIG => qm_tran!(&PHILO_THINKING),
        _ => qm_super!(&PHILO_ACTIVE),
    }
}

struct Table {
    forks: [bool; N_PHILO],
    waiting: VecDeque<usize>,
}

impl Table {
    fn new() -> Self {
        Self {
            forks: [false; N_PHILO],
            waiting: VecDeque::new(),
        }
    }

    fn left(index: usize) -> usize {
        index
    }

    fn right(index: usize) -> usize {
        (index + 1) % N_PHILO
    }

    fn try_grant(&mut self, index: usize) -> bool {
        if !self.forks[Self::left(index)] && !self.forks[Self::right(index)] {
            self.forks[Self::left(index)] = true;
            self.forks[Self::right(index)] = true;
            let philo_id = ActiveObjectId::new(PHILO_BASE_ID + index as u8);
            if let Some(kernel) = KERNEL.get() {
                let _ = kernel.post(philo_id, DynEvent::empty_dyn(EAT_SIG));
            }
            true
        } else {
            false
        }
    }

    fn release_forks(&mut self, index: usize) {
        self.forks[Self::left(index)] = false;
        self.forks[Self::right(index)] = false;
    }
}

static TABLE_ACTIVE: QMState<Table> = QMState {
    superstate: None,
    state_handler: table_active,
    entry_action: None,
    exit_action: None,
    init_action: Some(|_sm| Some(&TABLE_SERVING)),
};

static TABLE_SERVING: QMState<Table> = QMState {
    superstate: Some(&TABLE_ACTIVE),
    state_handler: table_serving,
    entry_action: Some(|_sm| {
        dpp_println!("Table active object initialised");
    }),
    exit_action: None,
    init_action: None,
};

fn table_active(_sm: &mut Table, _e: &DynEvent) -> QMsmResult<Table> {
    qm_ignored!()
}

fn table_serving(sm: &mut Table, e: &DynEvent) -> QMsmResult<Table> {
    match e.signal() {
        HUNGRY_SIG => {
            let payload = e.payload.clone();
            let msg = Arc::downcast::<TableMsg>(payload).expect("table message downcast");
            let index = msg.index;
            dpp_println!("Table: philosopher {} requests forks", index);
            if !sm.try_grant(index) {
                sm.waiting.push_back(index);
            }
            qm_handled!()
        }
        DONE_SIG => {
            let payload = e.payload.clone();
            let msg = Arc::downcast::<TableMsg>(payload).expect("table message downcast");
            let index = msg.index;
            dpp_println!("Table: philosopher {} releases forks", index);
            sm.release_forks(index);

            let mut still_waiting = VecDeque::new();
            while let Some(waiting_index) = sm.waiting.pop_front() {
                if !sm.try_grant(waiting_index) {
                    still_waiting.push_back(waiting_index);
                }
            }
            sm.waiting = still_waiting;
            qm_handled!()
        }
        _ => qm_super!(&TABLE_ACTIVE),
    }
}

#[derive(Debug)]
struct TableMsg {
    index: usize,
}
