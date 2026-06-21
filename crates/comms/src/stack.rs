//! RF stack composition and Active Object runner.

use alloc::sync::Arc;
use crate::buf::Frame;
use crate::error::CommsError;
use hal::rf::{RfPhy, RfTxConfig, RfRxConfig, RadioMode};
use qf::active::{ActiveBehavior, ActiveContext, ActiveObjectRef, ActiveObjectId};
use qf::event::DynEvent;
use qf::time::{TimeEvent, TimeEventConfig};
use crate::events::*;

/// Protocol layer. Layers are chained inside `RfStack`; data flows in-place
/// through a shared `Frame` buffer.
pub trait Layer: Send {
    /// Egress: encapsulate this layer's header/trailer around the payload.
    fn down(&mut self, frame: &mut Frame) -> Result<(), CommsError>;

    /// Ingress: validate and strip this layer's header/trailer.
    ///
    /// Returns `Ok(false)` to drop the frame (e.g. bad MIC, wrong DevAddr).
    fn up(&mut self, frame: &mut Frame) -> Result<bool, CommsError>;
}

/// Zero-cost composition of Transport / Network / MAC / PHY layers.
pub struct RfStack<T, N, M, P>
where
    T: Layer,
    N: Layer,
    M: Layer,
    P: RfPhy,
{
    pub transport: T,
    pub network:   N,
    pub mac:       M,
    pub phy:       P,
}

impl<T: Layer, N: Layer, M: Layer, P: RfPhy> RfStack<T, N, M, P> {
    pub fn new(transport: T, network: N, mac: M, phy: P) -> Self {
        Self { transport, network, mac, phy }
    }

    /// TX path: payload → transport header → net header → MAC frame → PHY air.
    pub fn transmit(
        &mut self,
        payload: &[u8],
        tx_cfg:  &RfTxConfig,
    ) -> Result<(), CommsError> {
        let mut frame = Frame::new();
        frame.write_payload(payload)?;
        self.transport.down(&mut frame)?;
        self.network.down(&mut frame)?;
        self.mac.down(&mut frame)?;
        self.phy.configure_tx(tx_cfg).map_err(CommsError::from)?;
        self.phy.transmit(frame.phy_bytes()).map_err(CommsError::from)
    }

    /// RX path: raw bytes → MAC parse → net dispatch → transport reorder → payload.
    pub fn receive_raw(
        &mut self,
        raw_frame: &mut Frame,
    ) -> Result<Option<Frame>, CommsError> {
        if !self.mac.up(raw_frame)?         { return Ok(None); }
        if !self.network.up(raw_frame)?     { return Ok(None); }
        if !self.transport.up(raw_frame)?   { return Ok(None); }
        let mut out = Frame::new();
        out.write_payload(raw_frame.payload())?;
        Ok(Some(out))
    }
}

