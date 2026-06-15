//! Firmware Over The Air (FOTA) over an [`Rf`] transport.
//!
//! # Architecture
//!
//! FOTA builds a simple chunked-transfer protocol on top of the generic
//! [`Rf::send`] / [`Rf::receive`] interface, so the same FOTA logic runs
//! regardless of whether the underlying radio is LoRa, BLE, or Wi-Fi.
//!
//! ```text
//! FotaSession
//!   │  send_chunk()  /  await_ack()
//!   ▼
//! Rf  (LoRaRf / NullRf / …)
//!   │  send(pkt) / receive(buf)
//!   ▼
//! RfDriver (SX1276 / SX1262 / …)
//! ```
//!
//! # Protocol Sketch (server → device)
//!
//! | Phase       | Packet type | Direction     |
//! |-------------|-------------|---------------|
//! | Announce    | 0x00        | server → bcast|
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

use crate::error::{CommsError, FotaError};
use crate::rf::Rf;

const PKT_ANNOUNCE: u8 = 0x00;
const PKT_CHUNK:    u8 = 0x01;
const PKT_VERIFY:   u8 = 0x03;

/// Maximum application data bytes per FOTA RF packet.
/// Conservative limit that fits within a LoRaWAN SF7/BW125 payload.
pub const FOTA_CHUNK_BYTES: usize = 200;

/// Server-side FOTA session.
///
/// Call [`FotaSession::send_chunk`] repeatedly from a QP-RS active object
/// (or a loop in host tests) to transfer a firmware image.
pub struct FotaSession<R: Rf> {
    rf:           R,
    image_size:   u32,
    total_chunks: u32,
}

impl<R: Rf> FotaSession<R> {
    /// Creates a FOTA session for an image of `image_size` bytes, computing the
    /// number of fixed-size chunks needed to transfer it.
    pub fn new(rf: R, image_size: u32) -> Self {
        let total_chunks =
            (image_size + FOTA_CHUNK_BYTES as u32 - 1) / FOTA_CHUNK_BYTES as u32;
        Self { rf, image_size, total_chunks }
    }

    /// Returns the total number of chunks the image is split into.
    pub fn total_chunks(&self) -> u32 { self.total_chunks }

    /// Broadcast a FOTA availability announcement.
    pub fn announce(&mut self, fw_version: u32) -> Result<(), CommsError> {
        let mut pkt = [0u8; 5];
        pkt[0] = PKT_ANNOUNCE;
        pkt[1..5].copy_from_slice(&fw_version.to_le_bytes());
        self.rf.send(&pkt)
    }

    /// Send one chunk of the firmware image.
    ///
    /// `chunk_index` is zero-based; `data` must be ≤ [`FOTA_CHUNK_BYTES`].
    pub fn send_chunk(&mut self, chunk_index: u32, data: &[u8])
        -> Result<(), CommsError>
    {
        if chunk_index >= self.total_chunks {
            return Err(CommsError::Fota(FotaError::ChunkOutOfRange));
        }
        let n = data.len().min(FOTA_CHUNK_BYTES);
        let mut pkt = [0u8; 9 + FOTA_CHUNK_BYTES];
        pkt[0] = PKT_CHUNK;
        pkt[1..5].copy_from_slice(&chunk_index.to_le_bytes());
        pkt[5..9].copy_from_slice(&self.total_chunks.to_le_bytes());
        pkt[9..9 + n].copy_from_slice(&data[..n]);
        self.rf.send(&pkt[..9 + n])
    }

    /// Send an image-complete verification packet carrying the CRC-32.
    pub fn send_verify(&mut self, image_crc32: u32) -> Result<(), CommsError> {
        let mut pkt = [0u8; 9];
        pkt[0] = PKT_VERIFY;
        pkt[1..5].copy_from_slice(&self.image_size.to_le_bytes());
        pkt[5..9].copy_from_slice(&image_crc32.to_le_bytes());
        self.rf.send(&pkt)
    }

    /// Access the underlying RF transport.
    pub fn rf_mut(&mut self) -> &mut R { &mut self.rf }
}
