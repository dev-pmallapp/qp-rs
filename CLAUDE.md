# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

QP-RS is a Rust port of the Quantum Platform (QP) real-time embedded framework. It implements active object-based event-driven architectures with cooperative (QF) and preemptive (QK) kernels, along with QS tracing for diagnostics.

## Build Commands

### Standard Development Workflow

```bash
# Build all workspace members
cargo build

# Build specific crate
cargo build -p qf
cargo build -p qk
cargo build -p qs

# Run tests
cargo test

# Run tests for specific crate
cargo test -p qf
cargo test -p qk

# Check without building
cargo check
```

### Running Examples

```bash
# Run DPP example on host (POSIX)
cargo run --bin dpp

# Run QXK examples
cargo run --example sync_primitives
cargo run --example producer_consumer

# Build for ESP32-S3
cargo build --bin dpp-esp32-s3 --features esp32s3 --no-default-features

# Build for ESP32-C6
cargo build --bin dpp-esp32-c6 --features esp32c6 --no-default-features
```

### Working with QSpy Tracing

```bash
# Run qspy host tool (if built)
cargo run --bin qspy -- --tcp localhost:6601

# Run DPP with tracing to qspy
cargo run --bin dpp  # Connects to localhost:6601 by default
```

## High-Level Architecture

### Crate Structure

The project is organized into three core crates with a layered architecture:

**qf (Quantum Framework)** - Foundation layer providing:
- Active object pattern implementation (`ActiveObject<B>`, `ActiveBehavior` trait)
- Event system with type-safe signals and dynamic event dispatch
- Cooperative priority-based scheduler (non-preemptive)
- Time event services with timer wheel
- Platform abstraction for `std`/`no_std` environments

**qk (Quantum Kernel)** - Preemptive scheduling layer extending QF:
- Fully preemptive priority-based scheduler
- Preemption threshold support (per-AO configurable)
- O(1) ready set using 64-bit bitmap
- Nested preemption with priority ceiling
- Builds on QF's active objects and events

**qxk (Quantum eXtended Kernel)** - Dual-mode kernel with blocking threads:
- Combines event-driven active objects with extended blocking threads
- Cooperative thread execution via polling-based handlers
- Scheduler-aware blocking primitives (Semaphore, Mutex, MessageQueue, CondVar)
- Threads can block and wake on synchronization primitives
- Priority-based thread scheduling (threads run when no AOs are ready)

**qs (Quantum Spy)** - Diagnostics and tracing layer:
- HDLC-framed binary protocol compatible with QP/Spy host tools
- Pluggable trace backends (TCP, UDP, file, stdout)
- Record types for kernel events, time events, and user-defined traces
- Dictionary system for symbol resolution in host tools

### Key Design Patterns

**Active Object Pattern**: Each `ActiveObject<B>` encapsulates:
- A behavior implementing `ActiveBehavior` (state machine)
- An event queue (VecDeque)
- A priority level
- Independent execution context

**Type-Erased Events**: `DynEvent = Event<Arc<dyn Any + Send + Sync>>` allows:
- Heterogeneous event queues while preserving type safety
- Downcast to concrete types in event handlers
- Zero-copy event sharing across active objects

**Platform Abstraction via Features**:
- `std` feature: Uses `std::sync::Mutex`
- `no_std`: Falls back to `spin::Mutex`
- `qs` feature: Conditionally enables tracing infrastructure

**Builder Pattern for Kernels**:
```rust
QkKernel::builder()
    .register(active_object)
    .register_with_threshold(another_ao, threshold)
    .with_trace_hook(trace_hook)
    .build()?
```

### QF vs QK Scheduling

| Aspect | QF (Cooperative) | QK (Preemptive) |
|--------|------------------|-----------------|
| Dispatch | Run to completion, then yield | Can be preempted mid-execution |
| Priority enforcement | By event dispatch order | By preemption threshold |
| Lock mechanism | Scheduler ceiling | Lock ceiling + nested locks |
| Ready set | Linear scan | O(1) bitmap lookup |
| Max priorities | Unlimited | 64 (priority 0 reserved) |

**Preemption Threshold**: An AO with priority P and threshold T can only be preempted by AOs with priority > T. This reduces context switching for groups of related tasks.

