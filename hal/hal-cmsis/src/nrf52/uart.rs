//! nRF52840 UARTE driver

use hal::uart::{UartPort, UartConfig, Parity, FlowControl};
use hal::error::HalResult;
use super::regs::UarteRegs;

/// nRF52 UARTE implementation
pub struct Nrf52Uart {
    regs: *const UarteRegs,
}

unsafe impl Send for Nrf52Uart {}
unsafe impl Sync for Nrf52Uart {}

impl Nrf52Uart {
    /// Create a new Nrf52Uart handle
    ///
    /// # Safety
    /// Unique ownership of this UARTE peripheral must be guaranteed by the caller.
    pub unsafe fn new(regs: *const UarteRegs) -> Self {
        Self { regs }
    }

    fn regs(&self) -> &UarteRegs {
        unsafe { &*self.regs }
    }
}

impl UartPort for Nrf52Uart {
    fn configure(&mut self, config: &UartConfig) -> HalResult<()> {
        // Set baud rate constants
        let baud = match config.baud_rate {
            115200 => 0x01D7E000,
            57600  => 0x00EB7000,
            38400  => 0x009D5000,
            19200  => 0x004EA000,
            9600   => 0x00275000,
            _      => 0x01D7E000, // Default to 115200
        };
        self.regs().baudrate.write(baud);

        let mut cfg = 0;
        match config.parity {
            Parity::None => {}
            _ => cfg |= 0b111 << 1, // Enable parity (both TX and RX config)
        }
        match config.flow_control {
            FlowControl::None => {}
            FlowControl::RtsCts => cfg |= 1, // Enable hardware flow control
        }
        self.regs().config.write(cfg);

        // Enable UARTE (value = 8)
        self.regs().enable.write(8);
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        if data.is_empty() {
            return Ok(0);
        }

        self.regs().txd.ptr.write(data.as_ptr() as u32);
        self.regs().txd.maxcnt.write(data.len() as u32);

        self.regs().events_endtx.write(0);
        self.regs().tasks_starttx.write(1);

        while self.regs().events_endtx.read() == 0 {}

        self.regs().tasks_stoptx.write(1);
        Ok(data.len())
    }

    fn read(&mut self, buffer: &mut [u8], _timeout_ms: u32) -> HalResult<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }

        self.regs().rxd.ptr.write(buffer.as_mut_ptr() as u32);
        self.regs().rxd.maxcnt.write(buffer.len() as u32);

        self.regs().events_endrx.write(0);
        self.regs().tasks_startrx.write(1);

        let mut timeout = 100_000;
        while self.regs().events_endrx.read() == 0 {
            timeout -= 1;
            if timeout == 0 {
                self.regs().tasks_stoprx.write(1);
                return Ok(self.regs().rxd.amount.read() as usize);
            }
        }

        Ok(buffer.len())
    }

    fn available(&self) -> usize {
        if self.regs().events_rxdrdy.read() != 0 {
            1
        } else {
            0
        }
    }

    fn flush(&mut self) -> HalResult<()> {
        Ok(())
    }
}
