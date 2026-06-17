# qxk — Quantum eXtended Kernel (dual-mode)

The dual-mode kernel of [qp-rs](../../README.md): combines event-driven active
objects (run-to-completion, like [`qk`](../qk/README.md)) with **extended
threads** that may block on synchronization primitives.

## Where it sits

```
comms / examples
       ↓
qxk  →  builds on qf
       ↓
hal
```

## Execution model

Extended threads use cooperative, polling-based handlers. A handler returns a
`ThreadAction`:

- `Continue` — keep running next cycle
- `Yield` — give other threads a turn
- `Blocked` — waiting on a primitive (scheduler removes it from the ready set)
- `Terminated` — done

Active objects run when ready; threads run when no AO is ready.

## Blocking primitives

`Semaphore`, `MutexPrim`, `MessageQueue`, and `CondVar` all follow the same
pattern: try the operation, return `WouldBlock` if it can't complete, and get
unblocked when the primitive is later signalled.

```rust,ignore
match semaphore.wait(ctx.thread_id(), ctx.priority().0, ctx.scheduler()) {
    Ok(()) => ThreadAction::Continue,
    Err(SyncError::WouldBlock) => ThreadAction::Blocked,
    Err(e) => panic!("{e}"),
}
```

See `examples/sync_primitives.rs` and `examples/producer_consumer.rs`.

## Key types

- `QxkKernel` / `ThreadConfig` / `ThreadContext` / `ThreadAction`
- `ScheduleMode` / `SchedStatus`
- `Semaphore`, `MutexPrim`, `MessageQueue`, `CondVar`, `SyncError`

## Feature flags

- `std` *(default)*
- `qs` — QS tracing integration

## Docs

API reference: `cargo doc -p qxk --open`.