### Tracing Integration

Three levels of trace emission:

1. **Kernel-level**: Scheduler state changes (LOCK, UNLOCK, NEXT, IDLE)
2. **Framework-level**: Time event lifecycle (ARM, DISARM, POST)
3. **Application-level**: State transitions + custom user records

Trace flow: `TraceHook` → `Tracer` → `TraceBackend` → TCP/UDP/File/Stdout

HDLC frame format: `FLAG(0x7E) | SEQ | RECORD_TYPE | [TIMESTAMP] | PAYLOAD | CHECKSUM | FLAG`

### Time Events

`TimeEvent` lifecycle:
- Create with `TimeEvent::new()`
- Arm with `arm(timeout, interval)` for one-shot or periodic
- Poll via `TimerWheel::tick()` or `QkTimerWheel::tick()`
- Auto-disarm on expiry (one-shot) or re-arm (periodic)
- Manual disarm with `disarm()`

### Kernel Configuration

**KernelConfig** provides system sizing and runtime configuration for QF kernels:

```rust
let config = KernelConfig::builder()
    .name("MyApp")
    .max_active(32)
    .max_event_pools(5)
    .max_tick_rate(10)
    .counter_sizes(2, 2)  // queue counters, time event counters
    .idle_callback(my_idle_fn)
    .version(740)
    .build();

let kernel = Kernel::with_config(config)
    .register(active_object)
    .build();
```

The config provides metadata for QS tracing (`TARGET_INFO` record) and allows customization of idle behavior.

### QXK Extended Threads

**Thread Execution Model**: QXK uses cooperative multitasking with polling-based handlers:

```rust
let thread_handler = Box::new(|ctx: &mut ThreadContext| -> ThreadAction {
    // Thread logic here
    // ctx provides: thread_id(), priority(), scheduler(), iteration()

    match some_operation() {
        Ok(result) => ThreadAction::Continue,    // Keep running
        Err(_) => ThreadAction::Yield,           // Give others a turn
    }
});

let thread = ThreadConfig::new(ThreadId(1), ThreadPriority(5), thread_handler);
```

**ThreadAction** values:
- `Continue`: Keep running in the next dispatch cycle
- `Yield`: Voluntarily yield to other threads
- `Blocked`: Thread is waiting on a synchronization primitive
- `Terminated`: Thread has completed execution

**Blocking Primitives** integrate with the scheduler:

```rust
// Semaphore wait - blocks if count is 0
match semaphore.wait(ctx.thread_id(), ctx.priority().0, ctx.scheduler()) {
    Ok(()) => {
        // Acquired semaphore, continue work
        ThreadAction::Continue
    }
    Err(SyncError::WouldBlock) => {
        // Scheduler blocked this thread, will wake when signaled
        ThreadAction::Blocked
    }
    Err(e) => panic!("Unexpected error: {}", e),
}

// Semaphore signal - wakes highest priority waiting thread
semaphore.signal(ctx.scheduler())?;
```

All primitives (`Semaphore`, `MutexPrim`, `MessageQueue`, `CondVar`) follow this pattern:
1. Try operation - succeeds immediately if possible
2. If would block, register as waiting and return `WouldBlock`
3. Scheduler blocks the thread (removes from ready queue)
4. When primitive is signaled, scheduler unblocks thread (adds back to ready queue)
5. Thread resumes on next dispatch cycle

**Producer-Consumer Example**:
```rust
// See crates/qxk/examples/producer_consumer.rs
let producer = ThreadConfig::new(ThreadId(1), ThreadPriority(5), Box::new(|ctx| {
    match empty_slots.wait(ctx.thread_id(), ctx.priority().0, ctx.scheduler()) {
        Ok(()) => {
            // Produce item
            queue.try_send(item, ctx.scheduler())?;
            full_slots.signal(ctx.scheduler())?;
            ThreadAction::Continue
        }
        Err(SyncError::WouldBlock) => ThreadAction::Blocked,
        Err(e) => panic!("{}", e),
    }
}));
```

### Ports and Examples

**Ports** (`/ports/`): Platform-specific runtime glue
- `posix`: POSIX implementation with `PosixQkRuntime`
- `esp32-s3`, `esp32-c6`: Embedded ESP32 targets

