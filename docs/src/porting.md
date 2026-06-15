# Porting from QP/C++

qp-rs aims to be a straightforward port target for QP/C++ applications. This chapter
summarises the API mapping; the repo-root `GAP_ANALYSIS.md` has the authoritative,
section-by-section comparison against QP/C++ v8.1.4 plus an up-to-date implementation
status table.

## What is implemented

The framework now covers the bulk of QP/C++:

- **QHsm** hierarchical state machines (`qf::hsm`)
- **Event pools** — `QMPool`, `q_new`/`q_new_x`/`gc`, `EventBox` (`qf::pool`, `qf::event_pool`)
- **Defer / recall** and the raw **QEQueue** (`qf::equeue`)
- **`QF::run()` / `stop()`** lifecycle (`qf::kernel`)
- **Time events** with `arm`/`disarm`/`rearm`/`was_disarmed`
- **ISR-safe APIs** — `post_from_isr`, `publish_from_isr`, `tick_from_isr` (`qf::isr`)
- **QS records** including semaphore/mutex, the **QS-RX** command parser, and **QUTest** probes
- **Cortex-M PendSV/SVC** context switching

## Remaining gaps

- **Selective publish-subscribe** — `publish()` currently broadcasts to all AOs; there is
  no `subscribe`/`unsubscribe` yet.
- **Multi-tick-rate** — a single tick domain (the `tick_rate` field exists only in trace
  metadata).
- **`QPrioSpec` / `Q_PRIO()`** — priority and preemption threshold are passed separately
  (`register_with_threshold`) rather than packed into one value.

## API mapping

| QP/C++ | qp-rs |
|--------|-------|
| `QActive` | `ActiveObject<B>` |
| `QHsm` / `QHsm::dispatch()` | `qf::hsm::QHsm` / `QHsm::dispatch` |
| `Q_TRAN(t)` / `Q_SUPER(s)` / `Q_HANDLED()` | `QHsmResult::Tran/Super/Handled` |
| `QF::run()` / `QF::stop()` | `Kernel::run` / `Kernel::stop` |
| `QTimeEvt::armX(n, i)` / `disarm()` / `rearm(n)` | `TimeEvent::arm` / `disarm` / `rearm` |
| `QF::q_new<T>(sig)` / `QF::gc(e)` | `qf::event_pool::q_new` / `gc` (or `Arc` drop) |
| `QActive::defer/recall` | `qf::equeue::defer` / `recall` |
| `QEQueue` | `qf::equeue::QEQueue` |
| `QActive::postFromISR` / `publishFromISR` | `Kernel::post_from_isr` / `publish_from_isr` |
| `QK::schedLock/schedUnlock` | `QkKernel::lock_scheduler` / `unlock_scheduler` |
| `QXSemaphore::wait` / `QXMutex::lock` | `qxk::Semaphore::wait` / `MutexPrim::lock` |
| `QS_SIG_DICTIONARY` / `QS_OBJ_DICTIONARY` | `qs::predefined` dictionary helpers |
| `QActive::subscribe(sig)` | *(not yet implemented)* |
| `Q_PRIO(prio, thre)` | `register_with_threshold(ao, thre)` |

## Where Rust differs

- State handlers capture `&mut self` rather than being bare function pointers.
- Static queue/pool storage uses `&'static mut` slices instead of C arrays.
- Critical sections use `core::sync::atomic` + architecture primitives, surfaced through
  the port rather than a global `QF_INT_DISABLE()` macro.
- `Q_ASSERT`/`Q_ERROR` map onto Rust `assert!`/`panic!`.
