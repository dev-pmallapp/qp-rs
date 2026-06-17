# Concepts

## Active objects

An `ActiveObject<B>` (crate `qf`) encapsulates:

- a **behavior** `B: ActiveBehavior` (the state machine),
- a private **event queue**,
- a **priority**, and
- an independent execution context.

A behavior reacts to two callbacks:

```rust
pub trait ActiveBehavior: Send + 'static {
    fn on_start(&mut self, ctx: &mut ActiveContext);
    fn on_event(&mut self, ctx: &mut ActiveContext, event: DynEvent);
}
```

For simple flat state machines, the `SignalHandler` convenience trait only requires a
`handle_signal` method.

## Events and signals

Events are lightweight messages identified by a `Signal` (a `u16`), optionally carrying a
payload:

- `Event<T>` — a strongly typed event with payload `T`.
- `DynEvent = Event<Arc<dyn Any + Send + Sync>>` — the type-erased envelope the kernel
  delivers, enabling heterogeneous queues. Handlers recover the concrete type with
  `event.downcast_ref::<ConcreteType>()`.
- Signal-only events use `Signal` directly.

Events are `Send + Sync` and shared zero-copy across AOs via `Arc`.

## Hierarchical state machines (HSM)

The `qf::hsm` module provides a QHsm-style hierarchical state machine: `QHsm<S>` drives
state handlers that return a `QHsmResult` (`Handled`, `Tran(target)`, `Super(parent)`,
`Ignored`, …). On a transition the framework executes the correct exit→entry chain across
the state hierarchy and emits the matching QS records.

## Time events

`TimeEvent` (crate `qf::time`) delivers timeouts to an active object:

```rust
let te = TimeEvent::new(target_ao_id, TimeEventConfig::new(Signal(TIMEOUT)));
te.arm(/* timeout */ 10, /* interval */ Some(10)); // periodic every 10 ticks
```

- `arm(timeout, interval)` — one-shot (`None`) or periodic (`Some`).
- `rearm(n)` — update the counter without a disarm/rearm cycle.
- `disarm()` / `is_armed()` / `was_disarmed()`.

A `TimerWheel` (QF) or `QkTimerWheel` (QK) is `tick()`ed at the system rate; expired events
are posted to their targets.

## Event pools

For `no_std` / heap-free operation, `qf` provides fixed-block event pools:

- `QMPool` — a fixed-size block allocator over `&'static mut [u8]` with free list and
  high-water-mark diagnostics (`get_free`/`get_use`/`get_min`).
- `PoolRegistry` + `q_new` / `q_new_x` / `gc` — allocate typed events from the
  smallest fitting pool; `EventBox<T>` returns the block to the pool on drop.

With `std`, ordinary heap allocation via `Arc` is also available.

## Defer / recall and raw queues

`qf::equeue` provides a standalone `QEQueue` plus `defer` / `recall` / `flush_deferred`
for the common pattern of postponing events while an AO is in a transitional state.
