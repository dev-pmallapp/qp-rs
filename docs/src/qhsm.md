# Using QHsm in Examples

`QHsm<S>` is the idiomatic way to write state machines in QP-RS.  It handles
the standard QHsm algorithm — LCA detection, exit/entry chain, nested initial
transitions, shallow history, and QS tracing — automatically.  You write plain
Rust functions; the framework does the bookkeeping.

---

## Why QHsm instead of raw ActiveBehavior

The `ActiveBehavior` trait gives full control but pushes all responsibility onto
the caller:

```rust
// Raw ActiveBehavior — you own every detail
impl ActiveBehavior for Philosopher {
    fn on_event(&mut self, ctx: &mut ActiveContext, event: DynEvent) {
        emit_qep_dispatch(ctx, self.object_addr, signal, self.state.addr());
        match (self.state, signal) {
            (PhiloState::Thinking, TIMEOUT_SIG) => {
                // manual exit action, entry action, QS records, state assignment
                emit_qep_state_exit(ctx, self.object_addr, source.addr());
                self.timer.disarm();
                self.state = PhiloState::Hungry;
                self.post_table(HUNGRY_SIG);
                emit_qep_state_entry(ctx, self.object_addr, target.addr());
                emit_qep_tran(ctx, ...);
            }
            // … many more arms …
        }
    }
}
```

With `QHsm<S>` the same state machine is three functions that return a
`QHsmResult`.  The framework calls entry/exit actions on transitions, walks the
hierarchy to find the LCA, and emits all QS records through the registered
trace hook automatically.

```rust
fn thinking(sm: &mut PhiloData, e: &DynEvent) -> QHsmResult<PhiloData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => { sm.timer.arm(sm.think_ticks(), None); q_handled!() }
        Q_EXIT_SIG_VAL  => { sm.timer.disarm(); q_handled!() }
        TIMEOUT_SIG     => q_tran!(hungry),
        _               => q_super!(QHsm::<PhiloData>::top_state),
    }
}
```

---

## Core concepts

### State handler functions

Every state is a free function (or a named function item):

```rust
pub type StateHandler<S> = fn(&mut S, &DynEvent) -> QHsmResult<S>;
```

The first argument is a mutable reference to your state machine data; the
second is the event being dispatched.

### `QHsmResult<S>`

Every state handler must return one of:

| Variant | Meaning |
|---------|---------|
| `Handled` | Event handled; no transition. |
| `Super(parent)` | Delegate to parent state (or `top_state`). |
| `Tran(target)` | Transition to `target`. |
| `TranHist(parent)` | Transition to the last active substate of `parent`. |
| `Ignored` | Event explicitly recognised but intentionally dropped. |
| `Unhandled` | Guard condition failed; treated like `Ignored`. |

### Reserved signals

The framework injects these into your state handlers during `init` and
`dispatch`.  **User-defined signals must start at `Q_USER_SIG` (value 4).**

| Constant | Value | Purpose |
|----------|-------|---------|
| `Q_EMPTY_SIG` | 0 | Hierarchy probe — return `Super(parent)`. |
| `Q_ENTRY_SIG` / `Q_ENTRY_SIG_VAL` | 1 | One-time setup on state entry. |
| `Q_EXIT_SIG` / `Q_EXIT_SIG_VAL` | 2 | Cleanup on state exit. |
| `Q_INIT_SIG` / `Q_INIT_SIG_VAL` | 3 | Initial transition of a composite state. |
| `Q_USER_SIG` | 4 | First value safe for application signals. |

Import them with:

```rust
use qf::hsm::reserved::*;
```

### Convenience macros

```rust
use qf::{q_tran, q_super, q_handled, q_ignored, q_tran_hist};

q_tran!(target_state)        // QHsmResult::Tran(target_state)
q_super!(parent_state)       // QHsmResult::Super(parent_state)
q_handled!()                 // QHsmResult::Handled
q_ignored!()                 // QHsmResult::Ignored
q_tran_hist!(parent_state)   // QHsmResult::TranHist(parent_state)
```

### Nesting limit

`QHsm` supports up to `MAX_NEST_DEPTH` (currently 6) levels of nesting.
Flat and two-level hierarchies are the most common; this limit is generous for
real embedded designs.

---

## Pattern 1 — Flat state machine (Blinky)

Two states, no composite structure.

```
top
 ├── off  (LED off)
 └── on   (LED on)
```

