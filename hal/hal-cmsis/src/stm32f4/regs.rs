//! STM32F4 Register maps and base addresses

use hal::mmio::{RO, RW, WO};

// STM32F4 Register Base Addresses
pub const GPIOA_BASE: usize = 0x4002_0000;
pub const GPIOB_BASE: usize = 0x4002_0400;
pub const GPIOC_BASE: usize = 0x4002_0800;
pub const GPIOD_BASE: usize = 0x4002_0C00;
pub const GPIOE_BASE: usize = 0x4002_1000;
pub const GPIOH_BASE: usize = 0x4002_1C00;

pub const SPI1_BASE: usize = 0x4001_3000;
pub const SPI2_BASE: usize = 0x4000_3800;

pub const USART1_BASE: usize = 0x4001_1000;
pub const USART2_BASE: usize = 0x4000_4400;

/// STM32F4 GPIO registers
#[repr(C)]
pub struct GpioRegs {
    pub moder:   RW<u32>,  // 0x00 Mode
    pub otyper:  RW<u32>,  // 0x04 Output type
    pub ospeedr: RW<u32>,  // 0x08 Output speed
    pub pupdr:   RW<u32>,  // 0x0C Pull-up/pull-down
    pub idr:     RO<u32>,  // 0x10 Input data
    pub odr:     RW<u32>,  // 0x14 Output data
    pub bsrr:    WO<u32>,  // 0x18 Bit set/reset
    pub lckr:    RW<u32>,  // 0x1C Configuration lock
    pub afrl:    RW<u32>,  // 0x20 Alternate function low
    pub afrh:    RW<u32>,  // 0x24 Alternate function high
}

/// STM32F4 SPI registers
#[repr(C)]
pub struct SpiRegs {
    pub cr1:     RW<u32>,  // 0x00 Control register 1
    pub cr2:     RW<u32>,  // 0x04 Control register 2
    pub sr:      RW<u32>,  // 0x08 Status register
    pub dr:      RW<u32>,  // 0x0C Data register
    pub crcpr:   RW<u32>,  // 0x10 CRC polynomial
    pub rxcrchr: RW<u32>,  // 0x14 RX CRC
    pub txcrchr: RW<u32>,  // 0x18 TX CRC
    pub i2scfgr: RW<u32>,  // 0x1C I2S configuration
    pub i2spr:   RW<u32>,  // 0x20 I2S prescaler
}

/// STM32F4 USART registers
#[repr(C)]
pub struct UsartRegs {
    pub sr:   RW<u32>,  // 0x00 Status register
    pub dr:   RW<u32>,  // 0x04 Data register
    pub brr:  RW<u32>,  // 0x08 Baud rate register
    pub cr1:  RW<u32>,  // 0x0C Control register 1
    pub cr2:  RW<u32>,  // 0x10 Control register 2
    pub cr3:  RW<u32>,  // 0x14 Control register 3
    pub gtpr: RW<u32>,  // 0x18 Guard time and prescaler
}
