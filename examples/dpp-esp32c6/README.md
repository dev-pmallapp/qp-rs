# Dining Philosophers Problem on ESP32-C6

A classic concurrency demonstration using the QP framework on ESP32-C6 RISC-V microcontroller.

## The Problem

Five philosophers sit at a round table with five forks placed between them. Each philosopher alternates between two states:
- **Thinking**: The philosopher contemplates deep questions
- **Eating**: The philosopher needs both adjacent forks to eat

This creates a resource contention scenario that demonstrates:
- Deadlock prevention
- Resource allocation
- Event-driven concurrency
- Active object patterns

## Implementation

This implementation uses the QP framework's key features:

### Active Objects
- **5 Philosopher Active Objects**: Each with a hierarchical state machine
- **1 Table Active Object**: Manages fork resources and allocation

### States
Each philosopher has three states:
1. **Thinking**: Idle state, occasionally becomes hungry
2. **Hungry**: Waiting for both forks to become available
3. **Eating**: Has both forks, eating for a period

### Events
- `Hungry`: Philosopher requests forks
- `Eat`: Table grants permission to eat
- `Done`: Philosopher finishes eating and releases forks
- `Timeout`: Triggers state transitions

### Resource Management
The Table active object implements:
- Fork availability tracking
- Deadlock-free allocation strategy
- Event-based permission granting

## Hardware Requirements

- ESP32-C6 development board
- LED on GPIO8 (built-in on most boards) for visual indication
- USB cable for programming and serial monitor

## Building

```bash
# Add RISC-V target if not installed
rustup target add riscv32imac-unknown-none-elf

# Build the project
cd examples/dpp-esp32c6
cargo build --release
```

## Flashing and Running

### Using espflash
```bash
cargo install espflash
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/dpp-esp32c6
```

### Using cargo-espflash
```bash
cargo install cargo-espflash
cargo espflash flash --monitor --release
```

## Expected Output

```
╔════════════════════════════════════════╗
║  QP Framework - Dining Philosophers    ║
║  Running on ESP32-C6                   ║
╚════════════════════════════════════════╝

Philosopher 0 initialized
Philosopher 1 initialized
Philosopher 2 initialized
Philosopher 3 initialized
Philosopher 4 initialized

╔════════════════════════════════════════╗
║  Simulation Starting...                ║
╚════════════════════════════════════════╝

[7] Philosopher 0 thinking -> HUNGRY
[7] Philosopher 0 got forks -> EATING
[14] Philosopher 1 thinking -> HUNGRY
[14] Philosopher 1 got forks -> EATING
[10] Philosopher 0 eating -> DONE
[10] Philosopher 0 released forks -> THINKING

╔════════ Status at cycle 100 ════════╗
║ Eating:      [ 0  -  2  -  - ]
║ Forks:       [ ✗  ✓  ✗  ✗  ✓ ]
║ Eat count:   [ 3  2  4  1  2 ]
╚═════════════════════════════════════════╝
```

## Visual Feedback

- **LED ON**: Philosopher 0 is eating
- **LED OFF**: Philosopher 0 is thinking or hungry

## Key Features

1. **Deadlock Prevention**: The table manages fork allocation to prevent circular wait
2. **No Starvation**: Round-robin scheduling ensures all philosophers get CPU time
3. **Event-Driven**: Uses QP framework's event mechanism for state transitions
4. **Hierarchical State Machines**: Clean state management for philosopher behavior
5. **Fair Scheduling**: Each philosopher gets equal time slices

## Architecture

```
┌─────────────────────────────────────────┐
│              Main Loop                  │
│  (Round-robin scheduler simulation)     │
└──────────┬──────────────────────────────┘
           │
           ├──► Philosopher 0 (HSM)
           ├──► Philosopher 1 (HSM)
           ├──► Philosopher 2 (HSM)
           ├──► Philosopher 3 (HSM)
           ├──► Philosopher 4 (HSM)
           │
           └──► Table (Resource Manager)
                 ├── Fork Allocation
                 ├── Deadlock Prevention
                 └── Event Response
```

## State Machine Diagram

```
          ┌────────────┐
    ┌────►│  Thinking  │◄────┐
    │     └─────┬──────┘     │
    │           │ TIMEOUT    │
    │           ▼            │
    │     ┌────────────┐     │
    │     │   Hungry   │     │ TIMEOUT
    │     └─────┬──────┘     │
    │           │ EAT        │
    │           ▼            │
    │     ┌────────────┐     │
    └─────│   Eating   │─────┘
          └────────────┘
```

## Customization

### Adjust Timing
Modify the cycle conditions in main loop:
```rust
if cycle % 7 == philo_idx {  // Thinking -> Hungry frequency
if cycle % 5 == philo_idx {  // Eating duration
delay.delay_millis(50);      // Loop delay
```

### Change Number of Philosophers
```rust
const N_PHILO: usize = 5;  // Try 3, 5, or 7
```

## License

MIT OR Apache-2.0
