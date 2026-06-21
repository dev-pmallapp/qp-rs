//! ESP32-C6 RF interrupt handler — DIO1 ISR bridge for SX1262.
//!
//! This module is the ONLY place where the ISR and QP-RS meet.
//! It lives in the port crate, never in `hal` or `comms`.
//!
//! ## Threading model
//!
//! - `register_rf_ao` is called once from `main` before interrupts are enabled.
//! - `DIO1_IRQHandler` runs in interrupt context, reads the SX1262 IRQ status
//!   register via polled SPI (≤ 4 bytes, ≈ 1 µs at 8 MHz), and posts a typed
//!   event to `RfStackAO` via `post_from_isr`.
//! - The ESP32-C6 uses a CLIC (Core Local Interrupt Controller).  The DIO1
//!   interrupt priority must be configured so that the scheduler's critical
//!   section does NOT mask it.
//!
//! ## SPI inside the ISR
//!
//! Only polled (non-DMA) SPI transfers of ≤ 4 bytes are performed in the ISR:
//! `GetIrqStatus` (0x12) + `ClearIrqStatus` (0x97).  The longer RX payload
//! read (`ReadBuffer`) is deferred to `RfPhy::read_rx`, which runs in AO
//! context where blocking is safe.

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

/// Pointer to the `RfStackAO` that should receive DIO1 interrupt events.
///
/// Set by `register_rf_ao` before interrupts are enabled; never changed after.
static RF_AO: AtomicPtr<dyn ActiveRunnable> = AtomicPtr::new(core::ptr::null_mut());

/// Register the RF Active Object that should receive ISR-posted events.
///
/// Must be called once from `main()` before enabling the DIO1 GPIO interrupt.
/// The reference must be `'static` (e.g. a static `Arc<QkKernel>`-owned AO).
pub fn register_rf_ao(ao: &'static dyn ActiveRunnable) {
    RF_AO.store(ao as *const _ as *mut _, Ordering::Release);
}

// ─────────────────────────────────────────────────────────────────────────────
// SPI handle for IRQ status read
// ─────────────────────────────────────────────────────────────────────────────

/// Function pointer type for the polled SPI transfer used inside the ISR.
///
/// The closure receives a TX buffer and fills an RX buffer of the same length.
/// It must be callable in interrupt context (no blocking, no DMA).
type IsrSpiTransferFn = fn(tx: &[u8], rx: &mut [u8]) -> bool;

/// Installed by `register_rf_spi_fn` during board init.
static RF_SPI_FN: Mutex<RefCell<Option<IsrSpiTransferFn>>> =
    Mutex::new(RefCell::new(None));

