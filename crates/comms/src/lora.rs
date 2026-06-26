//! LoRa / LoRaWAN Class A implementation of the [`Rf`] trait.

use hal::rf::{RfPhy, RfTxConfig, RadioParams, PhyEvent, RadioMode, RfRxConfig};
use hal::lora::LoRaTxConfig;
use qf::TraceHook;
use crate::stack::Layer;

#[cfg(feature = "qs")]
use qs::UserRecordBuilder;

use crate::buf::Frame;
use crate::error::CommsError;
#[cfg(feature = "qs")]
use crate::records::LORA_TX_PKT;
use crate::rf::Rf;
use crate::session::LoRaSession;
use crate::mac::lorawan::LoRaWanMac;
use crate::net::NoopNetwork;
use crate::transport::UnreliableTransport;
use crate::stack::RfStack;

/// LoRaWAN Class A RF implementation.
pub struct LoRaRf<P: RfPhy> {
    stack:      RfStack<UnreliableTransport, NoopNetwork, LoRaWanMac, P>,
    session:    LoRaSession,
    tx_config:  LoRaTxConfig,
    name:       &'static str,
    trace_hook: Option<TraceHook>,
}

impl<P: RfPhy> LoRaRf<P> {
    /// Creates a LoRa transport over the given RF driver, session, and TX config.
    pub fn new(phy: P, session: LoRaSession, tx_config: LoRaTxConfig) -> Self {
        let name = phy.chip_name();
        let mac = LoRaWanMac::new(session.clone(), 1); // default fport 1
        let stack = RfStack::new(UnreliableTransport::new(), NoopNetwork, mac, phy);
        Self { stack, session, tx_config, name, trace_hook: None }
    }

    /// Installs (or clears) the QS trace hook used to emit RF trace records.
    pub fn set_trace_hook(&mut self, hook: Option<TraceHook>) {
        self.trace_hook = hook;
    }

    /// Returns the current LoRaWAN session state.
    pub fn session(&self) -> &LoRaSession { &self.session }
    /// Returns the transmit configuration (frequency, spreading factor, …).
    pub fn tx_config(&self) -> &LoRaTxConfig { &self.tx_config }
    /// Returns the underlying radio chip name reported by the driver.
    pub fn chip_name(&self) -> &'static str { self.name }

    /// Initializes the underlying radio hardware.
    pub fn init(&mut self) -> Result<(), CommsError> {
        self.stack.phy.init().map_err(CommsError::from)
    }

    /// Build a LoRaWAN uplink frame, emit a QS trace record, then transmit.
    pub fn send_with_fport(&mut self, payload: &[u8], fport: u8)
        -> Result<(), CommsError>
    {
        // Reinitialize the MAC layer with the current session state and target FPort
        self.stack.mac = LoRaWanMac::new(self.session.clone(), fport);

        let tx_cfg = RfTxConfig {
            frequency_hz: self.tx_config.channel.frequency_hz,
            tx_power_dbm: self.tx_config.tx_power_dbm,
            params: RadioParams::LoRa(self.tx_config.modulation.clone()),
        };

        // Manually run stack down sequence in-place to get built frame for tracing
        let mut frame = Frame::new();
        frame.write_payload(payload)?;
        self.stack.transport.down(&mut frame)?;
        self.stack.network.down(&mut frame)?;
        self.stack.mac.down(&mut frame)?;

        let frame_bytes = frame.phy_bytes();

        #[cfg(feature = "qs")]
        if let Some(ref hook) = self.trace_hook {
            let cfg = &self.tx_config;
            let mut b = UserRecordBuilder::with_capacity(8 + frame_bytes.len());
            b.push_u32(4, cfg.channel.frequency_hz);
            b.push_u8(1, cfg.modulation.sf as u8);
            b.push_u8(1, cfg.modulation.bw as u8);
            b.push_u8(1, cfg.modulation.cr as u8);
            b.push_u8(1, cfg.tx_power_dbm as u8);
            b.push_mem(frame_bytes);
            let _ = hook(LORA_TX_PKT, &b.into_vec(), true);
        }

        self.stack.phy.configure_tx(&tx_cfg).map_err(CommsError::from)?;
        self.stack.phy.transmit(frame_bytes).map_err(CommsError::from)?;

        // Update frame counter in outer session state for consistency
        self.session.fcnt_up = self.stack.mac.fcnt_up();
        Ok(())
    }
}

impl<P: RfPhy> Rf for LoRaRf<P> {
    fn chip_name(&self) -> &'static str { self.name }

    fn send(&mut self, payload: &[u8]) -> Result<(), CommsError> {
        self.send_with_fport(payload, 1)
    }

    fn receive(&mut self, buf: &mut [u8]) -> Result<usize, CommsError> {
        self.stack.phy.configure_rx(&RfRxConfig {
            frequency_hz: self.tx_config.channel.frequency_hz,
            timeout_ms: Some(1000),
            params: RadioParams::LoRa(self.tx_config.modulation.clone()),
        }).map_err(CommsError::from)?;
        self.stack.phy.set_mode(RadioMode::Rx { timeout_ms: Some(1000) }).map_err(CommsError::from)?;

        if let Some(PhyEvent::RxDone(meta)) = self.stack.phy.poll_irq().map_err(CommsError::from)? {
            let mut raw_frame = Frame::new();
            let len = (meta.pkt_len as usize).min(buf.len());
            let mut tmp_buf = [0u8; 256];
            self.stack.phy.read_rx(&mut tmp_buf[..len], &meta).map_err(CommsError::from)?;
            raw_frame.set_received_len(len);
            raw_frame.raw_buf_for_dma()[..len].copy_from_slice(&tmp_buf[..len]);
            if let Some(out_frame) = self.stack.receive_raw(&mut raw_frame)? {
                let out_len = out_frame.len().min(buf.len());
                buf[..out_len].copy_from_slice(&out_frame.payload()[..out_len]);
                return Ok(out_len);
            }
        }
        Err(CommsError::NothingReceived)
    }
}
