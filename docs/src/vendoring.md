# Vendoring qp-rs

This chapter explains how to consume qp-rs from a downstream project — either as a
**git dependency** or as a **vendored copy** checked into your own repository — through
the single `qp-rs` facade crate.

## The facade crate

qp-rs is built from several crates (`qf`, `qk`, `qxk`, `qs`, …). Depending on each one
directly forces a downstream project to keep their versions and feature flags in lockstep
by hand. To avoid that, the workspace ships a thin facade crate at
[`crates/qp-rs`](https://github.com/) that re-exports the constituent crates as named
submodules and exposes a single, coherent feature set.

A downstream project depends on **`qp-rs` only**:

```toml
[dependencies]
qp-rs = { path = "vendor/qp-rs/crates/qp-rs", default-features = false,
          features = ["qk", "qs", "std"] }
```

and reaches everything through it:

```rust
use qp_rs::prelude::*;          // common types, kernel-aware
use qp_rs::qk::QkKernel;        // explicit crate path
use qp_rs::qf::QHsm;            // foundation layer
use qp_rs::qs::QsConfig;        // tracing (when `qs` is enabled)
```

## Module layout

The facade re-exports each crate under its own module, gated by the matching feature:

| Module          | Content                                                        | Feature gate |
|-----------------|----------------------------------------------------------------|--------------|
| `qp_rs::qf`     | Active objects, events, cooperative kernel, time events, HSM   | always       |
| `qp_rs::port`   | Platform contract: `Runtime`, `TraceSink`, `ContextSwitch`     | always       |
| `qp_rs::qk`     | Preemptive single-stack kernel                                 | `qk`         |
| `qp_rs::qxk`    | Dual-mode kernel with blocking threads                         | `qxk`        |
| `qp_rs::qs`     | QS binary tracing protocol                                     | `qs`         |
| `qp_rs::comms`  | LoRa/LoRaWAN + FOTA middleware (drives QF active objects)      | `comms`      |
| `qp_rs::hal`    | Framework-agnostic peripheral traits (embedded-hal based)      | `hal`        |

`qf` (and the always-on `port` contract) is present because it is the foundation for
every kernel variant. The other modules only exist when their feature is enabled, so a
project that never enables `qxk` does not compile (or link) the dual-mode kernel at all.

### The prelude

`qp_rs::prelude` collects the types most application code needs, and is itself
feature-aware — kernel-specific items only appear when their feature is on:

```rust
use qp_rs::prelude::*;
// Always available (from qf):
//   ActiveObject, QActive, Q, Event, Signal,
//   QAsm, QHsm, QMsm, same_state, same_qmstate,
//   Kernel, KernelConfig, TimeEvent, TimerWheel,
//   QPrioSpec, q_prio, TraceHook
//
// With feature "qk":  QkKernel, QkKernelBuilder, QkTimerWheel
// With feature "qxk": QxkKernel, QxkKernelBuilder, QxkScheduler,
//                     Semaphore, MutexPrim, MessageQueue, CondVar
// With feature "qs":  QsConfig, Tracer, TraceBackend
```

## Feature flags

The facade exposes one unified feature set and propagates each flag down to the right
constituent crates using Cargo's optional-dependency (`dep:`) and weak-dependency (`?/`)
syntax, so you never enable a feature on a crate that isn't part of your build.

| Feature  | Effect                                                                 |
|----------|------------------------------------------------------------------------|
| `std`    | Enables `std` support across every selected crate.                     |
| `qs`     | Enables QS tracing; propagates to `qf`, `qk`, and `comms`.             |
| `qk`     | Pulls in the preemptive single-stack kernel (`qp_rs::qk`).             |
| `qxk`    | Pulls in the dual-mode kernel (`qp_rs::qxk`); implies `qk`.            |
| `comms`  | Pulls in the LoRa/LoRaWAN + FOTA middleware (`qp_rs::comms`). Requires `std` today. |
| `hal`    | Re-exports the framework-agnostic peripheral traits (`qp_rs::hal`, embedded-hal based). `no_std`-ready. |

The platform/port contract (`qp_rs::port`: `Runtime`, `TraceSink`,
`ContextSwitch`) is always available — it lives in `qf` and carries no extra
dependency.

The default feature set is:

```toml
default = ["std", "qk", "qs"]
```

### Kernel selection

Pick **one** kernel variant (or none for cooperative `qf`-only use):

- **none** — cooperative QF scheduler only. Smallest footprint.
- **`qk`** — preemptive single-stack kernel. Includes `qf`.
- **`qxk`** — dual-mode kernel with blocking threads. Includes `qf` + `qk`.

### Recommended configurations

```toml
# Host / desktop, preemptive kernel + tracing (this is the default)
qp-rs = { path = "vendor/qp-rs/crates/qp-rs",
          features = ["std", "qk", "qs"] }

# Bare-metal embedded, dual-mode kernel, no_std, with tracing
qp-rs = { path = "vendor/qp-rs/crates/qp-rs", default-features = false,
          features = ["qxk", "qs"] }

# Minimal cooperative-only, no_std, no tracing
qp-rs = { path = "vendor/qp-rs/crates/qp-rs", default-features = false }

# Embedded radio app: kernel + tracing + comms middleware + peripheral traits
qp-rs = { path = "vendor/qp-rs/crates/qp-rs",
          features = ["std", "qk", "qs", "comms", "hal"] }
```

> **`no_std`:** Build with `default-features = false` and omit `std`. The facade applies
> `#![no_std]` automatically when the `std` feature is off, and all kernel/tracing
> features are verified to compile in that mode. The `comms` feature currently
> requires `std`; the `hal` traits are `no_std`-ready.

## Writing portable applications (the `port` contract)

The facade gives you the framework; a **thin port crate** supplies the platform
glue (tick source, trace transport, critical section, context switch). The two
are decoupled by the `qp_rs::port` traits, so application logic can be written
**generically over the runtime** instead of naming a concrete port type:

```rust
use qp_rs::prelude::*; // brings Runtime, TraceSink, ContextSwitch into scope

/// Works on any target — host, Cortex-M, RISC-V, … — without change.
fn run_app<R: Runtime>(rt: &R) {
    while rt.has_pending_work() {
        rt.run_until_idle();
    }
}
```

A consumer therefore depends on **one facade crate plus one small port crate**:

```toml
[dependencies]
qp-rs        = { path = "vendor/qp-rs/crates/qp-rs", features = ["qk", "qs"] }
qf-port-posix = { path = "vendor/qp-rs/ports/posix" }   # swap per target
```

The port crate implements `qp_rs::port::Runtime` (and `ContextSwitch` /
`TraceSink` where relevant); your `run_app` stays identical when you switch the
port dependency to `qf-port-cortex-m`, `qf-port-riscv`, and so on.

## Consumption methods

### 1. Git dependency

Point Cargo at the repository. Cargo fetches and caches it for you; no files are checked
into your project.

```toml
[dependencies]
qp-rs = { git = "https://github.com/<org>/qp-rs.git", tag = "v8.1.1",
          default-features = false, features = ["qk", "qs", "std"] }
```

Pin to a `tag`, `rev`, or `branch` for reproducible builds. The facade version tracks the
upstream QP release line (currently `8.1.1`).

### 2. Vendored copy (git submodule)

Embed the whole qp-rs workspace under `vendor/` and depend on it by path. This gives you
an auditable, offline-buildable, pinned copy.

```bash
# From your project root
git submodule add https://github.com/<org>/qp-rs.git vendor/qp-rs
git submodule update --init --recursive
```

```toml
[dependencies]
qp-rs = { path = "vendor/qp-rs/crates/qp-rs", default-features = false,
          features = ["qk", "qs", "std"] }
```

To update later:

```bash
cd vendor/qp-rs && git fetch && git checkout v8.1.2 && cd -
git add vendor/qp-rs && git commit -m "Bump vendored qp-rs to v8.1.2"
```

### 3. Vendored copy (plain subtree / file copy)

If you prefer not to use submodules, copy the repository into `vendor/qp-rs/` (or use
`git subtree`) and depend on it by the same path as above. The only requirement is that
the path resolves to `crates/qp-rs` and that the sibling crates (`crates/qf`, `crates/qk`,
…) it references by relative path are present alongside it.

> **Important:** the facade depends on its sibling crates with relative paths
> (`../qf`, `../qk`, …). When vendoring, copy the **whole `crates/` tree**, not just
> `crates/qp-rs/`, or the path dependencies will not resolve. The `hal/` sub-workspace is
> *not* required unless you also build a hardware port.

## What you do **not** need to vendor

The facade only re-exports the framework crates. These parts of the repository are
**not** dependencies of `qp-rs` and can be left out of a vendored copy:

- `examples/` — DPP and LoRa demos.
- `ports/` — platform runtime glue (you provide your own port, or copy the one you need).
- `tools/qspy/` — the host-side trace viewer.
- `hal/` — the separate, excluded HAL sub-workspace (only needed for hardware ports).

## Verifying a vendored build

After wiring the dependency, confirm the feature matrix compiles for your target:

```bash
# Default (std + qk + qs)
cargo build -p qp-rs

# no_std, cooperative only
cargo build -p qp-rs --no-default-features

# no_std, dual-mode kernel + tracing
cargo build -p qp-rs --no-default-features --features "qxk,qs"
```

All three configurations build cleanly from the facade alone, which is the signal that the
vendored copy is complete and the feature flags propagate correctly.
