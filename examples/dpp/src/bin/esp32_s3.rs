#![cfg(feature = "esp32s3")]

use std::collections::VecDeque;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use esp_idf_sys as _;

use qf::active::{new_active_object, ActiveContext, ActiveObjectId, SignalHandler};
use qf::event::{DynEvent, DynPayload, Event, Signal};
use qf::time::{TimeEvent, TimeEventConfig};
use qf_port_esp32_s3::{Esp32S3Port, Esp32S3QkRuntime, PortConfig};
use qk::{QkKernel, QkKernelBuilder};

static KERNEL: OnceLock<Arc<QkKernel>> = OnceLock::new();

const N_PHILO: usize = 5;
const TABLE_ID: ActiveObjectId = ActiveObjectId::new(1);
const PHILO_BASE_ID: u8 = 2;

const EAT_SIG: Signal = Signal(4);
const DONE_SIG: Signal = Signal(5);
const TIMEOUT_SIG: Signal = Signal(10);
const HUNGRY_SIG: Signal = Signal(11);

fn main() -> ! {
    esp_idf_sys::link_patches();
    println!("DPP starting on ESP32-S3");

    let resources = build_application();

    let kernel = Arc::new(resources.builder.build().expect("kernel should build"));
    kernel.start();

    KERNEL
        .set(Arc::clone(&kernel))
        .expect("kernel already initialised");

    let mut config = PortConfig::new();
    config.tick_hz = 100;

    let port = Esp32S3Port::new();
    let mut runtime = Esp32S3QkRuntime::new(kernel, port, config);

    for event in &resources.timers {
        runtime.register_time_event(Arc::clone(event));
    }

    loop {
        runtime
            .tick()
            .expect("timer wheel should tick successfully");
        runtime.run_until_idle();
        thread::sleep(Duration::from_millis(10));
    }
}

struct ApplicationResources {
    builder: QkKernelBuilder,
    timers: Vec<Arc<TimeEvent>>,
}

fn build_application() -> ApplicationResources {
    let mut builder = QkKernel::builder();
    builder = builder.register(new_active_object(TABLE_ID, 6, Table::new()));

    let mut timers = Vec::with_capacity(N_PHILO);

    for index in 0..N_PHILO {
        let philo_id = ActiveObjectId::new(PHILO_BASE_ID + index as u8);
        let timer = Arc::new(TimeEvent::new(philo_id, TimeEventConfig::new(TIMEOUT_SIG)));
        timers.push(Arc::clone(&timer));

        let priority = 3 + index as u8;
        let philo = new_active_object(
            philo_id,
            priority,
            Philosopher::new(index, philo_id, Arc::clone(&timer)),
        );
        builder = builder.register(philo);
    }

    ApplicationResources { builder, timers }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PhiloState {
    Thinking,
    Hungry,
    Eating,
}

struct Philosopher {
    index: usize,
    id: ActiveObjectId,
    state: PhiloState,
    timer: Arc<TimeEvent>,
}

impl Philosopher {
    fn new(index: usize, id: ActiveObjectId, timer: Arc<TimeEvent>) -> Self {
        Self {
            index,
            id,
            state: PhiloState::Thinking,
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

    fn transition(&mut self, ctx: &mut ActiveContext, signal: Signal, target: PhiloState) {
        match (self.state, target) {
            (PhiloState::Thinking, PhiloState::Hungry) => {
                self.timer.disarm();
                self.post_table(HUNGRY_SIG);
                println!("Philosopher {} is hungry", self.index);
            }
            (PhiloState::Hungry, PhiloState::Eating) => {
                self.schedule_eat();
                println!("Philosopher {} starts eating", self.index);
            }
            (PhiloState::Eating, PhiloState::Thinking) => {
                self.timer.disarm();
                self.post_table(DONE_SIG);
                self.schedule_think();
                println!("Philosopher {} returns to thinking", self.index);
            }
            _ => {}
        }

        self.state = target;
    }
}

impl SignalHandler for Philosopher {
    fn on_start(&mut self, _ctx: &mut ActiveContext) {
        self.schedule_think();
        println!("Philosopher {} starts thinking", self.index);
    }

    fn handle_signal(&mut self, signal: Signal, ctx: &mut ActiveContext) {
        match (self.state, signal) {
            (PhiloState::Thinking, TIMEOUT_SIG) => {
                self.transition(ctx, signal, PhiloState::Hungry);
            }
            (PhiloState::Hungry, EAT_SIG) => {
                self.transition(ctx, signal, PhiloState::Eating);
            }
            (PhiloState::Eating, TIMEOUT_SIG) => {
                self.transition(ctx, signal, PhiloState::Thinking);
            }
            _ => {}
        }
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

impl SignalHandler for Table {
    fn on_start(&mut self, _ctx: &mut ActiveContext) {
        println!("Table active object initialised");
    }

    fn handle_signal(&mut self, signal: Signal, event: DynEvent, _ctx: &mut ActiveContext) {
        match signal {
            HUNGRY_SIG => {
                let payload = event.payload.clone();
                let msg = Arc::downcast::<TableMsg>(payload).expect("table message downcast");
                let index = msg.index;
                println!("Table: philosopher {} requests forks", index);
                if !self.try_grant(index) {
                    self.waiting.push_back(index);
                }
            }
            DONE_SIG => {
                let payload = event.payload.clone();
                let msg = Arc::downcast::<TableMsg>(payload).expect("table message downcast");
                let index = msg.index;
                println!("Table: philosopher {} releases forks", index);
                self.release_forks(index);

                let mut still_waiting = VecDeque::new();
                while let Some(waiting_index) = self.waiting.pop_front() {
                    if !self.try_grant(waiting_index) {
                        still_waiting.push_back(waiting_index);
                    }
                }
                self.waiting = still_waiting;
            }
            _ => {}
        }
    }
}

#[derive(Debug)]
struct TableMsg {
    index: usize,
}
