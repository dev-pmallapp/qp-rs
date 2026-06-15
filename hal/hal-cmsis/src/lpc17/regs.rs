//! LPC1768 register maps and base addresses (NXP Cortex-M3)

use hal::mmio::{RO, RW, WO};

// LPC1768 Peripheral Base Addresses
pub const GPIO_BASE:  usize = 0x2009_C000; // Fast GPIO port 0 base
pub const SSP0_BASE:  usize = 0x4008_8000; // SSP0 (SPI master)
pub const SSP1_BASE:  usize = 0x4005_C000; // SSP1
pub const UART0_BASE: usize = 0x4000_C000;

/// LPC1768 Fast GPIO port registers (0x20 bytes per port, stride 0x20)
#[repr(C)]
pub struct GpioPortRegs {
    pub dir:  RW<u32>,  // 0x00 Direction (1 = output)
    _r0:      [u32; 3], // 0x04 – 0x0C (reserved)
    pub mask: RW<u32>,  // 0x10 Bit-mask for operations
    pub pin:  RW<u32>,  // 0x14 Pin state
    pub set:  WO<u32>,  // 0x18 Set output high
    pub clr:  WO<u32>,  // 0x1C Set output low
}

/// LPC1768 SSP (SPI) registers
#[repr(C)]
pub struct SspRegs {
    pub cr0:   RW<u32>,  // 0x000 Control register 0
    pub cr1:   RW<u32>,  // 0x004 Control register 1
    pub dr:    RW<u32>,  // 0x008 Data register (FIFO R/W)
    pub sr:    RO<u32>,  // 0x00C Status register
    pub cpsr:  RW<u32>,  // 0x010 Clock prescale (even, 2–254)
    pub imsc:  RW<u32>,  // 0x014 Interrupt mask
    pub ris:   RO<u32>,  // 0x018 Raw interrupt status
    pub mis:   RO<u32>,  // 0x01C Masked interrupt status
    pub icr:   WO<u32>,  // 0x020 Interrupt clear
    pub dmacr: RW<u32>,  // 0x024 DMA control
}

// SSP SR bits
pub const SSP_SR_TFE: u32 = 1 << 0; // TX FIFO empty
pub const SSP_SR_TNF: u32 = 1 << 1; // TX FIFO not full
pub const SSP_SR_RNE: u32 = 1 << 2; // RX FIFO not empty
pub const SSP_SR_BSY: u32 = 1 << 4; // Busy

// SSP CR1 bits
pub const SSP_CR1_SSE: u32 = 1 << 1; // SSP Enable

/// LPC1768 UART registers
#[repr(C)]
pub struct UartRegs {
    pub rbr_thr_dll: RW<u32>,  // 0x000 RBR (read)/THR (write)/DLL (DLAB)
    pub dlm_ier:     RW<u32>,  // 0x004 DLM (DLAB) / IER
    pub iir_fcr:     RW<u32>,  // 0x008 IIR (read) / FCR (write)
    pub lcr:         RW<u32>,  // 0x00C Line control
    pub mcr:         RW<u32>,  // 0x010 Modem control
    pub lsr:         RO<u32>,  // 0x014 Line status
    pub msr:         RO<u32>,  // 0x018 Modem status
    pub scr:         RW<u32>,  // 0x01C Scratch pad
    pub acr:         RW<u32>,  // 0x020 Auto-baud control
    _r0:             u32,      // 0x024 (reserved)
    pub fdr:         RW<u32>,  // 0x028 Fractional divider
    _r1:             u32,      // 0x02C (reserved)
    pub ter:         RW<u32>,  // 0x030 Transmit enable
}

// LSR bits
pub const UART_LSR_RDR:  u32 = 1 << 0; // Receive data ready
pub const UART_LSR_THRE: u32 = 1 << 5; // THR empty (TX ready)

// LCR DLAB bit — set to access DLL/DLM
pub const UART_LCR_DLAB: u32 = 1 << 7;

/// Get a reference to the GPIO port register block for the given port (0–4).
///
/// # Safety
/// Caller must guarantee exclusive access to the port.
pub unsafe fn gpio_port(port: u8) -> &'static GpioPortRegs {
    let addr = GPIO_BASE + (port as usize) * 0x20;
    &*(addr as *const GpioPortRegs)
}
