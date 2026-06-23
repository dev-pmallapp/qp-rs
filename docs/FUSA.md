# Functional Safety (FuSa) Roadmap for qp-rs

> Status: **proposal / roadmap**. Nothing in this document is implemented yet
> unless a task is explicitly marked ✅. This is the plan for evolving qp-rs
> toward the functional-safety posture of upstream Safe QP/C++.

## 1. Background

This roadmap mirrors the [QP/C++ Functional Safety
viewpoint](https://state-machine.com/qpcpp/sas-qp_fusa.html) (QP/C++ 8.1.4 —
the same release line qp-rs currently tracks) and adapts its techniques to
idiomatic Rust.

The upstream document defines how the framework supports software up to:

- **IEC 61508:2010** — SIL-3
- **IEC 62304:2015** — Class-C medical device software
- **ISO 26262:2018** — ASIL-D automotive

It achieves *systematic capability* through a fixed set of **Highly
Recommended (HR)** / **Recommended (R)** techniques, and a **crash-only**
fault model (detect → halt gracefully rather than attempt complex recovery).

## 2. Why Rust helps

Several FuSa measures that QP/C++ enforces with MISRA rules and static analysis
are *structural* in Rust:

| FuSa concern | Rust mechanism |
|---|---|
| Spatial memory safety, no aliasing faults | Ownership + borrow checker |
| Data races between AOs / ISRs | `Send`/`Sync` + borrow checker |
| Defensive handling of all inputs | Exhaustive `match` on signal/state enums |
| Safe language subset (cf. MISRA-C++:2023) | `#![no_std]` + `#![forbid(unsafe)]` per crate |
| Qualified compiler | **Ferrocene** (ISO 26262 / IEC 61508-qualified `rustc`) |

The roadmap leans on these and adds the mechanisms Rust does *not* give for
free: a fault/assertion model, fully static allocation, and error-detecting
codes.

## 3. Gap assessment (current state)

| FuSa mechanism (QP/C++) | qp-rs today | Gap |
|---|---|---|
| Semi-formal HFSM | ✅ `qf::hsm`, `qf::qmsm`, `q_tran!`/`qm_*` macros | Foundation in place |
| Static block pool | ✅ `qf::pool::QMPool` (O(1), `&'static mut` storage) | Present, not used everywhere |
| **Static allocation only** | ❌ `Arc`, `Box`, `Vec`, `VecDeque` across `active.rs`, `equeue.rs`, `time.rs`, `pubsub.rs`, `event.rs` | **Largest gap** |
| Failure-assertion programming | ✅ `qf::fusa` macros (`q_require!`/`q_ensure!`/`q_invariant!`/`q_assert!`/`q_error!`); core-path `unwrap()/expect()/panic!` migrated to `on_error` | Phase 1 complete |
| Crash-only fault handler (`Q_onError`) | ✅ `qf::fusa::on_error` + `set_error_handler` | Done; ports to install safe-stop handler |
| Error-detecting codes (Duplicate Inverse Storage) | ❌ None | New work |
| Event-queue safety margins | ⚠️ Counters exist; not formalized | Formalize |
| Safe language subset | ⚠️ `no_std`-capable; `unsafe` in `pool.rs` not lint-bounded | Lint policy + qualified toolchain |
| Memory isolation (MPU) | ❌ Not in ports | Port-level work |
| Forward/backward traceability | ❌ Ad hoc | Trace matrix needed |

## 4. Workstreams

### Phase 1 — Fault model & assertion subsystem *(foundational — do first)*

The single highest-leverage change: give every failure one well-defined path.

- [x] Add a `qf::fusa` module with assertion macros that carry a module id +
      line, mirroring QP/C's `Q_DEFINE_THIS_MODULE`:
  - `q_require!` — **precondition** (caller fault)
  - `q_ensure!` — **postcondition** (callee fault)
  - `q_invariant!` — **data-integrity** invariant
  - `q_assert!` — general assertion
  - `q_error!` — unconditional / unreachable-path fault

  *Implemented in `crates/qf/src/fusa.rs`. Module id = `module_path!()`,
  fault id = `line!()` by default (explicit id form also provided). Assertions
  are always-on (not gated on debug).*
- [x] Central `on_error(module: &'static str, id: u32) -> !` hook, overridable
      per port via `set_error_handler`, implementing the **crash-only** model.
  - `std` default: `panic!` with fault location (test-friendly).
  - `no_std` default: busy-halt; a port installs a handler that emits a QS
    frame then resets via watchdog / `cortex_m::asm::udf()`.
