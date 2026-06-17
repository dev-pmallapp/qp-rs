//! ESP32-C6 RF interrupt handler scaffolding.

#![cfg(feature = "rt")]

use core::sync::atomic::{AtomicPtr, Ordering};
use qf::active::ActiveRunnable;
use comms::stack::{PhyIrqPayload, RF_PHY_RX_DONE_SIG, RF_PHY_TX_DONE_SIG, RF_PHY_CRC_ERROR_SIG, RF_PHY_IRQ_SIG};
use hal::rf::PhyEvent;
use qf::event::DynEvent;

static RF_AO: AtomicPtr<dyn ActiveRunnable> = AtomicPtr::new(core::ptr::null_mut());

/// Register the RF Active Object that should receive ISR notifications.
pub fn register_rf_ao(ao: &'static dyn ActiveRunnable) {
    RF_AO.store(ao as *const _ as *mut _, Ordering::Release);
}

fn read_sx1262_irq_status() -> (PhyEvent, hal::rf::RxMetadata) {
    // Under Renode/hardware simulation, this reads from SPI.
    // Returns default stub for compiler verification.
    (PhyEvent::TxDone, hal::rf::RxMetadata::default())
}

/// DIO1 interrupt handler for SX1262 transceiver.
#[no_mangle]
pub extern "C" fn DIO1_IRQHandler() {
    let ao_ptr = RF_AO.load(Ordering::Acquire);
    if ao_ptr.is_null() { return; }
    let ao = unsafe { &*ao_ptr };

    let (event, meta) = read_sx1262_irq_status();

    let sig = match event {
        PhyEvent::TxDone        => RF_PHY_TX_DONE_SIG,
        PhyEvent::RxDone(_)     => RF_PHY_RX_DONE_SIG,
        PhyEvent::CrcError      => RF_PHY_CRC_ERROR_SIG,
        _                       => RF_PHY_IRQ_SIG,
    };
    let payload = PhyIrqPayload { event, meta };
    let ev = DynEvent::with_arc(sig, std::sync::Arc::new(payload));

    ao.post(ev);
}
