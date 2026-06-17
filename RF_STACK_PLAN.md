# RF Protocol-Stack — Detailed Implementation Plan

**Date**: 2026-06-15
**Status**: Design / proposal (not yet implemented)
**Scope**: Evolve the `comms` crate from a LoRa-centric middleware into a layered,
radio-agnostic protocol stack inspired by LwIP's buffer management and embedded TCP/IP
architecture, integrated with the QP-RS active-object model and Cortex-M / CMSIS
runtime constraints.

---

## 1. Why

`comms` is already a *partial* stack. It has the two hardest pieces:

- a **transport-agnostic interface** — `Rf::send` / `Rf::receive` (`crates/comms/src/rf.rs`)
- a **PHY driver abstraction** — `hal::lora::RfDriver` with SX1276/SX1262 impls
  (`hal/hal-esp/src/sx1276.rs`, `sx1262.rs`)

What stops it from being a real stack:

1. **`LoRaRf` collapses three layers into one** (`crates/comms/src/lora.rs`): MAC
   framing, AES-CMAC MIC (`mac.rs`), session/addressing (`session.rs`), and the PHY
   call are all in one struct. That is like baking TCP into the Ethernet driver.
2. **FOTA is an ad-hoc protocol bolted directly onto `Rf`** (`crates/comms/src/fota.rs`)
   — its chunking, ACK, and CRC logic is not reusable by other applications.
3. **No common framing/dispatch path** and **no stack composition type**, so a new
   radio cannot reuse the reliability/addressing logic.
4. **`RfDriver` is TX-only and synchronous** — no `receive()`, no DIO/IRQ events, no
   RSSI/SNR; a real stack needs an event-driven RX path integrated with the QK ISR
   bridge.

---

## 2. Current architecture

```
App / FOTA
  │  Rf::send(payload) / Rf::receive(buf)
  ▼
Rf trait                         (crates/comms/src/rf.rs)
  ├── LoRaRf   ── LoRaWAN frame + session + MIC, all in one
  │     │  RfDriver::transmit()
  │     └── hal::lora::RfDriver  (hal/src/lora.rs) ── Sx1276 / Sx1262
  └── NullRf   (host test stub)
```

---

## 3. Target architecture

Each layer is a trait that transforms a `Frame` payload in-place (egress: prepend
header; ingress: strip and validate header). A single `RfStack<T,N,M,P>` type composes
them at compile time — zero dynamic dispatch, zero heap allocation.

```
┌────────────────────────────────────────────┐
│ Application AOs  (FOTA, telemetry, RPC)     │  reusable across radios
├────────────────────────────────────────────┤
│ Transport  reliability, ACK, fragmentation  │  reusable across radios
│            CRC-32, sequence numbers,        │
│            retransmit timer (QF TimeEvent)  │
├────────────────────────────────────────────┤
│ Network    addressing / routing (DevAddr)   │  reusable across radios
│            port-based dispatch table        │
├────────────────────────────────────────────┤
│ MAC/Security  framing, MIC (AES-CMAC),      │  per-radio family
│               frame counters, encryption    │  (LoRaWAN, BLE L2CAP, …)
├────────────────────────────────────────────┤
│ PHY  trait RfPhy  (modulation, IRQ, RSSI)   │  per-chip (SX127x/SX126x/…)
└────────────────────────────────────────────┘
        ▲  driven by one RfStackAO (QF active object)
        RX events flow up via post_from_isr → DIO ISR bridge
        TX commands flow down via stack.transmit()
```

---

## 4. Mapping to TCP/IP (and LwIP)

| LwIP concept      | RF-stack equivalent            | Today (comms)         | Target file                    |
|-------------------|--------------------------------|-----------------------|--------------------------------|
| `pbuf` chain      | `Frame` with headroom cursor   | `[u8; 256]` ad-hoc   | `comms/src/buf.rs`             |
| `memp` pool       | `FramePool` static array       | heap / stack          | `comms/src/buf.rs`             |
| `netif`           | `RfPhy` + IRQ bridge           | `RfDriver` (TX only)  | `hal/src/rf.rs`, port ISR      |
| TCP / UDP         | `ReliableTransport` / `Dgram`  | inside `fota.rs`      | `comms/src/transport.rs`       |
| IP addressing     | `Network` + `SessionContext`   | inside `LoRaSession`  | `comms/src/net.rs`             |
| ARP / Ethernet    | `LoRaWanMac` / `BleL2cap`      | `LoRaRf` + `mac.rs`   | `comms/src/mac/lorawan.rs`     |
| `err_t` / `pbuf_free` | `CommsError` / `Frame::drop` | `CommsError`        | `comms/src/error.rs`           |

LwIP's critical embedded lessons applied here:

- **Headroom allocation**: allocate one contiguous buffer per frame; each layer
  *prepends* its header into reserved headroom (TX) or *strips* it (RX). No copies
  between layers for a max-size LoRaWAN frame (≤256 bytes).
- **Static pools**: `FramePool` is a compile-time-sized array with a free bitmask.
  No `alloc`; no fragmentation. Pool size = peak in-flight frames, not worst-case.
- **Event-driven RX**: LwIP's `netif->input()` callback maps to `post_from_isr()`
  from the DIO interrupt handler. The AO model eliminates the need for a separate
  "tcpip_thread".
- **Single-writer per layer**: each layer is owned by `RfStackAO`; no concurrent
  access, no locking inside the data path.

---

## 5. Buffer management (`comms/src/buf.rs`)

### 5.1 `Frame` — the common packet buffer

```rust
/// Total frame buffer size.  LoRaWAN PHYPayload ≤ 256 bytes.
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
/// raw bytes for the entire frame lifetime.  Rather than a linked list of
/// pbufs (LwIP), we use a single flat buffer because RF frames are short
/// (≤256 bytes) and scatter-gather adds no value at this scale.
///
/// TX path (going down through layers):
///   1. App writes payload starting at byte `FRAME_HEADROOM`.
///   2. Each layer calls `prepend_header(n)` to claim `n` bytes below the
///      current `start`, writes its header, and returns.
///   3. PHY reads `phy_bytes()` = `buf[start..end]` for the SPI DMA transfer.
///
/// RX path (going up through layers):
///   1. PHY DMA writes into `raw_buf_for_dma()` = `buf[0..]`.
///   2. PHY calls `set_received_len(n)` → start=0, end=n.
///   3. Each layer calls `strip_header(n)` to read and advance past `n` bytes.
///   4. App reads `payload()` = `buf[start..end]`.
///
/// The `align(4)` attribute satisfies the Cortex-M DMA requirement that
/// source/destination addresses be 32-bit-aligned.
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

    /// Write application payload (TX).  Overwrites any previous payload.
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
    /// The returned slice is valid until the next mutation.  Copy if needed.
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
```

### 5.2 `FramePool` — static allocation (no_std)

