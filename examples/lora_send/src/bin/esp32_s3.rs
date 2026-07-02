//! LoRa send — ESP32-S3 target.
//!
//! Run alongside qspy on the host:
//! ```sh
//! cargo run --bin qspy -- --tcp 127.0.0.1:7701
//! # flash and monitor
//! cargo build --bin lora_send_s3 --features esp32s3 --no-default-features
//! ```

#![cfg(feature = "esp32s3")]

use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use esp_idf_sys as _;

use hal_lxsis::esp32s3::{Esp32S3Pin, Esp32S3Spi, radio::Sx1276};

use qf::active::{new_active_object, ActiveObjectId, ActiveRunnable};
use qf::event::{DynEvent, DynPayload, Event, Signal};
use qf::hsm::reserved::*;
use qf::time::{TimeEvent, TimeEventConfig};
use qf::{q_handled, q_super, q_tran, QHsm, QHsmResult};
use qf_port_esp32_s3::{Esp32S3Port, Esp32S3QkRuntime, PortConfig, rf_isr};
use qk::QkKernel;

#[cfg(feature = "qs")]
use qs;

use comms::{
    events::{RfTxReqPayload, RF_TX_REQ_SIG, RF_TX_DONE_SIG},
    session::LoRaSession,
    stack::{RfStack, RfStackAO},
    net::NoopNetwork,
    transport::UnreliableTransport,
    mac::lorawan::LoRaWanMac,
};
use hal::{
    rf::{RfTxConfig, RfRxConfig, RadioParams},
    lora::LoRaModulation,
    gpio::PinMode,
    spi::SpiConfig,
};
use hal_lxsis::esp32s3::regs::{SPI2_BASE, SpiRegs};
use critical_section::Mutex;
use core::cell::RefCell;
use embedded_hal::spi::SpiBus;

// ── IDs & signals ─────────────────────────────────────────────────────────────

const SENDER_ID: ActiveObjectId = ActiveObjectId::new(1);
const RF_AO_ID:  ActiveObjectId = ActiveObjectId::new(2);
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
                let _ = kernel.post(RF_AO_ID, Event::with_arc(RF_TX_REQ_SIG, payload));
            }
            q_handled!()
        }
        sig if sig == RF_TX_DONE_SIG.0 => {
            println!("LoRaSenderAO: TX done");
            q_handled!()
        }
        _ => q_super!(QHsm::<LoRaSenderData>::top_state),
    }
}

// ── Wiring ────────────────────────────────────────────────────────────────────

static ISR_SPI: Mutex<RefCell<Option<Esp32S3Spi>>> = Mutex::new(RefCell::new(None));

fn isr_spi_transfer(tx: &[u8], rx: &mut [u8]) -> bool {
    critical_section::with(|cs| {
        if let Some(spi) = ISR_SPI.borrow_ref_mut(cs).as_mut() {
            let _ = spi.transfer(rx, tx);
            true
        } else {
            false
        }
    })
}

fn build_sx1276() -> Sx1276<Esp32S3Spi> {
    let mut spi = unsafe { Esp32S3Spi::new(SPI2_BASE as *const SpiRegs) };
    spi.configure(&SpiConfig::default()).expect("SPI config failed");

    let mut isr_spi = unsafe { Esp32S3Spi::new(SPI2_BASE as *const SpiRegs) };
    isr_spi.configure(&SpiConfig::default()).expect("ISR SPI config failed");
    critical_section::with(|cs| {
        *ISR_SPI.borrow_ref_mut(cs) = Some(isr_spi);
    });

    let mut reset = unsafe { Esp32S3Pin::new(4) };
    reset.set_mode(PinMode::Output).expect("reset pin config failed");

    Sx1276::new(spi, reset)
}

static mut RF_AO_REF: Option<&'static dyn ActiveRunnable> = None;

fn main() -> ! {
    esp_idf_sys::link_patches();
    println!("=== lora_send_s3 (RfStackAO) ===");

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

    let session   = LoRaSession::test_abp();
    let rf        = build_sx1276();
    let mac       = LoRaWanMac::new(session, 1);
    let transport = UnreliableTransport::new();
    let network   = NoopNetwork;
    let stack     = RfStack::new(transport, network, mac, rf);

    // RfStackAO expects `hal::rf::RfTxConfig`/`RfRxConfig` (radio-agnostic), not
    // the LoRa-specific `LoRaTxConfig`.
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

    let rf_ao_arc = new_active_object(
        RF_AO_ID, 4,
        RfStackAO::new(stack, tx_cfg, rx_cfg, Arc::clone(&sender_ao) as Arc<dyn ActiveRunnable>)
    );

    let rf_ao_static = unsafe {
        let ptr = Arc::into_raw(Arc::clone(&rf_ao_arc));
        &*ptr
    };

    let builder = QkKernel::builder()
        .register(rf_ao_arc).expect("rf register")
        .register(sender_ao).expect("sender register");

    let kernel = Arc::new(builder.build().expect("kernel build"));
    kernel.start();

    KERNEL.set(Arc::clone(&kernel))
        .unwrap_or_else(|_| panic!("kernel already set"));

    rf_isr::register_rf_ao(rf_ao_static);
    rf_isr::register_rf_spi_fn(isr_spi_transfer);

    let port = Esp32S3Port::new();
    let mut config = PortConfig::new();
    config.tick_hz = 10;

    let mut runtime = Esp32S3QkRuntime::new(Arc::clone(&kernel), port, config);
    runtime.register_time_event(Arc::clone(&timer));

    loop {
        runtime.tick().expect("tick");
        runtime.run_until_idle();
        thread::sleep(Duration::from_millis(100));
    }
}
