# Introduction

**qp-rs** is a clean-room, idiomatic Rust port of the
[Quantum Platform (QP)](https://www.state-machine.com/) real-time embedded framework.
It implements active-object–based, event-driven architectures with cooperative and
preemptive kernels, hierarchical state machines, and QS software tracing — runnable on a
hosted POSIX target as well as bare-metal Cortex-M and ESP32.

## The programming model

A QP application is built from **active objects** (AOs): independent components that

- own a private event queue,
- run a **state machine** to completion for each event, and
- communicate *only* by posting asynchronous **events** to each other.

There is no shared mutable state between AOs and no blocking inside event handlers, which
makes the system deterministic and easy to reason about. The **kernel** schedules AOs by
priority; **time events** deliver periodic or one-shot timeouts; and **QS tracing** streams
a binary record of everything that happens to the QSpy host tool.

## What qp-rs gives you over QP/C++

The Rust type system removes whole categories of bugs QP/C++ guards against by convention:

- ownership & borrowing replace the manual DIS integrity checks,
- `Arc`/event pools replace manual reference counting,
- `Event<T>` + `downcast_ref` replaces unchecked C casts,
- `no_std` via Cargo features replaces `#ifdef` configuration.

## How this manual is organized

- **[Getting Started](./getting-started.md)** — install, run the example, read a trace.
- **[Concepts](./concepts.md)** — active objects, events, HSMs, time events, event pools.
- **[Kernels](./kernels.md)** — QF (cooperative), QK (preemptive), QXK (dual-mode).
- **[QS Tracing](./tracing.md)** — the trace protocol and QSpy interop.
- **[Ports](./ports.md)** — POSIX, Cortex-M, ESP32; the HAL contract.
- **[Examples](./examples.md)** — DPP and lora_send walkthroughs.
- **[Porting from QP/C++](./porting.md)** — API mapping and current gaps.

For the per-symbol API reference, build the rustdoc:

```bash
cargo doc --no-deps --open
```
