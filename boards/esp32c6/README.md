# QP ESP32-C6 Board Support Package

This package provides a Board Support Package (BSP) for the ESP32-C6 microcontroller using the QP framework.

## Features

- ESP32-C6 RISC-V support
- QV cooperative kernel integration
- GPIO LED control example
- esp-hal based hardware abstraction

## Hardware Requirements

- ESP32-C6 development board
- LED connected to GPIO8 (or modify pin in code)

## Building

```bash
# Add RISC-V target if not already installed
rustup target add riscv32imac-unknown-none-elf

# Build the project
cargo build --release
```

## Flashing

```bash
# Using espflash
cargo install espflash
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/qp-bsp-esp32c6

# Or using cargo-espflash
cargo install cargo-espflash  
cargo espflash flash --monitor --release
```

## Example Application

The example demonstrates a simple blinky application using:
- QP active objects
- Hierarchical state machines (QHsm)
- State transitions with timeout events
- Hardware GPIO control

## Directory Structure

```
esp32c6/
├── Cargo.toml          # Project dependencies
├── .cargo/
│   └── config.toml     # Target configuration
├── src/
│   └── main.rs         # Blinky example
└── README.md           # This file
```

## License

MIT OR Apache-2.0
