//! Dining Philosophers Problem implemented on top of the Rust `qk` preemptive kernel.
//!
//! This example mirrors the reference application in
//! `scratch/qp-8.1.1/qpcpp/examples/posix-win32/dpp_comp`, showing how active
//! objects, the preemptive kernel, time events, and QS tracing integrate in
//! Rust using the QHsm framework.

use std::error::Error;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use rand::{rngs::SmallRng, SeedableRng};

use qf::active::{new_active_object, ActiveObjectId};
use qf::time::{TimeEvent, TimeEventConfig, TimeEventTraceInfo};
use qf::{TraceError, Signal, QHsm};
use qf_port_posix::{PosixPort, PosixQkRuntime};
use qk::{QkKernel, QkKernelError};
use qs::{TargetInfo, UserRecordBuilder};

mod philo;
mod table;
mod qspy;

use philo::{PhiloData, philo_initial, thinking, hungry, eating};
use table::{TableData, TableMsg, table_initial, active, serving, paused};
use qspy::init_port;

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

    port.emit_fun_dict(philo_initial as *const () as usize as u64, PHILO_INITIAL_NAME)?;
    port.emit_fun_dict(thinking      as *const () as usize as u64, PHILO_THINKING_NAME)?;
    port.emit_fun_dict(hungry        as *const () as usize as u64, PHILO_HUNGRY_NAME)?;
    port.emit_fun_dict(eating        as *const () as usize as u64, PHILO_EATING_NAME)?;
    port.emit_fun_dict(table_initial as *const () as usize as u64, "Table::initial")?;
    port.emit_fun_dict(active        as *const () as usize as u64, "Table::active")?;
    port.emit_fun_dict(serving       as *const () as usize as u64, TABLE_SERVING_NAME)?;
    port.emit_fun_dict(paused        as *const () as usize as u64, TABLE_PAUSED_NAME)?;

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

    #[cfg(feature = "smp")]
    let (running, worker_handles) = {
        let running = Arc::new(core::sync::atomic::AtomicBool::new(true));
        let mut handles = Vec::new();
        for i in 0..4 {
            let kernel_clone = Arc::clone(&kernel);
            let running_clone = Arc::clone(&running);
            handles.push(thread::spawn(move || {
                println!("Worker thread {} started", i);
                while running_clone.load(core::sync::atomic::Ordering::Relaxed) {
                    kernel_clone.run_until_idle();
                    thread::sleep(Duration::from_millis(10));
                }
                println!("Worker thread {} finished", i);
            }));
        }
        (running, handles)
    };

    const TICKS: usize = 60;
    for _ in 0..TICKS {
        runtime.tick()?;
        #[cfg(not(feature = "smp"))]
        runtime.run_until_idle();
        thread::sleep(Duration::from_millis(200));
    }

    #[cfg(feature = "smp")]
    {
        running.store(false, core::sync::atomic::Ordering::Relaxed);
        for handle in worker_handles {
            handle.join().unwrap();
        }
    }

    Ok(())
}