```rust
/// Number of frames kept in the static pool.
///
/// Sizing guidance (LoRaWAN Class A ABP):
///   1 frame being transmitted
///   1 frame buffered for retransmit
///   1 frame being received
///   1 frame queued for the application AO
/// = 4 minimum; 8 for safety margin.
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

    /// Allocate one frame from the pool.  O(1) via count-trailing-zeros.
    /// Returns `None` when the pool is exhausted.
    pub fn alloc(&self) -> Option<FrameIdx> {
        use core::sync::atomic::Ordering::AcqRel;
        loop {
            let mask = self.free_mask.load(core::sync::atomic::Ordering::Acquire);
            if mask == 0 { return None; }
            let bit = mask.trailing_zeros() as u8;
            let new = mask & !(1 << bit);
            if self.free_mask.compare_exchange(mask, new, AcqRel, AcqRel).is_ok() {
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

/// Declare a global pool in a `no_std` binary:
///
/// ```rust
/// static FRAME_POOL: FramePool = FramePool::new();
/// ```
```

> **LwIP analogy**: `FramePool` is `MEMP_NUM_PBUF` configured in `lwipopts.h`.
> `FrameIdx` plays the role of `pbuf *`; the flat backing array replaces the heap.
> Crucially, a `FrameIdx` is `Copy` so it can be carried in a QF `DynEvent`
> payload without `Arc`/`Box`.

---

## 6. PHY layer (`hal/src/rf.rs`)

Replaces and extends `hal::lora::RfDriver`.  `RfDriver` is kept for
backward-compatibility as a blanket-impl wrapper.

### 6.1 Radio mode

```rust
/// Requested radio operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioMode {
    Sleep,                       // lowest power; requires re-init on wake
    Standby,                     // fast wake; SPI accessible
    Rx { timeout_ms: Option<u32> }, // None = continuous (CAD or always-on)
    Tx,                          // set by PHY during transmit(); auto-clears
    Cad,                         // channel-activity detection (LoRa only)
}
```

### 6.2 RX metadata

```rust
/// Metadata captured by the radio at RX-done time.
///
/// Populated by the PHY from the chip's status registers immediately after the
/// DIO interrupt fires — values become stale after any subsequent SPI access.
#[derive(Debug, Clone, Copy, Default)]
pub struct RxMetadata {
    /// Received signal strength (dBm), e.g. from RegRssiValue / GetRssiInst.
    pub rssi_dbm:    i16,
    /// Signal-to-noise ratio (tenths of dB, LoRa only).
    pub snr_db_x10:  i16,
    /// On-chip RX timestamp in timer ticks (0 if not available).
    pub timestamp:   u32,
    /// Raw packet length in bytes as reported by the radio.
    pub pkt_len:     u8,
}
```

### 6.3 IRQ event enum

```rust
/// Asynchronous events the radio signals via DIO lines.
///
/// The PHY populates a `PhyEvent` by reading the chip's IRQ status register
/// inside the ISR (SX1276: RegIrqFlags; SX1262: GetIrqStatus) and passing it
/// upward via `post_from_isr`.
#[derive(Debug, Clone, Copy)]
pub enum PhyEvent {
    TxDone,
    RxDone(RxMetadata),
    RxTimeout,
    CrcError,
    CadDone { channel_active: bool },
    PreambleDetected,
}
```

### 6.4 RF configuration types

```rust
/// Radio-agnostic TX configuration passed through the stack.
#[derive(Debug, Clone)]
pub struct RfTxConfig {
    pub frequency_hz:  u32,
    pub tx_power_dbm:  i8,
    pub params:        RadioParams,
}

/// Radio-agnostic RX configuration.
#[derive(Debug, Clone)]
pub struct RfRxConfig {
    pub frequency_hz:  u32,
    pub timeout_ms:    Option<u32>,
    pub params:        RadioParams,
}

/// Modulation parameters — tagged union; new radios add a variant.
#[derive(Debug, Clone)]
pub enum RadioParams {
    LoRa(LoRaModulation),
    Fsk(FskModulation),
}

/// FSK modulation parameters (future).
#[derive(Debug, Clone)]
pub struct FskModulation {
    pub bitrate_bps:   u32,
    pub deviation_hz:  u32,
    pub rx_bandwidth:  u32,
}
```

### 6.5 `RfPhy` trait

```rust
/// Generic physical-layer radio trait.
///
/// Implemented by each chip driver (SX1276, SX1262, LoopbackPhy, …).
/// The trait is deliberately minimal: it handles one frame at a time in
/// a single-threaded ISR-safe model aligned with QK.
///
/// **Threading model**: only `RfStackAO` calls these methods; no concurrent
/// access.  ISR interaction happens exclusively through `PhyEvent` posted via
/// `post_from_isr` — the PHY itself does *not* call QF from inside the trait.
pub trait RfPhy: Send {
    /// One-time hardware initialisation (GPIO config, SPI open, register reset).
    fn init(&mut self) -> HalResult<()>;

    /// Set the radio operating mode.
    ///
    /// On SX1276: writes RegOpMode.  On SX1262: calls SetRx / SetTx / SetSleep.
    fn set_mode(&mut self, mode: RadioMode) -> HalResult<()>;

    /// Configure radio parameters without changing operating mode.
    ///
    /// Called before each TX or RX to apply frequency, SF, BW, power, etc.
    fn configure_tx(&mut self, cfg: &RfTxConfig) -> HalResult<()>;
    fn configure_rx(&mut self, cfg: &RfRxConfig) -> HalResult<()>;

    /// Queue `frame.phy_bytes()` for transmission.
    ///
    /// Precondition: radio is in Standby mode.
    /// Postcondition: radio transitions to Tx mode; DIO fires `TxDone` on
    /// completion.  Does *not* block for air-time.
    fn transmit(&mut self, frame: &Frame) -> HalResult<()>;

    /// Copy received bytes into `frame` after a `RxDone` event.
    ///
    /// Reads from the radio's FIFO or DMA buffer.  Frame must be freshly
    /// allocated; caller sets `frame.set_received_len(meta.pkt_len)`.
    fn read_rx(&mut self, frame: &mut Frame, meta: &RxMetadata) -> HalResult<()>;

    /// Poll IRQ status register (non-ISR, fallback for hosts without GPIO IRQ).
    fn poll_irq(&mut self) -> HalResult<Option<PhyEvent>>;

    /// Clear all IRQ flags after handling.
    fn clear_irq(&mut self) -> HalResult<()>;

    /// Instantaneous RSSI of the current channel (dBm, useful for CAD).
    fn rssi(&self) -> HalResult<i16>;

    /// Human-readable chip identifier, e.g. `"SX1262"`.
    fn chip_name(&self) -> &'static str;
}
```

### 6.6 SX1276 / SX1262 implementation notes

**SX1276 DIO mapping** (LoRa mode):
| DIO | Event          | IRQ flag bit |
|-----|----------------|--------------|
| DIO0 | RxDone / TxDone | bit 6 / bit 3 |
| DIO1 | RxTimeout      | bit 7         |
| DIO3 | CadDone        | bit 2         |

The ISR must read `RegIrqFlags` (0x12) immediately — it is cleared by writing
the same bits back to `RegIrqFlags` (0x12).  Read before clear to avoid race:
```rust
let flags = spi.read_reg(REG_IRQ_FLAGS)?;
spi.write_reg(REG_IRQ_FLAGS, flags)?;  // clear
```

**SX1262 DIO mapping**: single DIO1 line serves all events; event type is
determined by reading `GetIrqStatus()` (opcode 0x12).  Clear with
`ClearIrqStatus(0x03FF)`.

**DMA for SPI** (Cortex-M, e.g. ESP32-C6 / STM32):
- The SPI transfer buffer must be in DMA-accessible SRAM (not flash).
- `Frame::raw_buf_for_dma()` returns `&mut self.buf` which is stack/static
  SRAM — always accessible.
- After an RX DMA transfer, the CPU must invalidate its cache lines for the
  buffer range if the MCU has a data cache (Cortex-M7, M55).  On Cortex-M0/M3/M4
  (no data cache) this is unnecessary.
- `#[repr(C, align(4))]` on `Frame` ensures 32-bit DMA alignment on all
  Cortex-M variants.

