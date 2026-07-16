//! RF stack composition and Active Object runner.

use alloc::sync::Arc;
use crate::buf::{Frame, MAX_FRAME};
use crate::error::CommsError;
use hal::rf::{RfPhy, RfTxConfig, RfRxConfig, RadioMode, PhyEvent};
use qf::active::{ActiveBehavior, ActiveContext, ActiveObjectRef, ActiveObjectId};
use qf::event::DynEvent;
use qf::time::{TimeEvent, TimeEventConfig};
use crate::events::*;
use crate::transport::TransportAction;

// ─────────────────────────────────────────────────────────────────────────────
// Layer trait
// ─────────────────────────────────────────────────────────────────────────────

/// Protocol layer.  Layers are chained inside `RfStack`; data flows in-place
/// through a shared `Frame` buffer.
///
/// TX (egress):  call `down(&mut frame)` — layer prepends its header and
///               optionally appends a trailer.
/// RX (ingress): call `up(&mut frame)` — layer validates, strips its header,
///               and returns `Ok(false)` to silently drop invalid frames.
pub trait Layer: Send {
    /// Egress: encapsulate this layer's header/trailer around the payload.
    fn down(&mut self, frame: &mut Frame) -> Result<(), CommsError>;

    /// Ingress: validate and strip this layer's header/trailer.
    ///
    /// Returns `Ok(false)` to drop the frame (e.g. bad MIC, wrong DevAddr,
    /// duplicate sequence number).
    fn up(&mut self, frame: &mut Frame) -> Result<bool, CommsError>;

    /// Retransmit timeout callback. Only implemented by reliable transport layers.
    fn on_timeout(&mut self) -> TransportAction {
        TransportAction::Nothing
    }

    /// Set the destination address / kind tag for the *next* `down()` call.
    /// Only meaningful for an addressing layer (e.g. [`crate::net::Network`]);
    /// every other layer keeps the no-op default.
    fn set_tx_meta(&mut self, _dst: u16, _kind: u8) {}

