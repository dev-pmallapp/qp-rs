# qp-rs

A clean-room, idiomatic Rust port of the [Quantum Platform (QP)](https://www.state-machine.com/)
real-time embedded framework. QP-RS implements active-object–based, event-driven
architectures with cooperative and preemptive kernels, hierarchical state machines, and
QS software tracing — runnable on hosted POSIX as well as bare-metal Cortex-M and ESP32
targets.

It follows the upstream [QP/C++ Software Requirements Specification](https://www.state-machine.com/qpcpp/srs-qp.html)
and compiles in both `std` and `no_std` environments.

> **Status:** active development. The kernel layers (QF/QK/QXK), QS tracing, hierarchical
> state machines, event pools, and the DPP/LoRa examples work today. See
> [`GAP_ANALYSIS.md`](GAP_ANALYSIS.md) for a detailed feature comparison against QP/C++ v8.1.4.

## Why qp-rs

QP-RS brings the QP programming model — *active objects* that communicate only through
asynchronous events and run state machines to completion — to Rust, while leaning on the
type system and ownership model to eliminate whole classes of bugs that QP/C++ guards
against manually (aliasing, integrity checks, MISRA constraints).

## Crates & kernels

| Crate | Role | Highlights |
|-------|------|------------|
| [`qf`](crates/qf) | **Quantum Framework** — foundation | Active objects, events/signals, cooperative scheduler, time events, HSM, event pools |
| [`qk`](crates/qk) | **Quantum Kernel** — preemptive | Priority preemption with thresholds, O(1) bitmap ready set |
| [`qxk`](crates/qxk) | **Quantum eXtended Kernel** — dual-mode | Active objects + blocking extended threads, sync primitives |
| [`qs`](crates/qs) | **Quantum Spy** — tracing | HDLC-framed binary protocol, pluggable backends, QSpy interop |
| [`comms`](crates/comms) | Communication middleware | LoRa/LoRaWAN transport, FOTA, AES-CMAC |

| Kernel | Dispatch | Preemption | Ready set | Max priorities |
|--------|----------|------------|-----------|----------------|
| QF | Run-to-completion, cooperative | None | Linear scan | Unlimited |
| QK | Run-to-completion, preemptive | Threshold-based | O(1) 64-bit bitmap | 63 (0 reserved) |
| QXK | AOs + blocking threads | Threshold-based | O(1) bitmap | 63 |

## Layering

```
comms / examples          (protocol middleware, application)
       ↓ uses
qf / qk / qxk / qs        (framework — active objects, events, tracing)
       ↓ uses
hal                       (hardware-abstraction traits — framework-agnostic)
       ↓ uses
hal-esp / hal-cmsis / …   (chip-specific implementations)
```

The dependency direction is strict and must not be inverted. `hal/` knows only about
peripheral traits and never depends on a framework crate. See [`CLAUDE.md`](CLAUDE.md) for
the full layering rules.

## Workspace layout

```
crates/qf/        Core active-object framework
crates/qk/        Preemptive kernel primitives
crates/qxk/       Extended kernel with blocking threads
crates/qs/        QS tracing protocol
crates/comms/     LoRa/LoRaWAN and FOTA middleware
ports/posix/      POSIX (hosted) runtime
ports/cortex-m/   Cortex-M bare-metal port (PendSV/SVC context switch)
ports/esp32-s3/   ESP32-S3 runtime
ports/esp32-c6/   ESP32-C6 runtime
examples/dpp/     Dining Philosophers example (multi-target)
examples/lora_send/  App → comms → HAL → radio example
tools/qspy/       QSpy host tool
hal/              Separate HAL sub-workspace (excluded from root workspace)
```

## Quick start

```bash
# Build / test the whole workspace
cargo build
cargo test

# Run the Dining Philosophers example on the host (POSIX)
cargo run --bin dpp

# Run the QXK examples
cargo run --example sync_primitives
cargo run --example producer_consumer

# Run the LoRa send example on the host (simulated radio)
cargo run --bin lora_send
```

### Embedded targets

```bash
# ESP32-S3 / ESP32-C6
cargo build --bin dpp-esp32-s3 --features esp32s3 --no-default-features
cargo build --bin dpp-esp32-c6 --features esp32c6 --no-default-features
cargo build --bin lora_send_c6 --features esp32c6 --no-default-features
```

The `hal/` directory is a **separate workspace**; build it independently:

```bash
cd hal && cargo build
```

## Tracing with QSpy

QP-RS emits QS-compatible binary trace records (HDLC framed) that the standard QSpy host
tools can decode:

```bash
# Run qspy host tool (listens on TCP)
cargo run --bin qspy -- --tcp localhost:6601

# Run DPP with tracing — connects to localhost:6601 by default
cargo run --bin dpp
```

## Documentation

- **API reference (rustdoc):** `cargo doc --no-deps --open`
- **Per-crate guides:** [`qf`](crates/qf/README.md) · [`qk`](crates/qk/README.md) ·
  [`qxk`](crates/qxk/README.md) · [`qs`](crates/qs/README.md) · [`comms`](crates/comms/README.md)
- **Feature comparison vs QP/C++:** [`GAP_ANALYSIS.md`](GAP_ANALYSIS.md)
- **Upstream QP documentation:** <https://www.state-machine.com/>

## License

Licensed under either of MIT or Apache-2.0 at your option (per each crate's
`license = "MIT OR Apache-2.0"` manifest field).