---

## 7. `Layer` trait and stack composition (`comms/src/stack.rs`)

### 7.1 Core trait

```rust
/// Protocol layer.  Layers are chained inside `RfStack`; data flows in-place
/// through a shared `Frame` buffer (inspired by LwIP's `pbuf` passing model).
///
/// TX (egress):  call `down(&mut frame)` — layer prepends its header and
///               optionally appends a trailer.
/// RX (ingress): call `up(&mut frame)` — layer validates, strips its header,
///               and returns `Ok(false)` to silently drop invalid frames.
pub trait Layer: Send {
    /// Egress: encapsulate this layer's header/trailer around the payload.
    fn down(&mut self, frame: &mut Frame) -> Result<(), CommsError>;

    /// Ingress: validate and strip this layer's header/trailer.
    ///
    /// Returns `Ok(false)` to drop the frame (e.g. bad MIC, wrong DevAddr).
    fn up(&mut self, frame: &mut Frame) -> Result<bool, CommsError>;
}
```

### 7.2 `RfStack` composition type

```rust
/// Zero-cost composition of Transport / Network / MAC / PHY layers.
///
/// Monomorphises at compile time — no vtable, no per-packet allocation.
///
/// Type alias examples:
/// ```rust
/// type LoRaStack<SPI> =
///     RfStack<ReliableTransport, Network, LoRaWanMac, Sx1262Phy<SPI>>;
///
/// type LoopbackStack =
///     RfStack<UnreliableTransport, NoopNetwork, NoopMac, LoopbackPhy>;
/// ```
pub struct RfStack<T, N, M, P>
where
    T: Layer,
    N: Layer,
    M: Layer,
    P: RfPhy,
{
    pub transport: T,
    pub network:   N,
    pub mac:       M,
    pub phy:       P,
}

impl<T: Layer, N: Layer, M: Layer, P: RfPhy> RfStack<T, N, M, P> {
    pub fn new(transport: T, network: N, mac: M, phy: P) -> Self {
        Self { transport, network, mac, phy }
    }

    /// TX path: payload → transport header → net header → MAC frame → PHY air.
    pub fn transmit(
        &mut self,
        payload: &[u8],
        tx_cfg:  &RfTxConfig,
    ) -> Result<(), CommsError> {
        let mut frame = Frame::new();
        frame.write_payload(payload)?;
        self.transport.down(&mut frame)?;
        self.network.down(&mut frame)?;
        self.mac.down(&mut frame)?;
        self.phy.configure_tx(tx_cfg).map_err(CommsError::from)?;
        self.phy.transmit(&frame).map_err(CommsError::from)
    }

    /// RX path: raw bytes → MAC parse → net dispatch → transport reorder → payload.
    ///
    /// Called by `RfStackAO` after a `RxDone` PHY event with `meta.pkt_len` set.
    pub fn receive_raw(
        &mut self,
        raw_frame: &mut Frame,  // PHY has already written DMA bytes + set_received_len
    ) -> Result<Option<Frame>, CommsError> {
        if !self.mac.up(raw_frame)?         { return Ok(None); }
        if !self.network.up(raw_frame)?     { return Ok(None); }
        if !self.transport.up(raw_frame)?   { return Ok(None); }
        let mut out = Frame::new();
        out.write_payload(raw_frame.payload())?;
        Ok(Some(out))
    }
}
```

---

## 8. MAC layer (`comms/src/mac/lorawan.rs`)

Extract the LoRaWAN frame builder/parser from `lora.rs` into a standalone `Layer`.

### 8.1 Context and construction

```rust
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
}
```

### 8.2 TX path (`down`)

```
LoRaWAN PHYPayload layout:
  MHDR(1) | DevAddr(4LE) | FCtrl(1) | FCnt(2LE) | FPort(1) | FRMPayload | MIC(4)
  ─────────────────────────────────────── 9 bytes header ──────────────────── 4 bytes trailer
```

```rust
impl Layer for LoRaWanMac {
    fn down(&mut self, frame: &mut Frame) -> Result<(), CommsError> {
        // 1. Encrypt FRMPayload in-place (AES-128 CTR, AppSKey)
        encrypt_frm_payload(frame.payload_mut(), &self.app_skey,
                            &self.dev_addr, self.fcnt_up, 0 /* uplink */)?;

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
        let mic = compute_mic(frame.payload(), &self.nwk_skey,
                              &self.dev_addr, self.fcnt_up, 0)?;
        frame.append_trailer(&mic)?;

        self.fcnt_up = self.fcnt_up.wrapping_add(1);
        Ok(())
    }

    fn up(&mut self, frame: &mut Frame) -> Result<bool, CommsError> {
        if frame.len() < 13 { return Ok(false); }   // minimum LoRaWAN downlink

        // 1. Strip and parse 9-byte MAC header
        let hdr = {
            let raw = frame.strip_header(9)?;
            let mut h = [0u8; 9];
            h.copy_from_slice(raw);
            h
        };
        let mhdr     = hdr[0];
        let dev_addr = [hdr[1], hdr[2], hdr[3], hdr[4]];
        let fcnt     = u16::from_le_bytes([hdr[6], hdr[7]]) as u32;

        // 2. Validate DevAddr
        if dev_addr != self.dev_addr { return Ok(false); }

        // 3. Strip MIC (last 4 bytes) and verify
        let mic_recv = {
            let raw = frame.trim_trailer(4)?;
            let mut m = [0u8; 4];
            m.copy_from_slice(raw);
            m
        };
        let mic_calc = compute_mic(frame.payload(), &self.nwk_skey,
                                   &self.dev_addr, fcnt, 1 /* downlink */)?;
        if mic_recv != mic_calc { return Ok(false); }

        // 4. Decrypt FRMPayload in-place
        let fport_byte = frame.strip_header(1)?[0];
        let _ = mhdr; let _ = fport_byte;  // used for future dispatch
        encrypt_frm_payload(frame.payload_mut(), &self.app_skey,
                            &self.dev_addr, fcnt, 1)?;

        self.fcnt_dn = fcnt.wrapping_add(1);
        Ok(true)
    }
}
```

> **Private helpers** `encrypt_frm_payload` and `compute_mic` are extracted
> verbatim from `LoRaRf::build_frame` in `lora.rs:88-156` — same AES-128-CTR
> and AES-128-CMAC logic, now testable in isolation.

---

## 9. Network / session layer (`comms/src/net.rs`)

For LoRaWAN ABP the "network" layer is thin — addressing is embedded in the MAC
header.  It still earns its own struct because:

1. It holds the **port dispatch table** (LoRaWAN FPort → QF signal).
2. A future OTAA / roaming path needs session renegotiation here.
3. It is the insertion point for multi-hop routing in mesh variants.

```rust
/// Maximum port → signal bindings in the dispatch table.
const MAX_PORT_BINDINGS: usize = 8;

