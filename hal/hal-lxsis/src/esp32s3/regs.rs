//! ESP32-S3 peripheral register maps and base addresses

use hal::mmio::{RO, RW, WO};

// ESP32-S3 Peripheral Base Addresses
pub const GPIO_BASE:  usize = 0x6000_4000;
pub const SPI2_BASE:  usize = 0x6002_4000; // FSPI/HSPI
pub const UART0_BASE: usize = 0x6000_0000;

/// ESP32-S3 GPIO registers
#[repr(C)]
pub struct GpioRegs {
    pub bt_select:   RW<u32>,   // 0x000
    pub out:         RW<u32>,   // 0x004
    pub out_w1ts:    WO<u32>,   // 0x008
    pub out_w1tc:    WO<u32>,   // 0x00C
    pub out1:        RW<u32>,   // 0x010
    pub out1_w1ts:   WO<u32>,   // 0x014
    pub out1_w1tc:   WO<u32>,   // 0x018
    _reserved0:      u32,       // 0x01C
    pub enable:      RW<u32>,   // 0x020
    pub enable_w1ts: WO<u32>,   // 0x024
    pub enable_w1tc: WO<u32>,   // 0x028
    pub enable1:     RW<u32>,   // 0x02C
    pub enable1_w1ts: WO<u32>,  // 0x030
    pub enable1_w1tc: WO<u32>,  // 0x034
    _reserved1:      u32,       // 0x038
    pub in_:         RO<u32>,   // 0x03C
    pub in1:         RO<u32>,   // 0x040
}

/// ESP32-S3 SPI registers
#[repr(C)]
pub struct SpiRegs {
    pub cmd:       RW<u32>,   // 0x00
    pub addr:      RW<u32>,   // 0x04
    pub ctrl:      RW<u32>,   // 0x08
    pub ctrl1:     RW<u32>,   // 0x0C
    pub ctrl2:     RW<u32>,   // 0x10
    pub clock:     RW<u32>,   // 0x14
    _r0:           u32,       // 0x18
    pub user:      RW<u32>,   // 0x1C
    pub user1:     RW<u32>,   // 0x20
    pub user2:     RW<u32>,   // 0x24
    pub mosi_dlen: RW<u32>,   // 0x28
    pub miso_dlen: RW<u32>,   // 0x2C
    _r1:           [u32; 20], // 0x30 - 0x7C (20 * 4 = 80 bytes)
    pub w:         [RW<u32>; 16], // 0x80 - 0xBC (W0 to W15)
}

/// ESP32-S3 UART registers
#[repr(C)]
pub struct UartRegs {
    pub fifo:     RW<u32>,   // 0x00
    pub int_raw:  RO<u32>,   // 0x04
    pub int_st:   RO<u32>,   // 0x08
    pub int_ena:  RW<u32>,   // 0x0C
    pub int_clr:  WO<u32>,   // 0x10
    pub clkdiv:   RW<u32>,   // 0x14
    pub autobaud: RW<u32>,   // 0x18
    pub status:   RO<u32>,   // 0x1C
    pub conf0:    RW<u32>,   // 0x20
    pub conf1:    RW<u32>,   // 0x24
}

/// Get global reference to GpioRegs
pub fn gpio() -> &'static GpioRegs {
    unsafe { &*(GPIO_BASE as *const GpioRegs) }
}