    /// Source address / kind tag extracted by the most recent successful
    /// `up()` call. Only meaningful for an addressing layer; every other
    /// layer keeps the no-op default of `(0, 0)`.
    fn last_rx_meta(&self) -> (u16, u8) {
        (0, 0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RfStack — zero-cost composition
// ─────────────────────────────────────────────────────────────────────────────

/// Zero-cost composition of Transport / Network / MAC / PHY layers.
///
/// Monomorphises at compile time — no vtable, no per-packet allocation.
///
/// Type alias examples:
/// ```rust,ignore
/// type LoRaStack<SPI> =
///     RfStack<ReliableTransport, Network, LoRaWanMac, Sx1262Phy<SPI>>;
///
/// type LoopbackStack =
///     RfStack<UnreliableTransport, NoopNetwork, NoopMac, LoopbackPhy>;
/// ```
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
    ///
    /// Returns the fully-encoded frame so the caller can snapshot it for
    /// potential retransmit (see `RfStackAO::handle_tx_req`).
    pub fn transmit(
        &mut self,
        payload:  &[u8],
        tx_cfg:   &RfTxConfig,
    ) -> Result<(), CommsError> {
        let mut frame = Frame::new();
        frame.write_payload(payload)?;
        self.transport.down(&mut frame)?;
        self.network.down(&mut frame)?;
        self.mac.down(&mut frame)?;
        self.phy.configure_tx(tx_cfg).map_err(CommsError::from)?;
        self.phy.transmit(frame.phy_bytes()).map_err(CommsError::from)
    }

    /// Build a fully-encoded frame (transport → net → MAC) without transmitting.
    ///
    /// Used by `RfStackAO::handle_tx_req` to snapshot the post-MAC frame for
    /// retransmit before handing it to the PHY.
    pub fn build_frame(
        &mut self,
        payload: &[u8],
    ) -> Result<Frame, CommsError> {
        let mut frame = Frame::new();
        frame.write_payload(payload)?;
        self.transport.down(&mut frame)?;
        self.network.down(&mut frame)?;
        self.mac.down(&mut frame)?;
        Ok(frame)
    }

    /// RX path: raw bytes → MAC parse → net dispatch → transport reorder → payload.
    ///
    /// Called by `RfStackAO` after a `RxDone` PHY event with `meta.pkt_len` set.
    pub fn receive_raw(
        &mut self,
        raw_frame: &mut Frame,  // PHY has already written DMA bytes + set_received_len
    ) -> Result<Option<Frame>, CommsError> {
        if !self.mac.up(raw_frame)?         { return Ok(None); }
        if !self.network.up(raw_frame)?     { return Ok(None); }
        if !self.transport.up(raw_frame)?   { return Ok(None); }
        let mut out = Frame::new();
        out.write_payload(raw_frame.payload())?;
        Ok(Some(out))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RfStackAO — active object
// ─────────────────────────────────────────────────────────────────────────────

/// Active object that owns and drives the composed RF protocol stack.
///
/// ## State machine
/// ```text
///                    ┌──────────┐
///                    │  Initial │
///                    └────┬─────┘
///                         │ on_start: phy.init(), set_mode(Standby)
///                         ▼
///               ┌─────────────────┐
///         ┌────►│     Idle        │◄──────────────────────┐
///         │     └───┬─────────┬───┘                       │
///         │         │         │                           │
///         │   RF_TX_REQ  RF_RX_START                      │
///         │         │         │                           │
///         │         ▼         ▼                           │
///         │ ┌──────────┐ ┌─────────────────┐             │
///         │ │Transmit- │ │  Listening      │             │
///         │ │ing       │ │  (set_mode Rx)  │             │
///         │ └──┬───────┘ └──┬──────────────┘             │
///         │    │             │                             │
///         │ TX_DONE      RX_DONE (post_from_isr)          │
///         │    │             │                             │
///         │    ▼             ▼                             │
///         │ ┌──────────┐ ┌─────────────────┐             │
///     app ◄─┤ (notify) │ │  Processing RX  │             │
///   TX_DONE │          │ │  mac→net→xport  │             │
///         │ └──────────┘ └────────┬─────────┘             │
///         │                       │ → post to app          │
///         │                       └──────────────────────►┘
///         │
///         │  For ReliableTransport: TX transitions to WaitingAck
///         │  ACK received → TxComplete → notify app TX_DONE
///         │  TIMEOUT → ShouldRetransmit → do_retransmit
///         │  Exhausted → GiveUp → notify app TX_FAIL
/// ```
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
    /// Post-MAC fully-encoded frame saved for reliable retransmit.
    /// Set in `handle_tx_req` when reliable=true; cleared on ACK or GiveUp.
    retransmit_frame:  Option<[u8; MAX_FRAME]>,
    /// Number of bytes valid in `retransmit_frame` (= frame.phy_bytes().len()).
    retransmit_len:    usize,
    /// A single `RF_TX_REQ_SIG` request queued by `handle_tx_req` while a
    /// previous TX (or its ACK wait) was still in flight. Drained by
    /// `drain_pending_tx` the moment that resolves. Depth 1: a second
    /// request arriving while this slot is already occupied is still
    /// dropped, same as the pre-queue behaviour.
    pending_tx:        Option<RfTxReqPayload>,
    rx_frame:          Frame,
    state:             AoState,
    app_ao:            ActiveObjectRef,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AoState { Idle, Transmitting, WaitingAck, Listening, ProcessingRx }

/// QK tick counts for the TX watchdog (covers air-time + driver overhead).
pub const RETRANSMIT_TIMEOUT_TICKS: u64 = 10;
/// TX watchdog — fires if TxDone IRQ never arrives (driver hang recovery).
pub const TX_WATCHDOG_TICKS: u64 = 25;

impl<T: Layer, N: Layer, M: Layer, P: RfPhy> RfStackAO<T, N, M, P> {
    pub fn new(
        stack:   RfStack<T, N, M, P>,
        tx_cfg:  RfTxConfig,
        rx_cfg:  RfRxConfig,
        app_ao:  ActiveObjectRef,
    ) -> Self {
        Self {
            stack,
            tx_cfg,
            rx_cfg,
            retransmit_timer:  None,
            retransmit_frame:  None,
            retransmit_len:    0,
            pending_tx:        None,
            rx_frame:          Frame::new(),
            state:             AoState::Idle,
            app_ao,
        }
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Re-drive the PHY with the previously saved post-MAC frame bytes.
    ///
    /// The frame was encoded (transport + network + MAC) in `handle_tx_req`
    /// and stored byte-for-byte in `retransmit_frame`.  The MAC layer is NOT
    /// re-run here — we replay identical bytes so the MIC and FCnt match what
    /// the peer already expects.
    fn do_retransmit(&mut self) {
        if let Some(ref buf) = self.retransmit_frame {
            let bytes = &buf[..self.retransmit_len];
            if let Err(e) = self.stack.phy.transmit(bytes) {
                ceprintln!("RfStackAO: Retransmit PHY failed: {e}");
            }
        }
    }

    // ── Signal handlers ──────────────────────────────────────────────────────

    /// Handle an incoming TX request from the application.
    ///
    /// If a previous TX (or its ACK wait) is still in flight, queue this
    /// request (depth 1, see `pending_tx`) instead of dropping it — it is
    /// sent the moment the in-flight TX resolves, via `drain_pending_tx`.
    /// A second request arriving while the slot is already occupied is
    /// still dropped: depth 1 is sufficient for the two-node simultaneous
    /// pairing contention this exists for (Stage 1.5,
    /// `docs/03-design/DES_multi_oht_channel_access.md` in swm-rs);
    /// revisit sizing if that assumption changes.
    ///
    /// From an RX/listen state, however, a TX request pre-empts the
    /// receive window: a half-duplex node (e.g. a receive-first MC, which
    /// spends its idle time in `Listening`) must be able to transmit its
    /// reply without first leaving RX — that path is unaffected by the
    /// queue and goes straight to `start_tx` below.
    fn handle_tx_req(&mut self, ctx: &mut ActiveContext, event: &DynEvent) {
        let Some(req) = event.payload.as_ref().downcast_ref::<RfTxReqPayload>() else { return };

        if self.state == AoState::Transmitting || self.state == AoState::WaitingAck {
            if self.pending_tx.is_none() {
                self.pending_tx = Some(req.clone());
            }
            return;
        }

        self.start_tx(ctx, req);
    }

    /// Build the full frame (transport → network → MAC), snapshot the
    /// post-MAC bytes for potential retransmit, and hand it to the PHY.
    ///
    /// Shared by `handle_tx_req`'s direct dispatch path and
    /// `drain_pending_tx`.
    ///
    /// Emits two diagnostic markers (Stage 1.5,
    /// `docs/03-design/DES_multi_oht_channel_access.md` in swm-rs) around
    /// the physical `phy.transmit()` call: `RF_PHY_TX` (payload tag `1`)
    /// right before, `RF_PHY_TX_DONE` (payload tag `1`) right after it
    /// returns. Since `transmit()` blocks for the full on-air time, the gap
    /// between these two timestamps is the real TX air-time. Compare
    /// against `handle_tx_done`'s own `RF_PHY_TX_DONE` (tag `2`), which
    /// only fires a whole cooperative-loop iteration later, to see how much
    /// lag remains between physical readiness and the AO's own bookkeeping
    /// catching up.
    fn start_tx(&mut self, ctx: &mut ActiveContext, req: &RfTxReqPayload) {
        // Tell the network layer (if any) who this frame is for before
        // encoding — a no-op on layers that don't implement addressing.
        self.stack.network.set_tx_meta(req.dst, req.kind);

        // Build fully-encoded frame: transport → network → MAC.
        let frame = match self.stack.build_frame(&req.data) {
            Ok(f) => f,
            Err(e) => {
                #[cfg(feature = "std")]
                eprintln!("RfStackAO: frame build failed: {e}");
                return;
            }
        };

        // Snapshot post-MAC bytes for retransmit (avoids re-running MAC/crypto).
        let phy_bytes = frame.phy_bytes();
        let len = phy_bytes.len().min(MAX_FRAME);
        let mut buf = [0u8; MAX_FRAME];
        buf[..len].copy_from_slice(&phy_bytes[..len]);
        self.retransmit_frame = Some(buf);
        self.retransmit_len   = len;

        // Configure PHY and transmit.
        if self.stack.phy.configure_tx(&self.tx_cfg).is_err() { return; }
        let _ = ctx.emit_trace(crate::records::RF_PHY_TX, &[1, req.dst as u8]);
        match self.stack.phy.transmit(frame.phy_bytes()) {
            Ok(()) => {
                let _ = ctx.emit_trace(crate::records::RF_PHY_TX_DONE, &[1, req.dst as u8]);
                self.state = if req.reliable {
                    AoState::WaitingAck
                } else {
                    AoState::Transmitting
                };
                // Arm watchdog (catches PHY hang even for unreliable TX).
                if let Some(ref timer) = self.retransmit_timer {
                    timer.arm(TX_WATCHDOG_TICKS, None);
                }
            }
            Err(e) => {
                ceprintln!("RfStackAO: TX failed: {e}");
                self.retransmit_frame = None;
            }
        }
    }

    /// Drain a `pending_tx` request queued by `handle_tx_req` while a
    /// previous TX (or its ACK wait) was in flight. Called at every point
    /// a TX fully resolves: unreliable `TxDone`, an ACK received, or
    /// `GiveUp`/`TxComplete` on transport timeout.
    fn drain_pending_tx(&mut self, ctx: &mut ActiveContext) {
        if self.state == AoState::Transmitting || self.state == AoState::WaitingAck {
            return; // still busy — nothing to drain into yet
        }
        if let Some(req) = self.pending_tx.take() {
            self.start_tx(ctx, &req);
        }
    }

    /// Handle PHY TxDone event.
    ///
    /// - Unreliable TX: disarm watchdog, notify app `RF_TX_DONE_SIG`, enter RX.
    /// - Reliable TX (WaitingAck): stay in WaitingAck — ACK timeout timer
    ///   continues running.
    ///
    /// Emits `RF_PHY_TX_DONE` (payload tag `2`) here — see `start_tx`'s doc
    /// comment for why this is the "AO caught up" timestamp, one
    /// cooperative-loop iteration after the tag-`1` marker there.
    fn handle_tx_done(&mut self, ctx: &mut ActiveContext) {
        if let Some(ref timer) = self.retransmit_timer {
            timer.disarm();
        }
        match self.state {
            AoState::WaitingAck => {
                // Reliable TX — PHY has finished air-time but we still wait for
                // the peer's ACK.  Re-arm retransmit timer for the ACK window.
                if let Some(ref timer) = self.retransmit_timer {
                    timer.arm(RETRANSMIT_TIMEOUT_TICKS, None);
                }
                // Remain in WaitingAck.
            }
            _ => {
                // Unreliable or Transmitting — TX complete.
                let _ = ctx.emit_trace(crate::records::RF_PHY_TX_DONE, &[2, 0]);
                self.app_ao.post(DynEvent::empty_dyn(RF_TX_DONE_SIG));
                self.state = AoState::Idle;
                // Class A behaviour: enter RX1 window immediately after TX.
                let _ = self.stack.phy.set_mode(RadioMode::Rx { timeout_ms: Some(1000) });
                self.state = AoState::Listening;
                self.drain_pending_tx(ctx);
            }
        }
    }

    /// Handle retransmit timer expiry (reliable transport timeout).
    fn handle_transport_timeout(&mut self, ctx: &mut ActiveContext) {
        match self.stack.transport.on_timeout() {
            TransportAction::ShouldRetransmit => {
                self.do_retransmit();
                // Re-arm for next ACK window.
                if let Some(ref timer) = self.retransmit_timer {
                    timer.arm(RETRANSMIT_TIMEOUT_TICKS, None);
                }
            }
            TransportAction::GiveUp => {
                // Retransmit limit exhausted — clear state and notify application.
                self.retransmit_frame = None;
                self.retransmit_len   = 0;
                self.state            = AoState::Idle;
                self.app_ao.post(DynEvent::empty_dyn(RF_TX_FAIL_SIG));
                self.drain_pending_tx(ctx);
            }
            TransportAction::TxComplete => {
                // Shouldn't normally fire from a timeout, but handle defensively.
                self.retransmit_frame = None;
                self.state            = AoState::Idle;
                self.app_ao.post(DynEvent::empty_dyn(RF_TX_DONE_SIG));
                self.drain_pending_tx(ctx);
            }
            TransportAction::Nothing => {}
        }
    }

    /// Handle PHY RxDone event — read frame from radio and drive RX stack.
    ///
    /// Emits `RF_PHY_RX` (payload tag `1`) at entry — the moment the
    /// cooperative loop's `pump()` noticed a captured frame, not
    /// necessarily true physical arrival time (bounded by polling cadence).
    fn handle_rx_done(&mut self, ctx: &mut ActiveContext, event: &DynEvent) {
        let Some(payload) = event.payload.as_ref().downcast_ref::<PhyIrqPayload>() else { return };
        let meta = payload.meta;
        let _ = ctx.emit_trace(crate::records::RF_PHY_RX, &[1, meta.rssi_dbm as u8]);

        // Read raw bytes from radio into the DMA-aligned frame buffer.
        self.rx_frame = Frame::new();
        if self.stack.phy.read_rx(self.rx_frame.raw_buf_for_dma(), &meta).is_err() { return; }
        self.rx_frame.set_received_len(meta.pkt_len as usize);

        match self.stack.receive_raw(&mut self.rx_frame) {
            Ok(Some(app_frame)) => {
                // Check if this is an ACK for our outstanding reliable TX.
                // (The transport layer's `up` already mutated transport state;
                //  here we just check whether we were waiting.)
                if self.state == AoState::WaitingAck {
                    // Transport's `up` already called `on_ack_received` internally
                    // via the IS_ACK flag path.  If it returned Some(frame), that
                    // means a data frame (not just ACK) arrived — pass it up anyway.
                    // The ACK→TxComplete path is signalled by on_ack_received inside
                    // transport.up(), but we need to act on the state change here.
                    // Check: if transport is now Idle, the ACK was accepted.
                    // (For a data-bearing ACK the payload goes to app too.)
                    self.retransmit_frame = None;
                    if let Some(ref timer) = self.retransmit_timer { timer.disarm(); }
                    self.app_ao.post(DynEvent::empty_dyn(RF_TX_DONE_SIG));
                    self.state = AoState::Idle;
                }

                // Deliver received payload to application.
                let mut data = heapless::Vec::new();
                if data.extend_from_slice(app_frame.payload()).is_ok() {
                    let (src, kind) = self.stack.network.last_rx_meta();
                    let pld = RfRxFramePayload {
                        data,
                        port: 1,
                        rssi: meta.rssi_dbm,
                        snr:  meta.snr_db_x10,
                        src,
                        kind,
                    };
                    self.app_ao.post(DynEvent::with_arc(RF_RX_FRAME_SIG, Arc::new(pld)));
                }
                let _ = ctx.emit_trace(crate::records::RF_NET_ROUTE, &[meta.rssi_dbm as u8]);
            }
            Ok(None) => {
                // Frame dropped by stack (bad MIC, duplicate, wrong DevAddr, etc.)
            }
            Err(e) => {
                ceprintln!("RfStackAO: RX stack error: {e}");
            }
        }

        // Return to idle regardless of RX outcome — stack is ready for next frame.
        if self.state == AoState::Listening || self.state == AoState::ProcessingRx {
            self.state = AoState::Idle;
        }
        // Catches the ACK-received case above, which may have just resolved
        // WaitingAck → Idle; a no-op otherwise since nothing sets
        // `pending_tx` outside a busy state.
        self.drain_pending_tx(ctx);
    }

    fn handle_phy_irq(&mut self, _ctx: &mut ActiveContext, _event: &DynEvent) {
        // Generic DIO fallback — no specific action; individual signals
        // (TX_DONE, RX_DONE, RX_TIMEOUT, CRC_ERROR) are dispatched directly.
    }

    /// Arm continuous receive without a preceding TX (`RF_RX_START_SIG`).
    ///
    /// The Class-A flow only opens an RX window *after* a TX completes
    /// ([`handle_tx_done`](Self::handle_tx_done)), so a node that must receive
    /// before it ever transmits — e.g. an MC listening for a sensor's
    /// `PairRequest` / telemetry — would otherwise stay in `Standby` forever.
    /// A receive-first role posts `RF_RX_START_SIG` to enter (and, after each
    /// frame returns the AO to `Idle`, re-enter) continuous RX.
    ///
    /// Only acts from `Idle` so it never disturbs an in-flight TX or an open
    /// post-TX RX window; it is therefore safe to drive unconditionally from a
    /// cooperative poll loop.
    ///
    /// Emits `RF_RX_ARMED`: payload tag `1` if it actually armed continuous
    /// RX (state was `Idle`), tag `0` if it was a no-op (state was busy) —
    /// diagnostic for Stage 1.5 (see `start_tx`'s doc comment).
    fn handle_rx_start(&mut self, ctx: &mut ActiveContext) {
        if self.state != AoState::Idle {
            let _ = ctx.emit_trace(crate::records::RF_RX_ARMED, &[0]);
            return;
        }
        let _ = self.stack.phy.configure_rx(&self.rx_cfg);
        if self.stack.phy.set_mode(RadioMode::Rx { timeout_ms: None }).is_ok() {
            self.state = AoState::Listening;
            let _ = ctx.emit_trace(crate::records::RF_RX_ARMED, &[1]);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Cooperative polling
// ─────────────────────────────────────────────────────────────────────────────

impl<T: Layer + 'static, N: Layer + 'static, M: Layer + 'static, P: RfPhy + 'static>
    RfStackAO<T, N, M, P>
{
    /// Cooperatively poll the PHY for a pending IRQ and self-dispatch it.
    ///
    /// On bare-metal ports a port ISR bridge posts the PHY signals
    /// (`RF_PHY_*_SIG`) into this AO's queue. On polled targets (host
    /// simulation, or hardware without a wired DIO interrupt) there is no ISR —
    /// drive this method from the application loop or an idle hook to drain the
    /// radio's IRQ status and feed the resulting [`PhyEvent`] through the AO's
    /// normal signal handlers, exactly as the ISR bridge would.
    ///
    /// Returns `true` if an event was dispatched, `false` if the PHY was idle.
    pub fn pump(&mut self, ctx: &mut ActiveContext) -> bool {
        let event = match self.stack.phy.poll_irq() {
            Ok(Some(ev)) => ev,
            Ok(None)     => return false,
            Err(e)       => {
                ceprintln!("RfStackAO: poll_irq failed: {e}");
                return false;
            }
        };

        // Map the PHY event onto the same signals the ISR bridge posts, then
        // reuse `on_event` so the dispatch path is identical to the IRQ case.
        let dyn_event = match event {
            PhyEvent::TxDone       => DynEvent::empty_dyn(RF_PHY_TX_DONE_SIG),
            PhyEvent::RxDone(meta) => DynEvent::with_arc(
                RF_PHY_RX_DONE_SIG,
                Arc::new(PhyIrqPayload { event, meta }),
            ),
            PhyEvent::RxTimeout    => DynEvent::empty_dyn(RF_PHY_RX_TIMEOUT_SIG),
            PhyEvent::CrcError     => DynEvent::empty_dyn(RF_PHY_CRC_ERROR_SIG),
            // CadDone / PreambleDetected have no dedicated handler — route them
            // through the generic DIO fallback signal.
            _                      => DynEvent::empty_dyn(RF_PHY_IRQ_SIG),
        };

        self.on_event(ctx, dyn_event);
        true
    }

    /// Alias for [`pump`](Self::pump) for call sites that prefer `poll()`.
    #[inline]
    pub fn poll(&mut self, ctx: &mut ActiveContext) -> bool {
        self.pump(ctx)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ActiveBehavior impl
// ─────────────────────────────────────────────────────────────────────────────

impl<T: Layer + 'static, N: Layer + 'static, M: Layer + 'static, P: RfPhy + 'static>
    ActiveBehavior for RfStackAO<T, N, M, P>
{
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
            RF_TX_REQ_SIG            => self.handle_tx_req(ctx, &event),
            RF_RX_START_SIG          => self.handle_rx_start(ctx),
            RF_PHY_IRQ_SIG           => self.handle_phy_irq(ctx, &event),
            RF_PHY_RX_DONE_SIG       => self.handle_rx_done(ctx, &event),
            RF_PHY_TX_DONE_SIG       => self.handle_tx_done(ctx),
            RF_PHY_RX_TIMEOUT_SIG    => { self.state = AoState::Idle; }
            RF_PHY_CRC_ERROR_SIG     => { self.state = AoState::Idle; }
            RF_TRANSPORT_TIMEOUT_SIG => self.handle_transport_timeout(ctx),
            _                        => {}
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phy::loopback::LoopbackPhy;
    use crate::mac::lorawan::{LoRaWanMac, encrypt_frm_payload, compute_mic};
    use crate::mac::noop::NoopMac;
    use crate::net::NoopNetwork;
    use crate::transport::UnreliableTransport;
    use crate::session::LoRaSession;

    // ── RfStack TX → LoopbackPhy → inspect bytes ─────────────────────────────

    #[test]
    fn test_stack_tx_uplink() {
        let phy = LoopbackPhy::new();
        let session = LoRaSession::test_abp();
        let mac = LoRaWanMac::new(session.clone(), 1);
        let transport = UnreliableTransport::new();
        let network = NoopNetwork;

        let mut stack = RfStack::new(transport, network, mac, phy);

        let payload = b"hello modular stack tx";
        let tx_cfg = RfTxConfig {
            frequency_hz: 868_100_000,
            tx_power_dbm: 14,
            params: hal::rf::RadioParams::LoRa(hal::lora::LoRaModulation::default()),
        };
        stack.transmit(payload, &tx_cfg).expect("transmit failed");

        let rx_event = stack.phy.poll_irq().expect("poll_irq failed");
        assert!(rx_event.is_some());
        if let Some(hal::rf::PhyEvent::RxDone(meta)) = rx_event {
            let mut buf = vec![0u8; meta.pkt_len as usize];
            stack.phy.read_rx(&mut buf, &meta).unwrap();

            // 9 (MAC hdr) + 5 (Transport hdr) + 22 (payload) + 4 (MIC) = 40
            assert_eq!(buf.len(), 9 + 5 + payload.len() + 4);
            // MHDR = UnconfirmedDataUp
            assert_eq!(buf[0], 0x40);
            // DevAddr
            assert_eq!(&buf[1..5], &session.dev_addr);
        } else {
            panic!("Expected RxDone event");
        }
    }

    // ── RfStack RX downlink parse ─────────────────────────────────────────────

    #[test]
    fn test_stack_rx_downlink() {
        let phy = LoopbackPhy::new();
        let session = LoRaSession::test_abp();
        let mac = LoRaWanMac::new(session.clone(), 1);
        let transport = UnreliableTransport::new();
        let network = NoopNetwork;

        let mut stack = RfStack::new(transport, network, mac, phy);

        let dev_addr = session.dev_addr;
        let fcnt = 0u32;
        let dir = 1; // downlink

        let app_payload = b"hello modular stack rx";
        let mut transport_frame = Frame::new();
        transport_frame.write_payload(app_payload).unwrap();
        stack.transport.down(&mut transport_frame).unwrap();

        let mut frm_payload = transport_frame.payload().to_vec();
        encrypt_frm_payload(&mut frm_payload, &session.app_skey, &dev_addr, fcnt, dir).unwrap();

        let mut msg = Vec::new();
        msg.push(0x60); // MHDR: UnconfirmedDataDown
        msg.extend_from_slice(&dev_addr);
        msg.push(0x00); // FCtrl
        msg.push(fcnt as u8);
        msg.push((fcnt >> 8) as u8);
        msg.push(1); // FPort
        msg.extend_from_slice(&frm_payload);

        let mic = compute_mic(&msg, &session.nwk_skey, &dev_addr, fcnt, dir).unwrap();
        msg.extend_from_slice(&mic);

        let mut raw_frame = Frame::new();
        raw_frame.set_received_len(msg.len());
        raw_frame.raw_buf_for_dma()[..msg.len()].copy_from_slice(&msg);

        let out_frame = stack.receive_raw(&mut raw_frame).expect("receive_raw failed");
        assert!(out_frame.is_some());
        if let Some(out) = out_frame {
            assert_eq!(out.payload(), app_payload);
        }
    }

    // ── NoopMac compile-only composition tests ────────────────────────────────

    #[test]
    fn noop_mac_stack_compiles_and_tx_rx() {
        let mut stack = RfStack::new(
            UnreliableTransport::new(),
            NoopNetwork,
            NoopMac,
            LoopbackPhy::new(),
        );
        let tx_cfg = RfTxConfig {
            frequency_hz: 868_100_000,
            tx_power_dbm: 14,
            params: hal::rf::RadioParams::LoRa(hal::lora::LoRaModulation::default()),
        };
        stack.transmit(b"loopback test", &tx_cfg).unwrap();

        if let Some(hal::rf::PhyEvent::RxDone(meta)) = stack.phy.poll_irq().unwrap() {
            let mut raw = Frame::new();
            stack.phy.read_rx(raw.raw_buf_for_dma(), &meta).unwrap();
            raw.set_received_len(meta.pkt_len as usize);

            // NoopMac + NoopNetwork + UnreliableTransport: only 5-byte transport hdr
            let out = stack.receive_raw(&mut raw).unwrap();
            assert!(out.is_some());
            assert_eq!(out.unwrap().payload(), b"loopback test");
        }
    }

    // ── RfStackAO: receive-first (RF_RX_START) + TX from a listen state ───────

    /// A receive-first node (e.g. the SWM MC) must be able to arm RX without a
    /// preceding TX (`RF_RX_START_SIG`) and then transmit its reply straight
    /// out of the `Listening` state. Drive a real `RfStackAO` through a kernel:
    /// post `RF_RX_START`, then `RF_TX_REQ`; the loopback PHY echoes the TX, and
    /// `pump` must deliver it back to the app AO — proving the TX was *not*
    /// dropped while listening.
    #[test]
    fn ao_listens_then_transmits_from_listen_state() {
        use crate::events::{RfTxReqPayload, RfRxFramePayload, RF_RX_FRAME_SIG, RF_RX_START_SIG};
        use qf::active::{arc_as_runnable, ActiveObject, ActiveObjectId, ActiveBehavior, ActiveContext};
        use qf::kernel::Kernel;
        use alloc::sync::Arc;

        struct CaptureAo {
            rx: Vec<Vec<u8>>,
        }
        impl ActiveBehavior for CaptureAo {
            fn on_start(&mut self, _c: &mut ActiveContext) {}
            fn on_event(&mut self, _c: &mut ActiveContext, e: DynEvent) {
                if e.signal() == RF_RX_FRAME_SIG {
                    if let Some(p) = e.payload.as_ref().downcast_ref::<RfRxFramePayload>() {
                        self.rx.push(p.data.iter().copied().collect());
                    }
                }
            }
        }

        let tx_cfg = RfTxConfig {
            frequency_hz: 868_100_000,
            tx_power_dbm: 14,
            params: hal::rf::RadioParams::LoRa(hal::lora::LoRaModulation::default()),
        };
        let rx_cfg = RfRxConfig {
            frequency_hz: 868_100_000,
            timeout_ms: None,
            params: hal::rf::RadioParams::LoRa(hal::lora::LoRaModulation::default()),
        };
        let stack = RfStack::new(UnreliableTransport::new(), NoopNetwork, NoopMac, LoopbackPhy::new());

        let capture = ActiveObject::new(ActiveObjectId(0), 1, CaptureAo { rx: Vec::new() });
        let ao = ActiveObject::new(
            ActiveObjectId(2),
            2,
            RfStackAO::new(stack, tx_cfg, rx_cfg, arc_as_runnable(Arc::clone(&capture))),
        );

        let kernel = Kernel::builder()
            .register(arc_as_runnable(Arc::clone(&ao)))
            .register(arc_as_runnable(Arc::clone(&capture)))
            .build();
        kernel.start();

        // Arm RX without any TX (receive-first), then queue a reply.
        arc_as_runnable(Arc::clone(&ao)).post(DynEvent::empty_dyn(RF_RX_START_SIG));
        kernel.dispatch_once(); // RF AO: RF_RX_START -> Listening
        let frame = vec![0xA5_u8, 0x5A, 0x10, 0x20];
        arc_as_runnable(Arc::clone(&ao))
            .post(DynEvent::with_arc(RF_TX_REQ_SIG, Arc::new(RfTxReqPayload::new(frame.clone(), 1))));
        kernel.dispatch_once(); // RF AO: TX from Listening (loopback echoes)
        // Drain the looped RxDone and let the app AO record it.
        let mut ctx = ActiveContext::new(ActiveObjectId(2), None);
        assert!(ao.with_behavior_mut(|rf| rf.pump(&mut ctx)), "pump should dispatch the echo");
        kernel.dispatch_once();

        capture.with_behavior(|c| {
            assert_eq!(c.rx.len(), 1, "TX from Listening must not be dropped");
            assert_eq!(c.rx[0], frame, "looped frame must round-trip byte-identical");
        });
    }
}