/// Maps a LoRaWAN FPort (or generic "service identifier") to a QF signal.
pub struct PortBinding {
    pub port:   u8,
    pub signal: Signal,
}

pub struct Network {
    bindings: [Option<PortBinding>; MAX_PORT_BINDINGS],
}

impl Network {
    pub const fn new() -> Self {
        Self { bindings: [const { None }; MAX_PORT_BINDINGS] }
    }

    /// Register a port → signal mapping.  Returns `Err` if the table is full.
    pub fn bind(&mut self, port: u8, signal: Signal) -> Result<(), CommsError> {
        for slot in &mut self.bindings {
            if slot.is_none() {
                *slot = Some(PortBinding { port, signal });
                return Ok(());
            }
        }
        Err(CommsError::TableFull)
    }

    /// Resolve port to signal for application dispatch.
    pub fn resolve(&self, port: u8) -> Option<Signal> {
        self.bindings.iter()
            .find_map(|b| b.as_ref().filter(|b| b.port == port).map(|b| b.signal))
    }
}

impl Layer for Network {
    fn down(&mut self, _frame: &mut Frame) -> Result<(), CommsError> {
        // LoRaWAN: addressing is in MAC header; nothing to add here.
        Ok(())
    }

    fn up(&mut self, _frame: &mut Frame) -> Result<bool, CommsError> {
        // Port-based dispatch happens in RfStackAO::on_rx_done after
        // receive_raw() returns the reassembled payload.
        Ok(true)
    }
}

/// No-op network layer for LoopbackPhy tests.
pub struct NoopNetwork;
impl Layer for NoopNetwork {
    fn down(&mut self, _f: &mut Frame) -> Result<(), CommsError> { Ok(()) }
    fn up(&mut self, _f: &mut Frame) -> Result<bool, CommsError> { Ok(true) }
}
```

---

## 10. Transport layer (`comms/src/transport.rs`)

Lifts FOTA's chunking / ACK / CRC-32 logic into a reusable reliable transport.
Inspired by TCP's sequence numbers + sliding window; sized for embedded RF links
(window = 1, half-duplex).

### 10.1 Frame header

```
Transport PDU header (5 bytes):
  [0] SEQ  : u8  — sequence number of this PDU
  [1] ACK  : u8  — last in-sequence PDU received from peer
  [2] FLAGS: u8  — bitfield (see below)
  [3] LEN  : u8  — payload length low byte
  [4] LENHI: u8  — payload length high byte (always 0 for LoRa ≤ 242 bytes)

Flags:
  bit 0 FIRST_FRAG — first fragment of a multi-fragment SDU
  bit 1 LAST_FRAG  — last fragment (or complete single-fragment)
  bit 2 ACK_REQ    — receiver must send ACK (reliable mode)
  bit 3 IS_ACK     — this PDU is a pure ACK (no payload)
  bit 4 IS_NACK    — negative ACK (retransmit requested)
  bit 5 RESET      — reset transport state (new session)
```

### 10.2 `ReliableTransport`

```rust
pub struct ReliableTransport {
    seq:          u8,                  // next TX sequence number
    acked:        u8,                  // last ACKed SEQ from peer
    retransmit:   Option<Frame>,       // copy of last unACKed frame
    retries:      u8,                  // retransmit attempts remaining
    max_retries:  u8,                  // configurable (default 3)
    state:        TransportState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TransportState {
    Idle,
    WaitingAck,
    Receiving { total: u8, seen_mask: u8 },
}

impl ReliableTransport {
    pub fn new(max_retries: u8) -> Self {
        Self {
            seq: 0, acked: 0,
            retransmit: None,
            retries: 0, max_retries,
            state: TransportState::Idle,
        }
    }

    /// Called by `RfStackAO` when its retransmit `TimeEvent` fires.
    pub fn on_timeout(&mut self) -> TransportAction {
        if let Some(ref frame) = self.retransmit {
            if self.retries > 0 {
                self.retries -= 1;
                TransportAction::Retransmit(frame.clone())
            } else {
                self.state = TransportState::Idle;
                self.retransmit = None;
                TransportAction::GiveUp
            }
        } else {
            TransportAction::Nothing
        }
    }

    pub fn on_ack_received(&mut self, ack_seq: u8) -> TransportAction {
        if ack_seq == self.seq.wrapping_sub(1) {
            self.retransmit = None;
            self.state = TransportState::Idle;
            TransportAction::TxComplete
        } else {
            TransportAction::Nothing
        }
    }
}

pub enum TransportAction {
    Nothing,
    TxComplete,
    Retransmit(Frame),
    GiveUp,
}

impl Layer for ReliableTransport {
    fn down(&mut self, frame: &mut Frame) -> Result<(), CommsError> {
        let payload_len = frame.len();
        let hdr = frame.prepend_header(5)?;
        hdr[0] = self.seq;
        hdr[1] = self.acked;
        hdr[2] = TransportFlags::FIRST_FRAG
               | TransportFlags::LAST_FRAG
               | TransportFlags::ACK_REQ;
        hdr[3] = payload_len as u8;
        hdr[4] = 0;

        // Save for potential retransmit (clone the frame state)
        let mut save = Frame::new();
        save.write_payload(frame.payload())?;
        self.retransmit = Some(save);
        self.retries     = self.max_retries;
        self.state       = TransportState::WaitingAck;
        self.seq         = self.seq.wrapping_add(1);
        Ok(())
    }

    fn up(&mut self, frame: &mut Frame) -> Result<bool, CommsError> {
        if frame.len() < 5 { return Ok(false); }
        let raw = frame.strip_header(5)?;
        let hdr = [raw[0], raw[1], raw[2], raw[3], raw[4]];
        let seq     = hdr[0];
        let flags   = hdr[2];

        // Duplicate detection: discard if already seen
        if seq == self.acked { return Ok(false); }

        if flags & TransportFlags::IS_ACK != 0 {
            // Pure ACK — no payload to pass up
            self.on_ack_received(seq);
            return Ok(false);
        }

        self.acked = seq;
        Ok(true)
    }
}

/// Flag constants for the transport header FLAGS byte.
pub mod TransportFlags {
    pub const FIRST_FRAG: u8 = 0x01;
    pub const LAST_FRAG:  u8 = 0x02;
    pub const ACK_REQ:    u8 = 0x04;
    pub const IS_ACK:     u8 = 0x08;
    pub const IS_NACK:    u8 = 0x10;
    pub const RESET:      u8 = 0x20;
}
```

### 10.3 `UnreliableTransport` (datagram)

```rust
pub struct UnreliableTransport { seq: u8 }

impl Layer for UnreliableTransport {
    fn down(&mut self, frame: &mut Frame) -> Result<(), CommsError> {
        let len = frame.len() as u16;
        let hdr = frame.prepend_header(5)?;
        hdr[0] = self.seq;
        hdr[1] = 0;
        hdr[2] = TransportFlags::FIRST_FRAG | TransportFlags::LAST_FRAG;
        hdr[3] = len as u8;
        hdr[4] = (len >> 8) as u8;
        self.seq = self.seq.wrapping_add(1);
        Ok(())
    }

