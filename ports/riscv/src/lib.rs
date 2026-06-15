#![no_std]

//! RISC-V hardware port for the QXK dual-mode kernel.

extern crate alloc;

pub mod context;
pub mod mstatus_cfg;

pub use context::{ContextFrame, ThreadStack};
pub use mstatus_cfg::{qk_lock, qk_unlock};

use qf::kernel::Kernel;
use qf::time::TimerWheel;
use qk::QkKernel;
use qk::QkTimerWheel;
use qxk::QxkKernel;
#[cfg(feature = "hw")]
use qxk::ScheduleMode;

/// RISC-V QF runtime.
pub struct RiscVQfRuntime {
    kernel: alloc::sync::Arc<Kernel>,
    timers: TimerWheel,
}

impl RiscVQfRuntime {
    /// Creates a runtime from an already-built kernel.
    pub fn new(kernel: Kernel) -> Self {
        let kernel = alloc::sync::Arc::new(kernel);
        let timers = TimerWheel::new(alloc::sync::Arc::clone(&kernel));
        Self { kernel, timers }
    }

    /// Starts all registered active objects.
    pub fn start(&self) {
        self.kernel.start();
    }

    /// Registers a time event.
    pub fn register_time_event(&mut self, event: alloc::sync::Arc<qf::time::TimeEvent>) {
        self.timers.register(event);
    }

    /// Processes a system tick.
    pub fn tick(&self) -> Result<(), qf::time::TimeEventError> {
        self.timers.tick()
    }

    /// Processes a system tick from an ISR.
    pub fn tick_from_isr(&self) -> Result<(), qf::time::TimeEventError> {
        self.timers.tick_from_isr()
    }

    /// Runs one scheduling cycle.
    pub fn run_until_idle(&self) {
        self.kernel.run_until_idle();
    }

    /// Check if there is pending work.
    pub fn has_pending_work(&self) -> bool {
        self.kernel.has_pending_work()
    }
}

/// RISC-V QK runtime.
pub struct RiscVQkRuntime {
    kernel: alloc::sync::Arc<QkKernel>,
    timers: QkTimerWheel,
}

impl RiscVQkRuntime {
    /// Creates a runtime from an already-built kernel.
    pub fn new(kernel: QkKernel) -> Self {
        let kernel = alloc::sync::Arc::new(kernel);
        let timers = QkTimerWheel::new(alloc::sync::Arc::clone(&kernel));
        Self { kernel, timers }
    }

    /// Starts the kernel and all registered active objects.
    pub fn start(&self) {
        self.kernel.start();
    }

    /// Registers a time event.
    pub fn register_time_event(&mut self, event: alloc::sync::Arc<qf::time::TimeEvent>) {
        self.timers.register(event);
    }

    /// Processes a system tick.
    pub fn tick(&self) -> Result<(), qk::QkTimeEventError> {
        self.timers.tick()
    }

    /// Processes a system tick from an ISR.
    pub fn tick_from_isr(&self) -> Result<(), qk::QkTimeEventError> {
        self.timers.tick_from_isr()
    }

    /// Runs one scheduling cycle.
    pub fn run_until_idle(&self) {
        self.kernel.run_until_idle();
    }

    /// Check if there is pending work.
    pub fn has_pending_work(&self) -> bool {
        self.kernel.has_pending_work()
    }
}

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

/// RISC-V QXK runtime.
pub struct RiscVQxkRuntime {
    kernel: alloc::sync::Arc<spin::Mutex<QxkKernel>>,
}

impl RiscVQxkRuntime {
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

    /// Runs one scheduling cycle.
    pub fn run_until_idle(&self) {
        self.kernel.lock().run_until_idle();
    }

