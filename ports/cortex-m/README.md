# Cortex-M port (`qf-port-cortex-m`)

Bare-metal Cortex-M port for [qp-rs](../../README.md)'s QXK dual-mode kernel. This
is the reference hardware port that performs **true context switching** for
extended threads.

## What it provides

- `ContextFrame` — exception stack-frame layout used to initialise extended-thread
  stacks and resume after a context switch.
- `ThreadStack` — initialises a raw byte slice as a Cortex-M initial stack frame,
  ready for the first `PendSV` restore.
- `CortexMQxkRuntime` — integrates `qxk`'s dual-mode scheduler with the Cortex-M
  exception model.
- `PendSV_Handler` / `SVC_Handler` — context-switch and scheduler-lock primitives.

## Context-switch model

`PendSV` performs the context switch; `SVC #0` is the scheduler-lock primitive.
On exception entry the core auto-stacks `r0–r3, r12, lr, pc, xpsr`; the PendSV
handler saves/restores the callee-saved `r4–r11` and updates `SP`. FP-capable
cores (M4F/M7F) additionally handle the lazy-stacked FPU registers.

## Features

- `std` *(default)* — hosted / emulation mode; the handlers compile to no-op
  stubs so the crate (and its tests) build on the desktop.
- `hw` — real Cortex-M hardware: `no_std`, with the real `PendSV`/`SVC` handlers.

```bash
# Hosted tests (default)
cargo test -p qf-port-cortex-m

# Build for hardware
cargo build -p qf-port-cortex-m --no-default-features --features hw
```
