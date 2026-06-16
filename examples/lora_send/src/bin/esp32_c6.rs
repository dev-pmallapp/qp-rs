//! LoRa send — ESP32-C6 target.
//!
//! Wiring (adjust GPIO numbers to match your board):
//!   SPI2  MOSI=GPIO7  MISO=GPIO2  SCLK=GPIO6  CS=GPIO10
//!   SX1262 RESET=GPIO4  BUSY=GPIO5
//!
//! Run alongside qspy on the host:
//! ```sh
//! cargo run --bin qspy -- --tcp 127.0.0.1:7701
//! # flash and monitor
//! cargo build --bin lora_send_c6 --features esp32c6 --no-default-features
//! ```

#![cfg(feature = "esp32c6")]

use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use esp_idf_sys as _;

use hal_rvsis::esp32c6::{Esp32C6Pin, Esp32C6Spi, radio::Sx1262};

use qf::active::{new_active_object, ActiveObjectId};
use qf::event::{DynEvent, DynPayload, Event, Signal};
use qf::hsm::reserved::*;
use qf::time::{TimeEvent, TimeEventConfig};
use qf::{q_handled, q_super, q_tran, QHsm, QHsmResult};
use qf_port_esp32_c6::{Esp32C6Port, Esp32C6QkRuntime, PortConfig};
use qk::QkKernel;

#[cfg(feature = "qs")]
use qs;

use comms::{
    lora::LoRaRf,
    mac::CommsAO,
    session::LoRaSession,
    events::{RfTxReqPayload, RF_TX_REQ_SIG},
};
use hal::lora::LoRaTxConfig;

// ── IDs & signals ─────────────────────────────────────────────────────────────

const SENDER_ID: ActiveObjectId = ActiveObjectId::new(1);
const COMMS_ID:  ActiveObjectId = ActiveObjectId::new(2);
const TIMEOUT_SIG: Signal = Signal(10);

// ── Static kernel handle ──────────────────────────────────────────────────────

static KERNEL: OnceLock<Arc<qk::QkKernel>> = OnceLock::new();

// ── LoRaSenderAO ──────────────────────────────────────────────────────────────

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
                1,
            ));
            if let Some(kernel) = KERNEL.get() {
                let _ = kernel.post(COMMS_ID, Event::with_arc(RF_TX_REQ_SIG, payload));
            }
            q_handled!()
        }
        _ => q_super!(QHsm::<LoRaSenderData>::top_state),
    }
}

// ── Wiring ────────────────────────────────────────────────────────────────────

fn build_sx1262() -> Sx1262<Esp32C6Spi> {
    use hal::spi::SpiConfig;
    use hal_rvsis::esp32c6::regs::{SPI2_BASE, SpiRegs};

    let mut spi = unsafe { Esp32C6Spi::new(SPI2_BASE as *const SpiRegs) };
    spi.configure(&SpiConfig::default()).expect("SPI config failed");

    let reset = unsafe { Esp32C6Pin::new(4) };
    let busy  = unsafe { Esp32C6Pin::new(5) };

    Sx1262::with_busy(spi, reset, busy)
}

fn main() -> ! {
    esp_idf_sys::link_patches();
    println!("=== lora_send_c6 ===");

    let session   = LoRaSession::test_abp();
    let tx_config = LoRaTxConfig::eu868_default();
    let rf        = LoRaRf::new(build_sx1262(), session, tx_config);
    let comms_ao  = new_active_object(COMMS_ID, 5, CommsAO::new(rf));

    let timer     = Arc::new(TimeEvent::new(SENDER_ID, TimeEventConfig::new(TIMEOUT_SIG)));
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
        .register(comms_ao).expect("comms register")
        .register(sender_ao).expect("sender register");

    #[cfg(feature = "qs")]
    let builder = {
        let tracer = qs::Tracer::new(qs::QsConfig::default(), qs::stdout_backend()).into_handle();
        let mut target_info = qs::TargetInfo::default();
        target_info.obj_ptr_size = core::mem::size_of::<usize>() as u8;
        target_info.fun_ptr_size = core::mem::size_of::<usize>() as u8;
        let payload = qs::predefined::target_info_payload(&target_info);
        let _ = tracer.emit(qs::predefined::TARGET_INFO, &payload);

        let _ = tracer.emit(qs::predefined::SIG_DICT, &qs::predefined::sig_dict_payload(TIMEOUT_SIG.0, 0, "TIMEOUT"));
        let _ = tracer.emit(qs::predefined::SIG_DICT, &qs::predefined::sig_dict_payload(RF_TX_REQ_SIG.0, 0, "RF_TX_REQ"));

        builder.with_trace_hook(tracer.hook())
    };

    let kernel = Arc::new(builder.build().expect("kernel build"));
    kernel.start();

    KERNEL.set(Arc::clone(&kernel))
        .unwrap_or_else(|_| panic!("kernel already set"));

    let port = Esp32C6Port::new();
    let mut config = PortConfig::new();
    config.tick_hz = 10; // 100 ms ticks → 5-tick arm = 500 ms interval

    let mut runtime = Esp32C6QkRuntime::new(Arc::clone(&kernel), port, config);
    runtime.register_time_event(Arc::clone(&timer));

    loop {
        runtime.tick().expect("tick");
        runtime.run_until_idle();
        thread::sleep(Duration::from_millis(100));
    }
}