    /// Pends the scheduler software interrupt.
    #[inline]
    pub fn pend_sv() {
        #[cfg(feature = "hw")]
        unsafe {
            // Write 1 to CLINT MSIP0 to trigger software interrupt.
            const CLINT_MSIP0: *mut u32 = 0x0200_0000 as *mut u32;
            core::ptr::write_volatile(CLINT_MSIP0, 1);
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

// ── RISC-V Naked trap handler ──────────────────────────────────────────────────

#[cfg(feature = "hw")]
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn TrapHandler() {
    core::arch::naked_asm!(
        // Save caller-saved registers (ContextFrame space)
        "addi sp, sp, -72",
        "sw ra, 0(sp)",
        "sw t0, 4(sp)",
        "sw t1, 8(sp)",
        "sw t2, 12(sp)",
        "sw a0, 16(sp)",
        "sw a1, 20(sp)",
        "sw a2, 24(sp)",
        "sw a3, 28(sp)",
        "sw a4, 32(sp)",
        "sw a5, 36(sp)",
        "sw a6, 40(sp)",
        "sw a7, 44(sp)",
        "sw t3, 48(sp)",
        "sw t4, 52(sp)",
        "sw t5, 56(sp)",
        "sw t6, 60(sp)",
        "csrr t0, mepc",
        "sw t0, 64(sp)",
        "csrr t0, mstatus",
        "sw t0, 68(sp)",

        // Check cause
        "csrr t0, mcause",
        // MSB set (interrupt) and exception code 3 (Machine Software Interrupt)
        "li t1, 0x80000003",
        "bne t0, t1, 1f",

        // Clear the MSIP interrupt in CLINT
        "li t1, 0x02000000",
        "sw zero, 0(t1)",

        // Save current thread's callee-saved registers on stack
        "addi sp, sp, -48",
        "sw s0, 0(sp)",
        "sw s1, 4(sp)",
        "sw s2, 8(sp)",
        "sw s3, 12(sp)",
        "sw s4, 16(sp)",
        "sw s5, 20(sp)",
        "sw s6, 24(sp)",
        "sw s7, 28(sp)",
        "sw s8, 32(sp)",
        "sw s9, 36(sp)",
        "sw s10, 40(sp)",
        "sw s11, 44(sp)",

        // CURRENT_THREAD_SP = sp
        "la t0, CURRENT_THREAD_SP",
        "sw sp, 0(t0)",

        // Call qxk_schedule
        "call qxk_schedule",

        // sp = NEXT_THREAD_SP
        "la t0, NEXT_THREAD_SP",
        "lw sp, 0(t0)",
        "la t1, CURRENT_THREAD_SP",
        "sw sp, 0(t1)",

        // Restore next thread's callee-saved registers
        "lw s0, 0(sp)",
        "lw s1, 4(sp)",
        "lw s2, 8(sp)",
        "lw s3, 12(sp)",
        "lw s4, 16(sp)",
        "lw s5, 20(sp)",
        "lw s6, 24(sp)",
        "lw s7, 28(sp)",
        "lw s8, 32(sp)",
        "lw s9, 36(sp)",
        "lw s10, 40(sp)",
        "lw s11, 44(sp)",
        "addi sp, sp, 48",

        "1:",
        // Restore caller-saved registers
        "lw ra, 0(sp)",
        "lw t0, 4(sp)",
        "lw t1, 8(sp)",
        "lw t2, 12(sp)",
        "lw a0, 16(sp)",
        "lw a1, 20(sp)",
        "lw a2, 24(sp)",
        "lw a3, 28(sp)",
        "lw a4, 32(sp)",
        "lw a5, 36(sp)",
        "lw a6, 40(sp)",
        "lw a7, 44(sp)",
        "lw t3, 48(sp)",
        "lw t4, 52(sp)",
        "lw t5, 56(sp)",
        "lw t6, 60(sp)",
        "lw t0, 64(sp)",
        "csrw mepc, t0",
        "lw t0, 68(sp)",
        "csrw mstatus, t0",
        "addi sp, sp, 72",
        "mret"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxk::QxkKernelBuilder;

    #[test]
    fn runtime_builds_without_hw() {
        let kernel = QxkKernelBuilder::new().build().expect("kernel should build");
        let runtime = RiscVQxkRuntime::new(kernel);
        runtime.start();
        runtime.run_until_idle();
    }

    #[test]
    fn context_frame_size_is_correct() {
        assert_eq!(core::mem::size_of::<ContextFrame>(), 18 * 4);
    }
}
