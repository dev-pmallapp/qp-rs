//! RF stack integration tests.
//!
//! Drives `RfStackAO` through a real `Kernel` with a `LoopbackPhy` to verify
//! the end-to-end signal flows described in the RF_STACK_PLAN T7 card.

use std::sync::Arc;

use comms::events::{
    RfTxReqPayload, RfRxFramePayload,
    RF_TX_REQ_SIG, RF_TX_DONE_SIG, RF_TX_FAIL_SIG, RF_RX_FRAME_SIG, RF_RX_START_SIG,
    RF_PHY_TX_DONE_SIG,
};
use comms::stack::{RfStack, RfStackAO};
use comms::transport::{ReliableTransport, UnreliableTransport};
use comms::mac::noop::NoopMac;
use comms::net::NoopNetwork;
use comms::phy::loopback::LoopbackPhy;

/// Concrete RF AO type for the loopback/no-op unreliable stack used below.
/// `pump()` is an inherent method on `RfStackAO`, so helpers must name the
/// concrete type rather than `impl ActiveBehavior`.
type UnrelRfAo = RfStackAO<UnreliableTransport, NoopNetwork, NoopMac, LoopbackPhy>;

/// The RF AO is always registered at this id in these tests.
const RF_AO_ID: ActiveObjectId = ActiveObjectId(1);

use qf::active::{
    arc_as_runnable, ActiveBehavior, ActiveContext, ActiveObject, ActiveObjectId,
};
use qf::event::DynEvent;
use qf::kernel::Kernel;
use hal::rf::{RfTxConfig, RfRxConfig, RadioParams};
use hal::lora::LoRaModulation;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn eu868_tx() -> RfTxConfig {
    RfTxConfig {
        frequency_hz: 868_100_000,
        tx_power_dbm: 14,
        params: RadioParams::LoRa(LoRaModulation::default()),
    }
}

fn eu868_rx() -> RfRxConfig {
    RfRxConfig {
        frequency_hz: 868_100_000,
        timeout_ms: None,
        params: RadioParams::LoRa(LoRaModulation::default()),
    }
}

// Capture AO: records signals it receives
struct CaptureAo {
    done:  u32,
    fail:  u32,
    frames: Vec<Vec<u8>>,
}

impl ActiveBehavior for CaptureAo {
    fn on_start(&mut self, _c: &mut ActiveContext) {}
    fn on_event(&mut self, _c: &mut ActiveContext, e: DynEvent) {
        if e.signal() == RF_TX_DONE_SIG { self.done += 1; }
        if e.signal() == RF_TX_FAIL_SIG { self.fail += 1; }
        if e.signal() == RF_RX_FRAME_SIG {
            if let Some(p) = e.payload.as_ref().downcast_ref::<RfRxFramePayload>() {
                self.frames.push(p.data.iter().copied().collect());
            }
        }
    }
}

fn build_stack_unreliable() -> (
    Arc<ActiveObject<UnrelRfAo>>,
    Arc<ActiveObject<CaptureAo>>,
    Kernel,
) {
    let capture = ActiveObject::new(ActiveObjectId(0), 1, CaptureAo { done: 0, fail: 0, frames: vec![] });
    let stack = RfStack::new(UnreliableTransport::new(), NoopNetwork, NoopMac, LoopbackPhy::new());
    let rf_ao = ActiveObject::new(
        ActiveObjectId(1), 2,
        RfStackAO::new(stack, eu868_tx(), eu868_rx(), arc_as_runnable(Arc::clone(&capture))),
    );
    let kernel = Kernel::builder()
        .register(arc_as_runnable(Arc::clone(&rf_ao)))
        .register(arc_as_runnable(Arc::clone(&capture)))
        .build();
    kernel.start();
    (rf_ao, capture, kernel)
}

fn pump(rf_ao: &Arc<ActiveObject<UnrelRfAo>>, kernel: &Kernel, n: usize) {
    let mut ctx = ActiveContext::new(RF_AO_ID, None);
    for _ in 0..n {
        kernel.dispatch_once();
        rf_ao.with_behavior_mut(|rf| { rf.pump(&mut ctx); });
    }
}

// ── T7a: unreliable TX → RF_TX_DONE_SIG ─────────────────────────────────────

