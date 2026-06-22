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

- [ ] `static-alloc` cargo feature across `qf`/`qk`/`qxk`.
- [ ] Fixed-capacity event queue (ring buffer over `QMPool` / `heapless`) as a
      drop-in alternative to `VecDeque` in `equeue.rs`.
- [ ] Pool-allocated, reference-counted events replacing `Arc<dyn Any>`,
      adopting QP's `QEvt` header model (pool id + ref count in the event).
- [ ] Convert pub/sub (`pubsub.rs`) and the timer wheel (`time.rs`) to
      fixed-capacity `heapless` containers under the feature.
- [ ] Verify with a build that has **no global allocator** linked.

### Phase 3 — Error-detecting codes

- [ ] **Duplicate Inverse Storage (DIS)** wrapper: store value + bitwise
      inverse, verify on read, route mismatch to `q_on_error`. Apply to:
      event ref-counts, queue head/tail indices, pool free-list links, AO
      priority and current state.
- [ ] **Duplicate Storage** (non-inverted) for pool buffer links, per upstream.
- [ ] Event-queue **safety-margin** API: high-water mark + configurable margin
      → graceful-degradation signal instead of silent overflow.

### Phase 4 — Toolchain, lints & verification

- [ ] `#![forbid(unsafe)]` on every crate that can hold it; isolate
      unavoidable `unsafe` (`pool.rs`) into one audited module with `# Safety`
      proofs for each block.
- [ ] CI gates: `clippy -D warnings`, `cargo deny`, **MIRI** on the unsafe
      pool, plus a `no_std` link check.
- [ ] Document **Ferrocene** as the qualified reference toolchain and pin the
      qualified channel.
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