/// Register a polled-SPI transfer function for use inside the DIO1 ISR.
///
/// The function must perform a blocking polled SPI transfer without DMA.
/// On ESP32-C6 with esp-hal this is typically a `SpiMaster::transfer_blocking`.
pub fn register_rf_spi_fn(f: IsrSpiTransferFn) {
    critical_section::with(|cs| {
        *RF_SPI_FN.borrow_ref_mut(cs) = Some(f);
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// SX1262 IRQ status read (polled, inside ISR)
// ─────────────────────────────────────────────────────────────────────────────

/// Read SX1262 IRQ status and clear flags using polled SPI.
///
/// OpCode 0x12 = GetIrqStatus → returns [status_byte, irq_high, irq_low]
/// OpCode 0x97 = ClearIrqStatus(0x03FF) → clear all flags
///
/// This must be called from inside the DIO1 interrupt before any other SPI
/// access to avoid losing the flag state.
///
/// Returns a default `TxDone` stub when no SPI function is registered (e.g.
/// during host-side unit tests where no real SPI hardware exists).
fn read_sx1262_irq_status() -> (PhyEvent, RxMetadata) {
    critical_section::with(|cs| {
        let guard = RF_SPI_FN.borrow_ref(cs);
        let Some(spi_fn) = *guard else {
            // No SPI registered — return a no-op default for host builds.
            return (PhyEvent::TxDone, RxMetadata::default());
        };

        // GetIrqStatus: opcode + 1 dummy + 2 status bytes
        let get_irq = [0x12u8, 0x00, 0x00, 0x00];
        let mut irq_rx = [0u8; 4];
        if !spi_fn(&get_irq, &mut irq_rx) {
            return (PhyEvent::TxDone, RxMetadata::default());
        }
        let irq_status = u16::from_be_bytes([irq_rx[2], irq_rx[3]]);

        // ClearIrqStatus(0x03FF) — clear all IRQ flags
        let clear = [0x97u8, 0x03, 0xFF];
        let mut clear_rx = [0u8; 3];
        let _ = spi_fn(&clear, &mut clear_rx);

        parse_sx1262_irq(irq_status)
    })
}

/// Map the 16-bit SX1262 IRQ status word to a `PhyEvent`.
///
/// Priority order matches SX1262 datasheet §13.3.1:
/// bit 2 = CrcErr, bit 6 = RxTimeout, bit 1 = RxDone, bit 0 = TxDone.
///
/// `pkt_len` in `RxMetadata` is left 0 here — the actual length is read
/// from `GetRxBufferStatus` in `RfPhy::read_rx` (AO context, not ISR).
fn parse_sx1262_irq(status: u16) -> (PhyEvent, RxMetadata) {
    let meta = RxMetadata::default();
    if status & 0x0004 != 0 {
        (PhyEvent::CrcError, meta)
    } else if status & 0x0040 != 0 {
        (PhyEvent::RxTimeout, meta)
    } else if status & 0x0002 != 0 {
        // RxDone: metadata (RSSI, SNR, pkt_len) read by AO via read_rx()
        (PhyEvent::RxDone(meta), meta)
    } else if status & 0x0001 != 0 {
        (PhyEvent::TxDone, meta)
    } else if status & 0x0008 != 0 {
        (PhyEvent::CadDone { channel_active: true })
        // NOTE: CadDone 'channel_active' bit is in the IRQ status word too
        // (bit 3 = CadDetected vs bit 4 = CadDone); for now, assume active.
        // Fix: check bit 11 (CadDetected) to set `channel_active` correctly.
    } else if status & 0x0200 != 0 {
        (PhyEvent::PreambleDetected, meta)
    } else {
        // Unknown or spurious interrupt — treat as TxDone to keep AO moving.
        (PhyEvent::TxDone, meta)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ISR handler
// ─────────────────────────────────────────────────────────────────────────────

/// DIO1 GPIO interrupt handler for the SX1262 transceiver.
///
/// ## What this does (in order)
///
/// 1. Load the `RfStackAO` pointer (set by `register_rf_ao`).
/// 2. Read SX1262 IRQ status via polled SPI (≤ 4 bytes, ≈ 1 µs).
/// 3. Clear IRQ flags.
/// 4. Construct a typed `PhyIrqPayload` event.
/// 5. Call `qk::isr_entry()` — raises BASEPRI to QK ceiling (or CLIC equiv).
/// 6. Post the event to `RfStackAO`.
/// 7. Call `qk::isr_exit()` — lowers BASEPRI and triggers PendSV if needed.
///
/// ## Priority constraint
///
/// On ESP32-C6 (RISC-V CLIC), the DIO1 interrupt priority must be configured
/// at a level that allows `post_from_isr` to be called safely.  Consult the
/// `qk` port documentation for the correct CLIC priority level.
#[no_mangle]
pub extern "C" fn DIO1_IRQHandler() {
    let ao_ptr = RF_AO.load(Ordering::Acquire);
    if ao_ptr.is_null() { return; }

    // SAFETY: `RF_AO` is set once before interrupts are enabled and never
    // changed; the pointed-to AO is `'static`.
    let ao = unsafe { &*ao_ptr };

    // Read and clear IRQ status via polled SPI (must happen first to avoid
    // losing event information if another DIO edge arrives).
    let (event, meta) = read_sx1262_irq_status();

    let sig = match event {
        PhyEvent::TxDone        => RF_PHY_TX_DONE_SIG,
        PhyEvent::RxDone(_)     => RF_PHY_RX_DONE_SIG,
        PhyEvent::CrcError      => RF_PHY_CRC_ERROR_SIG,
        _                       => RF_PHY_IRQ_SIG,
    };

    let payload = PhyIrqPayload { event, meta };
    let ev = DynEvent::with_arc(sig, Arc::new(payload));

    // QK ISR entry/exit manage the scheduler lock and trigger PendSV.
    // On ESP32-C6 (RISC-V) these map to CLIC mip/mie manipulation.
    qk::isr_entry();
    ao.post(ev);
    qk::isr_exit();
}
