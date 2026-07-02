//! Firmware Over The Air (FOTA) over the event-driven RF stack.
//!
//! # Architecture
//!
//! FOTA drives chunked firmware transfers through `RfStackAO` using the
//! reliable transport path (`RF_TX_REQ_SIG { reliable: true }`).
//!
//! ```text
//! FotaDriver (this module)
//!   │  RF_TX_REQ_SIG (reliable=true) → RfStackAO
//!   ▼
//! RfStackAO  (ReliableTransport → Network → MAC → PHY)
//!   │  RF_TX_DONE_SIG / RF_TX_FAIL_SIG → app AO
//! ```
//!
//! # Protocol Sketch (server → device)
//!
//! | Phase       | Packet type | Direction      |
//! |-------------|-------------|----------------|
//! | Announce    | 0x00        | server → bcast |
//! | Chunk       | 0x01        | server → device|
//! | ACK request | 0x02        | server → device|
//! | ACK         | 0x10        | device → server|
//! | Verify      | 0x03        | server → device|
//! | Done        | 0x11        | device → server|
//!
//! # FOTA packet layout
//!
//! ```text
//! [type:u8][chunk_index:u32le][total_chunks:u32le][data:0..N]
//! ```
//!
//! The receiving device accumulates chunks in flash, verifies the CRC-32
//! of the complete image, then reboots into the bootloader.

extern crate alloc;
use alloc::sync::Arc;
use alloc::vec::Vec;

use qf::active::ActiveObjectRef;
use qf::event::DynEvent;

use crate::error::CommsError;
use crate::events::{RfTxReqPayload, RF_TX_REQ_SIG};

const PKT_ANNOUNCE: u8 = 0x00;
const PKT_CHUNK:    u8 = 0x01;
const PKT_VERIFY:   u8 = 0x03;

/// Maximum application data bytes per FOTA RF packet.
/// Conservative limit that fits within a LoRaWAN SF7/BW125 payload.
pub const FOTA_CHUNK_BYTES: usize = 200;

// ─────────────────────────────────────────────────────────────────────────────
// FotaDriver — event-driven chunked transfer over RfStackAO
// ─────────────────────────────────────────────────────────────────────────────

/// Drives a chunked FOTA transfer through an `RfStackAO` via reliable events.
///
/// ## Usage
///
/// 1. Create a `FotaDriver` from the firmware image bytes and a reference to
///    the RF AO.
/// 2. Call `start_announce()` once to broadcast firmware availability.
/// 3. On each `RF_TX_DONE_SIG`, call `next_chunk()` — it posts the next
///    `RF_TX_REQ_SIG` (reliable) and returns `FotaStatus::Sending`.
/// 4. When `next_chunk()` returns `FotaStatus::Done`, call `send_verify()`.
/// 5. On `RF_TX_FAIL_SIG`, the transfer has failed — check `is_failed()`.
pub struct FotaDriver {
    rf_ao:        ActiveObjectRef,
    image:        Vec<u8>,
    total_chunks: u32,
    next:         u32,
    failed:       bool,
}

/// Status returned by `FotaDriver::next_chunk`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FotaStatus {
    /// A chunk was posted; wait for `RF_TX_DONE_SIG` before calling again.
    Sending,
    /// All chunks posted and the verify packet was sent.
    Done,
    /// Transfer previously failed (call after `RF_TX_FAIL_SIG`).
    Failed,
}

impl FotaDriver {
    /// Create a new FOTA driver for `image` bytes, routing through `rf_ao`.
    pub fn new(rf_ao: ActiveObjectRef, image: Vec<u8>) -> Self {
        let total_chunks = (image.len() as u32).div_ceil(FOTA_CHUNK_BYTES as u32);
        Self { rf_ao, image, total_chunks, next: 0, failed: false }
    }

    /// Returns the total number of chunks the image is split into.
    pub fn total_chunks(&self) -> u32 { self.total_chunks }

    /// Returns the zero-based index of the next chunk to be sent.
    pub fn next_chunk_index(&self) -> u32 { self.next }

    /// Returns `true` if `RF_TX_FAIL_SIG` was received and the session aborted.
    pub fn is_failed(&self) -> bool { self.failed }

    /// Broadcast a FOTA availability announcement (unreliable — best-effort).
    pub fn start_announce(&self, fw_version: u32) -> Result<(), CommsError> {
        let mut pkt = [0u8; 5];
        pkt[0] = PKT_ANNOUNCE;
        pkt[1..5].copy_from_slice(&fw_version.to_le_bytes());
        self.post_tx(&pkt, false)
    }

    /// Call on every `RF_TX_DONE_SIG` to send the next chunk reliably.
    ///
    /// Returns `FotaStatus::Sending` if another chunk was posted,
    /// `FotaStatus::Done` after the final verify packet is sent,
    /// or `FotaStatus::Failed` if the session had already failed.
    pub fn on_tx_done(&mut self) -> FotaStatus {
        if self.failed { return FotaStatus::Failed; }

        if self.next >= self.total_chunks {
            // All chunks sent — send verify
            let crc = self.compute_crc32();
            let _ = self.send_verify(self.image.len() as u32, crc);
            return FotaStatus::Done;
        }

        let start = self.next as usize * FOTA_CHUNK_BYTES;
        let end   = (start + FOTA_CHUNK_BYTES).min(self.image.len());
        let chunk = &self.image[start..end];

        let n = chunk.len();
        let mut pkt = [0u8; 9 + FOTA_CHUNK_BYTES];
        pkt[0] = PKT_CHUNK;
        pkt[1..5].copy_from_slice(&self.next.to_le_bytes());
        pkt[5..9].copy_from_slice(&self.total_chunks.to_le_bytes());
        pkt[9..9 + n].copy_from_slice(chunk);

        if self.post_tx(&pkt[..9 + n], true).is_err() {
            self.failed = true;
            return FotaStatus::Failed;
        }

        self.next += 1;
        FotaStatus::Sending
    }

    /// Call on `RF_TX_FAIL_SIG` to mark the session as failed.
    pub fn on_tx_fail(&mut self) {
        self.failed = true;
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    fn post_tx(&self, payload: &[u8], reliable: bool) -> Result<(), CommsError> {
        let data = payload.to_vec();
        let ev = DynEvent::with_arc(
            RF_TX_REQ_SIG,
            Arc::new(RfTxReqPayload::with_reliability(data, 2, reliable)),
        );
        self.rf_ao.post(ev);
        Ok(())
    }

    fn send_verify(&self, image_size: u32, crc32: u32) -> Result<(), CommsError> {
        let mut pkt = [0u8; 9];
        pkt[0] = PKT_VERIFY;
        pkt[1..5].copy_from_slice(&image_size.to_le_bytes());
        pkt[5..9].copy_from_slice(&crc32.to_le_bytes());
        self.post_tx(&pkt, true)
    }

    fn compute_crc32(&self) -> u32 {
        // Simple CRC-32 (IEEE 802.3 polynomial).
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in &self.image {
            crc ^= byte as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }
}
