//! Null RF driver for POSIX host testing.
//!
//! `NullRf` implements both [`hal::lora::RfDriver`] (chip level) and [`RfPhy`] / [`Rf`]
//! (protocol level) without real hardware. It prints each transmission to
//! stdout so the full app→comms→HAL pipeline can be exercised on the host.

use hal::lora::{LoRaTxConfig, RfDriver};
use hal::rf::{RfPhy, RadioMode, RfTxConfig, RfRxConfig, RxMetadata, PhyEvent};
use hal::HalError;

use crate::error::CommsError;
use crate::rf::Rf;

/// No-op RF driver that logs transmitted frames.
pub struct NullRf;

impl RfDriver for NullRf {
    fn chip_name(&self) -> &'static str { "NullRf" }

    fn init(&mut self) -> Result<(), HalError> { Ok(()) }

    fn transmit(&mut self, cfg: &LoRaTxConfig, payload: &[u8]) -> Result<(), HalError> {
        cprint!("NullRf TX {:.3} MHz SF{} {} B: ",
            cfg.channel.frequency_hz as f32 / 1_000_000.0,
            cfg.modulation.sf as u8,
            payload.len(),
        );
        for b in payload { cprint!("{b:02x} "); }
        cprintln!();
        Ok(())
    }
}

impl RfPhy for NullRf {
    fn init(&mut self) -> Result<(), HalError> { Ok(()) }
    fn set_mode(&mut self, _mode: RadioMode) -> Result<(), HalError> { Ok(()) }
    fn configure_tx(&mut self, _cfg: &RfTxConfig) -> Result<(), HalError> { Ok(()) }
    fn configure_rx(&mut self, _cfg: &RfRxConfig) -> Result<(), HalError> { Ok(()) }
    fn transmit(&mut self, payload: &[u8]) -> Result<(), HalError> {
        cprint!("NullRf TX: ");
        for b in payload { cprint!("{b:02x} "); }
        cprintln!();
        Ok(())
    }
    fn read_rx(&mut self, _buf: &mut [u8], _meta: &RxMetadata) -> Result<(), HalError> { Ok(()) }
    fn poll_irq(&mut self) -> Result<Option<PhyEvent>, HalError> { Ok(None) }
    fn clear_irq(&mut self) -> Result<(), HalError> { Ok(()) }
    fn rssi(&mut self) -> Result<i16, HalError> { Ok(-50) }
    fn chip_name(&self) -> &'static str { "NullRf" }
}

impl Rf for NullRf {
    fn chip_name(&self) -> &'static str { "NullRf" }

    fn send(&mut self, payload: &[u8]) -> Result<(), CommsError> {
        RfDriver::transmit(self, &hal::lora::LoRaTxConfig::eu868_default(), payload)
            .map_err(CommsError::from)
    }

    fn receive(&mut self, _buf: &mut [u8]) -> Result<usize, CommsError> {
        Err(CommsError::NothingReceived)
    }
}

