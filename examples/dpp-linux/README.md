# Dining Philosophers Problem - Linux (POSIX) Port

Classic DPP example demonstrating the QP framework running on Linux using the POSIX port.

## Description

Five philosophers sit at a round table with five forks between them. Each philosopher alternates between:
- **Thinking** - Contemplating the mysteries of life
- **Hungry** - Wanting to eat, requesting forks
- **Eating** - Consuming noodles with both adjacent forks

This demonstrates:
- QP Framework state machines on Linux
- POSIX port with std library
- Resource contention and management
- Event-driven architecture on native platforms

## Building

### With QS Tracing (Default)

```bash
cd examples/dpp-linux
cargo build --release --target x86_64-unknown-linux-gnu
```

### Without QS Tracing (Zero Overhead)

```bash
cargo build --release --target x86_64-unknown-linux-gnu --no-default-features
```

## Running

### Two-Terminal Workflow with QSpy

The recommended way to run with real-time trace visualization:

**Terminal 1 - Start QSpy Host Tool:**
```bash
cd ../../tools/qspy
cargo run --release
```

You should see:
```
╔════════════════════════════════════════════════════════╗
║              QSpy Software Tracing Utility             ║
║              Version 8.1.0 (Rust)                      ║
╚════════════════════════════════════════════════════════╝

📡 Binding to UDP socket: 0.0.0.0:7701
✓ Socket ready, listening for QS traces...
  Press Ctrl-C to stop
```

**Terminal 2 - Run DPP Example:**
```bash
cd examples/dpp-linux
cargo run --release --target x86_64-unknown-linux-gnu
```

The DPP output appears in Terminal 2, while **formatted traces** appear in Terminal 1 (QSpy).

### Single Terminal (Without QSpy)

```bash
cargo run --release --target x86_64-unknown-linux-gnu --no-default-features
```

Press **Ctrl-C** to stop the simulation.

## Expected Output

### Terminal 2 (DPP Application)

```
╔════════════════════════════════════════╗
║  QP Framework - Dining Philosophers    ║
║  Running on Linux (POSIX)              ║
║  QS Tracing: ENABLED                   ║
╚════════════════════════════════════════╝

QS: Initialized UDP output to QSpy at 127.0.0.1:7701
[QS] Signal Dictionary:
  3 = EAT_SIG
  2 = DONE_SIG
  4 = TIMEOUT_SIG
  1 = HUNGRY_SIG

QS: Tracing initialized (POSIX port)
Philosopher 0 initialized in THINKING state
Philosopher 1 initialized in THINKING state
...

╔════════════════════════════════════════╗
║  Simulation Starting...                ║
║  Press Ctrl-C to stop                  ║
╚════════════════════════════════════════╝

[50] Philosopher 0 thinking -> HUNGRY
[50] Philosopher 0 got forks -> EATING
[60] Philosopher 1 thinking -> HUNGRY
[60] Philosopher 1 waiting for forks...
...

╔════════ Status at cycle 100 ════════╗
║ Eating:      [0 - - 3 4]
║ Forks:       [✗ ✓ ✓ ✗ ✗]
║ Eat count:   [ 3  2  2  2  2]
╚═════════════════════════════════════════╝
```

### Terminal 1 (QSpy Host Tool)

```
📡 Binding to UDP socket: 0.0.0.0:7701
✓ Socket ready, listening for QS traces...

EMPTY            QS_INIT
USER             56 34 12
SM_TRAN          00THINKING->HUNGRY
SM_TRAN          00HUNGRY->EATING
SM_TRAN          00EATING->THINKING
SM_TRAN          01THINKING->HUNGRY
...
```

**QSpy Features:**
- Real-time trace reception via UDP
- Colored output by trace category
- State machine transition tracking
- Signal dictionary interpretation

## Architecture

- **Philosophers**: Active objects with hierarchical state machines
- **Table**: Resource manager ensuring mutual exclusion
- **States**: Thinking, Hungry, Eating
- **Events**: Hungry, Done, Eat, Timeout

## QP Components Used

- `qp-core` - Core types and interfaces
- `qp-qep` - Hierarchical state machine (QHsm)
- `qp-qf` - Framework and active objects
- `qp-qv` - Cooperative scheduler
- `qp-qs` - Software tracing (QS) with UDP output
- `qp-posix` - POSIX port for Linux/Unix

## QS Software Tracing

This example includes **QS (Quantum Spy)** software tracing:

### Features
- **UDP-based output** to QSpy host tool
- **Zero overhead** when disabled via `--no-default-features`
- **Real-time visualization** of state machine transitions
- **Signal dictionary** for readable event names
- **Selective enabling** via feature flags

### Build Modes

**With QS (default):**
```bash
cargo build --release --target x86_64-unknown-linux-gnu
# QS tracing enabled, sends UDP to localhost:7701
```

**Without QS (production):**
```bash
cargo build --release --target x86_64-unknown-linux-gnu --no-default-features
# Zero overhead, no tracing code compiled in
```

### Documentation

See detailed documentation:
- `UDP_QS_INTEGRATION.md` - Complete QS/QSpy integration guide
- `QS_INTEGRATION.md` - Feature flags and conditional compilation
- `../../tools/qspy/README.md` - QSpy host tool documentation
