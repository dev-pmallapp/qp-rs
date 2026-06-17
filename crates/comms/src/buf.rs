//! Buffer management module for the RF protocol stack.
//!
//! Provides `Frame` (a DMA-aligned contiguous buffer with headroom)
//! and `FramePool` (static allocation pool for no_std execution).

use crate::error::CommsError;

/// Total frame buffer size. LoRaWAN PHYPayload ≤ 256 bytes.
pub const MAX_FRAME: usize = 256;

/// Headroom reserved for layer headers (TX path, prepended downward).
///
/// Budget:
///   Transport header : 5 bytes  (SEQ, ACK, FLAGS, LEN×2)
///   Network header   : 0 bytes  (LoRa encodes address in MAC)
///   MAC header       : 9 bytes  (MHDR + DevAddr + FCtrl + FCnt + FPort)
///   MAC trailer      : 4 bytes  (MIC appended after encryption)
///   Spare            : 14 bytes (for future net header / options)
/// Total              : 32 bytes
pub const FRAME_HEADROOM: usize = 32;

/// DMA-aligned frame buffer — one per in-flight RF frame.
///
/// Inspired by LwIP's `pbuf`: a single contiguous allocation carries the
/// raw bytes for the entire frame lifetime.
#[derive(Clone)]
#[repr(C, align(4))]
pub struct Frame {
    buf:   [u8; MAX_FRAME],
    start: u8,
    end:   u8,
}

impl Frame {
    /// New TX frame: payload region starts at `FRAME_HEADROOM`.
    pub const fn new() -> Self {
        Self { buf: [0; MAX_FRAME], start: FRAME_HEADROOM as u8, end: FRAME_HEADROOM as u8 }
    }

    /// Write application payload (TX). Overwrites any previous payload.
    pub fn write_payload(&mut self, data: &[u8]) -> Result<(), CommsError> {
        if data.len() > MAX_FRAME - FRAME_HEADROOM {
            return Err(CommsError::BufferTooSmall);
        }
        let s = FRAME_HEADROOM;
        let e = s + data.len();
        self.buf[s..e].copy_from_slice(data);
        self.start = s as u8;
        self.end   = e as u8;
        Ok(())
    }

    /// Prepend `n` header bytes below current `start` (TX, layer going down).
    ///
    /// Returns a mutable slice the layer should fill with its header bytes.
    /// Fails if `n` bytes of headroom are not available.
    pub fn prepend_header(&mut self, n: usize) -> Result<&mut [u8], CommsError> {
        if (self.start as usize) < n {
            return Err(CommsError::BufferTooSmall);
        }
        self.start -= n as u8;
        Ok(&mut self.buf[self.start as usize..self.start as usize + n])
    }

    /// Append `n` trailer bytes after current `end` (TX, e.g. MIC at MAC layer).
    pub fn append_trailer(&mut self, trailer: &[u8]) -> Result<(), CommsError> {
        let n = trailer.len();
        if self.end as usize + n > MAX_FRAME {
            return Err(CommsError::BufferTooSmall);
        }
        let e = self.end as usize;
        self.buf[e..e + n].copy_from_slice(trailer);
        self.end += n as u8;
        Ok(())
    }

    /// Strip and return `n` header bytes from current `start` (RX, layer going up).
    ///
    /// The returned slice is valid until the next mutation. Copy if needed.
    pub fn strip_header(&mut self, n: usize) -> Result<&[u8], CommsError> {
        if self.len() < n {
            return Err(CommsError::MacError);
        }
        let s = self.start as usize;
        self.start += n as u8;
        Ok(&self.buf[s..s + n])
    }

    /// Trim `n` trailer bytes from the end (RX, e.g. strip MIC after verify).
    pub fn trim_trailer(&mut self, n: usize) -> Result<&[u8], CommsError> {
        if self.len() < n {
            return Err(CommsError::MacError);
        }
        self.end -= n as u8;
        Ok(&self.buf[self.end as usize..self.end as usize + n])
    }

    /// Current payload slice `buf[start..end]`.
    pub fn payload(&self)     -> &[u8]     { &self.buf[self.start as usize..self.end as usize] }
    pub fn payload_mut(&mut self) -> &mut [u8] { &mut self.buf[self.start as usize..self.end as usize] }
    pub fn len(&self)         -> usize      { (self.end - self.start) as usize }
    pub fn is_empty(&self)    -> bool       { self.start == self.end }

    // ─── PHY interface ───────────────────────────────────────────────────────

    /// Slice passed to the PHY for TX DMA: `buf[start..end]`.
    pub fn phy_bytes(&self) -> &[u8] { self.payload() }

    /// Full backing buffer for PHY RX DMA write: `buf[0..MAX_FRAME]`.
    ///
    /// # Safety invariant
    /// Caller (PHY layer) must call `set_received_len` before any layer reads
    /// `start`/`end`, or the frame contents are undefined.
    pub fn raw_buf_for_dma(&mut self) -> &mut [u8] { &mut self.buf }

    /// After PHY RX DMA completes, set the valid byte range.
    pub fn set_received_len(&mut self, n: usize) {
        self.start = 0;
        self.end   = n.min(MAX_FRAME) as u8;
    }
}

/// Number of frames kept in the static pool.
pub const FRAME_POOL_SIZE: usize = 8;

/// Index into the frame pool (u8 is enough for ≤255 frames).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameIdx(u8);

pub struct FramePool {
    frames:    [Frame; FRAME_POOL_SIZE],
    free_mask: core::sync::atomic::AtomicU8,  // bit N = frame N is free
}

impl FramePool {
    pub const fn new() -> Self {
        Self {
            frames:    [const { Frame::new() }; FRAME_POOL_SIZE],
            free_mask: core::sync::atomic::AtomicU8::new(0xFF),
        }
    }

    /// Allocate one frame from the pool. O(1) via count-trailing-zeros.
    /// Returns `None` when the pool is exhausted.
    pub fn alloc(&self) -> Option<FrameIdx> {
        use core::sync::atomic::Ordering::{AcqRel, Acquire};
        loop {
            let mask = self.free_mask.load(Acquire);
            if mask == 0 { return None; }
            let bit = mask.trailing_zeros() as u8;
            let new = mask & !(1 << bit);
            if self.free_mask.compare_exchange(mask, new, AcqRel, Acquire).is_ok() {
                return Some(FrameIdx(bit));
            }
        }
    }

    pub fn free(&self, idx: FrameIdx) {
        use core::sync::atomic::Ordering::Release;
        self.free_mask.fetch_or(1 << idx.0, Release);
    }

    /// # Safety
    /// Caller must own the index (it must not be in the free mask).
    pub unsafe fn get(&self, idx: FrameIdx) -> &Frame {
        &self.frames[idx.0 as usize]
    }
    pub unsafe fn get_mut(&mut self, idx: FrameIdx) -> &mut Frame {
        &mut self.frames[idx.0 as usize]
    }
}