**Examples** (`/examples/dpp/`): Dining Philosophers Problem
- Demonstrates multiple active objects (Table + 5 Philosophers)
- Uses time events for thinking/eating timeouts
- Emits QEP state machine records and custom user records
- Multi-platform: host (POSIX), ESP32-S3, ESP32-C6

**QXK Examples** (`/crates/qxk/examples/`):
- `sync_primitives.rs`: Demonstrates semaphores, mutexes, message queues, condition variables
- `producer_consumer.rs`: Shows thread coordination with blocking primitives

## Important Implementation Details

### Priority Invariants

- Active objects must have unique priorities within a kernel
- QK reserves priority 0 for idle thread
- QK supports priorities 1-63 (inclusive)
- Preemption threshold must be >= priority for each AO

### Event Handling

- Events are `Send + Sync` via `Arc<dyn Any>`
- Use `event.downcast_ref::<ConcreteType>()` in handlers
- Events with payloads use `Event<T>` where `T: Any + Send + Sync`
- Signal-only events use `Signal` (u16) directly

### Mutex and Synchronization

- In `std` environments: Uses `std::sync::Mutex` (panics on poisoning)
- In `no_std`: Uses `spin::Mutex` (spinlock)
- Abstraction in `sync.rs` provides unified `Mutex<T>` and `Arc<T>`

### Tracing Considerations

- Trace hooks are optional (`Option<TraceHook>`)
- Can be disabled entirely by not enabling `qs` feature
- Timestamps are optional per-record (configured in `QsConfig`)
- Record sequence numbers wrap at u8::MAX

## Feature Flags

### qf crate
- `std` (default): Enable standard library support
- `qs`: Enable QS tracing integration
- `serde`: Enable serde serialization for events

### qk crate
- `std` (default): Enable standard library (enables `qf/std`)
- `qs`: Enable QS tracing (enables `qf/qs`)

### dpp example
- `host` (default): POSIX target with full tracing
- `esp32s3`: ESP32-S3 embedded target
- `esp32c6`: ESP32-C6 embedded target
- `qs`: Enable QS tracing (included in host)

## Workspace Structure

```
/crates/qf/       - Core active object framework
/crates/qk/       - Preemptive kernel primitives
/crates/qs/       - QS tracing protocol implementation
/ports/posix/     - POSIX platform runtime
/ports/esp32-s3/  - ESP32-S3 platform runtime
/ports/esp32-c6/  - ESP32-C6 platform runtime
/examples/dpp/    - Dining Philosophers example
/tools/qspy/      - QSpy host tool (optional)
/scratch/         - Reference QP/C implementation (not part of build)
```

## Code Organization Principles

### Core Framework (QF)
- `event.rs`: Signal and event primitives
- `active.rs`: Active object abstraction
- `kernel.rs`: Cooperative scheduler
- `time.rs`: Time event services
- `sync.rs`: Platform abstraction layer
- `trace.rs`: Optional tracing integration

### Preemptive Kernel (QK)
- `kernel.rs`: QK kernel with preemption logic
- `scheduler.rs`: Ready set and preemption threshold
- `time.rs`: Timer wheel for QK
- `sync.rs`: Same abstraction as QF

### Tracing (QS)
- `lib.rs`: Core tracer and frame encoder
- `record.rs`: User record builder utilities
- `records.rs`: Canonical record type IDs
- `predefined.rs`: Dictionary and metadata helpers

## Testing Strategy

- Unit tests in each crate's module files
- Integration tests in `tests/` directories
- Example applications serve as integration tests
- Platform-specific code tested on target hardware or emulators

## Common Patterns When Adding Features

### Adding a New Active Object
1. Define state machine implementing `ActiveBehavior`
2. Register with kernel builder
3. Define event signals and payloads
4. Emit trace records for observability (optional)

### Adding a New Trace Record
1. Define record ID in `qs/src/records.rs`
2. Use `UserRecordBuilder` to construct payload
3. Call trace hook with record type and payload
4. Add to dictionary in host initialization

### Porting to New Platform
1. Create new port in `/ports/<platform>/`
2. Implement runtime struct with kernel + timer wheel
3. Provide platform-specific initialization
4. Add feature flag to examples for the platform
