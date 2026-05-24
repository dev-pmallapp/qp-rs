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

use hal_esp::{EspGpioPin, EspSpiMaster, Sx1262};

use qf::active::{new_active_object, ActiveBehavior, ActiveContext, ActiveObjectId};
use qf::event::{DynEvent, DynPayload, Event, Signal};
use qf::time::{TimeEvent, TimeEventConfig};
use qf_port_esp32_c6::{Esp32C6Port, Esp32C6QkRuntime, PortConfig};
use qk::QkKernel;

use comms::{
    lora::LoRaRf,
    mac::CommsAO,
    records::LORA_TX_PKT,
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

struct LoRaSenderAO {
    timer: Arc<TimeEvent>,
    count: u32,
}

impl LoRaSenderAO {
    fn new(timer: Arc<TimeEvent>) -> Self {
        Self { timer, count: 0 }
    }
}

impl ActiveBehavior for LoRaSenderAO {
    fn on_start(&mut self, _ctx: &mut ActiveContext) {
        println!("LoRaSenderAO: started — sending every 5 ticks");
        self.timer.arm(5, Some(5));
    }

    fn on_event(&mut self, _ctx: &mut ActiveContext, event: DynEvent) {
        if event.signal() == TIMEOUT_SIG {
            self.count += 1;
            let msg = format!("hello LoRa #{}", self.count);
            let payload: DynPayload = Arc::new(RfTxReqPayload::new(
                msg.into_bytes(),
                1,
            ));
            if let Some(kernel) = KERNEL.get() {
                let _ = kernel.post(COMMS_ID, Event::with_arc(RF_TX_REQ_SIG, payload));
            }
        }
    }
}

// ── Wiring ────────────────────────────────────────────────────────────────────

fn build_sx1262() -> Sx1262<EspSpiMaster> {
    // SPI2 bus — MOSI=7, MISO=2, SCLK=6, CS=10, 1 MHz
    let spi = EspSpiMaster::new(2, 7, 2, 6, 10, 1_000_000)
        .expect("SPI init failed");

    let reset = EspGpioPin::output(4).expect("reset pin init failed");
    let busy  = EspGpioPin::input(5).expect("busy pin init failed");

    Sx1262::new(spi, reset, Some(busy))
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
        LoRaSenderAO::new(Arc::clone(&timer)),
    );

    let builder = QkKernel::builder()
        .register(comms_ao).expect("comms register")
        .register(sender_ao).expect("sender register");

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
