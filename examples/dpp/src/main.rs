//! Dining Philosophers Problem implemented on top of the Rust `qk` preemptive kernel.
//!
//! This example mirrors the reference application in
//! `scratch/qp-8.1.1/qpcpp/examples/posix-win32/dpp_comp`, showing how active
//! objects, the preemptive kernel, time events, and QS tracing integrate in
//! Rust using the QHsm framework.

use std::env;
use std::error::Error;
use std::io::Read;
use std::net::TcpStream;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use rand::{rngs::SmallRng, Rng, SeedableRng};

use qf::active::{new_active_object, ActiveContext, ActiveObjectId};
use qf::event::{DynEvent, DynPayload, Event};
use qf::hsm::reserved::*;
use qf::time::{TimeEvent, TimeEventConfig, TimeEventTraceInfo};
use qf::{q_handled, q_super, q_tran, QHsm, QHsmResult, Signal, TraceError, Q};
use qf_port_posix::{PosixPort, PosixQkRuntime};
use qk::{QkKernel, QkKernelError};
use qs::qutest::make_probe_record;
use qs::records::infra::TEST_PROBE as QS_TEST_PROBE_GET;
use qs::rx::{cmd as rx_cmd, RxCmd, RxParser};
use qs::{clear_test_probes, set_test_probe, GlbFilter, TargetInfo, UserRecordBuilder};

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

const DEFAULT_TICK_RATE: u8 = 0;

const QS_RX_NAME: &str = "QS_RX";
const CLOCK_TICK_NAME: &str = "l_clock_tick";
const EVT_POOL_NAME: &str = "EvtPool1";
const QHSM_TOP_NAME: &str = "QP::QHsm::top";
const TABLE_OBJECT_NAME: &str = "Table::inst";
const PHILO_INITIAL_NAME: &str = "Philo::initial";
const PHILO_THINKING_NAME: &str = "Philo::thinking";
const PHILO_HUNGRY_NAME: &str = "Philo::hungry";
const PHILO_EATING_NAME: &str = "Philo::eating";
const TABLE_ACTIVE_NAME: &str = "Table::active";
const TABLE_SERVING_NAME: &str = "Table::serving";
const TABLE_PAUSED_NAME: &str = "Table::paused";

const QS_TARGET_DONE: u8 = 65;
const QS_RX_STATUS: u8 = 66;

static KERNEL: OnceLock<Arc<QkKernel>> = OnceLock::new();
static PORT: OnceLock<Arc<PosixPort>> = OnceLock::new();

static NAMES: [&str; N_PHILO] = ["Aristotle", "Kant", "Spinoza", "Marx", "Russell"];

fn dict_handle(name: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for byte in name.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[derive(Clone)]
struct PhiloTraceEntry {
    index: usize,
    object_name: String,
    object_handle: u64,
    timer_name: String,
    timer_handle: u64,
}

impl PhiloTraceEntry {
    fn new(index: usize) -> Self {
        let object_name = format!("Philo::inst[{index}]");
        let timer_name = format!("Philo::inst[{index}].m_timeEvt");
        let object_handle = dict_handle(&object_name);
        let timer_handle = dict_handle(&timer_name);
        Self {
            index,
            object_name,
            object_handle,
            timer_name,
            timer_handle,
        }
    }
}

fn philo_trace_entries() -> &'static [PhiloTraceEntry] {
    static PHILO_TRACE: OnceLock<Vec<PhiloTraceEntry>> = OnceLock::new();
    PHILO_TRACE.get_or_init(|| (0..N_PHILO).map(PhiloTraceEntry::new).collect());
    PHILO_TRACE
        .get()
        .expect("philosopher trace entries initialised")
        .as_slice()
}

// ── Philosopher HSM Data & State Handlers ─────────────────────────────────────