/// Active object that wraps and drives the composed RF protocol stack.
pub struct RfStackAO<T, N, M, P>
where
    T: Layer,
    N: Layer,
    M: Layer,
    P: RfPhy,
{
    stack:             RfStack<T, N, M, P>,
    tx_cfg:            RfTxConfig,
    rx_cfg:            RfRxConfig,
    retransmit_timer:  Option<Arc<TimeEvent>>,
    rx_frame:          Frame,
    state:             AoState,
    app_ao:            ActiveObjectRef,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AoState { Idle, Transmitting, WaitingAck, Listening, ProcessingRx }

pub const RETRANSMIT_TIMEOUT_TICKS: u64 = 10;
pub const TX_WATCHDOG_TICKS: u64 = 25;

impl<T: Layer, N: Layer, M: Layer, P: RfPhy> RfStackAO<T, N, M, P> {
    pub fn new(
        stack: RfStack<T, N, M, P>,
        tx_cfg: RfTxConfig,
        rx_cfg: RfRxConfig,
        app_ao: ActiveObjectRef,
    ) -> Self {
        Self {
            stack,
            tx_cfg,
            rx_cfg,
            retransmit_timer: None,
            rx_frame: Frame::new(),
            state: AoState::Idle,
            app_ao,
        }
    }

    fn do_retransmit(&mut self, frame: Frame) {
        if let Err(e) = self.stack.phy.transmit(frame.phy_bytes()) {
            ceprintln!("RfStackAO: Retransmit failed: {e}");
        }
    }

    fn handle_tx_req(&mut self, _ctx: &mut ActiveContext, event: &DynEvent) {
        if self.state != AoState::Idle { return; }
        let Some(req) = event.payload.as_ref().downcast_ref::<RfTxReqPayload>() else { return };
        match self.stack.transmit(&req.data, &self.tx_cfg) {
            Ok(()) => {
                self.state = AoState::Transmitting;
                if let Some(ref timer) = self.retransmit_timer {
                    timer.arm(TX_WATCHDOG_TICKS, None);
                }
            }
            Err(e) => {
                ceprintln!("RfStackAO: TX failed: {e}");
            }
        }
    }

    fn handle_tx_done(&mut self) {
        if let Some(ref timer) = self.retransmit_timer {
            timer.disarm();
        }
        if self.state == AoState::WaitingAck {
            // Keep waiting; timer still running for ACK timeout
        } else {
            self.state = AoState::Idle;
            let _ = self.stack.phy.set_mode(RadioMode::Rx { timeout_ms: Some(1000) });
            self.state = AoState::Listening;
        }
    }

    fn handle_rx_done(&mut self, ctx: &mut ActiveContext, event: &DynEvent) {
        let Some(payload) = event.payload.as_ref().downcast_ref::<PhyIrqPayload>() else { return };
        let meta = payload.meta;

        self.rx_frame = Frame::new();
        if self.stack.phy.read_rx(self.rx_frame.raw_buf_for_dma(), &meta).is_err() { return; }
        self.rx_frame.set_received_len(meta.pkt_len as usize);

        match self.stack.receive_raw(&mut self.rx_frame) {
            Ok(Some(app_frame)) => {
                let rx_sig = RF_RX_FRAME_SIG;
                let mut data = heapless::Vec::new();
                if data.extend_from_slice(app_frame.payload()).is_ok() {
                    let pld = RfRxFramePayload {
                        data,
                        port: 1,
                        rssi: meta.rssi_dbm,
                        snr: meta.snr_db_x10,
                    };
                    self.app_ao.post(DynEvent::with_arc(rx_sig, Arc::new(pld)));
                }
                let _ = ctx.emit_trace(crate::records::RF_NET_ROUTE, &[meta.rssi_dbm as u8]);
            }
            Ok(None) => {}
            Err(e) => {
                ceprintln!("RfStackAO: RX stack error: {e}");
            }
        }
        self.state = AoState::Idle;
    }

    fn handle_phy_irq(&mut self, _ctx: &mut ActiveContext, _event: &DynEvent) {
        // Fallback or generic DIO handling
    }

    fn handle_transport_timeout(&mut self) {
        // Delegate transport timeout logic
    }
}

impl<T: Layer + 'static, N: Layer + 'static, M: Layer + 'static, P: RfPhy + 'static> ActiveBehavior for RfStackAO<T, N, M, P> {
    fn on_start(&mut self, ctx: &mut ActiveContext) {
        self.stack.phy.init().expect("RF PHY init failed");
        self.stack.phy.set_mode(RadioMode::Standby).expect("RF standby");
        self.retransmit_timer = Some(TimeEvent::new(
            ActiveObjectId::new(ctx.id().0),
            TimeEventConfig::new(RF_TRANSPORT_TIMEOUT_SIG),
        ));
        self.state = AoState::Idle;
    }

    fn on_event(&mut self, ctx: &mut ActiveContext, event: DynEvent) {
        match event.signal() {
            RF_TX_REQ_SIG          => self.handle_tx_req(ctx, &event),
            RF_PHY_IRQ_SIG         => self.handle_phy_irq(ctx, &event),
            RF_PHY_RX_DONE_SIG     => self.handle_rx_done(ctx, &event),
            RF_PHY_TX_DONE_SIG     => self.handle_tx_done(),
            RF_PHY_RX_TIMEOUT_SIG  => { self.state = AoState::Idle; }
            RF_TRANSPORT_TIMEOUT_SIG => self.handle_transport_timeout(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phy::loopback::LoopbackPhy;
    use crate::mac::lorawan::{LoRaWanMac, encrypt_frm_payload, compute_mic};
    use crate::net::NoopNetwork;
    use crate::transport::UnreliableTransport;
    use crate::session::LoRaSession;

    #[test]
    fn test_stack_tx_uplink() {
        let phy = LoopbackPhy::new();
        let session = LoRaSession::test_abp();
        let mac = LoRaWanMac::new(session.clone(), 1);
        let transport = UnreliableTransport::new();
        let network = NoopNetwork;

        let mut stack = RfStack::new(transport, network, mac, phy);

        // Transmit payload
        let payload = b"hello modular stack tx";
        let tx_cfg = RfTxConfig {
            frequency_hz: 868_100_000,
            tx_power_dbm: 14,
            params: hal::rf::RadioParams::LoRa(hal::lora::LoRaModulation::default()),
        };
        stack.transmit(payload, &tx_cfg).expect("transmit failed");

        // LoopbackPhy stores the transmitted payload in its internal queue
        let rx_event = stack.phy.poll_irq().expect("poll_irq failed");
        assert!(rx_event.is_some());
        if let Some(hal::rf::PhyEvent::RxDone(meta)) = rx_event {
            let mut buf = vec![0u8; meta.pkt_len as usize];
            stack.phy.read_rx(&mut buf, &meta).unwrap();

            // Verify size: 9 (MAC hdr) + 5 (Transport hdr) + 22 (payload) + 4 (MIC) = 40
            assert_eq!(buf.len(), 9 + 5 + payload.len() + 4);
            // Verify MHDR is uplink (0x40)
            assert_eq!(buf[0], 0x40);
            // Verify DevAddr
            assert_eq!(&buf[1..5], &session.dev_addr);
        } else {
            panic!("Expected RxDone event");
        }
    }

    #[test]
    fn test_stack_rx_downlink() {
        let phy = LoopbackPhy::new();
        let session = LoRaSession::test_abp();
        let mac = LoRaWanMac::new(session.clone(), 1);
        let transport = UnreliableTransport::new();
        let network = NoopNetwork;

        let mut stack = RfStack::new(transport, network, mac, phy);

        // Construct a valid downlink frame manually
        let dev_addr = session.dev_addr;
        let fcnt = 0u32;
        let dir = 1; // downlink

        // 1. App payload + transport header
        let app_payload = b"hello modular stack rx";
        let mut transport_frame = Frame::new();
        transport_frame.write_payload(app_payload).unwrap();
        // UnreliableTransport::down prepends 5 bytes header
        stack.transport.down(&mut transport_frame).unwrap();

        // 2. Encrypt transport payload in-place using dir = 1
        let mut frm_payload = transport_frame.payload().to_vec();
        encrypt_frm_payload(&mut frm_payload, &session.app_skey, &dev_addr, fcnt, dir).unwrap();

        // 3. Assemble MAC message: MHDR(1) | DevAddr(4LE) | FCtrl(1) | FCnt(2LE) | FPort(1) | FRMPayload
        let mut msg = Vec::new();
        msg.push(0x60); // MHDR: UnconfirmedDataDown
        msg.extend_from_slice(&dev_addr);
        msg.push(0x00); // FCtrl
        msg.push(fcnt as u8); // FCnt LSB
        msg.push((fcnt >> 8) as u8); // FCnt MSB
        msg.push(1); // FPort
        msg.extend_from_slice(&frm_payload);

        // 4. Compute MIC
        let mic = compute_mic(&msg, &session.nwk_skey, &dev_addr, fcnt, dir).unwrap();
        msg.extend_from_slice(&mic);

        // 5. Inject into raw_frame and process
        let mut raw_frame = Frame::new();
        raw_frame.set_received_len(msg.len());
        raw_frame.raw_buf_for_dma()[..msg.len()].copy_from_slice(&msg);

        let out_frame = stack.receive_raw(&mut raw_frame).expect("receive_raw failed");
        assert!(out_frame.is_some());
        if let Some(out) = out_frame {
            assert_eq!(out.payload(), app_payload);
        }
    }
}
