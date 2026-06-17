//! GD32VF103 peripheral register maps and base addresses

use hal::mmio::{RO, RW, WO};

pub const GPIOA_BASE:  usize = 0x4001_0800;
pub const SPI0_BASE:   usize = 0x4001_3000;
pub const USART0_BASE: usize = 0x4001_3800;

/// GD32VF GPIO port registers
#[repr(C)]
pub struct GpioRegs {
    pub ctl0:  RW<u32>, // 0x00 Port control register 0
    pub ctl1:  RW<u32>, // 0x04 Port control register 1
    pub istat: RO<u32>, // 0x08 Port input status register
    pub ostat: RW<u32>, // 0x0C Port output status register
    pub bop:   WO<u32>, // 0x10 Port bit operation register
    pub bc:    WO<u32>, // 0x14 Port bit clear register
    pub lock:  RW<u32>, // 0x18 Port configuration lock register
}

/// GD32VF SPI registers
#[repr(C)]
pub struct SpiRegs {
    pub ctl0:    RW<u32>, // 0x00 SPI control register 0
    pub ctl1:    RW<u32>, // 0x04 SPI control register 1
    pub stat:    RW<u32>, // 0x08 SPI status register
    pub data:    RW<u32>, // 0x0C SPI data register
    pub crcpoly: RW<u32>, // 0x10 SPI CRC polynomial register
    pub rcrc:    RO<u32>, // 0x14 SPI receive CRC register
    pub tcrc:    RO<u32>, // 0x18 SPI transmit CRC register
}

/// GD32VF USART registers
#[repr(C)]
pub struct UsartRegs {
    pub stat: RO<u32>, // 0x00 USART status register
    pub data: RW<u32>, // 0x04 USART data register
    pub baud: RW<u32>, // 0x08 USART baud rate register
    pub ctl0: RW<u32>, // 0x0C USART control register 0
    pub ctl1: RW<u32>, // 0x10 USART control register 1
    pub ctl2: RW<u32>, // 0x14 USART control register 2
    pub gp:   RW<u32>, // 0x18 USART guard-time and prescaler register
}
