# QS Tracing

The `qs` crate implements **Quantum Spy** — an HDLC-framed binary trace protocol
compatible with the QP/Spy host tools.

## Frame format

```
FLAG(0x7E) | SEQ | RECORD_TYPE | [TIMESTAMP] | PAYLOAD | CHECKSUM | FLAG
```

Bytes `0x7E` and `0x7D` are escaped as `0x7D, byte ^ 0x20`. Sequence numbers wrap at
`u8::MAX`. Timestamps are optional per-record (configured in `QsConfig`).

## Emitting records

A `Tracer` encodes records and writes frames to a `TraceBackend` (TCP, UDP, file/`Write`,
or stdout). The framework drives tracing through a shared `TraceHook` callback installed on
kernels and active objects:

```rust
let tracer = Tracer::new(QsConfig::default(), stdout_backend()).into_handle();
let hook: TraceHook = tracer.hook();
let kernel = QkKernel::builder().with_trace_hook(hook).register(ao)?.build()?;
```

Three levels of records are emitted:

1. **Kernel-level** — scheduler state changes (`LOCK`, `UNLOCK`, `NEXT`, `IDLE`).
2. **Framework-level** — time-event lifecycle (`ARM`, `DISARM`, `POST`), event new/gc.
3. **Application-level** — HSM state transitions and custom user records.

Record-type ids live in `qs::records` and match the QP/Spy protocol exactly, so the
standard QSpy host tool decodes qp-rs traces without modification.

## Filtering

`GlbFilter` is a 128-bit per-record-type filter; records whose bit is clear are suppressed
before reaching the backend (equivalent to `QS_GLB_FILTER()` in QP/C++).

## Host → target: QS-RX

`qs::rx` provides `RxParser`, an incremental decoder for the host→target command channel
(`RxCmd`: `Info`, `Tick`, `Peek`/`Poke`/`Fill`, filter commands, `Event` injection, and
the QUTest `TestSetup`/`TestProbe`/… commands). Command ids match the `QS_RX*` enum in
QP/C++.

## QUTest probes

`qs::qutest` provides test-probe support: production code calls `take_test_probe(fn_ptr)`
(or the `qs_test_probe!` macro) at injection points, and the host registers values with
`TestProbe` commands. See `examples/dpp/tests/qutest_dpp.rs`.

## Running QSpy

```bash
cargo run --bin qspy -- --tcp localhost:6601   # host tool
cargo run --bin dpp                            # connects to :6601 by default
```
