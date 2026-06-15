# RF Protocol-Stack Plan

**Date**: 2026-06-15
**Status**: Design / proposal (not yet implemented)
**Scope**: Evolve the `comms` crate from a LoRa-centric middleware into a layered,
radio-agnostic protocol stack — "TCP/IP for RF" — so that LoRa today and BLE /
802.15.4 / Wi-Fi / a host loopback tomorrow all share the same upper layers.

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
   RSSI; a real stack needs an event-driven RX path.

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

## 3. Target architecture

Each layer becomes a trait that transforms a payload coming from above into a PDU for
the layer below (egress) and vice-versa (ingress). A single `RfStack<...>` type composes
them, exactly as one would write `Tcp<Ip<Ethernet>>`.

```
┌────────────────────────────────────────────┐
│ Application AOs  (FOTA, telemetry, RPC)     │   reusable across radios
├────────────────────────────────────────────┤
│ Transport  reliability, ACK, fragmentation, │   reusable across radios
│            CRC, sequence numbers            │
├────────────────────────────────────────────┤
│ Network    addressing / routing (DevAddr)   │   reusable across radios
├────────────────────────────────────────────┤
│ MAC/Security  framing, MIC (AES-CMAC),      │   per-radio family
│               frame counters                │   (LoRaWAN, BLE L2CAP, …)
├────────────────────────────────────────────┤
│ PHY  trait RfPhy  (modulation, IRQ, RSSI)   │   per-chip (SX127x/SX126x/…)
└────────────────────────────────────────────┘
        ▲ driven by one RfStackAO (QF active object)
        events flow up via signals; commands flow down via send()
```

### Layer contract

```rust
/// One protocol layer. `Up` is the SDU exchanged with the layer above;
/// `Down` is the PDU handed to the layer below.
pub trait Layer {
    type Up;
    type Down;
    /// Egress: encapsulate an SDU from above into a PDU for below.
    fn down(&mut self, sdu: Self::Up) -> Result<Self::Down, CommsError>;
    /// Ingress: decapsulate bytes from below; `None` if more frames are needed.
    fn up(&mut self, pdu: &[u8]) -> Result<Option<Self::Up>, CommsError>;
}
```

Composition example (compile-time, zero dynamic dispatch):

```rust
type LoraStack<SPI> =
    Transport<Network<LoRaWanMac<Sx1262Phy<SPI>>>>;
// later:
type BleStack<P> =
    Transport<Network<BleL2cap<NordicPhy<P>>>>;
```

`Transport`, `Network`, and the application layer are written **once** and reused by
every radio — that is what "integrate all RF" delivers.

## 4. Mapping to TCP/IP

| TCP/IP layer | RF-stack layer | Today | Target home |
|---|---|---|---|
| Application | FOTA / telemetry / app AO | `fota.rs` (ad-hoc) | `comms/app/*` |
| Transport (TCP/UDP) | reliability, ACK, fragmentation, CRC | inside FOTA | `comms/transport.rs` |
| Network (IP) | addressing / routing | inside `LoRaSession` | `comms/net.rs` |
| Data-link / MAC | framing, MIC, counters | `LoRaRf` + `mac.rs` | `comms/mac/lorawan.rs` |
| Physical | modulation, SPI chip seq | `RfDriver` ✅ | `hal` `RfPhy` |

## 5. Phased integration plan

### Phase 1 — Generalise the PHY (`hal`)
- Introduce `RfPhy` in `hal` as the generic physical-layer trait; keep `RfDriver`
  (LoRa-specialised) implementing/extending it.
- Add the RX/event surface a stack needs: `receive(&mut self, buf) -> nbytes`,
  `set_mode(Rx/Tx/Idle)`, `rssi()`, and a DIO/IRQ hook that the port maps to a real
  interrupt and turns into a QF event via `post_from_isr`.
- Keep `hal` framework-agnostic (no `qf` dependency) — the IRQ→event bridge lives in
  the **port**, not in `hal`.