```rust
use qf::hsm::{QHsm, QHsmResult};
use qf::hsm::reserved::*;
use qf::event::DynEvent;
use qf::{q_tran, q_handled, q_super};

const TIMEOUT_SIG: u16 = Q_USER_SIG.0;   // 4

struct Blinky {
    count: u32,
}

fn initial(_sm: &mut Blinky, _e: &DynEvent) -> QHsmResult<Blinky> {
    q_tran!(off)
}

fn off(sm: &mut Blinky, e: &DynEvent) -> QHsmResult<Blinky> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => {
            println!("LED off");
            q_handled!()
        }
        TIMEOUT_SIG => q_tran!(on),
        _ => q_super!(QHsm::<Blinky>::top_state),
    }
}

fn on(sm: &mut Blinky, e: &DynEvent) -> QHsmResult<Blinky> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => {
            sm.count += 1;
            println!("LED on  (blink #{})", sm.count);
            q_handled!()
        }
        TIMEOUT_SIG => q_tran!(off),
        _ => q_super!(QHsm::<Blinky>::top_state),
    }
}
```

**Construct, initialise, and dispatch manually** (unit test or host-side stub):

```rust
let mut hsm = QHsm::new(Blinky { count: 0 }, initial);
hsm.init();   // → off: Q_ENTRY fires, "LED off" printed

let timeout = DynEvent::empty_dyn(Signal(TIMEOUT_SIG));
hsm.dispatch(&timeout);   // off → on
hsm.dispatch(&timeout);   // on  → off
```

---

## Pattern 2 — Hierarchical state machine (composite states)

A three-state hierarchy with `active` as the composite parent of `serving` and
`paused`, modelling the DPP Table AO.

```
top
 └── active                 (composite)
      │  initial → serving
      ├── serving           (leaf)
      └── paused            (leaf)
```

The key rule: a leaf state delegates unhandled events to its parent with
`q_super!(parent)`.  The framework walks the chain until an ancestor handles
the event or `top_state` is reached.

```rust
use qf::hsm::{QHsm, QHsmResult};
use qf::hsm::reserved::*;
use qf::event::DynEvent;
use qf::{q_tran, q_handled, q_super};

const HUNGRY_SIG: u16 = Q_USER_SIG.0;        // 4
const DONE_SIG:   u16 = Q_USER_SIG.0 + 1;    // 5
const PAUSE_SIG:  u16 = Q_USER_SIG.0 + 2;    // 6
const SERVE_SIG:  u16 = Q_USER_SIG.0 + 3;    // 7

struct TableData {
    forks:  [bool; 5],
    hungry: [bool; 5],
}

impl TableData {
    fn new() -> Self {
        Self { forks: [true; 5], hungry: [false; 5] }
    }
    fn handle_hungry_signal(&mut self, _idx: usize) { /* ... */ }
    fn handle_done_signal(&mut self, _idx: usize)   { /* ... */ }
    fn serve_pending(&mut self)                     { /* ... */ }
}

// ── Initial pseudo-state ──────────────────────────────────────────────────
fn initial(_sm: &mut TableData, _e: &DynEvent) -> QHsmResult<TableData> {
    q_tran!(serving)
}

// ── Composite parent state ────────────────────────────────────────────────
fn active(sm: &mut TableData, e: &DynEvent) -> QHsmResult<TableData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => { println!("Table active"); q_handled!() }
        Q_EXIT_SIG_VAL  => { println!("Table shutdown"); q_handled!() }
        Q_INIT_SIG_VAL  => q_tran!(serving),   // nested initial → serving
        _ => q_super!(QHsm::<TableData>::top_state),
    }
}

// ── Leaf: serving ─────────────────────────────────────────────────────────
fn serving(sm: &mut TableData, e: &DynEvent) -> QHsmResult<TableData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => { sm.serve_pending(); q_handled!() }
        HUNGRY_SIG      => { sm.handle_hungry_signal(0); q_handled!() }
        DONE_SIG        => { sm.handle_done_signal(0); q_handled!() }
        PAUSE_SIG       => q_tran!(paused),
        _               => q_super!(active),   // delegate unhandled to active
    }
}

// ── Leaf: paused ──────────────────────────────────────────────────────────
fn paused(sm: &mut TableData, e: &DynEvent) -> QHsmResult<TableData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => { println!("Table paused"); q_handled!() }
        Q_EXIT_SIG_VAL  => { println!("Table resumed"); q_handled!() }
        SERVE_SIG       => q_tran!(serving),
        HUNGRY_SIG      => { sm.hungry[0] = true; q_handled!() }
        DONE_SIG        => { sm.handle_done_signal(0); q_handled!() }
        _               => q_super!(active),   // delegate unhandled to active
    }
}
```

