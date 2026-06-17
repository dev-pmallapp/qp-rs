//! nRF52840 register structures and base addresses

use hal::mmio::{RO, RW, WO};

pub const GPIO_P0_BASE: usize = 0x5000_0000;
pub const SPIM0_BASE:   usize = 0x4000_3000;
pub const UARTE0_BASE:  usize = 0x4000_2000;

/// nRF52 GPIO port registers
#[repr(C)]
pub struct GpioRegs {
    _r0:        [u32; 321],
    pub out:    RW<u32>,     // 0x504 Output pin state
    pub outset: WO<u32>,     // 0x508 Set pin high
    pub outclr: WO<u32>,     // 0x50C Set pin low
    pub in_:    RO<u32>,     // 0x510 Read pin state
    pub dir:    RW<u32>,     // 0x514 Direction
    pub dirset: WO<u32>,     // 0x518 Set direction output
    pub dirclr: WO<u32>,     // 0x51C Set direction input
    _r1:        [u32; 120],
    pub pin_cnf: [RW<u32>; 32], // 0x700 Pin configuration
}

/// nRF52 EasyDMA Buffer registers
#[repr(C)]
pub struct DmaBufRegs {
    pub ptr:    RW<u32>,
    pub maxcnt: RW<u32>,
    pub amount: RO<u32>,
}

/// nRF52 SPIM Pin Select registers
#[repr(C)]
pub struct SpiPselRegs {
    pub sck:  RW<u32>, // 0x508
    pub mosi: RW<u32>, // 0x50C
    pub miso: RW<u32>, // 0x510
}

/// nRF52 SPIM (SPI Master with EasyDMA) registers
#[repr(C)]
pub struct SpiRegs {
    pub tasks_start:  WO<u32>, // 0x000
    pub tasks_stop:   WO<u32>, // 0x004
    _r0:             [u32; 64],
    pub events_ready: RW<u32>, // 0x108
    _r1:             [u32; 126],
    pub intenset:     RW<u32>, // 0x304
    pub intenclr:     RW<u32>, // 0x308
    _r2:             [u32; 125],
    pub enable:       RW<u32>, // 0x500
    _r3:             [u32; 1],
    pub psel:         SpiPselRegs, // 0x508
    _r4:             [u32; 4],
    pub frequency:    RW<u32>, // 0x524
    _r5:             [u32; 7],
    pub rxd:          DmaBufRegs, // 0x544
    pub txd:          DmaBufRegs, // 0x550
    _r6:             [u32; 26],
    pub config:       RW<u32>, // 0x5C4
}

/// nRF52 UARTE Pin Select registers
#[repr(C)]
pub struct UartPselRegs {
    pub rts: RW<u32>, // 0x508
    pub txd: RW<u32>, // 0x50C
    pub cts: RW<u32>, // 0x510
    pub rxd: RW<u32>, // 0x514
}

/// nRF52 UARTE (UART with EasyDMA) registers
#[repr(C)]
pub struct UarteRegs {
    pub tasks_startrx: WO<u32>, // 0x000
    pub tasks_stoprx:  WO<u32>, // 0x004
    pub tasks_starttx: WO<u32>, // 0x008
    pub tasks_stoptx:  WO<u32>, // 0x00C
    _r0:              [u32; 62],
    pub events_rxdrdy:  RW<u32>, // 0x108
    _r1:              [u32; 1],
    pub events_endrx:   RW<u32>, // 0x110
    _r2:              [u32; 2],
    pub events_txdrdy:  RW<u32>, // 0x11C
    pub events_endtx:   RW<u32>, // 0x120
    pub events_error:   RW<u32>, // 0x124
    _r3:              [u32; 246],
    pub enable:        RW<u32>, // 0x500
    _r4:              [u32; 1],
    pub psel:          UartPselRegs, // 0x508
    _r5:              [u32; 3],
    pub baudrate:      RW<u32>, // 0x524
    _r6:              [u32; 3],
    pub rxd:           DmaBufRegs, // 0x534
    _r7:              [u32; 1],
    pub txd:           DmaBufRegs, // 0x544
    _r8:              [u32; 7],
    pub config:        RW<u32>, // 0x56C
}
