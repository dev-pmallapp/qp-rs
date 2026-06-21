//! No-op MAC layer — pass-through for testing transport without encryption.
//!
//! `NoopMac` is useful when you need to test `ReliableTransport` or the
//! full `RfStack` composition without LoRaWAN cryptography overhead.
//! It simply passes frames through unchanged in both directions.

use crate::stack::Layer;
use crate::buf::Frame;
use crate::error::CommsError;

/// No-op MAC layer: adds no headers, performs no encryption or authentication.
///
/// Used in unit tests where MAC processing would obscure the transport layer
/// logic under test, and by the `BleL2cap` stub (future).
pub struct NoopMac;

impl Layer for NoopMac {
    fn down(&mut self, _frame: &mut Frame) -> Result<(), CommsError> {
        // No MAC header to prepend; frame passes through as-is.
        Ok(())
    }

    fn up(&mut self, _frame: &mut Frame) -> Result<bool, CommsError> {
        // No MAC header to validate or strip; always accept.
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buf::Frame;

    #[test]
    fn noop_mac_down_is_transparent() {
        let mut mac = NoopMac;
        let mut f = Frame::new();
        let payload = b"transparent payload";
        f.write_payload(payload).unwrap();

        let original_len = f.len();
        mac.down(&mut f).unwrap();
        assert_eq!(f.len(), original_len, "NoopMac::down must not change frame length");
        assert_eq!(f.payload(), payload);
    }

    #[test]
    fn noop_mac_up_is_transparent() {
        let mut mac = NoopMac;
        let mut f = Frame::new();
        f.write_payload(b"rx data").unwrap();

        let original_len = f.len();
        let keep = mac.up(&mut f).unwrap();
        assert!(keep, "NoopMac::up must always accept");
        assert_eq!(f.len(), original_len);
    }
}
