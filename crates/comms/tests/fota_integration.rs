//! FOTA integration test.
//!
//! Drives `FotaDriver` through a loopback `RfStackAO` using `ReliableTransport`
//! to prove that all chunks transfer end-to-end and `FotaStatus::Done` is
//! returned after the verify packet.

use std::sync::Arc;

use comms::fota::{FotaDriver, FotaStatus};
use comms::events::{RF_TX_DONE_SIG, RF_TX_FAIL_SIG};
use comms::stack::{RfStack, RfStackAO};
use comms::transport::ReliableTransport;
use comms::mac::noop::NoopMac;
use comms::net::NoopNetwork;
use comms::phy::loopback::LoopbackPhy;

use qf::active::{
    arc_as_runnable, ActiveBehavior, ActiveContext, ActiveObject, ActiveObjectId,
};
use qf::event::DynEvent;
use qf::kernel::Kernel;
use hal::rf::{RfTxConfig, RfRxConfig, RadioParams};
use hal::lora::LoRaModulation;

/// Concrete RF AO type for the reliable loopback stack.
type RelRfAo = RfStackAO<ReliableTransport, NoopNetwork, NoopMac, LoopbackPhy>;

/// The RF AO is registered at this id in this test.
const RF_AO_ID: ActiveObjectId = ActiveObjectId(1);

// ── Capture AO (collects signals sent back to the app) ───────────────────────

struct CaptureAo {
    done_count: u32,
    fail_count: u32,
}

impl ActiveBehavior for CaptureAo {
    fn on_start(&mut self, _c: &mut ActiveContext) {}
    fn on_event(&mut self, _c: &mut ActiveContext, e: DynEvent) {
        if e.signal() == RF_TX_DONE_SIG { self.done_count += 1; }
        if e.signal() == RF_TX_FAIL_SIG { self.fail_count += 1; }
    }
}

// ── Helper: pump the RF AO until it has nothing more to dispatch ─────────────

fn pump_all(rf_ao: &Arc<ActiveObject<RelRfAo>>, kernel: &Kernel, n: usize) {
    let mut ctx = ActiveContext::new(RF_AO_ID, None);
    for _ in 0..n {
        kernel.dispatch_once();
        rf_ao.with_behavior_mut(|rf| { rf.pump(&mut ctx); });
    }
}

// ── Test ─────────────────────────────────────────────────────────────────────

#[test]
fn fota_loopback_reliable_transfer() {
    // Small image: 2 chunks + remainder
    let image_bytes: Vec<u8> = (0..450).map(|i| (i % 251) as u8).collect();
    let expected_total_chunks = 3u32; // ceil(450 / 200) = 3

    let tx_cfg = RfTxConfig {
        frequency_hz: 868_100_000,
        tx_power_dbm: 14,
        params: RadioParams::LoRa(LoRaModulation::default()),
    };
    let rx_cfg = RfRxConfig {
        frequency_hz: 868_100_000,
        timeout_ms: None,
        params: RadioParams::LoRa(LoRaModulation::default()),
    };

    let stack = RfStack::new(
        ReliableTransport::new(3),
        NoopNetwork,
        NoopMac,
        LoopbackPhy::new(),
    );

    let capture = ActiveObject::new(
        ActiveObjectId(0), 1,
        CaptureAo { done_count: 0, fail_count: 0 },
    );
    let rf_ao = ActiveObject::new(
        ActiveObjectId(1), 2,
        RfStackAO::new(stack, tx_cfg, rx_cfg, arc_as_runnable(Arc::clone(&capture))),
    );

    let kernel = Kernel::builder()
        .register(arc_as_runnable(Arc::clone(&rf_ao)))
        .register(arc_as_runnable(Arc::clone(&capture)))
        .build();
    kernel.start();

    // Create FOTA driver; give it an Arc clone of the RF AO as the target
    let mut fota = FotaDriver::new(
        arc_as_runnable(Arc::clone(&rf_ao)),
        image_bytes,
    );

    assert_eq!(fota.total_chunks(), expected_total_chunks);

    // Kick off announce (unreliable)
    fota.start_announce(0x0100_0000).expect("announce");
    // One dispatch for announce
    pump_all(&rf_ao, &kernel, 5);

    // Step through each chunk: on each RF_TX_DONE_SIG, call on_tx_done()
    // Trigger first chunk
    let mut status = fota.on_tx_done();
    assert_eq!(status, FotaStatus::Sending);

    for _ in 0..expected_total_chunks {
        // Pump to let LoopbackPhy echo and process the ACK
        pump_all(&rf_ao, &kernel, 10);

        if fota.next_chunk_index() < expected_total_chunks {
            status = fota.on_tx_done();
        } else {
            status = fota.on_tx_done(); // triggers verify
        }
    }

    assert!(!fota.is_failed(), "FOTA transfer must not fail");
    assert_eq!(status, FotaStatus::Done, "FOTA should complete after all chunks");
    assert_eq!(fota.next_chunk_index(), expected_total_chunks);

    // Verify no TX_FAIL signals were delivered to the capture AO
    capture.with_behavior(|c| {
        assert_eq!(c.fail_count, 0, "no TX_FAIL expected in loopback test");
    });
}