**What happens on `PAUSE_SIG` dispatched while in `serving`:**

1. `serving` returns `Tran(paused)`.
2. Framework finds LCA of `serving` and `paused` → `active`.
3. Executes exit chain: `serving` exits (no exit action here).
4. Executes entry chain from `active` down to `paused`: `paused` entry fires.
5. State is now `paused`.

`active` is neither exited nor re-entered because it is the LCA.

**What happens on an unknown signal in `paused`:**

1. `paused` returns `Super(active)`.
2. Framework probes `active`.
3. `active` returns `Super(top_state)`.
4. `top_state` returns `Ignored` — event silently dropped.

---

## Pattern 3 — Guard conditions

Return `q_ignored!()` when a guard condition prevents the transition.

```rust
fn locked(sm: &mut DoorData, e: &DynEvent) -> QHsmResult<DoorData> {
    match e.signal().0 {
        OPEN_SIG => {
            if sm.key_present {
                q_tran!(open)           // guard passed
            } else {
                q_ignored!()            // guard failed: stay in locked
            }
        }
        _ => q_super!(QHsm::<DoorData>::top_state),
    }
}
```

You can also mutate state as part of the guard — mutations are visible even
when `Ignored` is returned.

---

## Pattern 4 — Shallow history (`TranHist`)

`TranHist(parent)` restores the last active direct child of `parent` rather
than entering `parent` fresh from its initial transition.

```rust
fn mode_select(sm: &mut AppData, e: &DynEvent) -> QHsmResult<AppData> {
    match e.signal().0 {
        RESUME_SIG => q_tran_hist!(operational),  // restore last op substate
        _          => q_super!(QHsm::<AppData>::top_state),
    }
}
```

On first use (before any substate has been visited), `TranHist` falls back to
`operational`'s initial transition.

History is tracked automatically per composite state — no extra data field
required in `S`.

---

## Pattern 5 — Accessing event payloads

Downcast the event payload inside a state handler using the standard `Arc<dyn
Any>` mechanism.

```rust
use std::sync::Arc;

#[derive(Clone)]
struct EatMsg {
    philosopher_id: u8,
}

fn serving(sm: &mut TableData, e: &DynEvent) -> QHsmResult<TableData> {
    match e.signal().0 {
        EAT_SIG => {
            if let Some(msg) = e.payload.as_ref().downcast_ref::<EatMsg>() {
                let idx = msg.philosopher_id as usize;
                sm.grant_forks(idx);
            }
            q_handled!()
        }
        _ => q_super!(active),
    }
}
```

Post a typed event from another AO:

```rust
use qf::event::Event;

let msg = Arc::new(EatMsg { philosopher_id: 2 });
let evt = Event::with_arc(Signal(EAT_SIG), msg as DynPayload);
kernel.post(TABLE_ID, evt)?;
```

---

## Pattern 6 — Registering with the QF/QK kernel

`QHsm<S>` implements `ActiveBehavior`, so it can be wrapped in an
`ActiveObject` and registered with any kernel.

```rust
use qf::active::new_active_object;
use qf::{ActiveObjectId, QHsm};
use qk::QkKernel;

let table_ao = new_active_object(
    ActiveObjectId::new(1),
    /* priority */ 10,
    QHsm::new(TableData::new(), initial),
);

let kernel = QkKernel::builder()
    .register(table_ao)?
    .build();
```

`QHsm::on_start` calls `init_traced(ctx.trace_hook())` and
`on_event` calls `dispatch_traced(&event, ctx.trace_hook())`, so QS records
are emitted automatically through whatever trace hook the kernel wires up —
no manual `emit_qep_*` calls needed.

---

## Pattern 7 — DPP Philosopher rewritten with QHsm

The existing `examples/dpp/src/main.rs` implements `Philosopher` with ~150
lines of `ActiveBehavior` boilerplate — manual exit/entry actions, explicit
QS record emission on every arm, and an `enum PhiloState` acting as a shadow
state variable.

`QHsm<PhiloData>` collapses this to a data struct and four handler functions.

### Data struct

