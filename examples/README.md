# QP Framework Examples

This directory contains example applications demonstrating the QP framework on various platforms.

## Structure

Examples are organized as a workspace member with platform-specific features. This allows:
- Single source file per example
- Platform selection via features
- Reusable across different boards/OSes

## Available Examples

### Dining Philosophers Problem (DPP)

**File**: `dpp.rs`  
**Platforms**: ESP32-C6 (RISC-V)

A classic concurrency problem demonstrating:
- 5 philosophers with state machines (thinking/hungry/eating)
- Resource management (fork allocation)
- Deadlock prevention
- Event-driven architecture

## Building Examples

### For ESP32-C6

```bash
# From workspace root
cargo build --example dpp --features esp32c6 --target riscv32imac-unknown-none-elf --release -p qp-examples

# Or from examples directory
cd examples
cargo build --example dpp --features esp32c6 --target riscv32imac-unknown-none-elf --release
```

### Flashing to Hardware

The standalone projects in subdirectories (e.g., `dpp-esp32c6/`) are complete board-specific packages with proper linker scripts and can be flashed directly:

```bash
cd examples/dpp-esp32c6
cargo build --release
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/dpp-esp32c6
```

## Adding New Examples

1. Create example file: `examples/myexample.rs`
2. Add to `Cargo.toml`:
   ```toml
   [[example]]
   name = "myexample"
   path = "myexample.rs"
   required-features = ["platform"]
   ```
3. Add platform-specific dependencies as optional
4. Use `#![cfg(feature = "platform")]` in example code

## Platform Features

- `esp32c6` - ESP32-C6 RISC-V microcontroller
- Future: `stm32`, `nrf52`, `linux`, etc.

## Notes

- Examples require `#![no_std]` and `#![no_main]` for embedded targets
- Each platform may have specific setup requirements (see standalone projects)
- The `.cargo/config.toml` sets default target for RISC-V builds
