# qs — Quantum Spy (tracing)

The diagnostics and tracing layer of [qp-rs](../../README.md): an HDLC-framed
binary trace protocol compatible with the QP/Spy host tools, plus a host→target
command parser (QS-RX).

## Where it sits

`qs` is an optional dependency of the framework crates (`qf`, `qk`, `qxk`,
`comms`), enabled by their `qs` feature. It depends on nothing in the framework,
so it can also be used standalone.

## Frame format

```
FLAG(0x7E) | SEQ | RECORD_TYPE | [TIMESTAMP] | PAYLOAD | CHECKSUM | FLAG
```

Bytes `0x7E`/`0x7D` are escaped as `0x7D, byte ^ 0x20`. Sequence numbers wrap at
`u8::MAX`.

## Key types

- `Tracer` / `TracerHandle` / `TraceBackend` — frame encoder and transports
- `TraceHook` — the shared callback installed on kernels and active objects
- `GlbFilter` — 128-bit per-record-type filter
- `QsRecord` / `QsConfig` — record model and configuration
- `records` — canonical QS record-id constants (matching QP/Spy)
- `rx` — `RxParser` / `RxCmd`: host→target command decoding
- `predefined` — dictionary helpers and `TargetInfo`

Backends include TCP, UDP, file/`Write`, and stdout.

## Minimal example

```rust,ignore
let tracer = Tracer::new(QsConfig::default(), stdout_backend()).into_handle();
let hook: TraceHook = tracer.hook();
// install `hook` on a kernel or active object
```

## Feature flags

- `std` *(default)* — std backends (TCP/UDP/file) and timestamps
- `no_std` builds provide the encoder, records, and RX parser

## Docs

API reference: `cargo doc -p qs --open`. See also the upstream
[QSpy](https://www.state-machine.com/qtools/qpspy.html) tool.