struct PhiloData {
    index: usize,
    name: &'static str,
    timer: Arc<TimeEvent>,
    rng: SmallRng,
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

fn philo_initial(_sm: &mut PhiloData, _e: &DynEvent) -> QHsmResult<PhiloData> {
    q_tran!(thinking)
}

fn thinking(sm: &mut PhiloData, e: &DynEvent) -> QHsmResult<PhiloData> {
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
            let tp_fn = thinking as usize as u64;
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

fn hungry(sm: &mut PhiloData, e: &DynEvent) -> QHsmResult<PhiloData> {
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

fn eating(sm: &mut PhiloData, e: &DynEvent) -> QHsmResult<PhiloData> {
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

// ── Table HSM Data & State Handlers ───────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
struct TableMsg {
    philo: ActiveObjectId,
}

impl TableMsg {
    fn new(philo: ActiveObjectId) -> Self {
        Self { philo }
    }
}

struct TableData {
    forks: [bool; N_PHILO],
    hungry: [bool; N_PHILO],
}

impl TableData {
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

fn table_initial(_sm: &mut TableData, _e: &DynEvent) -> QHsmResult<TableData> {
    q_tran!(serving)
}

fn active(sm: &mut TableData, e: &DynEvent) -> QHsmResult<TableData> {
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

fn serving(sm: &mut TableData, e: &DynEvent) -> QHsmResult<TableData> {
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

fn paused(sm: &mut TableData, e: &DynEvent) -> QHsmResult<TableData> {
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

// ── Wiring & Runtime ─────────────────────────────────────────────────────────

fn build_runtime() -> Result<(PosixQkRuntime, Arc<PosixPort>), QkKernelError> {
    let port = init_port();
    let mut builder = QkKernel::builder();

    let table_hsm = QHsm::new(TableData::new(), table_initial);
    builder = builder
        .register(new_active_object(TABLE_ID, 10, table_hsm))?;

    let mut timers = Vec::new();
    for entry in philo_trace_entries() {
        let id = ActiveObjectId::new(PHILO_BASE_ID + entry.index as u8);
        let timer = TimeEvent::new(id, TimeEventConfig::new(TIMEOUT_SIG));
        timer.set_trace_meta(TimeEventTraceInfo {
            time_event_addr: entry.timer_handle,
            target_addr: entry.object_handle,
            tick_rate: DEFAULT_TICK_RATE,
        });
        timers.push(Arc::clone(&timer));
        
        let philo_hsm = QHsm::new(
            PhiloData {
                index: entry.index,
                name: NAMES[entry.index],
                timer: Arc::clone(&timer),
                rng: SmallRng::seed_from_u64(entry.index as u64 + 1),
            },
            philo_initial,
        );
        
        builder = builder
            .register(new_active_object(id, (entry.index + 1) as u8, philo_hsm))?;
    }

    let mut runtime = PosixQkRuntime::with_port(builder, &port)?;
    for timer in timers {
        runtime.register_time_event(timer);
    }

    Ok((runtime, port))
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
    PORT.set(Arc::clone(&port)).unwrap_or_else(|_| panic!("port already set"));
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
    let mut ctx = QsRxContext::new(port);
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                for &byte in &buffer[..count] {
                    ctx.ingest(byte);
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

struct QsRxContext {
    port:   Arc<PosixPort>,
    parser: RxParser,
}

impl QsRxContext {
    fn new(port: Arc<PosixPort>) -> Self {
        Self { port, parser: RxParser::new() }
    }

    fn ingest(&mut self, byte: u8) {
        if let Some(cmd) = self.parser.push(byte) {
            self.handle_cmd(cmd);
        }
    }

    fn handle_cmd(&self, cmd: RxCmd) {
        match cmd {
            RxCmd::Info => {
                if let Err(err) = self.port.emit_target_info(&TargetInfo::default()) {
                    eprintln!("QS-RX INFO error: {err}");
                }
            }
            RxCmd::Reset => {
                eprintln!("QS-RX RESET (not implemented in this demo)");
            }
            RxCmd::Command { id, p1, p2, p3 } => {
                self.ack(rx_cmd::COMMAND);
                println!("QS command id={id} params=[{p1}, {p2}, {p3}]");
                self.done(rx_cmd::COMMAND);
            }
            RxCmd::TestSetup => {
                clear_test_probes();
                self.ack_done(rx_cmd::TEST_SETUP);
            }
            RxCmd::TestTeardown => {
                clear_test_probes();
                self.ack_done(rx_cmd::TEST_TEARDOWN);
            }
            RxCmd::TestContinue => {
                self.ack_done(rx_cmd::TEST_CONTINUE);
            }
            RxCmd::TestProbe { fn_ptr, data } => {
                set_test_probe(fn_ptr, data);
                self.ack_done(rx_cmd::TEST_PROBE);
            }
            RxCmd::Event { prio, signal, .. } => {
                if let Some(kernel) = KERNEL.get() {
                    let _ = kernel.post(
                        ActiveObjectId::new(prio),
                        DynEvent::empty_dyn(Signal(signal)),
                    );
                }
                self.ack_done(rx_cmd::EVENT);
            }
            RxCmd::GlbFilter { bits } => {
                self.port.set_filter(GlbFilter::from_bytes(bits));
                self.ack_done(rx_cmd::GLB_FILTER);
            }
            RxCmd::Tick { .. }       => self.ack_done(rx_cmd::TICK),
            RxCmd::AoFilter { .. }   => self.ack_done(rx_cmd::AO_FILTER),
            RxCmd::LocFilter { .. }  => self.ack_done(rx_cmd::LOC_FILTER),
            RxCmd::CurrObj { .. }    => self.ack_done(rx_cmd::CURR_OBJ),
            RxCmd::QueryCurr { .. }  => self.ack_done(rx_cmd::QUERY_CURR),
            RxCmd::Peek { .. }       => self.ack_done(rx_cmd::PEEK),
            RxCmd::Poke { .. }       => self.ack_done(rx_cmd::POKE),
            RxCmd::Fill { .. }       => self.ack_done(rx_cmd::FILL),
            RxCmd::Unknown { cmd, .. } => {
                eprintln!("unknown QS-RX record {cmd:#04x}");
                let _ = self.port.emit_record(QS_RX_STATUS, &[0x80 | 0x43u8], false);
            }
        }
    }

    fn ack(&self, rec_id: u8) {
        let _ = self.port.emit_record(QS_RX_STATUS, &[rec_id], false);
    }

    fn done(&self, rec_id: u8) {
        let _ = self.port.emit_record(QS_TARGET_DONE, &[rec_id], true);
    }

    fn ack_done(&self, rec_id: u8) {
        self.ack(rec_id);
        self.done(rec_id);
    }
}

fn emit_reference_dictionary(port: &PosixPort) -> Result<(), TraceError> {
    port.emit_target_info(&TargetInfo::default())?;
    port.emit_obj_dict(dict_handle(QS_RX_NAME), QS_RX_NAME)?;
    port.emit_obj_dict(dict_handle(CLOCK_TICK_NAME), CLOCK_TICK_NAME)?;
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

    port.emit_obj_dict(dict_handle(EVT_POOL_NAME), EVT_POOL_NAME)?;
    port.emit_fun_dict(dict_handle(QHSM_TOP_NAME), QHSM_TOP_NAME)?;
    port.emit_obj_dict(dict_handle(TABLE_OBJECT_NAME), TABLE_OBJECT_NAME)?;

    for entry in philo_trace_entries() {
        port.emit_obj_dict(entry.object_handle, entry.object_name.as_str())?;
        port.emit_obj_dict(entry.timer_handle, entry.timer_name.as_str())?;
    }

    port.emit_fun_dict(philo_initial as usize as u64, PHILO_INITIAL_NAME)?;
    port.emit_fun_dict(thinking      as usize as u64, PHILO_THINKING_NAME)?;
    port.emit_fun_dict(hungry        as usize as u64, PHILO_HUNGRY_NAME)?;
    port.emit_fun_dict(eating        as usize as u64, PHILO_EATING_NAME)?;
    port.emit_fun_dict(table_initial as usize as u64, "Table::initial")?;
    port.emit_fun_dict(active        as usize as u64, "Table::active")?;
    port.emit_fun_dict(serving       as usize as u64, TABLE_SERVING_NAME)?;
    port.emit_fun_dict(paused        as usize as u64, TABLE_PAUSED_NAME)?;

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("Starting Dining Philosophers demo");

    let (runtime, port) = build_runtime()?;
    emit_reference_dictionary(&port)?;
    let kernel = runtime.kernel();
    if KERNEL.set(Arc::clone(&kernel)).is_err() {
        panic!("kernel already initialised");
    }

    const TICKS: usize = 60;
    for _ in 0..TICKS {
        runtime.tick()?;
        runtime.run_until_idle();
        thread::sleep(Duration::from_millis(200));
    }

    Ok(())
}
