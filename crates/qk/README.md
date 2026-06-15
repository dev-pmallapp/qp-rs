# qk — Quantum Kernel (preemptive)

The preemptive scheduling layer of [qp-rs](../../README.md), built on top of
[`qf`](../qf/README.md). Active objects still run their state machines to
completion, but a higher-priority AO can preempt a lower-priority one
mid-dispatch.

## Where it sits

```
comms / examples
       ↓
qk  →  builds on qf
       ↓
hal
```

## Preemption threshold

Each active object may declare a preemption threshold `T`: an AO with priority
`P` and threshold `T` can only be preempted by priorities greater than `T`. This
lets groups of related tasks share a non-preemptible ceiling and reduces context
switching.

| Aspect | QF (cooperative) | QK (preemptive) |
|--------|------------------|-----------------|
| Dispatch | Run to completion, then yield | Can be preempted mid-dispatch |
| Ready set | Linear scan | O(1) 64-bit bitmap |
| Max priorities | Unlimited | 63 (priority 0 = idle) |

## Key types

- `QkKernel` / `QkKernelBuilder` — the preemptive kernel and its builder
- `QkScheduler` / `SchedStatus` — O(1) ready-set scheduler with lock ceiling
- `QkTimerWheel` — time-event driver for the QK kernel

## Minimal example

```rust,ignore
let kernel = QkKernel::builder()
    .register(high_prio_ao)?
    .register_with_threshold(group_ao, 5)?
    .build()?;
kernel.start();
```

## Feature flags

- `std` *(default)* — enables `qf/std`
- `qs` — enables QS tracing (`qf/qs`)

## Docs

API reference: `cargo doc -p qk --open`.
