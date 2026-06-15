//! BASEPRI register access for ARMv7-M

/// Read the current BASEPRI register value
#[inline(always)]
pub fn read() -> u8 {
    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    {
        let v: usize;
        unsafe {
            core::arch::asm!("mrs {}, BASEPRI", out(reg) v, options(nomem, nostack, preserves_flags))
        }
        v as u8
    }
    #[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
    {
        0
    }
}

/// Write a new value to the BASEPRI register
///
/// # Safety
/// Caller must ensure that restoring the previous BASEPRI value is handled properly.
#[inline(always)]
pub unsafe fn write(val: u8) {
    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    {
        let val_usize = val as usize;
        unsafe {
            core::arch::asm!("msr BASEPRI, {}", in(reg) val_usize, options(nomem, nostack, preserves_flags))
        }
    }
    #[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
    {
        let _ = val;
    }
}
