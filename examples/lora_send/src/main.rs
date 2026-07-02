//! LoRa send example — POSIX host.
//!
//! Architecture (top to bottom):
//!
//! ```text
//! LoRaSenderAO (this file)
//!   │  RF_TX_REQ_SIG  →  RfStackAO
//!   ▼
//! RfStackAO  (crates/comms)
//!   │  NullRf (prints frame to stdout; no real SPI)
//!   │  QS LORA_TX_PKT record → qspy
//!   ▼
//! qspy (tools/qspy) decodes and displays the LoRaWAN frame
//! ```
//!
//! Run alongside qspy:
//! ```sh
//! cargo run --bin qspy -- --tcp 127.0.0.1:7701
//! cargo run --bin lora_send
//! ```

#![cfg(feature = "host")]

use std::env;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use qf::active::{
    arc_as_runnable, new_active_object, ActiveContext, ActiveObject, ActiveObjectId,
};
use qf::event::{DynEvent, DynPayload, Event, Signal};
use qf::hsm::reserved::*;
use qf::time::{TimeEvent, TimeEventConfig};
use qf::{q_handled, q_super, q_tran, QHsm, QHsmResult, TraceError};
use qf_port_posix::{PosixPort, PosixQkRuntime};
use qk::{QkKernel, QkKernelError};
use qs::predefined::TargetInfo;

use comms::{
    null_rf::NullRf,
    records::LORA_TX_PKT,
    session::LoRaSession,
    events::{RfTxReqPayload, RF_TX_REQ_SIG, RF_TX_DONE_SIG},
    stack::{RfStack, RfStackAO},
    net::NoopNetwork,
    transport::UnreliableTransport,
    mac::lorawan::LoRaWanMac,
};
use hal::rf::{RfTxConfig, RfRxConfig, RadioParams};
use hal::lora::LoRaModulation;

// ── IDs & signals ────────────────────────────────────────────────────────────

const SENDER_ID: ActiveObjectId = ActiveObjectId::new(1);
const RF_AO_ID:  ActiveObjectId = ActiveObjectId::new(2);

const TIMEOUT_SIG: Signal = Signal(10);

/// Concrete RF AO type for the host stack, so the loop can `pump()` it directly.
type HostRfAo = RfStackAO<UnreliableTransport, NoopNetwork, LoRaWanMac, NullRf>;

// ── Static kernel handle ─────────────────────────────────────────────────────

static KERNEL: OnceLock<Arc<qk::QkKernel>> = OnceLock::new();

// ── LoRaSenderAO ─────────────────────────────────────────────────────────────

struct LoRaSenderData {
    timer: Arc<TimeEvent>,
    count: u32,
}

fn sender_initial(_sm: &mut LoRaSenderData, _e: &DynEvent) -> QHsmResult<LoRaSenderData> {
    q_tran!(sending)
}

fn sending(sm: &mut LoRaSenderData, e: &DynEvent) -> QHsmResult<LoRaSenderData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => {
            println!("LoRaSenderAO: started — sending every 5 ticks");
            sm.timer.arm(5, Some(5));
            q_handled!()
        }
        10 => { // TIMEOUT_SIG
            sm.count += 1;
            let msg = format!("hello LoRa #{}", sm.count);
            let payload: DynPayload = Arc::new(RfTxReqPayload::new(
                msg.into_bytes(),
                1, // FPort 1
            ));
            if let Some(kernel) = KERNEL.get() {
                let _ = kernel.post(RF_AO_ID, Event::with_arc(RF_TX_REQ_SIG, payload));
            }
            q_handled!()
        }
        sig if sig == RF_TX_DONE_SIG.0 => {
            println!("LoRaSenderAO: TX done ✓");
            q_handled!()
        }
        _ => q_super!(QHsm::<LoRaSenderData>::top_state),
    }
}

// ── Wiring ───────────────────────────────────────────────────────────────────

fn init_port() -> Arc<PosixPort> {
    let addr = env::var("QSPY_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:7701".to_string());
    let port = match PosixPort::connect_udp(&addr) {
        Ok(p)  => { println!("QS connected to udp://{addr}"); p }
        Err(e) => { eprintln!("qspy not reachable ({e}), using stdout"); PosixPort::new() }
    };
    Arc::new(port)
}

