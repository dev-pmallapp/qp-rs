#![no_std]

//! Xtensa LX hardware port for the QXK dual-mode kernel.

extern crate alloc;

pub mod context;
pub mod intlevel_cfg;

pub use context::{ContextFrame, ThreadStack};
pub use intlevel_cfg::{qk_lock, qk_unlock, QK_INTLEVEL};

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

/// Xtensa QXK runtime.
pub struct XtensaQxkRuntime {
    kernel: alloc::sync::Arc<spin::Mutex<QxkKernel>>,
}

impl XtensaQxkRuntime {
    /// Creates a runtime from an already-built kernel.
    pub fn new(kernel: QxkKernel) -> Self {
        Self {
            kernel: alloc::sync::Arc::new(spin::Mutex::new(kernel)),
        }
    }

    /// Starts all registered active objects and threads.
    pub fn start(&self) {
        self.kernel.lock().start();
        #[cfg(feature = "hw")]
        unsafe {
            KERNEL_PTR = &*self.kernel as *const spin::Mutex<QxkKernel>;
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
    pub fn run_until_idle(&self) {
        self.kernel.lock().run_until_idle();
    }

    /// Pends the scheduler software interrupt.
    #[inline]
    pub fn pend_sv() {
        #[cfg(feature = "hw")]
        unsafe {
            // Write to INTSET register to trigger software interrupt 2.
            core::arch::asm!("wsr.intset {0}", in(reg) 1 << 2, options(nomem, nostack));
        }
    }
}

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

// ── Xtensa Window Overflow/Underflow handlers ─────────────────────────────────

#[cfg(feature = "hw")]
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn WindowOverflow4() {
    core::arch::naked_asm!(
        "s32e a0, a9, -16",
        "s32e a1, a9, -12",
        "s32e a2, a9, -8",
        "s32e a3, a9, -4",
        "rfwo"
    );
}

#[cfg(feature = "hw")]
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn WindowUnderflow4() {
    core::arch::naked_asm!(
        "l32e a0, a5, -16",
        "l32e a1, a5, -12",
        "l32e a2, a5, -8",
        "l32e a3, a5, -4",
        "rfwu"
    );
}

#[cfg(feature = "hw")]
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn WindowOverflow8() {
    core::arch::naked_asm!(
        "s32e a0, a13, -16",
        "s32e a1, a13, -12",
        "s32e a2, a13, -8",
        "s32e a3, a13, -4",
        "s32e a4, a9, -32",
        "s32e a5, a9, -28",
        "s32e a6, a9, -24",
        "s32e a7, a9, -20",
        "rfwo"
    );
}

#[cfg(feature = "hw")]
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn WindowUnderflow8() {
    core::arch::naked_asm!(
        "l32e a0, a9, -16",
        "l32e a1, a9, -12",
        "l32e a2, a9, -8",
        "l32e a3, a9, -4",
        "l32e a4, a5, -32",
        "l32e a5, a5, -28",
        "l32e a6, a5, -24",
        "l32e a7, a5, -20",
        "rfwu"
    );
}

#[cfg(feature = "hw")]
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn WindowOverflow12() {
    core::arch::naked_asm!(
        "s32e a0, a13, -16",
        "s32e a1, a13, -12",
        "s32e a2, a13, -8",
        "s32e a3, a13, -4",
        "s32e a4, a9, -32",
        "s32e a5, a9, -28",
        "s32e a6, a9, -24",
        "s32e a7, a9, -20",
        "s32e a8, a9, -48",
        "s32e a9, a9, -44",
        "s32e a10, a9, -40",
        "s32e a11, a9, -36",
        "rfwo"
    );
}

#[cfg(feature = "hw")]
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn WindowUnderflow12() {
    core::arch::naked_asm!(
        "l32e a0, a13, -16",
        "l32e a1, a13, -12",
        "l32e a2, a13, -8",
        "l32e a3, a13, -4",
        "l32e a4, a9, -32",
        "l32e a5, a9, -28",
        "l32e a6, a9, -24",
        "l32e a7, a9, -20",
        "l32e a8, a5, -48",
        "l32e a9, a5, -44",
        "l32e a10, a5, -40",
        "l32e a11, a5, -36",
        "rfwu"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxk::QxkKernelBuilder;

    #[test]
    fn runtime_builds_without_hw() {
        let kernel = QxkKernelBuilder::new().build().expect("kernel should build");
        let runtime = XtensaQxkRuntime::new(kernel);
        runtime.start();
        runtime.run_until_idle();
    }

    #[test]
    fn context_frame_size_is_correct() {
        assert_eq!(core::mem::size_of::<ContextFrame>(), 19 * 4);
    }
}
