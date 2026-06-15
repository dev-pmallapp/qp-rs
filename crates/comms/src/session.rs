//! LoRaWAN ABP session parameters.

/// ABP (Activation By Personalisation) session keys and device address.
#[derive(Clone)]
pub struct LoRaSession {
    /// 4-byte device address (little-endian in frame).
    pub dev_addr:  [u8; 4],
    /// 128-bit Network Session Key (for MIC and MAC command encryption).
    pub nwk_skey:  [u8; 16],
    /// 128-bit Application Session Key (for FRMPayload encryption).
    pub app_skey:  [u8; 16],
    /// Uplink frame counter (incremented after each TX).
    pub fcnt_up:   u32,
}

impl LoRaSession {
    /// Creates a LoRaWAN session from a device address and the network/app
    /// session keys, with the uplink frame counter starting at zero.
    pub fn new(dev_addr: [u8; 4], nwk_skey: [u8; 16], app_skey: [u8; 16]) -> Self {
        Self { dev_addr, nwk_skey, app_skey, fcnt_up: 0 }
    }

    /// Well-known test session (all-zeros keys, DevAddr 0x01020304).
    pub fn test_abp() -> Self {
        Self::new([0x04, 0x03, 0x02, 0x01], [0u8; 16], [0u8; 16])
    }
}
