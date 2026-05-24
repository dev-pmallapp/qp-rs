//! Null RF driver for POSIX host testing.
//!
//! `NullRf` implements both [`hal::lora::RfDriver`] (chip level) and [`Rf`]
//! (protocol level) without real hardware.  It prints each transmission to
//! stdout so the full app→comms→HAL pipeline can be exercised on the host.

use hal::lora::{LoRaTxConfig, RfDriver};
use hal::HalError;

use crate::error::CommsError;
use crate::rf::Rf;

/// No-op RF driver that logs transmitted frames.
pub struct NullRf;

impl RfDriver for NullRf {
    fn chip_name(&self) -> &'static str { "NullRf" }

    fn init(&mut self) -> Result<(), HalError> { Ok(()) }

    fn transmit(&mut self, cfg: &LoRaTxConfig, payload: &[u8]) -> Result<(), HalError> {
        print!("NullRf TX {:.3} MHz SF{} {} B: ",
            cfg.channel.frequency_hz as f32 / 1_000_000.0,
            cfg.modulation.sf as u8,
            payload.len(),
        );
        for b in payload { print!("{b:02x} "); }
        println!();
        Ok(())
    }
}

impl Rf for NullRf {
    fn chip_name(&self) -> &'static str { "NullRf" }

    fn send(&mut self, payload: &[u8]) -> Result<(), CommsError> {
        self.transmit(&hal::lora::LoRaTxConfig::eu868_default(), payload)
            .map_err(CommsError::from)
    }

    fn receive(&mut self, _buf: &mut [u8]) -> Result<usize, CommsError> {
        Err(CommsError::NothingReceived)
    }
}
