# qf — Quantum Framework

The foundation layer of [qp-rs](../../README.md): the active-object pattern, the
event/signal system, a cooperative priority scheduler, time events, hierarchical
state machines, and event pools. A clean-room, idiomatic Rust port of QP's QF,
following the upstream [SRS](https://www.state-machine.com/qpcpp/srs-qp.html) and
compiling in both `std` and `no_std`.

## Where it sits

```
comms / examples
       ↓
qf  ←  qk / qxk / qs build on this
       ↓
hal
```

`qf` is the layer everything else builds on; it depends only on `hal`-style
abstractions and (optionally) `qs` for tracing.

## Key types

- `ActiveObject<B>` / `ActiveBehavior` — active objects and their state machines
- `Event<T>` / `DynEvent` / `Signal` — type-safe and type-erased events
- `Kernel` / `KernelBuilder` / `KernelConfig` — cooperative scheduler
- `TimeEvent` / `TimerWheel` — one-shot and periodic timeouts
- `hsm` — hierarchical state machine support
- `event_pool` / `pool` — fixed-block event memory pools
- `equeue` — standalone raw event queue

## Minimal example

```rust
use qf::active::{new_active_object, ActiveContext, ActiveObjectId, SignalHandler};
use qf::event::Signal;
use qf::kernel::Kernel;

struct Blinky;
impl SignalHandler for Blinky {
    fn handle_signal(&mut self, sig: Signal, _ctx: &mut ActiveContext) {
        println!("got {sig}");
    }
}

let ao = new_active_object(ActiveObjectId::new(1), 1, Blinky);
let kernel = Kernel::builder().register(ao).build();
kernel.start();
```

## Feature flags

- `std` *(default)* — use the standard library (`std::sync::Mutex`)
- `qs` — enable QS tracing integration
- `serde` — derive `Serialize`/`Deserialize` for events

## Docs

API reference: `cargo doc -p qf --open`.