- [x] Migrate `unwrap()/expect()/panic!` in `qf`, `qk`, `qxk` **core paths**
      to route through `on_error`. Migrated sites: the `std` mutex-poison path
      in each crate's `sync.rs`, the `QHsm` initial-transition contract in
      `hsm.rs`, and the registry/scheduler invariants in `qk`/`qxk` `kernel.rs`.
      *(Remaining `unwrap()/expect()` live only in `#[cfg(test)]` modules.)*

*Deliverable: small, self-contained first PR. Unlocks every later phase and
immediately improves diagnosability. **Phase 1 complete.***

### Phase 2 — Static allocation path *(largest SIL impact)*

Goal: a `no_std + static-alloc` build that links **zero heap**.

- [x] `static-alloc` cargo feature across `qf`/`qk`/`qxk` (pulls in optional
      `heapless`; off by default so the dynamic host/test path is unchanged).
- [x] Fixed-capacity, heap-free event queue primitive — `qf::equeue::StaticEQueue<N>`
      (inline `heapless::Deque`, `const fn new` for `static` placement, margin
      + sticky low-water-mark, dogfoods `fusa::on_error` for the never-full
      invariant). *Introduced as a parallel primitive; the in-place swap of the
      per-AO `VecDeque` is deferred to avoid forcing const generics through the
      whole kernel API in one step.*
- [x] Swap the per-AO `EventQueue` and `QEQueue` storage to heap-free inline
      `heapless::Deque` under the feature. Done without leaking const generics
      into the kernel API by using a uniform compile-time capacity
      (`active::AO_QUEUE_CAPACITY`, `equeue::QEQUEUE_CAPACITY`, both 16) and a
      feature-gated backend swap, so `ActiveObject<B>` and `QEQueue::new` keep
      their signatures. Queue overflow → `fusa::on_error` (size your queues),
      matching QP/C. *Known limitation: per-AO queue sizing is uniform for now;
      individual sizing is a later refinement.*
- [ ] Pool-allocated, reference-counted events replacing `Arc<dyn Any>`,
      adopting QP's `QEvt` header model (pool id + ref count in the event).
- [x] Convert pub/sub and the timer wheels to fixed-capacity `heapless`
      containers under the feature: `PubSubTable` (`PUBSUB_MAX_SIGNALS = 256`),
      `qf::TimerWheel` and `qk::QkTimerWheel` (`MAX_TICK_RATES = 4`,
      `MAX_TIMERS_PER_RATE = 32`). Over-capacity registration → `fusa::on_error`.
      *(The registered `TimeEvent`s and `DynEvent` payloads are still `Arc`-backed
      — that heap goes away with the `QEvt` item below; this removes the
      container/`Vec` heap.)*
- [x] Pool-allocated, reference-counted event **payloads** replacing
      `Arc<dyn Any>`. New `qf::pool_arc::PoolArc` is an `Arc<dyn Any>`-equivalent
      over `POOL_REGISTRY`/`QMPool`: ref-counted control block + value inside one
      pool block, `Clone` = atomic refcount, `Drop` = drop-glue + return block.
      `DynPayload` switches to it under `static-alloc`; `empty_dyn` is
      allocation-free (empty variant); `Event::with_payload` is the portable
      typed constructor; `EventBox::into_dyn` is heap-free. **Validated under
      Miri** (no UB; round-trip, refcount, drop-once, free-to-pool). Miri also
      surfaced and fixed a provenance bug in the Phase-1 fault handler (now a
      `spin::Mutex<Option<fn>>` instead of `AtomicUsize` + `transmute`).
- [ ] Remove the remaining structural `Arc`s on the registration handles
      (`Arc<ActiveObject>`, `Arc<Kernel>`, `Arc<TimeEvent>`, `ActiveObjectRef`)
      — e.g. `&'static`/static storage — the last heap users outside the event
      payload path.
- [ ] Verify with a build that has **no global allocator** linked (blocked on
      the `Arc` registration-handle removal above).

### Phase 3 — Error-detecting codes

- [ ] **Duplicate Inverse Storage (DIS)** wrapper: store value + bitwise
      inverse, verify on read, route mismatch to `q_on_error`. Apply to:
      event ref-counts, queue head/tail indices, pool free-list links, AO
      priority and current state.
- [ ] **Duplicate Storage** (non-inverted) for pool buffer links, per upstream.
- [x] Event-queue **safety-margin** API: a persistent per-queue margin reserves
      free slots for critical traffic. `post_normal` sheds normal-priority events
      (counted, `shed_count`) instead of overflowing, returning a `PostStatus`
      (`Accepted` / `AcceptedDegraded` / `Shed`); `post_critical` may consume the
      margin; `is_degraded()` exposes the degraded state. On `QEQueue` and
      `StaticEQueue` (`with_safety_margin`, `const` on the latter). *(Per-AO
      `ActiveObject` queue degradation is a later refinement; today it faults on
      overflow under `static-alloc`.)*

