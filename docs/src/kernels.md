# Kernels: QF, QK, QXK

qp-rs offers three kernels that share the same active-object and event model. Pick the
weakest one that meets your timing needs.

| Aspect | QF (cooperative) | QK (preemptive) | QXK (dual-mode) |
|--------|------------------|-----------------|-----------------|
| Dispatch | Run-to-completion, then yield | Can preempt mid-dispatch | AOs + blocking threads |
| Priority enforcement | Event dispatch order | Preemption threshold | Threshold + thread priority |
| Ready set | Linear scan | O(1) 64-bit bitmap | O(1) bitmap |
| Max priorities | Unlimited | 63 (0 = idle) | 63 |
| Blocking | No | No | Extended threads may block |

## QF — cooperative

`qf::kernel::Kernel` dispatches the highest-priority ready AO to completion, then moves on.
A scheduler **ceiling** can lock out lower-priority AOs for a critical region.

```rust
let kernel = Kernel::builder()
    .register(ao_a)
    .register(ao_b)
    .build();
kernel.start();
kernel.run(|| timer_wheel.tick().unwrap()); // blocking run loop; stop() to exit
```

## QK — preemptive

`qk::QkKernel` adds priority preemption. Each AO may declare a **preemption threshold** `T`:
it can only be preempted by priorities greater than `T` (and `T` must be `>=` the AO's own
priority). This batches related tasks under a shared non-preemptible ceiling and reduces
context switching.

```rust
let kernel = QkKernel::builder()
    .register(high_prio_ao)?
    .register_with_threshold(group_ao, 5)?
    .with_trace_hook(hook)
    .build()?;
kernel.start();
```

Priority `0` is reserved for the idle thread; application AOs use `1..=63`.

## QXK — dual-mode

`qxk::QxkKernel` runs event-driven AOs *and* **extended threads** that may block on
synchronization primitives. AOs run when ready; threads run when no AO is ready.

Extended-thread handlers are polled and return a `ThreadAction`:

- `Continue` — run again next cycle
- `Yield` — let others run
- `Blocked` — waiting on a primitive
- `Terminated` — done

Blocking primitives — `Semaphore`, `MutexPrim`, `MessageQueue`, `CondVar` — share one
pattern: try the operation; if it can't complete return `WouldBlock`; the scheduler
removes the thread from the ready set and re-adds it when the primitive is signalled.

```rust
match semaphore.wait(ctx.thread_id(), ctx.priority().0, ctx.scheduler()) {
    Ok(())                     => ThreadAction::Continue,
    Err(SyncError::WouldBlock) => ThreadAction::Blocked,
    Err(e)                     => panic!("{e}"),
}
```

> On the hosted target, extended threads use a cooperative polling model. The Cortex-M
> port performs real PendSV/SVC context switching — see [Ports](./ports.md).

## Kernel configuration

`KernelConfig` (QF) carries system sizing and runtime options used by QS tracing and the
idle path:

```rust
let config = KernelConfig::builder()
    .name("MyApp")
    .max_active(32)
    .max_tick_rate(10)
    .idle_callback(my_idle_fn)
    .version(740)
    .build();
let kernel = Kernel::with_config(config).register(ao).build();
```
