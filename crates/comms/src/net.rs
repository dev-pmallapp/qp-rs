//! Network routing and dispatch layer.

use crate::error::CommsError;
use crate::stack::Layer;
use crate::buf::Frame;
use qf::event::Signal;

/// Maximum port → signal bindings in the dispatch table.
const MAX_PORT_BINDINGS: usize = 8;

/// Maps a LoRaWAN FPort (or generic "service identifier") to a QF signal.
pub struct PortBinding {
    pub port:   u8,
    pub signal: Signal,
}

pub struct Network {
    bindings: [Option<PortBinding>; MAX_PORT_BINDINGS],
}

impl Network {
    pub const fn new() -> Self {
        Self { bindings: [const { None }; MAX_PORT_BINDINGS] }
    }

    /// Register a port → signal mapping. Returns `Err` if the table is full.
    pub fn bind(&mut self, port: u8, signal: Signal) -> Result<(), CommsError> {
        for slot in &mut self.bindings {
            if slot.is_none() {
                *slot = Some(PortBinding { port, signal });
                return Ok(());
            }
        }
        Err(CommsError::TableFull)
    }

    /// Resolve port to signal for application dispatch.
    pub fn resolve(&self, port: u8) -> Option<Signal> {
        self.bindings.iter()
            .find_map(|b| b.as_ref().filter(|b| b.port == port).map(|b| b.signal))
    }
}

impl Layer for Network {
    fn down(&mut self, _frame: &mut Frame) -> Result<(), CommsError> {
        // LoRaWAN: addressing is in MAC header; nothing to add here.
        Ok(())
    }

    fn up(&mut self, _frame: &mut Frame) -> Result<bool, CommsError> {
        // Port-based dispatch happens in RfStackAO after
        // receive_raw() returns the reassembled payload.
        Ok(true)
    }
}

/// No-op network layer for LoopbackPhy/NullRf tests.
pub struct NoopNetwork;
impl Layer for NoopNetwork {
    fn down(&mut self, _f: &mut Frame) -> Result<(), CommsError> { Ok(()) }
    fn up(&mut self, _f: &mut Frame) -> Result<bool, CommsError> { Ok(true) }
}
