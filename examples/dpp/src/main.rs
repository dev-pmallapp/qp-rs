//! Dining Philosophers Problem implemented on top of the Rust `qf` kernel.
//!
//! This example mirrors the reference application in
//! `scratch/qp-8.1.1/qpcpp/examples/posix-win32/dpp_comp`, showing how active
//! objects, the cooperative kernel, time events, and QS tracing integrate in
//! Rust.

use std::convert::TryInto;
use std::env;
use std::io::Read;
use std::net::TcpStream;
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

const QS_RX_INFO: u8 = 0;
const QS_RX_COMMAND: u8 = 1;
const QS_TARGET_DONE: u8 = 65;
const QS_RX_STATUS: u8 = 66;
const QS_FRAME_FLAG: u8 = 0x7E;
const QS_FRAME_ESCAPE: u8 = 0x7D;
const QS_FRAME_ESCAPE_XOR: u8 = 0x20;
const MAX_RX_FRAME: usize = 128;

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

fn build_kernel() -> (Arc<Kernel>, Vec<Arc<TimeEvent>>, Arc<PosixPort>) {
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

fn init_port() -> Arc<PosixPort> {
    let cmd_addr = env::var("QSPY_CMD_ADDR").unwrap_or_else(|_| "127.0.0.1:6601".to_string());
    let port = if let Ok(raw_addr) = env::var("QSPY_ADDR") {
        let addr = raw_addr.trim().to_string();
        match PosixPort::connect(&addr) {
            Ok(port) => {
                println!("QS tracing connected to tcp://{addr}");
                port
            }
            Err(err) => {
                eprintln!(
                    "failed to connect to qspy at {addr}: {err}; falling back to UDP default"
                );
                connect_udp_default()
            }
        }
    } else {
        connect_udp_default()
    };

    let port = Arc::new(port);
    start_command_channel(&cmd_addr, Arc::clone(&port));
    port
}

fn connect_udp_default() -> PosixPort {
    let udp_addr = env::var("QSPY_UDP_ADDR").unwrap_or_else(|_| "127.0.0.1:7701".to_string());
    match PosixPort::connect_udp(&udp_addr) {
        Ok(port) => {
            println!("QS tracing connected to udp://{udp_addr}");
            port
        }
        Err(err) => {
            eprintln!("failed to connect to qspy at {udp_addr}: {err}; falling back to stdout");
            PosixPort::new()
        }
    }
}

fn start_command_channel(addr: &str, port: Arc<PosixPort>) {
    let addr = addr.to_string();
    thread::spawn(move || loop {
        match TcpStream::connect(&addr) {
            Ok(stream) => {
                if let Err(err) = stream.set_nodelay(true) {
                    eprintln!("failed to configure QS command channel: {err}");
                }
                handle_command_stream(stream, Arc::clone(&port));
            }
            Err(err) => {
                eprintln!("failed to connect to QS command listener at {addr}: {err}");
            }
        }

        thread::sleep(Duration::from_secs(1));
    });
}

fn handle_command_stream(mut stream: TcpStream, port: Arc<PosixPort>) {
    if let Ok(peer) = stream.peer_addr() {
        println!("QS command channel connected to {peer}");
    }
    let mut buffer = [0u8; 128];
    let mut decoder = QsRxDecoder::new(port);
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                for &byte in &buffer[..count] {
                    decoder.ingest(byte);
                }
            }
            Err(err) => {
                eprintln!("QS command stream error: {err}");
                break;
            }
        }
    }
    if let Ok(peer) = stream.peer_addr() {
        println!("QS command channel from {peer} closed");
    }
}

struct QsRxDecoder {
    port: Arc<PosixPort>,
    frame: Vec<u8>,
    escaped: bool,
}

impl QsRxDecoder {
    fn new(port: Arc<PosixPort>) -> Self {
        Self {
            port,
            frame: Vec::with_capacity(32),
            escaped: false,
        }
    }

    fn ingest(&mut self, byte: u8) {
        if byte == QS_FRAME_FLAG {
            self.finish_frame();
            self.escaped = false;
            return;
        }

        if byte == QS_FRAME_ESCAPE {
            self.escaped = true;
            return;
        }

        let value = if self.escaped {
            self.escaped = false;
            byte ^ QS_FRAME_ESCAPE_XOR
        } else {
            byte
        };

        if self.frame.len() >= MAX_RX_FRAME {
            eprintln!("QS-RX frame exceeded {MAX_RX_FRAME} bytes; dropping");
            self.frame.clear();
            self.escaped = false;
            self.report_error(0x50);
            return;
        }

        self.frame.push(value);
    }

    fn finish_frame(&mut self) {
        if self.frame.is_empty() {
            return;
        }

        let mut frame = Vec::new();
        std::mem::swap(&mut frame, &mut self.frame);
        self.handle_frame(&frame);
    }

    fn handle_frame(&self, frame: &[u8]) {
        if frame.len() < 3 {
            self.report_error(0x50);
            return;
        }

        let (data, checksum_slice) = frame.split_at(frame.len() - 1);
        let checksum = checksum_slice[0];
        let mut sum: u8 = 0;
        for byte in data {
            sum = sum.wrapping_add(*byte);
        }

        if sum.wrapping_add(checksum) != 0xFF {
            eprintln!("QS-RX checksum mismatch (sum={sum:#04x}, checksum={checksum:#04x})");
            self.report_error(0x51);
            return;
        }

        if data.len() < 2 {
            self.report_error(0x52);
            return;
        }

        let record = data[1];
        let payload = &data[2..];

        match record {
            QS_RX_INFO => {
                if let Err(err) = self.port.emit_target_info(&TargetInfo::default()) {
                    eprintln!("failed to emit target info for QS-RX: {err}");
                }
            }
            QS_RX_COMMAND => {
                self.handle_command(payload);
            }
            other => {
                eprintln!("unsupported QS-RX record {other}");
                self.report_error(0x43);
            }
        }
    }

    fn handle_command(&self, payload: &[u8]) {
        if payload.len() < 13 {
            eprintln!("QS command payload too short: {} bytes", payload.len());
            self.report_error(0x50);
            return;
        }

        let cmd_id = payload[0];
        let param1 = u32::from_le_bytes(payload[1..5].try_into().expect("param1"));
        let param2 = u32::from_le_bytes(payload[5..9].try_into().expect("param2"));
        let param3 = u32::from_le_bytes(payload[9..13].try_into().expect("param3"));

        self.report_ack(QS_RX_COMMAND);
        self.dispatch_command(cmd_id, [param1, param2, param3]);
        self.report_done(QS_RX_COMMAND);
    }

    fn dispatch_command(&self, cmd_id: u8, params: [u32; 3]) {
        println!(
            "QS command received: id={cmd_id}, param1={}, param2={}, param3={}",
            params[0], params[1], params[2]
        );
    }

    fn report_ack(&self, rec_id: u8) {
        if let Err(err) = self.port.emit_record(QS_RX_STATUS, &[rec_id], false) {
            eprintln!("failed to emit QS-RX ack: {err}");
        }
    }

    fn report_done(&self, rec_id: u8) {
        if let Err(err) = self.port.emit_record(QS_TARGET_DONE, &[rec_id], true) {
            eprintln!("failed to emit QS target-done: {err}");
        }
    }

    fn report_error(&self, code: u8) {
        let payload = [0x80 | code];
        if let Err(err) = self.port.emit_record(QS_RX_STATUS, &payload, false) {
            eprintln!("failed to emit QS-RX error: {err}");
        }
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
