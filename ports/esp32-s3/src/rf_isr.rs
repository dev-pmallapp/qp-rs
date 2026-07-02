//! ESP32-S3 RF interrupt handler — DIO0/DIO1 ISR bridge for SX1276.
//!
//! This module bridges the hardware interrupt to the QP-RS RfStackAO.

#![cfg(feature = "rt")]

extern crate alloc;
use alloc::sync::Arc;

use core::sync::atomic::{AtomicPtr, Ordering};
use core::cell::RefCell;

use critical_section::Mutex;
use qf::active::ActiveRunnable;
use qf::event::DynEvent;
use comms::events::{
    PhyIrqPayload, RF_PHY_CRC_ERROR_SIG, RF_PHY_IRQ_SIG,
    RF_PHY_RX_DONE_SIG, RF_PHY_TX_DONE_SIG,
};
use hal::rf::{PhyEvent, RxMetadata};

// ─────────────────────────────────────────────────────────────────────────────
// Static RF AO pointer
// ─────────────────────────────────────────────────────────────────────────────

static RF_AO: AtomicPtr<dyn ActiveRunnable> = AtomicPtr::new(core::ptr::null_mut());

pub fn register_rf_ao(ao: &'static dyn ActiveRunnable) {
    RF_AO.store(ao as *const _ as *mut _, Ordering::Release);
}

// ─────────────────────────────────────────────────────────────────────────────
// SPI handle for IRQ status read
// ─────────────────────────────────────────────────────────────────────────────

type IsrSpiTransferFn = fn(tx: &[u8], rx: &mut [u8]) -> bool;

static RF_SPI_FN: Mutex<RefCell<Option<IsrSpiTransferFn>>> =
    Mutex::new(RefCell::new(None));

pub fn register_rf_spi_fn(f: IsrSpiTransferFn) {
    critical_section::with(|cs| {
        *RF_SPI_FN.borrow_ref_mut(cs) = Some(f);
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// SX1276 IRQ status read (polled, inside ISR)
// ─────────────────────────────────────────────────────────────────────────────

fn read_sx1276_irq_status() -> (PhyEvent, RxMetadata) {
    critical_section::with(|cs| {
        let guard = RF_SPI_FN.borrow_ref(cs);
        let Some(spi_fn) = *guard else {
            return (PhyEvent::TxDone, RxMetadata::default());
        };

        // RegIrqFlags is 0x12
        let get_irq = [0x12u8 & 0x7F, 0x00];
        let mut irq_rx = [0u8; 2];
        if !spi_fn(&get_irq, &mut irq_rx) {
            return (PhyEvent::TxDone, RxMetadata::default());
        }
        let irq_status = irq_rx[1];

        // Clear flags by writing back (0x12 | 0x80 = 0x92)
        let clear = [0x92, irq_status];
        let mut clear_rx = [0u8; 2];
        let _ = spi_fn(&clear, &mut clear_rx);

        parse_sx1276_irq(irq_status)
    })
}

fn parse_sx1276_irq(status: u8) -> (PhyEvent, RxMetadata) {
    let meta = RxMetadata::default();
    if status & 0x20 != 0 {
        (PhyEvent::CrcError, meta)
    } else if status & 0x80 != 0 {
        (PhyEvent::RxTimeout, meta)
    } else if status & 0x40 != 0 {
        (PhyEvent::RxDone(meta), meta)
    } else if status & 0x08 != 0 {
        (PhyEvent::TxDone, meta)
    } else if status & 0x04 != 0 {
        (PhyEvent::CadDone { channel_active: true }, meta)
    } else {
        (PhyEvent::TxDone, meta)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ISR handler
// ─────────────────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn DIO0_IRQHandler() {
    let ao_ptr = RF_AO.load(Ordering::Acquire);
    if ao_ptr.is_null() { return; }

    let ao = unsafe { &*ao_ptr };

    let (event, meta) = read_sx1276_irq_status();

    let sig = match event {
        PhyEvent::TxDone        => RF_PHY_TX_DONE_SIG,
        PhyEvent::RxDone(_)     => RF_PHY_RX_DONE_SIG,
        PhyEvent::CrcError      => RF_PHY_CRC_ERROR_SIG,
        _                       => RF_PHY_IRQ_SIG,
    };

    let payload = PhyIrqPayload { event, meta };
    let ev = DynEvent::with_arc(sig, Arc::new(payload));

    qk::isr_entry();
    ao.post(ev);
    qk::isr_exit();
}