```rust
struct PhiloData {
    index: usize,
    name:  &'static str,
    timer: Arc<TimeEvent>,
    rng:   SmallRng,
}

impl PhiloData {
    fn think_ticks(&mut self) -> u64 { self.rng.gen_range(3..=6) }
    fn eat_ticks(&mut self)   -> u64 { self.rng.gen_range(2..=5) }
    fn post_table(&self, sig: Signal) { /* kernel.post(TABLE_ID, …) */ }
}
```

### State handler functions

```rust
use qf::hsm::reserved::*;
use qf::{q_tran, q_handled, q_super};

const TIMEOUT_SIG: u16 = Q_USER_SIG.0;       // 4
const EAT_SIG:     u16 = Q_USER_SIG.0 + 1;   // 5
const DONE_SIG:    u16 = Q_USER_SIG.0 + 2;   // 6
const HUNGRY_SIG:  u16 = Q_USER_SIG.0 + 3;   // 7

fn philo_initial(_sm: &mut PhiloData, _e: &DynEvent) -> QHsmResult<PhiloData> {
    q_tran!(thinking)
}

fn thinking(sm: &mut PhiloData, e: &DynEvent) -> QHsmResult<PhiloData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => {
            sm.timer.arm(sm.think_ticks(), None);
            println!("{} is thinking", sm.name);
            q_handled!()
        }
        Q_EXIT_SIG_VAL => {
            sm.timer.disarm();
            q_handled!()
        }
        TIMEOUT_SIG => q_tran!(hungry),
        _           => q_super!(QHsm::<PhiloData>::top_state),
    }
}

fn hungry(sm: &mut PhiloData, e: &DynEvent) -> QHsmResult<PhiloData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => {
            sm.post_table(Signal(HUNGRY_SIG));
            println!("{} is hungry", sm.name);
            q_handled!()
        }
        EAT_SIG => q_tran!(eating),
        _       => q_super!(QHsm::<PhiloData>::top_state),
    }
}

fn eating(sm: &mut PhiloData, e: &DynEvent) -> QHsmResult<PhiloData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => {
            sm.timer.arm(sm.eat_ticks(), None);
            println!("{} is eating", sm.name);
            q_handled!()
        }
        Q_EXIT_SIG_VAL => {
            sm.timer.disarm();
            sm.post_table(Signal(DONE_SIG));
            q_handled!()
        }
        TIMEOUT_SIG => q_tran!(thinking),
        _           => q_super!(QHsm::<PhiloData>::top_state),
    }
}
```

### Registration

```rust
for i in 0..N_PHILO {
    let id    = ActiveObjectId::new(PHILO_BASE_ID + i as u8);
    let timer = TimeEvent::new(id, TimeEventConfig::new(Signal(TIMEOUT_SIG)));
    let data  = PhiloData {
        index: i,
        name:  NAMES[i],
        timer: Arc::clone(&timer),
        rng:   SmallRng::seed_from_u64(i as u64 + 1),
    };
    let ao = new_active_object(id, (i + 1) as u8, QHsm::new(data, philo_initial));
    builder = builder.register(ao)?;
    runtime.register_time_event(timer);
}
```

QS state-machine records (`QS_QEP_STATE_ENTRY`, `QS_QEP_TRAN`, etc.) are
emitted automatically.  The only QS work left to the application is the
**dictionary** registration at startup (symbol → address mapping), which
remains in `emit_reference_dictionary`.

---

## Pattern 8 — QHsm inside QXK extended threads

`QHsm` does not depend on any scheduler; it can also be driven by a QXK
extended thread's handler loop.

```rust
use qxk::thread::{ThreadConfig, ThreadAction, ThreadContext, ThreadId, ThreadPriority};
use qf::hsm::QHsm;

let mut hsm = QHsm::new(MyData::default(), my_initial);
hsm.init();

let thread = ThreadConfig::new(ThreadId(3), ThreadPriority(5), Box::new(move |ctx| {
    if let Some(evt) = ctx.scheduler().take_event_for(ctx.thread_id()) {
        hsm.dispatch(&evt);
        ThreadAction::Continue
    } else {
        ThreadAction::Yield
    }
}));
```

---

## QS tracing notes

When `QHsm` is registered as an `ActiveObject` and a trace hook is installed
on the kernel, the following QS records are emitted automatically:

