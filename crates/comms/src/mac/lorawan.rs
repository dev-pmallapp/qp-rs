//! LoRaWAN Class A MAC layer.

use crate::stack::Layer;
use crate::buf::Frame;
use crate::error::CommsError;
use crate::session::LoRaSession;

pub struct LoRaWanMac {
    dev_addr: [u8; 4],
    nwk_skey: [u8; 16],
    app_skey: [u8; 16],
    fcnt_up:  u32,
    fcnt_dn:  u32,
    fport:    u8,
}

impl LoRaWanMac {
    pub fn new(session: LoRaSession, fport: u8) -> Self {
        Self {
            dev_addr: session.dev_addr,
            nwk_skey: session.nwk_skey,
            app_skey: session.app_skey,
            fcnt_up:  session.fcnt_up,
            fcnt_dn:  0,
            fport,
        }
    }

    /// Access the uplink frame counter.
    pub fn fcnt_up(&self) -> u32 {
        self.fcnt_up
    }
}

impl Layer for LoRaWanMac {
    fn down(&mut self, frame: &mut Frame) -> Result<(), CommsError> {
        // 1. Encrypt FRMPayload in-place (AES-128 CTR, AppSKey)
        encrypt_frm_payload(frame.payload_mut(), &self.app_skey, &self.dev_addr, self.fcnt_up, 0)?;

        // 2. Prepend 9-byte LoRaWAN MAC header
        //    MHDR(1) | DevAddr(4LE) | FCtrl(1) | FCnt(2LE) | FPort(1)
        let hdr = frame.prepend_header(9)?;
        hdr[0] = 0x40;                                   // MHDR: UnconfirmedDataUp
        hdr[1..5].copy_from_slice(&self.dev_addr);       // DevAddr LE
        hdr[5] = 0x00;                                   // FCtrl: no ADR, no opts
        hdr[6] = self.fcnt_up as u8;                     // FCnt LSB
        hdr[7] = (self.fcnt_up >> 8) as u8;              // FCnt MSB
        hdr[8] = self.fport;                             // FPort

        // 3. Compute MIC = AES-128-CMAC(NwkSKey, B0 ‖ msg)[0..4]
        let mic = compute_mic(frame.payload(), &self.nwk_skey, &self.dev_addr, self.fcnt_up, 0)?;
        frame.append_trailer(&mic)?;

        self.fcnt_up = self.fcnt_up.wrapping_add(1);
        Ok(())
    }

    fn up(&mut self, frame: &mut Frame) -> Result<bool, CommsError> {
        if frame.len() < 13 { return Ok(false); }   // minimum LoRaWAN downlink

        // 1. Strip MIC (last 4 bytes) first
        let mic_recv = {
            let raw = frame.trim_trailer(4)?;
            let mut m = [0u8; 4];
            m.copy_from_slice(raw);
            m
        };

        // 2. Parse MAC header from the start of the payload
        let payload = frame.payload();
        let mhdr     = payload[0];
        let dev_addr = [payload[1], payload[2], payload[3], payload[4]];
        let fcnt     = u16::from_le_bytes([payload[6], payload[7]]) as u32;

        // 3. Validate DevAddr
        if dev_addr != self.dev_addr { return Ok(false); }

        // 4. Verify MIC over the entire remaining payload
        let mic_calc = compute_mic(frame.payload(), &self.nwk_skey, &self.dev_addr, fcnt, 1)?;
        if mic_recv != mic_calc { return Ok(false); }

        // 5. Now strip the 9-byte MAC header
        let fport_byte = frame.strip_header(9)?[8];
        let _ = mhdr; let _ = fport_byte;  // used for future dispatch

        // 6. Decrypt FRMPayload in-place
        encrypt_frm_payload(frame.payload_mut(), &self.app_skey, &self.dev_addr, fcnt, 1)?;

        self.fcnt_dn = fcnt.wrapping_add(1);
        Ok(true)
    }
}

pub fn encrypt_frm_payload(
    data: &mut [u8],
    key: &[u8; 16],
    dev_addr: &[u8; 4],
    fcnt: u32,
    dir: u8,
) -> Result<(), CommsError> {
    use aes::Aes128;
    use aes::cipher::{BlockEncrypt, KeyInit};
    let app_cipher = Aes128::new_from_slice(key)
        .map_err(|_| CommsError::MacError)?;

    let num_blocks = data.len().div_ceil(16);
    let mut keystream = [0u8; 256];
    for i in 1..=num_blocks {
        let mut a = [0u8; 16];
        a[0]  = 0x01;
        a[5]  = dir;
        a[6..10].copy_from_slice(dev_addr);
        a[10] = (fcnt      ) as u8;
        a[11] = (fcnt >>  8) as u8;
        a[12] = (fcnt >> 16) as u8;
        a[13] = (fcnt >> 24) as u8;
        a[15] = i as u8;
        let mut block = aes::Block::from(a);
        app_cipher.encrypt_block(&mut block);
        let s = (i - 1) * 16;
        keystream[s..s + 16].copy_from_slice(block.as_slice());
    }
    for (i, b) in data.iter_mut().enumerate() {
        *b ^= keystream[i];
    }
    Ok(())
}

pub fn compute_mic(
    msg: &[u8],
    key: &[u8; 16],
    dev_addr: &[u8; 4],
    fcnt: u32,
    dir: u8,
) -> Result<[u8; 4], CommsError> {
    use aes::Aes128;
    use cmac::{Cmac, Mac};
    use aes::cipher::KeyInit;
    let mut b0 = [0u8; 16];
    b0[0]  = 0x49;
    b0[5]  = dir;
    b0[6..10].copy_from_slice(dev_addr);
    b0[10] = (fcnt      ) as u8;
    b0[11] = (fcnt >>  8) as u8;
    b0[12] = (fcnt >> 16) as u8;
    b0[13] = (fcnt >> 24) as u8;
    b0[15] = msg.len() as u8;

    let mut mac: Cmac<Aes128> = <Cmac<Aes128> as KeyInit>::new_from_slice(key)
        .map_err(|_| CommsError::MacError)?;
    mac.update(&b0);
    mac.update(msg);
    let mic_bytes = mac.finalize().into_bytes();
    let mut mic = [0u8; 4];
    mic.copy_from_slice(&mic_bytes[..4]);
    Ok(mic)
}