    fn up(&mut self, frame: &mut Frame) -> Result<bool, CommsError> {
        if frame.len() < 5 { return Ok(false); }
        frame.strip_header(5)?;
        Ok(true)
    }
}
```

### 10.4 Retransmit timer integration

```rust
/// QF TimeEvent signal for transport retransmit timeout.
/// Arm in on_tx_queued(); disarm in on_ack_received().
pub const RF_TRANSPORT_TIMEOUT_SIG: Signal = Signal(30);

// In RfStackAO::on_event, RF_TRANSPORT_TIMEOUT_SIG triggers:
fn handle_transport_timeout(&mut self, ctx: &mut ActiveContext) {
    match self.stack.transport.on_timeout() {
        TransportAction::Retransmit(frame) => {
            // Re-drive the MAC/PHY layers with saved frame
            self.do_retransmit(frame, ctx);
            self.retransmit_timer.arm(
                TimeEventConfig::new(RF_TRANSPORT_TIMEOUT_SIG),
                RETRANSMIT_TIMEOUT_TICKS, None,
            );
        }
        TransportAction::GiveUp => {
            // Post RF_TX_FAIL_SIG to application
        }
        _ => {}
    }
}
```

---

## 11. `RfStackAO` — active object and state machine (`comms/src/stack.rs`)

### 11.1 Signals

```rust
// ── Application → RfStackAO ───────────────────────────────────────────────
pub const RF_TX_REQ_SIG:          Signal = Signal(20);
pub const RF_RX_START_SIG:        Signal = Signal(21);  // enter RX mode

// ── PHY ISR → RfStackAO (posted from port ISR bridge) ────────────────────
pub const RF_PHY_IRQ_SIG:         Signal = Signal(22);  // generic DIO fire
pub const RF_PHY_TX_DONE_SIG:     Signal = Signal(23);
pub const RF_PHY_RX_DONE_SIG:     Signal = Signal(24);
pub const RF_PHY_RX_TIMEOUT_SIG:  Signal = Signal(25);
pub const RF_PHY_CRC_ERROR_SIG:   Signal = Signal(26);

// ── RfStackAO → Application ───────────────────────────────────────────────
pub const RF_TX_DONE_SIG:         Signal = Signal(27);
pub const RF_TX_FAIL_SIG:         Signal = Signal(28);
pub const RF_RX_FRAME_SIG:        Signal = Signal(29);  // payload received

// ── Internal ──────────────────────────────────────────────────────────────
pub const RF_TRANSPORT_TIMEOUT_SIG: Signal = Signal(30);
```

### 11.2 Event payloads

```rust
/// Payload for RF_TX_REQ_SIG.
pub struct RfTxReqPayload {
    pub data:     heapless::Vec<u8, 242>,  // no_std; 242 = max LoRaWAN payload
    pub fport:    u8,
    pub reliable: bool,                    // use ReliableTransport vs Dgram
}

/// Payload for RF_RX_FRAME_SIG (application receives this).
pub struct RfRxFramePayload {
    pub data:    heapless::Vec<u8, 242>,
    pub port:    u8,
    pub rssi:    i16,
    pub snr:     i16,
}

/// Payload for RF_PHY_IRQ_SIG (posted from ISR).
#[derive(Clone, Copy)]
pub struct PhyIrqPayload {
    pub event: PhyEvent,
    pub meta:  RxMetadata,  // only valid for RxDone
}
```

### 11.3 State machine

```
                   ┌──────────┐
                   │  Initial │
                   └────┬─────┘
                        │ on_start: phy.init(), set_mode(Standby)
                        ▼
              ┌─────────────────┐
        ┌────►│     Idle        │◄──────────────────────┐
        │     └───┬─────────┬───┘                       │
        │         │         │                           │
        │  RF_TX_REQ_SIG  RF_RX_START_SIG              │
        │         │         │                           │
        │         ▼         ▼                           │
        │ ┌────────────┐ ┌─────────────────┐           │
        │ │Transmitting│ │  Listening      │           │
        │ │(arm timer) │ │  (set_mode Rx)  │           │
        │ └──┬─────────┘ └──┬──────────────┘           │
        │    │               │                           │
        │  RF_PHY_TX_DONE   RF_PHY_RX_DONE_SIG          │
        │    │               │ (post_from_isr fills      │
        │    ▼               │  PhyIrqPayload)           │
        │ ┌──────────────┐   ▼                           │
        │ │WaitingAck    │ ┌─────────────────┐          │
        │ │(ReliableOnly)│ │  Processing RX  │          │
        │ └──┬──────┬────┘ │  mac.up → net   │          │
        │    │      │      │  → transport.up  │          │
        │    │   TIMEOUT   │  → post to app  │          │
        │    │      │      └────────┬─────────┘          │
        │  ACK rx   │               │                    │
        │    │   retransmit         └───────────────────►┘
        └────┘      │
                    ▼
               ┌──────────────┐
               │ Retransmit   │──── max retries exhausted ──► GiveUp (post FAIL)
               └──────────────┘
```

### 11.4 `RfStackAO` struct

```rust
pub struct RfStackAO<T, N, M, P>
where
    T: Layer,
    N: Layer,
    M: Layer,
    P: RfPhy,
{
    stack:             RfStack<T, N, M, P>,
    tx_cfg:            RfTxConfig,
    rx_cfg:            RfRxConfig,
    retransmit_timer:  TimeEvent,
    rx_frame:          Frame,          // reused for DMA; cleared after each RX
    state:             AoState,
    app_ao:            Arc<dyn ActiveRunnable>,  // destination for RX frames
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AoState { Idle, Transmitting, WaitingAck, Listening, ProcessingRx }

impl<T: Layer, N: Layer, M: Layer, P: RfPhy> ActiveBehavior for RfStackAO<T, N, M, P> {
    fn on_start(&mut self, ctx: &mut ActiveContext) {
        self.stack.phy.init().expect("RF PHY init failed");
        self.stack.phy.set_mode(RadioMode::Standby).expect("RF standby");
        self.retransmit_timer = TimeEvent::new(
            ActiveObjectId::new(ctx.id().0),
            TimeEventConfig::new(RF_TRANSPORT_TIMEOUT_SIG),
        );
        self.state = AoState::Idle;
    }

    fn on_event(&mut self, ctx: &mut ActiveContext, event: DynEvent) {
        match event.signal() {
            RF_TX_REQ_SIG          => self.handle_tx_req(ctx, &event),
            RF_PHY_IRQ_SIG         => self.handle_phy_irq(ctx, &event),
            RF_PHY_RX_DONE_SIG     => self.handle_rx_done(ctx, &event),
            RF_PHY_TX_DONE_SIG     => self.handle_tx_done(ctx),
            RF_PHY_RX_TIMEOUT_SIG  => { self.state = AoState::Idle; }
            RF_TRANSPORT_TIMEOUT_SIG => self.handle_transport_timeout(ctx),
            _ => {}
        }
    }
}

impl<T: Layer, N: Layer, M: Layer, P: RfPhy> RfStackAO<T, N, M, P> {
    fn handle_tx_req(&mut self, _ctx: &mut ActiveContext, event: &DynEvent) {
        if self.state != AoState::Idle { return; }  // drop if busy
        let Some(req) = event.payload.as_ref().downcast_ref::<RfTxReqPayload>() else { return };
        match self.stack.transmit(&req.data, &self.tx_cfg) {
            Ok(()) => {
                self.state = AoState::Transmitting;
                // Arm retransmit watchdog even for unreliable (catches PHY hang)
                self.retransmit_timer.arm(
                    TimeEventConfig::new(RF_TRANSPORT_TIMEOUT_SIG),
                    TX_WATCHDOG_TICKS, None,
                );
            }
            Err(e) => { eprintln!("RfStackAO: TX failed: {e}"); }
        }
    }

    fn handle_tx_done(&mut self, _ctx: &mut ActiveContext) {
        self.retransmit_timer.disarm();
        if self.state == AoState::WaitingAck {
            // Keep waiting; timer still running for ACK timeout
        } else {
            self.state = AoState::Idle;
            // Enter RX immediately (Class A: RX1/RX2 windows after TX)
            let _ = self.stack.phy.set_mode(RadioMode::Rx { timeout_ms: Some(1000) });
            self.state = AoState::Listening;
        }
    }

    fn handle_rx_done(&mut self, ctx: &mut ActiveContext, event: &DynEvent) {
        let Some(payload) = event.payload.as_ref().downcast_ref::<PhyIrqPayload>() else { return };
        let meta = payload.meta;

        // Read RX bytes from radio into the DMA buffer
        self.rx_frame = Frame::new();
        if self.stack.phy.read_rx(&mut self.rx_frame, &meta).is_err() { return; }
        self.rx_frame.set_received_len(meta.pkt_len as usize);

        match self.stack.receive_raw(&mut self.rx_frame) {
            Ok(Some(app_frame)) => {
                let rx_sig = Signal(RF_RX_FRAME_SIG.0);
                let pld = RfRxFramePayload {
                    data:  heapless::Vec::from_slice(app_frame.payload()).unwrap_or_default(),
                    port:  1,
                    rssi:  meta.rssi_dbm,
                    snr:   meta.snr_db_x10,
                };
                let ev = DynEvent::with_payload(rx_sig, pld);
                self.app_ao.post(ev);
                let _ = ctx.emit_trace(RF_NET_ROUTE, &[meta.rssi_dbm as u8]);
            }
            Ok(None) => {}  // filtered by MAC/transport (bad MIC, duplicate, etc.)
            Err(e) => { eprintln!("RfStackAO: RX stack error: {e}"); }
        }
        self.state = AoState::Idle;
    }
}
```

---

## 12. IRQ → event bridge (port layer)

This is the only place where the ISR and QP-RS meet.  It lives in the **port**
crate (`ports/esp32-c6/src/rf_isr.rs`, `ports/cortex-m/src/rf_isr.rs`), never in
`hal` or `comms`.

### 12.1 Pattern

```rust
// ports/esp32-c6/src/rf_isr.rs

use core::sync::atomic::{AtomicPtr, Ordering};
use qf::active::ActiveRunnable;
use comms::stack::{PhyIrqPayload, RF_PHY_RX_DONE_SIG, RF_PHY_TX_DONE_SIG};
use hal::rf::PhyEvent;

/// Set by main before interrupts are enabled; never changes after that.
static RF_AO: AtomicPtr<dyn ActiveRunnable> = AtomicPtr::new(core::ptr::null_mut());

pub fn register_rf_ao(ao: &'static dyn ActiveRunnable) {
    RF_AO.store(ao as *const _ as *mut _, Ordering::Release);
}

/// DIO1 GPIO EXTI interrupt handler (ESP32-C6 / SX1262).
///
/// Reads IRQ status from the SX1262 over SPI, clears flags, constructs a
/// typed `PhyIrqPayload`, and posts it to `RfStackAO` via `post_from_isr`.
///
/// # CMSIS / Cortex-M notes
/// - This function runs at the NVIC priority configured for DIO1 GPIO.
/// - That priority MUST be ≤ QK_BASEPRI (numerically ≥ on Cortex-M which
///   uses descending priority numbers) so QK scheduler can safely call
///   `post_from_isr` here.
/// - On ESP32 (RISC-V), equivalent priority is set in the CLIC or PLIC.
#[no_mangle]
pub extern "C" fn DIO1_IRQHandler() {
    let ao_ptr = RF_AO.load(Ordering::Acquire);
    if ao_ptr.is_null() { return; }
    let ao = unsafe { &*ao_ptr };

    // Read IRQ status from chip (must happen inside ISR to avoid race)
    let (event, meta) = read_sx1262_irq_status();   // SPI read; clears flags

    let sig = match event {
        PhyEvent::TxDone        => RF_PHY_TX_DONE_SIG,
        PhyEvent::RxDone(_)     => RF_PHY_RX_DONE_SIG,
        PhyEvent::CrcError      => RF_PHY_CRC_ERROR_SIG,
        _                       => RF_PHY_IRQ_SIG,
    };
    let payload = PhyIrqPayload { event, meta };
    let ev = DynEvent::with_payload(sig, payload);

    // QK ISR entry / exit macros manage BASEPRI and trigger PendSV
    qk::isr_entry();
    ao.post(ev);
    qk::isr_exit();
}
```

### 12.2 CMSIS NVIC priority configuration

On Cortex-M, QK uses BASEPRI to mask interrupts during the scheduler run.
All ISRs that call `post_from_isr` (or `qk::isr_entry`) must be configured
at a priority number **numerically greater than** `QK_BASEPRI` (lower
urgency), so they are not blocked by the scheduler lock.

```rust
// ports/cortex-m/src/init.rs

/// QK scheduler ceiling — ISRs at lower numeric priority than this value
/// are masked during scheduler lock via BASEPRI.
pub const QK_BASEPRI: u8 = 0x50;  // top 4 bits on 8-priority-bit MCU

pub fn configure_rf_interrupt() {
    use cortex_m::peripheral::NVIC;

    // DIO1 interrupt priority: must be numerically > QK_BASEPRI (lower urgency)
    // so BASEPRI masking does NOT block it during scheduler lock.
    // Example: 0xC0 on a 8-bit-priority Cortex-M (numerical > 0x50 ✓)
    unsafe {
        NVIC::unmask(Interrupt::EXTI4_15);
        (*cortex_m::peripheral::SCB::PTR).aircr.modify(|r| r); // priority group 4
        cortex_m::peripheral::NVIC::set_priority(
            Interrupt::EXTI4_15,
            0xC0,  // one group below QK_BASEPRI
        );
    }

    // SysTick (QK tick source) at even lower priority (numerically greater):
    // unsafe { core::ptr::write_volatile(0xE000_ED23 as *mut u8, 0xFF); }
}
```

**Priority table** (Cortex-M, 8-priority-bit MCU, QK_BASEPRI = 0x50):

| ISR                       | Priority | Masked by QK lock? |
|---------------------------|----------|---------------------|
| HardFault / NMI           | 0x00     | never               |
| QK-unaware (e.g. UART DMA)| 0x40     | **yes** (< 0x50)    |
| QK scheduler lock ceiling | 0x50     | boundary            |
| SysTick (QK tick)         | 0xC0     | no (> 0x50)         |
| DIO1 GPIO (RF IRQ)        | 0xC0     | no (> 0x50)         |
| PendSV (context switch)   | 0xFF     | no                  |

> **Rule**: any ISR that calls `post_from_isr` must be at priority **≥ QK_BASEPRI**
> (numerically), i.e. it must NOT be blocked by the scheduler's BASEPRI write.
> DIO and SysTick both satisfy this; random peripheral DMA ISRs that don't
> interact with QF should be at higher priority (lower number) if they need to be
> unblockable.

### 12.3 SPI DMA considerations

```
SX1262 GetIrqStatus SPI transaction inside ISR:
  ┌──────────┐  MOSI 0x12, 0x00, 0x00, 0x00
  │ SPI DMA  │──────────────────────────────────────────────────────►
  │ (polled) │  MISO ──, status_high, status_low
  └──────────┘

Inside an ISR: use polled (non-DMA) SPI for the 4-byte IRQ status read.
DMA with callbacks cannot be used inside a non-reentrant ISR safely.
The polled transfer is ≤ 8 SPI clock cycles at 8 MHz ≈ 1 µs — acceptable.

For the RX payload read (up to 256 bytes) which happens in `RfPhy::read_rx`
called from the AO context (NOT the ISR):
  - Use DMA SPI transfer.
  - `Frame::raw_buf_for_dma()` returns the 4-byte-aligned buffer.
  - Arm the DMA; block via QXK semaphore or QF event (preferred).
  - After DMA complete, call `frame.set_received_len(n)`.
  - On Cortex-M7/M55 with D-cache: call SCB::clean_dcache_by_addr before
    passing the buffer to DMA, and SCB::invalidate_dcache_by_addr after.
```

---

## 13. LoopbackPhy (`comms/src/phy/loopback.rs`)

For host tests (no real radio):

```rust
use std::collections::VecDeque;

pub struct LoopbackPhy {
    rx_queue: VecDeque<(Vec<u8>, RxMetadata)>,
}

impl LoopbackPhy {
    pub fn new() -> Self { Self { rx_queue: VecDeque::new() } }

    /// Inject a raw frame as if received over the air.
    pub fn inject(&mut self, bytes: &[u8]) {
        self.rx_queue.push_back((bytes.to_vec(), RxMetadata::default()));
    }
}

impl RfPhy for LoopbackPhy {
    fn init(&mut self) -> HalResult<()> { Ok(()) }
    fn set_mode(&mut self, _m: RadioMode) -> HalResult<()> { Ok(()) }
    fn configure_tx(&mut self, _c: &RfTxConfig) -> HalResult<()> { Ok(()) }
    fn configure_rx(&mut self, _c: &RfRxConfig) -> HalResult<()> { Ok(()) }

    fn transmit(&mut self, frame: &Frame) -> HalResult<()> {
        // Echo back (loopback): inject what was transmitted as an RX frame
        self.rx_queue.push_back((frame.phy_bytes().to_vec(), RxMetadata::default()));
        Ok(())
    }

    fn read_rx(&mut self, frame: &mut Frame, _meta: &RxMetadata) -> HalResult<()> {
        // Caller must set_received_len via poll_irq metadata
        Ok(())
    }

    fn poll_irq(&mut self) -> HalResult<Option<PhyEvent>> {
        if let Some((bytes, meta)) = self.rx_queue.pop_front() {
            // stash bytes somewhere the read_rx can find them — for the plan,
            // this would be a pending_rx field in the struct
            let mut m = meta;
            m.pkt_len = bytes.len() as u8;
            Ok(Some(PhyEvent::RxDone(m)))
        } else {
            Ok(None)
        }
    }

    fn clear_irq(&mut self) -> HalResult<()> { Ok(()) }
    fn rssi(&self)  -> HalResult<i16>          { Ok(-50) }
    fn chip_name(&self) -> &'static str         { "Loopback" }
}
```

---

## 14. QS tracing records (`crates/comms/src/records.rs`)

```rust
// Per-layer records — IDs 100–127 reserved for qp-rs application records.

/// PHY: frame queued for TX (freq, sf, bw, power, frame bytes).
pub const RF_PHY_TX:        u8 = 110;
/// PHY: TX on-air complete (from ISR bridge; wall-clock timestamp).
pub const RF_PHY_TX_DONE:   u8 = 111;
/// PHY: RX frame captured (rssi, snr, pkt_len, raw bytes).
pub const RF_PHY_RX:        u8 = 112;
/// MAC: LoRaWAN frame built — DevAddr, FCnt, MIC (4 bytes).
pub const RF_MAC_FRAME:     u8 = 113;
/// MAC: incoming frame validated (or dropped) — DevAddr, FCnt, pass/fail.
pub const RF_MAC_PARSE:     u8 = 114;
/// Network: port dispatch resolved (port → signal).
pub const RF_NET_ROUTE:     u8 = 115;
/// Transport: PDU enqueued with SEQ, flags, payload length.
pub const RF_TRANSPORT_TX:  u8 = 116;
/// Transport: ACK received — SEQ, round-trip ticks.
pub const RF_TRANSPORT_ACK: u8 = 117;
/// Transport: retransmit attempt — SEQ, attempt count.
pub const RF_TRANSPORT_RET: u8 = 118;
/// FOTA: chunk sent — chunk index, total chunks.
pub const FOTA_CHUNK:       u8 = 119;
```

Emit in each layer's `down`/`up` using the `ctx.emit_trace(record_id, &payload_bytes)` pattern already established in `lora.rs:69-79`.

---

## 15. File inventory

### New files

| File | Purpose |
|------|---------|
| `crates/comms/src/buf.rs` | `Frame`, `FramePool`, `FrameIdx` |
| `crates/comms/src/stack.rs` | `Layer` trait, `RfStack<T,N,M,P>`, `RfStackAO`, all signals |
| `crates/comms/src/transport.rs` | `ReliableTransport`, `UnreliableTransport`, `TransportFlags` |
| `crates/comms/src/net.rs` | `Network`, `NoopNetwork`, port dispatch table |
| `crates/comms/src/mac/mod.rs` | mac sub-module root |
| `crates/comms/src/mac/lorawan.rs` | `LoRaWanMac` (extracted from `lora.rs`) |
| `crates/comms/src/phy/mod.rs` | phy sub-module root |
| `crates/comms/src/phy/loopback.rs` | `LoopbackPhy` for host tests |
| `hal/src/rf.rs` | `RfPhy`, `RadioMode`, `RxMetadata`, `PhyEvent`, `RfTxConfig`, `RfRxConfig`, `RadioParams` |
| `ports/esp32-c6/src/rf_isr.rs` | DIO1 IRQ handler, `register_rf_ao` |
| `ports/cortex-m/src/rf_isr.rs` | Generic Cortex-M DIO ISR, NVIC priority setup |

### Modified files

| File | Change |
|------|--------|
| `crates/comms/src/lora.rs` | Thin wrapper: `LoRaRf<D>` = `RfStack<Dgram, NoopNetwork, LoRaWanMac, D>`; `build_frame` logic moves to `mac/lorawan.rs` |
| `crates/comms/src/fota.rs` | FOTA drives `ReliableTransport` + `Network` directly; chunking logic unchanged but no longer duplicates transport state |
| `crates/comms/src/mac.rs` | Renamed `mac/lorawan.rs`; old file becomes a re-export |
| `crates/comms/src/events.rs` | Add `PhyIrqPayload`, `RfRxFramePayload`; update signals to match new numbering |
| `crates/comms/src/records.rs` | Expanded per-layer record IDs (see §14) |
| `crates/comms/src/error.rs` | Add `CommsError::TableFull` for net dispatch table |
| `crates/comms/src/lib.rs` | Re-export `buf`, `stack`, `transport`, `net`, `mac`, `phy` |
| `hal/src/lora.rs` | `RfDriver` kept for backward-compat; `Sx1276Phy` / `Sx1262Phy` implement both `RfDriver` and `RfPhy` |
| `hal/src/lib.rs` | Re-export `rf` module |

---

## 16. Phased implementation plan

### Phase 1 — Buffer management + PHY trait

1. Add `crates/comms/src/buf.rs` with `Frame` and `FramePool`.
2. Add `hal/src/rf.rs` with `RfPhy`, `RadioMode`, `RxMetadata`, `PhyEvent`,
   `RfTxConfig`, `RfRxConfig`, `RadioParams`.
3. Make `Sx1276` and `Sx1262` implement `RfPhy` (add `configure_rx`, `read_rx`,
   `poll_irq`, `clear_irq`; keep existing `init` + `transmit`).
4. Backward-compat: `RfDriver` blanket impl over `RfPhy`.

**Verification**: `cargo build -p hal` and `cargo build -p hal-esp` green.

### Phase 2 — MAC layer extraction

5. Add `crates/comms/src/mac/lorawan.rs` with `LoRaWanMac : Layer`.
   Helpers `encrypt_frm_payload` and `compute_mic` extracted from `lora.rs:88-156`.
6. Wire `LoRaRf<D>` to wrap `LoRaWanMac<D>` (thin alias; same behavior).
7. Unit-test `LoRaWanMac::down` / `up` round-trip with known LoRaWAN test vectors.

**Verification**: `cargo test -p comms` green; `examples/lora_send` still runs.

### Phase 3 — Network and Transport layers

8. Add `comms/src/net.rs` with `Network` and `NoopNetwork`.
9. Add `comms/src/transport.rs` with `ReliableTransport` and `UnreliableTransport`.
10. Wire `FotaSession` to use `ReliableTransport::down/up` instead of
    ad-hoc seq/ack state.
11. Unit tests: `ReliableTransport` → `NoopMac` → `LoopbackPhy` loopback;
    verify retransmit fires after timeout.

**Verification**: `cargo test -p comms` green; FOTA example unchanged.

### Phase 4 — Stack composition + `RfStackAO`

12. Add `comms/src/stack.rs` with `Layer`, `RfStack`, `RfStackAO` (§7, §11).
13. Add `comms/src/phy/loopback.rs`.
14. Integration test: `LoRaStack<LoopbackPhy>` transmit → loopback → receive.
15. Emit per-layer QS records (§14).

**Verification**: End-to-end test passes; QSpy shows `RF_PHY_TX` → `RF_MAC_FRAME`
→ `RF_TRANSPORT_TX` records in order.

### Phase 5 — ISR bridge (port layer)

16. Add `ports/esp32-c6/src/rf_isr.rs` with DIO1 handler (§12.1).
17. Configure NVIC priorities for DIO1 and SysTick (§12.2).
18. Test on real ESP32-C6 hardware with SX1262:
    TX a frame, verify `RF_PHY_TX_DONE_SIG` arrives in `RfStackAO`.

**Verification**: `lora_send` example on ESP32-C6 with real radio; QSpy shows
`RF_PHY_TX_DONE` record with correct timestamp.

### Phase 6 — Prove radio-agnosticism

19. Scaffold `BleL2cap` MAC layer stub (compiles but unimplemented).
20. Verify that `Transport<Network<BleL2cap<NordicPhy<SPI>>>>` compiles with the
    same `ReliableTransport` and `Network` as the LoRa path.
21. Confirm no transport/net/app changes needed — only MAC + PHY differ.

### Phase 7 — Polish and cortex-m port

22. Wire DIO ISR bridge for Cortex-M (`ports/cortex-m/src/rf_isr.rs`).
23. Document NVIC priority invariants in `ports/cortex-m/README.md`.
24. Ensure `cargo build --target thumbv7em-none-eabihf -p comms` is green
    (no `std`, no `alloc` in transport/mac/net/buf).

---

## 17. Design constraints

| Constraint | Rationale |
|------------|-----------|
| `no_std` first: layers compile without `alloc` | `heapless::Vec` for payloads; `FramePool` for buffers; retransmit buffer is a `Frame` value (stack or pool) |
| `#[repr(align(4))]` on `Frame` | Cortex-M DMA requires 32-bit alignment; also natural for 32-bit AES block operations |
| Zero-cost stack composition (`RfStack<T,N,M,P>`) | No vtable; no per-packet `Box`; generics monomorphise to one concrete stack per radio |
| ISR reads IRQ status; AO reads RX payload | Separates fast ISR path (≤ 4 SPI bytes polled) from slower DMA read (AO context) |
| QK BASEPRI rule: DIO ISR priority ≥ QK_BASEPRI | ISR must not be masked by scheduler lock; must call `qk::isr_entry`/`isr_exit` |
| `hal/` stays framework-agnostic | `RfPhy` has no `qf` dependency; `post_from_isr` call lives in the port only |
| Backward-compat through all phases | `LoRaRf`, `FotaSession`, `Rf` trait, `examples/lora_send` work at every phase boundary |
| Single writer per layer | `RfStack` owned by one `RfStackAO`; no mutex needed on the data path |

---

## 18. Verification matrix

| Check | Phase |
|-------|-------|
| `cargo build -p hal` and `-p hal-esp` | 1 |
| `cargo build -p comms` (std + no_std) | 1 |
| `cargo test -p comms` — MAC round-trip with LoRaWAN test vectors | 2 |
| `examples/lora_send` host unchanged | 2 |
| Transport retransmit unit test | 3 |
| `LoopbackPhy` end-to-end test (TX → full stack → RX) | 4 |
| QSpy shows per-layer records in correct order | 4 |
| ESP32-C6 DIO1 IRQ → `RfStackAO` `RxDone` event | 5 |
| `BleL2cap` stub compiles with same Transport/Network | 6 |
| `cargo build --target thumbv7em-none-eabihf -p comms` | 7 |