fn emit_dictionaries(port: &PosixPort) -> Result<(), TraceError> {
    port.emit_target_info(&TargetInfo::default())?;
    port.emit_usr_dict(LORA_TX_PKT, "LORA_TX_PKT")?;
    port.emit_sig_dict(RF_TX_REQ_SIG.0, 0, "RF_TX_REQ_SIG")?;
    port.emit_sig_dict(RF_TX_DONE_SIG.0, 0, "RF_TX_DONE_SIG")?;
    port.emit_sig_dict(TIMEOUT_SIG.0, 0, "TIMEOUT_SIG")?;
    Ok(())
}

fn build_runtime(port: &Arc<PosixPort>)
    -> Result<(PosixQkRuntime, Arc<ActiveObject<HostRfAo>>), QkKernelError>
{
    // Build sender AO first so we can pass a reference to the RF stack.
    let timer = Arc::new(TimeEvent::new(SENDER_ID, TimeEventConfig::new(TIMEOUT_SIG)));
    let sender_ao = new_active_object(
        SENDER_ID, 3,
        QHsm::new(
            LoRaSenderData {
                timer: Arc::clone(&timer),
                count: 0,
            },
            sender_initial,
        ),
    );

    // Build NullRf (POSIX host — no SPI hardware); NullRf implements RfPhy directly.
    let session   = LoRaSession::test_abp();
    let mac       = LoRaWanMac::new(session.clone(), 1);
    let transport = UnreliableTransport::new();
    let network   = NoopNetwork;
    let stack     = RfStack::new(transport, network, mac, NullRf::new());

    let tx_cfg = RfTxConfig {
        frequency_hz: 868_100_000,
        tx_power_dbm: 14,
        params: RadioParams::LoRa(LoRaModulation::default()),
    };
    let rx_cfg = RfRxConfig {
        frequency_hz: 868_100_000,
        timeout_ms: None,
        params: RadioParams::LoRa(LoRaModulation::default()),
    };

    // Keep a concrete handle so the host loop can `pump()` the PHY (no ISR).
    let rf_ao: Arc<ActiveObject<HostRfAo>> = ActiveObject::new(
        RF_AO_ID, 4,
        RfStackAO::new(stack, tx_cfg, rx_cfg, Arc::clone(&sender_ao)),
    );

    let builder = QkKernel::builder()
        .register(arc_as_runnable(Arc::clone(&rf_ao)))?
        .register(sender_ao)?;

    let mut runtime = PosixQkRuntime::with_port(builder, port)?;
    runtime.register_time_event(Arc::clone(&timer));
    Ok((runtime, rf_ao))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== lora_send (host) ===");

    let port          = init_port();
    let (runtime, rf_ao) = build_runtime(&port)?;
    emit_dictionaries(&port)?;

    let kernel = runtime.kernel();
    KERNEL.set(Arc::clone(&kernel)).unwrap_or_else(|_| panic!("kernel already set"));

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

    // Host has no radio ISR, so drain the PHY cooperatively: `pump()` turns the
    // NullRf's synthetic TxDone into RF_TX_DONE_SIG back to the sender AO.
    #[cfg(not(feature = "smp"))]
    let mut rf_ctx = ActiveContext::new(RF_AO_ID, None);

    // Run for 30 ticks (≈ 6 seconds at 200 ms/tick → ~6 TX events).
    for _ in 0..30 {
        runtime.tick()?;
        #[cfg(not(feature = "smp"))]
        {
            runtime.run_until_idle();
            while rf_ao.with_behavior_mut(|rf| rf.pump(&mut rf_ctx)) {
                runtime.run_until_idle();
            }
        }
        thread::sleep(Duration::from_millis(200));
    }

    // `rf_ao` is drained via the kernel's registered handle under `smp`.
    #[cfg(feature = "smp")]
    let _ = &rf_ao;

    #[cfg(feature = "smp")]
    {
        running.store(false, core::sync::atomic::Ordering::Relaxed);
        for handle in worker_handles {
            handle.join().unwrap();
        }
    }

    println!("=== done ===");
    Ok(())
}
