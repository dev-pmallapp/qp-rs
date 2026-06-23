# Traceability — Assumed Safety Requirements (ASR)

> Companion to [`FUSA.md`](./FUSA.md), Phase 4. This file is the **forward**
> half of the trace matrix: it defines the Assumed Safety Requirements (ASRs)
> qp-rs provides. The **backward** half — which source sites implement each ASR
> — is generated from `ASR-NNN` tags in the code by
> [`tools/trace-matrix.sh`](../tools/trace-matrix.sh).

## What an ASR is

An **Assumed Safety Requirement** is a safety property the framework commits to
providing, which an integrator's safety case may then *assume* when arguing
their system's compliance (analogous to upstream QP/C++'s assumptions-of-use and
its Spexygen requirement tags). Each ASR maps to one of the upstream Highly
Recommended / Recommended techniques (see `FUSA.md` §5) and is tagged at the
code site(s) that realise it with a `ASR-NNN` marker in a doc-comment.

The fault model is **crash-only**: every ASR's violation path ends at
[`qf::fusa::on_error`] (detect → halt gracefully), never silent corruption.

## Catalogue

### ASR-001 — Single fault path (crash-only)
Every detected fault — assertion failure, queue overflow, pool exhaustion,
error-detecting-code mismatch — routes to one overridable handler that does not
return, so a port installs exactly one safe-stop/reset policy.
*Technique: Fault detection (HR). Realised in `qf::fusa` (`on_error`,
`set_error_handler`).*

### ASR-002 — Failure-assertion programming
Preconditions, postconditions and data invariants are checked at runtime with
macros that carry a module id + line for diagnosis.
*Technique: Failure-assertion programming (R). Realised by the
`q_require!`/`q_ensure!`/`q_invariant!`/`q_assert!`/`q_error!` macros in
`qf::fusa`.*

### ASR-003 — Static (heap-free) resource allocation
Under `static-alloc` the framework links no global allocator: event queues,
timer wheels, pub/sub and event payloads all live in fixed, statically-sized
storage, so memory exhaustion is impossible by construction (over-capacity →
ASR-001).
*Technique: Static resource allocation (HR). Realised in `qf::pool` (`QMPool`),
`qf::equeue` (`StaticEQueue`), `qf::pool_arc` (`PoolArc`), and the heapless
backends of the AO queue / timer wheels.*

### ASR-004 — Error-detecting codes for memory corruption (SEU / bit flip)
Safety-critical scalar state is stored redundantly and verified on read; a
mismatch (single-event upset, bit flip) is a fault (ASR-001), not a silent
miscompute.
*Technique: Error-detecting codes (R). Realised in `qf::dis` (`Dis` inverse
storage, `Dup` duplicate storage, `DisAtomicU16` atomic refcount) and applied to
the AO scheduling priority, the pool free-list, the HSM current state, and the
pooled-payload refcount / pool id.*

### ASR-005 — Graceful degradation under overload
When an event queue fills into its configured safety margin, normal-priority
traffic is shed in a controlled, counted way to preserve headroom for critical
events, rather than overflowing.
*Technique: Graceful degradation (HR). Realised by the safety-margin API
(`post_normal`/`post_critical`/`PostStatus`/`is_degraded`) on `qf::equeue`.*

### ASR-006 — Memory-safe language subset / trusted elements only
The kernel layers are `#![forbid(unsafe_code)]`; all `unsafe` is confined to
`qf`'s allocation/ISR code, each block carrying a `# Safety` proof and validated
under Miri, and every dependency passes the `cargo deny` supply-chain gate.
*Technique: Trusted/verified elements only (HR). Realised by
`#![forbid(unsafe_code)]` on `qk`/`qxk`, the per-block `# Safety` proofs in
`qf`, `deny.toml`, and the CI Miri jobs.*

### ASR-007 — Semi-formal behavioural model (HFSM)
Application behaviour is expressed as a hierarchical state machine with
exhaustive event handling, giving a semi-formal, reviewable model rather than ad
hoc control flow.
*Technique: Semi-formal methods / FSM (HR). Realised in `qf::hsm` (`QHsm`) and
`qf::qmsm` (`QMsm`).*

## Generating the backward matrix

```bash
tools/trace-matrix.sh          # print forward+backward matrix
tools/trace-matrix.sh --check  # CI mode: non-zero exit if any ASR is untagged
```

The script reads the `### ASR-NNN` headings in this file as the canonical
requirement set, scans `crates/` for `ASR-NNN` code tags, and reports:

- **Backward trace** — each tagged source site and the ASR it satisfies.
- **Coverage** — any ASR with no implementing tag (a forward-trace gap), and any
  code tag referencing an ASR not defined here (a dangling tag).

`--check` fails CI on either gap, keeping the matrix bidirectional and current.
