# QP-RS Architecture Overview

## Directory Structure

```
qp-rs/
├── qp/                    # Core QP Framework (Workspace)
│   ├── core/             # Core types, events, signals, state machines
│   ├── mem/              # Memory management and pools
│   ├── qep/              # Event Processor - hierarchical state machines
│   ├── qf/               # Framework - active objects
│   ├── qv/               # Vanilla kernel - cooperative scheduling
│   ├── qk/               # Preemptive kernel
│   └── qs/               # Spy - software tracing
│
├── ports/                 # Platform Middleware (Standalone Crates)
│   ├── posix/            # Linux/Unix port with std
│   └── esp32c6/          # ESP32-C6 RISC-V port
│
├── boards/                # Board Support Packages (Standalone Crates)
│   └── esp32c6/          # ESP32-C6 low-level hardware access
│
└── examples/              # Applications
    ├── dpp-linux/        # Dining Philosophers for Linux
    └── dpp-esp32c6/      # Dining Philosophers for ESP32-C6
```

## Layered Architecture

```
┌─────────────────────────────────────────┐
│        Application Layer                 │
│  (examples/dpp-linux, dpp-esp32c6)      │
└────────────────┬────────────────────────┘
                 │
┌────────────────▼────────────────────────┐
│      QP Framework Layer                  │
│  (qp-core, qp-qep, qp-qf, qp-qv)        │
│  - Platform-independent                  │
│  - State machines, events, active objs   │
└────────────────┬────────────────────────┘
                 │
┌────────────────▼────────────────────────┐
│       Port/Middleware Layer              │
│  (ports/posix, ports/esp32c6)           │
│  - Critical sections                     │
│  - Time/tick service                     │
│  - Scheduler integration                 │
└────────────────┬────────────────────────┘
                 │
┌────────────────▼────────────────────────┐
│    Board Support Package Layer           │
│  (boards/esp32c6 OR OS APIs)            │
│  - Low-level hardware access             │
│  - GPIO, timers, UART, etc.              │
│  - HAL integration (esp-hal, etc.)       │
└────────────────┬────────────────────────┘
                 │
┌────────────────▼────────────────────────┐
│           Hardware Layer                 │
│  (ESP32-C6 chip, x86-64 CPU, etc.)      │
└─────────────────────────────────────────┘
```

## Key Concepts

### QP Framework (qp/*)
- **Platform-independent** event-driven framework
- Part of the Cargo workspace
- Provides: State machines (QEP), Active Objects (QF), Schedulers (QV/QK)
- No direct hardware or OS dependencies

### Ports (ports/*)
- **Middleware layer** between QP and hardware/OS
- Standalone crates (NOT in workspace)
- Depends on QP framework crates
- Implements platform-specific primitives:
  - Critical sections (interrupt control)
  - Time service (tick generation)
  - Scheduler integration (event loops)
- **Does NOT** directly access hardware

### Boards (boards/*)
- **Low-level hardware access** layer
- Standalone crates (NOT in workspace)
- Direct peripheral control (GPIO, timers, UART)
- HAL integration (esp-hal, embedded-hal)
- Platform-specific initialization
- **Does NOT** know about QP framework

### Examples (examples/*)
- Complete applications
- Standalone projects (NOT in workspace for embedded)
- Depend on both QP framework and ports
- Demonstrate QP patterns and usage

## Dependency Flow

```
Application
    ↓
QP Framework ←──────┐
    ↓               │
Port/Middleware     │ (depends on)
    ↓               │
Board BSP ──────────┘
    ↓
Hardware
```

## Building

### QP Framework (Workspace)
```bash
cd qp-rs
cargo build          # Builds all framework crates
cargo test           # Tests framework
```

### POSIX Port
```bash
cd ports/posix
cargo build --release
```

### ESP32-C6 Port
```bash
cd ports/esp32c6
cargo build --release
```

### Applications
```bash
# Linux
cd examples/dpp-linux
cargo run --release --target x86_64-unknown-linux-gnu

# ESP32-C6
cd examples/dpp-esp32c6
cargo build --release
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/dpp-esp32c6
```

## Advantages of This Architecture

1. **Separation of Concerns**
   - Framework is 100% platform-independent
   - Ports provide thin integration layer
   - Boards handle low-level hardware details

2. **Reusability**
   - QP framework can be used on any platform with a port
   - Ports can be shared across multiple applications
   - Boards can support multiple frameworks

3. **Testability**
   - Framework can be tested independently
   - Ports can mock hardware interfaces
   - Applications can unit test business logic

4. **Maintainability**
   - Clear boundaries between layers
   - Changes to one layer don't affect others
   - Easy to add new platforms

5. **Flexibility**
   - Mix and match ports and boards
   - Support multiple versions concurrently
   - Easy to prototype on different hardware

## Examples of Responsibility

### Framework (qp-qep)
```rust
// Platform-independent state machine
pub struct QHsm {
    state: QStateHandler,
}

impl QHsm {
    pub fn dispatch(&mut self, event: &dyn QEvent) -> QStateReturn {
        // Pure logic, no hardware access
    }
}
```

### Port (ports/posix/critical.rs)
```rust
// Platform-specific critical section
use std::sync::Mutex;

static CRITICAL_SECTION: Mutex<()> = Mutex::new(());

pub fn enter_critical() -> CriticalSection {
    let guard = CRITICAL_SECTION.lock().unwrap();
    CriticalSection { _guard: guard }
}
```

### Board (boards/esp32c6/src/lib.rs)
```rust
// Low-level hardware access
pub use esp_hal;
pub use esp_println;

pub fn init_gpio() -> Output<'static> {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    peripherals.GPIO8.into_push_pull_output()
}
```

### Application (examples/dpp-linux/src/main.rs)
```rust
use qp_port_posix as qp_port;
use qp_core::{QEvent, QStateMachine};

fn main() {
    qp_port::init();
    
    // Create active objects
    let mut philo = Philosopher::new(0);
    
    // Run application
    qp_port::run()
}
```

## Adding a New Platform

To support a new platform (e.g., STM32):

1. **Create Board BSP** (optional if HAL exists)
   ```bash
   mkdir boards/stm32f4
   cd boards/stm32f4
   # Add stm32f4xx-hal dependencies
   # Implement low-level drivers
   ```

2. **Create Port**
   ```bash
   mkdir ports/stm32f4
   cd ports/stm32f4
   # Depend on qp/* and boards/stm32f4
   # Implement critical sections, time, scheduler
   ```

3. **Create Application**
   ```bash
   mkdir examples/dpp-stm32f4
   cd examples/dpp-stm32f4
   # Depend on qp-port-stm32f4
   # Use QP framework to build app
   ```

## Current Status

✅ **Complete**:
- QP Framework core (qp/core, qp/qep, qp/qf)
- POSIX port for Linux (ports/posix)
- DPP example on Linux (examples/dpp-linux)

🚧 **In Progress**:
- ESP32-C6 port (ports/esp32c6) - stub implementation
- ESP32-C6 board BSP (boards/esp32c6)
- Full scheduler integration

📋 **Planned**:
- STM32 port and boards
- nRF52 port and boards
- More examples (Calculator, Blinky, IoT)
