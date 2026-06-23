#![doc = r#"# qk

A clean-room, idiomatic Rust port of the [Quantum Kernel (QK)](https://www.state-machine.com/qpcpp/),
the preemptive scheduling layer that builds on the cooperative [`qf`] framework. Like `qf`
it compiles in both `std` and `no_std` environments.

QK is a *single-stack* preemptive kernel: active objects still run their state machines to
completion, but a higher-priority active object can **preempt** a lower-priority one
mid-dispatch. Each active object may declare a **preemption threshold** `T`: an AO with
priority `P` and threshold `T` can only be preempted by AOs with priority greater than `T`,
which lets groups of related tasks share a non-preemptible ceiling and cuts context
switching.

## Module overview
- [`QkKernel`] / [`QkKernelBuilder`] – preemptive kernel and its builder (register AOs,
  optionally with a preemption threshold).
- [`QkScheduler`] / [`SchedStatus`] – O(1) ready-set scheduler (64-bit bitmap) with
  nested scheduler locking and priority ceiling.
- [`QkTimerWheel`] – timer wheel driving [`qf`] time events under the QK kernel.

## QF vs QK

| Aspect | QF (cooperative) | QK (preemptive) |
|--------|------------------|-----------------|
| Dispatch | Run to completion, then yield | Can be preempted mid-dispatch |
| Priority enforcement | Event dispatch order | Preemption threshold |
| Ready set | Linear scan | O(1) 64-bit bitmap |
| Max priorities | Unlimited | 63 (priority 0 reserved for idle) |

Priority `0` is reserved for the idle thread; application AOs use priorities `1..=63`, and a
preemption threshold must be `>=` the AO's own priority.
"#]
#![cfg_attr(not(feature = "std"), no_std)]
// Functional safety (docs/FUSA.md, Phase 4): the preemptive-kernel layer is
// memory-safe by construction — all unsafe lives below it in `qf`.
#![forbid(unsafe_code)]

extern crate alloc;

mod kernel;
mod scheduler;
mod sync;
mod time;

pub use kernel::{QkKernel, QkKernelBuilder, QkKernelError};
pub use scheduler::{QkScheduler, SchedStatus};
pub use time::{QkTimeEventError, QkTimerWheel};
pub use qf::{ContextSwitchHook, QPrioSpec, q_prio};
