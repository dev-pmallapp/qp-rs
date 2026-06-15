//! LoRa send example — POSIX host.
//!
//! Architecture (top to bottom):
//!
//! ```text
//! LoRaSenderAO (this file)
//!   │  RF_TX_REQ_SIG  →  CommsAO
//!   ▼
//! CommsAO  (crates/comms)
//!   │  LoRaRf::send_with_fport()
//!   ▼
//! LoRaRf<NullRf>  (NullRf: prints frame to stdout; no real SPI)
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

use qf::active::{new_active_object, ActiveObjectId};
use qf::event::{DynEvent, DynPayload, Event, Signal};
use qf::hsm::reserved::*;
use qf::time::{TimeEvent, TimeEventConfig};
use qf::{q_handled, q_super, q_tran, QHsm, QHsmResult, TraceError};
use qf_port_posix::{PosixPort, PosixQkRuntime};
use qk::{QkKernel, QkKernelError};
use qs::predefined::TargetInfo;

use comms::{
    lora::LoRaRf,
    mac::CommsAO,
    null_rf::NullRf,
    records::LORA_TX_PKT,
    session::LoRaSession,
    events::{RfTxReqPayload, RF_TX_REQ_SIG},
};
use hal::lora::LoRaTxConfig;

// ── IDs & signals ────────────────────────────────────────────────────────────

const SENDER_ID:   ActiveObjectId = ActiveObjectId::new(1);
const COMMS_ID:    ActiveObjectId = ActiveObjectId::new(2);

const TIMEOUT_SIG: Signal = Signal(10);

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
                let _ = kernel.post(COMMS_ID, Event::with_arc(RF_TX_REQ_SIG, payload));
            }
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
    port.emit_sig_dict(TIMEOUT_SIG.0, 0, "TIMEOUT_SIG")?;
    Ok(())
}

fn build_runtime(port: &Arc<PosixPort>)
    -> Result<PosixQkRuntime, QkKernelError>
{
    // Build NullRf radio (POSIX host — no SPI hardware)
    let session   = LoRaSession::test_abp();
    let tx_config = LoRaTxConfig::eu868_default();
    let rf        = LoRaRf::new(NullRf, session, tx_config);
    let comms_ao  = new_active_object(COMMS_ID, 5, CommsAO::new(rf));

    // Sender timer
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

    let builder = QkKernel::builder()
        .register(comms_ao)?
        .register(sender_ao)?;

    let mut runtime = PosixQkRuntime::with_port(builder, port)?;
    runtime.register_time_event(Arc::clone(&timer));
    Ok(runtime)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== lora_send (host) ===");

    let port    = init_port();
    let runtime = build_runtime(&port)?;
    emit_dictionaries(&port)?;

    let kernel = runtime.kernel();
    KERNEL.set(Arc::clone(&kernel)).unwrap_or_else(|_| panic!("kernel already set"));

    // Run for 30 ticks (≈ 6 seconds at 200 ms/tick → ~6 TX events)
    for _ in 0..30 {
        runtime.tick()?;
        runtime.run_until_idle();
        thread::sleep(Duration::from_millis(200));
    }

    println!("=== done ===");
    Ok(())
}
