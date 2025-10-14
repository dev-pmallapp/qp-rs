# QP Framework Port for ESP32-C6# QP ESP32-C6 Board Support Package



**Middleware layer** integrating QP framework with ESP32-C6 hardware.This package provides a Board Support Package (BSP) for the ESP32-C6 microcontroller using the QP framework.



## Architecture## Features



This port sits between the QP framework and the board support package:- ESP32-C6 RISC-V support

- QV cooperative kernel integration

```- GPIO LED control example

Application (examples/dpp-esp32c6)- esp-hal based hardware abstraction

        ↓

QP Framework (qp-core, qp-qep, qp-qf, qp-qv)## Hardware Requirements

        ↓

Port Layer (ports/esp32c6) ← YOU ARE HERE- ESP32-C6 development board

        ↓- LED connected to GPIO8 (or modify pin in code)

Board BSP (boards/esp32c6) - Low-level hardware access

        ↓## Building

Hardware (ESP32-C6 chip)

``````bash

# Add RISC-V target if not already installed

## Responsibilitiesrustup target add riscv32imac-unknown-none-elf



### Port Layer (`ports/esp32c6`)# Build the project

- Implement QP critical section APIcargo build --release

- Setup tick timers for QP time events```

- Integrate QP scheduler with board event loop

- Coordinate initialization sequence## Flashing

- **Does NOT** directly touch hardware registers

```bash

### Board BSP (`boards/esp32c6`)# Using espflash

- Direct hardware access (GPIO, timers, UART, etc.)cargo install espflash

- esp-hal integrationespflash flash --monitor target/riscv32imac-unknown-none-elf/release/qp-bsp-esp32c6

- Low-level peripheral drivers

- Interrupt handlers# Or using cargo-espflash

- Memory layout and linker scriptscargo install cargo-espflash  

cargo espflash flash --monitor --release

## Dependencies```



```toml## Example Application

[dependencies]

qp-core = { path = "../../qp/core" }          # QP frameworkThe example demonstrates a simple blinky application using:

qp-qep = { path = "../../qp/qep" }            # State machines- QP active objects

qp-qf = { path = "../../qp/qf" }              # Active objects- Hierarchical state machines (QHsm)

qp-qv = { path = "../../qp/qv" }              # Scheduler- State transitions with timeout events

qp-bsp-esp32c6 = { path = "../../boards/esp32c6" }  # Board BSP- Hardware GPIO control

```

## Directory Structure

## Building

```

```bashesp32c6/

cd ports/esp32c6├── Cargo.toml          # Project dependencies

cargo build --release├── .cargo/

```│   └── config.toml     # Target configuration

├── src/

## Usage in Applications│   └── main.rs         # Blinky example

└── README.md           # This file

```toml```

[dependencies]

qp-port-esp32c6 = { path = "../../ports/esp32c6" }## License

```

MIT OR Apache-2.0

```rust
#![no_std]
#![no_main]

use qp_port_esp32c6 as qp_port;

#[entry]
fn main() -> ! {
    // 1. Initialize board (low-level)
    let peripherals = qp_port::board::init();
    
    // 2. Initialize QP port (middleware)
    qp_port::init();
    
    // 3. Initialize your active objects
    // ...
    
    // 4. Run QP scheduler
    qp_port::run()
}
```

## Components

### `critical.rs`
Critical section management using board interrupt control:
- `enter_critical()` - Disable interrupts
- `exit_critical()` - Restore interrupts

### `time.rs`
Time service integration:
- `init()` - Setup hardware timer
- `set_tick_rate()` - Configure tick frequency
- `start_ticker()` - Start periodic ticks
- `register_tick_callback()` - Connect to QP time events

### `scheduler.rs`
QP scheduler integration:
- `run()` - Main event loop
- `stop()` - Halt scheduler
- Integrates with board idle/sleep

## Status

🚧 **In Development**

Current implementations are stubs. TODO:
- [ ] Implement critical sections using board BSP
- [ ] Setup hardware timer for ticks
- [ ] Integrate QV scheduler with event loop
- [ ] Add power management (idle/sleep)

## See Also

- `boards/esp32c6/` - Low-level board support
- `examples/dpp-esp32c6/` - Complete application example
- `ports/README.md` - Port architecture overview