| Record | When |
|--------|------|
| `QS_QEP_INIT_TRAN` | After the initial transition in `on_start`. |
| `QS_QEP_STATE_ENTRY` | For each state entered (top-down). |
| `QS_QEP_STATE_EXIT` | For each state exited (bottom-up). |
| `QS_QEP_STATE_INIT` | When a composite state's nested initial fires. |
| `QS_QEP_DISPATCH` | On every `on_event` call (with timestamp). |
| `QS_QEP_TRAN` | For every state transition. |
| `QS_QEP_INTERN_TRAN` | For every handled event that does not transition. |
| `QS_QEP_IGNORED` | For ignored / unhandled events. |
| `QS_QEP_TRAN_HIST` | For every `TranHist` transition. |

All record IDs match QP/C++ v8.x canonical values, so QSpy visualises them
without any configuration change.

**Dictionary entries still needed at startup** (QHsm provides no names, only
function-pointer addresses):

```rust
port.emit_fun_dict(philo_initial as usize as u64, "Philo::initial")?;
port.emit_fun_dict(thinking      as usize as u64, "Philo::thinking")?;
port.emit_fun_dict(hungry        as usize as u64, "Philo::hungry")?;
port.emit_fun_dict(eating        as usize as u64, "Philo::eating")?;
port.emit_fun_dict(
    QHsm::<PhiloData>::top_state as usize as u64,
    "QP::QHsm::top",
)?;
```

---

## State topology notation

When designing a QHsm use the following conventions to document the hierarchy
before writing code:

```
top                   ← always the implicit root
 └── <composite>      ← composite state: has Q_INIT_SIG handler
      │  initial → <leaf>
      ├── <leaf>      ← leaf state: all arms end in q_super!(<composite>)
      └── <leaf>
```

**Rules:**

- Every leaf state's default arm is `q_super!(parent)`.
- Every composite state's `Q_INIT_SIG` arm returns `q_tran!(first_substate)`.
- The initial pseudo-state (`fn initial`) also returns `q_tran!(first_state)`.
- Use `q_super!(QHsm::<S>::top_state)` only from the outermost layer(s).
- Depth is limited to `qf::MAX_NEST_DEPTH` (6) levels.

**Example — traffic light with pedestrian request:**

```
top
 └── operational          (composite; initial → red)
      ├── red             (leaf; on entry arm output; TIMER → green)
      ├── green           (leaf; on entry arm output; TIMER → yellow)
      ├── yellow          (leaf; on entry arm output; TIMER → red)
      └── (pedestrian handled at operational; deferred during yellow/green)
```

```rust
fn operational(sm: &mut TrafficData, e: &DynEvent) -> QHsmResult<TrafficData> {
    match e.signal().0 {
        Q_INIT_SIG_VAL  => q_tran!(red),
        PED_REQ_SIG     => { sm.ped_pending = true; q_handled!() }
        _               => q_super!(QHsm::<TrafficData>::top_state),
    }
}

fn red(sm: &mut TrafficData, e: &DynEvent) -> QHsmResult<TrafficData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => { sm.set_light(Light::Red); q_handled!() }
        TIMER_SIG       => q_tran!(green),
        _               => q_super!(operational),
    }
}

fn green(sm: &mut TrafficData, e: &DynEvent) -> QHsmResult<TrafficData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => { sm.set_light(Light::Green); q_handled!() }
        TIMER_SIG       => {
            if sm.ped_pending { sm.ped_pending = false; }
            q_tran!(yellow)
        }
        _               => q_super!(operational),
    }
}

fn yellow(sm: &mut TrafficData, e: &DynEvent) -> QHsmResult<TrafficData> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => { sm.set_light(Light::Yellow); q_handled!() }
        TIMER_SIG       => q_tran!(red),
        _               => q_super!(operational),
    }
}
```

---

## Summary: do's and don'ts

| Do | Don't |
|----|-------|
| Define signals starting at `Q_USER_SIG` (4) | Use 0, 1, 2, 3 as user signals |
| Return `q_super!(parent)` from every default arm | Fall off the end of a match without a default arm |
| Put timer arm/disarm in `Q_ENTRY_SIG_VAL` / `Q_EXIT_SIG_VAL` | Arm/disarm timers in event arms (misses re-entry) |
| Keep `S` as a plain data struct | Put event-handling logic in methods on `S` |
| Register function-pointer dictionary entries at startup | Expect QSpy to know function names automatically |
| Use `QHsm::sm_mut()` from outside if needed | Share `&mut S` across threads (ownership enforced by `Send`) |
