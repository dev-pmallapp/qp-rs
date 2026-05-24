//! LoRa / LoRaWAN Class A implementation of the [`Rf`] trait.
//!
//! [`LoRaRf`] wraps a chip-level [`RfDriver`] together with a LoRaWAN ABP
//! session and builds the full uplink MAC frame before delegating to the
//! driver for the actual SPI/radio operations.
//!
//! Frame layout (LoRaWAN 1.0.x):
//! ```text
//! MHDR(1) | DevAddr(4LE) | FCtrl(1) | FCnt(2LE) | FPort(1) | FRMPayload | MIC(4)
//! ```
//! FRMPayload is AES-128 CTR encrypted with AppSKey; MIC is AES-128 CMAC
//! over B0‖msg with NwkSKey.

use aes::Aes128;
use aes::cipher::{BlockEncrypt, KeyInit};
use cmac::{Cmac, Mac};

use hal::lora::{LoRaTxConfig, RfDriver};
use qf::TraceHook;

#[cfg(feature = "qs")]
use qs::UserRecordBuilder;

use crate::error::CommsError;
use crate::records::LORA_TX_PKT;
use crate::rf::Rf;
use crate::session::LoRaSession;

/// LoRaWAN Class A RF implementation.
pub struct LoRaRf<D: RfDriver> {
    driver:     D,
    session:    LoRaSession,
    tx_config:  LoRaTxConfig,
    name:       &'static str,
    trace_hook: Option<TraceHook>,
}

impl<D: RfDriver> LoRaRf<D> {
    pub fn new(driver: D, session: LoRaSession, tx_config: LoRaTxConfig) -> Self {
        let name = driver.chip_name();
        Self { driver, session, tx_config, name, trace_hook: None }
    }

    pub fn set_trace_hook(&mut self, hook: Option<TraceHook>) {
        self.trace_hook = hook;
    }

    pub fn session(&self) -> &LoRaSession { &self.session }
    pub fn tx_config(&self) -> &LoRaTxConfig { &self.tx_config }
    pub fn chip_name(&self) -> &'static str { self.name }

    pub fn init(&mut self) -> Result<(), CommsError> {
        self.driver.init().map_err(CommsError::from)
    }

    /// Build a LoRaWAN uplink frame, emit a QS trace record, then transmit.
    pub fn send_with_fport(&mut self, payload: &[u8], fport: u8)
        -> Result<(), CommsError>
    {
        let (frame_buf, frame_len) = self.build_frame(payload, fport)?;
        let frame = &frame_buf[..frame_len];

        #[cfg(feature = "qs")]
        if let Some(ref hook) = self.trace_hook {
            let cfg = &self.tx_config;
            let mut b = UserRecordBuilder::with_capacity(8 + frame.len());
            b.push_u32(4, cfg.channel.frequency_hz);
            b.push_u8(1, cfg.modulation.sf as u8);
            b.push_u8(1, cfg.modulation.bw as u8);
            b.push_u8(1, cfg.modulation.cr as u8);
            b.push_u8(1, cfg.tx_power_dbm as u8);
            b.push_mem(frame);
            let _ = hook(LORA_TX_PKT, &b.into_vec(), true);
        }

        self.driver.transmit(&self.tx_config, frame)
            .map_err(CommsError::from)
    }

    // ─── private ─────────────────────────────────────────────────────────────

    fn build_frame(&mut self, data: &[u8], fport: u8)
        -> Result<([u8; 256], usize), CommsError>
    {
        if data.len() > 242 { return Err(CommsError::MacError); }

        let dev_addr = self.session.dev_addr;
        let fcnt     = self.session.fcnt_up;
        const DIR: u8 = 0; // uplink

        // ── 1. Encrypt FRMPayload (AES-128 CTR, AppSKey) ─────────────────────
        let app_cipher = Aes128::new_from_slice(&self.session.app_skey)
            .map_err(|_| CommsError::MacError)?;

        let num_blocks = data.len().div_ceil(16);
        let mut keystream = [0u8; 256];
        for i in 1..=num_blocks {
            let mut a = [0u8; 16];
            a[0]  = 0x01;
            a[5]  = DIR;
            a[6..10].copy_from_slice(&dev_addr);
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
        let mut frm = [0u8; 242];
        for (i, b) in data.iter().enumerate() { frm[i] = b ^ keystream[i]; }

        // ── 2. Assemble raw message ───────────────────────────────────────────
        let mut msg = [0u8; 252]; // 1+7+1+242+1 fits well inside 252
        let mut p = 0usize;
        msg[p] = 0x40; p += 1;                         // MHDR UnconfirmedDataUp
        msg[p..p+4].copy_from_slice(&dev_addr); p += 4; // DevAddr LE
        msg[p] = 0x00; p += 1;                          // FCtrl (no opts)
        msg[p] = (fcnt      ) as u8; p += 1;            // FCnt LSB
        msg[p] = (fcnt >>  8) as u8; p += 1;            // FCnt MSB
        msg[p] = fport; p += 1;                          // FPort
        msg[p..p + data.len()].copy_from_slice(&frm[..data.len()]); p += data.len();
        let msg_len = p;

        // ── 3. MIC = AES-128-CMAC(NwkSKey, B0 ‖ msg)[0..4] ─────────────────
        let mut b0 = [0u8; 16];
        b0[0]  = 0x49;
        b0[5]  = DIR;
        b0[6..10].copy_from_slice(&dev_addr);
        b0[10] = (fcnt      ) as u8;
        b0[11] = (fcnt >>  8) as u8;
        b0[12] = (fcnt >> 16) as u8;
        b0[13] = (fcnt >> 24) as u8;
        b0[15] = msg_len as u8;

        let mut mac: Cmac<Aes128> = <Cmac<Aes128> as KeyInit>::new_from_slice(&self.session.nwk_skey)
            .map_err(|_| CommsError::MacError)?;
        mac.update(&b0);
        mac.update(&msg[..msg_len]);
        let mic_bytes = mac.finalize().into_bytes();

        // ── 4. Final PHYPayload ───────────────────────────────────────────────
        let mut buf = [0u8; 256];
        buf[..msg_len].copy_from_slice(&msg[..msg_len]);
        buf[msg_len..msg_len + 4].copy_from_slice(&mic_bytes[..4]);
        let total_len = msg_len + 4;

        self.session.fcnt_up = self.session.fcnt_up.wrapping_add(1);
        Ok((buf, total_len))
    }
}

impl<D: RfDriver> Rf for LoRaRf<D> {
    fn chip_name(&self) -> &'static str { self.name }

    fn send(&mut self, payload: &[u8]) -> Result<(), CommsError> {
        self.send_with_fport(payload, 1)
    }

    fn receive(&mut self, _buf: &mut [u8]) -> Result<usize, CommsError> {
        Err(CommsError::NothingReceived)
    }
}