### Phase 4 — Toolchain, lints & verification

- [x] `#![forbid(unsafe_code)]` on the kernel layers that can hold it — `qk`
      and `qxk` are memory-safe by construction (all `unsafe` lives in `qf`).
      *(Remaining: isolate `qf`'s `unsafe` — `pool`, `pool_arc`, `event_pool`,
      `isr`, `qmsm` — behind per-block `# Safety` proofs; `pool_arc` already
      documents each block.)*
- [x] CI gates (`.github/workflows/fusa.yml`): dynamic + `static-alloc` test
      runs, a `no_std + static-alloc` heap-free build, `clippy -D warnings` on
      the unsafe-free `qk`/`qxk`, and **Miri** over the unsafe `pool` and
      `pool_arc` modules. *(Remaining: `cargo deny`, broaden the clippy gate as
      `qf`/`qs` warnings are cleared.)*
- [x] Reference toolchain documented — see [§8 Reference toolchain](#8-reference-toolchain).
- [ ] **Traceability**: tag each Assumed Safety Requirement (ASR) in
      doc-comments and generate a forward/backward trace matrix (analog to
      QP's Spexygen).

### Phase 5 — Port-level memory isolation

- [ ] MPU-based isolation in `ports/cortex-m`: per-AO stack guard regions,
      read-only `.rodata` for state tables.
- [ ] Equivalent isolation review for the ESP32 (RISC-V / Xtensa) ports.

## 5. Mapping to upstream techniques

| Upstream technique | Rec. | qp-rs phase |
|---|---|---|
| Fault detection | HR | Phase 1 |
| Graceful degradation | HR | Phase 1 + 3 |
| Failure-assertion programming | R | Phase 1 |
| Error-detecting codes | R | Phase 3 |
| Static resource allocation | HR | Phase 2 |
| Static synchronization | R | Phase 2 (existing `sync.rs`) |
| Modular approach | HR | Existing crate split |
| Semi-formal methods (FSM) | HR | Existing `hsm`/`qmsm` |
| Event-driven w/ guaranteed response | HR | Existing kernels |
| Trusted/verified elements only | HR | Phase 4 |
| Forward/backward traceability | HR | Phase 4 |

**Not Recommended (and not planned):** AI-based fault correction, dynamic
reconfiguration — consistent with upstream.

## 6. Suggested sequencing

1. **Phase 1** (`qf::fusa` + `q_on_error`) — first PR, self-contained.
2. **Phase 2** static-allocation feature — the big enabler for embedded SIL.
3. **Phase 3** DIS + safety margins — builds on the fault hook from Phase 1.
4. **Phase 4** lints/toolchain/traceability — can proceed in parallel.
5. **Phase 5** MPU isolation — per-port, after the core is static.

## 7. Open questions

- Target an explicit SIL/ASIL claim, or "FuSa-ready architecture" only?
- Commit to Ferrocene as the qualified toolchain baseline?
- Keep the dynamic (`std`, `Arc`-based) path as a first-class host/test
  configuration alongside the static path? (Recommended: yes — dynamic for
  host tests, static for the safety build.)

## 8. Reference toolchain

A functional-safety argument requires a **qualified compiler** — the analog of
the MISRA-checked, qualified C++ toolchain upstream QP/C++ assumes.

- **[Ferrocene](https://ferrocene.dev/)** (Ferrous Systems' qualified
  downstream of `rustc`) is the reference toolchain for the qp-rs safety build.
  It is qualified against **ISO 26262 (ASIL-D)** and **IEC 61508 (SIL-4)**,
  matching the standards this viewpoint targets, and tracks specific upstream
  `rustc` releases — so qp-rs is built and tested on stable `rustc` and pinned
  to the corresponding **qualified Ferrocene channel** for a safety release.
- **Edition / MSRV**: Rust 2021. Pin the exact toolchain via `rust-toolchain.toml`
  for a safety build so the qualified compiler version is reproducible.
- **`core`/`alloc` only**: the safety build is `#![no_std]`; under
  `static-alloc` the event path is heap-free (see Phase 2), so the qualified
  `core` library surface is what matters — `std` is host/test only.
- **Verification toolchain** (CI, see `.github/workflows/fusa.yml`): stable
  `rustc` for tests and the heap-free build, `clippy` as the lint oracle on the
  `#![forbid(unsafe_code)]` kernel layers, and **Miri** as the dynamic
  UB-checker for the `unsafe` allocation code (`pool`, `pool_arc`).

> Status: Ferrocene is documented as the **intended** qualified baseline. Pinning
> a specific qualified channel (`rust-toolchain.toml`) is deferred to the first
> tagged safety release.
