//! SCB (System Control Block) and Cache operations for ARM Cortex-M7

/// Flush cache lines covering `buf` to SRAM before a DMA TX.
///
/// # Safety
/// `buf` must be 32-byte aligned (M7 cache line size).
pub unsafe fn clean_dcache(buf: &[u8]) {
    let (mut addr, end) = (buf.as_ptr() as usize & !0x1F, buf.as_ptr() as usize + buf.len());
    while addr < end {
        core::ptr::write_volatile(0xE000_ED68 as *mut u32, addr as u32); // DCCMVAC
        addr += 32;
    }
    crate::asm::dsb();
    crate::asm::isb();
}

/// Invalidate cache lines covering `buf` after a DMA RX.
///
/// # Safety
/// Caller must ensure cache line coherency.
pub unsafe fn invalidate_dcache(buf: &[u8]) {
    let (mut addr, end) = (buf.as_ptr() as usize & !0x1F, buf.as_ptr() as usize + buf.len());
    while addr < end {
        core::ptr::write_volatile(0xE000_ED6C as *mut u32, addr as u32); // DCIMVAC
        addr += 32;
    }
    crate::asm::dsb();
    crate::asm::isb();
}