### Phase 2 — Split `LoRaRf` into MAC + Network + Session
- Extract AES-CMAC MIC + LoRaWAN framing + frame counters into `LoRaWanMac` (`mac/lorawan.rs`).
- Extract DevAddr / session keys / counters into a Network/Session layer (`net.rs`,
  reusing `session.rs`).
- `LoRaRf` becomes a thin alias for the composed `Network<LoRaWanMac<Phy>>`.
- Behaviour-preserving: existing `examples/lora_send` must still pass.

### Phase 3 — Reusable Transport layer
- Lift FOTA's chunking / ACK / CRC-32 / sequencing out of `fota.rs` into a generic
  `ReliableTransport` (`transport.rs`) with fragmentation + retransmit + ordered delivery,
  plus an `UnreliableTransport` (datagram) variant.
- FOTA becomes a pure application over `Transport` — and so can telemetry / RPC.

### Phase 4 — `RfStack` composition + QF integration
- Add `RfStack<L: Layer>` and an `RfStackAO` active object that owns it.
- Downlink: app posts `RfTxReq` → `RfStackAO` walks layers `down()` → PHY transmits.
- Uplink: PHY RX-done IRQ → `post_from_isr` → `RfStackAO` walks layers `up()` →
  app receives a typed event. Generalises today's `CommsAO` / `events.rs`.

### Phase 5 — Prove radio-agnosticism with a second PHY
- Implement one non-LoRa PHY: a deterministic `LoopbackPhy` for host tests (beyond
  `NullRf`), and at least scaffold one real alternative (BLE or 802.15.4).
- Success criterion: Transport / Network / App layers compile **unchanged** against it.

### Phase 6 — Tracing + tests
- Add per-layer QS records (`RF_PHY_TX`, `RF_MAC_FRAME`, `RF_NET_ROUTE`,
  `RF_TRANSPORT_ACK`, …) in `crates/comms/src/records.rs` / `crates/qs/src/records.rs`
  so QSpy visualises packet flow through the stack.
- Layer-level unit tests + a `LoopbackPhy` end-to-end integration test
  (app → transport → net → mac → loopback → back up).

## 6. Design constraints

- **Keep the layering direction intact** (see `CLAUDE.md`): `hal` must stay
  framework-agnostic; `comms` depends on `qf` + `hal`. The IRQ→event bridge belongs in
  the port.
- **`no_std` first**: layers must compile without `alloc` where possible; the reliable
  transport's reassembly buffer should be backed by an event pool (`qf::event_pool`),
  not the heap.
- **Zero-cost composition**: prefer generics (`Transport<Network<...>>`) over
  `Box<dyn Layer>` so the stack monomorphises with no per-packet dynamic dispatch.
- **Backwards compatibility**: `Rf`, `LoRaRf`, `FotaSession`, and `examples/lora_send`
  keep working at each phase (the new layers sit behind the existing facade until
  Phase 4 flips the internals).

## 7. Files

**New:** `crates/comms/src/transport.rs`, `crates/comms/src/net.rs`,
`crates/comms/src/mac/lorawan.rs`, `crates/comms/src/stack.rs` (the `Layer` trait +
`RfStack` + `RfStackAO`), `crates/comms/src/phy/loopback.rs`; `hal/src/rf.rs` (`RfPhy`).

**Modified:** `crates/comms/src/lora.rs` (thin composition), `fota.rs` (app over
Transport), `lib.rs` (re-exports), `events.rs`/`records.rs` (per-layer events/records);
`hal/src/lora.rs` (RX/IRQ/RSSI on `RfDriver`); the ESP32-C6 port (IRQ→event bridge).

## 8. Verification

- `cargo build -p comms` and `cargo build` (workspace) green at every phase.
- `cargo test -p comms` — per-layer unit tests + loopback end-to-end test.
- `examples/lora_send` still runs on host (simulated radio) unchanged.
- QSpy shows the new per-layer records in order for a single uplink.
