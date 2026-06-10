#![no_std]

//! Cortex-M hardware port for the QXK dual-mode kernel.
//!
//! ## What this port provides
//!
//! - [`ContextFrame`] — layout of the exception stack frame used to
//!   initialise extended-thread stacks and resume execution after a context
//!   switch.
//! - [`ThreadStack`] — helper that initialises a raw byte slice as a
//!   Cortex-M initial stack frame, ready for the first `PendSV` restore.
//! - [`CortexMQxkRuntime`] — integrates [`qxk`]'s dual-mode scheduler with the
//!   Cortex-M exception model.  The `PendSV` exception is used as the
//!   context-switch mechanism; `SVC #0` is used as the scheduler-lock primitive.
//! - Stubs for `PendSV_Handler` and `SVC_Handler` that must be provided in the
//!   `#[cfg(feature = "hw")]` configuration.  Without `hw` they are unreachable
//!   no-op stubs so the crate compiles on the host for testing.
//!
//! ## Context-switch model
//!
//! On Cortex-M3/M4/M7 the processor automatically stacks eight registers on
//! exception entry (`r0–r3`, `r12`, `lr`, `pc`, `xpsr`).  The PendSV handler
//! must save/restore the callee-saved registers (`r4–r11`) and update `SP`.
//! On FP-capable cores (M4F/M7F) the lazy-stacking FPU registers must also be
//! handled.
//!
//! ```text
//! ┌──────────────────────────────────────────────────────┐
//! │  Higher address (bottom of descending stack)         │
//! │  [auto-saved by hardware on exception entry]         │
//! │   xpsr  pc  lr  r12  r3  r2  r1  r0                  │
//! │  [saved by PendSV_Handler (software)]                │
//! │   r11  r10  r9  r8  r7  r6  r5  r4                   │
//! │  ← SP after software save                            │
//! └──────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage sketch
//!
//! ```ignore
//! // In your application startup (after BSS/data init):
//! let mut runtime = CortexMQxkRuntime::new(kernel, timers);
//! runtime.start();   // marks all AOs & threads ready
//! // ...
//! // In SysTick_Handler:
//! runtime.tick();    // advance timer wheel; pend PendSV if higher-prio task ready
//! ```

extern crate alloc;

pub mod context;

pub use context::{ContextFrame, ThreadStack};

use qxk::QxkKernel;

/// Cortex-M QXK runtime.
///
/// Owns the QXK kernel and glues it to the Cortex-M exception model.
/// `start()` enables PendSV at the lowest priority so it only fires when
/// no higher-priority exception is active (correct for context switches).
pub struct CortexMQxkRuntime {
    kernel: alloc::sync::Arc<spin::Mutex<QxkKernel>>,
}

impl CortexMQxkRuntime {
    /// Creates a runtime from an already-built kernel.
    pub fn new(kernel: QxkKernel) -> Self {
        Self {
            kernel: alloc::sync::Arc::new(spin::Mutex::new(kernel)),
        }
    }

    /// Starts all registered active objects and threads, then sets PendSV to
    /// the lowest interrupt priority.
    ///
    /// On a real target this writes to `SCB.SHPR3`.  Without the `hw` feature
    /// this is a no-op; the kernel still starts in the polling model.
    pub fn start(&self) {
        self.kernel.lock().start();
        #[cfg(feature = "hw")]
        Self::set_pendsv_priority_lowest();
    }

    /// Runs one scheduling cycle: dispatch pending events, then let threads run.
    ///
    /// Call this in the SysTick handler after advancing the timer wheel.
    pub fn run_until_idle(&self) {
        self.kernel.lock().run_until_idle();
    }

    /// Pends the PendSV exception to trigger a context switch.
    ///
    /// Safe to call from any exception or thread context.  On real hardware
    /// this sets bit 28 in `SCB.ICSR`; in the host build it is a no-op.
    #[inline]
    pub fn pend_sv() {
        #[cfg(feature = "hw")]
        unsafe {
            // SAFETY: write-only bit in SCB_ICSR, no other side effects.
            const SCB_ICSR: *mut u32 = 0xE000_ED04 as *mut u32;
            core::ptr::write_volatile(SCB_ICSR, 1 << 28);
        }
    }

    #[cfg(feature = "hw")]
    fn set_pendsv_priority_lowest() {
        // Set PendSV (exception 14) to the lowest priority (0xFF on M3/M4).
        // SHPR3 bits [23:16] hold PendSV priority.
        unsafe {
            const SCB_SHPR3: *mut u32 = 0xE000_ED20 as *mut u32;
            let prev = core::ptr::read_volatile(SCB_SHPR3);
            core::ptr::write_volatile(SCB_SHPR3, (prev & !0x00FF_0000) | 0x00FF_0000);
        }
    }
}

// ── PendSV / SVC stubs ────────────────────────────────────────────────────────

/// PendSV exception handler (context switch).
///
/// On a real Cortex-M target the linker script must route `PendSV_Handler` here.
/// The assembly body saves callee-saved registers (`r4–r11`), switches `SP` to
/// the next thread's saved `SP`, and restores `r4–r11`.  On FP cores the
/// lazy-stacking `s16–s31` must also be saved/restored.
///
/// # Safety
///
/// This is an interrupt handler; it must be called only by the processor's
/// exception entry mechanism.
#[cfg(feature = "hw")]
#[no_mangle]
pub unsafe extern "C" fn PendSV_Handler() {
    // TODO: implement in inline assembly for the target triple.
    // Required assembly steps:
    //   1. MRS  r0, PSP          ; get current thread SP
    //   2. STMDB r0!, {r4-r11}   ; save callee-saved registers (+ s16-s31 on FP)
    //   3. Store r0 to current thread's stack pointer field
    //   4. Call scheduler to pick next thread (via SVC or direct call)
    //   5. Load next thread's saved SP into r0
    //   6. LDMIA r0!, {r4-r11}   ; restore callee-saved registers
    //   7. MSR PSP, r0           ; set PSP to restored value
    //   8. BX   LR               ; return from exception
    core::hint::unreachable_unchecked()
}

/// SVC exception handler (privileged kernel calls).
///
/// Used for operations that must execute at the kernel privilege level:
/// scheduler lock, thread yield, etc.
#[cfg(feature = "hw")]
#[no_mangle]
pub unsafe extern "C" fn SVC_Handler() {
    // TODO: decode SVC number from stacked PC and dispatch.
    core::hint::unreachable_unchecked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxk::QxkKernelBuilder;

    #[test]
    fn runtime_builds_without_hw() {
        let kernel = QxkKernelBuilder::new().build().expect("kernel should build");
        let runtime = CortexMQxkRuntime::new(kernel);
        runtime.start();
        runtime.run_until_idle();
    }

    #[test]
    fn context_frame_size_is_correct() {
        assert_eq!(core::mem::size_of::<ContextFrame>(), 8 * 4);
    }
}