#[test]
fn unreliable_tx_delivers_done_to_app() {
    let (rf_ao, capture, kernel) = build_stack_unreliable();

    // App requests an unreliable TX.
    arc_as_runnable(Arc::clone(&rf_ao)).post(DynEvent::with_arc(
        RF_TX_REQ_SIG,
        Arc::new(RfTxReqPayload::new(b"hello".to_vec(), 1)),
    ));
    kernel.dispatch_once(); // AO transmits, enters Transmitting

    // `LoopbackPhy` models TX as an echoed RxDone and never raises TxDone, so
    // post the `RF_PHY_TX_DONE_SIG` the port ISR bridge would post on a real
    // radio. That drives `handle_tx_done`, which notifies the app of TX_DONE.
    arc_as_runnable(Arc::clone(&rf_ao)).post(DynEvent::empty_dyn(RF_PHY_TX_DONE_SIG));
    kernel.dispatch_once(); // handle_tx_done -> posts RF_TX_DONE_SIG
    kernel.dispatch_once(); // capture AO receives it

    capture.with_behavior(|c| {
        assert_eq!(c.done, 1, "TX_DONE must be delivered for unreliable TX");
        assert_eq!(c.fail, 0);
    });
}

// ── T7b: receive-first → RF_RX_START_SIG → frame delivered ──────────────────

#[test]
fn receive_first_rx_frame_delivered() {
    let (rf_ao, capture, kernel) = build_stack_unreliable();

    // Arm RX without prior TX
    arc_as_runnable(Arc::clone(&rf_ao)).post(DynEvent::empty_dyn(RF_RX_START_SIG));
    kernel.dispatch_once(); // enters Listening

    // Then send a frame — loopback echoes it back as RxDone
    arc_as_runnable(Arc::clone(&rf_ao)).post(DynEvent::with_arc(
        RF_TX_REQ_SIG,
        Arc::new(RfTxReqPayload::new(b"from_listen".to_vec(), 1)),
    ));
    kernel.dispatch_once(); // TX from Listening — loopback echoes

    // Pump to process echo → deliver RF_RX_FRAME_SIG to capture AO
    pump(&rf_ao, &kernel, 10);

    capture.with_behavior(|c| {
        assert_eq!(c.frames.len(), 1, "RX frame must be delivered to app AO");
    });
}

// ── T7c: reliable TX no ACK → retransmits → RF_TX_FAIL_SIG ──────────────────

#[test]
fn reliable_tx_no_ack_gives_tx_fail_after_exhaustion() {

    // Use a LoopbackPhy that we'll prevent from echoing by draining manually
    // The trick: use ReliableTransport with max_retries=2, then fire the
    // RF_TRANSPORT_TIMEOUT_SIG manually via a fake TimeEvent tick sequence.
    // In the unit stack we drive on_timeout() via the AO's dispatch.

    // Build a reliable stack — LoopbackPhy WILL echo, but we intercept via
    // a short-circuit: don't pump after TX (so ACK never arrives).
    let capture = ActiveObject::new(ActiveObjectId(0), 1, CaptureAo { done: 0, fail: 0, frames: vec![] });
    let stack = RfStack::new(ReliableTransport::new(2), NoopNetwork, NoopMac, LoopbackPhy::new());
    let rf_ao = ActiveObject::new(
        ActiveObjectId(1), 2,
        RfStackAO::new(stack, eu868_tx(), eu868_rx(), arc_as_runnable(Arc::clone(&capture))),
    );

    let kernel = Kernel::builder()
        .register(arc_as_runnable(Arc::clone(&rf_ao)))
        .register(arc_as_runnable(Arc::clone(&capture)))
        .build();
    kernel.start();

    // Post a reliable TX request
    arc_as_runnable(Arc::clone(&rf_ao)).post(DynEvent::with_arc(
        RF_TX_REQ_SIG,
        Arc::new(RfTxReqPayload::with_reliability(b"reliable".to_vec(), 1, true)),
    ));
    kernel.dispatch_once(); // AO processes TX request (now in WaitingAck)

    // Drive retransmit timeouts without pumping (no ACK arrives)
    // max_retries=2 → need 3 timeouts to GiveUp
    use comms::events::RF_TRANSPORT_TIMEOUT_SIG;
    for _ in 0..3 {
        arc_as_runnable(Arc::clone(&rf_ao)).post(DynEvent::empty_dyn(RF_TRANSPORT_TIMEOUT_SIG));
        kernel.dispatch_once();
        kernel.dispatch_once(); // capture AO
    }

    capture.with_behavior(|c| {
        assert_eq!(c.fail, 1, "TX_FAIL must be delivered after retransmit exhaustion");
        assert_eq!(c.done, 0);
    });
}
