# comms — communication middleware

The communication layer of [qp-rs](../../README.md): a LoRa/LoRaWAN RF transport
and firmware-over-the-air (FOTA) support, wired into [`qf`](../qf/README.md)
active objects and events.

## Where it sits

```
comms  →  application middleware
       ↓ uses
qf                 (active objects, events)
hal                (RfDriver and peripheral traits)
```

`comms` depends on `qf` (it drives RF workflows through QF active objects) and on
`hal` trait abstractions (`RfDriver`) for hardware independence — which is why it
lives in the main workspace, not in the framework-agnostic `hal/` workspace.

## What it provides

- `LoRaRf<D: RfDriver>` — LoRaWAN uplink transport over a radio driver
- `CommsAO` — a QF active object that drives the RF stack
- `FotaSession` — chunked firmware-over-the-air transfer
- AES-CMAC message authentication (no_std)
- `LoRaSession` — DevAddr + network/app session keys + uplink frame counter

See `examples/lora_send` for an end-to-end App → comms → HAL → radio example
(host uses a simulated radio; ESP32-C6 uses real SX127x/SX126x hardware).

## Feature flags

- `std` *(default)*
- `qs` — QS tracing integration

## Docs

API reference: `cargo doc -p comms --open`.
