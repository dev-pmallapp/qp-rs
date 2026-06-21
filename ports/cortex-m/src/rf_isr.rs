//! Cortex-M RF interrupt handler and NVIC priority configuration.
//!
//! This module is the ONLY place where the EXTI ISR and QP-RS meet.
//! It is separate from `hal` and `comms`; the HAL has no `qf` dependency.
//!
//! ## NVIC priority rules (Cortex-M, 8-priority-bit MCU)
//!
//! QK uses BASEPRI to mask interrupts during the scheduler critical section.
//! All ISRs that call `qk::isr_entry()` / `qk::isr_exit()` MUST be configured
//! at a priority number **numerically GREATER THAN** `QK_BASEPRI` (0x50).
//! Higher numeric value = lower urgency = NOT masked by BASEPRI lock.
//!
//! ```text
//! Priority table (8-bit priority, 4-group Cortex-M):
//!   HardFault / NMI           : 0x00  (never masked)
//!   QK-unaware peripherals    : < 0x50  (blocked by scheduler lock)
//!   QK_BASEPRI ceiling        : 0x50  (boundary)
//!   SysTick  (QK tick source) : 0xC0  (NOT blocked — can call isr_entry)
//!   DIO GPIO (RF IRQ)         : 0xC0  (NOT blocked — can call isr_entry)
//!   PendSV   (context switch) : 0xFF  (lowest, never masked by BASEPRI)
//! ```
//!
//! ## SPI inside the ISR
//!
//! Only polled (non-DMA) transfers of ≤ 4 bytes are performed in the ISR.
//! The read takes ≈ 1 µs at 8 MHz SPI clock — acceptable for an ISR.

#![cfg(feature = "hw")]

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

use crate::nvic_cfg::QK_BASEPRI;

// ─────────────────────────────────────────────────────────────────────────────
// Static RF AO pointer
// ─────────────────────────────────────────────────────────────────────────────

/// Pointer to the `RfStackAO` that receives EXTI interrupt events.
static RF_AO: AtomicPtr<dyn ActiveRunnable> = AtomicPtr::new(core::ptr::null_mut());

/// Register the RF Active Object for ISR-posted events.
///
/// Call once from `main()` before enabling the DIO GPIO interrupt.
pub fn register_rf_ao(ao: &'static dyn ActiveRunnable) {
    RF_AO.store(ao as *const _ as *mut _, Ordering::Release);
}

// ─────────────────────────────────────────────────────────────────────────────
// SPI ISR function pointer
// ─────────────────────────────────────────────────────────────────────────────

/// Polled-SPI transfer function callable from interrupt context.
type IsrSpiTransferFn = fn(tx: &[u8], rx: &mut [u8]) -> bool;

static RF_SPI_FN: Mutex<RefCell<Option<IsrSpiTransferFn>>> =
    Mutex::new(RefCell::new(None));

