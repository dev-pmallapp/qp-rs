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

```bash
cd examples/dpp-linux
cargo build --release
```

## Running

```bash
cargo run --release
```

Press **Ctrl-C** to stop the simulation.

## Expected Output

```
╔════════════════════════════════════════╗
║  QP Framework - Dining Philosophers    ║
║  Running on Linux (POSIX)              ║
╚════════════════════════════════════════╝

Philosopher 0 initialized in THINKING state
Philosopher 1 initialized in THINKING state
...

╔════════════════════════════════════════╗
║  Simulation Starting...                ║
║  Press Ctrl-C to stop                  ║
╚════════════════════════════════════════╝

[7] Philosopher 0 thinking -> HUNGRY
[7] Philosopher 0 got forks -> EATING
[14] Philosopher 1 thinking -> HUNGRY
[14] Philosopher 1 waiting for forks...
...

╔════════ Status at cycle 100 ════════╗
║ Eating:      [0 - - 3 4]
║ Forks:       [✗ ✓ ✓ ✗ ✗]
║ Eat count:   [ 3  2  2  2  2]
╚═════════════════════════════════════════╝
```

## Architecture

- **Philosophers**: Active objects with hierarchical state machines
- **Table**: Resource manager ensuring mutual exclusion
- **States**: Thinking, Hungry, Eating
- **Events**: Hungry, Done, Eat, Timeout

## QP Components Used

- `qp-core` - Core types and interfaces
- `qp-qep` - Hierarchical state machine (QHsm)
- `qp-posix` - POSIX port for Linux/Unix
