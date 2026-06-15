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
pub mod nvic_cfg;

pub use context::{ContextFrame, ThreadStack};
pub use nvic_cfg::{qk_lock, qk_unlock, QK_BASEPRI};

use qxk::QxkKernel;
#[cfg(feature = "hw")]
use qxk::ScheduleMode;

#[cfg(feature = "hw")]
#[no_mangle]
pub static mut CURRENT_THREAD_SP: *mut u8 = core::ptr::null_mut();

#[cfg(feature = "hw")]
#[no_mangle]
pub static mut NEXT_THREAD_SP: *mut u8 = core::ptr::null_mut();

#[cfg(feature = "hw")]
static mut KERNEL_PTR: *const spin::Mutex<QxkKernel> = core::ptr::null();

#[cfg(feature = "hw")]
const MAX_THREADS: usize = 32;

#[cfg(feature = "hw")]
static mut THREAD_SPS: [*mut u8; MAX_THREADS] = [core::ptr::null_mut(); MAX_THREADS];

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
        unsafe {
            KERNEL_PTR = &*self.kernel as *const spin::Mutex<QxkKernel>;
            Self::set_pendsv_priority_lowest();
        }
    }

    /// Registers a thread's stack pointer with the runtime.
    pub fn register_thread_sp(&self, _id: u8, _sp: *mut u8) {
        #[cfg(feature = "hw")]
        unsafe {
            let id = _id as usize;
            if id < MAX_THREADS {
                THREAD_SPS[id] = _sp;
                if CURRENT_THREAD_SP.is_null() {
                    CURRENT_THREAD_SP = _sp;
                }
            }
        }
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

#[cfg(feature = "hw")]
#[no_mangle]
pub unsafe extern "C" fn qxk_schedule() {
    if KERNEL_PTR.is_null() {
        return;
    }
    let kernel = &*KERNEL_PTR;
    let guard = kernel.lock();
    let next_mode = guard.scheduler().plan_next();
    guard.scheduler().set_active(next_mode);
    if let ScheduleMode::ExtendedThread { id, .. } = next_mode {
        let thread_id = id.0 as usize;
        if thread_id < MAX_THREADS {
            NEXT_THREAD_SP = THREAD_SPS[thread_id];
        }
    }
}

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
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn PendSV_Handler() {
    core::arch::naked_asm!(
        "mrs r0, psp",
        "stmdb r0!, {{r4-r11}}",
        "ldr r1, =CURRENT_THREAD_SP",
        "str r0, [r1]",
        "push {{lr}}",
        "bl qxk_schedule",
        "pop {{lr}}",
        "ldr r1, =NEXT_THREAD_SP",
        "ldr r0, [r1]",
        "ldr r2, =CURRENT_THREAD_SP",
        "str r0, [r2]",
        "ldmia r0!, {{r4-r11}}",
        "msr psp, r0",
        "bx lr"
    );
}

#[cfg(feature = "hw")]
#[no_mangle]
pub unsafe extern "C" fn rust_svc_handler(frame: *mut ContextFrame) {
    let pc = (*frame).pc;
    // PC points to instruction after SVC. The SVC opcode is 2 bytes before PC.
    let svc_instr_ptr = (pc - 2) as *const u16;
    let svc_instr = core::ptr::read_volatile(svc_instr_ptr);
    let svc_num = (svc_instr & 0xFF) as u8;

    match svc_num {
        0 => {
            // SVC #0: scheduler lock/unlock/yield.
            CortexMQxkRuntime::pend_sv();
        }
        _ => {}
    }
}

/// SVC exception handler (privileged kernel calls).
///
/// Used for operations that must execute at the kernel privilege level:
/// scheduler lock, thread yield, etc.
#[cfg(feature = "hw")]
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn SVC_Handler() {
    core::arch::naked_asm!(
        "tst lr, #4",
        "ite eq",
        "mrseq r0, msp",
        "mrsne r0, psp",
        "b {rust_svc_handler}",
        rust_svc_handler = sym rust_svc_handler
    );
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