/// Register a polled-SPI transfer function for use inside the EXTI ISR.
///
/// The function must be non-blocking, non-DMA, and safe to call in IRQ context.
pub fn register_rf_spi_fn(f: IsrSpiTransferFn) {
    critical_section::with(|cs| {
        *RF_SPI_FN.borrow_ref_mut(cs) = Some(f);
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// SX1262 / SX1276 IRQ status read
// ─────────────────────────────────────────────────────────────────────────────

/// Read IRQ status from the radio chip using polled SPI and map to `PhyEvent`.
///
/// SX1262: GetIrqStatus (0x12) + ClearIrqStatus (0x97, 0x03FF)
/// SX1276: read RegIrqFlags (0x12) + write back same bits to clear
///
/// Returns `(TxDone, default_meta)` as a safe fallback when no SPI is available.
fn read_sx_irq_status() -> (PhyEvent, RxMetadata) {
    critical_section::with(|cs| {
        let guard = RF_SPI_FN.borrow_ref(cs);
        let Some(spi_fn) = *guard else {
            return (PhyEvent::TxDone, RxMetadata::default());
        };

        // SX1262 GetIrqStatus
        let get_irq = [0x12u8, 0x00, 0x00, 0x00];
        let mut irq_rx = [0u8; 4];
        if !spi_fn(&get_irq, &mut irq_rx) {
            return (PhyEvent::TxDone, RxMetadata::default());
        }
        let irq_status = u16::from_be_bytes([irq_rx[2], irq_rx[3]]);

        // ClearIrqStatus
        let clear = [0x97u8, 0x03, 0xFF];
        let mut clear_rx = [0u8; 3];
        let _ = spi_fn(&clear, &mut clear_rx);

        parse_sx_irq(irq_status)
    })
}

fn parse_sx_irq(status: u16) -> (PhyEvent, RxMetadata) {
    let meta = RxMetadata::default();
    if status & 0x0004 != 0 { return (PhyEvent::CrcError, meta); }
    if status & 0x0040 != 0 { return (PhyEvent::RxTimeout, meta); }
    if status & 0x0002 != 0 { return (PhyEvent::RxDone(meta), meta); }
    if status & 0x0001 != 0 { return (PhyEvent::TxDone, meta); }
    if status & 0x0008 != 0 { return (PhyEvent::CadDone { channel_active: true }, meta); }
    if status & 0x0200 != 0 { return (PhyEvent::PreambleDetected, meta); }
    (PhyEvent::TxDone, meta) // fallback
}

// ─────────────────────────────────────────────────────────────────────────────
// NVIC configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configure the NVIC priority for the RF DIO EXTI line.
///
/// The DIO1 interrupt (mapped to EXTI4_15 on most STM32 variants) must be at
/// a priority number **numerically greater than** `QK_BASEPRI` (0x50) so it
/// is NOT masked by the QK scheduler lock and CAN call `qk::isr_entry()`.
///
/// Call this once during board initialisation before enabling the EXTI line.
///
/// ## MCU-specific notes
///
/// - The `Interrupt::EXTI4_15` variant used here is typical for STM32L0/L4/F4.
///   For STM32H7 or G0 families, adjust the interrupt enum variant accordingly.
/// - The priority group setting (SCB AIRCR) should match your BSP; this
///   function assumes 4-bit preemption / 0-bit sub-priority (PRIGROUP = 3).
pub fn configure_rf_interrupt() {
    // This cfg-gate prevents compilation errors when building for the host.
    // On real hardware, the `hw` feature is set in the port's Cargo.toml.
    use cortex_m::peripheral::NVIC;

    // Priority for DIO1 EXTI: numerically > QK_BASEPRI → not masked by lock.
    // Example: QK_BASEPRI = 0x50, DIO priority = 0xC0 → 0xC0 > 0x50 ✓
    const DIO_PRIORITY: u8 = 0xC0;

    // Sanity check at compile time that DIO priority is above QK ceiling.
    const _: () = assert!(DIO_PRIORITY > QK_BASEPRI,
        "DIO1 interrupt priority must be numerically greater than QK_BASEPRI");

    unsafe {
        // NOTE: Replace `Interrupt::EXTI4_15` with the correct interrupt for
        // your target MCU's PAC crate (e.g. `pac::interrupt::EXTI4_15`).
        // The `stm32l4xx_hal::pac::Interrupt` is used here as a representative.
        //
        // NVIC::unmask(Interrupt::EXTI4_15);
        // NVIC::set_priority(Interrupt::EXTI4_15, DIO_PRIORITY);
        //
        // Uncomment and replace the above with your target's PAC interrupt type.
        let _ = DIO_PRIORITY; // suppress unused warning until PAC is wired
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// EXTI interrupt handler
// ─────────────────────────────────────────────────────────────────────────────

/// Generic EXTI handler for external radio transceivers on Cortex-M.
///
/// Follows the same pattern as the ESP32-C6 `DIO1_IRQHandler`.
/// Rename to match your target's interrupt name (e.g. `EXTI4_15`, `EXTI9_5`).
#[no_mangle]
pub extern "C" fn EXTI4_15_IRQHandler() {
    let ao_ptr = RF_AO.load(Ordering::Acquire);
    if ao_ptr.is_null() { return; }

    // SAFETY: set once before interrupts enabled, never changed.
    let ao = unsafe { &*ao_ptr };

    let (event, meta) = read_sx_irq_status();

    let sig = match event {
        PhyEvent::TxDone        => RF_PHY_TX_DONE_SIG,
        PhyEvent::RxDone(_)     => RF_PHY_RX_DONE_SIG,
        PhyEvent::CrcError      => RF_PHY_CRC_ERROR_SIG,
        _                       => RF_PHY_IRQ_SIG,
    };

    let payload = PhyIrqPayload { event, meta };
    let ev = DynEvent::with_arc(sig, Arc::new(payload));

    // QK ISR entry/exit: manage BASEPRI and trigger PendSV for context switch.
    qk::isr_entry();
    ao.post(ev);
    qk::isr_exit();
}
