//! MAC layer implementations and legacy CommsAO.

pub mod lorawan;
pub mod noop;
pub mod ble_l2cap;


use qf::active::{ActiveBehavior, ActiveContext};
use qf::event::DynEvent;
use crate::events::{RfTxReqPayload, RF_TX_REQ_SIG};
use crate::lora::LoRaRf;
use hal::rf::RfPhy;

/// Active object that owns and drives an [`LoRaRf`].
pub struct CommsAO<D: RfPhy + 'static> {
    rf:          LoRaRf<D>,
    initialized: bool,
}

impl<D: RfPhy + 'static> CommsAO<D> {
    /// Creates a comms active object wrapping the given LoRa transport.
    pub fn new(rf: LoRaRf<D>) -> Self {
        Self { rf, initialized: false }
    }
}

impl<D: RfPhy + Send + 'static> ActiveBehavior for CommsAO<D> {
    fn on_start(&mut self, ctx: &mut ActiveContext) {
        self.rf.set_trace_hook(ctx.trace_hook());

        cprintln!("CommsAO: initialising RF ({})", self.rf.chip_name());
        match self.rf.init() {
            Ok(()) => {
                self.initialized = true;
                cprintln!("CommsAO: RF ready");
            }
            Err(e) => ceprintln!("CommsAO: RF init failed: {e}"),
        }
    }

    fn on_event(&mut self, _ctx: &mut ActiveContext, event: DynEvent) {
        match event.signal() {
            RF_TX_REQ_SIG => {
                if !self.initialized {
                    ceprintln!("CommsAO: RF not initialised, dropping TX request");
                    return;
                }
                if let Some(req) = event.payload.as_ref().downcast_ref::<RfTxReqPayload>() {
                    match self.rf.send_with_fport(&req.data, req.fport) {
                        Ok(()) => cprintln!(
                            "CommsAO: TX ok via {}, FCnt={}",
                            self.rf.chip_name(),
                            self.rf.session().fcnt_up,
                        ),
                        Err(e) => ceprintln!("CommsAO: TX failed: {e}"),
                    }
                }
            }
            _ => {}
        }
    }
}
